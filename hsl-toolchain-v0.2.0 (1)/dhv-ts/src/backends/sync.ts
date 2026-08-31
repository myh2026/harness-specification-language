// ============================================================================
// dhv-ts/src/backends/sync.ts — 双向工程：生成文件 → HSL 源码回写（总纲 §6）
// ----------------------------------------------------------------------------
// 围栏协议（与 decls.ts fence() 对齐）：
//   {lc} @dhv:source-map: <module>:<line>, block: <name> [(live)]
//   [活体翻译区（live 模式）—— 内核生成，sync 忽略]
//   {lc} @dhv:hsl-mirror — HSL 源镜像（编辑此区后 dhv sync 回写源码）
//   {lc} <HSL 源码行>            ← 可编辑区（回写依据）
//   {lc} @dhv:end-source-map
// 回写：镜像内容 ≠ .hsl 中该项源码 → 替换 .hsl 对应行区间 → 重新语法校验
// ============================================================================

import * as fs from 'node:fs';
import * as path from 'node:path';
import { parseHslFile } from '../linker';

export interface SyncBlock {
  module: string;      // 相对路径（围栏内记录）
  line: number;
  block: string;
  mirror: string[];    // 围栏内 HSL 镜像行（已去注释前缀）
}

export interface SyncResult {
  file: string;
  blocks: SyncBlock[];
  written: { module: string; block: string }[];
  errors: string[];
}

const OPEN_RE = /@dhv:source-map:\s*(.+?):(\d+),\s*block:\s*([\w:]+)/;
const MIRROR_RE = /@dhv:hsl-mirror/;
const CLOSE_RE = /@dhv:end-source-map/;
const COMMENT_RE = /^\s*(?:#|--|\/\/|%|'|\(?\*|<!--|;)\s?/;

export function extractFences(text: string): SyncBlock[] {
  const lines = text.split('\n');
  const blocks: SyncBlock[] = [];
  let i = 0;
  while (i < lines.length) {
    const m = OPEN_RE.exec(lines[i]!);
    if (!m) { i++; continue; }
    const module = m[1]!;
    const line = parseInt(m[2]!, 10);
    const block = m[3]!;
    // 找 mirror 标记与 close
    let j = i + 1;
    let mirrorStart = -1;
    let close = -1;
    while (j < lines.length) {
      if (MIRROR_RE.test(lines[j]!)) { mirrorStart = j + 1; }
      if (CLOSE_RE.test(lines[j]!)) { close = j; break; }
      j++;
    }
    if (close < 0 || mirrorStart < 0) { i++; continue; }
    const mirror: string[] = [];
    for (let k = mirrorStart; k < close; k++) {
      let raw = lines[k]!;
      raw = raw.replace(COMMENT_RE, '');
      // OCaml `(* ... *)` 行注释尾缀
      raw = raw.replace(/\s*\*\)\s*$/, '');
      mirror.push(raw);
    }
    // 去尾部空行
    while (mirror.length > 0 && mirror[mirror.length - 1]!.trim() === '') mirror.pop();
    blocks.push({ module, line, block, mirror });
    i = close + 1;
  }
  return blocks;
}

/**
 * 回写：将围栏镜像写回 .hsl 源码。
 * @param generatedFile 生成的物理文件路径
 * @param root 模块路径解析根（围栏内 module 为相对路径）
 */
export function syncFile(generatedFile: string, root: string): SyncResult {
  const text = fs.readFileSync(generatedFile, 'utf-8');
  const blocks = extractFences(text);
  const written: { module: string; block: string }[] = [];
  const errors: string[] = [];

  const byModule = new Map<string, SyncBlock[]>();
  for (const b of blocks) {
    const arr = byModule.get(b.module) ?? [];
    arr.push(b);
    byModule.set(b.module, arr);
  }

  for (const [module, mods] of byModule) {
    const abs = path.resolve(root, module);
    if (!fs.existsSync(abs)) {
      errors.push(`模块文件不存在：${module}（root=${root}）`);
      continue;
    }
    let srcLines: string[];
    try {
      // 回写前先确认文件本身可解析（防手改坏文件）
      parseHslFile(abs);
      srcLines = fs.readFileSync(abs, 'utf-8').split('\n');
    } catch (err) {
      errors.push(`${module} 当前不可解析，拒绝回写：${(err as Error).message}`);
      continue;
    }

    // 解析当前 AST 拿项行界
    let ast;
    try {
      ast = parseHslFile(abs);
    } catch (err) {
      errors.push(`${module} 解析失败：${(err as Error).message}`);
      continue;
    }
    const sorted = [...ast.items].filter((it) => it.span?.line).sort((a, b) => a.span.line - b.span.line);
    const nameOf = (it: typeof sorted[number]): string =>
      it.kind === 'fn' ? it.fn.name : it.kind === 'graph' ? it.graph.name : it.kind === 'impl' ? it.typeName : (it as { name?: string }).name ?? '';

    // 按行号降序替换（避免行号漂移）
    const edits = [...mods].sort((a, b) => b.line - a.line);
    for (const b of edits) {
      // 找到对应项（优先行号附近，其次名字）
      let idx = sorted.findIndex((it) => nameOf(it) === b.block || nameOf(it) === b.block.replace(/^[^:]+_/, ''));
      // impl 方法围栏名为 Type_method → 匹配 impl Type 块
      if (idx < 0 && b.block.includes('_')) {
        const tyName = b.block.split('_')[0]!;
        idx = sorted.findIndex((it) => it.kind === 'impl' && it.typeName === tyName);
      }
      if (idx < 0) {
        errors.push(`${module} 中找不到围栏目标项 "${b.block}"`);
        continue;
      }
      const item = sorted[idx]!;
      const start = item.span.line - 1;
      const end = idx + 1 < sorted.length ? sorted[idx + 1]!.span.line - 1 : srcLines.length;
      const current = srcLines.slice(start, end);
      while (current.length > 1 && current[current.length - 1]!.trim() === '') current.pop();
      const curText = current.join('\n').trimEnd();
      const newText = b.mirror.join('\n').trimEnd();
      if (curText === newText) continue; // 无改动
      // 替换（保留原区间末尾换行结构）
      const replacement = [...b.mirror];
      srcLines.splice(start, end - start, ...replacement);
      written.push({ module, block: b.block });
    }

    if (written.some((w) => w.module === module)) {
      let newSrc = srcLines.join('\n');
      if (!newSrc.endsWith('\n')) newSrc += '\n';
      // 回写校验：新源码必须仍可解析（总纲 §6：违反即拒绝）
      try {
        fs.writeFileSync(abs, newSrc, 'utf-8');
        parseHslFile(abs);
      } catch (err) {
        // 回滚
        fs.writeFileSync(abs, srcLines.join('\n'), 'utf-8');
        errors.push(`${module} 回写后校验失败，已回滚：${(err as Error).message}`);
        for (let k = written.length - 1; k >= 0; k--) {
          if (written[k]!.module === module) written.splice(k, 1);
        }
      }
    }
  }

  return { file: generatedFile, blocks, written, errors };
}
