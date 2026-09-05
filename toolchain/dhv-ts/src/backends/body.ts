// ============================================================================
// dhv-ts/src/backends/body.ts — 函数体活体翻译器（python/ts/js/rust/go/cpp）
// ----------------------------------------------------------------------------
// 能力边界（诚实声明）：翻译「实际子集」——let/赋值/if-elif/while/for-range/
// for-in/loop/match(枚举|字面量|_)/return/break/continue/调用链/方法链(常用 std
// 方法映射表)/format! 插值/native 同语言块原样透传。遇到任何不支持的构件抛出
// TranspileError，decls.ts 捕获后回退 contract 围栏模式（绝不输出半翻译代码）。
// ============================================================================

import * as A from '../ast';
import { LangSpec } from './registry';
import { Lexer } from '../lexer';
import { parseExprsFromTokens, treeText } from '../parser';
import type { Token } from '../lexer';

export class TranspileError extends Error {}

export interface BodyCtx {
  ty(t: A.HType | undefined): string;
  enums: Map<string, A.Item & { kind: 'enum' }>;
  strLit(s: string): string;
}

// ---- 关键字避让 ----
const PY_KW = new Set(['class', 'def', 'lambda', 'None', 'True', 'False', 'import', 'from', 'as', 'in', 'is', 'not', 'and', 'or', 'pass', 'del', 'global', 'nonlocal', 'with', 'try', 'except', 'finally', 'raise', 'assert', 'yield', 'async', 'await', 'print']);
const JS_KW = new Set(['class', 'function', 'typeof', 'instanceof', 'delete', 'in', 'of', 'new', 'this', 'super', 'extends', 'default', 'switch', 'case', 'do', 'void', 'var', 'with', 'debugger', 'export', 'import']);
const GO_KW = new Set(['type', 'range', 'func', 'var', 'const', 'map', 'chan', 'interface', 'select', 'struct', 'package', 'go', 'defer', 'fallthrough', 'if', 'else', 'for', 'switch', 'case', 'default', 'break', 'continue', 'return']);
const RS_KW = new Set(['fn', 'let', 'match', 'box', 'ref', 'move', 'type', 'trait', 'impl', 'where', 'as', 'dyn', 'pub', 'mod', 'use', 'crate', 'self', 'super']);
const CPP_KW = new Set(['class', 'template', 'namespace', 'new', 'delete', 'operator', 'this', 'try', 'catch', 'throw', 'typedef', 'union', 'enum', 'switch', 'case', 'default', 'public', 'private', 'protected', 'virtual', 'friend', 'inline', 'auto', 'bool', 'true', 'false', 'nullptr', 'constexpr']);

function kwSet(langId: string): Set<string> | null {
  switch (langId) {
    case 'python': return PY_KW;
    case 'typescript': case 'javascript': return JS_KW;
    case 'go': return GO_KW;
    case 'rust': return RS_KW;
    case 'cpp': return CPP_KW;
    default: return null;
  }
}
function ident(name: string, langId: string): string {
  return kwSet(langId)?.has(name) ? `${name}_` : name;
}
function patternName(p: A.Pattern): string | undefined {
  if (p.kind === 'binding') return p.name;
  if (p.kind === 'wildcard') return '_';
  if (p.kind === 'path') return p.segs[p.segs.length - 1];
  return undefined;
}

/**
 * 把单段 Some/None/Ok/Err 简写模式归一化为两段 Option::Some / Option::None /
 * Result::Ok / Result::Err。其它模式原样返回。这样 armInfo / rustPattern 等下游
 * 只需处理标准两段形式，无需重复简写特化。
 *
 * 合理性：HSL 的 Some/None/Ok/Err 在作用域中没有任何独立绑定语义（不允许
 * 用户自定义这些名字作为 pattern 变体），因此单段形式等价于 builtin 路径。
 */
function normalizePattern(p: A.Pattern): A.Pattern {
  if (p.kind !== 'path' || p.segs.length !== 1) return p;
  const name = p.segs[0]!;
  if (name === 'Some' || name === 'None') {
    return { ...p, segs: ['Option', name] };
  }
  if (name === 'Ok' || name === 'Err') {
    return { ...p, segs: ['Result', name] };
  }
  return p;
}

/**
 * v1.4.9：闭包参数引用的内联替换（深克隆表达式树，把 path[param] 替换为目标串）。
 * 用途：go 后端无法直投闭包值（func literal 需显式参数类型），但 sort.SliceStable
 * 的 comparator / filter 谓词等只需把闭包体中的参数引用替换为 v[i] / *o 等目标
 * 表达式即可内联合成。替换节点用 native（lang 匹配时原文透传）承载。
 * 不支持的表达式形态（少见的复杂结构）诚实抛 TranspileError 回退 contract。
 */
function substParam(e: A.Expr, param: string, target: string, langId: string): A.Expr {
  const walk = (x: A.Expr): A.Expr => {
    switch (x.kind) {
      case 'path':
        if (x.segs.length === 1 && x.segs[0] === param) {
          return { kind: 'native', lang: langId, body: target } as A.Expr;
        }
        return x;
      case 'binary': return { ...x, lhs: walk(x.lhs), rhs: walk(x.rhs) };
      case 'unary': return { ...x, operand: walk(x.operand) };
      case 'method': return { ...x, recv: walk(x.recv), args: x.args.map(walk) };
      case 'field': return { ...x, recv: walk(x.recv) };
      case 'index': return { ...x, recv: walk(x.recv), index: walk(x.index) };
      case 'call': return { ...x, args: x.args.map(walk) };
      case 'lit': case 'native': return x;
      default: throw new TranspileError(`go 闭包内联替换不支持表达式 ${x.kind}`);
    }
  };
  return walk(e);
}

type TyKind = 'int' | 'float' | 'str' | 'bool' | 'vec' | 'map' | 'option' | 'result' | 'unknown';

// ---------------------------------------------------------------------------
// 方法映射表
// ---------------------------------------------------------------------------

interface MethodCtxIn { recv: string; args: string[]; kind: TyKind }
type Renderer = (c: MethodCtxIn) => string;
type MethodEntry = Partial<Record<string, Renderer>>;

const METHOD_TABLE: Record<string, MethodEntry> = {
  len: {
    python: (c) => `len(${c.recv})`,
    typescript: (c) => `${c.recv}.length`,
    javascript: (c) => `${c.recv}.length`,
    rust: (c) => `${c.recv}.len()`,
    // v1.4.10（真机 go build 实测）：go len() 返回 int，与 HSL i64 映射（int64）
    // 混算报 mismatched types —— 统一包 int64() 归一到 HSL 语义类型
    go: (c) => `int64(len(${c.recv}))`,
    cpp: (c) => `${c.recv}.size()`,
  },
  is_empty: {
    python: (c) => `(len(${c.recv}) == 0)`,
    typescript: (c) => `(${c.recv}.length === 0)`,
    javascript: (c) => `(${c.recv}.length === 0)`,
    rust: (c) => `${c.recv}.is_empty()`,
    go: (c) => `(len(${c.recv}) == 0)`,
    cpp: (c) => `${c.recv}.empty()`,
  },
  push: {
    python: (c) => `${c.recv}.append(${c.args.join(', ')})`,
    typescript: (c) => `${c.recv}.push(${c.args.join(', ')})`,
    javascript: (c) => `${c.recv}.push(${c.args.join(', ')})`,
    rust: (c) => `${c.recv}.push(${c.args.join(', ')})`,
    go: (c) => `${c.recv} = append(${c.recv}, ${c.args.join(', ')})`,
    cpp: (c) => `${c.recv}.push_back(${c.args.join(', ')})`,
  },
  get: {
    python: (c) => `${c.recv}[${c.args[0] ?? 0}]`,
    typescript: (c) => `${c.recv}[${c.args[0] ?? 0}]`,
    javascript: (c) => `${c.recv}[${c.args[0] ?? 0}]`,
    rust: (c) => `${c.recv}[${c.args[0] ?? 0}]`,
    go: (c) => `${c.recv}[${c.args[0] ?? 0}]`,
    cpp: (c) => `${c.recv}[${c.args[0] ?? 0}]`,
  },
  contains: {
    python: (c) => `(${c.args[0]} in ${c.recv})`,
    typescript: (c) => `${c.recv}.includes(${c.args.join(', ')})`,
    javascript: (c) => `${c.recv}.includes(${c.args.join(', ')})`,
    rust: (c) => `${c.recv}.contains(${c.args.join(', ')})`,
    go: (c) => `slices.Contains(${c.recv}, ${c.args[0] ?? ''})`,
    cpp: (c) => `(std::find(${c.recv}.begin(), ${c.recv}.end(), ${c.args[0] ?? ''}) != ${c.recv}.end())`,
  },
  first: {
    python: (c) => `(${c.recv}[0] if ${c.recv} else None)`,
    typescript: (c) => `(${c.recv}.length > 0 ? ${c.recv}[0] : null)`,
    javascript: (c) => `(${c.recv}.length > 0 ? ${c.recv}[0] : null)`,
    rust: (c) => `${c.recv}.first()`,
    cpp: (c) => `_dhvFirst(${c.recv})`,
    go: (c) => `_dhvFirst(&${c.recv})`,
  },
  last: {
    python: (c) => `(${c.recv}[-1] if ${c.recv} else None)`,
    typescript: (c) => `(${c.recv}.length > 0 ? ${c.recv}[${c.recv}.length - 1] : null)`,
    javascript: (c) => `(${c.recv}.length > 0 ? ${c.recv}[${c.recv}.length - 1] : null)`,
    rust: (c) => `${c.recv}.last()`,
    cpp: (c) => `_dhvLast(${c.recv})`,
    go: (c) => `_dhvLast(&${c.recv})`,
  },
  clone: {
    python: (c) => `_dhv_clone(${c.recv})`,
    typescript: (c) => `_dhvClone(${c.recv})`,
    javascript: (c) => `_dhvClone(${c.recv})`,
    rust: (c) => `${c.recv}.clone()`,
    // cpp/go：值语义拷贝（std::vector 隐式拷贝构造 / slice header 拷贝对 backing array 共享——
    // interp 中 Vec<i32>::clone 是浅拷贝原语，对 Vec<Struct> 含 owned 字段需用户显式处理）
    cpp: (c) => `${c.recv}`,
    go: (c) => `${c.recv}`,
  },
  to_string: {
    python: (c) => `str(${c.recv})`,
    typescript: (c) => `String(${c.recv})`,
    javascript: (c) => `String(${c.recv})`,
    rust: (c) => `${c.recv}.to_string()`,
    go: (c) => `fmt.Sprintf("%v", ${c.recv})`,
    cpp: (c) => `std::to_string(${c.recv})`,
  },
  trim: {
    python: (c) => `${c.recv}.strip()`,
    typescript: (c) => `${c.recv}.trim()`,
    javascript: (c) => `${c.recv}.trim()`,
    rust: (c) => `${c.recv}.trim()`,
    go: (c) => `strings.TrimSpace(${c.recv})`,
    // cpp：C++ 标准库无 trim —— _dhvTrim 助手（首尾空白裁剪）
    cpp: (c) => `_dhvTrim(${c.recv})`,
  },
  to_lowercase: {
    python: (c) => `${c.recv}.lower()`,
    typescript: (c) => `${c.recv}.toLowerCase()`,
    javascript: (c) => `${c.recv}.toLowerCase()`,
    rust: (c) => `${c.recv}.to_lowercase()`,
    go: (c) => `strings.ToLower(${c.recv})`,
    cpp: (c) => `_dhvToLower(${c.recv})`,
  },
  to_uppercase: {
    python: (c) => `${c.recv}.upper()`,
    typescript: (c) => `${c.recv}.toUpperCase()`,
    javascript: (c) => `${c.recv}.toUpperCase()`,
    rust: (c) => `${c.recv}.to_uppercase()`,
    go: (c) => `strings.ToUpper(${c.recv})`,
    cpp: (c) => `_dhvToUpper(${c.recv})`,
  },
  starts_with: {
    python: (c) => `${c.recv}.startswith(${c.args.join(', ')})`,
    typescript: (c) => `${c.recv}.startsWith(${c.args.join(', ')})`,
    javascript: (c) => `${c.recv}.startsWith(${c.args.join(', ')})`,
    rust: (c) => `${c.recv}.starts_with(${c.args.join(', ')})`,
    go: (c) => `strings.HasPrefix(${c.recv}, ${c.args[0] ?? '""'})`,
    // cpp：C++20 std::string::starts_with（string_view 隐式转换）
    cpp: (c) => `${c.recv}.starts_with(${c.args[0] ?? '""'})`,
  },
  ends_with: {
    python: (c) => `${c.recv}.endswith(${c.args.join(', ')})`,
    typescript: (c) => `${c.recv}.endsWith(${c.args.join(', ')})`,
    javascript: (c) => `${c.recv}.endsWith(${c.args.join(', ')})`,
    rust: (c) => `${c.recv}.ends_with(${c.args.join(', ')})`,
    go: (c) => `strings.HasSuffix(${c.recv}, ${c.args[0] ?? '""'})`,
    cpp: (c) => `${c.recv}.ends_with(${c.args[0] ?? '""'})`,
  },
  replace: {
    python: (c) => `${c.recv}.replace(${c.args.join(', ')})`,
    typescript: (c) => `${c.recv}.replaceAll(${c.args.join(', ')})`,
    javascript: (c) => `${c.recv}.replaceAll(${c.args.join(', ')})`,
    rust: (c) => `${c.recv}.replace(${c.args.join(', ')})`,
    go: (c) => `strings.ReplaceAll(${c.recv}, ${c.args[0] ?? '""'}, ${c.args[1] ?? '""'})`,
    // cpp：std::string::replace 是下标式非查找式 —— _dhvReplaceAll（全部替换，py 语义）
    cpp: (c) => `_dhvReplaceAll(${c.recv}, ${c.args[0] ?? '""'}, ${c.args[1] ?? '""'})`,
  },
  split: {
    python: (c) => `${c.recv}.split(${c.args.join(', ')})`,
    typescript: (c) => `${c.recv}.split(${c.args.join(', ')})`,
    javascript: (c) => `${c.recv}.split(${c.args.join(', ')})`,
    go: (c) => `strings.Split(${c.recv}, ${c.args[0] ?? '""'})`,
    cpp: (c) => `_dhvSplit(${c.recv}, ${c.args[0] ?? '""'})`,
  },
  join: {
    python: (c) => `${c.args[0]}.join(${c.recv})`,
    typescript: (c) => `${c.recv}.join(${c.args[0] ?? '""'})`,
    javascript: (c) => `${c.recv}.join(${c.args[0] ?? '""'})`,
    rust: (c) => `${c.recv}.join(${c.args.join(', ')})`,
    go: (c) => `strings.Join(${c.recv}, ${c.args[0] ?? '""'})`,
    // cpp：if constexpr 分发 string/数值元素（interp join 对数值元素走 display）
    cpp: (c) => `_dhvJoin(${c.recv}, ${c.args[0] ?? '""'})`,
  },
  chars: {
    python: (c) => `list(${c.recv})`,
    typescript: (c) => `[...${c.recv}]`,
    javascript: (c) => `[...${c.recv}]`,
    rust: (c) => `${c.recv}.chars()`,
    go: (c) => `[]rune(${c.recv})`,
  },
  take: {
    python: (c) => `${c.recv}[:${c.args[0] ?? 0}]`,
    typescript: (c) => `${c.recv}.slice(0, ${c.args[0] ?? 0})`,
    javascript: (c) => `${c.recv}.slice(0, ${c.args[0] ?? 0})`,
    rust: (c) => `${c.recv}.take(${c.args.join(', ')})`,
  },
  skip: {
    python: (c) => `${c.recv}[${c.args[0] ?? 0}:]`,
    typescript: (c) => `${c.recv}.slice(${c.args[0] ?? 0})`,
    javascript: (c) => `${c.recv}.slice(${c.args[0] ?? 0})`,
  },
  rev: {
    python: (c) => `${c.recv}[::-1]`,
    typescript: (c) => `[...${c.recv}].reverse()`,
    javascript: (c) => `[...${c.recv}].reverse()`,
    rust: (c) => `${c.recv}.rev()`,
  },
  sum: {
    python: (c) => `sum(${c.recv})`,
    typescript: (c) => `${c.recv}.reduce((a, b) => a + b, 0)`,
    javascript: (c) => `${c.recv}.reduce((a, b) => a + b, 0)`,
  },
  map: {
    python: (c) => `[(${c.args[0]})(_x) for _x in ${c.recv}]`,
    typescript: (c) => `${c.recv}.map(${c.args[0]})`,
    javascript: (c) => `${c.recv}.map(${c.args[0]})`,
    rust: (c) => `${c.recv}.into_iter().map(${c.args[0]}).collect::<Vec<_>>()`,
  },
  filter: {
    python: (c) => `[_x for _x in ${c.recv} if (${c.args[0]})(_x)]`,
    typescript: (c) => `${c.recv}.filter(${c.args[0]})`,
    javascript: (c) => `${c.recv}.filter(${c.args[0]})`,
    rust: (c) => `${c.recv}.into_iter().filter(${c.args[0]}).collect::<Vec<_>>()`,
  },
  insert: {
    python: (c) => `${c.recv}[${c.args[0]}] = ${c.args[1]}`,
    typescript: (c) => `${c.recv}.set(${c.args.join(', ')})`,
    javascript: (c) => `${c.recv}.set(${c.args.join(', ')})`,
    rust: (c) => `${c.recv}.insert(${c.args.join(', ')})`,
    go: (c) => `${c.recv}[${c.args[0]}] = ${c.args[1]}`,
    // cpp：operator[] 写入（返回 mapped_type 引用，语句/表达式双兼容）
    cpp: (c) => `${c.recv}[${c.args[0]}] = ${c.args[1]}`,
  },
  contains_key: {
    python: (c) => `(${c.args[0]} in ${c.recv})`,
    typescript: (c) => `${c.recv}.has(${c.args.join(', ')})`,
    javascript: (c) => `${c.recv}.has(${c.args.join(', ')})`,
    rust: (c) => `${c.recv}.contains_key(${c.args.join(', ')})`,
    go: (c) => `func() bool { _, ok := ${c.recv}[${c.args[0]}]; return ok }()`,
    // cpp：find != end（std::map / std::unordered_map 通用）
    cpp: (c) => `(${c.recv}.find(${c.args[0]}) != ${c.recv}.end())`,
  },
  keys: {
    python: (c) => `list(${c.recv}.keys())`,
    typescript: (c) => `[...${c.recv}.keys()]`,
    javascript: (c) => `[...${c.recv}.keys()]`,
    rust: (c) => `${c.recv}.keys()`,
    // cpp/go：泛型/模板助手（map→key 切片；类型推导免手写 key_type）
    go: (c) => `_dhvKeys(${c.recv})`,
    cpp: (c) => `_dhvKeys(${c.recv})`,
  },
  values: {
    python: (c) => `list(${c.recv}.values())`,
    typescript: (c) => `[...${c.recv}.values()]`,
    javascript: (c) => `[...${c.recv}.values()]`,
    rust: (c) => `${c.recv}.values()`,
    go: (c) => `_dhvValues(${c.recv})`,
    cpp: (c) => `_dhvValues(${c.recv})`,
  },
  is_some: {
    python: (c) => `(${c.recv} is not None)`,
    typescript: (c) => `(${c.recv} != null)`,
    javascript: (c) => `(${c.recv} != null)`,
    rust: (c) => `${c.recv}.is_some()`,
    cpp: (c) => `${c.recv}.has_value()`,
    go: (c) => `(${c.recv} != nil)`,
  },
  is_none: {
    python: (c) => `(${c.recv} is None)`,
    typescript: (c) => `(${c.recv} == null)`,
    javascript: (c) => `(${c.recv} == null)`,
    rust: (c) => `${c.recv}.is_none()`,
    cpp: (c) => `(!${c.recv}.has_value())`,
    go: (c) => `(${c.recv} == nil)`,
  },
  unwrap_or: {
    // prelude 助手保证 recv 只求值一次（副作用接收者如 m.remove(k) 双重求值会破坏语义）
    python: (c) => `_dhv_unwrap_or(${c.recv}, ${c.args[0] ?? 'None'})`,
    typescript: (c) => `(${c.recv} ?? ${c.args[0] ?? 'null'})`,
    javascript: (c) => `(${c.recv} ?? ${c.args[0] ?? 'null'})`,
    rust: (c) => `${c.recv}.unwrap_or(${c.args.join(', ')})`,
    // cpp：std::optional 原生 value_or（recv 必须为 std::optional<T> 类型）
    cpp: (c) => `${c.recv}.value_or(${c.args[0] ?? '0'})`,
    // go：v1.4.10 修复（真机 go build 实测）—— go 无三元表达式，且裸 `cond ? a : b` 是
    // 非法语法；改用 _dhvUnwrapOr 泛型助手（表达式位置合法 + 单次求值 + 副作用接收者安全）
    go: (c) => `_dhvUnwrapOr(${c.recv}, ${c.args[0] ?? '0'})`,
  },
  unwrap: {
    python: (c) => `_dhv_unwrap(${c.recv})`,
    typescript: (c) => `_dhvUnwrap(${c.recv})`,
    javascript: (c) => `_dhvUnwrap(${c.recv})`,
    rust: (c) => `${c.recv}.unwrap()`,
    // cpp：std::optional 解引用（None 时抛 bad_optional_access，与 interp 的 RuntimeError 同语义）
    cpp: (c) => `*${c.recv}`,
    // go：*T 解引用（nil 时 panic —— 与 interp 的 RuntimeError 同语义）
    go: (c) => `*${c.recv}`,
  },
  abs: {
    python: (c) => `abs(${c.recv})`,
    typescript: (c) => `Math.abs(${c.recv})`,
    javascript: (c) => `Math.abs(${c.recv})`,
    rust: (c) => `${c.recv}.abs()`,
  },
  min: {
    python: (c) => `min(${c.recv}, ${c.args[0] ?? 0})`,
    typescript: (c) => `Math.min(${c.recv}, ${c.args[0] ?? 0})`,
    javascript: (c) => `Math.min(${c.recv}, ${c.args[0] ?? 0})`,
    rust: (c) => `${c.recv}.min(${c.args.join(', ')})`,
  },
  max: {
    python: (c) => `max(${c.recv}, ${c.args[0] ?? 0})`,
    typescript: (c) => `Math.max(${c.recv}, ${c.args[0] ?? 0})`,
    javascript: (c) => `Math.max(${c.recv}, ${c.args[0] ?? 0})`,
    rust: (c) => `${c.recv}.max(${c.args.join(', ')})`,
  },
  // append 与 extend 在 interp 中语义相同（均接收 iterable 展开）—— v1.4.8 cpp _dhvExtend 助手
  //（临时变量迭代器对：原内联 (tmp).begin(), (tmp).end() 创建两个不同临时 → length_error）
  append: {
    python: (c) => `${c.recv}.extend(${c.args[0] ?? '[]'})`,
    typescript: (c) => `${c.recv}.push(...(${c.args[0] ?? '[]'}))`,
    javascript: (c) => `${c.recv}.push(...(${c.args[0] ?? '[]'}))`,
    rust: (c) => `${c.recv}.extend(${c.args[0] ?? '[]'})`,
    go: (c) => `${c.recv} = append(${c.recv}, (${c.args[0] ?? '[]'})...)`,
    // v1.4.8：cpp _dhvExtend 助手（const ref 绑定临时，保证 begin/end 同源）
    cpp: (c) => `_dhvExtend(${c.recv}, ${c.args[0] ?? '{}'})`,
  },
  // ---- Vec 补充 ----
  // pop：返回 Option<T>（Vec 空时 None）—— 接收者按引用传递，pop 副作用对调用者可见
  // cpp 模板助手（std::vector<T>& → std::optional<T>）；go 泛型助手（*[]T → *T，1.18+）
  pop: {
    python: (c) => `_dhv_pop(${c.recv})`,
    typescript: (c) => `_dhvPop(${c.recv})`,
    javascript: (c) => `_dhvPop(${c.recv})`,
    rust: (c) => `${c.recv}.pop()`,
    cpp: (c) => `_dhvPop(${c.recv})`,
    go: (c) => `_dhvPop(&${c.recv})`,
  },
  // is_sorted / pop / clone：副作用接收者安全（prelude 助手单次求值）
  // v1.4.8：cpp std::is_sorted + go slices.IsSorted 补全
  is_sorted: {
    python: (c) => `_dhv_is_sorted(${c.recv})`,
    typescript: (c) => `${c.recv}.every((v, i) => i === 0 || ${c.recv}[i - 1] <= v)`,
    javascript: (c) => `${c.recv}.every((v, i) => i === 0 || ${c.recv}[i - 1] <= v)`,
    cpp: (c) => `std::is_sorted(${c.recv}.begin(), ${c.recv}.end())`,
    go: (c) => `slices.IsSorted(${c.recv})`,
  },
  // v1.4.8：cpp clear 补全（std::vector::clear）
  clear: {
    python: (c) => `${c.recv}.clear()`,
    typescript: (c) => `${c.recv}.length = 0`,
    javascript: (c) => `${c.recv}.length = 0`,
    rust: (c) => `${c.recv}.clear()`,
    go: (c) => `${c.recv} = nil`,
    cpp: (c) => `${c.recv}.clear()`,
  },
  // v1.4.8：cpp sort 补全（std::sort；升序对齐 interp 的 (a<b?-1:a>b?1:0)）
  sort: {
    python: (c) => `${c.recv}.sort()`,
    typescript: (c) => `${c.recv}.sort((a, b) => a - b)`,
    javascript: (c) => `${c.recv}.sort((a, b) => a - b)`,
    rust: (c) => `${c.recv}.sort()`,
    go: (c) => `slices.Sort(${c.recv})`,
    cpp: (c) => `std::sort(${c.recv}.begin(), ${c.recv}.end())`,
  },
  // ---- HashMap 补充 ----
  // Map::remove → Option 语义（interp：缺键返回 None，返回旧值）。
  // python pop(k, None) 天然对齐；ts/js 的 delete 只返回 bool，需 _dhvRemove 助手取旧值；
  // go 从匿名函数升级为 _dhvMapRemove 泛型助手（返回 *V 与 Option 指针表示一致 ——
  // 旧版返回 any，链式 .unwrap_or(d) 解引用 any 是编译错误）；cpp _dhvMapRemove 模板助手
  remove: {
    python: (c) => `${c.recv}.pop(${c.args[0]}, None)`,
    typescript: (c) => `_dhvRemove(${c.recv}, ${c.args[0] ?? ''})`,
    javascript: (c) => `_dhvRemove(${c.recv}, ${c.args[0] ?? ''})`,
    rust: (c) => `${c.recv}.remove(${c.args.join(', ')})`,
    go: (c) => `_dhvMapRemove(${c.recv}, ${c.args[0] ?? ''})`,
    cpp: (c) => `_dhvMapRemove(${c.recv}, ${c.args[0] ?? ''})`,
  },
  // ---- String 补充 ----
  char_count: {
    python: (c) => `len(${c.recv})`,
    typescript: (c) => `[...${c.recv}].length`,
    javascript: (c) => `[...${c.recv}].length`,
    rust: (c) => `${c.recv}.chars().count()`,
    go: (c) => `len([]rune(${c.recv}))`,
    // cpp：UTF-8 前导字节计数（std::string::size 是字节数非码点数）
    cpp: (c) => `_dhvCharCount(${c.recv})`,
  },
  split_whitespace: {
    python: (c) => `${c.recv}.split()`,
    typescript: (c) => `${c.recv}.trim().split(/\\s+/).filter((s) => s.length > 0)`,
    javascript: (c) => `${c.recv}.trim().split(/\\s+/).filter((s) => s.length > 0)`,
    rust: (c) => `${c.recv}.split_whitespace().collect::<Vec<_>>()`,
    go: (c) => `strings.Fields(${c.recv})`,
    cpp: (c) => `_dhvSplitWS(${c.recv})`,
  },
  lines: {
    python: (c) => `${c.recv}.split('\\n')`,
    typescript: (c) => `${c.recv}.split('\\n')`,
    javascript: (c) => `${c.recv}.split('\\n')`,
    go: (c) => `strings.Split(${c.recv}, "\\n")`,
    // cpp：与 split("\n") 同语义（含尾空串，py 语义对齐）
    cpp: (c) => `_dhvSplit(${c.recv}, "\\n")`,
  },
  repeat: {
    python: (c) => `${c.recv} * ${c.args[0] ?? 1}`,
    typescript: (c) => `${c.recv}.repeat(${c.args[0] ?? 1})`,
    javascript: (c) => `${c.recv}.repeat(${c.args[0] ?? 1})`,
    rust: (c) => `${c.recv}.repeat(${c.args[0] ?? 1})`,
    cpp: (c) => `_dhvRepeat(${c.recv}, ${c.args[0] ?? 1})`,
  },
  // find 同上不映射（Option 语义）
  // ---- Option 补充（expect 需 _dhv_expect 预置助手）----
  // v1.4.8：cpp/go expect 助手化（cpp throw std::runtime_error / go panic）
  expect: {
    python: (c) => `_dhv_expect(${c.recv}, ${c.args[0] ?? '"None"'})`,
    typescript: (c) => `_dhvExpect(${c.recv}, ${c.args[0] ?? '"None"'})`,
    javascript: (c) => `_dhvExpect(${c.recv}, ${c.args[0] ?? '"None"'})`,
    rust: (c) => `${c.recv}.expect(${c.args.join(', ')})`,
    cpp: (c) => `_dhvOptExpect(${c.recv}, ${c.args[0] ?? '"None"'})`,
    go: (c) => `_dhvOptExpect(${c.recv}, ${c.args[0] ?? '"None"'})`,
  },
  // v1.4.8：cpp and_then 助手化（lambda 用 auto 参数 + decltype 推导返回 std::optional<R>）
  // go 缺乏类型推导（HSL 闭包无类型注解），诚实回退 contract
  and_then: {
    python: (c) => `_dhv_and_then(${c.recv}, ${c.args[0]})`,
    typescript: (c) => `_dhvAndThen(${c.recv}, ${c.args[0]})`,
    javascript: (c) => `_dhvAndThen(${c.recv}, ${c.args[0]})`,
    cpp: (c) => `_dhvOptAndThen(${c.recv}, ${c.args[0]})`,
  },
  // ---- Option 补充（v1.4.3 扩面 + v1.4.8 cpp/go）----
  // cpp：_dhvOptOr 模板助手（同型 std::optional<T> 选择）
  // go：_dhvOptOr 泛型助手（同型 *T 选择）
  or: {
    python: (c) => `_dhv_or(${c.recv}, ${c.args[0] ?? 'None'})`,
    typescript: (c) => `(${c.recv} ?? ${c.args[0] ?? 'null'})`,
    javascript: (c) => `(${c.recv} ?? ${c.args[0] ?? 'null'})`,
    rust: (c) => `${c.recv}.or(${c.args.join(', ')})`,
    cpp: (c) => `_dhvOptOr(${c.recv}, ${c.args[0] ?? 'std::nullopt'})`,
    go: (c) => `_dhvOptOr(${c.recv}, ${c.args[0] ?? 'nil'})`,
  },
  // v1.4.8：cpp unwrap_or_else 助手化（lambda 零参返回 T）
  unwrap_or_else: {
    python: (c) => `_dhv_unwrap_or_else(${c.recv}, ${c.args[0] ?? 'None'})`,
    typescript: (c) => `_dhvUnwrapOrElse(${c.recv}, ${c.args[0] ?? 'null'})`,
    javascript: (c) => `_dhvUnwrapOrElse(${c.recv}, ${c.args[0] ?? 'null'})`,
    rust: (c) => `${c.recv}.unwrap_or_else(${c.args.join(', ')})`,
    cpp: (c) => `_dhvOptUnwrapOrElse(${c.recv}, ${c.args[0]})`,
  },
  // 注：is_ok/is_err 不映射 —— Result 在各后端的表示不同（python 值+异常 / ts 值|null / rust 原生），
  // 错误映射比回退 contract 更危险，宁缺毋滥
  // ---- 数值方法（v1.4.3 扩面，interp NUM_METHODS 对齐）----
  pow: {
    python: (c) => `(${c.recv} ** ${c.args[0] ?? 2})`,
    typescript: (c) => `(${c.recv} ** ${c.args[0] ?? 2})`,
    javascript: (c) => `(${c.recv} ** ${c.args[0] ?? 2})`,
    rust: (c) => `${c.recv}.pow(${c.args.join(', ')})`,
  },
  sqrt: {
    python: (c) => `(${c.recv} ** 0.5)`,
    typescript: (c) => `Math.sqrt(${c.recv})`,
    javascript: (c) => `Math.sqrt(${c.recv})`,
    rust: (c) => `${c.recv}.sqrt()`,
  },
  floor: {
    python: (c) => `int(${c.recv} // 1)`,
    typescript: (c) => `Math.floor(${c.recv})`,
    javascript: (c) => `Math.floor(${c.recv})`,
    rust: (c) => `${c.recv}.floor()`,
  },
  ceil: {
    python: (c) => `(-int(-${c.recv} // 1))`,
    typescript: (c) => `Math.ceil(${c.recv})`,
    javascript: (c) => `Math.ceil(${c.recv})`,
    rust: (c) => `${c.recv}.ceil()`,
  },
  round: {
    // python round() 是银行家舍入；int((x + 0.5) // 1) 与 JS Math.round / Rust f64::round 同为半值向上
    python: (c) => `int((${c.recv} + 0.5) // 1)`,
    typescript: (c) => `Math.round(${c.recv})`,
    javascript: (c) => `Math.round(${c.recv})`,
    rust: (c) => `${c.recv}.round()`,
  },
  clamp: {
    python: (c) => `min(max(${c.recv}, ${c.args[0] ?? 0}), ${c.args[1] ?? 0})`,
    typescript: (c) => `Math.min(Math.max(${c.recv}, ${c.args[0] ?? 0}), ${c.args[1] ?? 0})`,
    javascript: (c) => `Math.min(Math.max(${c.recv}, ${c.args[0] ?? 0}), ${c.args[1] ?? 0})`,
    rust: (c) => `${c.recv}.clamp(${c.args.join(', ')})`,
    go: (c) => `min(max(${c.recv}, ${c.args[0] ?? 0}), ${c.args[1] ?? 0})`,
  },
  // ---- Vec 迭代器/聚合（v1.4.3 扩面）----
  any: {
    python: (c) => `any((${c.args[0]})(_x) for _x in ${c.recv})`,
    typescript: (c) => `${c.recv}.some(${c.args[0]})`,
    javascript: (c) => `${c.recv}.some(${c.args[0]})`,
    rust: (c) => `${c.recv}.iter().any(${c.args[0]})`,
  },
  all: {
    python: (c) => `all((${c.args[0]})(_x) for _x in ${c.recv})`,
    typescript: (c) => `${c.recv}.every(${c.args[0]})`,
    javascript: (c) => `${c.recv}.every(${c.args[0]})`,
    rust: (c) => `${c.recv}.iter().all(${c.args[0]})`,
  },
  // HSL 签名 fold(init, f)（Rust 序）；python reduce / ts reduce 是 (f, init) 序 —— 此处换序
  fold: {
    python: (c) => `__import__('functools').reduce(${c.args[1]}, ${c.recv}, ${c.args[0]})`,
    typescript: (c) => `${c.recv}.reduce(${c.args[1]}, ${c.args[0]})`,
    javascript: (c) => `${c.recv}.reduce(${c.args[1]}, ${c.args[0]})`,
    rust: (c) => `${c.recv}.into_iter().fold(${c.args[0]}, ${c.args[1]})`,
  },
  for_each: {
    // python 无表达式形式的副作用迭代 —— 不映射（回退 contract）
    typescript: (c) => `${c.recv}.forEach(${c.args[0]})`,
    javascript: (c) => `${c.recv}.forEach(${c.args[0]})`,
    rust: (c) => `${c.recv}.into_iter().for_each(${c.args[0]})`,
  },
  extend: {
    python: (c) => `${c.recv}.extend(${c.args[0] ?? '[]'})`,
    typescript: (c) => `${c.recv}.push(...(${c.args[0] ?? '[]'}))`,
    javascript: (c) => `${c.recv}.push(...(${c.args[0] ?? '[]'}))`,
    rust: (c) => `${c.recv}.extend(${c.args[0] ?? '[]'})`,
    go: (c) => `${c.recv} = append(${c.recv}, (${c.args[0] ?? '[]'})...)`,
    // v1.4.8：cpp _dhvExtend 助手（const ref 绑定临时，防 begin/end 不同源 length_error）
    cpp: (c) => `_dhvExtend(${c.recv}, ${c.args[0] ?? '{}'})`,
  },
  // ---- rust 专属迭代器链拼块（py/ts 的 map/filter 直接产出数组，无需链拼）----
  iter: {
    rust: (c) => `${c.recv}.iter()`,
  },
  collect: {
    rust: (c) => `${c.recv}.collect::<Vec<_>>()`,
  },
  // ---- Option/String 补充（v1.4.4 扩面，interp builtins 对齐）----
  // 以下映射统一走 prelude 助手：副作用接收者（如 name.trim().strip_prefix(...)）
  // 在内联三元中会被多次求值，助手函数参数保证只求值一次
  strip_prefix: {
    python: (c) => `_dhv_strip_prefix(${c.recv}, ${c.args[0] ?? '""'})`,
    typescript: (c) => `(${c.recv}.startsWith(${c.args[0] ?? '""'}) ? ${c.recv}.slice((${c.args[0] ?? '""'}).length) : null)`,
    javascript: (c) => `(${c.recv}.startsWith(${c.args[0] ?? '""'}) ? ${c.recv}.slice((${c.args[0] ?? '""'}).length) : null)`,
    rust: (c) => `${c.recv}.strip_prefix(${c.args.join(', ')})`,
  },
  strip_suffix: {
    python: (c) => `_dhv_strip_suffix(${c.recv}, ${c.args[0] ?? '""'})`,
    typescript: (c) => `(${c.recv}.endsWith(${c.args[0] ?? '""'}) ? ${c.recv}.slice(0, ${c.recv}.length - (${c.args[0] ?? '""'}).length) : null)`,
    javascript: (c) => `(${c.recv}.endsWith(${c.args[0] ?? '""'}) ? ${c.recv}.slice(0, ${c.recv}.length - (${c.args[0] ?? '""'}).length) : null)`,
    rust: (c) => `${c.recv}.strip_suffix(${c.args.join(', ')})`,
  },
  // find：Option 语义（interp str::find —— 未命中 None）
  find: {
    python: (c) => `_dhv_find(${c.recv}, ${c.args[0] ?? '""'})`,
    typescript: (c) => `(${c.recv}.indexOf(${c.args[0] ?? '""'}) >= 0 ? ${c.recv}.indexOf(${c.args[0] ?? '""'}) : null)`,
    javascript: (c) => `(${c.recv}.indexOf(${c.args[0] ?? '""'}) >= 0 ? ${c.recv}.indexOf(${c.args[0] ?? '""'}) : null)`,
    rust: (c) => `${c.recv}.find(${c.args.join(', ')})`,
  },
  // position：Option 语义（interp iter::position —— 未命中 None）
  position: {
    python: (c) => `next((i for i, _x in enumerate(${c.recv}) if (${c.args[0]})(_x)), None)`,
    typescript: (c) => `(${c.recv}.findIndex(${c.args[0]}) >= 0 ? ${c.recv}.findIndex(${c.args[0]}) : null)`,
    javascript: (c) => `(${c.recv}.findIndex(${c.args[0]}) >= 0 ? ${c.recv}.findIndex(${c.args[0]}) : null)`,
    rust: (c) => `${c.recv}.iter().position(${c.args[0]})`,
  },
  // Vec::enumerate → [(i, x)] 对列表（interp 返回 [i, v] 数组对；元组下标访问生成端已对齐为 [0]/[1]）
  enumerate: {
    python: (c) => `list(enumerate(${c.recv}))`,
    typescript: (c) => `${c.recv}.map((x, i) => [i, x])`,
    javascript: (c) => `${c.recv}.map((x, i) => [i, x])`,
    rust: (c) => `${c.recv}.iter().enumerate().collect::<Vec<_>>()`,
  },
  // Option::cloned —— python/ts/js 无所有权概念，值本身即克隆
  cloned: {
    python: (c) => `(${c.recv})`,
    typescript: (c) => `(${c.recv})`,
    javascript: (c) => `(${c.recv})`,
    rust: (c) => `${c.recv}.cloned()`,
  },
  as_str: {
    python: (c) => `str(${c.recv})`,
    typescript: (c) => `String(${c.recv})`,
    javascript: (c) => `String(${c.recv})`,
    rust: (c) => `${c.recv}.as_str()`,
  },
  trim_start: {
    python: (c) => `${c.recv}.lstrip()`,
    typescript: (c) => `${c.recv}.trimStart()`,
    javascript: (c) => `${c.recv}.trimStart()`,
    rust: (c) => `${c.recv}.trim_start()`,
  },
  trim_end: {
    python: (c) => `${c.recv}.rstrip()`,
    typescript: (c) => `${c.recv}.trimEnd()`,
    javascript: (c) => `${c.recv}.trimEnd()`,
    rust: (c) => `${c.recv}.trim_end()`,
  },
  // char_at：越界返回 ''（interp 语义）；python 切片 OOB 天然返回 ''
  char_at: {
    python: (c) => `${c.recv}[(${c.args[0] ?? 0}):((${c.args[0] ?? 0}) + 1)]`,
    typescript: (c) => `(${c.recv}[${c.args[0] ?? 0}] ?? '')`,
    javascript: (c) => `(${c.recv}[${c.args[0] ?? 0}] ?? '')`,
  },
  is_alphabetic: {
    python: (c) => `${c.recv}.isalpha()`,
    // v1.4.9：ts/js 正则与 interp 同源（[A-Za-z] 或 ≥ U+0080 —— UTF-8 下首字节 ≥ 0x80 即非 ASCII）
    typescript: (c) => `/[A-Za-z\\u0080-\\uFFFF]/.test(${c.recv})`,
    javascript: (c) => `/[A-Za-z\\u0080-\\uFFFF]/.test(${c.recv})`,
    rust: (c) => `${c.recv}.is_alphabetic()`,
    // v1.4.9：cpp/go 助手（UTF-8 首字节 ≥ 0x80 判非 ASCII，与 interp 正则语义精确对齐）
    cpp: (c) => `_dhvIsAlpha(${c.recv})`,
    go: (c) => `_dhvIsAlpha(${c.recv})`,
  },
  is_numeric: {
    python: (c) => `${c.recv}.isdigit()`,
    typescript: (c) => `/[0-9]/.test(${c.recv})`,
    javascript: (c) => `/[0-9]/.test(${c.recv})`,
    rust: (c) => `${c.recv}.is_numeric()`,
    cpp: (c) => `_dhvIsDigit(${c.recv})`,
    go: (c) => `_dhvIsDigit(${c.recv})`,
  },
  // v1.4.9：Result 消费面（Option-flavored 表示下与 is_some/is_none 同构）。
  // rust 原生 .is_ok()/.is_err()；其余语言 Err → None/null/nullopt/nil
  is_ok: {
    python: (c) => `(${c.recv} is not None)`,
    typescript: (c) => `(${c.recv} != null)`,
    javascript: (c) => `(${c.recv} != null)`,
    rust: (c) => `${c.recv}.is_ok()`,
    cpp: (c) => `${c.recv}.has_value()`,
    go: (c) => `(${c.recv} != nil)`,
  },
  is_err: {
    python: (c) => `(${c.recv} is None)`,
    typescript: (c) => `(${c.recv} == null)`,
    javascript: (c) => `(${c.recv} == null)`,
    rust: (c) => `${c.recv}.is_err()`,
    cpp: (c) => `(!${c.recv}.has_value())`,
    go: (c) => `(${c.recv} == nil)`,
  },
  // sort_by 是 key 语义（interp 按 f(x) 数值装饰排序，非比较器）：python sort(key=f) / rust sort_by_key
  sort_by: {
    python: (c) => `${c.recv}.sort(key=${c.args[0]})`,
    typescript: (c) => `${c.recv}.sort((a, b) => (${c.args[0]})(a) - (${c.args[0]})(b))`,
    javascript: (c) => `${c.recv}.sort((a, b) => (${c.args[0]})(a) - (${c.args[0]})(b))`,
    rust: (c) => `${c.recv}.sort_by_key(${c.args[0]})`,
    // v1.4.9：cpp stable_sort + 泛型 lambda comparator（key 语义：f(a) < f(b)；
    // std::stable_sort 稳定序与 interp Array.prototype.sort / rust sort_by_key 同源）
    cpp: (c) => `std::stable_sort(${c.recv}.begin(), ${c.recv}.end(), [&](const auto& _a, const auto& _b) { return (${c.args[0] ?? '_x'})(_a) < (${c.args[0] ?? '_x'})(_b); })`,
  },
  sort_desc: {
    python: (c) => `${c.recv}.sort(reverse=True)`,
    typescript: (c) => `${c.recv}.sort((a, b) => b - a)`,
    javascript: (c) => `${c.recv}.sort((a, b) => b - a)`,
    rust: (c) => `${c.recv}.sort_by(|a, b| b.cmp(a))`,
  },
};

// ---------------------------------------------------------------------------
// 翻译器
// ---------------------------------------------------------------------------

interface MatchArmOut { pattern: A.Pattern; guard?: A.Expr; body: string[] }

export class Body {
  private env = new Map<string, TyKind>();
  private parentEnv?: Map<string, TyKind>;
  /** 临时变量计数器：用于 if-let / while-let 的 scrutinee 缓存，避免副作用表达式被多次求值 */
  private tmpSeq = 0;
  constructor(public lang: LangSpec, public ctx: BodyCtx) {}

  /** 生成一个唯一的临时变量名（按语言关键字避让 + 加下划线前缀） */
  private freshTmp(prefix = 'scrut'): string {
    const n = `_${prefix}_${this.tmpSeq++}`;
    return ident(n, this.lang.id);
  }

  /**
   * 把 scrutinee 表达式缓存到一个临时变量，返回临时变量名 + 必要时已生成的赋值语句。
   * 用于 if-let / while-let（每次迭代只需一次求值），以及 match（避免多次求值）。
   */
  private hoistScrut(scrutExpr: string, indent: string, out: string[]): { tmp: string; prelude: string[] } {
    const prelude: string[] = [];
    const L = this.lang.id;
    // 简单标识符 / 字面量 / 不变表达式直接复用，无需 hoist
    if (/^[A-Za-z_][A-Za-z0-9_]*$/.test(scrutExpr)) return { tmp: scrutExpr, prelude };
    const tmp = this.freshTmp('scrut');
    if (L === 'python') prelude.push(`${indent}${tmp} = ${scrutExpr}`);
    else if (L === 'go') prelude.push(`${indent}${tmp} := ${scrutExpr}`);
    else if (L === 'cpp') prelude.push(`${indent}auto ${tmp} = ${scrutExpr};`);
    else if (L === 'rust') prelude.push(`${indent}let ${tmp} = ${scrutExpr};`);
    else prelude.push(`${indent}var ${tmp} = ${scrutExpr};`);
    return { tmp, prelude };
  }

  bindParam(name: string, ty?: A.HType): void {
    this.env.set(name, ty ? this.tyKind(ty) : 'unknown');
  }

  private tyKind(t: A.HType): TyKind {
    if (t.kind === 'path') {
      const h = t.segs[0]!;
      if (h === 'String' || h === 'str') return 'str';
      if (h === 'f32' || h === 'f64') return 'float';
      if (h === 'bool') return 'bool';
      if (/^[iu](8|16|32|64|128)$/.test(h) || h === 'usize' || h === 'isize') return 'int';
      if (h === 'Vec') return 'vec';
      if (h === 'HashMap') return 'map';
      if (h === 'Option') return 'option';
      if (h === 'Result') return 'result';
    }
    return 'unknown';
  }

  // ---------- 表达式 ----------
  expr(e: A.Expr): string {
    const L = this.lang.id;
    switch (e.kind) {
      case 'lit': {
        if (e.lit.t === 'bool') return L === 'python' ? ((e.lit.v as boolean) ? 'True' : 'False') : String(e.lit.v);
        if (e.lit.t === 'str') return this.ctx.strLit(e.lit.v as string);
        if (e.lit.t === 'char') return L === 'rust' ? `'${e.lit.v}'` : this.ctx.strLit(e.lit.v as string);
        return String(e.lit.v);
      }
      case 'path': {
        if (e.segs.length === 1) {
          // 裸 None 值：py None / ts-js null / rust None / cpp std::nullopt / go nil
          if (e.segs[0] === 'None') {
            if (L === 'python') return 'None';
            if (L === 'typescript' || L === 'javascript') return 'null';
            if (L === 'rust') return 'None';
            if (L === 'cpp') return 'std::nullopt';
            if (L === 'go') return 'nil';
          }
          // v0.2.51：std/math 常量（PI/E）—— 此前裸名直出，生成物 NameError
          if (e.segs[0] === 'PI' || e.segs[0] === 'E') {
            const n = e.segs[0]!;
            if (L === 'python') return `math.${n === 'PI' ? 'pi' : 'e'}`;
            if (L === 'typescript' || L === 'javascript') return `Math.${n}`;
            if (L === 'rust') return `std::f64::consts::${n}`;
            if (L === 'go') return `math.${n}`;
            if (L === 'cpp') return n === 'PI' ? '3.141592653589793' : '2.718281828459045';
          }
          return ident(e.segs[0]!, L);
        }
        return this.pathValue(e.segs);
      }
      case 'binary': {
        const op = e.op;
        const a = this.expr(e.lhs);
        const b = this.expr(e.rhs);
        if (op === '&&') return L === 'python' ? `(${a} and ${b})` : `(${a} && ${b})`;
        if (op === '||') return L === 'python' ? `(${a} or ${b})` : `(${a} || ${b})`;
        if (op === '/') {
          if (L === 'python') {
            const lk = this.exprKind(e.lhs), rk = this.exprKind(e.rhs);
            return (lk !== 'float' && rk !== 'float' && lk !== 'unknown' && rk !== 'unknown') ? `(${a} // ${b})` : `(${a} / ${b})`;
          }
          return `(${a} / ${b})`;
        }
        if (['+', '-', '*', '%', '<', '>', '<=', '>=', '==', '!=', '&', '|', '^', '<<', '>>'].includes(op)) return `(${a} ${op} ${b})`;
        throw new TranspileError(`二元运算 ${op}`);
      }
      case 'unary': {
        const v = this.expr(e.operand);
        if (e.op === '!') return L === 'python' ? `(not ${v})` : `(!${v})`;
        return `(${e.op}${v})`;
      }
      case 'assign': {
        const t = this.expr(e.target);
        const v = this.expr(e.value);
        return e.op === '=' ? `${t} = ${v}` : `${t} ${e.op}= ${v}`;
      }
      case 'call': {
        if (e.callee.kind === 'path') {
          const args = e.args.map((a) => this.expr(a));
          return this.pathCall(e.callee.segs, args);
        }
        throw new TranspileError('复杂调用链');
      }
      case 'method': {
        const recv = this.expr(e.recv);
        const args = e.args.map((a) => this.expr(a));
        // v1.4.9：透传原始实参 AST（go sort_by 闭包体内联替换需要）+ turbofish 泛型实参（parse::<T>）
        return this.method(recv, e.recv, e.name, args, e.args, e.generics);
      }
      case 'field': {
        const recv = this.expr(e.recv);
        // 元组下标访问：interp 把元组表示为数组（readField recv[name]）；
        // rust 原生 `t.0`，其余语言一律下标 `t[0]`（此前 `t.0` 在 python/ts/js 是非法语法）
        if (typeof e.name === 'number') {
          if (L === 'rust') return `${recv}.${e.name}`;
          return `${recv}[${e.name}]`;
        }
        const f = ident(String(e.name), L);
        return L === 'objectivec' ? `[${recv} ${f}]` : `${recv}.${f}`;
      }
      case 'index': {
        return `${this.expr(e.recv)}[${this.expr(e.index)}]`;
      }
      case 'cast': return this.expr(e.expr);
      case 'tuple': {
        if (e.items.length !== 2) throw new TranspileError('元组字面量');
        const a = this.expr(e.items[0]!), b = this.expr(e.items[1]!);
        return L === 'python' ? `(${a}, ${b})` : `[${a}, ${b}]`;
      }
      case 'array': {
        const items = e.items.map((x) => this.expr(x));
        if (L === 'rust') return `vec![${items.join(', ')}]`;
        // v1.4.8：cpp 用 CTAD std::vector{...}（C++17+ class template arg deduction）——
        // 此前 [1, 2] 是 lambda 捕获语法（非法数组字面量）
        if (L === 'cpp') return `std::vector{${items.join(', ')}}`;
        // v1.4.8：go []T{} 切片字面量（与 Vec 切片头语义对齐；[N]M 是固定数组非切片）
        if (L === 'go') return `[]any{${items.join(', ')}}`;
        return `[${items.join(', ')}]`;
      }
      case 'arrayrep': {
        const v = this.expr(e.value), n = this.expr(e.count);
        if (L === 'python') return `[${v}] * ${n}`;
        if (L === 'rust') return `vec![${v}; ${n}]`;
        if (L === 'typescript' || L === 'javascript') return `Array.from({ length: ${n} }, () => ${v})`;
        throw new TranspileError('arrayrep');
      }
      case 'struct': return this.structLit(e);
      case 'if': {
        const c = this.expr(e.cond), a = this.expr(e.then);
        const b = e.els ? this.expr(e.els) : (L === 'python' ? 'None' : L === 'rust' ? '()' : 'null');
        return L === 'python' ? `(${a} if ${c} else ${b})` : `(${c} ? ${a} : ${b})`;
      }
      case 'closure': {
        const ps = e.params.map((p) => ident(patternName(p.pat) ?? '_x', L));
        if (e.body.kind === 'block') throw new TranspileError('多语句闭包');
        const v = this.expr(e.body);
        if (L === 'python') return `(lambda ${ps.join(', ')}: ${v})`;
        if (L === 'rust') return `|${ps.join(', ')}| ${v}`;
        // v1.4.8：cpp lambda（[&] 捕获外层变量 + auto 参数 + decltype 推导返回；
        // 与 _dhvOptMap/_dhvOptAndThen 模板助手对接）
        // go 闭包需显式参数类型（HSL 闭包无类型注解，无法满足），诚实 throw 回退 contract
        if (L === 'cpp') return `[&](${ps.map((p) => `auto ${p}`).join(', ')}) { return ${v}; }`;
        return `(${ps.join(', ')}) => ${v}`;
      }
      case 'macro': return this.macro(e);
      case 'native': {
        if (this.nativeMatch(e.lang)) return e.body;
        throw new TranspileError(`native ${e.lang} ≠ 目标 ${L}`);
      }
      case 'await': {
        const v = this.expr(e.expr);
        if (L === 'rust') return `${v}.await`;
        if (L === 'python' || L === 'typescript' || L === 'javascript') return `await ${v}`;
        throw new TranspileError('await');
      }
      case 'range': {
        // v0.2.28: 修复 Python inclusive bug（此前总是 +1）；补齐半开 range
        const lo = e.lo ? this.expr(e.lo) : undefined;
        const hi = e.hi ? this.expr(e.hi) : undefined;
        if (L === 'python') {
          if (!hi) throw new TranspileError('值语境 range（无上界）');
          return `range(${lo ?? '0'}, ${hi}${e.inclusive ? ' + 1' : ''})`;
        }
        if (L === 'rust') return `${lo ?? ''}..${hi ?? ''}${e.inclusive ? '=' : ''}`;
        // 其他语言：值语境 range 作为值暂不支持，for 内联由 forRangeLines 处理
        throw new TranspileError('值语境 range');
      }
      case 'slice': {
        const recv = this.expr(e.recv);
        const lo = e.lo ? this.expr(e.lo) : '';
        const hi = e.hi ? this.expr(e.hi) : '';
        if (L === 'python') {
          if (e.inclusive) throw new TranspileError('含端切片');
          return `${recv}[${lo}:${hi}]`;
        }
        if (L === 'typescript' || L === 'javascript') return `${recv}.slice(${lo ? lo + ', ' : ''}${hi})`;
        if (L === 'rust') return `${recv}[${lo}..${hi}${e.inclusive ? '=' : ''}]`;
        throw new TranspileError('切片');
      }
      case 'unit': return L === 'python' ? 'None' : L === 'rust' ? '()' : 'undefined';
      default:
        throw new TranspileError(`表达式 ${e.kind} 不在子集`);
    }
  }

  private nativeMatch(lang: string): boolean {
    if (lang === this.lang.id) return true;
    if (lang === 'typescript' && this.lang.id === 'typescript') return true;
    return false;
  }

  private pathValue(segs: string[]): string {
    const L = this.lang.id;
    const head = segs[0]!;
    const tail = segs[segs.length - 1]!;
    // 枚举无负载变体（值位置）
    const en = this.ctx.enums.get(head);
    if (en && segs.length === 2 && en.variants.some((v) => v.name === tail)) {
      const v = en.variants.find((x) => x.name === tail)!;
      if (!v.fields) {
        if (L === 'python' || L === 'typescript' || L === 'javascript') return snakeUpper(tail);
        if (L === 'rust') return `${head}::${tail}`;
        if (L === 'go') return `${tail}{}`;
      }
      throw new TranspileError('带负载变体在值位置需构造调用');
    }
    if (head === 'Option' && tail === 'None') {
      if (L === 'python') return 'None';
      if (L === 'typescript' || L === 'javascript') return 'null';
      if (L === 'rust') return 'None';
      if (L === 'cpp') return 'std::nullopt';
      if (L === 'go') return 'nil';
    }
    throw new TranspileError(`路径值 ${segs.join('::')}`);
  }

  private pathCall(segs: string[], args: string[]): string {
    const L = this.lang.id;
    const head = segs[0]!;
    const tail = segs[segs.length - 1]!;
    if (segs.length === 1) {
      // v0.2.51：std/math 自由函数映射 —— 此前裸名直出（sin(x) → sin(x)），
      // python/ts 生成物 NameError / 编译错误。rust/go/cpp 的自由函数是方法
      // 形态（(x).sin()），不可廉价映射 —— 抛错触发诚实 contract 回退。
      const mathed = stdMathFreeCall(head, args, L);
      if (mathed !== null) return mathed;
      if (head === 'Some') {
        if (L === 'python' || L === 'typescript' || L === 'javascript') return args[0] ?? 'None';
        if (L === 'rust') return `Some(${args.join(', ')})`;
        // cpp/go：类型推导助手（cpp 模板推导 / go 泛型 1.18+，定义在文件头 prelude）
        if (L === 'cpp' || L === 'go') return `_dhvSome(${args[0] ?? ''})`;
        throw new TranspileError('Some 构造');
      }
      if (head === 'None') {
        if (L === 'python') return 'None';
        if (L === 'typescript' || L === 'javascript') return 'null';
        if (L === 'rust') return 'None';
        if (L === 'cpp') return 'std::nullopt';
        if (L === 'go') return 'nil';
      }
      if (head === 'Ok' || head === 'Err') throw new TranspileError('Result 构造不在活体子集');
      return `${ident(head, L)}(${args.join(', ')})`;
    }
    const second = segs[1]!;
    if (head === 'String' && second === 'from') {
      if (L === 'python') return args[0] ?? "''";
      if (L === 'typescript' || L === 'javascript') return `String(${args[0] ?? ''})`;
      if (L === 'rust') return `String::from(${args.join(', ')})`;
      if (L === 'go') return args[0] ?? '""';
      if (L === 'cpp') return `std::string(${args[0] ?? '""'})`;
    }
    if (head === 'Vec' && second === 'new') {
      if (L === 'python' || L === 'typescript' || L === 'javascript') return '[]';
      if (L === 'rust') return 'Vec::new()';
      throw new TranspileError('Vec::new 需类型');
    }
    if (head === 'Vec' && second === 'from') {
      if (L === 'python' || L === 'typescript' || L === 'javascript') return `[${args.join(', ')}]`;
      if (L === 'rust') return `Vec::from(${args.join(', ')})`;
    }
    if (head === 'HashMap' && second === 'new') {
      if (L === 'python') return '{}';
      if (L === 'typescript' || L === 'javascript') return 'new Map()';
      if (L === 'rust') return 'HashMap::new()';
      throw new TranspileError('HashMap::new 需类型');
    }
    if (head === 'Option' && second === 'Some') {
      if (L === 'python' || L === 'typescript' || L === 'javascript') return args[0] ?? 'None';
      if (L === 'rust') return `Some(${args.join(', ')})`;
      if (L === 'cpp' || L === 'go') return `_dhvSome(${args[0] ?? ''})`;
    }
    if (head === 'Option' && second === 'None') {
      if (L === 'python') return 'None';
      if (L === 'typescript' || L === 'javascript') return 'null';
      if (L === 'rust') return 'None';
    }
    const en = this.ctx.enums.get(head);
    if (en && en.variants.some((v) => v.name === tail)) {
      if (L === 'python' || L === 'typescript' || L === 'javascript') return `${tail}(${args.join(', ')})`;
      if (L === 'rust') return `${head}::${tail}${args.length ? `(${args.join(', ')})` : ''}`;
      if (L === 'go') return `${tail}{${args.join(', ')}}`;
      if (L === 'cpp') return `${tail}{${args.join(', ')}}`;
    }
    if (tail === 'new') {
      if (L === 'python') return `${head}(${args.join(', ')})`;
      if (L === 'typescript' || L === 'javascript') return `new ${head}(${args.join(', ')})`;
      if (L === 'rust') return `${head}::new(${args.join(', ')})`;
      if (L === 'go') return `New${head}(${args.join(', ')})`;
      if (L === 'cpp') return `${head}(${args.join(', ')})`;
    }
    if (segs.length === 2 && !en) {
      // Trait::method / 模块函数 → 自由函数调用
      return `${ident(tail, L)}(${args.join(', ')})`;
    }
    throw new TranspileError(`路径调用 ${segs.join('::')}`);
  }

  private structLit(e: A.Expr & { kind: 'struct' }): string {
    const L = this.lang.id;
    const name = e.segs[e.segs.length - 1]!;
    if (e.fields.some((f) => f.base)) throw new TranspileError('..base 更新语法');
    if (e.fields.some((f) => !f.value)) throw new TranspileError('字段简写');
    const vals = e.fields.map((f) => this.expr(f.value!));
    if (L === 'python') return `${name}(${e.fields.map((f, i) => `${ident(f.name, L)}=${vals[i]}`).join(', ')})`;
    if (L === 'typescript' || L === 'javascript') return `${lowerFirst(name)}(${vals.join(', ')})`;
    if (L === 'rust') return `${name} { ${e.fields.map((f, i) => `${f.name}: ${vals[i]}`).join(', ')} }`;
    if (L === 'go') return `&${name}{${e.fields.map((f, i) => `${capitalize(f.name)}: ${vals[i]}`).join(', ')}}`;
    if (L === 'cpp') return `${name}{${vals.join(', ')}}`;
    throw new TranspileError('struct 字面量');
  }

  private method(recv: string, recvExpr: A.Expr, name: string, args: string[], rawArgs?: A.Expr[], generics?: A.HType[]): string {
    const kind = this.exprKind(recvExpr);
    const L = this.lang.id;
    // ---- v1.4.9：None 字面量接收者的链式方法专门派发 ----
    // 🔴 修复一整类「启发式校验盲区」bug：裸 Option::None（无注解 let 中转）在 cpp 生成
    // `_dhvOptMap(std::nullopt, f)`（nullopt_t 模板推导失败）/ `std::nullopt.value_or(0)` /
    // `*std::nullopt`（均编译必炸）；go 生成 `*nil`（untyped nil 解引用非法）。
    // v1.4.8 测试全用带注解 let（a: Option<i64> = Option::None → optional/指针类型 ✓）故未暴露。
    // 语义化简（单次求值，精确）：None.unwrap_or(d) ≡ d；None.or(alt) ≡ alt；
    // None.is_some/is_ok ≡ false；None.is_none/is_err ≡ true；map/and_then/filter ≡ None
    //（cpp 用 _dhvNoneT 链式包装器支撑后续 .value_or/.has_value 链）。
    const isNoneRecv = recvExpr.kind === 'path' && (
      (recvExpr.segs.length === 1 && recvExpr.segs[0] === 'None') ||
      (recvExpr.segs.length === 2 && recvExpr.segs[0] === 'Option' && recvExpr.segs[1] === 'None')
    );
    if (isNoneRecv && (L === 'cpp' || L === 'go')) {
      if (name === 'unwrap_or') return `(${args[0] ?? '0'})`;
      if (name === 'or') return `(${args[0] ?? (L === 'cpp' ? 'std::nullopt' : 'nil')})`;
      if (name === 'is_some' || name === 'is_ok') return 'false';
      if (name === 'is_none' || name === 'is_err') return 'true';
      if (L === 'cpp' && (name === 'map' || name === 'and_then' || name === 'filter' || name === 'unwrap_or_else')) {
        return `_dhvNone.${name}(${args[0] ?? ''})`;
      }
      // unwrap/expect（interp 运行期即中止）与 go 的闭包族（untyped nil 无类型通道）
      // —— 诚实 throw 回退 contract（宁缺毋滥）
      throw new TranspileError(`None.${name} 裸 None 接收者`);
    }
    if (name === 'parse') {
      // ---- v1.4.9：String::parse::<T>()（turbofish 泛型实参首次接线，此前全部语言回退 contract） ----
      // 语义：interp 返回 Result<T, String>；生成端非 rust 语言采用 Option-flavored Result 表示
      // （Err → None/null/nullopt/nil，错误消息不可观察 —— 已知限制，文档化）。
      // 链式消费（unwrap_or/unwrap/expect/is_ok/is_err）复用既有 Option 映射，表示同构无缝衔接。
      const g = generics?.[0];
      const gName = g && g.kind === 'path' ? g.segs.join('::') : '';
      if (L === 'rust') return `${recv}.parse::<${gName || 'f64'}>()`; // rust 原生 turbofish 直投
      const isInt = gName.startsWith('i') || gName.startsWith('u');
      if (L === 'python') return isInt ? `_dhv_parse_int(${recv}, '${gName || 'i64'}')` : `_dhv_parse_float(${recv})`;
      if (L === 'typescript' || L === 'javascript') return isInt ? `_dhvParseInt(${recv}, '${gName || 'i64'}')` : `_dhvParseFloat(${recv})`;
      if (L === 'cpp') return `_dhvParse<${this.ctx.ty(g ?? { kind: 'path', segs: ['f64'] })}>(${recv})`;
      if (L === 'go') return isInt ? `_dhvParseInt(${recv}, ${gName.startsWith('u')})` : `_dhvParseFloat(${recv})`;
      throw new TranspileError('parse');
    }
    // ---- v1.4.9：Option::filter（interp 同步新增 builtin；Vec::filter 同名不同义，走通用表不变） ----
    if (name === 'filter' && kind === 'option') {
      const f = args[0] ?? '(_x) => _x';
      if (L === 'rust') return `${recv}.filter(${f})`;
      if (L === 'python') return `_dhv_filter(${recv}, ${f})`;
      if (L === 'typescript' || L === 'javascript') return `_dhvFilter(${recv}, ${f})`;
      if (L === 'cpp') return `_dhvOptFilter(${recv}, ${f})`;
      // go：表达式位置无三元 + 闭包需显式类型 → 诚实 throw 回退 contract（宁缺毋滥）
      throw new TranspileError('Option::filter');
    }
    // ---- v1.4.9：go Vec::sort_by（闭包体内联替换 —— go func literal 需显式参数类型无法直投闭包值，
    // 但 sort.SliceStable 的 comparator 只需把闭包体中参数引用替换为 v[i]/v[j] 即可内联合成） ----
    if (name === 'sort_by' && L === 'go') {
      const cl = rawArgs?.[0];
      if (!cl || cl.kind !== 'closure' || cl.params.length !== 1) throw new TranspileError('go sort_by 需单参闭包');
      const pName = patternName(cl.params[0]!.pat);
      if (!pName || pName === '_') throw new TranspileError('go sort_by 闭包参数需可命名');
      const ka = this.expr(substParam(cl.body, pName, `${recv}[i]`, 'go'));
      const kb = this.expr(substParam(cl.body, pName, `${recv}[j]`, 'go'));
      // interp sort_by 是 key 语义（稳定排序 + 数值 key 比较）；sort.SliceStable 稳定 ✓
      return `sort.SliceStable(${recv}, func(i, j int) bool { return ${ka} < ${kb} })`;
    }
    // ---- 类型感知同名分发（interp 按接收者运行时分发，生成端按静态 kind 分发） ----
    // String::to_string：接收者已是字符串 —— cpp 的 std::to_string 只接受数值
    // （`"non-point".to_string()` 曾生成 std::to_string("non-point") 非法代码）
    if (name === 'to_string' && kind === 'str' && L === 'cpp') {
      return `std::string(${recv})`;
    }
    // String::contains 与 Vec::contains 同名不同义：String 是子串查找。
    // 🔴 v1.4.7 修复：此前 cpp 走 Vec 表生成 std::find(s.begin(), s.end(), "x")
    // （char 与 const char* 比较 = g++ 编译错误）、go 生成 slices.Contains(s, "x")
    // （string 非切片 = 编译错误）—— 均通过启发式平衡校验但真机编译必炸
    if (name === 'contains' && kind === 'str') {
      const x = args[0] ?? '""';
      if (L === 'go') return `strings.Contains(${recv}, ${x})`;
      if (L === 'cpp') return `(${recv}.find(${x}) != std::string::npos)`;
      // py/ts/js/rust 走通用表（`x in s` / includes / contains 均正确）
    }
    // Option::map 与 Vec::map 同名不同义：Option 版本返回 Option（生成端 Some 透明 → 直接返回 f 值或 null）
    // v1.4.8：cpp _dhvOptMap 模板助手（lambda 用 auto 参数 + decltype 推导 std::optional<R>）
    // go 闭包需显式类型，HSL 闭包无类型注解 → 诚实 throw 回退 contract
    if (name === 'map' && kind === 'option') {
      const f = args[0] ?? '(_x) => _x';
      if (L === 'python') return `(${recv} if ${recv} is None else (${f})(${recv}))`;
      if (L === 'typescript' || L === 'javascript') return `(${recv} != null ? (${f})(${recv}) : null)`;
      if (L === 'rust') return `${recv}.map(${f})`;
      if (L === 'cpp') return `_dhvOptMap(${recv}, ${f})`;
      throw new TranspileError('Option::map');
    }
    // Vec::insert(i, x) 与 HashMap::insert(k, v) 同名不同义：
    // Vec 版本按下标插入（interp splice），Map 版本是键值写入
    if (name === 'insert' && kind === 'vec') {
      const i = args[0] ?? '0', x = args[1] ?? 'None';
      if (L === 'python') return `${recv}.insert(${i}, ${x})`;
      if (L === 'typescript' || L === 'javascript') return `${recv}.splice(${i}, 0, ${x})`;
      if (L === 'rust') return `${recv}.insert(${i}, ${x})`;
      // cpp/go：_dhvInsert 助手（越界 clean throw/panic，与 Rust 语义同源；
      // go 三语句 append+copy+赋值不可内联）
      if (L === 'cpp') return `_dhvInsert(${recv}, ${i}, ${x})`;
      if (L === 'go') return `_dhvInsert(&${recv}, ${i}, ${x})`;
      throw new TranspileError('Vec::insert');
    }
    // Vec::remove(i) 返回被删元素（interp splice(i,1)[0]）；HashMap::remove(k) 返回 Option 旧值
    if (name === 'remove' && kind === 'vec') {
      const i = args[0] ?? '0';
      if (L === 'python') return `${recv}.pop(${i})`;
      if (L === 'typescript' || L === 'javascript') return `${recv}.splice(${i}, 1)[0]`;
      if (L === 'rust') return `${recv}.remove(${i})`;
      // cpp/go：_dhvRemoveAt 助手（返回被删元素；越界 clean throw/panic ——
      // std::vector::erase 返回 iterator 非元素；go append 切片拼接）
      if (L === 'cpp') return `_dhvRemoveAt(${recv}, ${i})`;
      if (L === 'go') return `_dhvRemoveAt(&${recv}, ${i})`;
      throw new TranspileError('Vec::remove');
    }
    // Vec::get / HashMap::get → Option（interp 语义：越界/缺失返回 None，而非 panic）
    if (name === 'get') {
      const k = args[0] ?? '0';
      if (L === 'python') return `_dhv_get(${recv}, ${k})`;
      if (L === 'typescript' || L === 'javascript') {
        if (kind === 'map') return `(${recv}.get(${k}) ?? null)`;
        if (kind === 'vec') return `(${recv}[${k}] ?? null)`;
        return `((${recv}) instanceof Map ? (${recv}.get(${k}) ?? null) : (${recv}[${k}] ?? null))`;
      }
      if (L === 'rust') return `${recv}.get(${k})`;
      // v1.4.7：cpp/go 类型感知 Option 语义（关闭 v1.4.3 遗留的「下标近似」历史行为）——
      // vec → _dhvVecGet（越界 nullopt/nil）；map → _dhvMapGet（缺键 nullopt/nil）。
      // unknown → 下标近似（静态类型无运行时分发通道，语义由 @dhv:source-map 围栏纪律保障）
      if (L === 'cpp' && kind === 'vec') return `_dhvVecGet(${recv}, ${k})`;
      if (L === 'cpp' && kind === 'map') return `_dhvMapGet(${recv}, ${k})`;
      if (L === 'go' && kind === 'vec') return `_dhvVecGet(${recv}, ${k})`;
      if (L === 'go' && kind === 'map') return `_dhvMapGet(${recv}, ${k})`;
      const gt = METHOD_TABLE['get']![L];
      if (!gt) throw new TranspileError('.get() 该语言未映射');
      return gt({ recv, args, kind });
    }
    const tbl = METHOD_TABLE[name];
    if (!tbl) throw new TranspileError(`方法 .${name}() 不在映射表`);
    const render = tbl[this.lang.id];
    if (!render) throw new TranspileError(`.${name}() 该语言未映射`);
    return render({ recv, args, kind });
  }

  /** let x = <if/match/if-let 块表达式> —— 声明 + 分支尾赋值模式（v1.4.7） */
  private letBlockInit(st: A.Stmt & { kind: 'let' }, safe: string, indent: string, out: string[]): void {
    const L = this.lang.id;
    const init = st.init!;
    // 值语境分支必须齐全（无 else 的 if/if-let 类型为 ()，不能作右值）
    if (init.kind === 'if' && !init.els) throw new TranspileError('let 块初始化 if 需 else 分支');
    if (init.kind === 'iflet' && !init.els) throw new TranspileError('let 块初始化 if-let 需 else 分支');
    // 声明形式：python 免声明（分支内首次赋值即声明）；ts/js `let x;`；rust `let x;`
    //（延迟初始化合法）；cpp/go 需显式类型（注解在场照实投射，否则按分支尾 kind 推导基元类型，
    // 推导不出 → 诚实 throw 回退 contract）
    if (L === 'python') {
      // 免声明
    } else if (L === 'typescript' || L === 'javascript') {
      out.push(`${indent}let ${safe};`);
    } else if (L === 'rust') {
      out.push(`${indent}let ${st.mut ? 'mut ' : ''}${safe};`);
    } else {
      // cpp / go：需显式类型。注解在场照实投射；否则按分支尾 kind 推导基元类型
      //（宽松策略：所有分支必须同型才合法，取首个可推导分支即可钉住类型 ——
      // iflet 的 then 分支常引用模式绑定（kind unknown），但 else 常是字面量可推导）。
      // 推导不出 → 诚实 throw 回退 contract
      const kind = st.ty ? this.tyKind(st.ty) : this.blockExprKind(init);
      let tyText: string | null = null;
      if (st.ty) {
        tyText = this.ctx.ty(st.ty);
      } else {
        tyText = this.primTypeOf(kind);
      }
      if (!tyText) throw new TranspileError(`let 块初始化需类型注解（无法推导 ${kind}）`);
      out.push(L === 'cpp' ? `${indent}${tyText} ${safe};` : `${indent}var ${safe} ${tyText}`);
    }
    // 控制流 + 分支尾赋值（assign 模式：mode = 变量名）
    if (init.kind === 'if') {
      this.ifChain(init, indent, out, safe);
    } else if (init.kind === 'iflet') {
      this.ifLet(init, indent, out, safe);
    } else {
      const m = init as A.Expr & { kind: 'match' };
      this.matchDispatch(this.expr(m.expr), m.arms, indent, out, safe);
    }
  }

  /** TyKind → 语言基元类型（cpp/go 无注解 let 块初始化的声明类型推导；null = 推导不出） */
  private primTypeOf(kind: TyKind): string | null {
    const L = this.lang.id;
    switch (kind) {
      case 'int': return L === 'rust' ? 'i64' : L === 'go' ? 'int64' : L === 'cpp' ? 'int64_t' : null;
      case 'float': return L === 'rust' ? 'f64' : L === 'go' ? 'float64' : L === 'cpp' ? 'double' : null;
      case 'str': return L === 'rust' ? 'String' : L === 'go' ? 'string' : L === 'cpp' ? 'std::string' : null;
      case 'bool': return L === 'rust' ? 'bool' : L === 'go' ? 'bool' : L === 'cpp' ? 'bool' : null;
      default: return null;
    }
  }

  /** 块尾表达式 kind（if/match 分支值推导）：块取末尾无分号表达式，非块直接 exprKind */
  private blockTailKind(e?: A.Expr): TyKind {
    if (!e) return 'unknown';
    if (e.kind === 'block') {
      const last = e.stmts[e.stmts.length - 1];
      if (last && last.kind === 'expr' && !last.hasSemi) return this.exprKind(last.expr);
      return 'unknown';
    }
    return this.exprKind(e);
  }

  /** let 块初始化的宽松类型推导：所有分支必须同型才合法，取首个可推导分支钉住类型
   *（iflet 的 then 分支常引用模式绑定 —— kind unknown，但 else 字面量可推导） */
  private blockExprKind(e: A.Expr): TyKind {
    if (e.kind === 'if' || e.kind === 'iflet') {
      const thenK = this.blockTailKind(e.then);
      if (thenK !== 'unknown') return thenK;
      return e.els ? this.blockTailKind(e.els) : 'unknown';
    }
    if (e.kind === 'match') {
      for (const a of e.arms) {
        const k = this.blockTailKind(a.body);
        if (k !== 'unknown') return k;
      }
      return 'unknown';
    }
    return this.exprKind(e);
  }

  exprKind(e: A.Expr): TyKind {
    switch (e.kind) {
      case 'lit': return e.lit.t === 'int' ? 'int' : e.lit.t === 'float' ? 'float' : e.lit.t === 'str' ? 'str' : e.lit.t === 'bool' ? 'bool' : 'unknown';
      case 'path': {
        if (e.segs.length === 1) return this.env.get(e.segs[0]!) ?? this.parentEnv?.get(e.segs[0]!) ?? 'unknown';
        // v1.4.9：两段 builtin 路径值（Option::None / Result::Ok 裸变体等）——
        // 此前返回 unknown，导致 `Option::None.filter(f)` 走 Vec 分支回退 contract
        if (e.segs.length === 2) {
          if (e.segs[0] === 'Option') return 'option';
          if (e.segs[0] === 'Result') return 'result';
        }
        return 'unknown';
      }
      case 'binary': {
        if (['/', '*', '-', '+'].includes(e.op)) {
          const lk = this.exprKind(e.lhs), rk = this.exprKind(e.rhs);
          if (lk === 'float' || rk === 'float') return 'float';
          if (lk === 'int' && rk === 'int') return 'int';
          return 'unknown';
        }
        if (['==', '!=', '<', '>', '<=', '>=', '&&', '||'].includes(e.op)) return 'bool';
        return 'unknown';
      }
      case 'unary': return e.op === '!' ? 'bool' : this.exprKind(e.operand);
      case 'cast': return this.tyKind(e.ty);
      case 'method': {
        if (e.name === 'len' || e.name === 'sum') return 'int';
        if (['to_string', 'join', 'trim', 'to_lowercase', 'to_uppercase', 'replace', 'as_str', 'char_at'].includes(e.name)) return 'str';
        if (['is_empty', 'contains', 'starts_with', 'ends_with', 'is_some', 'is_none', 'is_ok', 'is_err', 'is_alphabetic', 'is_numeric', 'is_sorted', 'any', 'all'].includes(e.name)) return 'bool';
        if (e.name === 'clone' || e.name === 'iter') return this.exprKind(e.recv);
        // Option 保留：get/first/last/pop 返回 Option；map/and_then/or/cloned 保留接收者 Option 性
        if (['get', 'first', 'last', 'pop'].includes(e.name)) {
          const rk = this.exprKind(e.recv);
          return rk === 'vec' || rk === 'map' || rk === 'unknown' ? 'option' : 'unknown';
        }
        if (['map', 'and_then', 'or', 'cloned'].includes(e.name)) {
          const rk = this.exprKind(e.recv);
          return rk === 'option' || rk === 'result' ? rk : 'unknown';
        }
        return 'unknown';
      }
      // v1.4.8：Option::Some/None、Result::Ok/Err、Vec::from、HashMap::new 等路径调用的返回 kind
      case 'call': {
        if (e.callee.kind === 'path') {
          const p = e.callee.segs;
          const last = p[p.length - 1]!;
          const root = p[0]!;
          if (root === 'Option' && (last === 'Some' || last === 'None')) return 'option';
          if (root === 'Result' && (last === 'Ok' || last === 'Err')) return 'result';
          if (root === 'Vec' && last === 'from') return 'vec';
          if (root === 'HashMap' && last === 'new') return 'map';
        }
        return 'unknown';
      }
      // v1.4.8：vec! 宏返回 kind = 'vec'（此前为 unknown，导致 v.get(i) 对 vec! 字面量退化为下标）
      case 'macro': {
        const name = e.path[e.path.length - 1]!;
        if (name === 'format') return 'str';
        if (name === 'vec') return 'vec';
        return 'unknown';
      }
      case 'await': return this.exprKind(e.expr);
      // v1.4.7：if / match 块表达式的分支值推导（两/全部分支尾同 kind 才返回该 kind，
      // 供 let 块初始化的 cpp/go 无注解声明类型推导与后续方法类型感知分发）
      case 'if': {
        const tk = this.blockTailKind(e.then);
        if (tk === 'unknown') return 'unknown';
        const ek2 = e.els ? this.blockTailKind(e.els) : 'unknown';
        return tk === ek2 ? tk : 'unknown';
      }
      case 'match': {
        if (e.arms.length === 0) return 'unknown';
        const kinds = e.arms.map((a) => this.blockTailKind(a.body));
        const first = kinds[0]!;
        return kinds.every((k) => k === first) ? first : 'unknown';
      }
      default: return 'unknown';
    }
  }

  // ---------- 宏 ----------
  private macro(e: A.Expr & { kind: 'macro' }): string {
    const name = e.path[e.path.length - 1]!;
    // 剥掉最外层定界符（与 interp.macroArgTokens 同构），保留内层结构
    const inner = e.tree.t === 'delim' ? e.tree.items : [e.tree];
    const toks: FlatTok[] = [];
    for (const it of inner) toks.push(...flatTokens(it));
    const groups = splitTopLevel(toks);
    if (name === 'format') return this.formatString(macroFmtString(groups), groups.slice(1).map((g) => this.tokensToExpr(g)));
    if (name === 'println' || name === 'print' || name === 'eprintln') {
      const text = groups.length === 0 ? this.ctx.strLit('') : this.formatString(macroFmtString(groups), groups.slice(1).map((g) => this.tokensToExpr(g)));
      // v0.2.53 修复（rustc 真机实测）：rust 的 println!/print!/eprintln! 宏实参
      // 必须是「格式串 + 位置参数」，不能是 format!(…) 调用 —— 此前生成
      // println!(format!("…", a))，rustc 报 "format argument must be a string literal"
      // （所有 println 样本必炸，但被 emit 启发式校验绿灯掩盖）。
      // 剥掉 format!( … ) 外壳取内芯；其余语言 print/console/fmt 接受任意表达式，不受影响。
      if (this.lang.id === 'rust' && text.startsWith('format!(') && text.endsWith(')')) {
        return `${name}!(${text.slice(8, -1)})`;
      }
      return this.printCall(text, name !== 'print');
    }
    if (name === 'vec') {
      const items = groups.map((g) => this.tokensToExpr(g));
      const L = this.lang.id;
      if (L === 'rust') return `vec![${items.join(', ')}]`;
      // v1.4.8：cpp CTAD std::vector{...}（此前 [1, 2] 是 lambda 捕获非法语法）
      if (L === 'cpp') return `std::vector{${items.join(', ')}}`;
      // v1.4.8：go []any{...} 切片字面量（与 Vec 切片头语义对齐）
      if (L === 'go') return `[]any{${items.join(', ')}}`;
      return `[${items.join(', ')}]`;
    }
    if (name === 'panic') {
      const t = groups.length ? this.tokensToExpr(groups[0]!) : this.ctx.strLit('panic!');
      const L = this.lang.id;
      if (L === 'python') return `raise RuntimeError(${t})`;
      if (L === 'typescript' || L === 'javascript') return `throw new Error(${t})`;
      if (L === 'rust') return `panic!(${t})`;
      if (L === 'go') return `panic(${t})`;
      return `throw std::runtime_error(${t})`;
    }
    if (name === 'assert' || name === 'assert_eq') {
      const a = this.tokensToExpr(groups[0] ?? []);
      const b = groups[1] ? this.tokensToExpr(groups[1]) : null;
      const L = this.lang.id;
      const cond = b ? `${a} == ${b}` : a;
      if (L === 'python') return `assert ${cond}`;
      if (L === 'typescript' || L === 'javascript') return `if (!(${cond})) throw new Error('assertion failed')`;
      if (L === 'rust') return b ? `assert_eq!(${a}, ${b})` : `assert!(${a})`;
      if (L === 'go') return `if !(${cond}) { panic('assertion failed') }`;
      return `if (!(${cond})) throw std::runtime_error("assertion failed")`;
    }
    throw new TranspileError(`宏 ${name}!`);
  }

  private printCall(text: string, newline: boolean): string {
    const L = this.lang.id;
    if (L === 'python') return `print(${text})`;
    if (L === 'typescript' || L === 'javascript') return newline ? `console.log(${text})` : `process.stdout.write(String(${text}))`;
    if (L === 'rust') return `println!(${text})`;
    if (L === 'go') return `fmt.Println(${text})`;
    return `std::cout << (${text}) << std::endl;`;
  }

  private tokensToExpr(toks: FlatTok[]): string {
    if (toks.length === 0) throw new TranspileError('空宏参数');
    // v0.2.51 修复：string/char/rawstr token 的 text 是**不带引号的裸内容** ——
    // 此前直接 join 再重词法化，宏实参里的字符串字面量丢失引号：含 `'` 的
    // （如 SQL 注入签名 "' or 1=1"）触发 LexError；不含特殊字符的静默变成
    // 标识符（错误但不报错）。现按 token 类型重建可词法化源码。
    const requote = (t: FlatTok): string => {
      switch (t.kind) {
        case 'string':
          return JSON.stringify(t.text);
        case 'char':
          return `'${t.text.replace(/\\/g, '\\\\').replace(/'/g, "\\'")}'`;
        case 'rawstr':
          return t.text.includes('"') ? `r#"${t.text}"#` : `r"${t.text}"`;
        default:
          return t.text;
      }
    };
    const src = toks.map(requote).join(' ');
    const lexToks = new Lexer(src, '<macro-emit>').tokenize();
    const exprs = parseExprsFromTokens(lexToks, '<macro-emit>');
    if (exprs.length !== 1) throw new TranspileError('宏参数非单一表达式');
    return this.expr(exprs[0]!);
  }

  /** 宏实参重建源码（保留定界符）：优先用 parser.treeText 原文 */
  private macroArgSrc(toks: FlatTok[]): string {
    return toks.map((t) => t.text).join(' ');
  }

  private formatString(fmt: string, args: string[]): string {
    const L = this.lang.id;
    const parts: { lit?: string; arg?: string; prec?: number }[] = [];
    let ai = 0;
    let i = 0;
    let literal = '';
    while (i < fmt.length) {
      const c = fmt[i]!;
      if (c === '{' && fmt[i + 1] === '{') { literal += '{'; i += 2; continue; }
      if (c === '}' && fmt[i + 1] === '}') { literal += '}'; i += 2; continue; }
      if (c === '{') {
        const end = fmt.indexOf('}', i);
        if (end < 0) throw new TranspileError('format 缺 }');
        const spec = fmt.slice(i + 1, end);
        i = end + 1;
        if (literal) { parts.push({ lit: literal }); literal = ''; }
        // 形态解析（与 values.ts hslFormat 同步）：<idx|空>[:<flags>]
        // v0.2.51：实现 .N 精度子集 —— 此前精度说明符被静默丢弃，
        // 生成端输出无精度的插值（错误但不报错），违反宁缺毋滥纪律
        const colon = spec.indexOf(':');
        const namePart = colon >= 0 ? spec.slice(0, colon) : spec;
        const flags = colon >= 0 ? spec.slice(colon + 1) : '';
        let idx = ai;
        if (/^\d+$/.test(namePart)) idx = parseInt(namePart, 10);
        else ai++;
        const isDebug = flags === '?';
        const precMatch = /^\.(\d+)$/.exec(flags);
        const prec = !isDebug && precMatch ? parseInt(precMatch[1]!, 10) : undefined;
        parts.push({ arg: isDebug ? this.debugWrap(args[idx] ?? 'null') : args[idx] ?? 'null', prec });
        continue;
      }
      literal += c;
      i++;
    }
    if (literal) parts.push({ lit: literal });

    const escLit = (s: string): string => {
      if (L === 'python') return s.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n').replace(/\{/g, '{{').replace(/\}/g, '}}');
      if (L === 'typescript' || L === 'javascript') return s.replace(/\\/g, '\\\\').replace(/`/g, '\\`').replace(/\$/g, '\\$').replace(/\n/g, '\\n');
      if (L === 'rust') return s.replace(/"/g, '\\"').replace(/\n/g, '\\n').replace(/\{/g, '{{').replace(/\}/g, '}}');
      if (L === 'go') return s.replace(/%/g, '%%').replace(/"/g, '\\"').replace(/\n/g, '\\n');
      return s.replace(/"/g, '\\"').replace(/\n/g, '\\n').replace(/\{/g, '{{').replace(/\}/g, '}}');
    };
    const argList = parts.filter((p) => p.arg !== undefined).map((p) => p.arg);

    if (L === 'python') {
      let out = '';
      for (const p of parts) {
        if (p.lit !== undefined) { out += escLit(p.lit); continue; }
        out += p.prec !== undefined ? `{(${p.arg}):.${p.prec}f}` : `{${p.arg}}`;
      }
      return `f"${out}"`;
    }
    if (L === 'typescript' || L === 'javascript') {
      let out = '';
      for (const p of parts) {
        if (p.lit !== undefined) { out += escLit(p.lit); continue; }
        out += p.prec !== undefined ? `\${(${p.arg}).toFixed(${p.prec})}` : `\${${p.arg}}`;
      }
      return `\`${out}\``;
    }
    if (L === 'rust') {
      // v1.4.10 修复（真机 rustc 实测）：format! 占位符必须区分两种合法形态 ——
      //  · 纯标识符（非关键字）→ 内联捕获 {name} / {name:?}（且不重复传参）
      //  · 表达式（方法调用/字段链等）→ 位置 {} / {:?} + 参数列表
      // 此前把表达式直接嵌入 {…}（如 {args.len()}）且同时传位置参数 ——
      // rustc 报 invalid format string（内联捕获不支持方法调用）
      let out = '';
      const posArgs: string[] = [];
      for (const p of parts) {
        if (p.lit !== undefined) { out += escLit(p.lit); continue; }
        let a = p.arg!;
        let dbg = false;
        if (a.endsWith(':?')) { dbg = true; a = a.slice(0, -2); }
        const precSuffix = p.prec !== undefined && !dbg ? `:.${p.prec}` : '';
        const plain = /^[A-Za-z_][A-Za-z0-9_]*$/.test(a) && !RS_KW.has(a);
        if (plain) {
          out += `{${a}${dbg ? ':?' : precSuffix}}`;
        } else {
          out += dbg ? `{:?}` : (p.prec !== undefined ? `{:.${p.prec}}` : `{}`);
          posArgs.push(a);
        }
      }
      return `format!("${out}"${posArgs.length ? ', ' + posArgs.join(', ') : ''})`;
    }
    if (L === 'go') {
      let out = '';
      for (const p of parts) {
        if (p.lit !== undefined) { out += escLit(p.lit); continue; }
        out += p.prec !== undefined ? `%.${p.prec}f` : '%v';
      }
      return `fmt.Sprintf("${out}"${argList.length ? ', ' + argList.join(', ') : ''})`;
    }
    // cpp
    let out = '';
    for (const p of parts) {
      if (p.lit !== undefined) { out += escLit(p.lit); continue; }
      out += p.prec !== undefined ? `{:.${p.prec}}` : '{}';
    }
    return `std::format("${out}"${argList.length ? ', ' + argList.join(', ') : ''})`;
  }

  private debugWrap(v: string): string {
    const L = this.lang.id;
    if (L === 'python') return `${v}!r`;
    if (L === 'typescript' || L === 'javascript') return `JSON.stringify(${v})`;
    if (L === 'rust') return `${v}:?`;
    return v;
  }

  // ---------- 语句 ----------
  stmt(st: A.Stmt, indent: string, out: string[]): void {
    if (st.kind !== 'expr') { this.stmtEx(st, indent, out); return; }
    const e = st.expr;
    if (e.kind === 'unit') return;
    if (e.kind === 'lit' && e.lit.t === 'str') return; // 文档字符串
    out.push(this.exprStmtLine(indent, this.expr(e)));
  }

  stmtEx(st: A.Stmt, indent: string, out: string[]): void {
    const L = this.lang.id;
    if (st.kind === 'let') {
      if (st.elseBlock) throw new TranspileError('let-else');
      const name = st.pat.kind === 'binding' ? st.pat.name : null;
      if (!name) throw new TranspileError('let 复杂模式');
      const safe = ident(name, L);
      this.env.set(name, st.ty ? this.tyKind(st.ty) : st.init ? this.exprKind(st.init) : 'unknown');
      // 🔴 v1.4.7：块表达式初始化器（let x = if/match/if-let ...）—— 此前全部 7 语言回退 contract
      // （interp 支持；生成端 expr() 遇块直接 throw）。现走「声明 + 分支尾赋值」模式：
      // 先声明变量，再发射控制流，每个分支尾赋值（与 ifLetTail/matchTail 值语义同源）
      if (st.init && (st.init.kind === 'if' || st.init.kind === 'iflet' || st.init.kind === 'match')) {
        this.letBlockInit(st, safe, indent, out);
        return;
      }
      if (st.init) out.push(this.varDeclLine(indent, safe, st.mut, this.expr(st.init), st.ty));
      else out.push(this.varNoInitLine(indent, safe, st.mut));
      return;
    }
    if (st.kind === 'item') throw new TranspileError('体内 item');
    if (st.kind === 'empty') return;
    const e = st.expr;
    switch (e.kind) {
      case 'if': this.ifChain(e, indent, out); return;
      case 'iflet': this.ifLet(e, indent, out); return;
      case 'while': {
        out.push(this.whileLine(indent, this.expr(e.cond)));
        this.blockInto(e.body, indent + '    ', out);
        out.push(this.closeLine(indent));
        return;
      }
      case 'whilelet': {
        // 简写归一化：Some(x) → Option::Some(x) 等
        const pat = normalizePattern(e.pat);
        // Rust：原生 while let 语法
        if (L === 'rust') {
          out.push(`${indent}while let ${this.rustPattern(pat)} = ${this.expr(e.expr)} {`);
          this.blockInto(e.body, indent + '    ', out);
          out.push(`${indent}}`);
          return;
        }
        // cpp/go：合成 loop + 每迭代求值一次 + break（与 Rust 每迭代求值语义同源）
        if (L === 'cpp' || L === 'go') {
          const scrutExpr = this.expr(e.expr);
          out.push(this.loopLine(indent)); // cpp: while (true) { / go: for {
          const inner = indent + '    ';
          let scrut: string = scrutExpr;
          if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(scrutExpr)) {
            scrut = this.freshTmp('wl');
            out.push(L === 'cpp' ? `${inner}auto ${scrut} = ${scrutExpr};` : `${inner}${scrut} := ${scrutExpr}`);
          }
          const info = this.armInfo(pat, scrut);
          if (!info) throw new TranspileError('while-let 复杂模式');
          if (L === 'cpp') {
            let cond: string;
            if (info.variant) cond = `std::holds_alternative<${info.variant}>(${scrut})`;
            else if (info.cond === null) cond = 'true';
            else cond = info.cond;
            out.push(`${inner}if (!(${cond})) {`);
            out.push(`${inner}    break;`);
            out.push(`${inner}}`);
            if (info.variant) {
              if (info.binds.length > 0) out.push(`${inner}auto& _v = std::get<${info.variant}>(${scrut});`);
              for (const bd of info.binds) out.push(`${inner}auto ${bd.replace('=', ' = ').replace(`${scrut}.`, '_v.')};`);
            } else {
              this.emitBinds(info.binds, inner, out, 'auto');
            }
          } else {
            if (info.variant) {
              const fresh = this.freshTmp('ifv');
              const okv = this.freshTmp('ok');
              const recv = info.binds.length > 0 ? fresh : '_';
              out.push(`${inner}${recv}, ${okv} := ${scrut}.(${info.variant})`);
              out.push(`${inner}if !${okv} {`);
              out.push(`${inner}    break`);
              out.push(`${inner}}`);
              if (info.binds.length > 0) {
                for (const bd of info.binds) out.push(`${inner}${bd.replace('=', ' := ').replace(`${scrut}.`, `${fresh}.`)}`);
              }
            } else {
              let cond: string;
              if (info.cond === null) cond = 'true';
              else cond = info.cond;
              out.push(`${inner}if !(${cond}) {`);
              out.push(`${inner}    break`);
              out.push(`${inner}}`);
              this.emitBinds(info.binds, inner, out, '');
            }
          }
          this.blockInto(e.body, inner, out);
          out.push(this.closeLine(indent));
          return;
        }
        // 子集语言（python/ts/js）：合成 while + break 模式
        if (L !== 'python' && L !== 'typescript' && L !== 'javascript') {
          throw new TranspileError('while-let 此语言未支持');
        }
        const scrutExpr = this.expr(e.expr);
        if (L === 'python') {
          out.push(`${indent}while True:`);
          // 每次迭代先求值一次到 _scrut_n，再做 cond 检查 + 绑定
          let scrut: string = scrutExpr;
          if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(scrutExpr)) {
            scrut = this.freshTmp('wl');
            out.push(`${indent}    ${scrut} = ${scrutExpr}`);
          }
          const info = this.armInfo(pat, scrut);
          if (!info) throw new TranspileError('while-let 复杂模式');
          let cond: string;
          if (info.variant) cond = `isinstance(${scrut}, ${info.variant})`;
          else if (info.cond === null) cond = 'True';
          else cond = info.cond;
          out.push(`${indent}    if not (${cond}):`);
          out.push(`${indent}        break`);
          this.emitBinds(info.binds, indent + '    ', out, '');
          this.blockInto(e.body, indent + '    ', out);
        } else {
          out.push(`${indent}while (true) {`);
          let scrut: string = scrutExpr;
          if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(scrutExpr)) {
            scrut = this.freshTmp('wl');
            out.push(`${indent}  const ${scrut} = ${scrutExpr};`);
          }
          const info = this.armInfo(pat, scrut);
          if (!info) throw new TranspileError('while-let 复杂模式');
          let cond: string;
          if (info.variant) cond = `${scrut}?.kind === '${info.variant}'`;
          else if (info.cond === null) cond = 'true';
          else cond = info.cond;
          out.push(`${indent}  if (!(${cond})) break;`);
          this.emitBinds(info.binds, indent + '  ', out, 'const');
          this.blockInto(e.body, indent + '  ', out);
          out.push(`${indent}}`);
        }
        return;
      }
      case 'loop': {
        out.push(this.loopLine(indent));
        this.blockInto(e.body, indent + '    ', out);
        out.push(this.closeLine(indent));
        return;
      }
      case 'for': {
        const name = patternName(e.pat);
        if (!name || e.pat.kind !== 'binding') throw new TranspileError('for 复杂模式');
        const safe = ident(name, L);
        // v0.2.28: 检测 e.iter 为 range 表达式（parseExprNoStruct 会消费 a..b 为 range expr，
        // 导致 e.range 始终 undefined）
        const rng = e.range ?? (e.iter?.kind === 'range' ? e.iter : undefined);
        if (rng) {
          const rlo = rng.lo ? this.expr(rng.lo) : undefined;
          const rhi = rng.hi ? this.expr(rng.hi) : undefined;
          out.push(this.forRangeLines(indent, safe, rlo, rhi, rng.inclusive));
        } else {
          out.push(this.forInLines(indent, safe, this.expr(e.iter!)));
        }
        this.blockInto(e.body, indent + '    ', out);
        out.push(this.closeLine(indent));
        return;
      }
      case 'match': {
        this.matchDispatch(this.expr(e.expr), e.arms, indent, out, false);
        return;
      }
      case 'break': {
        if (e.value) throw new TranspileError('break 带值');
        out.push(L === 'python' ? `${indent}break` : L === 'go' ? `${indent}break` : `${indent}break;`);
        return;
      }
      case 'continue': {
        out.push(L === 'python' || L === 'go' ? `${indent}continue` : `${indent}continue;`);
        return;
      }
      case 'return': {
        if (!e.value || e.value.kind === 'unit') { out.push(this.returnLine(indent, null)); return; }
        out.push(this.returnLine(indent, this.expr(e.value)));
        return;
      }
      default: {
        out.push(this.exprStmtLine(indent, this.expr(e)));
        return;
      }
    }
  }

  /** 块内语句 + 值语义尾（match/if 尾表达式 → return） */
  private blockIntoValue(body: A.Expr, indent: string, out: string[], asValue: boolean | string): void {
    if (body.kind !== 'block') {
      out.push(this.valueTailLine(indent, this.expr(body), asValue));
      return;
    }
    const stmts = body.stmts;
    if (stmts.length === 0) { out.push(this.passLine(indent)); return; }
    const last = stmts[stmts.length - 1]!;
    for (const st of stmts.slice(0, -1)) this.stmtEx(st, indent, out);
    if (last.kind === 'expr' && !last.hasSemi) {
      const ek = last.expr.kind;
      // 分支结构尾（if / if-let / match）：值模式递归产出 return/赋值，语句模式整体作语句
      if (ek === 'if') { this.ifChain(last.expr, indent, out, asValue); return; }
      if (ek === 'iflet') { this.ifLet(last.expr, indent, out, asValue); return; }
      if (ek === 'match') {
        const m = last.expr as A.Expr & { kind: 'match' };
        this.matchDispatch(this.expr(m.expr), m.arms, indent, out, asValue);
        return;
      }
      if (!['loop', 'while', 'whilelet', 'for', 'block'].includes(ek)) {
        out.push(this.valueTailLine(indent, this.expr(last.expr), asValue));
        return;
      }
    }
    this.stmtEx(last, indent, out);
  }

  private blockInto(body: A.Expr, indent: string, out: string[]): void {
    this.blockIntoValue(body, indent, out, false);
  }

  ifChain(e: A.Expr & { kind: 'if' }, indent: string, out: string[], asValue: boolean | string = false): void {
    const L = this.lang.id;
    out.push(this.ifLine(indent, this.expr(e.cond)));
    this.blockIntoValue(e.then, indent + '    ', out, asValue);
    if (e.els) {
      // else if / else if let 链：递归后改写首行（python: elif；brace 语言：} else if）
      if (e.els.kind === 'if' || e.els.kind === 'iflet') {
        const sub: string[] = [];
        if (e.els.kind === 'if') this.ifChain(e.els, indent, sub, asValue);
        else this.ifLet(e.els, indent, sub, asValue);
        if (sub.length > 0 && sub[0]!.startsWith(`${indent}if `)) {
          sub[0] = L === 'python'
            ? `${indent}elif` + sub[0]!.slice(indent.length + 2)
            : `${indent}} else ` + sub[0]!.slice(indent.length);
        }
        out.push(...sub);
        return;
      }
      out.push(this.elseLine(indent));
      this.blockIntoValue(e.els, indent + '    ', out, asValue);
    }
    out.push(this.closeLine(indent));
  }

  private ifLet(e: A.Expr & { kind: 'iflet' }, indent: string, out: string[], asValue: boolean | string = false): void {
    const L = this.lang.id;
    // 值/赋值语义要求两分支齐全（Rust 语义：无 else 的 if let 表达式类型为 ()）
    if (asValue !== false && !e.els) throw new TranspileError('if-let 值语义需 else 分支');
    // 简写归一化：Some(x) → Option::Some(x) 等，便于下游 armInfo / rustPattern 一致处理
    const pat = normalizePattern(e.pat);
    // Rust：原生 if let 语法
    if (L === 'rust') {
      out.push(`${indent}if let ${this.rustPattern(pat)} = ${this.expr(e.expr)} {`);
      this.blockIntoValue(e.then, indent + '    ', out, asValue);
      if (e.els) {
        if (e.els.kind === 'iflet' || e.els.kind === 'if') {
          // } else if let / } else if 链
          const sub: string[] = [];
          if (e.els.kind === 'iflet') this.ifLet(e.els, indent, sub, asValue);
          else this.ifChain(e.els, indent, sub, asValue);
          if (sub.length > 0 && sub[0]!.startsWith(`${indent}if `)) {
            sub[0] = `${indent}} else ` + sub[0]!.slice(indent.length);
          }
          out.push(...sub);
          return;
        }
        out.push(`${indent}} else {`);
        this.blockIntoValue(e.els, indent + '    ', out, asValue);
      }
      out.push(`${indent}}`);
      return;
    }
    // cpp/go：与 matchDispatch 相同的变体/Option 判定，合成 if 链
    if (L === 'cpp' || L === 'go') {
      const scrutExpr = this.expr(e.expr);
      const { tmp: scrut, prelude } = this.hoistScrut(scrutExpr, indent, out);
      for (const ln of prelude) out.push(ln);
      const info = this.armInfo(pat, scrut);
      if (!info) throw new TranspileError('if-let 复杂模式');
      const inner = indent + '    ';
      if (L === 'cpp') {
        let cond: string;
        if (info.variant) cond = `std::holds_alternative<${info.variant}>(${scrut})`;
        else if (info.cond === null) cond = 'true';
        else cond = info.cond;
        out.push(`${indent}if (${cond}) {`);
        if (info.variant) {
          // 变体解包：get + 字段绑定（与 matchDispatch cpp 路径同构）
          if (info.binds.length > 0) out.push(`${inner}auto& _v = std::get<${info.variant}>(${scrut});`);
          for (const bd of info.binds) out.push(`${inner}auto ${bd.replace('=', ' = ').replace(`${scrut}.`, '_v.')};`);
        } else {
          this.emitBinds(info.binds, inner, out, 'auto');
        }
        this.blockIntoValue(e.then, inner, out, asValue);
        if (e.els) {
          if (e.els.kind === 'iflet' || e.els.kind === 'if') {
            const sub: string[] = [];
            if (e.els.kind === 'iflet') this.ifLet(e.els, indent, sub, asValue);
            else this.ifChain(e.els, indent, sub, asValue);
            if (sub.length > 0 && sub[0]!.startsWith(`${indent}if `)) {
              sub[0] = `${indent}} else ` + sub[0]!.slice(indent.length);
            }
            out.push(...sub);
            return;
          }
          out.push(`${indent}} else {`);
          this.blockIntoValue(e.els, inner, out, asValue);
        }
        out.push(`${indent}}`);
        return;
      }
      // go：变体用类型断言 init-statement；Option/字面量/绑定用普通条件
      if (info.variant) {
        const fresh = this.freshTmp('ifv');
        const okv = this.freshTmp('ok');
        const recv = info.binds.length > 0 ? fresh : '_';
        out.push(`${indent}if ${recv}, ${okv} := ${scrut}.(${info.variant}); ${okv} {`);
        if (info.binds.length > 0) {
          for (const bd of info.binds) out.push(`${inner}${bd.replace('=', ' := ').replace(`${scrut}.`, `${fresh}.`)}`);
        }
      } else {
        let cond: string;
        if (info.cond === null) cond = 'true';
        else cond = info.cond;
        out.push(`${indent}if ${cond} {`);
        this.emitBinds(info.binds, inner, out, '');
      }
      this.blockIntoValue(e.then, inner, out, asValue);
      if (e.els) {
        if (e.els.kind === 'iflet' || e.els.kind === 'if') {
          const sub: string[] = [];
          if (e.els.kind === 'iflet') this.ifLet(e.els, indent, sub, asValue);
          else this.ifChain(e.els, indent, sub, asValue);
          if (sub.length > 0 && sub[0]!.startsWith(`${indent}if `)) {
            sub[0] = `${indent}} else ` + sub[0]!.slice(indent.length);
          }
          out.push(...sub);
          return;
        }
        out.push(`${indent}} else {`);
        this.blockIntoValue(e.els, inner, out, asValue);
      }
      out.push(`${indent}}`);
      return;
    }
    // 子集语言（python/ts/js）：通过 armInfo 复用 match 的模式识别
    if (L !== 'python' && L !== 'typescript' && L !== 'javascript') {
      throw new TranspileError('if-let 此语言未支持');
    }
    const scrutExpr = this.expr(e.expr);
    // 把副作用表达式 hoist 到临时变量，避免在条件 + 绑定中多次求值
    const { tmp: scrut, prelude } = this.hoistScrut(scrutExpr, indent, out);
    for (const ln of prelude) out.push(ln);
    const info = this.armInfo(pat, scrut);
    if (!info) throw new TranspileError('if-let 复杂模式');
    let cond: string;
    if (info.variant) {
      // 枚举变体模式：python isinstance / ts-js .kind 比较
      if (L === 'python') cond = `isinstance(${scrut}, ${info.variant})`;
      else cond = `${scrut}?.kind === '${info.variant}'`;
    } else if (info.cond === null) {
      // 绑定 / 通配：恒真
      cond = L === 'python' ? 'True' : 'true';
    } else {
      cond = info.cond;
    }
    if (L === 'python') {
      out.push(`${indent}if ${cond}:`);
    } else {
      out.push(`${indent}if (${cond}) {`);
    }
    // 绑定：在 then 块顶部赋值（Option::Some 已被 armInfo 转为 cond=null + binds）
    // cpp 风格的 _v 临时变量已在 match 中处理；if-let 这里走通用 emitBinds
    const bindIndent = L === 'python' ? indent + '    ' : indent + '  ';
    this.emitBinds(info.binds, bindIndent, out, L === 'python' ? '' : 'const');
    this.blockIntoValue(e.then, bindIndent, out, asValue);
    if (e.els) {
      if (e.els.kind === 'iflet' || e.els.kind === 'if') {
        // else if let / else if 链：递归改写首行（python: elif；brace 语言：} else if）
        const sub: string[] = [];
        if (e.els.kind === 'iflet') this.ifLet(e.els, indent, sub, asValue);
        else this.ifChain(e.els, indent, sub, asValue);
        if (sub.length > 0 && sub[0]!.startsWith(`${indent}if `)) {
          sub[0] = L === 'python'
            ? `${indent}elif` + sub[0]!.slice(indent.length + 2)
            : `${indent}} else ` + sub[0]!.slice(indent.length);
        }
        out.push(...sub);
        return;
      }
      if (L === 'python') {
        out.push(`${indent}else:`);
        this.blockIntoValue(e.els, indent + '    ', out, asValue);
      } else {
        out.push(`${indent}} else {`);
        this.blockIntoValue(e.els, indent + '  ', out, asValue);
      }
    }
    if (L !== 'python') out.push(`${indent}}`);
  }

  // ---------- match ----------
  /** 尾位置 match（值语义）：分支产出 return */
  matchTail(e: A.Expr & { kind: 'match' }, indent: string, out: string[]): void {
    this.matchDispatch(this.expr(e.expr), e.arms, indent, out, true);
  }

  /** 尾位置 if-let（值语义）：分支产出 return（无 else 则回退 contract） */
  ifLetTail(e: A.Expr & { kind: 'iflet' }, indent: string, out: string[]): void {
    this.ifLet(e, indent, out, true);
  }

  /** 供 transpileBody 使用的公共面 */
  exprPublic(e: A.Expr): string { return this.expr(e); }
  returnLinePublic(indent: string, v: string | null): string { return this.returnLine(indent, v); }

  private armInfo(pattern: A.Pattern, scrut: string): { cond: string | null; variant?: string; binds: string[] } | null {
    const L = this.lang.id;
    if (pattern.kind === 'wildcard') return { cond: null, binds: [] };
    if (pattern.kind === 'literal') {
      const lit = pattern.value;
      let text: string;
      if (typeof lit === 'string') text = this.ctx.strLit(lit);
      else if (typeof lit === 'boolean') text = L === 'python' ? (lit ? 'True' : 'False') : String(lit);
      else text = String(lit);
      return { cond: `${scrut} == ${text}`, binds: [] };
    }
    if (pattern.kind === 'binding') {
      if (pattern.sub) return null;
      return { cond: null, binds: [`${ident(pattern.name, L)}=${scrut}`] };
    }
    // 枚举变体结构模式：Enum::Variant { fields } 解析为 kind:'struct' + segs
    if (pattern.kind === 'struct' && pattern.segs.length >= 2) {
      const head = pattern.segs[0]!;
      const variant = pattern.segs[pattern.segs.length - 1]!;
      const en = this.ctx.enums.get(head);
      if (en && head !== 'Option' && head !== 'Result') {
        const binds: string[] = [];
        for (const f of pattern.fields) {
          const fn = patternName(f.pat);
          // go 变体结构体字段名导出大写（decls capitalize 同源）
          if (fn && fn !== '_') binds.push(`${ident(fn, L)}=${scrut}.${L === 'go' ? capitalize(f.name) : f.name}`);
        }
        return { cond: `@variant:${variant}`, variant, binds };
      }
      return null;
    }
    if (pattern.kind === 'path') {
      const head = pattern.segs[0]!;
      const variant = pattern.segs[pattern.segs.length - 1]!;
      const en = this.ctx.enums.get(head);
      if (en && head !== 'Option' && head !== 'Result') {
        const binds: string[] = [];
        if (pattern.sub?.kind === 'struct') {
          for (const f of pattern.sub.fields) {
            const fn = patternName(f.pat);
            if (fn && fn !== '_') binds.push(`${ident(fn, L)}=${scrut}.${L === 'go' ? capitalize(f.name) : f.name}`);
          }
        }
        if (pattern.sub?.kind === 'tuple') {
          pattern.sub.items.forEach((p, i) => {
            const fn = patternName(p);
            if (fn && fn !== '_') binds.push(`${ident(fn, L)}=${scrut}.${L === 'go' ? `F${i}` : `f${i}`}`);
          });
        }
        return { cond: `@variant:${variant}`, variant, binds };
      }
      if (head === 'Option' && variant === 'Some' && pattern.sub?.kind === 'tuple') {
        const fn = patternName(pattern.sub.items[0]!);
        // cpp：std::optional → has_value() + *deref；go：*T 指针 → != nil + *deref
        if (L === 'cpp') return { cond: `${scrut}.has_value()`, binds: fn && fn !== '_' ? [`${ident(fn, L)}=*${scrut}`] : [] };
        if (L === 'go') return { cond: `${scrut} != nil`, binds: fn && fn !== '_' ? [`${ident(fn, L)}=*${scrut}`] : [] };
        return {
          cond: L === 'python' ? `${scrut} is not None` : `${scrut} != null`,
          binds: fn && fn !== '_' ? [`${ident(fn, L)}=${scrut}`] : [],
        };
      }
      if (head === 'Option' && variant === 'None') {
        if (L === 'cpp') return { cond: `!(${scrut}.has_value())`, binds: [] };
        if (L === 'go') return { cond: `${scrut} == nil`, binds: [] };
        return { cond: L === 'python' ? `${scrut} is None` : `${scrut} == null`, binds: [] };
      }
      // Result::Ok(x) / Result::Err(e) —— 运行期 Ok/Err 是带 f0 字段的变体类
      // （tuple 变体 → f0 命名域）；匹配模式与用户自定义 tuple 变体同构。
      // cpp/go 例外：类型映射无变体通道（cpp Result→%T 裸值 / go→(T, error) 多返回值），
      // 诚实回退 contract（宁缺毋滥纪律）
      if (head === 'Result' && (variant === 'Ok' || variant === 'Err')) {
        if (L === 'cpp' || L === 'go') return null;
        if (pattern.sub?.kind === 'tuple') {
          const binds: string[] = [];
          pattern.sub.items.forEach((p, i) => {
            const fn = patternName(p);
            if (fn && fn !== '_') binds.push(`${ident(fn, L)}=${scrut}.f${i}`);
          });
          return { cond: `@variant:${variant}`, variant, binds };
        }
        // 无负载变体：Result::Ok / Result::Err 单元形式（罕见）
        return { cond: `@variant:${variant}`, variant, binds: [] };
      }
      return null;
    }
    return null;
  }

  private matchDispatch(scrut: string, arms: A.MatchArm[], indent: string, out: string[], asValue: boolean | string): void {
    const L = this.lang.id;
    // 🔴 副作用 scrutinee hoist：if/elif 路径（python/cpp）每臂 cond 与 binds 都引用 scrut，
    //   未 hoist 时 `match v.pop() { Some(x) => ..., None => ... }` 会多次求值 pop（破坏语义）。
    //   rust/ts/js/go 路径仅在 match/switch 头求值一次（原生语义保证），无需 hoist。
    //   与 while-let hoistScrut 同源（Task 17 引入；match 路径本轮补齐）。
    if (L === 'python' || L === 'cpp') {
      if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(scrut)) {
        const tmp = this.freshTmp('m');
        out.push(L === 'python' ? `${indent}${tmp} = ${scrut}` : `${indent}auto ${tmp} = ${scrut};`);
        scrut = tmp;
      }
    }
    // 简写归一化：arm 内的 Some(x) / Ok(x) 等单段简写同步归一
    const infos = arms.map((arm) => ({ arm, info: this.armInfo(normalizePattern(arm.pattern), scrut) }));
    if (infos.some((x) => !x.info)) throw new TranspileError('match 模式不在子集');
    const isEnum = infos.some((x) => x.info!.variant !== undefined);
    const isOpt = !isEnum && infos.some((x) => x.info!.cond?.includes('null') || x.info!.cond?.includes('None'));

    if (L === 'rust') {
      out.push(`${indent}match ${scrut} {`);
      for (const { arm, info } of infos) {
        const patText = this.rustPattern(normalizePattern(arm.pattern));
        const guard = arm.guard ? ` if ${this.expr(arm.guard)}` : '';
        if (asValue !== false && arm.body.kind !== 'block') {
          // rust：赋值模式下臂体为 name = expr（赋值是合法 rust 表达式）
          if (typeof asValue === 'string') {
            out.push(`${indent}    ${patText}${guard} => ${asValue} = ${this.expr(arm.body)},`);
          } else {
            out.push(`${indent}    ${patText}${guard} => ${this.expr(arm.body)},`);
          }
        } else {
          out.push(`${indent}    ${patText}${guard} => {`);
          this.blockIntoValue(arm.body, indent + '        ', out, asValue);
          out.push(`${indent}    }`);
        }
      }
      out.push(`${indent}}`);
      return;
    }

    if (isEnum && (L === 'typescript' || L === 'javascript')) {
      out.push(`${indent}switch (${scrut}.kind) {`);
      for (const { arm, info } of infos) {
        const v = info!.variant;
        if (v) {
          if (arm.guard) throw new TranspileError('case guard');
          out.push(`${indent}  case '${v}': {`);
          this.emitBinds(info!.binds, indent + '    ', out, 'const');
          this.blockIntoValue(arm.body, indent + '    ', out, asValue);
          out.push(`${indent}    break;`);
          out.push(`${indent}  }`);
        } else {
          out.push(`${indent}  default: {`);
          this.emitBinds(info!.binds, indent + '    ', out, 'const');
          this.blockIntoValue(arm.body, indent + '    ', out, asValue);
          out.push(`${indent}  }`);
        }
      }
      out.push(`${indent}}`);
      return;
    }

    if (isEnum && L === 'go') {
      out.push(`${indent}switch _v := (${scrut}).(type) {`);
      for (const { arm, info } of infos) {
        const v = info!.variant;
        if (v) {
          out.push(`${indent}case ${v}:`);
          for (const bd of info!.binds) out.push(`${indent}\t${bd.replace('=', ' := ').replace(`${scrut}.`, '_v.')}`);
          this.blockIntoValue(arm.body, indent + '\t', out, asValue);
        } else {
          out.push(`${indent}default:`);
          this.emitBinds(info!.binds, indent + '\t', out, 'var');
          this.blockIntoValue(arm.body, indent + '\t', out, asValue);
        }
      }
      out.push(`${indent}}`);
      return;
    }

    // if/elif 链（python / 字面量 match / Option match / cpp 变体）
    // 大括号语言：每臂 opener 自带 '}' 前缀，整链只在末尾 close 一次
    let first = true;
    for (const { arm, info } of infos) {
      let cond = info!.cond;
      const guard = arm.guard
        ? (L === 'python' ? ` and (${this.expr(arm.guard)})` : ` && (${this.expr(arm.guard)})`)
        : '';
      if (cond?.startsWith('@variant:')) {
        const variant = info!.variant!;
        if (L === 'python') cond = `isinstance(${scrut}, ${variant})`;
        else if (L === 'cpp') cond = `std::holds_alternative<${variant}>(${scrut})`;
        else if (L === 'typescript' || L === 'javascript') cond = `${scrut}.kind === '${variant}'`;
        else if (L === 'go') cond = `true`; // 不应到达（上面处理）
      }
      if (cond === null) {
        if (!first) {
          if (L === 'python') out.push(`${indent}else:`);
          else out.push(`${indent}} else {`);
          this.emitBinds(info!.binds, indent + '    ', out, L === 'python' ? '' : 'const');
          this.armBlock(arm, info!, indent, out, asValue, scrut);
        } else {
          // 首臂即通配：直接放语句块（无分支结构）
          this.emitBinds(info!.binds, indent, out, L === 'python' ? '' : 'const');
          this.armBlock(arm, info!, indent, out, asValue, scrut);
        }
        first = false;
        continue;
      }
      const kw = first ? 'if' : (L === 'python' ? 'elif' : '} else if');
      if (L === 'python') out.push(`${indent}${kw} ${cond}${guard}:`);
      else out.push(`${indent}${kw} (${cond}${guard}) {`);
      const bindIndent = indent + '    ';
      if (L === 'cpp' && info!.variant) {
        out.push(`${bindIndent}auto& _v = std::get<${info!.variant}>(${scrut});`);
        for (const bd of info!.binds) out.push(`${bindIndent}auto ${bd.replace('=', ' = ').replace(`${scrut}.`, '_v.')};`);
      } else {
        this.emitBinds(info!.binds, bindIndent, out, L === 'python' ? '' : 'const');
      }
      this.armBlock(arm, info!, indent, out, asValue, scrut);
      first = false;
    }
    const hasWildcard = infos.some((x) => x.info!.cond === null);
    if (!hasWildcard) {
      if (L === 'python') {
        out.push(`${indent}else:`);
        out.push(`${indent}    raise ValueError('dhv: match 不可达分支（S-6 穷尽性）')`);
      } else {
        out.push(`${indent}} else {`);
        out.push(L === 'go'
          ? `${indent}\tpanic('dhv: match 不可达分支（S-6）')`
          : L === 'cpp'
            ? `${indent}    throw std::runtime_error("dhv: match 不可达分支（S-6）");`
            : `${indent}    throw new Error('dhv: match 不可达分支（S-6）');`);
      }
    }
    if (L !== 'python') out.push(this.closeLine(indent));
  }

  private armBlock(arm: A.MatchArm, info: { binds: string[] }, indent: string, out: string[], asValue: boolean | string, scrut: string): void {
    void info; void scrut;
    const bodyIndent = indent + (this.lang.id === 'python' ? '    ' : '    ');
    const bk = arm.body.kind;
    // 分支结构作为臂体（if / if-let / match）：值模式产出 return/赋值，语句模式整体作语句
    if (bk === 'if') { this.ifChain(arm.body, bodyIndent, out, asValue); return; }
    if (bk === 'iflet') { this.ifLet(arm.body, bodyIndent, out, asValue); return; }
    if (bk === 'match') {
      this.matchDispatch(this.expr(arm.body.expr), arm.body.arms, bodyIndent, out, asValue);
      return;
    }
    if (arm.body.kind === 'block') {
      this.blockIntoValue(arm.body, bodyIndent, out, asValue);
    } else {
      out.push(this.valueTailLine(bodyIndent, this.expr(arm.body), asValue));
    }
  }

  private emitBinds(binds: string[], indent: string, out: string[], kw: string): void {
    const L = this.lang.id;
    for (const bd of binds) {
      if (L === 'python') out.push(`${indent}${bd.replace('=', ' = ')}`);
      else if (L === 'go') out.push(`${indent}${bd.replace('=', ' := ')}`);
      else if (L === 'cpp') out.push(`${indent}auto ${bd.replace('=', ' = ')};`);
      else if (kw) out.push(`${indent}${kw} ${bd.replace('=', ' = ')};`);
      else out.push(`${indent}${bd.replace('=', ' = ')}`);
    }
  }

  private rustPattern(p: A.Pattern): string {
    const nm = (pp: A.Pattern): string => (patternName(pp) ?? '_');
    switch (p.kind) {
      case 'wildcard': return '_';
      case 'literal': {
        if (typeof p.value === 'string') return this.ctx.strLit(p.value);
        return String(p.value);
      }
      case 'binding': return nm(p);
      case 'struct': {
        // Enum::Variant { fields }（多段）→ 原样；单段 struct 模式 → 绑定字段
        if (p.segs.length >= 2) {
          return `${p.segs.join('::')} { ${p.fields.map((f) => `${f.name}: ${nm(f.pat)}`).join(', ')} }`;
        }
        return `${p.segs.join('::')} { ${p.fields.map((f) => `${f.name}: ${nm(f.pat)}`).join(', ')} }`;
      }
      case 'path': {
        const head = p.segs[0]!;
        const variant = p.segs[p.segs.length - 1]!;
        const en = this.ctx.enums.get(head);
        if (en && head !== 'Option') {
          if (p.sub?.kind === 'struct') return `${head}::${variant} { ${p.sub.fields.map((f) => `${f.name}: ${nm(f.pat)}`).join(', ')} }`;
          if (p.sub?.kind === 'tuple') return `${head}::${variant}(${p.sub.items.map((x) => nm(x)).join(', ')})`;
          return `${head}::${variant}`;
        }
        if (head === 'Option' && variant === 'Some' && p.sub?.kind === 'tuple') return `Some(${nm(p.sub.items[0]!)})`;
        return variant;
      }
      default:
        throw new TranspileError('rust match 模式');
    }
  }

  // ---------- 行打印 ----------
  private exprStmtLine(i: string, e: string): string {
    const L = this.lang.id;
    if (L === 'python' || L === 'go') return `${i}${e}`;
    return `${i}${e};`;
  }
  private varDeclLine(i: string, n: string, mut: boolean, init: string, ty?: A.HType): string {
    const L = this.lang.id;
    if (L === 'python') return `${i}${n} = ${init}`;
    if (L === 'typescript' || L === 'javascript') return `${i}${mut ? 'let' : 'const'} ${n} = ${init};`;
    // rust/cpp：有类型注解时显式声明（i64 语义 —— rust 字面量默认推断 i32，cpp auto 推断 int，
    // 均会在大值场景截断；注解在场则照实投射）
    if (L === 'rust') {
      const t = ty ? this.ctx.ty(ty) : '';
      return t ? `${i}let ${mut ? 'mut ' : ''}${n}: ${t} = ${init};` : `${i}let ${mut ? 'mut ' : ''}${n} = ${init};`;
    }
    if (L === 'go') {
      const t = ty ? this.ctx.ty(ty) : '';
      return t ? `${i}var ${n} ${t} = ${init}` : `${i}${n} := ${init}`;
    }
    if (L === 'cpp') {
      const t = ty ? this.ctx.ty(ty) : '';
      return t ? `${i}${t} ${n} = ${init};` : `${i}auto ${n} = ${init};`;
    }
    return `${i}auto ${n} = ${init};`;
  }
  private varNoInitLine(i: string, n: string, _mut: boolean): string {
    const L = this.lang.id;
    if (L === 'python') return `${i}${n} = None`;
    if (L === 'typescript' || L === 'javascript') return `${i}let ${n} = null;`;
    if (L === 'rust') return `${i}let ${n}: _ = todo!();`;
    if (L === 'go') return `${i}var ${n} any`;
    return `${i}auto ${n}{};`;
  }
  private ifLine(i: string, c: string): string {
    const L = this.lang.id;
    if (L === 'python') return `${i}if ${c}:`;
    if (L === 'rust' || L === 'go') return `${i}if ${c} {`;
    return `${i}if (${c}) {`;
  }
  private elseLine(i: string): string {
    const L = this.lang.id;
    if (L === 'python') return `${i}else:`;
    return `${i}} else {`;
  }
  private closeLine(i: string): string {
    return this.lang.id === 'python' ? '' : `${i}}`;
  }
  private whileLine(i: string, c: string): string {
    const L = this.lang.id;
    if (L === 'python') return `${i}while ${c}:`;
    if (L === 'rust' || L === 'go') return `${i}for ${c} {`;
    return `${i}while (${c}) {`;
  }
  private loopLine(i: string): string {
    const L = this.lang.id;
    if (L === 'python') return `${i}while True:`;
    if (L === 'rust') return `${i}loop {`;
    if (L === 'go') return `${i}for {`;
    return `${i}while (true) {`;
  }
  private forRangeLines(i: string, n: string, lo: string | undefined, hi: string | undefined, inc: boolean): string {
    const L = this.lang.id;
    // v0.2.28: 支持半开 range（..n / n..）
    const loStr = lo ?? '0';
    if (!hi) throw new TranspileError('for range 无上界');
    if (L === 'python') return `${i}for ${n} in range(${loStr}, ${hi}${inc ? ' + 1' : ''}):`;
    if (L === 'rust') return `${i}for ${n} in ${lo ?? ''}..${inc ? '=' : ''}${hi} {`;
    if (L === 'go') return `${i}for ${n} := ${loStr}; ${n} < ${hi}${inc ? ' + 1' : ''}; ${n}++ {`;
    if (L === 'typescript' || L === 'javascript') return `${i}for (let ${n} = ${loStr}; ${n} < ${hi}${inc ? ' + 1' : ''}; ${n}++) {`;
    return `${i}for (auto ${n} = ${loStr}; ${n} < ${hi}${inc ? ' + 1' : ''}; ${n}++) {`;
  }
  private forInLines(i: string, n: string, it: string): string {
    const L = this.lang.id;
    if (L === 'python') return `${i}for ${n} in ${it}:`;
    if (L === 'rust') return `${i}for ${n} in &${it} {`;
    if (L === 'go') return `${i}for _, ${n} := range ${it} {`;
    if (L === 'typescript' || L === 'javascript') return `${i}for (const ${n} of ${it}) {`;
    return `${i}for (auto& ${n} : ${it}) {`;
  }
  private returnLine(i: string, v: string | null): string {
    const L = this.lang.id;
    if (L === 'python' || L === 'go') return v ? `${i}return ${v}` : `${i}return`;
    return v ? `${i}return ${v};` : `${i}return;`;
  }
  /** 分支尾行：true → return；string（变量名）→ 赋值（let 块初始化）；false → 表达式语句 */
  private valueTailLine(i: string, v: string, mode: boolean | string): string {
    if (mode === true) return this.returnLine(i, v);
    if (typeof mode === 'string') return this.assignLine(i, mode, v);
    return this.exprStmtLine(i, v);
  }
  private assignLine(i: string, n: string, v: string): string {
    const L = this.lang.id;
    if (L === 'python' || L === 'go') return `${i}${n} = ${v}`;
    return `${i}${n} = ${v};`;
  }
  private passLine(i: string): string {
    const L = this.lang.id;
    if (L === 'python') return `${i}pass`;
    if (L === 'go') return `${i}_ = 0`;
    if (L === 'rust') return `${i}/* pass */`;
    return `${i}(void)0;`;
  }
}

// ---------------------------------------------------------------------------
// 入口
// ---------------------------------------------------------------------------

export function transpileBody(
  fn: A.FnDef,
  lang: LangSpec,
  ctx: BodyCtx,
  indent = '    ',
): string[] {
  if (!fn.body) throw new TranspileError('无函数体');
  const b = new Body(lang, ctx);
  for (const prm of fn.params) {
    const n = patternName(prm.pat);
    if (n) b.bindParam(n, prm.ty);
  }
  const out: string[] = [];
  const body = fn.body;
  if (body.length === 0) { out.push(lang.id === 'python' ? `${indent}pass` : lang.id === 'go' ? `${indent}_ = 0` : `${indent}(void)0;`); return out; }
  for (let i = 0; i < body.length; i++) {
    const st = body[i]!;
    const isTail = i === body.length - 1;
    // 尾表达式（无分号）→ 值语义：match/if 分支产出 return；普通表达式按返回类型决定
    if (isTail && st.kind === 'expr' && !st.hasSemi) {
      const ek = st.expr.kind;
      if (ek === 'match') {
        b.matchTail(st.expr, indent, out);
        continue;
      }
      if (ek === 'if') {
        b.ifChain(st.expr, indent, out, true);
        continue;
      }
      if (ek === 'iflet') {
        const hasRet = !!fn.ret && !(fn.ret.kind === 'path' && fn.ret.segs.join('') === '()');
        if (hasRet) {
          // 尾位置 if-let 值语义（需 else，否则内部 throw 回退 contract）
          b.ifLetTail(st.expr, indent, out);
          continue;
        }
      }
      if (!['loop', 'while', 'whilelet', 'for', 'block'].includes(ek)) {
        const hasRet = !!fn.ret && !(fn.ret.kind === 'path' && fn.ret.segs.join('') === '()');
        if (hasRet) {
          out.push(b.returnLinePublic(indent, b.exprPublic(st.expr)));
          continue;
        }
      }
    }
    b.stmtEx(st, indent, out);
  }
  return out;
}

// ---------------------------------------------------------------------------
// 宏 token 工具（保留定界符，与 interp.treeToTokens 同构）
// ---------------------------------------------------------------------------

interface FlatTok { kind: string; text: string }

function flatTokens(tree: A.TokenTree): FlatTok[] {
  const out: FlatTok[] = [];
  const walk = (t: A.TokenTree): void => {
    if (t.t === 'tok') out.push({ kind: t.tok.kind, text: t.tok.text });
    else {
      out.push({ kind: 'punct', text: t.open });
      for (const item of t.items) walk(item);
      out.push({ kind: 'punct', text: t.close });
    }
  };
  walk(tree);
  return out;
}

function splitTopLevel(toks: FlatTok[]): FlatTok[][] {
  const groups: FlatTok[][] = [[]];
  let depth = 0;
  for (const t of toks) {
    if (t.text === '(' || t.text === '[' || t.text === '{') depth++;
    if (t.text === ')' || t.text === ']' || t.text === '}') depth--;
    if (t.text === ',' && depth === 0) { groups.push([]); continue; }
    groups[groups.length - 1]!.push(t);
  }
  return groups.filter((g) => g.length > 0);
}

function macroFmtString(groups: FlatTok[][]): string {
  const tok = groups[0]?.[0];
  if (!tok || (tok.kind !== 'string' && tok.kind !== 'str')) throw new TranspileError('宏首参须字符串字面量');
  return tok.text;
}

// ---------------------------------------------------------------------------
// 运行期 prelude（@dhv:generated）
// ---------------------------------------------------------------------------

export function languagePrelude(langId: string, goSkipHelpers = false): string[] {
  switch (langId) {
    case 'python':
      return [
        '# @dhv:generated — 运行期助手（不可手改，下次编译覆盖）',
        'def _dhv_unwrap(x):',
        '    if x is None:',
        "        raise RuntimeError('dhv: unwrap on None')",
        '    return x',
        'def _dhv_expect(x, msg):',
        '    if x is None:',
        '        raise RuntimeError(f"dhv: {msg}")',
        '    return x',
        'def _dhv_get(c, k):',
        '    try:',
        '        return c[k]',
        '    except (IndexError, KeyError):',
        '        return None',
        'def _dhv_unwrap_or(x, d):',
        '    return x if x is not None else d',
        'def _dhv_unwrap_or_else(x, f):',
        '    return x if x is not None else f()',
        'def _dhv_and_then(x, f):',
        '    return f(x) if x is not None else None',
        'def _dhv_or(x, alt):',
        '    return x if x is not None else alt',
        'def _dhv_pop(v):',
        '    return v.pop() if v else None',
        'def _dhv_clone(x):',
        '    if isinstance(x, list): return list(x)',
        '    if isinstance(x, dict): return dict(x)',
        '    return x',
        'def _dhv_is_sorted(v):',
        '    return all(v[i] <= v[i + 1] for i in range(len(v) - 1))',
        'def _dhv_strip_prefix(s, p):',
        '    return s[len(p):] if s.startswith(p) else None',
        'def _dhv_strip_suffix(s, p):',
        '    return s[:len(s) - len(p)] if s.endswith(p) else None',
        'def _dhv_find(s, sub):',
        '    i = s.find(sub)',
        '    return i if i >= 0 else None',
        // v1.4.9：Option::filter（单次求值，副作用接收者安全）
        'def _dhv_filter(x, f):',
        '    return x if x is not None and f(x) else None',
        // v1.4.9：String::parse::<T>()（interp 语义：trim 后严格 [+-]?\d+；u 型拒负）
        'def _dhv_parse_int(s, ty):',
        '    import re as _dhv_re',
        "    t = str(s).strip()",
        "    if _dhv_re.fullmatch(r'[+-]?\\d+', t) is None:",
        '        return None',
        '    v = int(t)',
        "    if ty[:1] == 'u' and v < 0:",
        '        return None',
        '    return v',
        // JS Number 语义子集：Infinity/NaN 接受，空串/下划线/裸 inf 拒绝（与 interp 逐字对齐）
        'def _dhv_parse_float(s):',
        '    import re as _dhv_re',
        "    t = str(s).strip()",
        "    if t == '':",
        '        return None',
        "    if t in ('Infinity', '+Infinity'):",
        "        return float('inf')",
        "    if t == '-Infinity':",
        "        return float('-inf')",
        "    if t in ('NaN', '+NaN', '-NaN'):",
        "        return float('nan')",
        "    if _dhv_re.fullmatch(r'[+-]?(\\d+\\.?\\d*|\\.\\d+)([eE][+-]?\\d+)?', t) is None:",
        '        return None',
        '    return float(t)',
        '',
      ];
    case 'typescript':
      return [
        '// @dhv:generated — 运行期助手（不可手改，下次编译覆盖）',
        'export function _dhvUnwrap<T>(x: T | null | undefined): T {',
        "  if (x == null) throw new Error('dhv: unwrap on null');",
        '  return x;',
        '}',
        'export function _dhvExpect<T>(x: T | null | undefined, msg: string): T {',
        '  if (x == null) throw new Error(`dhv: ${msg}`);',
        '  return x;',
        '}',
        'export function _dhvRemove<K, V>(m: Map<K, V>, k: K): V | null {',
        '  const v = m.get(k) ?? null;',
        '  m.delete(k);',
        '  return v;',
        '}',
        'export function _dhvAndThen<T, R>(x: T | null, f: (v: T) => R | null): R | null {',
        '  return x != null ? f(x) : null;',
        '}',
        'export function _dhvUnwrapOrElse<T>(x: T | null, f: () => T): T {',
        '  return x != null ? x : f();',
        '}',
        'export function _dhvPop<T>(v: T[]): T | null {',
        '  return v.length > 0 ? (v.pop() as T) : null;',
        '}',
        'export function _dhvClone<T>(x: T): T {',
        '  return Array.isArray(x) ? ([...x] as T) : (x instanceof Map ? (new Map(x) as T) : x);',
        '}',
        // v1.4.9：Option::filter（单次求值，副作用接收者安全）
        'export function _dhvFilter<T>(x: T | null, f: (v: T) => boolean): T | null {',
        '  return x != null && f(x) ? x : null;',
        '}',
        // v1.4.9：String::parse::<T>()（JS Number 语义与 interp 同源 —— interp 即 JS 实现）
        'export function _dhvParseInt(s: string, ty: string): number | null {',
        "  const t = s.trim();",
        "  if (!/^[+-]?\\d+$/.test(t)) return null;",
        '  const v = Number(t);',
        "  if (ty.startsWith('u') && v < 0) return null;",
        '  return v;',
        '}',
        'export function _dhvParseFloat(s: string): number | null {',
        "  const t = s.trim();",
        "  if (t === '') return null;",
        '  const f = Number(t);',
        '  return Number.isNaN(f) ? null : f;',
        '}',
        '',
      ];
    case 'javascript':
      return [
        '// @dhv:generated — 运行期助手（不可手改，下次编译覆盖）',
        'function _dhvUnwrap(x) {',
        "  if (x == null) throw new Error('dhv: unwrap on null');",
        '  return x;',
        '}',
        'function _dhvExpect(x, msg) {',
        '  if (x == null) throw new Error(`dhv: ${msg}`);',
        '  return x;',
        '}',
        'function _dhvRemove(m, k) {',
        '  const v = m.get(k) ?? null;',
        '  m.delete(k);',
        '  return v;',
        '}',
        'function _dhvAndThen(x, f) {',
        '  return x != null ? f(x) : null;',
        '}',
        'function _dhvUnwrapOrElse(x, f) {',
        '  return x != null ? x : f();',
        '}',
        'function _dhvPop(v) {',
        '  return v.length > 0 ? v.pop() : null;',
        '}',
        'function _dhvClone(x) {',
        '  return Array.isArray(x) ? [...x] : x;',
        '}',
        // v1.4.9：Option::filter + String::parse::<T>()
        'function _dhvFilter(x, f) {',
        '  return x != null && f(x) ? x : null;',
        '}',
        'function _dhvParseInt(s, ty) {',
        "  const t = s.trim();",
        "  if (!/^[+-]?\\d+$/.test(t)) return null;",
        '  const v = Number(t);',
        "  if (ty.startsWith('u') && v < 0) return null;",
        '  return v;',
        '}',
        'function _dhvParseFloat(s) {',
        "  const t = s.trim();",
        "  if (t === '') return null;",
        '  const f = Number(t);',
        '  return Number.isNaN(f) ? null : f;',
        '}',
        '',
      ];
    case 'go': {
      // v1.4.10：go 同 package 多文件顶级助手去重 —— import 块每文件独立，
      // 助手声明仅注入首个 go 文件（goSkipHelpers 由 emit 层跨文件状态驱动；
      // 真机 go build 实测：重复声明 = 编译错误）
      const goImportBlock = [
        '// @dhv:generated — 运行期 import + 助手（不可手改，下次编译覆盖）',
        'import (',
        '\t"fmt"',
        '\t"strings"',
        '\t"slices"',
        '\t"sort"',
        '\t"strconv"',
        ')',
      ];
      if (goSkipHelpers) {
        return [
          '// @dhv:generated — 运行期 import（助手声明于本 package 首个生成文件）',
          ...goImportBlock.slice(1),
          '',
        ];
      }
      return [
        ...goImportBlock,
        '// _dhvSome — HSL Some(x) → *T（go 泛型 1.18+，类型推导取址）',
        'func _dhvSome[T any](v T) *T { return &v }',
        '// _dhvPop — HSL Vec::pop() → *T（空 Vec 返回 nil；副作用：截断末元素）',
        'func _dhvPop[T any](v *[]T) *T {',
        '\tif len(*v) == 0 { return nil }',
        '\tn := len(*v) - 1',
        '\tx := (*v)[n]',
        '\t*v = (*v)[:n]',
        '\treturn &x',
        '}',
        '// _dhvFirst / _dhvLast — HSL Vec::first()/last() → *T（空 Vec 返回 nil）',
        'func _dhvFirst[T any](v *[]T) *T {',
        '\tif len(*v) == 0 { return nil }',
        '\treturn &(*v)[0]',
        '}',
        'func _dhvLast[T any](v *[]T) *T {',
        '\tif len(*v) == 0 { return nil }',
        '\treturn &(*v)[len(*v)-1]',
        '}',
        '// _dhvVecGet — HSL Vec::get(i) → *T（越界 nil，Option 语义）',
        'func _dhvVecGet[T any](v []T, i int) *T {',
        '\tif i < 0 || i >= len(v) { return nil }',
        '\treturn &v[i]',
        '}',
        '// _dhvInsert — HSL Vec::insert(i, x)（副作用插入；三语句 append+copy+赋值不可内联）',
        'func _dhvInsert[T any](v *[]T, i int, x T) {',
        '\t*v = append(*v, x)',
        '\tcopy((*v)[i+1:], (*v)[i:])',
        '\t(*v)[i] = x',
        '}',
        '// _dhvRemoveAt — HSL Vec::remove(i) → 被删元素（越界 panic，与 Rust 同语义）',
        'func _dhvRemoveAt[T any](v *[]T, i int) T {',
        '\tx := (*v)[i]',
        '\t*v = append((*v)[:i], (*v)[i+1:]...)',
        '\treturn x',
        '}',
        '// _dhvMapGet — HSL HashMap::get(k) → *V（缺键 nil，Option 语义）',
        'func _dhvMapGet[K comparable, V any](m map[K]V, k K) *V {',
        '\tv, ok := m[k]',
        '\tif !ok { return nil }',
        '\treturn &v',
        '}',
        '// _dhvMapRemove — HSL HashMap::remove(k) → *V（缺键 nil；副作用删除。返回 *V 与',
        '// Option 指针表示一致 —— 可安全链式 .unwrap_or(d) 解引用）',
        'func _dhvMapRemove[K comparable, V any](m map[K]V, k K) *V {',
        '\tv, ok := m[k]',
        '\tif !ok { return nil }',
        '\tdelete(m, k)',
        '\treturn &v',
        '}',
        '// _dhvKeys / _dhvValues — HSL HashMap::keys()/values() → 切片',
        'func _dhvKeys[K comparable, V any](m map[K]V) []K {',
        '\tout := make([]K, 0, len(m))',
        '\tfor k := range m { out = append(out, k) }',
        '\treturn out',
        '}',
        'func _dhvValues[K comparable, V any](m map[K]V) []V {',
        '\tout := make([]V, 0, len(m))',
        '\tfor _, v := range m { out = append(out, v) }',
        '\treturn out',
        '}',
        '// v1.4.10：Option::unwrap_or —— go 无三元表达式，泛型助手（单次求值，副作用接收者安全）',
        'func _dhvUnwrapOr[T any](opt *T, def T) T {',
        '\tif opt != nil { return *opt }',
        '\treturn def',
        '}',
        '// v1.4.8：Option 链式家族非闭包方法（or / expect）',
        '// _dhvOptOr — HSL Option::or(alt) → *T（a 非 nil 取 a，否则取 b；同型指针选择）',
        'func _dhvOptOr[T any](a, b *T) *T {',
        '\tif a != nil { return a }',
        '\treturn b',
        '}',
        '// _dhvOptExpect — HSL Option::expect(msg) → T（nil 时 panic 与 interp RuntimeError 同语义）',
        'func _dhvOptExpect[T any](opt *T, msg string) T {',
        '\tif opt == nil { panic("dhv: " + msg) }',
        '\treturn *opt',
        '}',
        '// v1.4.9：String::parse::<T>() → *int64 / *float64（失败 nil；Option-flavored Result 表示）',
        '// _dhvParseInt — interp 语义：TrimSpace 后严格 [+-]?digits；unsigned 拒负',
        'func _dhvParseInt(s string, unsigned bool) *int64 {',
        '\tt := strings.TrimSpace(s)',
        '\tif t == "" { return nil }',
        '\tbody := t',
        "\tif body[0] == \'+\' || body[0] == \'-\' { body = body[1:] }",
        '\tif body == "" { return nil }',
        '\tfor i := 0; i < len(body); i++ {',
        "\t\tif body[i] < '0' || body[i] > '9' { return nil }",
        '\t}',
        '\tv, err := strconv.ParseInt(t, 10, 64)',
        '\tif err != nil { return nil }',
        '\tif unsigned && v < 0 { return nil }',
        '\treturn &v',
        '}',
        '// _dhvParseFloat — interp 语义：TrimSpace 后 ParseFloat（空串/非法 nil）',
        'func _dhvParseFloat(s string) *float64 {',
        '\tt := strings.TrimSpace(s)',
        '\tif t == "" { return nil }',
        '\tv, err := strconv.ParseFloat(t, 64)',
        '\tif err != nil { return nil }',
        '\treturn &v',
        '}',
        '// v1.4.9：char 谓词（interp 语义：[A-Za-z] 或 ≥ U+0080 —— UTF-8 首字节 ≥ 0x80 即非 ASCII）',
        'func _dhvIsAlpha(s string) bool {',
        '\tif s == "" { return false }',
        '\tc := s[0]',
        "\treturn c >= 0x80 || (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')",
        '}',
        'func _dhvIsDigit(s string) bool {',
        "\treturn len(s) == 1 && s[0] >= '0' && s[0] <= '9'",
        '}',
        '',
      ];
    }
    case 'cpp':
      // 模板/内联助手（ODR 免声明，include guard 兜底单 TU 拼接场景）
      return [
        '// @dhv:generated — 运行期助手（不可手改，下次编译覆盖）',
        '#ifndef DHV_SOME_HELPER',
        '#define DHV_SOME_HELPER',
        'template <typename T> std::optional<T> _dhvSome(const T& v) { return std::optional<T>(v); }',
        '#endif',
        '// v1.4.8：Option 链式家族（map / and_then / or / unwrap_or_else / expect）',
        '// cpp 用 auto 参数 lambda + decltype 推导返回类型，与 HSL 无类型闭包对接',
        '#ifndef DHV_OPT_HELPER',
        '#define DHV_OPT_HELPER',
        '// Option::map → std::optional<decltype(f(*opt))>（lambda 返回 R，自动包 std::optional<R>）',
        'template <typename T, typename F>',
        'auto _dhvOptMap(const std::optional<T>& opt, F f) -> std::optional<decltype(f(*opt))> {',
        '    if (opt) return std::optional<decltype(f(*opt))>(f(*opt));',
        '    return std::nullopt;',
        '}',
        '// Option::and_then → lambda 必须返回 std::optional<R>（语义对齐 Rust and_then）',
        'template <typename T, typename F>',
        'auto _dhvOptAndThen(const std::optional<T>& opt, F f) -> decltype(f(*opt)) {',
        '    if (opt) return f(*opt);',
        '    return std::nullopt;',
        '}',
        '// Option::or(alt) → 同型 optional 选择（a 有值取 a，否则取 b）',
        'template <typename T>',
        'std::optional<T> _dhvOptOr(const std::optional<T>& a, const std::optional<T>& b) {',
        '    return a ? a : b;',
        '}',
        '// Option::unwrap_or_else(f) → 有值返回 *opt，否则调零参 lambda 返回 T',
        'template <typename T, typename F>',
        'T _dhvOptUnwrapOrElse(const std::optional<T>& opt, F f) {',
        '    return opt ? *opt : f();',
        '}',
        '// Option::expect(msg) → 有值返回 *opt，否则 throw std::runtime_error',
        'template <typename T>',
        'T _dhvOptExpect(const std::optional<T>& opt, const std::string& msg) {',
        '    if (!opt) throw std::runtime_error("dhv: " + msg);',
        '    return *opt;',
        '}',
        '// v1.4.9：Option::filter → 谓词真保留 Some，否则 nullopt（与 interp Option::filter 同语义）',
        'template <typename T, typename F>',
        'std::optional<T> _dhvOptFilter(const std::optional<T>& opt, F f) {',
        '    if (opt && f(*opt)) return opt;',
        '    return std::nullopt;',
        '}',
        '// v1.4.9：裸 Option::None 链式包装器（std::nullopt_t 无成员/无模板推导通道 ——',
        '// None.map(f).value_or(d) 等链式场景；语义全部化简为 None 恒等）',
        '#ifndef DHV_NONE_HELPER',
        '#define DHV_NONE_HELPER',
        'struct _dhvNoneT {',
        '    bool has_value() const { return false; }',
        '    template <typename D> D value_or(D&& d) const { return std::forward<D>(d); }',
        '    template <typename F> _dhvNoneT map(F&&) const { return {}; }',
        '    template <typename F> _dhvNoneT and_then(F&&) const { return {}; }',
        '    template <typename F> _dhvNoneT filter(F&&) const { return {}; }',
        '    template <typename F> auto unwrap_or_else(F&& g) const -> decltype(g()) { return g(); }',
        '    template <typename T> operator std::optional<T>() const { return std::nullopt; }',
        '};',
        'inline constexpr _dhvNoneT _dhvNone{};',
        '#endif',
        '#endif',
        '#ifndef DHV_POP_HELPER',
        '#define DHV_POP_HELPER',
        'template <typename T> std::optional<T> _dhvPop(std::vector<T>& v) {',
        '    if (v.empty()) return std::nullopt;',
        '    T x = v.back();',
        '    v.pop_back();',
        '    return x;',
        '}',
        'template <typename T> std::optional<T> _dhvFirst(std::vector<T>& v) {',
        '    return v.empty() ? std::nullopt : std::optional<T>(v.front());',
        '}',
        'template <typename T> std::optional<T> _dhvLast(std::vector<T>& v) {',
        '    return v.empty() ? std::nullopt : std::optional<T>(v.back());',
        '}',
        '// Vec::get → Option（越界 nullopt）',
        'template <typename T> std::optional<T> _dhvVecGet(const std::vector<T>& v, int64_t i) {',
        '    if (i < 0 || static_cast<size_t>(i) >= v.size()) return std::nullopt;',
        '    return v[static_cast<size_t>(i)];',
        '}',
        '// Vec::insert(i, x)（越界 clean throw，Rust 同语义）',
        'template <typename T> void _dhvInsert(std::vector<T>& v, int64_t i, const T& x) {',
        '    if (i < 0 || static_cast<size_t>(i) > v.size()) throw std::out_of_range("dhv: Vec::insert index out of range");',
        '    v.insert(v.begin() + i, x);',
        '}',
        '// Vec::remove(i) → 被删元素（std::vector::erase 返回 iterator 非元素）',
        'template <typename T> T _dhvRemoveAt(std::vector<T>& v, int64_t i) {',
        '    if (i < 0 || static_cast<size_t>(i) >= v.size()) throw std::out_of_range("dhv: Vec::remove index out of range");',
        '    T x = v[static_cast<size_t>(i)];',
        '    v.erase(v.begin() + i);',
        '    return x;',
        '}',
        '// Vec::extend/append → 批量追加（const ref 绑定临时，防 begin/end 不同源 length_error）',
        'template <typename T> void _dhvExtend(std::vector<T>& v, const std::vector<T>& other) {',
        '    v.insert(v.end(), other.begin(), other.end());',
        '}',
        '#endif',
        '#ifndef DHV_MAP_HELPER',
        '#define DHV_MAP_HELPER',
        'template <typename M> std::vector<typename M::key_type> _dhvKeys(const M& m) {',
        '    std::vector<typename M::key_type> out;',
        '    out.reserve(m.size());',
        '    for (const auto& _kv : m) out.push_back(_kv.first);',
        '    return out;',
        '}',
        'template <typename M> std::vector<typename M::mapped_type> _dhvValues(const M& m) {',
        '    std::vector<typename M::mapped_type> out;',
        '    out.reserve(m.size());',
        '    for (const auto& _kv : m) out.push_back(_kv.second);',
        '    return out;',
        '}',
        '// HashMap::get → Option（缺键 nullopt）',
        'template <typename M> std::optional<typename M::mapped_type> _dhvMapGet(const M& m, const typename M::key_type& k) {',
        '    auto _it = m.find(k);',
        '    if (_it == m.end()) return std::nullopt;',
        '    return _it->second;',
        '}',
        '// HashMap::remove → Option 旧值（缺键 nullopt；副作用删除）',
        'template <typename M> std::optional<typename M::mapped_type> _dhvMapRemove(M& m, const typename M::key_type& k) {',
        '    auto _it = m.find(k);',
        '    if (_it == m.end()) return std::nullopt;',
        '    typename M::mapped_type _v = _it->second;',
        '    m.erase(_it);',
        '    return _v;',
        '}',
        '#endif',
        '#ifndef DHV_STR_HELPER',
        '#define DHV_STR_HELPER',
        '// String 助手（C++ 标准库无 trim/replaceAll/split 等便捷函数）',
        'inline std::string _dhvTrim(const std::string& s) {',
        '    size_t b = s.find_first_not_of(" \\t\\r\\n");',
        '    if (b == std::string::npos) return "";',
        '    size_t e = s.find_last_not_of(" \\t\\r\\n");',
        '    return s.substr(b, e - b + 1);',
        '}',
        'inline std::string _dhvToLower(std::string s) {',
        '    std::transform(s.begin(), s.end(), s.begin(), [](unsigned char c) { return static_cast<char>(std::tolower(c)); });',
        '    return s;',
        '}',
        'inline std::string _dhvToUpper(std::string s) {',
        '    std::transform(s.begin(), s.end(), s.begin(), [](unsigned char c) { return static_cast<char>(std::toupper(c)); });',
        '    return s;',
        '}',
        'inline std::string _dhvReplaceAll(std::string s, const std::string& from, const std::string& to) {',
        '    if (from.empty()) return s;',
        '    size_t pos = 0;',
        '    while ((pos = s.find(from, pos)) != std::string::npos) {',
        '        s.replace(pos, from.size(), to);',
        '        pos += to.size();',
        '    }',
        '    return s;',
        '}',
        'inline std::vector<std::string> _dhvSplit(const std::string& s, const std::string& sep) {',
        '    std::vector<std::string> out;',
        '    if (sep.empty()) {',
        '        for (char c : s) out.push_back(std::string(1, c));',
        '        return out;',
        '    }',
        '    size_t start = 0;',
        '    for (size_t p; (p = s.find(sep, start)) != std::string::npos; start = p + sep.size()) {',
        '        out.push_back(s.substr(start, p - start));',
        '    }',
        '    out.push_back(s.substr(start));',
        '    return out;',
        '}',
        'inline std::vector<std::string> _dhvSplitWS(const std::string& s) {',
        '    std::vector<std::string> out;',
        '    size_t i = 0, n = s.size();',
        '    while (i < n) {',
        '        while (i < n && std::isspace(static_cast<unsigned char>(s[i]))) i++;',
        '        size_t b = i;',
        '        while (i < n && !std::isspace(static_cast<unsigned char>(s[i]))) i++;',
        '        if (i > b) out.push_back(s.substr(b, i - b));',
        '    }',
        '    return out;',
        '}',
        '// UTF-8 码点计数（std::string::size 是字节数）',
        'inline size_t _dhvCharCount(const std::string& s) {',
        '    size_t n = 0;',
        '    for (unsigned char c : s) if ((c & 0xC0) != 0x80) n++;',
        '    return n;',
        '}',
        'inline std::string _dhvRepeat(const std::string& s, int64_t n) {',
        '    std::string out;',
        '    if (n > 0) out.reserve(s.size() * static_cast<size_t>(n));',
        '    for (int64_t i = 0; i < n; i++) out += s;',
        '    return out;',
        '}',
        '// Vec::join —— if constexpr 分发 string/数值元素（interp join 对数值走 display）',
        'template <typename T> std::string _dhvJoin(const std::vector<T>& v, const std::string& sep) {',
        '    std::string out;',
        '    for (size_t i = 0; i < v.size(); i++) {',
        '        if (i > 0) out += sep;',
        '        if constexpr (std::is_same_v<T, std::string>) out += v[i];',
        '        else out += std::to_string(v[i]);',
        '    }',
        '    return out;',
        '}',
        '#endif',
        '#ifndef DHV_PARSE_HELPER',
        '#define DHV_PARSE_HELPER',
        '// v1.4.9：String::parse::<T>() → std::optional<T>（Err → nullopt，Option-flavored Result 表示）',
        '// interp 语义：trim 后严格整数/浮点语法；unsigned 拒负；i64 溢出 → nullopt（interp BigInt 任意精度，文档化差异）',
        'template <typename T>',
        'std::optional<T> _dhvParse(const std::string& s) {',
        '    size_t b = s.find_first_not_of(" \\t\\r\\n");',
        '    if (b == std::string::npos) return std::nullopt;',
        '    size_t e = s.find_last_not_of(" \\t\\r\\n");',
        '    std::string t = s.substr(b, e - b + 1);',
        '    try {',
        '        if constexpr (std::is_integral_v<T>) {',
        '            size_t pos = 0;',
        '            long long v = std::stoll(t, &pos);',
        '            if (pos != t.size()) return std::nullopt;',
        '            if (!std::is_signed_v<T> && v < 0) return std::nullopt;',
        '            return static_cast<T>(v);',
        '        } else {',
        '            size_t pos = 0;',
        '            double v = std::stod(t, &pos);',
        '            if (pos != t.size()) return std::nullopt;',
        '            return static_cast<T>(v);',
        '        }',
        '    } catch (...) {',
        '        return std::nullopt;',
        '    }',
        '}',
        '#endif',
        '#ifndef DHV_CHAR_HELPER',
        '#define DHV_CHAR_HELPER',
        '// v1.4.9：char 谓词（interp 语义：[A-Za-z] 或 ≥ U+0080 —— UTF-8 首字节 ≥ 0x80 即非 ASCII，精确对齐）',
        'inline bool _dhvIsAlpha(const std::string& s) {',
        '    if (s.empty()) return false;',
        '    unsigned char c = static_cast<unsigned char>(s[0]);',
        '    return c >= 0x80 || std::isalpha(c) != 0;',
        '}',
        'inline bool _dhvIsDigit(const std::string& s) {',
        '    return !s.empty() && std::isdigit(static_cast<unsigned char>(s[0])) != 0;',
        '}',
        '#endif',
        '',
      ];
    default:
      return [];
  }
}

/**
 * v0.2.51：std/math 自由函数跨语言映射（pathCall 入口）。
 * 此前 std 函数调用在活体翻译中裸名直出 —— sin(x) 生成 sin(x)，python
 * NameError / ts 编译错误。映射覆盖可廉价直译的子集；不可廉价直译的
 * （rust/go/cpp 方法形态、python 的整数数论族）返回 null 由调用方决定，
 * 在 pathCall 语境下抛 TranspileError → 触发该函数诚实回退 contract。
 */
function stdMathFreeCall(name: string, args: string[], L: string): string | null {
  const a = args.join(', ');
  const one = args[0] ?? '';
  if (L === 'python') {
    const PY: Record<string, string> = {
      sin: `math.sin(${a})`, cos: `math.cos(${a})`, tan: `math.tan(${a})`,
      asin: `math.asin(${a})`, acos: `math.acos(${a})`, atan: `math.atan(${a})`,
      atan2: `math.atan2(${a})`, exp: `math.exp(${a})`, ln: `math.log(${a})`,
      log2: `math.log2(${a})`, log10: `math.log10(${a})`, pow: `math.pow(${a})`,
      sqrt: `math.sqrt(${a})`, hypot: `math.hypot(${a})`,
      is_nan: `math.isnan(${a})`, is_infinite: `math.isinf(${a})`,
      gcd: `math.gcd(${a})`, lcm: `math.lcm(${a})`, isqrt: `math.isqrt(${a})`,
      signum: `(0.0 if ${one} == 0 else (1.0 if ${one} > 0 else -1.0))`,
    };
    if (name === 'inf') return 'math.inf';
    const hit = PY[name];
    if (hit) return hit;
    return null;
  }
  if (L === 'typescript' || L === 'javascript') {
    const TS: Record<string, string> = {
      sin: `Math.sin(${a})`, cos: `Math.cos(${a})`, tan: `Math.tan(${a})`,
      asin: `Math.asin(${a})`, acos: `Math.acos(${a})`, atan: `Math.atan(${a})`,
      atan2: `Math.atan2(${a})`, exp: `Math.exp(${a})`, ln: `Math.log(${a})`,
      log2: `Math.log2(${a})`, log10: `Math.log10(${a})`, pow: `Math.pow(${a})`,
      sqrt: `Math.sqrt(${a})`, hypot: `Math.hypot(${a})`,
      signum: `Math.sign(${a})`,
      is_nan: `Number.isNaN(${a})`, is_infinite: `(!Number.isFinite(${a}))`,
    };
    if (name === 'inf') return 'Infinity';
    const hit = TS[name];
    if (hit) return hit;
    return null;
  }
  // rust / go / cpp：自由函数是方法形态（(x).sin()），不可廉价直译 → null
  return null;
}

function snakeUpper(s: string): string {
  return s.replace(/([a-z0-9])([A-Z])/g, '$1_$2').toUpperCase();
}
function lowerFirst(s: string): string {
  return s ? s[0]!.toLowerCase() + s.slice(1) : s;
}
function capitalize(s: string): string {
  return s ? s[0]!.toUpperCase() + s.slice(1) : s;
}
