// ============================================================================
// dhv-ts/src/backends/emit.ts — 投射编排器（总纲 §4/§5 的运行级实现）
// ----------------------------------------------------------------------------
// 遍历全程序 project{} 声明 → 按 (物理路径, 语言) 聚合逻辑项 → 调 decls 打印器
// 生成代码 / interp 渲染静态资源 → 写盘 + manifest.json + 交叉语法校验（Lint 第 2 层）
// ============================================================================

import * as fs from 'node:fs';
import * as path from 'node:path';
import * as A from '../ast';
import { LoadedProgram } from '../linker';
import { Interp } from '../interp';
import { getLang, isStaticLangId, codegenTier } from './registry';
import { emitFile, emitTypeDeclOnly, ProjectedItem, EmitCtx, javaWrapperOf } from './decls';
import { validateGeneratedFile } from './validate';

// ---------------------------------------------------------------------------
// 跨文件类型依赖解析（总纲 §4 物理层：投射产物之间的类型可见性）
// ---------------------------------------------------------------------------
// 机制：遍历每个投射项的 AST，收集引用的用户类型名；若该类型被投射到同语言的
// 另一个物理文件，则按目标语言的导入机制接线：
//   python  → from <stem> import A, B          （同目录平铺导入）
//   ts/js   → import { A, B } from './<rel>'   （相对路径）
//   rust    → use crate::<stem>::{A, B};       （模块组装约定）
//   go      → 无需导入（全部产物同包 hsl）
//   cpp     → 内联类型声明（ODR 兼容：与源文件定义逐字一致，多 TU 安全）
// 其余（contract 级）不接线 —— 函数体本就是围栏，签名类型名由契约纪律保障。

/** 走访一个项，收集其引用的全部用户类型名（类型路径根 + 多段路径表达式首段） */
function collectTypeRefs(item: A.Item, out: Set<string>): void {
  const walkType = (t?: A.HType): void => {
    if (!t) return;
    switch (t.kind) {
      case 'path':
        out.add(t.segs[0]!);
        for (const a of t.args ?? []) walkType(a);
        break;
      case 'ref': case 'paren':
        walkType(t.inner);
        break;
      case 'tuple':
        for (const i of t.items) walkType(i);
        break;
      case 'array': case 'slice':
        walkType(t.elem);
        break;
      case 'fnptr':
        for (const p of t.params) walkType(p);
        walkType(t.ret);
        break;
      case 'dyn': case 'implt':
        for (const b of t.bounds) out.add(b);
        break;
      default: break;
    }
  };
  const walkExpr = (e?: A.Expr): void => {
    if (!e) return;
    switch (e.kind) {
      case 'path':
        if (e.segs.length >= 2) out.add(e.segs[0]!);
        break;
      case 'binary': walkExpr(e.lhs); walkExpr(e.rhs); break;
      case 'unary': walkExpr(e.operand); break;
      case 'assign': walkExpr(e.target); walkExpr(e.value); break;
      case 'call': walkExpr(e.callee); for (const a of e.args) walkExpr(a); break;
      case 'method': walkExpr(e.recv); for (const a of e.args) walkExpr(a); for (const g of e.generics ?? []) walkType(g); break;
      case 'field': walkExpr(e.recv); break;
      case 'index': walkExpr(e.recv); walkExpr(e.index); break;
      case 'slice': walkExpr(e.recv); walkExpr(e.lo); walkExpr(e.hi); break;
      case 'try': case 'await': walkExpr(e.expr); break;
      case 'cast': walkExpr(e.expr); walkType(e.ty); break;
      case 'tuple': for (const i of e.items) walkExpr(i); break;
      case 'array': for (const i of e.items) walkExpr(i); break;
      case 'arrayrep': walkExpr(e.value); walkExpr(e.count); break;
      case 'struct':
        out.add(e.segs[0]!);
        for (const f of e.fields) walkExpr(f.value ?? f.base);
        break;
      case 'closure':
        for (const p of e.params) { walkPat(p.pat); walkType(p.ty); }
        walkType(e.ret);
        walkExpr(e.body);
        break;
      case 'if': walkExpr(e.cond); walkExpr(e.then); walkExpr(e.els); break;
      case 'iflet': walkPat(e.pat); walkExpr(e.expr); walkExpr(e.then); walkExpr(e.els); break;
      case 'match':
        walkExpr(e.expr);
        for (const arm of e.arms) { walkPat(arm.pattern); walkExpr(arm.guard); walkExpr(arm.body); }
        break;
      case 'block': case 'asyncblock':
        for (const s of e.stmts) walkStmt(s);
        break;
      case 'loop': walkExpr(e.body); break;
      case 'while': walkExpr(e.cond); walkExpr(e.body); break;
      case 'whilelet': walkPat(e.pat); walkExpr(e.expr); walkExpr(e.body); break;
      case 'for': walkPat(e.pat); walkExpr(e.iter); walkExpr(e.body); break;
      case 'break': walkExpr(e.value); break;
      case 'return': walkExpr(e.value); break;
      case 'macro':
        // 宏树只收集多段路径 token（如 Action::CallTool）
        walkTokenTree(e.tree, out);
        break;
      case 'native': case 'unit': break;
      default: break;
    }
  };
  const walkTokenTree = (t: A.TokenTree, acc: Set<string>): void => {
    if (t.t === 'tok') return;
    const toks = t.items;
    for (let i = 0; i < toks.length; i++) {
      const cur = toks[i]!;
      if (cur.t === 'tok' && cur.tok.kind === 'ident' && i + 2 < toks.length) {
        const sep = toks[i + 1]!;
        const nxt = toks[i + 2]!;
        if (sep.t === 'tok' && sep.tok.text === '::' && nxt.t === 'tok' && nxt.tok.kind === 'ident') {
          acc.add(cur.tok.text);
        }
      } else if (cur.t === 'delim') {
        walkTokenTree(cur, acc);
      }
    }
  };
  const walkPat = (p?: A.Pattern): void => {
    if (!p) return;
    switch (p.kind) {
      case 'path':
        if (p.segs.length >= 2) out.add(p.segs[0]!);
        break;
      case 'binding': walkPat(p.sub); break;
      case 'tuple': for (const i of p.items) walkPat(i); break;
      case 'struct':
        if (p.segs.length >= 1) out.add(p.segs[0]!);
        for (const f of p.fields) walkPat(f.pat);
        break;
      case 'or': for (const a of p.alts) walkPat(a); break;
      case 'range': walkPat(p.lo); walkPat(p.hi); break;
      default: break;
    }
  };
  const walkFn = (f: A.FnDef): void => {
    for (const p of f.params) { walkPat(p.pat); walkType(p.ty); }
    walkType(f.ret);
    for (const s of f.body ?? []) walkStmt(s);
  };
  const walkStmt = (s: A.Stmt): void => {
    switch (s.kind) {
      case 'let': walkPat(s.pat); walkType(s.ty); walkExpr(s.init); break;
      case 'expr': walkExpr(s.expr); break;
      case 'item': walkItem(s.item); break;
      default: break;
    }
  };
  const walkItem = (it: A.Item): void => {
    switch (it.kind) {
      case 'struct':
        for (const f of it.fields) walkType(f.ty);
        for (const t of it.tupleFields ?? []) walkType(t);
        break;
      case 'enum':
        for (const v of it.variants) {
          if (v.fields) {
            if ('named' in v.fields) for (const f of v.fields.named) walkType(f.ty);
            else for (const t of v.fields.tuple) walkType(t);
          }
        }
        break;
      case 'trait':
        for (const ti of it.items) {
          if (ti.fn) walkFn(ti.fn);
          walkType(ti.ty);
          walkExpr(ti.value);
        }
        break;
      case 'impl':
        if (it.traitSegs?.length) out.add(it.traitSegs[0]!);
        out.add(it.typeName);
        for (const t of it.traitArgs ?? []) walkType(t);
        for (const m of it.methods) walkFn(m);
        break;
      case 'fn': walkFn(it.fn); break;
      case 'const': walkType(it.ty); walkExpr(it.value); break;
      case 'typealias': walkType(it.value); break;
      case 'graph':
        // graph 脚手架（monolith/microkernel）为通用插件注册表/循环骨架 ——
        // 不活体引用类型（类型名只出现在 HSL 镜像注释里），不参与依赖接线
        break;
      default: break;
    }
  };
  walkItem(item);
}

/** posix 相对路径（去扩展名），保证以 ./ 开头 —— ts/js import 专用 */
function tsImportPath(fromFile: string, toFile: string): string {
  const rel = path.posix.normalize(path.posix.relative(path.posix.dirname(fromFile), toFile)).replace(/\.[tj]s$/, '');
  return rel.startsWith('.') ? rel : `./${rel}`;
}

/** rust 模块路径约定：crate::<目录链>::<stem>（要求各段为合法 rust 标识符） */
function rustModulePath(targetFile: string): string | null {
  const parts = targetFile.replace(/\.rs$/, '').split('/').filter(Boolean);
  if (parts.length === 0) return null;
  for (const p of parts) if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(p)) return null;
  return parts.join('::');
}

/**
 * Java / C# 跨文件类型引用告警（Task 20 引入 Java；Task 21 扩展 C#）：
 * 类型已顶层声明（Java 同包裸名互见 / C# 同命名空间裸名互见，无需限定名），
 * 但被引用而未投射到该语言的类型仍是未定义名 —— 诚实 X-1 告警。
 */
function warnTopLevelTypeRefs(
  agg: { lang: string; items: ProjectedItem[] },
  relPath: string,
  typeLoc: Map<string, Map<string, { path: string; pi: ProjectedItem }>>,
  allTypes: Map<string, ProjectedItem>,
  warnings: string[],
): void {
  const refs = new Set<string>();
  for (const pi of agg.items) collectTypeRefs(pi.item, refs);
  const localNames = new Set(agg.items.map((i) => i.name));
  const langTypes = typeLoc.get(agg.lang);
  for (const name of refs) {
    if (localNames.has(name)) continue;
    if (langTypes?.has(name)) continue; // 已投射（可能在他文件，同包/命名空间裸名可见）
    if (allTypes.has(name)) {
      const langLabel = agg.lang === 'java' ? 'Java' : 'C#';
      warnings.push(`X-1：类型 ${name} 被 ${relPath} 引用但未投射到 ${langLabel}（生成物引用未定义名）`);
    }
  }
}

/**
 * 计算一个物理文件的跨文件类型依赖接线行（置于文件头 prelude 之后、项声明之前）。
 * 只对 full/logic 级语言接线；contract 级不接线（围栏纪律）。
 */
function importHeaderForTypeDeps(
  agg: { lang: string; items: ProjectedItem[] },
  relPath: string,
  typeLoc: Map<string, Map<string, { path: string; pi: ProjectedItem }>>,
  allTypes: Map<string, ProjectedItem>,
  ctx: EmitCtx,
  warnings: string[],
): string[] {
  const langId = agg.lang;
  const lang = ctx.lang;
  const tier = codegenTier(lang);
  if (tier === 'contract' || tier === 'static') return [];

  // 收集本文件全部项引用的类型名
  const refs = new Set<string>();
  for (const pi of agg.items) collectTypeRefs(pi.item, refs);

  const localNames = new Set(agg.items.map((i) => i.name));
  const langTypes = typeLoc.get(langId);
  const out: string[] = [];
  const noted = new Set<string>(); // 已处理（含警告）的类型名

  // 源文件分组：targetPath → 类型名列表（排序稳定）
  const bySrc = new Map<string, string[]>();
  for (const name of [...refs].sort()) {
    if (localNames.has(name) || noted.has(name)) continue;
    const loc = langTypes?.get(name);
    if (!loc) {
      // 类型未被投射到该语言：cpp 可从 AST 内联兜底；其余语言诚实告警
      if (langId === 'cpp' && allTypes.has(name)) continue; // 走内联路径
      if (allTypes.has(name) && (langId === 'python' || langId === 'typescript' || langId === 'javascript' || langId === 'rust' || langId === 'go')) {
        warnings.push(`X-1：类型 ${name} 被 ${relPath} 引用但未投射到 ${lang.name}（生成物引用未定义名）`);
        noted.add(name);
      }
      continue;
    }
    if (loc.path === relPath) continue; // 同文件，已本地定义
    if (!bySrc.has(loc.path)) bySrc.set(loc.path, []);
    bySrc.get(loc.path)!.push(name);
    noted.add(name);
  }

  // ---- 按语言生成接线行 ----
  if (bySrc.size === 0 && langId !== 'cpp') return [];

  const srcEntries = [...bySrc.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  switch (langId) {
    case 'python': {
      for (const [src, names] of srcEntries) {
        const stem = path.posix.basename(src).replace(/\.py$/, '');
        if (path.posix.dirname(src) === path.posix.dirname(relPath)) {
          out.push(`from ${stem} import ${names.join(', ')}`);
        } else {
          // 跨目录：python 平铺导入不可达 —— 诚实告警，不生成错误导入
          warnings.push(`X-2：${relPath} 引用 ${names.join(', ')}（位于 ${src}，跨目录 python 导入需手动接线）`);
        }
      }
      break;
    }
    case 'typescript': case 'javascript': {
      for (const [src, names] of srcEntries) {
        out.push(`import { ${names.join(', ')} } from '${tsImportPath(relPath, src)}';`);
      }
      break;
    }
    case 'rust': {
      for (const [src, names] of srcEntries) {
        const mod = rustModulePath(src);
        if (mod) out.push(`use crate::${mod}::{${names.join(', ')}};`);
        else warnings.push(`X-3：${relPath} 引用 ${names.join(', ')}（${src} 不是合法 rust 模块路径，需手动 use 接线）`);
      }
      break;
    }
    case 'go': {
      // 同包（hsl）免导入；跨目录 = 跨包，诚实告警
      for (const [src, names] of srcEntries) {
        if (path.posix.dirname(src) !== path.posix.dirname(relPath)) {
          warnings.push(`X-4：${relPath} 引用 ${names.join(', ')}（位于 ${src}，跨目录 go 包需手动接线）`);
        }
        // 同目录：go 同包类型直接可见，无需任何导入行
      }
      break;
    }
    case 'cpp': {
      // 内联类型声明（ODR 兼容：与被投射文件中的定义逐字一致）
      const inlined = new Set<string>();
      for (const [src, names] of srcEntries) {
        out.push(`// ---- 跨文件类型依赖（投射自 ${src}，ODR 兼容内联）----`);
        for (const n of names) {
          const pi = typeLoc.get('cpp')?.get(n)?.pi;
          if (pi) {
            out.push(...emitTypeDeclOnly(lang, pi, ctx));
            inlined.add(n);
          }
        }
      }
      // 未投射类型兜底：从全程序 AST 内联
      const unproj = [...refs].filter((n) => !localNames.has(n) && !inlined.has(n) && !noted.has(n) && allTypes.has(n));
      if (unproj.length > 0) {
        out.push(`// ---- 跨文件类型依赖（源 ${unproj.map((n) => allTypes.get(n)!.module).filter((v, i, a) => a.indexOf(v) === i).join(', ')}，未单独投射，内联声明）----`);
        for (const n of unproj.sort()) {
          const pi = allTypes.get(n)!;
          out.push(...emitTypeDeclOnly(lang, pi, ctx));
        }
      }
      break;
    }
    default:
      return [];
  }
  if (out.length > 0) out.push('');
  return out;
}

export interface EmittedFile {
  path: string;          // 相对 outDir
  lang: string;
  bytes: number;
  items: string[];
  tier: string;          // full / logic / contract / static（语言能力级）
  // full/logic 语言中实际回退 contract 的块名（诚实边界：tier 是语言能力级，
  // 本字段记录该文件内未活体翻译的项 —— CI 可断言 !contract_fallbacks）
  contract_fallbacks?: string[];
  validation: { ok: boolean; tool: string; detail?: string };
}

export interface EmitReport {
  outDir: string;
  scale: string;
  entry: string;
  files: EmittedFile[];
  warnings: string[];
}

/** 从生成文本中提取实际回退 contract 的块名（各语言未实现标记均含 "dhv: <name> 未翻译"） */
function contractFallbacksOf(text: string): string[] | undefined {
  const names = [...text.matchAll(/dhv: ([A-Za-z_][A-Za-z0-9_]*) 未翻译/g)].map((m) => m[1]!);
  const uniq = [...new Set(names)];
  return uniq.length > 0 ? uniq : undefined;
}

/** 项的 HSL 源码行（按相邻项行界近似切片） */
function buildItemLineMap(ast: A.File): Map<A.Item, string[]> {
  const lines = fs.readFileSync(ast.file, 'utf-8').split('\n');
  const sorted = [...ast.items].filter((i) => i.span?.line).sort((a, b) => a.span.line - b.span.line);
  const map = new Map<A.Item, string[]>();
  sorted.forEach((item, i) => {
    const start = item.span.line - 1;
    const end = i + 1 < sorted.length ? sorted[i + 1]!.span.line - 1 : lines.length;
    const slice = lines.slice(start, Math.max(start + 1, end)).filter((l, idx, arr) =>
      // 去掉尾部空行
      idx < arr.length - 1 || l.trim().length > 0
    );
    // 修剪尾部纯空行（保留中间空行）
    while (slice.length > 1 && slice[slice.length - 1]!.trim() === '') slice.pop();
    map.set(item, slice);
  });
  return map;
}

export async function emitProgram(
  program: LoadedProgram,
  outDir: string,
  opts: { scale?: string; validate?: boolean } = {},
): Promise<EmitReport> {
  const warnings: string[] = [];
  const interp = new Interp({ hostApi: null, stdout: () => {}, stderr: () => {} });
  for (const f of program.order) interp.addModule(f, program.files.get(f)!);
  await interp.linkProgram();

  // 枚举注册表（全程序）
  const enums = new Map<string, A.Item & { kind: 'enum' }>();
  for (const [, ast] of program.files) {
    for (const item of ast.items) if (item.kind === 'enum') enums.set(item.name, item);
  }

  // scale：入口文件声明 > 选项 > microkernel
  const entryAst = program.files.get(program.entry)!;
  const scale = opts.scale ?? entryAst.scale?.mode ?? 'microkernel';

  // 项源码行映射
  const lineMaps = new Map<string, Map<A.Item, string[]>>();
  for (const [f, ast] of program.files) lineMaps.set(f, buildItemLineMap(ast));

  // 收集投射：物理路径 → { lang, items[] }
  interface Agg { lang: string; items: ProjectedItem[]; span: A.Span }
  const byPath = new Map<string, Agg>();
  const entryDir = path.dirname(program.entry);

  const resolveTarget = (name: string, fromAst: A.File): { item: A.Item; module: string } | null => {
    // 本文件
    const local = fromAst.items.find((it) =>
      (it.kind === 'fn' ? it.fn.name
        : it.kind === 'graph' ? it.graph.name
        : it.kind === 'impl' ? it.typeName
        : (it as { name?: string }).name) === name);
    if (local) return { item: local, module: fromAst.file };
    // import 链跨模块（P3 允许经 import 投射）
    for (const [mf, mAst] of program.files) {
      if (mf === fromAst.file) continue;
      const found = mAst.items.find((it) =>
        ((it.kind === 'fn' || it.kind === 'graph' || it.kind === 'blockres' || it.kind === 'struct' || it.kind === 'enum' || it.kind === 'trait' || it.kind === 'impl' || it.kind === 'const' || it.kind === 'typealias')
          && (it.kind === 'fn' ? it.fn.name : it.kind === 'graph' ? it.graph.name : it.kind === 'impl' ? it.typeName : (it as { name: string }).name) === name
          && it.exported));
      if (found) return { item: found, module: mf };
    }
    return null;
  };

  for (const f of program.order) {
    const ast = program.files.get(f)!;
    if (!ast.project) continue;
    for (const p of ast.project.items) {
      const name = p.target.join('::');
      const resolved = resolveTarget(name, ast);
      if (!resolved) {
        warnings.push(`P-3：投射目标 "${name}" 未定义（${path.relative(entryDir, f)}）`);
        continue;
      }
      const langId = p.lang;
      const lang = getLang(langId);
      if (!lang) {
        warnings.push(`P-4：未注册的后端语言 "${langId}"（${name}）`);
        continue;
      }
      const agg = byPath.get(p.path) ?? { lang: langId, items: [], span: p.span };
      if (agg.lang !== langId) {
        warnings.push(`P-5：物理文件 ${p.path} 被投射为多种语言（${agg.lang} vs ${langId}）`);
        continue;
      }
      agg.items.push({ item: resolved.item, module: resolved.module, kind: resolved.item.kind, name });
      byPath.set(p.path, agg);
    }
  }

  // 生成
  const files: EmittedFile[] = [];
  // v1.4.10：go 同 package 助手去重共享状态（首个 go 文件注入助手，其余仅 import）
  const goHelpersState = { done: false };

  // ---- 跨文件类型依赖：类型位置索引（lang → 类型名 → 物理文件） ----
  const typeLoc = new Map<string, Map<string, { path: string; pi: ProjectedItem }>>();
  for (const [relPath, agg] of byPath) {
    if (isStaticLangId(agg.lang)) continue;
    for (const pi of agg.items) {
      if (pi.kind === 'struct' || pi.kind === 'enum' || pi.kind === 'trait' || pi.kind === 'typealias') {
        if (!typeLoc.has(agg.lang)) typeLoc.set(agg.lang, new Map());
        typeLoc.get(agg.lang)!.set(pi.name, { path: relPath, pi });
      }
    }
  }
  // 全程序类型注册表（cpp 内联兜底：类型未被投射时仍可从 AST 内联声明）
  const allTypes = new Map<string, ProjectedItem>();
  for (const [mf, ast] of program.files) {
    for (const item of ast.items) {
      if (item.kind === 'struct' || item.kind === 'enum' || item.kind === 'trait' || item.kind === 'typealias') {
        allTypes.set(item.name, { item, module: mf, kind: item.kind, name: item.name });
      }
    }
  }

  for (const [relPath, agg] of byPath) {
    const abs = path.resolve(outDir, relPath);
    fs.mkdirSync(path.dirname(abs), { recursive: true });
    let text: string;
    if (isStaticLangId(agg.lang)) {
      // 静态资源：blockres 渲染（{{}} 插值已固化）
      const blockItem = agg.items[0]!.item;
      if (blockItem.kind !== 'blockres') {
        warnings.push(`P-4：${relPath} 为静态目标但项 ${agg.items[0]!.name} 不是 block/static`);
        continue;
      }
      const hit = interp.modules.get(agg.items[0]!.module)?.env.lookup(blockItem.name);
      text = typeof hit?.value === 'string'
        ? hit.value
        : await interp.renderBlockres(hit?.value as { __blockres: true; item: A.Item; module: string });
    } else {
      // 代码目标：decls 打印器
      const lang = getLang(agg.lang)!;
      const ctxModule = path.relative(entryDir, agg.items[0]!.module);
      const firstModuleAst = program.files.get(agg.items[0]!.module)!;
      const lineMap = lineMaps.get(agg.items[0]!.module)!;
      // hslLinesOf：跨模块项按各自源文件切片
      const perItemLines = new Map<A.Item, string[]>();
      for (const pi of agg.items) {
        const lm = lineMaps.get(pi.module);
        if (lm?.has(pi.item)) perItemLines.set(pi.item, lm.get(pi.item)!);
      }
      // 跨模块类型项的源码行（供内联声明用，类型声明不使用围栏，无镜像也安全）
      for (const [, tl] of typeLoc) {
        for (const { pi } of tl.values()) {
          if (!perItemLines.has(pi.item)) {
            const lm = lineMaps.get(pi.module);
            if (lm?.has(pi.item)) perItemLines.set(pi.item, lm.get(pi.item)!);
          }
        }
      }
      const ctx = {
        lang,
        module: ctxModule,
        scale,
        enums,
        hslLinesOf: (item: A.Item) => perItemLines.get(item) ?? [`${firstModuleAst.items.length ? '' : ''}`],
        hslSourceOf: (item: A.Item) => (perItemLines.get(item) ?? []).join('\n'),
        // v1.4.10：go 同 package 多文件顶级助手去重（真机 go build 实测修复）
        goHelpersState,
      };
      // ---- 跨文件类型依赖接线 ----
      const extraHeader = importHeaderForTypeDeps(
        agg, relPath, typeLoc, allTypes, ctx, warnings,
      );
      text = emitFile(lang, agg.items, ctx, extraHeader.length > 0 ? extraHeader : undefined, path.posix.basename(relPath, path.posix.extname(relPath)));
      // Java / C# 跨文件类型告警：类型已顶层（同包/命名空间裸名互见），未投射类型诚实 X-1
      if (agg.lang === 'java' || agg.lang === 'csharp') {
        warnTopLevelTypeRefs(agg, relPath, typeLoc, allTypes, warnings);
      }
    }
    fs.writeFileSync(abs, text, 'utf-8');
    const langTier = codegenTier(getLang(agg.lang)!);
    files.push({
      path: relPath,
      lang: agg.lang,
      bytes: Buffer.byteLength(text, 'utf-8'),
      items: agg.items.map((i) => i.name),
      tier: langTier,
      // 诚实边界：full/logic 文件内逐块回退检测（标记形如 "dhv: <name> 未翻译"）
      ...(langTier === 'full' || langTier === 'logic'
        ? { contract_fallbacks: contractFallbacksOf(text) }
        : {}),
      validation: { ok: true, tool: 'none' },
    });
  }

  // 交叉语法校验（Lint 第 2 层：目标语言真实工具链）
  if (opts.validate !== false) {
    for (const file of files) {
      if (file.tier === 'static') {
        file.validation = { ok: true, tool: 'embedded' };
        continue;
      }
      const abs = path.resolve(outDir, file.path);
      file.validation = await validateGeneratedFile(abs, file.lang);
    }
  }

  // manifest
  const manifest = {
    dhv: 'dhv-ts 0.2.10',
    entry: path.basename(program.entry),
    scale,
    backends: 38,
    generated_at: new Date().toISOString(),
    files: files.map((f) => ({
      path: f.path, lang: f.lang, tier: f.tier, bytes: f.bytes, items: f.items,
      ...(f.contract_fallbacks ? { contract_fallbacks: f.contract_fallbacks } : {}),
      syntax_check: f.validation.ok ? 'pass' : 'fail',
      syntax_tool: f.validation.tool,
      syntax_detail: f.validation.detail,
    })),
    warnings,
    protocol: {
      fence: '@dhv:source-map / @dhv:hsl-mirror / @dhv:end-source-map',
      sync: '编辑围栏内 HSL 镜像 → dhv sync <file> 回写 .hsl → dhv emit 重新生成',
      honesty: {
        full: '活体语句翻译（python/typescript/javascript）',
        logic: '语句子集翻译（rust/go/cpp），不可翻译时回退 contract',
        contract: '类型契约 + 签名真实翻译，函数体围栏内嵌 HSL 源镜像 + 未实现标记',
        contract_fallbacks: 'tier 是语言能力级；本字段列出 full/logic 文件内实际回退 contract 的块名（无此字段 = 全部活体）',
      },
    },
  };
  fs.writeFileSync(path.resolve(outDir, 'manifest.json'), JSON.stringify(manifest, null, 2), 'utf-8');

  return { outDir, scale, entry: program.entry, files, warnings };
}
