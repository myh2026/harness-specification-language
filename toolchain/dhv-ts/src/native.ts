// ============================================================================
// dhv-ts/src/native.ts — native 逃生舱执行桥（BNF §3.3 / §5.5 N1-N3）
// ----------------------------------------------------------------------------
// native typescript  → 进程内 new Function 执行（$host 注入 + 捕获变量按名映射）
// native python      → python3 子进程（JSON 编组进出，末表达式为返回值）
// 其他语言后端       → 明确报错（由 dhv Rust 编译器负责静态投射）
// ============================================================================

import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { HRuntimeError, isStruct, isEnum, HEnum } from './values';
import type { Env } from './interp';
import { KEYWORDS, JS_RESERVED } from './lexer';

const execFileP = promisify(execFile);

const PY_STMT_KEYWORDS = /^(if|else|elif|for|while|def|class|return|import|from|try|except|finally|with|pass|break|continue|print|raise|assert|del|global|nonlocal|lambda|yield|and|or|not|in|is)\b/;

/**
 * 去除 native 块体的公共前导缩进（textwrap.dedent 语义）。
 *
 * 背景（v0.2.51 修复）：scanRawBody 按源码原样搬运块体，native python 路径
 * 此前不做 dedent 直接拼进 wrapper —— 块体缩进恰好对齐 wrapper 的 for 循环体
 * （4 空格）时"碰巧能跑"（且表达式会被执行 N 次捕获变量个数次）；嵌套更深
 * （8 空格）则 IndentationError。任何缩进层级都必须正确工作的语义不应依赖
 * 源码书写缩进 —— 故统一 dedent 后再嵌入。
 */
function dedentBlock(body: string): string {
  const lines = body.split('\n');
  let min = Infinity;
  for (const l of lines) {
    if (l.trim().length === 0) continue;
    const ws = l.match(/^[ \t]*/)![0].length;
    if (ws < min) min = ws;
  }
  if (!Number.isFinite(min) || min === 0) return body;
  return lines
    .map((l) => (l.trim().length === 0 ? '' : l.slice(min)))
    .join('\n');
}

/** 扫描 native 体中引用的、存在于 HSL 词法作用域的变量名（N1：按名捕获） */
export function scanCaptured(body: string, env: Env): string[] {
  const names = new Set<string>();
  for (const m of body.matchAll(/[A-Za-z_$][A-Za-z0-9_$]*/g)) {
    const name = m[0]!;
    if (name.startsWith('$')) continue;
    if (JS_RESERVED.has(name)) continue;
    if (KEYWORDS.has(name) && name !== 'self') continue;
    if (env.lookup(name)) names.add(name);
  }
  return [...names];
}

/** HSL 值 → python 侧 JSON 安全表示 */
export function marshalForPython(v: unknown): unknown {
  if (v === undefined || v === null) return null;
  if (typeof v === 'string' || typeof v === 'number' || typeof v === 'boolean') return v;
  if (typeof v === 'bigint') return Number(v);
  if (Array.isArray(v)) return v.map(marshalForPython);
  if (v instanceof Map) {
    const out: Record<string, unknown> = {};
    for (const [k, val] of v) out[String(k)] = marshalForPython(val);
    return out;
  }
  if (isStruct(v)) {
    const out: Record<string, unknown> = {};
    for (const [k, val] of Object.entries(v)) if (k !== '__struct') out[k] = marshalForPython(val);
    return out;
  }
  if (isEnum(v)) {
    const e = v as HEnum;
    const out: Record<string, unknown> = { __variant: e.variant };
    if (e.payload?.named) for (const [k, val] of Object.entries(e.payload.named)) out[k] = marshalForPython(val);
    if (e.payload?.tuple) out.__tuple = e.payload.tuple.map(marshalForPython);
    return out;
  }
  try {
    return JSON.parse(JSON.stringify(v));
  } catch {
    return String(v);
  }
}

/** python 侧返回值 → HSL 值（JSON 解码；数组/对象按原样成为 Vec/foreign） */
function unmarshalFromPython(v: unknown): unknown {
  return v;
}

/** 剥离行内字符串字面量（保留引号占位）—— 供语句判定免受字面量内容干扰 */
function stripPyStrings(line: string): string {
  return line.replace(/(["'])(?:\\.|(?!\1)[^\\])*\1/g, '$1$1');
}

/** 末表达式变换：`EXPR` → `__hsl_result__ = (EXPR)`（带语句关键字防护） */
function transformPythonBody(body: string): string {
  const lines = body.split('\n');
  let lastIdx = -1;
  for (let i = lines.length - 1; i >= 0; i--) {
    const t = lines[i]!.trim();
    if (t.length > 0 && !t.startsWith('#')) { lastIdx = i; break; }
  }
  if (lastIdx < 0) return body + '\n__hsl_result__ = None';
  const line = lines[lastIdx]!.trim();
  // v0.2.51 修复：语句判定在剥离字符串字面量后进行 —— 此前 `"a=b" % x`
  // 这类末表达式因字面量内的 `=` 被赋值正则误判为语句，导致返回值静默丢失。
  const code = stripPyStrings(line);
  const isStatement =
    PY_STMT_KEYWORDS.test(code) ||
    code.endsWith(':') ||
    /[^=!<>+\-*/%]=[^=]/.test(code) ||
    code.startsWith('#');
  if (isStatement || line.startsWith('__hsl_result__')) {
    return body + '\n__hsl_result__ = __hsl_result__ if "__hsl_result__" in dir() else None';
  }
  lines[lastIdx] = `__hsl_result__ = (${line})`;
  return lines.join('\n');
}

export async function evalNativeBlock(
  lang: string,
  body: string,
  env: Env,
  hostApi: unknown,
): Promise<unknown> {
  if (lang === 'typescript' || lang === 'javascript') {
    const captured = scanCaptured(body, env);
    const ctx: Record<string, unknown> = {};
    for (const n of captured) ctx[n] = env.lookup(n)!.value;
    const hasReturn = /\breturn\b/.test(body);
    const code = hasReturn ? body : `return (\n${body}\n);`;
    const fn = new Function(
      '$host',
      '$ctx',
      `const { ${captured.join(', ')} } = ($ctx ?? {});\nreturn (async () => {\n${code}\n})();`,
    );
    try {
      const result = await fn(hostApi, ctx);
      return result === null ? undefined : result;
    } catch (err) {
      throw new HRuntimeError(`native typescript 块执行失败：${(err as Error).message}`);
    }
  }

  if (lang === 'python') {
    const captured = scanCaptured(body, env);
    const ctx: Record<string, unknown> = {};
    for (const n of captured) ctx[n] = marshalForPython(env.lookup(n)!.value);
    const ctxJson = JSON.stringify(ctx);
    const userCode = transformPythonBody(dedentBlock(body));
    const wrapper = [
      'import json, os, sys',
      '__ctx = json.loads(os.environ.get("HSL_NATIVE_CTX", "{}"))',
      'for __k, __v in __ctx.items():',
      '    globals()[__k] = __v',
      userCode,
      'sys.stdout.write("__HSL_OUT__" + json.dumps(__hsl_result__, default=str))',
    ].join('\n');
    try {
      const { stdout } = await execFileP('python3', ['-c', wrapper], {
        env: { ...process.env, HSL_NATIVE_CTX: ctxJson },
        timeout: 30_000,
        maxBuffer: 10 * 1024 * 1024,
      });
      const marker = stdout.indexOf('__HSL_OUT__');
      if (marker < 0) throw new HRuntimeError('native python 块没有产出 __HSL_OUT__ 标记');
      const json = stdout.slice(marker + '__HSL_OUT__'.length);
      return unmarshalFromPython(JSON.parse(json));
    } catch (err) {
      const e = err as { stderr?: string; message?: string };
      throw new HRuntimeError(`native python 块执行失败：${e.stderr?.trim() || e.message}`);
    }
  }

  throw new HRuntimeError(
    `native ${lang} 后端未接入解释器（dhv-ts 运行期支持 typescript / python；${lang} 由 dhv Rust 编译器静态投射，P5）`,
  );
}
