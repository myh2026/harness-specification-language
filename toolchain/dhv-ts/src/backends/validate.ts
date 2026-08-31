// ============================================================================
// dhv-ts/src/backends/validate.ts — 生成文件交叉语法校验（Lint 第 2 层）
// ----------------------------------------------------------------------------
// 用宿主真实工具链验证生成文件的语法合法性：
//   python3 -m py_compile   → python
//   Bun.Transpiler          → typescript / javascript（进程内）
//   bash -n                 → bash
//   平衡检查（启发式）        → 其余语言（括号/引号平衡，报告为 heuristic）
// ============================================================================

import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import * as fs from 'node:fs';

const execFileP = promisify(execFile);

export interface ValidationResult {
  ok: boolean;
  tool: string;
  detail?: string;
}

export async function validateGeneratedFile(absPath: string, langId: string): Promise<ValidationResult> {
  try {
    switch (langId) {
      case 'python': {
        await execFileP('python3', ['-m', 'py_compile', absPath], { timeout: 15_000 });
        return { ok: true, tool: 'python3 -m py_compile' };
      }
      case 'typescript': {
        const code = fs.readFileSync(absPath, 'utf-8');
        const t = new Bun.Transpiler({ loader: 'ts' });
        t.transformSync(code);
        return { ok: true, tool: 'bun transpiler (ts)' };
      }
      case 'javascript': {
        const code = fs.readFileSync(absPath, 'utf-8');
        const t = new Bun.Transpiler({ loader: 'js' });
        t.transformSync(code);
        return { ok: true, tool: 'bun transpiler (js)' };
      }
      case 'bash': {
        await execFileP('bash', ['-n', absPath], { timeout: 10_000 });
        return { ok: true, tool: 'bash -n' };
      }
      default: {
        const code = fs.readFileSync(absPath, 'utf-8');
        const bal = balanceCheck(code, langId);
        return bal.ok
          ? { ok: true, tool: 'heuristic:balanced' }
          : { ok: false, tool: 'heuristic:balanced', detail: bal.detail };
      }
    }
  } catch (err) {
    const e = err as { stderr?: string; message?: string };
    const detail = (e.stderr?.trim() || e.message || '').split('\n').slice(0, 5).join(' | ');
    return { ok: false, tool: toolName(langId), detail: detail.slice(0, 400) };
  }
}

function toolName(langId: string): string {
  switch (langId) {
    case 'python': return 'python3 -m py_compile';
    case 'typescript': return 'bun transpiler (ts)';
    case 'javascript': return 'bun transpiler (js)';
    case 'bash': return 'bash -n';
    default: return 'heuristic:balanced';
  }
}

/** 括号/引号平衡启发式（注释与字符串内忽略；识别 (* *)、' 注释等方言） */
export function balanceCheck(code: string, langId = ''): { ok: boolean; detail?: string } {
  let depth = 0;
  let inStr: string | null = null;
  let inMlComment = false; // (* ... *)（ocaml/fsharp/pascal）
  let i = 0;
  let line = 1;
  const vb = langId === 'vb';
  // 仅 OCaml/FSharp/Pascal 方言把 (* 视作块注释；go/cpp 的 (*ptr) 是解引用
  const ocamlStyleComment = langId === 'ocaml' || langId === 'fsharp' || langId === 'pascal';
  while (i < code.length) {
    const c = code[i]!;
    if (c === '\n') { line++; i++; continue; }
    if (inMlComment) {
      if (c === '*' && code[i + 1] === ')') { inMlComment = false; i += 2; continue; }
      i++;
      continue;
    }
    if (inStr) {
      if (c === '\\') { i += 2; continue; }
      if (c === inStr) inStr = null;
      i++;
      continue;
    }
    // vb 行注释：' 开头整行
    if (vb && c === "'") {
      while (i < code.length && code[i] !== '\n') i++;
      continue;
    }
    // (* ... *)（ocaml/fsharp/pascal 方言）
    if (ocamlStyleComment && c === '(' && code[i + 1] === '*') { inMlComment = true; i += 2; continue; }
    if (c === '"' || c === "'") { inStr = c; i++; continue; }
    // 行注释（# -- 等，仅行首位置；% 仅 erlang）
    if (c === '#' || (c === '%' && langId === 'erlang')) {
      const lineStart = i === 0 || code[i - 1] === '\n' || /\s/.test(code[i - 1] ?? '');
      if (lineStart) { while (i < code.length && code[i] !== '\n') i++; continue; }
    }
    if (c === '-' && code[i + 1] === '-' && (i === 0 || code[i - 1] === '\n' || /\s/.test(code[i - 1] ?? ''))) {
      while (i < code.length && code[i] !== '\n') i++;
      continue;
    }
    if (c === '/' && code[i + 1] === '/') { while (i < code.length && code[i] !== '\n') i++; continue; }
    if (c === '{' || c === '(' || c === '[') depth++;
    if (c === '}' || c === ')' || c === ']') depth--;
    if (depth < 0) return { ok: false, detail: `第 ${line} 行出现多余闭合括号` };
    i++;
  }
  if (inMlComment) return { ok: false, detail: '(* ... *) 注释未闭合' };
  if (depth !== 0) return { ok: false, detail: `括号不平衡（depth=${depth}）` };
  if (inStr) return { ok: false, detail: `字符串未闭合（${inStr}）` };
  return { ok: true };
}
