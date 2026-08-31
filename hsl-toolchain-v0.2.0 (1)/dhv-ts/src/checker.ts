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
import { isStdPath } from './std';

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

export function checkProgram(program: LoadedProgram): Diag[] {
  const diags: Diag[] = [];
  const enums = new Map<string, string[]>(); // name -> variants
  for (const [, ast] of program.files) {
    for (const item of ast.items) {
      if (item.kind === 'enum') enums.set(item.name, item.variants.map((v) => v.name));
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
function spanOf(item: A.Item): A.Span {
  return (item as { span: A.Span }).span;
}

function checkItem(item: A.Item, enums: Map<string, string[]>, diags: Diag[], file: string): void {
  if (item.kind === 'fn' && item.fn.body) {
    checkBody(item.fn.body, enums, diags, file, undefined);
  }
  if (item.kind === 'graph') {
    checkGraph(item.graph, enums, diags, file);
  }
  if (item.kind === 'impl') {
    for (const m of item.methods) {
      if (m.body) checkBody(m.body, enums, diags, file, undefined);
    }
  }
  if (item.kind === 'trait') {
    for (const ti of item.items) {
      if (ti.fn?.body) checkBody(ti.fn.body, enums, diags, file, undefined);
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
  // graph 体内的 match/赋值等检查（含 AgentLoop 内 _ 检查）
  const gscope: Scope = { vars: new Map(), used: new Set() };
  for (const gs of g.body) {
    if (gs.t === 'stmt') {
      // let 声明由 checkStmt 内部完成（此处不再重复 declare —— 否则 S-8 误报）
      checkStmt(gs.stmt, gscope, enums, diags, file, true);
    }
    if (gs.t === 'item') checkItem(gs.item, enums, diags, file);
  }
}

// ---- 语句/表达式检查（S2/S4/S6/S7/S8 + N1）----
interface Scope {
  vars: Map<string, { mut: boolean; span: A.Span }>;
  parent?: Scope;
  used: Set<string>;
}

function checkBody(stmts: A.Stmt[], enums: Map<string, string[]>, diags: Diag[], file: string, inAgentLoop?: boolean): void {
  
  const scope: Scope = { vars: new Map(), used: new Set() };
  checkStmts(stmts, scope, enums, diags, file, inAgentLoop ?? false);
}

function checkStmts(stmts: A.Stmt[], scope: Scope, enums: Map<string, string[]>, diags: Diag[], file: string, inAgentLoop: boolean): void {
  for (const st of stmts) checkStmt(st, scope, enums, diags, file, inAgentLoop);
  // S-7：未使用绑定（本层声明且本层及子层未用）
  if (process.env.HSL_CHECK_DEBUG) console.error('[S7-loop] vars:', [...scope.vars.keys()], 'used:', [...scope.used]);
  for (const [name, info] of scope.vars) {
    if (name.startsWith('_')) continue;
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

function checkExpr(e: A.Expr, scope: Scope, enums: Map<string, string[]>, diags: Diag[], file: string, inAgentLoop: boolean): void {
  switch (e.kind) {
    case 'path':
      for (const s of e.segs) {
        // 只有首段是变量名（后续是枚举/结构体命名空间）
        markUsed(scope, s);
      }
      break;
    case 'binary':
      checkExpr(e.lhs, scope, enums, diags, file, inAgentLoop);
      checkExpr(e.rhs, scope, enums, diags, file, inAgentLoop);
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
      break;
    }
    case 'call':
      checkExpr(e.callee, scope, enums, diags, file, inAgentLoop);
      for (const a of e.args) checkExpr(a, scope, enums, diags, file, inAgentLoop);
      break;
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
      if (info.enumName && enums.has(info.enumName)) {
        if (!enumArms.has(info.enumName)) enumArms.set(info.enumName, new Set());
        enumArms.get(info.enumName)!.add(info.variant);
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
