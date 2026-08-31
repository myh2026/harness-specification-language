// ============================================================================
// dhv-ts/src/linker.ts — 模块链接器（文件即模块，M1-M5）
// ============================================================================

import * as fs from 'node:fs';
import * as path from 'node:path';
import { Lexer } from './lexer';
import { Parser } from './parser';
import { expandMacros } from './macro';
import { isStdPath } from './std';
import * as A from './ast';

export interface LoadedProgram {
  entry: string;
  order: string[]; // 模块加载顺序
  files: Map<string, A.File>;
}

export class LinkError extends Error {
  constructor(msg: string, public file?: string) {
    super(msg);
  }
}

export function parseHslFile(absPath: string): A.File {
  const src = fs.readFileSync(absPath, 'utf-8');
  const toks = new Lexer(src, absPath).tokenize();
  const expanded = expandMacros(toks, absPath);
  return new Parser(expanded, absPath).parseFile();
}

export function loadProgram(entryFile: string): LoadedProgram {
  const entry = path.resolve(entryFile);
  if (!fs.existsSync(entry)) throw new LinkError(`入口文件不存在：${entry}`);
  const files = new Map<string, A.File>();
  const order: string[] = [];
  const loading = new Set<string>();

  const load = (abs: string): void => {
    if (files.has(abs)) return;
    if (loading.has(abs)) return; // 循环 import：允许（按引用补全）
    loading.add(abs);
    const ast = parseHslFile(abs);
    files.set(abs, ast);
    order.push(abs);
    for (const item of ast.items) {
      if (item.kind === 'import') {
        // 标准库虚拟模块：不触文件系统
        if (isStdPath(item.path)) continue;
        const base = path.dirname(abs);
        const resolved = path.resolve(base, item.path);
        if (!fs.existsSync(resolved)) {
          throw new LinkError(`import 路径不存在："${item.path}"（from ${abs}）`, abs);
        }
        load(resolved);
      }
    }
    loading.delete(abs);
  };

  load(entry);
  return { entry, order, files };
}
