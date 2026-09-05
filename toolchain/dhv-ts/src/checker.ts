// ============================================================================
// dhv-ts/src/checker.ts — dhv-ts 静态检查（解释器级）
// ----------------------------------------------------------------------------
// 检查范围（完整类型推导归 dhv Rust 编译器管，此处做结构级/词法级铁律）：
//   S-2  裸 .unwrap() 警告
//   S-4  对不可变绑定赋值（作用域分析）
//   S-6  match 穷尽性（枚举注册表）+ graph AgentLoop 内 _ 通配兜底
//   S-7  未使用的 let / import
//   S-8  同作用域遮蔽
//   M3   import 名未被源模块 export（静态版，与 interp 运行期 M3 同权）
//   G-1  graph 必含 AgentLoop
//   G-2  edge 端点必须已声明
//   G-3  无条件环（编译期可判定死锁）
//   G-4  孤岛节点警告
//   P-3  投射目标存在性
//   P-4  投射语言合法性
//   P-6  scale 不在入口文件（警告）
//   N-1  native 语言标识合法性
// ============================================================================

import * as A from './ast';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { LoadedProgram } from './linker';
import { getLang, isStaticLangId, resolveLangId, listLangs } from './backends/registry';
import { treeToTokens } from './interp';
import { parseExprsFromTokens } from './parser';
import { isStdPath, STD_MODULES } from './std';

export interface Diag {
  severity: 'error' | 'warning';
  code: string;
  msg: string;
  file: string;
  line: number;
  col: number;
}

// 后端语言集合来自 backends/registry（BNF v1.4 §5.2，38 后端）
const NATIVE_LANGS = new Set(listLangs().map((l) => l.id));

// v0.2.51 E-2：当前文件可见的可调用名（顶层 fn/graph/macrodef/import 名）。
// 检查器按文件串行运行，模块级游标是单线程安全的；
// 修复盲区：此前调用未定义/未 import 的函数（如 sort_desc 漏 import）
// check 全绿、run 才炸 —— 符号解析是纯静态可判定的，不应留到运行期。
let visibleCallables: Set<string> = new Set();
// 单段调用白名单：预导入构造器（两段路径形式不在本检查范围）
const CALL_WHITELIST = new Set(['Ok', 'Err', 'Some', 'None']);

// v0.2.53 L-2：import 别名 → 枚举原名映射（S-6 穷尽性对别名臂生效）。
// 修复盲区：此前 `import { Shape as S }` 后 match `S::Circle` 臂因首段 'S'
// 不在 enums 注册表而被穷尽性检查忽略 —— 别名臂完全绕过 S-6。
let enumAlias: Map<string, string> = new Map();

// v0.2.56 S-18（#L-22）：已知结构体名（值模型断层预警用）。
// 语义：native 块返回的 plain object 不带 __struct/__enum 运行时标记 →
// foreign 值 —— 字段直通可用，但 .clone()/方法/模式派发全失效（Curator
// 实录运行期 panic「foreign 没有方法 clone」）。结构体字面量注解（let :
// Vec<Entity> = native）是静态可判定的断层现场 —— 提前警告。
let knownStructs: Set<string> = new Set();

export function checkProgram(program: LoadedProgram): Diag[] {
  const diags: Diag[] = [];
  const enums = new Map<string, string[]>(); // name -> variants
  knownStructs = new Set<string>();
  for (const [, ast] of program.files) {
    for (const item of ast.items) {
      if (item.kind === 'enum') enums.set(item.name, item.variants.map((v) => v.name));
      if (item.kind === 'struct') knownStructs.add(item.name);
    }
  }

  // L-2：构建别名 → 原名映射（仅枚举；struct 别名不参与穷尽性）
  enumAlias = new Map<string, string>();
  for (const [file, ast] of program.files) {
    for (const item of ast.items) {
      if (item.kind !== 'import') continue;
      const entries =
        item.spec.t === 'single'
          ? [{ name: item.spec.name, alias: item.spec.alias }]
          : item.spec.t === 'items'
            ? item.spec.items.map((it) => ({ name: it.name, alias: it.alias }))
            : [];
      const srcPath = path.resolve(path.dirname(file), item.path);
      const srcAst = program.files.get(srcPath);
      if (!srcAst) continue; // 标准库虚拟模块 / 不可解析路径（M3/L-0 由别处报）
      for (const e of entries) {
        if (!e.alias) continue;
        const hit = srcAst.items.find(
          (it) => it.kind === 'enum' && (it as { name?: string }).name === e.name,
        );
        if (hit) enumAlias.set(e.alias, e.name);
      }
    }
  }

  // M3（静态）：import 名必须被源模块 export —— 与 interp linkProgram 运行期检查同权，
  // 但提前到 check 阶段（否则 emit/run 才报错，check 却全绿 —— nova 实录过的盲区）。
  const exportMap = new Map<string, Set<string>>();
  for (const [file, ast] of program.files) {
    const s = new Set<string>();
    for (const item of ast.items) {
      if (item.kind === 'import' || item.kind === 'macrodef' || item.kind === 'macrocallitem') continue;
      const name = item.kind === 'fn' ? item.fn.name : item.kind === 'graph' ? item.graph.name : item.kind === 'impl' ? '' : (item as { name?: string }).name;
      if (name && (item as { exported?: boolean }).exported) s.add(name);
    }
    exportMap.set(file, s);
  }
  for (const [file, ast] of program.files) {
    for (const item of ast.items) {
      if (item.kind !== 'import') continue;
      if (isStdPath(item.path)) continue; // 标准库虚拟模块：名字全部隐式可见
      const resolved = path.resolve(path.dirname(file), item.path);
      const exp = exportMap.get(resolved);
      if (!exp) continue; // 路径不存在已由 linker L-0 报告
      const names: string[] = item.spec.t === 'items' ? item.spec.items.map((x) => x.name) : item.spec.t === 'single' ? [item.spec.name] : [];
      for (const n of names) {
        if (!exp.has(n)) {
          diags.push(err('M3', `import 失败："${n}" 未被 ${path.basename(resolved)} export`, item.span, file));
        }
      }
    }
  }

  for (const [file, ast] of program.files) {
    const topLevel = new Set<string>();
    // v0.2.51 E-2：收集本文件可见可调用名（含 import 别名）
    visibleCallables = new Set<string>();
    for (const item of ast.items) {
      const nm = itemCallableName(item);
      if (nm) visibleCallables.add(nm);
    }
    for (const item of ast.items) {
      if (item.kind === 'import') {
        if (item.spec.t === 'items') for (const x of item.spec.items) visibleCallables.add(x.alias ?? x.name);
        if (item.spec.t === 'single') visibleCallables.add(item.spec.alias ?? item.spec.name);
      }
    }
    // 嵌套 fn（graph body / 块语句内的 fn 项）同样可调用（nova 实录：
    // graph 体内的 fn evidence_mass 等嵌套定义）。与 S-7 同款源码行扫描，
    // 方向是过宽（字符串/注释里的 fn 名进白名单）而非误报 —— 误报会阻断合法工程。
    {
      const lines = astSource(file);
      for (const line of lines) {
        const m = /\b(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)/.exec(line);
        if (m) visibleCallables.add(m[1]!);
      }
    }
    // P-3/P-4：project 投射
    if (ast.project) {
      for (const p of ast.project.items) {
        const name = p.target[p.target.length - 1]!;
        const item = ast.items.find((it) => (it.kind === 'fn' ? it.fn.name : it.kind === 'graph' ? it.graph.name : it.kind === 'blockres' || it.kind === 'struct' || it.kind === 'enum' || it.kind === 'impl' || it.kind === 'trait' || it.kind === 'const' || it.kind === 'typealias' ? (it as { name: string }).name : '') === name);
        if (!item) {
          // 也可能是 import 引入的名字
          const imported = ast.items.some((it) => it.kind === 'import' && (it.spec.t === 'items' && it.spec.items.some((x) => x.name === name) || it.spec.t === 'single' && it.spec.name === name || it.spec.t === 'glob'));
          if (!imported) {
            diags.push(err('P-3', `投射目标 "${p.target.join('::')}" 未定义（或未 import）`, p.span, file));
            continue;
          }
        }
        const kind = item?.kind;
        if (kind === 'blockres') {
          if (!isStaticLangId(resolveLangId(p.lang))) {
            const staticList = listLangs().filter((l) => l.tier === 0).map((l) => l.id).join('/');
            diags.push(err('P-4', `静态资源 ${name} 只能投射到 ${staticList}（得到 ${p.lang}）`, p.span, file));
          }
        } else if (kind === 'fn' || kind === 'graph' || kind === 'struct' || kind === 'impl' || kind === 'trait') {
          if (!getLang(resolveLangId(p.lang)) || isStaticLangId(resolveLangId(p.lang))) {
            const codeList = listLangs().filter((l) => l.tier !== 0).map((l) => l.id).join('/');
            diags.push(err('P-4', `${kind} ${name} 只能投射到编程语言后端（得到 ${p.lang}；注册表：${codeList}）`, p.span, file));
          }
        }
      }
    }
    // §2.15（BNF v1.5）rules 声明校验：R3 重复类型 / R4 未知类型 / R2 占位符 / 语言注册
    if (ast.project && ast.project.rules.length > 0) {
      const RULE_KINDS = new Set(['graph', 'fn', 'struct', 'enum', 'trait', 'const', 'type', 'block', 'static']);
      const seenRuleKinds = new Set<string>();
      for (const r of ast.project.rules) {
        if (!RULE_KINDS.has(r.kind)) {
          diags.push(err('P-5', `投射规则类型 "${r.kind}" 未注册（支持 graph/fn/struct/enum/trait/const/type/block/static）`, r.span, file));
          continue;
        }
        if (seenRuleKinds.has(r.kind)) {
          diags.push(err('P-5', `投射规则类型 "${r.kind}" 重复声明（R3：同一类型只允许一条规则）`, r.span, file));
          continue;
        }
        seenRuleKinds.add(r.kind);
        for (const m of r.path.match(/\{([^}]*)\}/g) ?? []) {
          if (m !== '{name}') {
            diags.push(err('P-5', `投射规则路径占位符 ${m} 未注册（v1 仅支持 {name}）`, r.span, file));
          }
        }
        if (!getLang(resolveLangId(r.lang))) {
          diags.push(err('P-4', `投射规则语言 "${r.lang}" 未注册（registry 见 targets）`, r.span, file));
        }
      }
    }

    // P-6：scale 应在含 graph 的文件
    if (ast.scale && !ast.items.some((it) => it.kind === 'graph')) {
      diags.push(warn('P-6', 'scale 声明应出现在含 graph 的入口文件', ast.scale.span, file));
    }

    // 顶层重名 E-001
    for (const item of ast.items) {
      const name = itemName(item);
      if (!name) continue;
      if (topLevel.has(name)) {
        diags.push(err('E-1', `重复定义顶层项 "${name}"`, spanOf(item), file));
      }
      topLevel.add(name);
    }

    // import 使用检查（S-7）：源码行扫描（排除 import 行自身）
    const srcLines = astSource(file);
    for (const item of ast.items) {
      if (item.kind !== 'import') continue;
      const names: string[] = [];
      if (item.spec.t === 'items') for (const x of item.spec.items) names.push(x.alias ?? x.name);
      if (item.spec.t === 'single') names.push(item.spec.alias ?? item.spec.name);
      for (const n of names) {
        if (n.startsWith('_')) continue;
        const used = srcLines.some((line, idx) => {
          const trimmed = line.trim();
          if (trimmed.startsWith('import') || trimmed.startsWith('//') || trimmed.startsWith('*')) return false;
          void idx;
          return new RegExp(`\\b${escapeRe(n)}\\b`).test(line);
        });
        if (!used) diags.push(err('S-7', `import "${n}" 未使用`, item.span, file));
      }
      // v0.2.51 E-2：std 导入名必须在对应虚拟模块中真实存在
      // （此前 import { sort_dsc } 拼写错误要到 run 才报“不是可调用项”）
      if (isStdPath(item.path) && item.path !== 'std') {
        const mod = STD_MODULES[item.path];
        if (!mod) {
          diags.push(err('E-2', `std 模块 "${item.path}" 不存在（可用：${Object.keys(STD_MODULES).join('/')}）`, item.span, file));
        } else {
          for (const n of names) {
            if (n.startsWith('_')) continue;
            if (!(n in mod)) {
              diags.push(err('E-2', `std 模块 "${item.path}" 无导出名 "${n}"（候选：${Object.keys(mod).slice(0, 12).join(', ')}…）`, item.span, file));
            }
          }
        }
      }
    }

    // 逐项检查
    for (const item of ast.items) {
      checkItem(item, enums, diags, file);
    }
  }
  diags.sort((a, b) => a.file.localeCompare(b.file) || a.line - b.line || a.col - b.col);
  return diags;
}

function escapeRe(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

const srcCache = new Map<string, string[]>();
function astSource(file: string): string[] {
  let lines = srcCache.get(file);
  if (!lines) {
    try {
      lines = fs.readFileSync(file, 'utf-8').split('\n');
    } catch {
      lines = [];
    }
    srcCache.set(file, lines);
  }
  return lines;
}

function itemName(item: A.Item): string | undefined {
  switch (item.kind) {
    case 'fn': return item.fn.name;
    case 'struct': case 'enum': case 'trait': case 'typealias': case 'blockres': case 'macrodef':
      return (item as { name?: string }).name;
    case 'const': return item.name;
    case 'graph': return item.graph.name;
    default: return undefined;
  }
}

// E-2 辅助：项的「可调用名」—— fn / graph（Graph::run）/ macrodef；
// 类型/常量不是调用目标（枚举变体经两段路径，不在本检查范围）
function itemCallableName(item: A.Item): string | undefined {
  switch (item.kind) {
    case 'fn': return item.fn.name;
    case 'graph': return item.graph.name;
    case 'macrodef': return (item as { name?: string }).name;
    default: return undefined;
  }
}
function spanOf(item: A.Item): A.Span {
  return (item as { span: A.Span }).span;
}

function checkItem(item: A.Item, enums: Map<string, string[]>, diags: Diag[], file: string): void {
  if (item.kind === 'const') {
    // v0.2.53 S-13：const 的整型域校验（与 let 同规则；const 必有值）
    if (item.ty) checkIntLiteralRange(item.ty, item.value, diags, file);
  }
  if (item.kind === 'fn' && item.fn.body) {
    // v0.2.51：函数参数进入作用域（E-2 调用检查需要；param 标记豁免 S-7）
    const params = item.fn.params.flatMap((p) => patternNames(p.pat));
    checkBody(item.fn.body, enums, diags, file, undefined, params);
  }
  if (item.kind === 'graph') {
    checkGraph(item.graph, enums, diags, file);
  }
  if (item.kind === 'impl') {
    for (const m of item.methods) {
      if (m.body) {
        const params = m.params.flatMap((p) => patternNames(p.pat));
        checkBody(m.body, enums, diags, file, undefined, params);
      }
    }
  }
  if (item.kind === 'trait') {
    for (const ti of item.items) {
      if (ti.fn?.body) {
        const params = ti.fn.params.flatMap((p) => patternNames(p.pat));
        checkBody(ti.fn.body, enums, diags, file, undefined, params);
      }
    }
  }
}

// ---- graph 检查（G1-G4）----
function checkGraph(g: A.GraphDef, enums: Map<string, string[]>, diags: Diag[], file: string): void {
  const declared = new Set<string>();
  for (const p of g.params) declared.add(p.name);
  const nodeNames = new Set<string>();
  const edgeList: { from: string; to: string; guarded: boolean; span: A.Span }[] = [];
  let hasLoop = false;
  for (const gs of g.body) {
    if (gs.t === 'node') {
      declared.add(gs.decl.name);
      nodeNames.add(gs.decl.name);
    } else if (gs.t === 'edge') {
      for (let i = 0; i + 1 < gs.decl.endpoints.length; i++) {
        edgeList.push({
          from: gs.decl.endpoints[i]!,
          to: gs.decl.endpoints[i + 1]!,
          guarded: !!(gs.decl.guardExpr || gs.decl.guardPattern),
          span: gs.decl.span,
        });
      }
    } else if (gs.t === 'stmt' && gs.stmt.kind === 'expr' && (gs.stmt.expr as { kind: string }).kind === 'loop') {
      hasLoop = true;
    }
  }
  // G-1
  if (!hasLoop) diags.push(err('G-1', `graph ${g.name} 缺少 AgentLoop（graph body 必须恰含至少一个 loop）`, g.span, file));
  // G-2（重新扫描，要求声明先于 edge）
  const seen = new Set<string>(g.params.map((p) => p.name));
  for (const gs of g.body) {
    if (gs.t === 'node') seen.add(gs.decl.name);
    if (gs.t === 'edge') {
      for (const ep of gs.decl.endpoints) {
        if (!seen.has(ep)) diags.push(err('G-2', `edge 端点 "${ep}" 未在 graph body 中声明（node/let，且声明须先于 edge）`, gs.decl.span, file));
      }
    }
    if (gs.t === 'stmt' && gs.stmt.kind === 'let') {
      for (const n of patternNames(gs.stmt.pat)) seen.add(n);
    }
  }
  // G-3：无条件环
  const adj = new Map<string, { to: string; guarded: boolean }[]>();
  for (const e of edgeList) {
    if (!adj.has(e.from)) adj.set(e.from, []);
    adj.get(e.from)!.push({ to: e.to, guarded: e.guarded });
  }
  for (const start of adj.keys()) {
    // DFS 找回到 start 的路径，且路径上无 guard
    const stack: { node: string; anyGuard: boolean }[] = [{ node: start, anyGuard: false }];
    const visited = new Set<string>();
    while (stack.length) {
      const { node, anyGuard } = stack.pop()!;
      if (node === start && anyGuard && stack.length === 0 && visited.size > 0) continue;
      for (const e of adj.get(node) ?? []) {
        if (e.to === start) {
          if (!anyGuard && !e.guarded) {
            diags.push(err('G-3', `拓扑存在无条件环：${start} -> ... -> ${start}（环上至少一条边需 on Guard 打破，G3）`, g.span, file));
            stack.length = 0;
            break;
          }
          continue;
        }
        const key = node + '->' + e.to;
        if (visited.has(key)) continue;
        visited.add(key);
        stack.push({ node: e.to, anyGuard: anyGuard || e.guarded });
      }
    }
  }
  // G-4：孤岛节点
  const touched = new Set<string>();
  for (const e of edgeList) { touched.add(e.from); touched.add(e.to); }
  for (const n of nodeNames) {
    if (!touched.has(n)) diags.push(warn('G-4', `节点 "${n}" 没有任何 edge（孤岛节点；若为插件注入位请加注释说明）`, g.span, file));
  }
  // G-8（v0.2.53）：重复边声明 —— 同 (from, to, 守卫指纹) 二次声明报错。
  // 实证：同一条 edge 复制两遍静默通过，拓扑统计（边数/覆盖率分母/变异基线）
  // 直接翻倍污染（Gauntlet 场景实测）。守卫指纹：pattern 递归序列化；expr 守卫
  // 用源码位置（同位置 + 同端点 ≡ 复制粘贴）—— 保守口径，不误报合法的
  // 「同向多守卫」并行边（Vigil 惯用法）。
  const edgeKeys = new Map<string, A.Span>();
  for (const gs of g.body) {
    if (gs.t !== 'edge') continue;
    for (let i = 0; i + 1 < gs.decl.endpoints.length; i++) {
      const guardFp = gs.decl.guardPattern
        ? patternFingerprint(gs.decl.guardPattern)
        : gs.decl.guardExpr
          ? `expr@${gs.decl.guardExpr.span[0]}:${gs.decl.guardExpr.span[1]}`
          : 'unguarded';
      const key = `${gs.decl.endpoints[i]}->${gs.decl.endpoints[i + 1]}|${guardFp}`;
      const prev = edgeKeys.get(key);
      if (prev) {
        diags.push(err('G-8', `重复边声明：${gs.decl.endpoints[i]} -> ${gs.decl.endpoints[i + 1]}${gs.decl.guardPattern ? ' on ' + patternDisplay(gs.decl.guardPattern) : ''}（拓扑统计将翻倍污染；同向多守卫请用不同 Guard 变体）`, gs.decl.span, file));
      } else {
        edgeKeys.set(key, gs.decl.span);
      }
    }
  }
  // graph 体内的 match/赋值等检查（含 AgentLoop 内 _ 检查）
  // v0.2.51：graph 参数进入作用域（param 标记豁免 S-7，E-2 可见）
  const gscope: Scope = { vars: new Map(), used: new Set() };
  for (const p of g.params) declareParam(gscope, p.name);
  for (const gs of g.body) {
    if (gs.t === 'stmt') {
      // let 声明由 checkStmt 内部完成（此处不再重复 declare —— 否则 S-8 误报）
      checkStmt(gs.stmt, gscope, enums, diags, file, true);
    }
    if (gs.t === 'item') checkItem(gs.item, enums, diags, file);
  }
}

// ---- 语句/表达式检查（S2/S4/S6/S7/S8 + N1 + E2）----
interface Scope {
  vars: Map<string, { mut: boolean; span: A.Span; param?: boolean; litTy?: LitTy; litVal?: bigint; dom?: string }>;
  parent?: Scope;
  used: Set<string>;
}

function checkBody(stmts: A.Stmt[], enums: Map<string, string[]>, diags: Diag[], file: string, inAgentLoop?: boolean, paramNames?: string[]): void {
  
  const scope: Scope = { vars: new Map(), used: new Set() };
  if (paramNames) for (const n of paramNames) declareParam(scope, n);
  checkStmts(stmts, scope, enums, diags, file, inAgentLoop ?? false);
}

/** 声明参数绑定：参与 S-4/E-2 作用域解析，但豁免 S-7（未使用参数是合法风格） */
function declareParam(scope: Scope, name: string): void {
  if (name === '_' || name.startsWith('_')) return;
  if (scope.vars.has(name)) return;
  scope.vars.set(name, { mut: true, span: { line: 0, col: 0, file: '' }, param: true });
}

function checkStmts(stmts: A.Stmt[], scope: Scope, enums: Map<string, string[]>, diags: Diag[], file: string, inAgentLoop: boolean): void {
  for (const st of stmts) checkStmt(st, scope, enums, diags, file, inAgentLoop);
  // S-7：未使用绑定（本层声明且本层及子层未用；参数豁免 —— 未使用参数是合法风格）
  if (process.env.HSL_CHECK_DEBUG) console.error('[S7-loop] vars:', [...scope.vars.keys()], 'used:', [...scope.used]);
  for (const [name, info] of scope.vars) {
    if (name.startsWith('_')) continue;
    if (info.param) continue;
    if (!isUsed(name, scope)) {
      diags.push(err('S-7', `绑定 "${name}" 声明后未使用（_ 前缀可豁免）`, info.span, file));
    }
  }
}

function isUsed(name: string, scope: Scope): boolean {
  if (scope.used.has(name)) return true;
  return scope.parent ? isUsed(name, scope.parent) : false;
}

function declareVar(scope: Scope, name: string, mut: boolean, diags: Diag[], span: A.Span, file: string): void {
  if (name === '_') return; // 通配可重复绑定
  if (name.startsWith('_') && scope.vars.has(name)) return; // _ 前缀重复绑定豁免（Rust 语义）
  if (scope.vars.has(name)) {
    diags.push(err('S-8', `同作用域遮蔽："${name}" 已声明（S8）`, span, file));
    return;
  }
  scope.vars.set(name, { mut, span });
}

function markUsed(scope: Scope, name: string): void {
  // 沿祖先链向上标记：子作用域中的使用必须让声明作用域的 S-7 检查可见
  let s: Scope | undefined = scope;
  while (s) {
    s.used.add(name);
    s = s.parent;
  }
}

function checkStmt(st: A.Stmt, scope: Scope, enums: Map<string, string[]>, diags: Diag[], file: string, inAgentLoop: boolean): void {
  switch (st.kind) {
    case 'let': {
      
      for (const n of patternNames(st.pat)) declareVar(scope, n, st.mut, diags, st.span, file);
      if (st.init) checkExpr(st.init, scope, enums, diags, file, inAgentLoop);
      // v0.2.53 S-14（v2）：let 声明的静态字面量类型记入作用域 ——
      // 后续 path 引用可判（`let s = "abc"; s * 3` 的 lhs 是 path 不是 lit，
      // 纯 lit 口径拦不住变量中转 —— h01 样本实录）。
      if (st.init && st.pat.kind === 'binding') {
        const t = litTypeOf(st.init, scope);
        if (t !== null) {
          const cur = scope.vars.get(st.pat.name);
          if (cur) {
            cur.litTy = t;
            // v0.2.54 S-15：整型字面量值 + 域（注解/后缀）一并入作用域 ——
            // 后续 a + 1 / a + a 的静态折叠检查与域传播都依赖这两项事实。
            if (t === 'int') {
              const v = intValOf(st.init, scope);
              if (v !== null) cur.litVal = v;
              const d = (st.ty && st.ty.kind === 'path' && st.ty.segs.length === 1 && INT_LIMITS[st.ty.segs[0]!])
                ? st.ty.segs[0]!
                : intDomainOf(st.init, scope);
              if (d) cur.dom = d;
            }
          }
        }
      }
      // v0.2.53 S-13：整型注解的字面量域校验。
      // rustc 真机对拍实证：`let x: i8 = 300` 在 check 双端放行、interp 打印 300、
      // emit rust 后 rustc 报 literal out of range —— 跨后端语义漂移（python/js
      // 放行）。静态拦截：注解为 12 种整型之一且 init 为（可带负号的）整数字面量
      // 时，值必须落在注解类型域内。非字面量/运行期动态值不判（BigInt 任意精度
      // 为既定设计，见 guide 已知限制 #48）；显式转换请用 as（S-1 零隐式转换）。
      if (st.ty && st.init) checkIntLiteralRange(st.ty, st.init, diags, file);
      // v0.2.54 S-15（let 层）：注解域 + 可折叠算术 init → 结果域检查。
      // S-13 只判纯字面量；这里下沉到折叠算术（250 + 250 对 u8）——
      // 操作数无域时 binary 层查不到，注解域在 let 处才可见。
      if (st.ty && st.init && st.init.kind === 'binary' && st.ty.kind === 'path' && st.ty.segs.length === 1) {
        const limits = INT_LIMITS[st.ty.segs[0]!];
        if (limits && ARITH_OPS.has(st.init.op)) {
          const v = intValOf(st.init, scope);
          if (v !== null && (v < limits[0] || v > limits[1])) {
            diags.push(err('S-15', `注解域算术溢出：折叠结果 ${v} 超出 ${st.ty.segs[0]} 域 [${limits[0]}, ${limits[1]}]（interp BigInt 静默越域，rust 后端环绕/panic —— 跨后端漂移；显式扩域请用 as）`, st.init.span, file));
          }
        }
      }
      // v0.2.56 S-18（#L-22）：native 值模型断层预警 ——
      // let 注解提及结构体/枚举族 + init 是 native 块 + 体无 $host.make
      // → 运行期必得 foreign 值（字段直通可用，clone/方法/模式派发失效）。
      // 口径：仅判「注解 + 初始化器」同现场（静态可判定、零误报面窄）；
      // fn 返回值经变量中转的场景不判（S-14 v3 的追踪不覆盖运行期值标记）。
      if (st.ty && st.init && st.init.kind === 'native') {
        checkNativeValueModel(st.ty, st.init.body, st.span, enums, diags, file);
      }
      if (st.elseBlock) {
        const elseScope: Scope = { vars: new Map(), used: new Set(), parent: scope };
        checkStmts(st.elseBlock, elseScope, enums, diags, file, inAgentLoop);
      }
      break;
    }
    case 'expr':
      checkExpr(st.expr, scope, enums, diags, file, inAgentLoop);
      break;
    case 'item': {
      const item = st.item;
      if (item.kind === 'fn' && item.fn.body) checkBody(item.fn.body, enums, diags, file, inAgentLoop);
      break;
    }
    default:
      break;
  }
}

type Scope2 = Scope;
void ({} as Scope2);

// ---- v0.2.56 S-18（#L-22）：native 值模型断层预警 ----

/** 类型提及结构体/枚举族名（穿泛型实参/引用/元组；返回首个命中名） */
function typeMentionsStructOrEnum(ty: A.HType, enums: Map<string, string[]>): string | null {
  switch (ty.kind) {
    case 'path': {
      const head = ty.segs[0]!;
      if (knownStructs.has(head) || enums.has(head) || enumAlias.has(head)) return head;
      for (const a of ty.args ?? []) {
        const hit = typeMentionsStructOrEnum(a, enums);
        if (hit) return hit;
      }
      return null;
    }
    case 'ref':
    case 'paren':
      return typeMentionsStructOrEnum(ty.inner, enums);
    case 'tuple':
    case 'array':
    case 'slice': {
      const items = ty.kind === 'tuple' ? ty.items : [ty.kind === 'array' ? ty.elem : ty.elem];
      for (const it of items) {
        const hit = typeMentionsStructOrEnum(it, enums);
        if (hit) return hit;
      }
      return null;
    }
    case 'fnptr': {
      for (const p of ty.params) {
        const hit = typeMentionsStructOrEnum(p, enums);
        if (hit) return hit;
      }
      if (ty.ret) return typeMentionsStructOrEnum(ty.ret, enums);
      return null;
    }
    default:
      return null;
  }
}

/** S-18 判定：注解类型提及 struct/enum 族 + native 体无 $host.make → 警告 */
function checkNativeValueModel(ty: A.HType, nativeBody: string, span: A.Span, enums: Map<string, string[]>, diags: Diag[], file: string): void {
  if (nativeBody.includes('$host.make')) return; // 官方构造通道已使用
  const hit = typeMentionsStructOrEnum(ty, enums);
  if (!hit) return;
  diags.push(
    warn(
      'S-18',
      `native 块产物进入含结构体/枚举族 "${hit}" 的注解绑定，但 native 返回的 plain object 不带运行时 __struct/__enum 标记（foreign 值：字段直通可用，.clone()/方法/模式派发失效，#L-22）。修复取向：native 体内用 $host.make("${hit}", {...}) 构造带标记的合法值，或收窄为 I/O 拍平协议（字符串进、HSL 侧重建）`,
      span,
      file,
    ),
  );
}

// ---- v0.2.53 S-13：整型域字面量校验 ----
const INT_LIMITS: Record<string, [bigint, bigint]> = {
  i8: [-128n, 127n],
  i16: [-32768n, 32767n],
  i32: [-2147483648n, 2147483647n],
  i64: [-9223372036854775808n, 9223372036854775807n],
  i128: [-(2n ** 127n), 2n ** 127n - 1n],
  isize: [-9223372036854775808n, 9223372036854775807n],
  u8: [0n, 255n],
  u16: [0n, 65535n],
  u32: [0n, 4294967295n],
  u64: [0n, 18446744073709551615n],
  u128: [0n, 2n ** 128n - 1n],
  usize: [0n, 18446744073709551615n],
};

function checkIntLiteralRange(ty: A.HType, init: A.Expr, diags: Diag[], file: string): void {
  if (ty.kind !== 'path' || ty.segs.length !== 1) return;
  const limits = INT_LIMITS[ty.segs[0]!];
  if (!limits) return;
  // 展开一元负号（u* 域外的负值同样在此拦截）
  let expr: A.Expr = init;
  let neg = false;
  if (expr.kind === 'unary' && expr.op === '-') { neg = true; expr = expr.operand; }
  if (expr.kind !== 'lit' || expr.lit.t !== 'int') return;
  let v = BigInt(expr.lit.v);
  if (neg) v = -v;
  if (v < limits[0] || v > limits[1]) {
    diags.push(err('S-13', `整数字面量 ${v} 超出 ${ty.segs[0]} 域 [${limits[0]}, ${limits[1]}]（rustc 后端将拒绝编译，python/js 后端静默放行 —— 跨后端漂移；显式截断请用 as）`, init.span, file));
  }
}

// ---- v0.2.53 G-8：守卫指纹（递归序列化，足够判等） ----
function patternFingerprint(p: A.Pattern): string {
  switch (p.kind) {
    case 'wildcard': return '_';
    case 'literal': return `lit:${typeof p.value === 'bigint' ? p.value.toString() : JSON.stringify(p.value)}`;
    case 'binding': return `bind:${p.name}${p.sub ? ':' + patternFingerprint(p.sub) : ''}`;
    case 'path': return `path:${p.segs.join('::')}${p.sub ? ':' + patternFingerprint(p.sub) : ''}`;
    case 'tuple': return `tup:[${p.items.map(patternFingerprint).join(',')}]${p.rest ? '+rest' : ''}`;
    case 'struct': return `st:${p.segs.join('::')}{${p.fields.map((f) => `${f.name}=${patternFingerprint(f.pat)}`).join(',')}}${p.rest ? '+rest' : ''}`;
    case 'or': return `or(${p.alts.map(patternFingerprint).join('|')})`;
    case 'range': return `rng:${patternFingerprint(p.lo)}..${p.inclusive ? '=' : ''}${patternFingerprint(p.hi)}`;
    default: return 'other';
  }
}

function patternDisplay(p: A.Pattern): string {
  if (p.kind === 'path') return p.segs.join('::');
  return p.kind;
}

// ---- v0.2.53 S-14：二元运算符保守静态类型检查 ----
// 表达式的静态可判类型（字面量域；未知 → null 不判）
type LitTy = 'int' | 'float' | 'bool' | 'str' | 'char' | null;

// ---- v0.2.54 S-15：注解域整型算术的静态溢出检查 ----
// L-9 证据链（四运行时真机对拍）：
//   let a: i64 = 9223372036854775807; let b = a + 1; →
//     interp:     9223372036854775808（BigInt 任意精度，静默越域不环绕）
//     rust(release): 环绕 -9223372036854775808（debug: panic）
//     python:     9223372036854775808（任意精度，与 interp 一致）
//     js/ts:      字面量读入即舍入（Number 精度城外）
//   let a: u8 = 250; a + a → interp 500（越域）/ rust 环绕 244
// 域语义只能静态守门（interp 参考语义 = 任意精度不环绕；rust 后端环绕为投射差异）。
// 口径：仅当「两侧静态可折叠整数值 + 至少一侧域已知」或「let 注解域 + 可折叠算术 init」
// 才判 —— 零误报优先，动态值留运行期。

// 整型域事实来源：显式 cast 目标 / 带后缀字面量 / 作用域声明（注解或后缀）
function intDomainOf(e: A.Expr, scope?: Scope): string | null {
  if (e.kind === 'cast' && e.ty.kind === 'path' && e.ty.segs.length === 1) {
    const t = e.ty.segs[0]!;
    if (INT_LIMITS[t]) return t;
  }
  if (e.kind === 'lit' && e.lit.t === 'int' && e.lit.suffix && INT_LIMITS[e.lit.suffix]) return e.lit.suffix;
  if (e.kind === 'path' && e.segs.length === 1 && scope) {
    let s: Scope | undefined = scope;
    while (s) {
      const hit = s.vars.get(e.segs[0]!);
      if (hit) return hit.dom ?? null;
      s = s.parent;
    }
    return null;
  }
  return null;
}

// 静态可折叠整数值（BigInt 任意精度）：lit / 一元负 / path 查作用域 litVal / 二元折叠
function intValOf(e: A.Expr, scope?: Scope): bigint | null {
  if (e.kind === 'lit' && e.lit.t === 'int') return BigInt(e.lit.v);
  if (e.kind === 'unary' && e.op === '-') {
    const v = intValOf(e.operand, scope);
    return v === null ? null : -v;
  }
  if (e.kind === 'unary' && e.op === '+') return intValOf(e.operand, scope);
  if (e.kind === 'path' && e.segs.length === 1 && scope) {
    let s: Scope | undefined = scope;
    while (s) {
      const hit = s.vars.get(e.segs[0]!);
      if (hit) return hit.litVal ?? null;
      s = s.parent;
    }
    return null;
  }
  // v0.2.56 S-17：cast 域折叠（truncation-aware）—— intValOf 此前不穿 cast，
  // `300 as u8 + 300`（折叠后 344 越域）静态漏报。cast 到整型域 = 显式截断
  // 投射（interp castValue 环绕语义同构，BigInt 精确）；cast 到 float/其他
  // = 离开整数值域 → 不折叠。
  if (e.kind === 'cast' && e.ty.kind === 'path' && e.ty.segs.length === 1) {
    const t = e.ty.segs[0]!;
    const lim = INT_LIMITS[t];
    if (!lim) return null; // f32/f64/String/bool/char → 非整数域
    const v = intValOf(e.expr, scope);
    if (v === null) return null;
    const signed = t.startsWith('i');
    const span = lim[1] - lim[0] + 1n; // 2^N
    const mod = ((v % span) + span) % span; // [0, 2^N)
    return signed && mod > lim[1] ? mod - span : mod;
  }
  if (e.kind === 'binary' && (e.op === '+' || e.op === '-' || e.op === '*' || e.op === '/' || e.op === '%')) {
    const l = intValOf(e.lhs, scope);
    const r = intValOf(e.rhs, scope);
    if (l === null || r === null) return null;
    if ((e.op === '/' || e.op === '%') && r === 0n) return null; // 除零不折叠（静态另有专项诊断）
    switch (e.op) {
      case '+': return l + r;
      case '-': return l - r;
      case '*': return l * r;
      case '/': return l / r;
      case '%': return l % r;
    }
  }
  return null;
}

const ARITH_OPS = new Set(['+', '-', '*', '/', '%']);

function checkDomainArith(e: A.Expr & { kind: 'binary' }, scope: Scope, diags: Diag[], file: string): void {
  // S-15 主检查：静态可折叠 + 域已知 → 结果域检查；除零专项
  const lv = intValOf(e.lhs, scope);
  const rv = intValOf(e.rhs, scope);
  if (lv === null || rv === null) return;
  if ((e.op === '/' || e.op === '%') && rv === 0n) {
    diags.push(err('S-15', `静态可证除零：${lv} ${e.op} 0（interp 运行期 HRuntimeError，rustc 后端 deny(unconditional_panic) 编译期拒绝；python ZeroDivisionError，js 静默 NaN）`, e.span, file));
    return;
  }
  const dom = intDomainOf(e.lhs, scope) ?? intDomainOf(e.rhs, scope);
  if (!dom) return;
  const limits = INT_LIMITS[dom]!;
  let res: bigint;
  switch (e.op) {
    case '+': res = lv + rv; break;
    case '-': res = lv - rv; break;
    case '*': res = lv * rv; break;
    case '/': res = lv / rv; break;
    default: res = lv % rv; break;
  }
  if (res < limits[0] || res > limits[1]) {
    diags.push(err('S-15', `注解域算术溢出：${lv} ${e.op} ${rv} = ${res} 超出 ${dom} 域 [${limits[0]}, ${limits[1]}]（interp BigInt 任意精度静默越域、rust 后端环绕/panic —— 跨后端漂移；显式扩域请用 as）`, e.span, file));
  }
}

function litTypeOf(e: A.Expr, scope?: Scope): LitTy {
  if (e.kind === 'lit') {
    if (e.lit.t === 'int') return 'int';
    if (e.lit.t === 'float') return 'float';
    if (e.lit.t === 'bool') return 'bool';
    if (e.lit.t === 'str') return 'str';
    if (e.lit.t === 'char') return 'char';
    return null;
  }
  if (e.kind === 'unary' && (e.op === '-' || e.op === '+')) return litTypeOf(e.operand, scope);
  // v0.2.53 S-14（v2）：path 引用沿作用域查声明处记入的 litTy
  // （let s = "abc" 后，s 的静态类型事实 = str）
  if (e.kind === 'path' && e.segs.length === 1 && scope) {
    let s: Scope | undefined = scope;
    while (s) {
      const hit = s.vars.get(e.segs[0]!);
      if (hit) return hit.litTy ?? null;
      s = s.parent;
    }
    return null;
  }
  if (e.kind === 'cast' && e.ty.kind === 'path' && e.ty.segs.length === 1) {
    // 显式 cast 的目标类型可作为静态事实（as i64 / as f64 / as bool / as String）
    const t = e.ty.segs[0]!;
    if (t === 'i8' || t === 'i16' || t === 'i32' || t === 'i64' || t === 'i128' || t === 'isize' || t === 'u8' || t === 'u16' || t === 'u32' || t === 'u64' || t === 'u128' || t === 'usize') return 'int';
    if (t === 'f32' || t === 'f64') return 'float';
    if (t === 'bool') return 'bool';
    if (t === 'String' || t === 'str') return 'str';
    if (t === 'char') return 'char';
  }
  return null; // path / call / 方法链等动态值 → 不判
}

const NUMERIC_TYS = new Set<LitTy>(['int', 'float']);

function checkBinaryOpTypes(e: A.Expr & { kind: 'binary' }, scope: Scope, diags: Diag[], file: string): void {
  const lhs = litTypeOf(e.lhs, scope);
  const rhs = litTypeOf(e.rhs, scope);
  if (lhs === null || rhs === null) return; // 静态不可判 → 保守放行
  const op = e.op;
  const at = `${lhs} ${op} ${rhs}`;
  const fail = (note: string): void => {
    diags.push(err('S-14', `二元运算类型不匹配：${at}（${note}；rustc 后端编译期拒绝，python/typescript 后端静默放行或产生垃圾值 —— 跨后端漂移）`, e.span, file));
  };
  // 字符串拼接：+ 且两侧同为 str 合法（Rust/JS/Python 一致）；str 与数值 + 非法
  if (op === '+') {
    if (lhs === 'str' && rhs === 'str') return;
    if ((lhs === 'str') !== (rhs === 'str')) { fail('str 与数值相加：仅 str+str 拼接与数值加法合法'); return; }
  }
  if (op === '*' || op === '-' || op === '/' || op === '%') {
    // 算术：仅数值域（str*int 是 python 的字符串重复但非 HSL 语义 —— interp 运行期拒绝，静态提前拦）
    if (!NUMERIC_TYS.has(lhs) || !NUMERIC_TYS.has(rhs)) { fail('算术运算符要求两侧数值（str 重复/拼接请用显式转换或 str 方法）'); return; }
    if ((lhs === 'float') !== (rhs === 'float')) { fail('int 与 float 混算需显式 as 转换（S1 零隐式转换）'); return; }
    // v0.2.54 S-15：注解域整型算术的静态溢出/除零检查（两侧静态可折叠时）
    if (lhs === 'int' && rhs === 'int') checkDomainArith(e, scope, diags, file);
    return;
  }
  if (op === '+' ) {
    if (!NUMERIC_TYS.has(lhs) || !NUMERIC_TYS.has(rhs)) { fail('加法仅数值加法或 str+str 拼接'); return; }
    if ((lhs === 'float') !== (rhs === 'float')) { fail('int 与 float 混算需显式 as 转换（S1 零隐式转换）'); return; }
    // v0.2.54 S-15：同上
    if (lhs === 'int' && rhs === 'int') checkDomainArith(e, scope, diags, file);
    return;
  }
  // 比较运算：==/!=/</>/<=/>= —— 数值×数值 / str×str / bool×bool / char×char 合法；跨类非法
  if (op === '==' || op === '!=' || op === '<' || op === '>' || op === '<=' || op === '>=') {
    if (lhs !== rhs) { fail('比较运算两侧类型不同（数值×字符串等跨类比较在 rustc 拒绝，python/typescript 静默给出错误结果）'); return; }
    return;
  }
  // 逻辑运算：&&/|| 仅 bool（int&&bool 等在 rustc 非法，js 静默真值化）
  if (op === '&&' || op === '||') {
    if (lhs !== 'bool' || rhs !== 'bool') { fail('逻辑运算符要求两侧 bool（js 后端静默真值化是语义漂移源）'); return; }
    return;
  }
}

function checkExpr(e: A.Expr, scope: Scope, enums: Map<string, string[]>, diags: Diag[], file: string, inAgentLoop: boolean): void {
  switch (e.kind) {
    case 'lit': {
      // v0.2.54 S-13（v2）：后缀字面量域检查（300u8 —— 注解路径 S-13 已有，
      // 后缀路径此前漏拦；lexer 记 suffix，域界 INT_LIMITS）。
      if (e.lit.t === 'int' && e.lit.suffix && INT_LIMITS[e.lit.suffix]) {
        const limits = INT_LIMITS[e.lit.suffix]!;
        const v = BigInt(e.lit.v);
        if (v < limits[0] || v > limits[1]) {
          diags.push(err('S-13', `整数字面量 ${v} 超出后缀 ${e.lit.suffix} 域 [${limits[0]}, ${limits[1]}]（rustc 后端将拒绝编译，python/js 后端静默放行 —— 跨后端漂移；显式截断请用 as）`, e.span, file));
        }
      }
      // v0.2.54 S-16（L-10）：超 i128 容量字面量静态拒绝 —— dhv(Rust) parser
      // 此前静默归零（值损坏比溢出更糟）双端分歧实录；静态域分析以 i128 为
      // 容量上界（与 dhv walk_expr Literal 臂同口径）。
      if (e.lit.t === 'int') {
        const v = BigInt(e.lit.v);
        if (v > 170141183460469231731687303715884105727n || v < -170141183460469231731687303715884105728n) {
          diags.push(err('S-16', `整数字面量超出 i128 静态容量：${e.lit.v}（dhv(Rust) parse 静默归零为 0 —— 值损坏；interp BigInt 精确解析、rust 后端 i128 无域 —— 静态域分析以 i128 为容量上界，源字面量必须可精确表示）`, e.span, file));
        }
      }
      break;
    }
    case 'path':
      for (const s of e.segs) {
        // 只有首段是变量名（后续是枚举/结构体命名空间）
        markUsed(scope, s);
      }
      break;
    case 'binary':
      checkExpr(e.lhs, scope, enums, diags, file, inAgentLoop);
      checkExpr(e.rhs, scope, enums, diags, file, inAgentLoop);
      // v0.2.53 S-14：二元运算符的保守静态类型检查（字面量域）。
      // 三后端真机对拍实证（L-8）：`"abc" * 3` check 双端放行后 ——
      // interp 运行期报错 / rustc 编译期拒绝 / python 打印 abcabcabc /
      // bun 打印 NaN（静默垃圾值）。NaN 是最坏结局（静默污染数据流）。
      // 保守口径：只判「两侧都能静态判为字面量类型」的表达式（lit /
      // 一元负号包裹的 lit / 显式 cast 的目标类型）——未知类型不判，
      // 零误报优先；动态值留给运行期（S1 零隐式转换的运行期镜像）。
      checkBinaryOpTypes(e, scope, diags, file);
      break;
    case 'unary':
      checkExpr(e.operand, scope, enums, diags, file, inAgentLoop);
      break;
    case 'assign': {
      // S-4：目标可变性
      if (e.target.kind === 'path' && e.target.segs.length === 1) {
        const name = e.target.segs[0]!;
        markUsed(scope, name);
        const mut = lookupMut(scope, name);
        if (mut === false) {
          diags.push(err('S-4', `对不可变绑定 "${name}" 赋值（请用 let mut）`, e.span, file));
        }
      } else if (e.target.kind === 'field' || e.target.kind === 'index') {
        checkExpr(e.target, scope, enums, diags, file, inAgentLoop);
      }
      checkExpr(e.value, scope, enums, diags, file, inAgentLoop);
      // v0.2.54 S-14（v3）：重赋值更新字面量事实 —— 消除「先 int 后 str 重赋值」
      // 中转的假阴性（e04 实录：let mut x = 1; x = "s"; x * 2 静态漏拦）。
      // 字面量/可折叠赋值 → 记新事实（含域）；非字面量 → 清除（保守放行，
      // 运行期仍守门）。注解变量的 dom 跟随变量类型不随值变；无注解则域
      // 事实来自字面量后缀/cast，重赋值时随之更新。
      if (e.target.kind === 'path' && e.target.segs.length === 1) {
        const name = e.target.segs[0]!;
        let s: Scope | undefined = scope;
        while (s) {
          const cur = s.vars.get(name);
          if (cur) {
            if (e.op === '=') {
              const t = litTypeOf(e.value, scope);
              cur.litTy = t ?? undefined;
              if (t === 'int') {
                cur.litVal = intValOf(e.value, scope) ?? undefined;
                const d = intDomainOf(e.value, scope) ?? cur.dom ?? undefined;
                cur.dom = d;
                // v0.2.54 S-15：赋值域检查（let mut x: u8 = 0; x = 300 ——
                // S-13 只判 let 声明，赋值路径此前漏拦）
                if (cur.litVal !== undefined && d) {
                  const limits = INT_LIMITS[d];
                  if (limits && (cur.litVal < limits[0] || cur.litVal > limits[1])) {
                    diags.push(err('S-15', `赋值域越界：${cur.litVal} 超出 ${d} 域 [${limits[0]}, ${limits[1]}]（interp BigInt 静默越域，rust 后端字面量编译期拒绝 —— 跨后端漂移；显式截断请用 as）`, e.span, file));
                  }
                }
              } else {
                cur.litVal = undefined;
                cur.dom = undefined;
              }
            } else if (ARITH_OPS.has(e.op.slice(0, -1))) {
              // 复合赋值（+= 等）：折叠更新 litVal + 域检查（a: u8 = 250; a += 10）
              const binOp = e.op.slice(0, -1);
              const base = cur.litVal;
              const rv = intValOf(e.value, scope);
              if (base !== undefined && rv !== null) {
                if ((binOp === '/' || binOp === '%') && rv === 0n) {
                  diags.push(err('S-15', `静态可证除零：${base} ${binOp}= 0（interp 运行期 HRuntimeError，rustc 后端编译期拒绝；js 静默 NaN）`, e.span, file));
                } else {
                  let res: bigint | null = null;
                  switch (binOp) {
                    case '+': res = base + rv; break;
                    case '-': res = base - rv; break;
                    case '*': res = base * rv; break;
                    case '/': res = base / rv; break;
                    case '%': res = base % rv; break;
                  }
                  if (res !== null) {
                    cur.litVal = res;
                    if (cur.dom) {
                      const limits = INT_LIMITS[cur.dom]!;
                      if (res < limits[0] || res > limits[1]) {
                        diags.push(err('S-15', `注解域算术溢出：${base} ${e.op} ${rv} = ${res} 超出 ${cur.dom} 域 [${limits[0]}, ${limits[1]}]（interp BigInt 静默越域，rust 后端环绕/panic —— 跨后端漂移）`, e.span, file));
                      }
                    }
                  }
                }
              } else {
                // 动态值复合赋值 → 折叠值事实失效保守清除；类型事实按 RHS
                // 字面量类型对齐（a += <dyn数值> 保持 int；非数值 RHS 清除）
                cur.litVal = undefined;
                const vt = litTypeOf(e.value, scope);
                if (vt !== 'int' && vt !== null) cur.litTy = vt;
              }
            }
            break;
          }
          s = s.parent;
        }
      }
      break;
    }
    case 'call': {
      // v0.2.51 E-2：单段路径调用的符号解析 —— 未定义/未 import 的函数名
      // 此前 check 静默通过、run 才报“不是可调用项”。保守边界：
      //  · 两段路径（Type::variant / Enum::ctor）不查 —— 命名空间成员形状复杂
      //  · 局部作用域有同名绑定（闭包值/参数/match 绑定）不查
      if (e.callee.kind === 'path' && e.callee.segs.length === 1) {
        const name = e.callee.segs[0]!;
        if (!CALL_WHITELIST.has(name) && !visibleCallables.has(name) && !lookupLocal(scope, name)) {
          diags.push(err('E-2', `调用了未定义的函数 "${name}"（未定义、未 import，或拼写错误）`, e.span, file));
        }
      }
      checkExpr(e.callee, scope, enums, diags, file, inAgentLoop);
      for (const a of e.args) checkExpr(a, scope, enums, diags, file, inAgentLoop);
      break;
    }
    case 'method': {
      checkExpr(e.recv, scope, enums, diags, file, inAgentLoop);
      for (const a of e.args) checkExpr(a, scope, enums, diags, file, inAgentLoop);
      // S-2：裸 unwrap
      if (e.name === 'unwrap' && e.args.length === 0) {
        diags.push(warn('S-2', '裸 .unwrap()：非空默认建议使用 unwrap_or / match / ?（S2）', e.span, file));
      }
      break;
    }
    case 'field':
      checkExpr(e.recv, scope, enums, diags, file, inAgentLoop);
      break;
    case 'index':
      checkExpr(e.recv, scope, enums, diags, file, inAgentLoop);
      checkExpr(e.index, scope, enums, diags, file, inAgentLoop);
      break;
    case 'slice':
      checkExpr(e.recv, scope, enums, diags, file, inAgentLoop);
      if (e.lo) checkExpr(e.lo, scope, enums, diags, file, inAgentLoop);
      if (e.hi) checkExpr(e.hi, scope, enums, diags, file, inAgentLoop);
      break;
    case 'try':
      checkExpr(e.expr, scope, enums, diags, file, inAgentLoop);
      break;
    case 'await':
      checkExpr(e.expr, scope, enums, diags, file, inAgentLoop);
      break;
    case 'cast':
      checkExpr(e.expr, scope, enums, diags, file, inAgentLoop);
      break;
    case 'tuple':
    case 'array':
      for (const it of e.items) checkExpr(it, scope, enums, diags, file, inAgentLoop);
      break;
    case 'arrayrep':
      checkExpr(e.value, scope, enums, diags, file, inAgentLoop);
      checkExpr(e.count, scope, enums, diags, file, inAgentLoop);
      break;
    case 'struct': {
      for (const f of e.fields) {
        if (f.value) checkExpr(f.value, scope, enums, diags, file, inAgentLoop);
        else if (f.base) checkExpr(f.base, scope, enums, diags, file, inAgentLoop);
        else markUsed(scope, f.name); // 简写
      }
      break;
    }
    case 'closure': {
      const child: Scope = { vars: new Map(), used: new Set(), parent: scope };
      for (const p of e.params) for (const n of patternNames(p.pat)) declareVar(child, n, false, diags, e.span, file);
      checkExpr(e.body, child, enums, diags, file, inAgentLoop);
      break;
    }
    case 'if':
      checkExpr(e.cond, scope, enums, diags, file, inAgentLoop);
      checkExpr(e.then, scope, enums, diags, file, inAgentLoop);
      if (e.els) checkExpr(e.els, scope, enums, diags, file, inAgentLoop);
      break;
    case 'iflet': {
      checkExpr(e.expr, scope, enums, diags, file, inAgentLoop);
      const child: Scope = { vars: new Map(), used: new Set(), parent: scope };
      for (const n of patternNames(e.pat)) declareVar(child, n, false, diags, e.span, file);
      checkExpr(e.then, child, enums, diags, file, inAgentLoop);
      if (e.els) checkExpr(e.els, scope, enums, diags, file, inAgentLoop);
      break;
    }
    case 'match': {
      checkExpr(e.expr, scope, enums, diags, file, inAgentLoop);
      // S-6：穷尽性
      checkExhaustiveness(e, enums, diags, file, inAgentLoop);
      for (const arm of e.arms) {
        const child: Scope = { vars: new Map(), used: new Set(), parent: scope };
        for (const n of patternNames(arm.pattern)) declareVar(child, n, false, diags, arm.span, file);
        if (arm.guard) checkExpr(arm.guard, child, enums, diags, file, inAgentLoop);
        checkExpr(arm.body, child, enums, diags, file, inAgentLoop);
      }
      break;
    }
    case 'block': {
      const child: Scope = { vars: new Map(), used: new Set(), parent: scope };
      checkStmts(e.stmts, child, enums, diags, file, inAgentLoop);
      break;
    }
    case 'asyncblock': {
      const child: Scope = { vars: new Map(), used: new Set(), parent: scope };
      checkStmts(e.stmts, child, enums, diags, file, inAgentLoop);
      break;
    }
    case 'loop': {
      const child: Scope = { vars: new Map(), used: new Set(), parent: scope };
      checkStmts((e.body as A.Expr & { kind: 'block' }).stmts ?? [], child, enums, diags, file, inAgentLoop);
      break;
    }
    case 'while':
      checkExpr(e.cond, scope, enums, diags, file, inAgentLoop);
      checkExpr(e.body, scope, enums, diags, file, inAgentLoop);
      break;
    case 'whilelet':
      checkExpr(e.expr, scope, enums, diags, file, inAgentLoop);
      checkExpr(e.body, scope, enums, diags, file, inAgentLoop);
      break;
    case 'for':
      checkExpr(e.iter, scope, enums, diags, file, inAgentLoop);
      checkExpr(e.body, scope, enums, diags, file, inAgentLoop);
      break;
    case 'break':
      if (e.value) checkExpr(e.value, scope, enums, diags, file, inAgentLoop);
      break;
    case 'return':
      if (e.value) checkExpr(e.value, scope, enums, diags, file, inAgentLoop);
      break;
    case 'macro':
      for (const sub of macroExprs(e.tree)) {
        checkExpr(sub, scope, enums, diags, file, inAgentLoop);
      }
      break;
    case 'range':
      if (e.lo) checkExpr(e.lo, scope, enums, diags, file, inAgentLoop);
      if (e.hi) checkExpr(e.hi, scope, enums, diags, file, inAgentLoop);
      break;
    case 'native': {
      // N-1：语言标识
      if (!NATIVE_LANGS.has(e.lang)) {
        diags.push(err('N-1', `native 语言 "${e.lang}" 未注册（已注册：rust/python/typescript/...）`, e.span, file));
      }
      // N-1 捕获语义：native 体按名词法捕获外层变量 —— 标记使用（防 S-7 误报）
      for (const m of e.body.matchAll(/[A-Za-z_$][A-Za-z0-9_$]*/g)) {
        markUsed(scope, m[0]!);
      }
      break;
    }
    default:
      break;
  }
}

function lookupMut(scope: Scope, name: string): boolean | undefined {
  if (scope.vars.has(name)) return scope.vars.get(name)!.mut;
  return scope.parent ? lookupMut(scope.parent, name) : undefined;
}

/** E-2 辅助：名字是否是当前（或祖先）作用域的局部绑定 */
function lookupLocal(scope: Scope, name: string): boolean {
  if (scope.vars.has(name)) return true;
  return scope.parent ? lookupLocal(scope.parent, name) : false;
}

function checkExhaustiveness(e: A.Expr & { kind: 'match' }, enums: Map<string, string[]>, diags: Diag[], file: string, inAgentLoop: boolean): void {
  // 收集 arm 的枚举/变体信息
  const enumArms = new Map<string, Set<string>>(); // enumName -> variants covered
  let hasWildcard = false;
  let totalEnumArms = 0;
  for (const arm of e.arms) {
    const infos = pathPatternInfos(arm.pattern);
    if (infos.length === 0) {
      // `_` 解析为 binding(name:'_')（Rust 语义：通配兜底 = 穷尽）
      if (arm.pattern.kind === 'wildcard'
        || (arm.pattern.kind === 'binding' && arm.pattern.name === '_' && !arm.pattern.sub)) {
        hasWildcard = true;
      }
      continue;
    }
    totalEnumArms++;
    for (const info of infos) {
      // L-2：别名首段解析回枚举原名（S-6 对别名臂生效）
      const resolved = (info.enumName && enumAlias.get(info.enumName)) || info.enumName;
      if (resolved && enums.has(resolved)) {
        if (!enumArms.has(resolved)) enumArms.set(resolved, new Set());
        enumArms.get(resolved)!.add(info.variant);
      }
    }
  }
  if (totalEnumArms === 0) return;
  // AgentLoop 中的 Action match 禁止 _ 兜底（S-6 铁律：编译期直面新分支）
  if (hasWildcard && inAgentLoop && enumArms.size > 0) {
    diags.push(err('S-6', `graph AgentLoop 内的枚举 match 不允许 _ 通配兜底（必须显式穷尽，直面新分支）`, e.span, file));
  }
  for (const [enumName, covered] of enumArms) {
    const all = enums.get(enumName) ?? [];
    const missing = all.filter((v) => !covered.has(v));
    if (missing.length > 0 && !hasWildcard) {
      diags.push(err('S-6', `match 不穷尽：${enumName} 缺少变体 ${missing.join(', ')}`, e.span, file));
    }
  }
}

interface PatInfo {
  enumName?: string;
  variant: string;
}

function pathPatternInfos(pat: A.Pattern): PatInfo[] {
  switch (pat.kind) {
    case 'path': {
      if (pat.segs.length === 2) return [{ enumName: pat.segs[0], variant: pat.segs[1]! }];
      if (pat.segs.length === 1) {
        const n = pat.segs[0]!;
        if (n === 'Ok' || n === 'Err') return [{ enumName: 'Result', variant: n }];
        if (n === 'Some' || n === 'None') return [{ enumName: 'Option', variant: n }];
        return [];
      }
      return [];
    }
    case 'struct': {
      // Enum::Variant { fields } —— 枚举变体的结构模式（两段路径）
      if (pat.segs.length === 2) return [{ enumName: pat.segs[0], variant: pat.segs[1]! }];
      return [];
    }
    case 'or':
      return pat.alts.flatMap(pathPatternInfos);
    default:
      return [];
  }
}

function patternNames(pat: A.Pattern): string[] {
  const out: string[] = [];
  const walk = (p: A.Pattern): void => {
    switch (p.kind) {
      case 'binding':
        out.push(p.name);
        if (p.sub) walk(p.sub);
        break;
      case 'tuple':
        for (const it of p.items) walk(it);
        break;
      case 'struct':
        for (const f of p.fields) walk(f.pat);
        break;
      case 'path':
        if (p.sub?.kind === 'tuple') for (const it of p.sub.items) walk(it);
        if (p.sub?.kind === 'struct') for (const f of p.sub.fields) walk(f.pat);
        break;
      case 'or':
        for (const a of p.alts) walk(a);
        break;
      default:
        break;
    }
  };
  walk(pat);
  return out;
}

function macroExprs(tree: A.TokenTree): A.Expr[] {
  // 宏参数表达式提取：与解释器同构（treeToTokens + EOF 终止符 + parseExprsFromTokens），
  // 供 S-7 使用标记 / S-1 等规则下探宏实参；解析失败则跳过（宏体不阻塞检查）
  try {
    const toks = treeToTokens(tree);
    if (toks.length === 0) return [];
    toks.push({ kind: 'eof', text: '<eof>', line: 0, col: 0 });
    return parseExprsFromTokens(toks, '<macro>');
  } catch {
    return [];
  }
}

function err(code: string, msg: string, span: A.Span, file: string): Diag {
  return { severity: 'error', code, msg, file, line: span.line, col: span.col };
}
function warn(code: string, msg: string, span: A.Span, file: string): Diag {
  return { severity: 'warning', code, msg, file, line: span.line, col: span.col };
}

export function formatDiags(diags: Diag[]): string {
  const lines: string[] = [];
  for (const d of diags) {
    lines.push(`${d.severity === 'error' ? 'error' : 'warning'}[${d.code}]: ${d.msg}`);
    lines.push(`  --> ${d.file}:${d.line}:${d.col}`);
  }
  const errors = diags.filter((d) => d.severity === 'error').length;
  const warnings = diags.filter((d) => d.severity === 'warning').length;
  lines.push('');
  lines.push(`dhv-ts check: ${errors} error(s), ${warnings} warning(s)`);
  return lines.join('\n');
}
