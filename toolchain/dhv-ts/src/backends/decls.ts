// ============================================================================
// dhv-ts/src/backends/decls.ts — 项声明打印器（32 编程语言）
// ----------------------------------------------------------------------------
// 每个语言一个打印器：struct / enum / trait / impl / fn / const / typealias /
// graph 脚手架。声明层是"真实翻译"（合法目标语言语法）；函数体：
//   full/logic 语言 → body.ts 活体翻译，失败回退 contract
//   contract 语言   → @dhv:source-map 围栏内嵌 HSL 原文 + 未实现标记
// 围栏协议（与 dhv/src/sourcemap.rs 对齐，总纲 §6）：
//   {lineComment} @dhv:source-map: <module>:<line>, block: <item>
//   ... 可编辑区 ...
//   {lineComment} @dhv:end-source-map
// 内核代码以 {lineComment} @dhv:generated 标记（不可手改）。
// ============================================================================

import * as A from '../ast';
import { LangSpec } from './registry';
import { printType } from './typrint';
import { transpileBody, TranspileError, languagePrelude } from './body';
import { VERSION } from '../version';

// ---------------------------------------------------------------------------
// 上下文与工具
// ---------------------------------------------------------------------------

export interface EmitCtx {
  lang: LangSpec;
  module: string;          // 逻辑模块文件（.hsl 路径，供围栏定位）
  scale: string;           // monolith | microkernel
  /** 取该项的 HSL 源码行（按行界近似切片） */
  hslLinesOf(item: A.Item): string[];
  /** 项源码整体文本（去首尾空行） */
  hslSourceOf(item: A.Item): string;
  enums: Map<string, A.Item & { kind: 'enum' }>; // 全程序枚举注册表
  /** go 同 package 多文件顶级助手去重状态（v1.4.10：跨文件共享，首文件注入助手） */
  goHelpersState?: { done: boolean };
}

export interface P {
  lang: LangSpec;
  ctx: EmitCtx;
  ind: string;
  ty(t: A.HType | undefined): string;
  lc: string;              // 行注释前缀
  fence(item: A.Item, name: string, body: string[] | null, indent: string): string[];
  contractFence(item: A.Item, name: string, indent: string): string[];
  unimpl(indent: string, what: string): string;
  comment(indent: string, text: string): string;
  strLit(s: string): string;
  params(fn: A.FnDef, typed: boolean): string;
}

/** 注释行（带闭合尾缀的语言如 OCaml `(* ... *)`） */
function cline(lang: LangSpec, text: string): string {
  return lang.lineCommentClose
    ? `${lang.lineComment} ${text} ${lang.lineCommentClose}`
    : `${lang.lineComment} ${text}`;
}

function makeP(ctx: EmitCtx): P {
  const lang = ctx.lang;
  const p: P = {
    lang,
    ctx,
    ind: '    ',
    ty: (t) => printType(t, lang),
    lc: lang.lineComment,
    fence: (item, name, body, indent) => {
      const line = item.span.line;
      const out: string[] = [];
      out.push(`${indent}${cline(lang, `@dhv:source-map: ${ctx.module}:${line}, block: ${name}${body ? ' (live)' : ''}`)}`);
      if (body) {
        // 活体翻译区（内核生成，重编译覆盖；HSL 改动经 sync 回写后再 emit 更新此区）
        out.push(...body);
      }
      // HSL 源镜像 —— dhv sync 的回写依据（可编辑区：修改后 dhv sync <file>）
      out.push(`${indent}${cline(lang, '@dhv:hsl-mirror — HSL 源镜像（编辑此区后 dhv sync 回写源码）')}`);
      const src = ctx.hslLinesOf(item);
      for (const l of src) out.push(l.length ? `${indent}${cline(lang, l)}` : (lang.lineCommentClose ? `${indent}${lang.lineComment}${lang.lineCommentClose}` : ''));
      out.push(`${indent}${cline(lang, '@dhv:end-source-map')}`);
      return out;
    },
    contractFence: (item, name, indent) => p.fence(item, name, null, indent),
    unimpl: (indent, what) => unimplFor(lang.id, indent, what),
    comment: (indent, text) => `${indent}${lang.lineComment} ${text}`,
    strLit: (s) => strLitFor(lang.id, s),
    params: (fn, typed) => paramsFor(p, fn, typed),
  };
  return p;
}

function strLitFor(langId: string, s: string): string {
  const body = s.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n').replace(/\r/g, '\\r').replace(/\t/g, '\\t');
  if (langId === 'python' || langId === 'ruby' || langId === 'lua' || langId === 'julia') {
    return `'${body.replace(/'/g, "\\'")}'`;
  }
  return `"${body}"`;
}

function unimplFor(langId: string, indent: string, what: string): string {
  const msg = `dhv: ${what} — HSL 逻辑在 @dhv:source-map 围栏内，运行请用 dhv-ts 或 dhv 编译`;
  switch (langId) {
    case 'python': return `${indent}raise NotImplementedError('${msg.replace(/'/g, "\\'")}')`;
    case 'typescript': return `${indent}throw new Error('${msg}');`;
    case 'javascript': return `${indent}throw new Error('${msg}');`;
    case 'rust': return `${indent}todo!("${msg}");`;
    case 'go': return `${indent}panic("${msg}");`;
    case 'cpp': return `${indent}throw std::runtime_error("${msg}");`;
    case 'java': return `${indent}throw new UnsupportedOperationException("${msg}");`;
    case 'csharp': return `${indent}throw new NotImplementedException("${msg}");`;
    case 'kotlin': return `${indent}TODO("${msg}")`;
    case 'swift': return `${indent}fatalError("${msg}")`;
    case 'ruby': return `${indent}raise NotImplementedError, '${msg.replace(/'/g, "\\'")}'`;
    case 'php': return `${indent}throw new RuntimeException('${msg}');`;
    case 'lua': return `${indent}error('${msg.replace(/'/g, "\\'")}')`;
    case 'perl': return `${indent}die '${msg.replace(/'/g, "\\'")}';`;
    case 'bash': return `${indent}echo '${msg.replace(/'/g, "'\\''")}' >&2; return 1`;
    case 'powershell': return `${indent}throw '${msg}'`;
    case 'r': return `${indent}stop("${msg}")`;
    case 'julia': return `${indent}error("${msg}")`;
    case 'scala': return `${indent}throw new NotImplementedError("${msg}")`;
    case 'elixir': return `${indent}raise "${msg}"`;
    case 'erlang': return `${indent}erlang:error(dhv_contract).`;
    case 'haskell': return `${indent}error "${msg}"`;
    case 'ocaml': return `${indent}failwith "${msg}"`;
    case 'fsharp': return `${indent}failwith "${msg}"`;
    case 'zig': return `${indent}return error.DhvContract; // ${msg}`;
    case 'nim': return `${indent}raise newException(CatchableError, "${msg}")`;
    case 'crystal': return `${indent}raise "${msg}"`;
    case 'dart': return `${indent}throw UnimplementedError('${msg}');`;
    case 'groovy': return `${indent}throw new UnsupportedOperationException('${msg}')`;
    case 'objectivec': return `${indent}@throw [NSException exceptionWithName:@"DhvContract" reason:@"${msg}" userInfo:nil];`;
    case 'd': return `${indent}throw new Exception("${msg}");`;
    case 'vb': return `${indent}Throw New NotImplementedException("${msg}")`;
    default: return `${indent}/* ${msg} */`;
  }
}

/** 参数表打印（typed=输出类型；self 参数交给各打印器处理） */
function paramsFor(p: P, fn: A.FnDef, typed: boolean): string {
  const out: string[] = [];
  for (const prm of fn.params) {
    if (prm.self) continue; // 由调用方处理
    const name = patternName(prm.pat) ?? 'arg';
    if (typed && prm.ty) {
      out.push(typedParam(p, name, prm.ty));
    } else {
      out.push(name);
    }
  }
  return out.join(', ');
}

function typedParam(p: P, name: string, ty: A.HType): string {
  switch (p.lang.id) {
    case 'python': return `${name}: ${p.ty(ty)}`;
    case 'typescript': return `${name}: ${p.ty(ty)}`;
    case 'rust': return `${name}: ${p.ty(ty)}`;
    case 'go': return `${name} ${p.ty(ty)}`;
    case 'cpp': return `${p.ty(ty)} ${name}`;
    case 'java': case 'csharp': case 'dart': case 'groovy':
      return `${p.ty(ty)} ${name}`;
    case 'kotlin': case 'swift': case 'scala':
      // 类型后置冒号语法：fun f(x: T) / func f(x: T) / def f(x: T)
      return `${name}: ${p.ty(ty)}`;
    case 'php': return `${p.ty(ty)} $${name}`;
    case 'powershell': return `${p.ty(ty)}$${name}`;
    case 'vb': return `ByVal ${name} As ${p.ty(ty)}`;
    case 'haskell': return `(${name} :: ${p.ty(ty)})`;
    case 'ocaml': case 'fsharp': return `(${name} : ${p.ty(ty)})`;
    default: return `${name}: ${p.ty(ty)}`;
  }
}

export function patternName(pat: A.Pattern): string | undefined {
  if (pat.kind === 'binding') return pat.name;
  if (pat.kind === 'wildcard') return '_';
  if (pat.kind === 'path') return pat.segs[pat.segs.length - 1];
  return undefined;
}

/** 顶层 fn 的声明签名前缀（含 async / 返回类型），返回 [声明头, 是否需要体] */
function fnHeader(p: P, fn: A.FnDef, opts: { isMethod?: boolean; selfType?: string } = {}): { head: string; ret: string } {
  const L = p.lang.id;
  const asyncKw = fn.isAsync
    ? (L === 'python' ? 'async ' : L === 'rust' ? 'async ' : L === 'typescript' || L === 'javascript' ? 'async ' : '')
    : '';
  const name = fn.name;
  const params = p.params(fn, true);
  const selfP = opts.isMethod ? methodSelfParam(p, opts.selfType ?? 'Self') + (params ? ', ' : '') : '';
  const retTy = fn.ret ? p.ty(fn.ret) : '';
  const unit = p.lang.types.unit ?? 'void';
  switch (L) {
    case 'python': return { head: `${asyncKw}def ${name}(${selfP}${params})`, ret: ` -> ${retTy || unit}` };
    case 'typescript': case 'javascript': return { head: `${asyncKw}function ${name}(${selfP}${params})`, ret: retTy ? `: ${retTy}` : '' };
    case 'rust': return { head: `${asyncKw}${opts.isMethod ? '' : 'pub '}fn ${name}(${selfP}${params})`, ret: retTy ? ` -> ${retTy}` : '' };
    case 'go': return { head: `func ${name}(${selfP}${params})`, ret: retTy ? ` ${goRet(retTy)}` : '' };
    case 'cpp': return { head: `${retTy || 'void'} ${name}(${selfP}${params})`, ret: '' };
    case 'java': return { head: `static ${retTy || 'Object'} ${name}(${selfP}${params})`, ret: '' };
    case 'csharp': return { head: `public static ${retTy || 'object'} ${name}(${selfP}${params})`, ret: '' };
    case 'kotlin': return { head: `${fn.isAsync ? 'suspend ' : ''}fun ${name}(${selfP}${params})`, ret: retTy ? `: ${retTy}` : '' };
    case 'swift': return { head: `func ${name}(${selfP}${params})${fn.isAsync ? ' async' : ''}`, ret: retTy ? ` -> ${retTy}` : '' };
    case 'ruby': return { head: `def ${name}(${selfP.replace(/: /g, ', ') && ''}${selfP ? 'self, ' : ''}${params})`, ret: '' };
    case 'php': return { head: `function ${name}(${selfP}${params})`, ret: retTy ? `: ${retTy}` : '' };
    case 'lua': return { head: `function ${name}(${selfP}${params})`, ret: '' };
    case 'perl': return { head: `sub ${name} {`, ret: '' };
    case 'bash': return { head: `${name}() {`, ret: '' };
    case 'powershell': return { head: `function ${name} {`, ret: '' };
    case 'r': return { head: `${name} <- function(${selfP}${params})`, ret: '' };
    case 'julia': return { head: `function ${name}(${selfP}${params})`, ret: retTy ? `::${retTy}` : '' };
    case 'scala': return { head: `def ${name}(${selfP}${params})`, ret: retTy ? `: ${retTy}` : '' };
    case 'elixir': return { head: `def ${name}(${selfP}${params}) do`, ret: '' };
    case 'erlang': return { head: `${name}(${selfP}${params}) ->`, ret: '' };
    case 'haskell': return { head: `${name} :: ${fn.ret ? `HSLFn` : `HSLFn`}`, ret: '' }; // 签名行独立处理
    case 'ocaml': return { head: `let ${name} ${selfP}${params}`, ret: '' };
    case 'fsharp': return { head: `let ${name} (${selfP}${params})`, ret: '' };
    case 'zig': return { head: `fn ${name}(${selfP}${params})`, ret: retTy ? ` ${retTy}` : ' void' };
    case 'nim': return { head: `proc ${name}*(${selfP}${params})`, ret: retTy ? `: ${retTy}` : '' };
    case 'crystal': return { head: `def ${name}(${selfP}${params})`, ret: retTy ? ` : ${retTy}` : '' };
    case 'dart': return { head: `${fn.isAsync ? 'async ' : ''}${retTy || 'dynamic'} ${name}(${selfP}${params})`, ret: '' };
    case 'groovy': return { head: `${retTy || 'def'} ${name}(${selfP}${params})`, ret: '' };
    case 'objectivec': return { head: `${retTy || 'id'} ${name}(${selfP}${params})`, ret: '' };
    case 'd': return { head: `${retTy || 'void'} ${name}(${selfP}${params})`, ret: '' };
    case 'vb': return { head: `Function ${name}(${selfP}${params})`, ret: ` As ${retTy || 'Object'}` };
    default: return { head: `function ${name}(${selfP}${params})`, ret: retTy ? ` -> ${retTy}` : '' };
  }
}

function goRet(retTy: string): string {
  // Go 的 Result 映射 (T, error) → 多返回值签名
  if (retTy.startsWith('(')) return retTy.replace(/^\(|\)$/g, '');
  return retTy;
}

function methodSelfParam(p: P, selfType: string): string {
  switch (p.lang.id) {
    case 'python': return `self`;
    case 'typescript': case 'javascript': return `this: ${selfType}`;
    case 'rust': return `self: &${selfType}`;
    case 'go': return `self *${selfType}`;
    case 'cpp': return `${selfType}& self`;
    default: return `self`;
  }
}

// ---------------------------------------------------------------------------
// 主入口
// ---------------------------------------------------------------------------

export interface ProjectedItem {
  item: A.Item;
  module: string;   // 定义所在 .hsl（可能与 ctx.module 不同——跨模块投射）
  kind: string;     // item.kind
  name: string;
}

/**
 * 只打印单个类型项的声明（struct/enum/trait/typealias，无围栏无镜像）——
 * 用于 cpp 后端的跨文件类型内联（ODR 兼容：与被投射文件中的定义逐字一致）。
 * 非类型项返回空数组（调用方过滤）。
 */
export function emitTypeDeclOnly(lang: LangSpec, pi: ProjectedItem, ctx: EmitCtx): string[] {
  if (pi.kind !== 'struct' && pi.kind !== 'enum' && pi.kind !== 'trait' && pi.kind !== 'typealias') return [];
  const p = makeP(ctx);
  return emitItem(p, pi, [pi]);
}

/** 生成一个物理文件的全部内容。items 为投射到该文件的全部逻辑项。 */
export function emitFile(
  lang: LangSpec,
  items: ProjectedItem[],
  ctx: EmitCtx,
  extraHeader?: string[],
  fileStem?: string,
): string {
  const p = makeP(ctx);
  const out: string[] = [];
  const L = lang.id;

  // ---- 文件头（@dhv:generated 内核标记，不可手改）----
  // go：同 package 多文件助手去重（v1.4.10 真机 go build 实测：重复声明 = 编译错误）
  let goSkipHelpers = false;
  if (lang.id === 'go' && ctx.goHelpersState) {
    goSkipHelpers = ctx.goHelpersState.done;
    ctx.goHelpersState.done = true;
  }
  out.push(...fileHeader(p, items, goSkipHelpers));
  if (extraHeader) {
    // v0.2.51：接线 import 置于「运行期助手」def 之前 —— 此前附加在助手之后，
    // 在「exec 裸函数体」消费形态（tests/hsl 语义级验证）下 `from color import Red`
    // 会遮蔽消费方在 exec 命名空间提供的同名桩类（isinstance 判定失效）。
    // 置于助手前既符合 import 靠近文件头的惯例，也消除遮蔽。
    const helperIdx = out.findIndex((l) => l.includes('运行期助手'));
    if (helperIdx >= 0) out.splice(helperIdx, 0, ...extraHeader, '');
    else out.push(...extraHeader);
  }

  // ---- 项按 类别排序：类型 → trait → impl → fn → graph ----
  const order: Record<string, number> = { struct: 0, enum: 1, typealias: 2, const: 3, trait: 4, impl: 5, fn: 6, graph: 7, blockres: 8 };
  const sorted = [...items].sort((a, b) => (order[a.kind] ?? 9) - (order[b.kind] ?? 9));

  const bodyLines: string[] = [];
  // Java / C# 分流：类型项（record/sealed/interface/struct/enum）顶层合法；仅 fn/const/impl 需宿主类
  // （Java：顶层函数/常量非法；C#：方法/常量必须属于 class —— 二者同构，仅 wrapper 语法不同）
  const wrapLang = L === 'java' || L === 'csharp';
  const typeLines: string[] = [];
  const hostLines: string[] = [];
  for (const pi of sorted) {
    const itemLines = emitItem(p, pi, items);
    if (wrapLang) {
      if (pi.kind === 'struct' || pi.kind === 'enum' || pi.kind === 'trait' || pi.kind === 'typealias') {
        typeLines.push('', ...itemLines);
      } else {
        hostLines.push('', ...itemLines);
      }
    } else {
      bodyLines.push('', ...itemLines);
    }
  }

  // Java / C#：类型顶层声明（同包/同命名空间裸名互见，跨文件引用无需限定）+
  // 宿主 class 仅色 fn/const/impl（顶层函数/常量非法）——
  // 宿主名 Dhv<文件名 stem>（每文件唯一，防同模块多文件互撞；
  // 前缀防与顶层类型名碰撞；package-private / internal：无需匹配文件名）
  if (wrapLang && sorted.length > 0) {
    out.push(...typeLines);
    if (hostLines.length > 0) {
      const cls = javaHostClass(fileStem ?? items[0]?.module);
      // Java：class Dhv<Stem> { ... }；C#：internal static class Dhv<Stem> { ... }
      // （static class：所有成员必须 static，与 fn/const 投射形态一致；防实例化）
      const opener = L === 'csharp' ? `internal static class ${cls} {` : `class ${cls} {`;
      out.push('', opener);
      for (const ln of hostLines) out.push(ln === '' ? '' : `    ${ln}`);
      out.push('}');
    }
  } else {
    out.push(...bodyLines);
  }

  // 语言收尾
  if (L === 'php') {
    const text = out.join('\n');
    if (!text.startsWith('<?php')) return `<?php\n${text}\n`;
  }
  if (L === 'perl') out.push('1;');
  if (L === 'erlang') out.push('');

  // v1.4.10 go：import 按需裁剪（真机 go build 实测：未使用 import = 编译错误）。
  // import 块之后正文无 `pkg.` 引用的包从 import 中移除；全部未用则替换为空白导入。
  if (L === 'go') return trimGoImports(out);

  return out.join('\n').replace(/\n{3,}/g, '\n\n').trim() + '\n';
}

/** v1.4.10：go body 尾 return 判定（跳过尾随兜底 return，防 vet unreachable） */
function goBodyEndsWithReturn(body: string[]): boolean {
  for (let i = body.length - 1; i >= 0; i--) {
    const t = body[i]!.trim();
    if (t === '' || t.startsWith('//')) continue;
    return t.startsWith('return');
  }
  return false;
}

/** go import 按需裁剪：正文（import 块之后）无 `pkg.` 引用的包移除 */
function trimGoImports(lines: string[]): string {
  const text = lines.join('\n');
  const m = text.match(/^import \(\n((?:\t"[^"]+"\n)+)\)/m);
  if (!m) return text.replace(/\n{3,}/g, '\n\n').trim() + '\n';
  const importBlock = m[0];
  const pkgs = Array.from(m[1].matchAll(/\t"([^"]+)"/g)).map((x) => x[1]);
  const after = text.slice(text.indexOf(importBlock) + importBlock.length);
  // 助手声明若被去重跳过，本文件可能不含 fmt. 等引用 —— import 集合按正文实际引用裁剪
  const used = pkgs.filter((p) => new RegExp(`\\b${p.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\.`).test(after));
  // 全部未用：直接省略 import 块（go：空 import 块 / 空白导入均非必要）
  const replacement = used.length === 0
    ? ''
    : `import (\n${used.map((p) => `\t"${p}"`).join('\n')}\n)`;
  return text
    .replace(importBlock, replacement)
    .replace(/\n{3,}/g, '\n\n')
    .trim() + '\n';
}

function fileHeader(p: P, items: ProjectedItem[], goSkipHelpers = false): string[] {
  const { lang, ctx } = p;
  const c = (text: string): string => cline(lang, text);
  const head: string[] = [];
  head.push(c('='.repeat(70)));
  head.push(c(`本文件由 DHV v${VERSION} 从 HSL 源码投射生成（${lang.name} 后端 · ${ctx.scale} 尺度）`));
  head.push(c(`逻辑源：${items.map((i) => i.module).filter((v, i, a) => a.indexOf(v) === i).join(', ')}`));
  head.push(c('@dhv:generated — 本区块为内核标记，不可手改（下次编译覆盖）'));
  head.push(c('可编辑区：@dhv:source-map 围栏之间（修改后 dhv sync 回写 HSL 源码）'));
  head.push(c(`能力级：${lang.body === 'full' ? 'full 活体翻译' : lang.body === 'logic' ? 'logic 语句子集' : 'contract 类型契约'}（BNF v1.4 §5.2）`));
  head.push(c('='.repeat(70)));

  // 语言级 import 头
  switch (lang.id) {
    case 'cpp': {
      head.push('#include <cstdint>', '#include <string>', '#include <vector>', '#include <map>', '#include <unordered_map>', '#include <optional>', '#include <variant>', '#include <memory>', '#include <stdexcept>', '#include <iostream>', '#include <format>', '#include <algorithm>', '#include <cctype>', '#include <type_traits>');
      // _dhvSome / _dhvPop / _dhvFirst / _dhvLast / _dhvVecGet / _dhvInsert / _dhvRemoveAt /
      // _dhvKeys / _dhvValues / _dhvMapGet / _dhvMapRemove / String 助手（定义于 body.languagePrelude）
      // include guard 兜底单 TU 拼接场景（多文件 concat）；跨 TU 各自实例化独立符号
      head.push(...languagePrelude('cpp'));
      break;
    }
    case 'go': {
      head.unshift('package hsl', '');
      head.push(...languagePrelude('go', goSkipHelpers));
      break;
    }
    case 'java': head.push('import java.util.*;'); break;
    case 'kotlin': head.push('import java.util.*'); break;
    case 'csharp': head.push('using System;', 'using System.Collections.Generic;'); break;
    case 'typescript': {
      head.push('/* eslint-disable */');
      head.push(...languagePrelude('typescript'));
      break;
    }
    case 'javascript': {
      head.push(...languagePrelude('javascript'));
      break;
    }
    case 'rust': {
      // v1.4.10（真机 rustc 实测）：HashMap 非 prelude 类型，统一头部导入
      // （未使用由 #![allow(unused_imports)] 豁免；Vec/Option/Result 在 prelude）
      head.push('#![allow(dead_code, unused_variables, unused_imports)]', 'use std::collections::HashMap;');
      break;
    }
    case 'python': {
      head.push('from __future__ import annotations', 'from dataclasses import dataclass, field', 'from typing import Any, Optional, Dict, List, Callable');
      // v0.2.51：std/math 函数与常量映射为 math.*（body.ts 路径映射；未使用无害）
      head.push('import math');
      head.push(...languagePrelude('python'));
      break;
    }
    case 'swift': head.push('import Foundation'); break;
    case 'objectivec': head.push('#import <Foundation/Foundation.h>'); break;
    case 'zig': head.push('const std = @import("std");'); break;
    case 'nim': head.push('import std/[tables, options, json]'); break;
    case 'crystal': head.push('require "json"'); break;
    case 'julia': head.push('using Dates'); break;
    case 'scala': head.push('import scala.collection.immutable.*', 'import scala.util.*'); break;
    case 'erlang': {
      const mod = erlModuleName(items);
      const fns = items.filter((i) => i.kind === 'fn');
      head.push(`-module(${mod}).`);
      if (fns.length > 0) head.push(`-export([${fns.map((i) => `${snakeLower(i.name)}/${fnArity(i.item)}`).join(', ')}]).`);
      break;
    }
  }
  return head;
}


function erlModuleName(items: ProjectedItem[]): string {
  const first = items[0];
  if (!first) return 'hsl';
  const base = (first.module.split(/[\\/]/).pop() ?? 'hsl').replace(/\.hsl$/, '');
  return 'hsl_' + base.toLowerCase();
}

/** Java 宿主类名：Dhv<文件 stem PascalCase>（每文件唯一 + 前缀防与顶层类型名碰撞） */
export function javaHostClass(stem: string | undefined): string {
  const base = (stem ?? 'hsl').replace(/\.hsl$/, '').replace(/[^A-Za-z0-9_]/g, '_');
  const pascal = base.split(/[_\-\s]+/).filter(Boolean)
    .map((s) => s[0]!.toUpperCase() + s.slice(1)).join('') || 'Module';
  return /^[A-Za-z_]/.test(pascal) ? `Dhv${pascal}` : `Dhv${pascal}`;
}
/** 由源模块路径推导 java 包裹类名（历史兼容导出） */
export function javaWrapperOf(modulePath: string | undefined): string {
  const base = (modulePath?.split(/[\\/]/).pop() ?? 'hsl').replace(/\.hsl$/, '');
  const ident = base.replace(/[^A-Za-z0-9_]/g, '_');
  const pascal = ident.split(/[_\-\s]+/).filter(Boolean)
    .map((s) => s[0]!.toUpperCase() + s.slice(1)).join('') || 'HslModule';
  return /^[A-Za-z_]/.test(pascal) ? pascal : `Hsl${pascal}`;
}
function fnArity(item: A.Item): number {
  if (item.kind === 'fn') return item.fn.params.length;
  return 0;
}

// ---------------------------------------------------------------------------
// 单项打印
// ---------------------------------------------------------------------------

function emitItem(p: P, pi: ProjectedItem, fileItems: ProjectedItem[]): string[] {
  const { item } = pi;
  switch (item.kind) {
    case 'struct': return structDecl(p, item, pi);
    case 'enum': return enumDecl(p, item, pi);
    case 'trait': return traitDecl(p, item, pi);
    case 'impl': return implDecl(p, item, pi, fileItems);
    case 'fn': return fnDecl(p, item.fn, pi, false);
    case 'const': return constDecl(p, item, pi);
    case 'typealias': return typealiasDecl(p, item, pi);
    case 'graph': return graphDecl(p, item, pi);
    default: return [cline(p.lang, `(dhv: ${pi.kind} ${pi.name} 不投射为代码)`)];
  }
}

// ===== struct =====

function structDecl(p: P, d: A.Item & { kind: 'struct' }, pi: ProjectedItem): string[] {
  const out: string[] = [];
  const name = d.name;
  const fields = d.fields;
  const lc = p.lc;
  switch (p.lang.id) {
    case 'python': {
      out.push('@dataclass');
      out.push(`class ${name}:`);
      if (fields.length === 0) out.push('    pass');
      for (const f of fields) out.push(`    ${f.name}: ${p.ty(f.ty)}`);
      break;
    }
    case 'typescript': {
      out.push(`export interface ${name} {`);
      if (fields.length === 0) out.push('  // empty struct');
      for (const f of fields) out.push(`  ${f.name}: ${p.ty(f.ty)};`);
      out.push('}');
      out.push(`export function ${camel(name)}(${fields.map((f) => `${f.name}: ${p.ty(f.ty)}`).join(', ')}): ${name} {`);
      out.push(`  return { ${fields.map((f) => f.name).join(', ')} };`);
      out.push('}');
      break;
    }
    case 'javascript': {
      // JS 无接口：JSDoc 契约 + 工厂函数
      out.push(`/** @typedef {Object} ${name} HSL struct 契约：${fields.map((f) => `${f.name}: ${p.ty(f.ty)}`).join(', ')} */`);
      out.push(`export function ${camel(name)}(${fields.map((f) => f.name).join(', ')}) {`);
      out.push(`  return { ${fields.map((f) => f.name).join(', ')} };`);
      out.push('}');
      break;
    }
    case 'rust': {
      out.push(`#[derive(Debug, Clone, PartialEq)]`);
      out.push(`pub struct ${name} {`);
      if (fields.length === 0) out.push('}');
      for (const f of fields) out.push(`    pub ${f.name}: ${p.ty(f.ty)},`);
      if (fields.length > 0) out.push('}');
      break;
    }
    case 'go': {
      out.push(`type ${name} struct {`);
      if (fields.length === 0) out.push('}');
      for (const f of fields) out.push(`\t${f.name} ${p.ty(f.ty)}`);
      if (fields.length > 0) out.push('}');
      break;
    }
    case 'cpp': {
      out.push(`struct ${name} {`);
      for (const f of fields) out.push(`    ${p.ty(f.ty)} ${f.name};`);
      out.push('};');
      break;
    }
    case 'java': {
      out.push(`record ${name}(${fields.map((f) => `${p.ty(f.ty)} ${f.name}`).join(', ')}) {}`);
      break;
    }
    case 'csharp': {
      out.push(`internal record ${name}(${fields.map((f) => `${p.ty(f.ty)} ${capitalize(f.name)}`).join(', ')});`);
      break;
    }
    case 'kotlin': {
      out.push(`data class ${name}(${fields.map((f) => `val ${f.name}: ${p.ty(f.ty)}`).join(', ')})`);
      break;
    }
    case 'swift': {
      out.push(`struct ${name} {`);
      for (const f of fields) out.push(`    let ${f.name}: ${p.ty(f.ty)}`);
      out.push('}');
      break;
    }
    case 'ruby': {
      out.push(`${name} = Struct.new(${fields.map((f) => `:${f.name}`).join(', ')}, keyword_init: true)`);
      break;
    }
    case 'php': {
      out.push(`final class ${name} {`);
      out.push('    public function __construct(');
      for (const f of fields) out.push(`        public readonly ${p.ty(f.ty)} $${f.name},`);
      out.push('    ) {}');
      out.push('}');
      break;
    }
    case 'lua': {
      out.push(`${name} = {}`);
      out.push(`${name}.__index = ${name}`);
      out.push(`function ${name}.new(${fields.map((f) => f.name).join(', ')})`);
      out.push('    local self = setmetatable({}, ' + name + ')');
      for (const f of fields) out.push(`    self.${f.name} = ${f.name}`);
      out.push('    return self');
      out.push('end');
      break;
    }
    case 'julia': {
      out.push(`struct ${name}`);
      for (const f of fields) out.push(`    ${f.name}::${p.ty(f.ty)}`);
      out.push('end');
      break;
    }
    case 'scala': {
      out.push(`final case class ${name}(${fields.map((f) => `${f.name}: ${p.ty(f.ty)}`).join(', ')})`);
      break;
    }
    case 'haskell': {
      out.push(`data ${name} = ${name}`);
      if (fields.length > 0) {
        out.push('  { ' + fields.map((f) => `${haskellField(name, f.name)} :: ${p.ty(f.ty)}`).join('\n  , ') + ' }');
      }
      out.push('  deriving (Show, Eq)');
      break;
    }
    case 'ocaml': {
      out.push(`type ${lower1(name)} = {`);
      out.push(fields.map((f) => `  ${lowerSnake(f.name)} : ${p.ty(f.ty)};`).join('\n'));
      out.push('}');
      break;
    }
    case 'fsharp': {
      out.push(`type ${name} = {`);
      out.push(fields.map((f) => `  ${capitalize(f.name)} : ${p.ty(f.ty)}`).join('\n'));
      out.push('}');
      break;
    }
    case 'zig': {
      out.push(`pub const ${name} = struct {`);
      for (const f of fields) out.push(`    ${f.name}: ${p.ty(f.ty)},`);
      out.push('};');
      break;
    }
    case 'nim': {
      out.push('type');
      out.push(`  ${name}* = object`);
      for (const f of fields) out.push(`    ${f.name}* : ${p.ty(f.ty)}`);
      break;
    }
    case 'crystal': {
      out.push(`class ${name}`);
      for (const f of fields) out.push(`  property ${f.name} : ${p.ty(f.ty)}`);
      out.push(`  def initialize(${fields.map((f) => `@${f.name} : ${p.ty(f.ty)}`).join(', ')}); end`);
      out.push('end');
      break;
    }
    case 'dart': {
      out.push(`class ${name} {`);
      for (const f of fields) out.push(`  final ${p.ty(f.ty)} ${camel(f.name)};`);
      out.push(`  const ${name}({${fields.map((f) => `required this.${camel(f.name)}`).join(', ')}});`);
      out.push('}');
      break;
    }
    case 'groovy': {
      out.push(`@groovy.transform.Canonical`);
      out.push(`class ${name} {`);
      for (const f of fields) out.push(`    ${p.ty(f.ty)} ${f.name}`);
      out.push('}');
      break;
    }
    case 'objectivec': {
      out.push(`@interface ${name} : NSObject`);
      for (const f of fields) out.push(`@property (nonatomic, strong) ${p.ty(f.ty)} ${f.name};`);
      out.push('@end');
      break;
    }
    case 'd': {
      out.push(`struct ${name} {`);
      for (const f of fields) out.push(`    ${p.ty(f.ty)} ${f.name};`);
      out.push('}');
      break;
    }
    case 'vb': {
      out.push(`Public Structure ${name}`);
      for (const f of fields) out.push(`    Public ${capitalize(f.name)} As ${p.ty(f.ty)}`);
      out.push('End Structure');
      break;
    }
    case 'perl': {
      out.push(`package ${name};`);
      out.push(`sub new {`);
      out.push(`    my (\$class, %args) = @_;`);
      out.push(`    my \$self = { ${fields.map((f) => `${f.name} => \$args{${f.name}}`).join(', ')} };`);
      out.push(`    return bless \$self, \$class;`);
      out.push(`}`);
      break;
    }
    case 'r': {
      out.push(`${lower1(name)} <- function(${fields.map((f) => f.name).join(', ')}) {`);
      out.push(`  out <- list(${fields.map((f) => `${f.name} = ${f.name}`).join(', ')})`);
      out.push(`  class(out) <- "${name}"`);
      out.push(`  return(out)`);
      out.push(`}`);
      break;
    }
    case 'elixir': {
      out.push(`defmodule ${name} do`);
      out.push(`  defstruct [${fields.map((f) => `:${snakeAtom(f.name)}`).join(', ')}]`);
      out.push(`  def new(${fields.map((f) => snakeAtom(f.name)).join(', ')}) do`);
      out.push(`    %__MODULE__{${fields.map((f) => `${snakeAtom(f.name)}: ${snakeAtom(f.name)}`).join(', ')}}`);
      out.push(`  end`);
      out.push('end');
      break;
    }
    default: {
      // bash/powershell/erlang 等：注释契约
      out.push(`${lc} struct ${name} { ${fields.map((f) => `${f.name}: ${p.ty(f.ty)}`).join(', ')} }`);
      out.push(`${lc} （${p.lang.name} 无静态结构体：按运行期约定使用）`);
    }
  }
  return out;
}

// ===== enum（和类型）=====

function enumDecl(p: P, d: A.Item & { kind: 'enum' }, pi: ProjectedItem): string[] {
  const out: string[] = [];
  const name = d.name;
  const vs = d.variants;
  switch (p.lang.id) {
    case 'python': {
      out.push(`class ${name}:  # sealed — HSL enum 和类型`);
      out.push('    __slots__ = ()');
      out.push('');
      for (const v of vs) {
        const vfields = variantFields(v);
        if (vfields.length === 0) {
          out.push(`class ${v.name}(${name}):`);
          out.push('    __slots__ = ()');
          out.push(`${snakeUpper(v.name)} = ${v.name}()  # 无负载单例`);
          out.push('');
        } else {
          out.push('@dataclass');
          out.push(`class ${v.name}(${name}):`);
          for (const f of vfields) out.push(`    ${f.name}: ${p.ty(f.ty)}`);
          out.push('');
        }
      }
      break;
    }
    case 'typescript': {
      out.push(`export type ${name} =`);
      const parts: string[] = [];
      for (const v of vs) {
        const vfields = variantFields(v);
        if (vfields.length === 0) parts.push(`  | { kind: '${v.name}' }`);
        else parts.push(`  | { kind: '${v.name}'; ${vfields.map((f) => `${f.name}: ${p.ty(f.ty)}`).join('; ')} }`);
      }
      out.push(...parts);
      out.push(';');
      out.push('');
      for (const v of vs) {
        const vfields = variantFields(v);
        if (vfields.length === 0) {
          out.push(`export const ${snakeUpper(v.name)}: ${name} = { kind: '${v.name}' };`);
        } else {
          out.push(`export function ${v.name}(${vfields.map((f) => `${f.name}: ${p.ty(f.ty)}`).join(', ')}): ${name} {`);
          out.push(`  return { kind: '${v.name}', ${vfields.map((f) => f.name).join(', ')} };`);
          out.push('}');
        }
      }
      break;
    }
    case 'javascript': {
      // JS：标签对象 + 构造函数（无类型语法）
      out.push(`// ${name} — HSL enum 和类型（标签对象）`);
      for (const v of vs) {
        const vfields = variantFields(v);
        if (vfields.length === 0) {
          out.push(`export const ${snakeUpper(v.name)} = { kind: '${v.name}' };`);
        } else {
          out.push(`export function ${v.name}(${vfields.map((f) => f.name).join(', ')}) {`);
          out.push(`  return { kind: '${v.name}', ${vfields.map((f) => f.name).join(', ')} };`);
          out.push('}');
        }
      }
      break;
    }
    case 'rust': {
      out.push('#[derive(Debug, Clone, PartialEq)]');
      out.push(`pub enum ${name} {`);
      for (const v of vs) {
        const vfields = variantFields(v);
        if (vfields.length === 0) out.push(`    ${v.name},`);
        else if (v.fields && 'named' in v.fields) out.push(`    ${v.name} { ${vfields.map((f) => `${f.name}: ${p.ty(f.ty)}`).join(', ')} },`);
        else out.push(`    ${v.name}(${vfields.map((f) => p.ty(f.ty)).join(', ')}),`);
      }
      out.push('}');
      break;
    }
    case 'go': {
      out.push(`// ${name} — HSL enum 和类型（Go 以接口 + 类型开关表达）`);
      out.push(`type ${name} interface {`);
      out.push(`\t${name}Kind() string`);
      out.push('}');
      for (const v of vs) {
        const vfields = variantFields(v);
        out.push('');
        out.push(`type ${v.name} struct {`);
        for (const f of vfields) out.push(`\t${capitalize(f.name)} ${p.ty(f.ty)}`);
        out.push('}');
        out.push(`func (${v.name}) ${name}Kind() string { return "${v.name}" }`);
      }
      break;
    }
    case 'cpp': {
      out.push(`// ${name} — HSL enum 和类型（C++17 std::variant）`);
      for (const v of vs) {
        const vfields = variantFields(v);
        out.push(`struct ${v.name} {`);
        for (const f of vfields) out.push(`    ${p.ty(f.ty)} ${f.name};`);
        out.push('};');
      }
      out.push(`using ${name} = std::variant<${vs.map((v) => v.name).join(', ')}>;`);
      break;
    }
    case 'java': {
      out.push(`sealed interface ${name} permits ${vs.map((v) => v.name).join(', ')} {}`);
      for (const v of vs) {
        const vfields = variantFields(v);
        out.push(`record ${v.name}(${vfields.map((f) => `${p.ty(f.ty)} ${f.name}`).join(', ')}) implements ${name} {}`);
      }
      break;
    }
    case 'csharp': {
      out.push(`internal abstract record ${name} {`);
      out.push(`    private ${name}() { }`);
      out.push('}');
      for (const v of vs) {
        const vfields = variantFields(v);
        out.push(`internal record ${v.name}(${vfields.map((f) => `${p.ty(f.ty)} ${capitalize(f.name)}`).join(', ')}) : ${name};`);
      }
      break;
    }
    case 'kotlin': {
      out.push(`sealed class ${name} {`);
      for (const v of vs) {
        const vfields = variantFields(v);
        if (vfields.length === 0) out.push(`    object ${v.name} : ${name}()`);
        else out.push(`    data class ${v.name}(${vfields.map((f) => `val ${f.name}: ${p.ty(f.ty)}`).join(', ')}) : ${name}()`);
      }
      out.push('}');
      break;
    }
    case 'swift': {
      out.push(`enum ${name} {`);
      for (const v of vs) {
        const vfields = variantFields(v);
        if (vfields.length === 0) out.push(`    case ${v.name}`);
        else out.push(`    case ${v.name}(${vfields.map((f) => `${f.name}: ${p.ty(f.ty)}`).join(', ')})`);
      }
      out.push('}');
      break;
    }
    case 'scala': {
      out.push(`sealed trait ${name}`);
      for (const v of vs) {
        const vfields = variantFields(v);
        if (vfields.length === 0) out.push(`case object ${v.name} extends ${name}`);
        else out.push(`final case class ${v.name}(${vfields.map((f) => `${f.name}: ${p.ty(f.ty)}`).join(', ')}) extends ${name}`);
      }
      break;
    }
    case 'haskell': {
      out.push(`data ${name} =`);
      const parts: string[] = [];
      for (const v of vs) {
        const vfields = variantFields(v);
        parts.push(`  ${v.name} ${vfields.map((f) => p.ty(f.ty)).join(' ')}`);
      }
      out.push(parts.join('\n  | '));
      out.push('  deriving (Show, Eq)');
      break;
    }
    case 'ocaml': {
      out.push(`type ${lower1(name)} =`);
      const parts: string[] = [];
      for (const v of vs) {
        const vfields = variantFields(v);
        if (vfields.length === 0) parts.push(`  | ${v.name}`);
        else if (v.fields && 'named' in v.fields) parts.push(`  | ${v.name} of { ${vfields.map((f) => `${lowerSnake(f.name)} : ${p.ty(f.ty)}`).join('; ')} }`);
        else parts.push(`  | ${v.name} of ${vfields.map((f) => p.ty(f.ty)).join(' * ')}`);
      }
      out.push(parts.join('\n'));
      break;
    }
    case 'fsharp': {
      out.push(`type ${name} =`);
      const parts: string[] = [];
      for (const v of vs) {
        const vfields = variantFields(v);
        if (vfields.length === 0) parts.push(`    | ${v.name}`);
        else if (v.fields && 'named' in v.fields) parts.push(`    | ${v.name} of ${vfields.map((f) => `${capitalize(f.name)}: ${p.ty(f.ty)}`).join(' * ')}`);
        else parts.push(`    | ${v.name} of ${vfields.map((f) => p.ty(f.ty)).join(' * ')}`);
      }
      out.push(parts.join('\n'));
      break;
    }
    case 'zig': {
      out.push(`pub const ${name} = union(enum) {`);
      for (const v of vs) {
        const vfields = variantFields(v);
        if (vfields.length === 0) out.push(`    ${v.name},`);
        else out.push(`    ${v.name}: struct { ${vfields.map((f) => `${f.name}: ${p.ty(f.ty)}`).join(', ')} },`);
      }
      out.push('};');
      break;
    }
    case 'nim': {
      out.push('type');
      out.push(`  ${name}Kind* = enum`);
      for (const v of vs) out.push(`    k${v.name}`);
      out.push(`  ${name}* = object`);
      out.push(`    case kind*: ${name}Kind`);
      for (const v of vs) {
        const vfields = variantFields(v);
        if (vfields.length > 0) {
          out.push(`    of k${v.name}:`);
          for (const f of vfields) out.push(`      ${f.name}* : ${p.ty(f.ty)}`);
        }
      }
      out.push('    else:');
      out.push('      discard');
      break;
    }
    case 'julia': {
      out.push(`abstract type ${name} end`);
      for (const v of vs) {
        const vfields = variantFields(v);
        if (vfields.length === 0) out.push(`struct ${v.name} <: ${name} end`);
        else {
          out.push(`struct ${v.name} <: ${name}`);
          for (const f of vfields) out.push(`    ${f.name}::${p.ty(f.ty)}`);
          out.push('end');
        }
      }
      break;
    }
    case 'elixir': {
      out.push(`defmodule ${name} do`);
      out.push(`  defstruct [:kind${vs.some((v) => variantFields(v).length > 0) ? ', ' + [...new Set(vs.flatMap((v) => variantFields(v).map((f) => snakeAtom(f.name))))].join(', ') : ''}]`);
      out.push(`  # 构造器`);
      for (const v of vs) {
        const vfields = variantFields(v);
        const snakeV = snakeAtom(v.name);
        if (vfields.length === 0) {
          out.push(`  def ${snakeV}, do: %__MODULE__{kind: :${snakeV}}`);
        } else {
          out.push(`  def ${snakeV}(${vfields.map((f) => snakeAtom(f.name)).join(', ')}) do`);
          out.push(`    %__MODULE__{kind: :${snakeV}, ${vfields.map((f) => `${snakeAtom(f.name)}: ${snakeAtom(f.name)}`).join(', ')}}`);
          out.push(`  end`);
        }
      }
      out.push('end');
      break;
    }
    case 'erlang': {
      out.push(`%% ${name} — tagged tuple 和类型`);
      for (const v of vs) {
        const vfields = variantFields(v);
        if (vfields.length === 0) out.push(`${snakeUpper(v.name)}() -> {${snakeAtom(v.name)}}.`);  
        else out.push(`${snakeUpper(v.name)}(${vfields.map((f) => `${snakeAtom(f.name)}`).join(', ')}) -> {${snakeAtom(v.name)}, ${vfields.map((f) => snakeAtom(f.name)).join(', ')}}.`);
      }
      break;
    }
    case 'ruby': {
      out.push(`# ${name} — HSL enum 和类型`);
      out.push(`class ${name}; end`);
      for (const v of vs) {
        const vfields = variantFields(v);
        if (vfields.length === 0) {
          out.push(`class ${v.name} < ${name}`);
          out.push('  def self.instance = @instance ||= new');
          out.push('end');
          out.push(`${snakeUpper(v.name)} = ${v.name}.instance`);
        } else {
          out.push(`class ${v.name} < ${name}`);
          out.push(`  ${vfields.map((f) => `attr_accessor :${f.name}`).join(', ')}`);
          out.push(`  def initialize(${vfields.map((f) => f.name).join(', ')})`);
          for (const f of vfields) out.push(`    @${f.name} = ${f.name}`);
          out.push('  end');
          out.push('end');
        }
      }
      break;
    }
    case 'php': {
      out.push(`abstract class ${name} {`);
      out.push(`    final private function __construct() {}`);
      out.push('}');
      for (const v of vs) {
        const vfields = variantFields(v);
        out.push(`final class ${v.name} extends ${name} {`);
        out.push('    public function __construct(');
        for (const f of vfields) out.push(`        public readonly ${p.ty(f.ty)} $${f.name},`);
        out.push('    ) {}');
        out.push('}');
      }
      break;
    }
    case 'lua': {
      out.push(`${name} = {}`);
      for (const v of vs) {
        const vfields = variantFields(v);
        if (vfields.length === 0) {
          out.push(`${name}.${v.name} = { kind = '${v.name}' }`);
        } else {
          out.push(`function ${name}.${v.name}(${vfields.map((f) => f.name).join(', ')})`);
          out.push(`    return { kind = '${v.name}', ${vfields.map((f) => `${f.name} = ${f.name}`).join(', ')} }`);
          out.push('end');
        }
      }
      break;
    }
    case 'dart': {
      out.push(`sealed class ${name} {}`);
      for (const v of vs) {
        const vfields = variantFields(v);
        if (vfields.length === 0) out.push(`class ${v.name} extends ${name} { const ${v.name}(); }`);
        else {
          out.push(`class ${v.name} extends ${name} {`);
          out.push(`  final ${vfields.map((f) => `${p.ty(f.ty)} ${camel(f.name)}`).join('; final ')}`);
          out.push(`  const ${v.name}({${vfields.map((f) => `required this.${camel(f.name)}`).join(', ')}});`);
          out.push('}');
        }
      }
      break;
    }
    case 'crystal': {
      out.push(`abstract class ${name}; end`);
      for (const v of vs) {
        const vfields = variantFields(v);
        out.push(`class ${v.name} < ${name}`);
        for (const f of vfields) out.push(`  property ${f.name} : ${p.ty(f.ty)}`);
        if (vfields.length > 0) {
          out.push(`  def initialize(${vfields.map((f) => `@${f.name}`).join(', ')}); end`);
        }
        out.push('end');
      }
      break;
    }
    default: {
      // 注释契约 + 运行期约定
      out.push(cline(p.lang, `enum ${name}（HSL 和类型）— ${p.lang.name} 契约:`));
      for (const v of vs) {
        const vfields = variantFields(v);
        out.push(cline(p.lang, `  | ${v.name}${vfields.length ? ' { ' + vfields.map((f) => `${f.name}: ${p.ty(f.ty)}`).join(', ') + ' }' : ''}`));
      }
      out.push(...taggedUnionFallback(p, name, vs));
    }
  }
  return out;
}

/** 无静态和类型的语言（perl/r/bash/powershell/objectivec/d/vb/groovy…）：运行期标签约定 */
function taggedUnionFallback(p: P, name: string, vs: A.VariantDef[]): string[] {
  const out: string[] = [];
  const lc = p.lc;
  const mk = `${lower1(name)}_variant`;
  out.push(`${lc} 运行期约定：值携带 kind 标签（字符串）+ 负载字段`);
  const id = p.lang.id;
  if (id === 'powershell') {
    out.push(`function New-${name} {`);
    out.push(`    param([string]$Kind, [hashtable]$Fields = @{})`);
    out.push(`    $Fields['kind'] = $Kind; return [PSCustomObject]$Fields`);
    out.push('}');
    for (const v of vs) {
      const vf = variantFields(v);
      if (vf.length === 0) out.push(`$${snakeUpper(v.name)} = New-${name} -Kind '${v.name}'`);
    }
  } else if (id === 'bash') {
    for (const v of vs) {
      const vf = variantFields(v);
      out.push(`${mk}_${lower1(v.name)}() { echo "kind=${v.name}${vf.map((f) => ` ${f.name}=\$1`).join('')}"; }`);
    }
  } else if (id === 'r') {
    out.push(`${mk} <- function(kind, ...) {`);
    out.push(`  out <- list(...); out$kind <- kind; class(out) <- "${name}"; return(out)`);
    out.push(`}`);
  } else if (id === 'perl') {
    out.push(`sub ${mk} { my (\$kind, %fields) = @_; return bless { kind => \$kind, %fields }, '${name}'; }`);
  } else if (id === 'd') {
    out.push(`struct ${name} {`);
    out.push(`    enum Kind { ${vs.map((v) => lower1(v.name)).join(', ')} }`);
    out.push(`    Kind kind;`);
    const allFields = [...new Set(vs.flatMap((v) => variantFields(v).map((f) => f.name)))];
    for (const f of allFields) out.push(`    // ${f}: 负载字段（按 kind 生效）`);
    out.push('}');
  } else if (id === 'vb') {
    out.push(`' ${name}: kind 标签 + 负载字段的运行期对象`);
  } else if (id === 'objectivec') {
    out.push(`@interface ${name} : NSObject`);
    out.push(`@property (nonatomic, copy) NSString *kind;`);
    out.push('@end');
  }
  return out;
}

/** 变体字段：命名变体直出；元组变体合成 f0/f1… 字段名（保证负载不丢失） */
function variantFields(v: A.VariantDef): A.FieldDef[] {
  if (v.fields && 'named' in v.fields) return v.fields.named;
  if (v.fields && 'tuple' in v.fields) {
    return v.fields.tuple.map((ty, i) => ({ name: `f${i}`, ty, attrs: [] }));
  }
  return [];
}

// ===== trait =====

function traitDecl(p: P, d: A.Item & { kind: 'trait' }, pi: ProjectedItem): string[] {
  const out: string[] = [];
  const name = d.name;
  const methods = d.items.filter((it): it is A.TraitItem & { fn: A.FnDef } => it.kind === 'fn' || (it.kind === 'sig' && !!it.fn));
  switch (p.lang.id) {
    case 'python': {
      out.push(`class ${name}(Protocol):  # HSL trait`);
      if (methods.length === 0) out.push('    pass');
      for (const m of methods) {
        const params = m.fn.params.filter((x) => !x.self).map((x) => `${patternName(x.pat) ?? 'arg'}: ${x.ty ? p.ty(x.ty) : 'Any'}`).join(', ');
        const ret = m.fn.ret ? p.ty(m.fn.ret) : 'None';
        out.push(`    def ${m.fn.name}(self${params ? ', ' + params : ''}) -> ${ret}: ...`);
      }
      break;
    }
    case 'typescript': {
      out.push(`export interface ${name} {`);
      if (methods.length === 0) out.push('  // empty trait');
      for (const m of methods) {
        const params = m.fn.params.filter((x) => !x.self).map((x) => `${patternName(x.pat) ?? 'arg'}: ${x.ty ? p.ty(x.ty) : 'unknown'}`).join(', ');
        const ret = m.fn.ret ? p.ty(m.fn.ret) : 'void';
        out.push(`  ${m.fn.name}(${params}): ${m.fn.isAsync ? 'Promise<' + ret + '>' : ret};`);
      }
      out.push('}');
      break;
    }
    case 'javascript': {
      // JS：JSDoc 契约（运行期鸭子类型）
      out.push(`/** @interface ${name} — HSL trait 契约（JS 运行期鸭子类型）`);
      for (const m of methods) {
        out.push(` * ${m.fn.name}(${m.fn.params.filter((x) => !x.self).map((x) => patternName(x.pat) ?? 'arg').join(', ')})${m.fn.isAsync ? ' [async]' : ''}`);
      }
      out.push(' */');
      break;
    }
    case 'rust': {
      out.push(`pub trait ${name} {`);
      if (methods.length === 0) out.push('}');
      for (const m of methods) {
        const params = m.fn.params.filter((x) => !x.self).map((x) => `${patternName(x.pat) ?? 'arg'}: ${x.ty ? p.ty(x.ty) : '()'}`).join(', ');
        const ret = m.fn.ret ? ` -> ${p.ty(m.fn.ret)}` : '';
        const asyn = m.fn.isAsync ? 'async ' : '';
        out.push(`    ${asyn}fn ${m.fn.name}(&self${params ? ', ' + params : ''})${ret};`);
      }
      if (methods.length > 0) out.push('}');
      break;
    }
    case 'go': {
      out.push(`type ${name} interface {`);
      if (methods.length === 0) out.push('}');
      for (const m of methods) {
        const params = m.fn.params.filter((x) => !x.self).map((x) => `${patternName(x.pat) ?? 'arg'} ${x.ty ? p.ty(x.ty) : 'any'}`).join(', ');
        const rets: string[] = [];
        if (m.fn.ret) rets.push(p.ty(m.fn.ret));
        out.push(`\t${m.fn.name}(${params})${rets.length ? ' ' + rets.join(', ') : ''}`);
      }
      if (methods.length > 0) out.push('}');
      break;
    }
    case 'cpp': {
      out.push(`class ${name} {  // HSL trait → 抽象类`);
      out.push('public:');
      out.push(`    virtual ~${name}() = default;`);
      for (const m of methods) {
        const params = m.fn.params.filter((x) => !x.self).map((x) => `${x.ty ? p.ty(x.ty) : 'auto'} ${patternName(x.pat) ?? 'arg'}`).join(', ');
        const ret = m.fn.ret ? p.ty(m.fn.ret) : 'void';
        out.push(`    virtual ${ret} ${m.fn.name}(${params}) = 0;`);
      }
      out.push('};');
      break;
    }
    case 'java': out.push(`interface ${name} {`, ...methods.map((m) => `    ${m.fn.ret ? p.ty(m.fn.ret) : 'Object'} ${m.fn.name}(${m.fn.params.filter((x) => !x.self).map((x) => `${x.ty ? p.ty(x.ty) : 'Object'} ${patternName(x.pat) ?? 'arg'}`).join(', ')}) throws Exception;`), '}'); break;
    case 'csharp': out.push(`internal interface ${name} {`, ...methods.map((m) => `    ${m.fn.ret ? p.ty(m.fn.ret) : 'void'} ${capitalize(m.fn.name)}(${m.fn.params.filter((x) => !x.self).map((x) => `${x.ty ? p.ty(x.ty) : 'object'} ${patternName(x.pat) ?? 'arg'}`).join(', ')});`), '}'); break;
    case 'kotlin': out.push(`interface ${name} {`, ...methods.map((m) => `    ${m.fn.isAsync ? 'suspend ' : ''}fun ${m.fn.name}(${m.fn.params.filter((x) => !x.self).map((x) => `${patternName(x.pat) ?? 'arg'}: ${x.ty ? p.ty(x.ty) : 'Any'}`).join(', ')})${m.fn.ret ? `: ${p.ty(m.fn.ret)}` : ''}`), '}'); break;
    case 'swift': out.push(`protocol ${name} {`, ...methods.map((m) => `    func ${m.fn.name}(${m.fn.params.filter((x) => !x.self).map((x) => `${patternName(x.pat) ?? 'arg'}: ${x.ty ? p.ty(x.ty) : 'Any'}`).join(', ')})${m.fn.isAsync ? ' async' : ''}${m.fn.ret ? ` -> ${p.ty(m.fn.ret)}` : ''}`), '}'); break;
    case 'scala': out.push(`trait ${name} {`, ...methods.map((m) => `  def ${m.fn.name}(${m.fn.params.filter((x) => !x.self).map((x) => `${patternName(x.pat) ?? 'arg'}: ${x.ty ? p.ty(x.ty) : 'Any'}`).join(', ')})${m.fn.ret ? `: ${p.ty(m.fn.ret)}` : ''}`), '}'); break;
    case 'haskell': {
      out.push(`class ${name} m where`);
      for (const m of methods) {
        out.push(`  ${m.fn.name} :: ${m.fn.ret ? 'm HSLResult' : 'm ()'}  -- 参数见 HSL 源围栏`);
      }
      break;
    }
    case 'ruby': {
      out.push(`module ${name}  # HSL trait`);
      for (const m of methods) {
        const params = m.fn.params.filter((x) => !x.self).map((x) => patternName(x.pat) ?? 'arg').join(', ');
        out.push(`  def ${m.fn.name}(${params}) raise NotImplementedError, '${name}#${m.fn.name}' end`);
      }
      out.push('end');
      break;
    }
    case 'php': out.push(`interface ${name} {`, ...methods.map((m) => `    public function ${m.fn.name}(${m.fn.params.filter((x) => !x.self).map((x) => `${x.ty ? p.ty(x.ty) : 'mixed'} $${patternName(x.pat) ?? 'arg'}`).join(', ')})${m.fn.ret ? `: ${p.ty(m.fn.ret)}` : ''};`), '}'); break;
    case 'dart': {
      out.push(`abstract interface class ${name} {`);
      for (const m of methods) {
        const baseRet = m.fn.ret ? p.ty(m.fn.ret) : 'void';
        const retTy = m.fn.isAsync ? `Future<${baseRet}>` : baseRet;
        const params = m.fn.params.filter((x) => !x.self).map((x) => `${x.ty ? p.ty(x.ty) : 'dynamic'} ${patternName(x.pat) ?? 'arg'}`).join(', ');
        out.push(`  ${retTy} ${camel(m.fn.name)}(${params});`);
      }
      out.push('}');
      break;
    }
    case 'julia': {
      out.push(`abstract type ${name} end`);
      for (const m of methods) out.push(`# ${name}::${m.fn.name} — 由具体类型实现`);
      break;
    }
    case 'crystal': {
      out.push(`module ${name}`);
      for (const m of methods) out.push(`  abstract def ${m.fn.name}(${m.fn.params.filter((x) => !x.self).map((x) => patternName(x.pat) ?? 'arg').join(', ')})`);
      out.push('end');
      break;
    }
    default: {
      out.push(cline(p.lang, `trait ${name}（HSL trait）— 方法契约:`));
      for (const m of methods) out.push(cline(p.lang, `  · ${m.fn.name}(${m.fn.params.filter((x) => !x.self).map((x) => `${patternName(x.pat) ?? 'arg'}: ${x.ty ? p.ty(x.ty) : 'Any'}`).join(', ')})${m.fn.ret ? ' -> ' + p.ty(m.fn.ret) : ''}`));
    }
  }
  return out;
}

// ===== impl =====

function implDecl(p: P, d: A.Item & { kind: 'impl' }, pi: ProjectedItem, fileItems: ProjectedItem[]): string[] {
  const out: string[] = [];
  const typeName = d.typeName;
  const traitNote = d.traitSegs ? ` for ${d.traitSegs.join('::')}` : '';
  switch (p.lang.id) {
    case 'python': {
      // 若 struct 同文件：方法并入类体（由 merge 处理，此处独立文件时输出自由函数）
      for (const m of d.methods) {
        out.push(...fnDecl(p, m, pi, true, typeName));
      }
      break;
    }
    case 'typescript': case 'javascript': {
      for (const m of d.methods) out.push(...fnDecl(p, m, pi, true, typeName));
      break;
    }
    case 'rust': {
      out.push(`impl${traitNote ? ' ' + d.traitSegs!.join('::') + ' for ' : ' '}${typeName} {`);
      for (const m of d.methods) {
        out.push(...fnDecl(p, m, pi, true, typeName).map((l) => '    ' + l));
      }
      out.push('}');
      break;
    }
    case 'go': {
      // 方法接收者
      for (const m of d.methods) {
        const params = m.params.filter((x) => !x.self).map((x) => `${patternName(x.pat) ?? 'arg'} ${x.ty ? p.ty(x.ty) : 'any'}`).join(', ');
        const rets = m.ret ? ` ${goRet(p.ty(m.ret))}` : '';
        out.push(`func (self *${typeName}) ${m.name}(${params})${rets} {`);
        out.push(...tryBody(p, m, pi, typeName));
        out.push('}');
      }
      break;
    }
    case 'cpp': {
      for (const m of d.methods) {
        const params = m.params.filter((x) => !x.self).map((x) => `${x.ty ? p.ty(x.ty) : 'auto'} ${patternName(x.pat) ?? 'arg'}`).join(', ');
        const ret = m.ret ? p.ty(m.ret) : 'void';
        out.push(`${ret} ${typeName}::${m.name}(${params}) {`);
        out.push(...tryBody(p, m, pi, typeName));
        out.push('}');
      }
      break;
    }
    case 'swift': {
      out.push(`extension ${typeName} {`);
      for (const m of d.methods) {
        const params = m.params.filter((x) => !x.self).map((x) => `${patternName(x.pat) ?? 'arg'}: ${x.ty ? p.ty(x.ty) : 'Any'}`).join(', ');
        out.push(`    func ${m.name}(${params})${m.isAsync ? ' async' : ''}${m.ret ? ` -> ${p.ty(m.ret)}` : ''} {`);
        out.push(...tryBody(p, m, pi, typeName).map((l) => '        ' + l));
        out.push('    }');
      }
      out.push('}');
      break;
    }
    case 'kotlin': {
      for (const m of d.methods) {
        const params = m.params.filter((x) => !x.self).map((x) => `${patternName(x.pat) ?? 'arg'}: ${x.ty ? p.ty(x.ty) : 'Any'}`).join(', ');
        out.push(`fun ${typeName}.${m.name}(${params})${m.ret ? `: ${p.ty(m.ret)}` : ''} {`);
        out.push(...tryBody(p, m, pi, typeName));
        out.push('}');
      }
      break;
    }
    case 'ruby': {
      out.push(`class ${typeName}  # HSL impl${traitNote}`);
      for (const m of d.methods) {
        const params = m.params.filter((x) => !x.self).map((x) => patternName(x.pat) ?? 'arg').join(', ');
        out.push(`  def ${m.name}(${params})`);
        out.push(...tryBody(p, m, pi, typeName));
        out.push('  end');
      }
      out.push('end');
      break;
    }
    case 'php': {
      for (const m of d.methods) {
        const params = m.params.filter((x) => !x.self).map((x) => `${x.ty ? p.ty(x.ty) : 'mixed'} $${patternName(x.pat) ?? 'arg'}`).join(', ');
        out.push(`function ${typeName}_${m.name}(${params}) {`);
        out.push(...tryBody(p, m, pi, typeName));
        out.push('}');
      }
      break;
    }
    case 'scala': {
      out.push(`object ${typeName}Ops {  // HSL impl${traitNote}`);
      for (const m of d.methods) {
        const params = [`self: ${typeName}`, ...m.params.filter((x) => !x.self).map((x) => `${patternName(x.pat) ?? 'arg'}: ${x.ty ? p.ty(x.ty) : 'Any'}`)].join(', ');
        out.push(`  def ${m.name}(${params})${m.ret ? `: ${p.ty(m.ret)}` : ''} = {`);
        out.push(...tryBody(p, m, pi, typeName).map((l) => '    ' + l));
        out.push('  }');
      }
      out.push('}');
      break;
    }
    case 'haskell': {
      if (d.traitSegs) {
        out.push(`instance ${d.traitSegs.join('::')} ${typeName} where`);
        for (const m of d.methods) {
          out.push(`  ${m.name} _ = error "dhv: ${typeName}.${m.name}"`);
        }
      } else {
        for (const m of d.methods) out.push(`${snakeLower(typeName)}_${m.name} :: HSLFn`, `${snakeLower(typeName)}_${m.name} = error "dhv: ${typeName}.${m.name}"`);
      }
      break;
    }
    case 'julia': {
      for (const m of d.methods) {
        const params = [`self::${typeName}`, ...m.params.filter((x) => !x.self).map((x) => `${patternName(x.pat) ?? 'arg'}::${x.ty ? p.ty(x.ty) : 'Any'}`)].join(', ');
        out.push(`function ${m.name}(${params})`);
        out.push(...tryBody(p, m, pi, typeName));
        out.push('end');
      }
      break;
    }
    default: {
      out.push(cline(p.lang, `impl ${typeName}${traitNote} — 方法:`));
      for (const m of d.methods) out.push(cline(p.lang, `  · ${m.name}（见围栏 HSL 源）`));
      for (const m of d.methods) {
        out.push(...fnDecl(p, m, pi, false, typeName));
      }
    }
  }
  return out;
}

// ===== fn =====

export function fnDecl(p: P, fn: A.FnDef, pi: ProjectedItem, isMethod: boolean, selfType?: string): string[] {
  const out: string[] = [];
  const L = p.lang.id;
  const { head, ret } = fnHeader(p, fn, { isMethod, selfType });
  const name = isMethod && selfType && ['python', 'typescript', 'javascript'].includes(L) === false && ['java', 'csharp'].includes(L)
    ? fn.name : fn.name;

  // full/logic 语言尝试活体翻译
  let body: string[] | null = null;
  if ((p.lang.body === 'full' || p.lang.body === 'logic') && fn.body) {
    try {
      body = transpileBody(fn, p.lang, {
        ty: p.ty,
        enums: p.ctx.enums,
        strLit: p.strLit,
      }, p.ind);
    } catch (err) {
      if (!(err instanceof TranspileError)) throw err;
      if (process.env.DHV_DEBUG_BODY) console.error(`[body] ${fn.name} -> ${err.message}`);
      body = null;
    }
  }

  const fenceName = isMethod && selfType ? `${selfType}_${fn.name}` : fn.name;

  switch (L) {
    case 'python': {
      out.push(`${head}${ret}:`);
      out.push(...p.fence(pi.item, fenceName, body, p.ind));
      if (body === null) out.push(p.unimpl(p.ind, `${fenceName} 未翻译`));
      else if (fn.ret && !/None|->\s*$/.test(ret)) { /* python 隐式返回尾表达式 */ }
      break;
    }
    case 'typescript': {
      out.push(`export ${head}${ret} {`);
      out.push(...p.fence(pi.item, fenceName, body, p.ind));
      if (body === null) out.push(p.unimpl(p.ind, `${fenceName} 未翻译`));
      out.push('}');
      break;
    }
    case 'javascript': {
      const params = fn.params.filter((x) => !x.self).map((x) => patternName(x.pat) ?? 'arg');
      const allParams = isMethod && selfType ? ['self', ...params] : params;
      out.push(`export ${fn.isAsync ? 'async ' : ''}function ${name}(${allParams.join(', ')}) {`);
      out.push(...p.fence(pi.item, fenceName, body, p.ind));
      if (body === null) out.push(p.unimpl(p.ind, `${fenceName} 未翻译`));
      out.push('}');
      break;
    }
    case 'rust': {
      // v0.2.53 修复（rustc 真机实测）：Rust 的 fn main 签名受 Termination trait 约束
      // （i64 不实现 —— 合法集仅 ()/bool/i32/u8-u32/ExitCode/Result 等）。
      // HSL 入口约定 fn main() -> i64（R-1，无参）投 rust 时改名 hsl_main +
      // 生成进程级 wrapper（exit code = 返回值截断 i32，语义与 interp run 对齐）。
      // 带参数的 main 不属入口形态（R-1 之外），保持原样不触发本规则。
      const isEntry = !isMethod && fn.name === 'main' && !!fn.ret && fn.params.length === 0;
      const head2 = isEntry ? head.replace(/fn main\(/, 'fn hsl_main(') : head;
      out.push(`${head2}${ret} {`);
      out.push(...p.fence(pi.item, fenceName, body, p.ind));
      if (body === null) out.push(p.unimpl(p.ind, `${fenceName} 未翻译`));
      out.push('}');
      if (isEntry) out.push(`fn main() { std::process::exit(hsl_main() as i32); }`);
      break;
    }
    case 'go': {
      out.push(`${head}${ret} {`);
      out.push(...p.fence(pi.item, fenceName, body, p.ind));
      if (body === null) {
        out.push(p.unimpl(p.ind, `${fenceName} 未翻译`));
      } else if (fn.ret && !goBodyEndsWithReturn(body)) {
        // v1.4.10（真机 go vet 实测）：body 尾已是 return 时再补零值兜底 = unreachable code。
        // 仅当尾不是 return（如尾表达式为 if 链翻译为分支 return 之外的场景）才补兜底。
        // 注：if/else 全分支 return 的尾链场景仍会补（go 编译器接受，vet 警告为已知限制）
        out.push('\treturn' + ` ${goZero(p.ty(fn.ret))}`);
      } else if (!fn.ret && !goBodyEndsWithReturn(body)) {
        out.push('\treturn');
      }
      out.push('}');
      break;
    }
    case 'cpp': {
      out.push(`${head} {`);
      out.push(...p.fence(pi.item, fenceName, body, p.ind));
      if (body === null) out.push(p.unimpl(p.ind, `${fenceName} 未翻译`));
      out.push('}');
      break;
    }
    case 'swift': case 'kotlin': case 'scala': case 'julia': case 'crystal': case 'dart': case 'groovy': {
      if (L === 'scala') {
        out.push(`${head}${ret} = {`);
        out.push(...p.fence(pi.item, fenceName, null, p.ind));
        out.push(p.unimpl(p.ind, `${fenceName} 未翻译`));
        out.push('}');
      } else if (L === 'kotlin') {
        out.push(`${head}${ret} {`);
        out.push(...p.fence(pi.item, fenceName, null, p.ind));
        out.push(p.unimpl(p.ind, `${fenceName} 未翻译`));
        out.push('}');
      } else if (L === 'dart') {
        const retTy = fn.ret ? p.ty(fn.ret) : 'dynamic';
        const params = fn.params.filter((x) => !x.self).map((x) => `${x.ty ? p.ty(x.ty) : 'dynamic'} ${patternName(x.pat) ?? 'arg'}`).join(', ');
        out.push(`${retTy} ${fn.name}(${params}) {`);
        out.push(...p.fence(pi.item, fenceName, null, p.ind));
        out.push(p.unimpl(p.ind, `${fenceName} 未翻译`));
        out.push('}');
      } else {
        out.push(`${head}${ret} {`);
        out.push(...p.fence(pi.item, fenceName, null, p.ind));
        out.push(p.unimpl(p.ind, `${fenceName} 未翻译`));
        out.push('}');
      }
      break;
    }
    case 'ruby': {
      out.push(`${head}`);
      out.push(...p.fence(pi.item, fenceName, null, p.ind));
      out.push(p.unimpl(p.ind, `${fenceName} 未翻译`));
      out.push('end');
      break;
    }
    case 'php': {
      out.push(`${head}${ret} {`);
      out.push(...p.fence(pi.item, fenceName, null, p.ind));
      out.push(p.unimpl(p.ind, `${fenceName} 未翻译`));
      out.push('}');
      break;
    }
    case 'lua': {
      const params = fn.params.filter((x) => !x.self).map((x) => patternName(x.pat) ?? 'arg').join(', ');
      out.push(`function ${fn.name}(${params})`);
      out.push(...p.fence(pi.item, fenceName, null, p.ind));
      out.push(p.unimpl(p.ind, `${fenceName} 未翻译`));
      out.push('end');
      break;
    }
    case 'perl': {
      out.push(head);
      out.push(`    my (${fn.params.filter((x) => !x.self).map((x) => '$' + (patternName(x.pat) ?? 'arg')).join(', ')}) = @_;`);
      out.push(...p.fence(pi.item, fenceName, null, p.ind));
      out.push(p.unimpl(p.ind, `${fenceName} 未翻译`));
      out.push('}');
      break;
    }
    case 'bash': {
      out.push(head);
      out.push(...p.fence(pi.item, fenceName, null, '    '));
      out.push(p.unimpl('    ', `${fenceName} 未翻译`));
      out.push('}');
      break;
    }
    case 'powershell': {
      out.push(head);
      out.push(`    param(${fn.params.filter((x) => !x.self).map((x) => `[${x.ty ? p.ty(x.ty) : 'object'}]$${patternName(x.pat) ?? 'arg'}`).join(', ')})`);
      out.push(...p.fence(pi.item, fenceName, null, '    '));
      out.push(p.unimpl('    ', `${fenceName} 未翻译`));
      out.push('}');
      break;
    }
    case 'r': {
      out.push(`${head} {`);
      out.push(...p.fence(pi.item, fenceName, null, '  '));
      out.push(p.unimpl('  ', `${fenceName} 未翻译`));
      out.push('}');
      break;
    }
    case 'elixir': {
      out.push(head);
      out.push(...p.fence(pi.item, fenceName, null, '  '));
      out.push(p.unimpl('  ', `${fenceName} 未翻译`));
      out.push('end');
      break;
    }
    case 'erlang': {
      out.push(`${snakeLower(name)}(${fn.params.filter((x) => !x.self).map((x) => snakeLower(patternName(x.pat) ?? 'Arg')).join(', ')}) ->`);
      out.push(...p.fence(pi.item, fenceName, null, ''));
      out.push(p.unimpl('    ', `${fenceName} 未翻译`));
      break;
    }
    case 'haskell': {
      out.push(`-- ${fn.name} :: ${fn.ret ? p.ty(fn.ret) : 'IO ()'}（${fn.name} ${fn.params.map((x) => patternName(x.pat) ?? 'arg').join(' ')}）`);
      out.push(`${snakeLower(fn.name)} :: HSLFn`);
      out.push(`${snakeLower(fn.name)} = error "dhv: ${fenceName} 未翻译"`);
      out.push(...p.fence(pi.item, fenceName, null, ''));
      break;
    }
    case 'ocaml': {
      const params = fn.params.filter((x) => !x.self).map((x) => `(${patternName(x.pat) ?? 'arg'} : ${x.ty ? p.ty(x.ty) : 'obj'})`).join(' ');
      out.push(`let ${fn.name} ${params} =`);
      out.push(...p.fence(pi.item, fenceName, null, '  '));
      out.push(p.unimpl('  ', `${fenceName} 未翻译`));
      break;
    }
    case 'fsharp': {
      const params = fn.params.filter((x) => !x.self).map((x) => `(${patternName(x.pat) ?? 'arg'} : ${x.ty ? p.ty(x.ty) : 'obj'})`).join(' ');
      out.push(`let ${fn.name} ${params} =`);
      out.push(...p.fence(pi.item, fenceName, null, '    '));
      out.push(p.unimpl('    ', `${fenceName} 未翻译`));
      break;
    }
    case 'zig': {
      out.push(`pub fn ${fn.name}(${fn.params.filter((x) => !x.self).map((x) => `${patternName(x.pat) ?? 'arg'}: ${x.ty ? p.ty(x.ty) : 'anytype'}`).join(', ')})${fn.ret ? ` ${p.ty(fn.ret)}` : ' void'} {`);
      out.push(...p.fence(pi.item, fenceName, null, '    '));
      out.push(p.unimpl('    ', `${fenceName} 未翻译`));
      out.push('}');
      break;
    }
    case 'nim': {
      out.push(`proc ${fn.name}*(${fn.params.filter((x) => !x.self).map((x) => `${patternName(x.pat) ?? 'arg'}: ${x.ty ? p.ty(x.ty) : 'auto'}`).join(', ')})${fn.ret ? `: ${p.ty(fn.ret)}` : ''} =`);
      out.push(...p.fence(pi.item, fenceName, null, '  '));
      out.push(p.unimpl('  ', `${fenceName} 未翻译`));
      break;
    }
    case 'java': {
      const retTy = fn.ret ? p.ty(fn.ret) : 'Object';
      const params = fn.params.filter((x) => !x.self).map((x) => `${x.ty ? p.ty(x.ty) : 'Object'} ${patternName(x.pat) ?? 'arg'}`).join(', ');
      out.push(`static ${fn.isAsync ? 'java.util.concurrent.CompletableFuture<' + retTy + '>' : retTy} ${fn.name}(${params}) throws Exception {`);
      out.push(...p.fence(pi.item, fenceName, null, '    '));
      out.push(p.unimpl('    ', `${fenceName} 未翻译`));
      out.push('}');
      break;
    }
    case 'csharp': {
      const retTy = fn.ret ? p.ty(fn.ret) : 'void';
      const params = fn.params.filter((x) => !x.self).map((x) => `${x.ty ? p.ty(x.ty) : 'object'} ${patternName(x.pat) ?? 'arg'}`).join(', ');
      out.push(`public static ${fn.isAsync ? 'async System.Threading.Tasks.Task<' + retTy + '>' : retTy} ${capitalize(fn.name)}(${params}) {`);
      out.push(...p.fence(pi.item, fenceName, null, '    '));
      out.push(p.unimpl('    ', `${fenceName} 未翻译`));
      out.push('}');
      break;
    }
    case 'objectivec': {
      const retTy = fn.ret ? p.ty(fn.ret) : 'void';
      const params = fn.params.filter((x) => !x.self).map((x) => `${x.ty ? p.ty(x.ty) : 'id'} ${patternName(x.pat) ?? 'arg'}`).join(' ');
      out.push(`static ${retTy} ${fn.name}(${params}) {`);
      out.push(...p.fence(pi.item, fenceName, null, '    '));
      out.push(p.unimpl('    ', `${fenceName} 未翻译`));
      out.push('}');
      break;
    }
    case 'd': case 'groovy': {
      const retTy = fn.ret ? p.ty(fn.ret) : (L === 'groovy' ? 'def' : 'void');
      const params = fn.params.filter((x) => !x.self).map((x) => `${x.ty ? p.ty(x.ty) : 'auto'} ${patternName(x.pat) ?? 'arg'}`).join(', ');
      out.push(`${retTy} ${fn.name}(${params}) {`);
      out.push(...p.fence(pi.item, fenceName, null, '    '));
      out.push(p.unimpl('    ', `${fenceName} 未翻译`));
      out.push('}');
      break;
    }
    case 'vb': {
      const retTy = fn.ret ? p.ty(fn.ret) : 'Object';
      const params = fn.params.filter((x) => !x.self).map((x) => `ByVal ${patternName(x.pat) ?? 'arg'} As ${x.ty ? p.ty(x.ty) : 'Object'}`).join(', ');
      out.push(`Function ${capitalize(fn.name)}(${params}) As ${retTy}`);
      out.push(...p.fence(pi.item, fenceName, null, '    '));
      out.push(p.unimpl('    ', `${fenceName} 未翻译`));
      out.push('End Function');
      break;
    }
    default: {
      out.push(cline(p.lang, `fn ${fn.name} — 见围栏 HSL 源`));
      out.push(...p.contractFence(pi.item, fenceName, ''));
    }
  }
  void name;
  return out;
}

function goZero(ty: string): string {
  if (ty === 'string') return '""';
  if (/^(int|uint|float|size)/.test(ty) || /^(int|uint|float)\d+/.test(ty)) return '0';
  if (ty === 'bool') return 'false';
  if (ty.startsWith('*')) return 'nil';
  if (ty.startsWith('[]') || ty.startsWith('map[')) return 'nil';
  return 'nil';
}

/** impl 方法体（contract 语言统一走 fnDecl 的 fence 路径） */
function tryBody(p: P, fn: A.FnDef, pi: ProjectedItem, selfType: string): string[] {
  if ((p.lang.body === 'full' || p.lang.body === 'logic') && fn.body) {
    try {
      const lines = transpileBody(fn, p.lang, { ty: p.ty, enums: p.ctx.enums, strLit: p.strLit }, p.ind);
      return p.fence(pi.item, `${selfType}_${fn.name}`, lines, p.ind);
    } catch {
      /* fallthrough */
    }
  }
  return p.fence(pi.item, `${selfType}_${fn.name}`, null, p.ind);
}

// ===== const / typealias =====

function constDecl(p: P, d: A.Item & { kind: 'const' }, pi: ProjectedItem): string[] {
  const out: string[] = [];
  const val = exprLitText(d.value);
  const ty = d.ty ? p.ty(d.ty) : '';
  switch (p.lang.id) {
    case 'python': out.push(`${snakeUpper(d.name)} = ${val ?? 'None'}  # ${ty}`); break;
    case 'typescript': out.push(`export const ${snakeUpper(d.name)} = ${val ?? 'undefined'} as any;`); break;
    case 'javascript': out.push(`export const ${snakeUpper(d.name)} = ${val ?? 'undefined'};`); break;
    case 'rust': out.push(`pub const ${d.name.toUpperCase()}: ${ty || 'i64'} = ${val ?? '0'};`); break;
    case 'go': out.push(`const ${d.name} = ${val ?? 'nil'}  // ${ty}`); break;
    case 'cpp': out.push(`static const ${ty || 'int64_t'} ${d.name} = ${val ?? '0'};`); break;
    case 'java': out.push(`static final ${ty || 'Object'} ${d.name.toUpperCase()} = ${val ?? 'null'};`); break;
    case 'csharp': out.push(`internal const ${ty || 'object'} ${d.name} = ${val ?? 'null'};`); break;
    case 'kotlin': out.push(`const val ${d.name.toUpperCase()}: ${ty || 'Any'} = ${val ?? 'TODO()'};`); break;
    case 'swift': out.push(`let ${snakeUpper(d.name)}: ${ty || 'Any'} = ${val ?? 'TODO()'}`); break;
    case 'haskell': out.push(`${snakeLower(d.name)} :: ${ty || 'HSLVal'}`); out.push(`${snakeLower(d.name)} = ${val ?? 'undefined'}`); break;
    case 'erlang': out.push(`-define(${snakeUpper(d.name)}, ${val ?? 'undefined'}).`); break;
    case 'elixir': out.push(`@${snakeLower(d.name)} ${val ?? 'nil'}  # ${ty}`); break;
    case 'vb': out.push(`Const ${d.name} As ${ty || 'Object'} = ${val ?? 'Nothing'}`); break;
    case 'lua': out.push(`${d.name} = ${val ?? 'nil'}  -- ${ty}`); break;
    case 'ruby': out.push(`${snakeUpper(d.name)} = ${val ?? 'nil'}  # ${ty}`); break;
    case 'php': out.push(`const ${d.name.toUpperCase()} = ${val ?? 'null'};`); break;
    default: out.push(cline(p.lang, `const ${d.name}: ${ty} = ${val ?? '?'}（契约）`));
  }
  return out;
}

function typealiasDecl(p: P, d: A.Item & { kind: 'typealias' }, pi: ProjectedItem): string[] {
  const out: string[] = [];
  const ty = p.ty(d.value);
  switch (p.lang.id) {
    case 'python': out.push(`${d.name} = ${ty}  # type alias`); break;
    case 'typescript': out.push(`export type ${d.name} = ${ty};`); break;
    case 'javascript': out.push(`// typealias ${d.name} = ${ty}`); break;
    case 'rust': out.push(`pub type ${d.name} = ${ty};`); break;
    case 'go': out.push(`type ${d.name} = ${ty}`); break;
    case 'cpp': out.push(`using ${d.name} = ${ty};`); break;
    case 'csharp': out.push(`using ${d.name} = ${ty};`); break;
    case 'kotlin': out.push(`typealias ${d.name} = ${ty}`); break;
    case 'swift': out.push(`typealias ${d.name} = ${ty}`); break;
    case 'scala': out.push(`type ${d.name} = ${ty}`); break;
    case 'ocaml': case 'fsharp': out.push(`type ${lower1(d.name)} = ${ty}`); break;
    case 'haskell': out.push(`type ${d.name} = ${ty}`); break;
    case 'julia': out.push(`const ${d.name} = ${ty}`); break;
    case 'dart': out.push(`typedef ${d.name} = ${ty};`); break;
    case 'zig': out.push(`pub const ${d.name} = ${ty};`); break;
    case 'nim': out.push(`type ${d.name}* = ${ty}`); break;
    case 'd': out.push(`alias ${d.name} = ${ty};`); break;
    case 'vb': out.push(`' typealias ${d.name} = ${ty}`); break;
    default: out.push(cline(p.lang, `typealias ${d.name} = ${ty}`));
  }
  return out;
}

// ===== graph 脚手架 =====

function graphDecl(p: P, d: A.Item & { kind: 'graph' }, pi: ProjectedItem): string[] {
  const out: string[] = [];
  const g = d.graph;
  const nodes = g.body.filter((s) => s.t === 'node').map((s) => s.decl as A.NodeDecl);
  const edges = g.body.filter((s) => s.t === 'edge').map((s) => s.decl as A.EdgeDecl);
  const hasLoop = g.body.some((s) => s.t === 'stmt' && s.stmt.kind === 'expr' && s.stmt.expr.kind === 'loop');
  const lg = p.lang;

  out.push(cline(lg, ''));
  out.push(cline(lg, `@dhv:generated — graph ${g.name} 脚手架（${p.ctx.scale} 尺度）· 不可手改`));
  out.push(cline(lg, `拓扑：${nodes.map((n) => n.name).join(', ') || '(无节点)'}`));
  for (const e of edges) {
    const guard = e.guardExpr ? '（guard）' : '';
    out.push(cline(lg, `边：${e.endpoints.join(' -> ')}${guard}`));
  }
  out.push(cline(lg, `AgentLoop：${hasLoop ? '✓（编译期强制 match 全分支）' : '✗ G-1 违规'}`));
  out.push(cline(lg, ''));

  // 尺度形态
  if (p.ctx.scale === 'microkernel') {
    out.push(cline(p.lang, 'microkernel：节点 → Plugin 实现，边 → 事件总线订阅'));
    out.push(...microkernelScaffold(p, g.name, nodes));
  } else {
    out.push(cline(p.lang, 'monolith：节点 → 函数，边 → 直接调用'));
    out.push(...monolithScaffold(p, g.name, nodes));
  }
  // 围栏 HSL 全文
  out.push('');
  out.push(...p.contractFence(pi.item, `graph_${g.name}`, ''));
  return out;
}

function monolithScaffold(p: P, name: string, nodes: A.NodeDecl[]): string[] {
  const out: string[] = [];
  const L = p.lang.id;
  const fnName = `${snakeLower(name)}_run`;
  if (L === 'python') {
    out.push(`def ${fnName}():`);
    out.push(`    ${cline(p.lang, '节点实例化（monolith：局部变量）')}`);
    for (const n of nodes) out.push(`    ${n.name} = None  # ${p.ty(n.ty)}`);
    out.push(`    while True:`);
    out.push(`        ${cline(p.lang, 'AgentLoop：match Action — 全分支处理（S-6）')}`);
    out.push(`        break`);
  } else if (L === 'typescript' || L === 'javascript') {
    out.push(`export function ${fnName}(): void {`);
    for (const n of nodes) out.push(`  let ${n.name}: any = null; // ${p.ty(n.ty)}`);
    out.push(`  while (true) {`);
    out.push(`    // AgentLoop：match Action — 全分支处理（S-6）`);
    out.push(`    break;`);
    out.push(`  }`);
    out.push(`}`);
  } else if (L === 'rust') {
    out.push(`pub fn ${fnName}() {`);
    for (const n of nodes) out.push(`    let mut ${n.name}: ${p.ty(n.ty)} = todo!();`);
    out.push(`    loop {`);
    out.push(`        // AgentLoop：match Action — 全分支处理（S-6）`);
    out.push(`        break;`);
    out.push(`    }`);
    out.push(`}`);
  } else if (L === 'go') {
    out.push(`func ${fnName}() {`);
    for (const n of nodes) out.push(`\tvar ${n.name} ${p.ty(n.ty)}`);
    out.push(`\tfor {`);
    out.push(`\t\t_ = ${nodes[0]?.name ?? '_'} // AgentLoop`);
    out.push(`\t\tbreak`);
    out.push(`\t}`);
    out.push(`}`);
  } else if (L === 'cpp') {
    out.push(`void ${fnName}() {`);
    for (const n of nodes) out.push(`    ${p.ty(n.ty)} ${n.name};`);
    out.push(`    while (true) {`);
    out.push(`        // AgentLoop：match Action — 全分支处理（S-6）`);
    out.push(`        break;`);
    out.push(`    }`);
    out.push(`}`);
  } else {
    out.push(cline(p.lang, `${fnName}()：节点局部化 + while 循环 + 全分支 match（见围栏 HSL）`));
  }
  return out;
}

function microkernelScaffold(p: P, name: string, nodes: A.NodeDecl[]): string[] {
  const out: string[] = [];
  const L = p.lang.id;
  const regName = `${snakeLower(name)}_plugins`;
  if (L === 'python') {
    out.push(`${regName} = {}  # 事件总线注册表`);
    for (const n of nodes) out.push(`${regName}['${n.name}'] = None  # Plugin 注入位`);
    out.push(`def ${snakeLower(name)}_run():`);
    out.push(`    while True:`);
    out.push(`        ${cline(p.lang, '事件总线驱动 AgentLoop（边 = 订阅）')}`);
    out.push(`        break`);
  } else if (L === 'typescript' || L === 'javascript') {
    out.push(`export const ${regName} = new Map<string, unknown>();`);
    for (const n of nodes) out.push(`${regName}.set('${n.name}', null); // Plugin 注入位`);
    out.push(`export function ${snakeLower(name)}_run(): void {`);
    out.push(`  while (true) {`);
    out.push(`    // 事件总线驱动 AgentLoop（边 = 订阅）`);
    out.push(`    break;`);
    out.push(`  }`);
    out.push(`}`);
  } else if (L === 'rust') {
    out.push(`pub struct ${name}Plugin {`);
    out.push(`    pub name: String,`);
    out.push(`    pub on_event: fn(evt: &str, payload: &str) -> String,`);
    out.push(`}`);
    out.push(`pub fn ${snakeLower(name)}_run() {`);
    out.push(`    let mut bus: Vec<${name}Plugin> = Vec::new();`);
    for (const n of nodes) out.push(`    bus.push(${name}Plugin { name: "${n.name}".into(), on_event: todo!() });`);
    out.push(`    loop {`);
    out.push(`        // 事件总线驱动 AgentLoop`);
    out.push(`        break;`);
    out.push(`    }`);
    out.push(`}`);
  } else if (L === 'go') {
    out.push(`type ${name}Plugin interface {`);
    out.push(`\tName() string`);
    out.push(`\tOnEvent(evt string, payload any) any`);
    out.push(`}`);
    out.push(`var ${regName} = map[string]${name}Plugin{}`);
    for (const n of nodes) out.push(`// ${regName}["${n.name}"] = /* Plugin 注入位 */`);
  } else {
    out.push(cline(p.lang, `事件总线 + Plugin 注册表（${nodes.map((n) => n.name).join(', ')}）`));
  }
  return out;
}

// ---------------------------------------------------------------------------
// 小工具
// ---------------------------------------------------------------------------

function exprLitText(e: A.Expr): string | undefined {
  if (e.kind === 'lit') {
    if (e.lit.t === 'int' || e.lit.t === 'float') return String(e.lit.v);
    if (e.lit.t === 'bool') return e.lit.v ? 'true' : 'false';
    if (e.lit.t === 'str') return JSON.stringify(e.lit.v);
  }
  return undefined;
}

export function capitalize(s: string): string {
  return s ? s[0]!.toUpperCase() + s.slice(1) : s;
}
export function lower1(s: string): string {
  return s ? s[0]!.toLowerCase() + s.slice(1) : s;
}
export function camel(s: string): string {
  return s.replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase());
}
function snakeUpper(s: string): string {
  return s.replace(/([a-z0-9])([A-Z])/g, '$1_$2').toUpperCase();
}
function snakeLower(s: string): string {
  return s.replace(/([a-z0-9])([A-Z])/g, '$1_$2').toLowerCase();
}
function lowerSnake(s: string): string {
  return s.replace(/([a-z0-9])([A-Z])/g, '$1_$2').toLowerCase();
}
function snakeAtom(s: string): string {
  return s.replace(/([a-z0-9])([A-Z])/g, '$1_$2').toLowerCase();
}
function haskellField(struct: string, field: string): string {
  return `_f_${snakeLower(struct)}_${snakeLower(field)}`;
}
