#!/usr/bin/env bun
// ============================================================================
// tests/run_value_conformance.ts — HSL 双编译器【值级】一致性回归
// ----------------------------------------------------------------------------
// 用法：bun tests/run_value_conformance.ts
//
// 背景（L-11 教训，v0.2.54 第四轮）：
//   run_conformance.sh 只对拍「check 过/不过」结论 —— dhv parser 曾把带后缀
//   整数字面量（250u8）静默解析为 0 且 check 双端全绿（值损坏），结论对拍
//   完全看不见。修复一个 bug，就要关上一类门：值级对拍把「同源 .hsl 的
//   每个字面量在两端的解析值」逐条比对（raw / 十进制值 / 后缀），
//   parse 层静默损坏（归零、舍入、后缀吞字）无处遁形。
//
// v0.2.56 三族扩展（第六轮，L-12/L-13/L-14 实锤之后）：
//   int    : `int\t<raw>\t<十进制值>[u <域>]`
//   float  : `float\t<raw>\t<IEEE754 位模式 16hex>\t<f32|f64|>` —— 位模式是
//            双端唯一可靠等价判据（Rust "inf" vs JS "Infinity"、大数指数形态
//            均不同，字符串格式化不可比）
//   string : `string\t<raw>\t<统一转义 repr>` —— unescape 层吞字/错值（L-12）
//            由此进入机器护栏
//
// 机制：
//   dhv      parse <file> --dump-values   → 上述行流
//   dhv-ts   本脚本内嵌 AST 遍历（同序：项 → 语句 → 模式/表达式 → 宏 token）
//   比对 = 逐行严格相等（kind \t value \t suffix）
//
// 用例集 = dhv/tests/fixtures/values/*.hsl（专设值域语料库）
// 退出码：0 = 全部一致；1 = 任一文件或字面量不一致。
// ============================================================================
import * as fs from 'node:fs';
import * as path from 'node:path';
import { execFileSync } from 'node:child_process';
import { parseFileSource } from '../dhv-ts/src/parser';
import type * as A from '../dhv-ts/src/ast';

const ROOT = path.resolve(import.meta.dir, '..');
const DHV = path.join(ROOT, 'dhv/target/release/dhv');
const VALDIR = path.join(ROOT, 'dhv/tests/fixtures/values');

// ---- 工具：float IEEE754 位模式 / string 统一转义 repr（与 dhv escape_for_dump 同规则） ----

const fbuf = new ArrayBuffer(8);
const fview = new Float64Array(fbuf);
const uview = new BigUint64Array(fbuf);

function floatBits(v: number): string {
  fview[0] = v;
  return uview[0].toString(16).padStart(16, '0');
}

function escapeForDump(s: string): string {
  let out = '';
  for (const ch of s) {
    const cp = ch.codePointAt(0)!;
    if (ch === '\\') out += '\\\\';
    else if (ch === '"') out += '\\"';
    else if (ch === '\n') out += '\\n';
    else if (ch === '\r') out += '\\r';
    else if (ch === '\t') out += '\\t';
    else if (ch === '\0') out += '\\0';
    else if (cp < 0x20) out += `\\x${cp.toString(16).padStart(2, '0')}`;
    else out += ch;
  }
  return out;
}

// ---- dhv-ts 侧：AST 遍历收集字面量（与 dhv main.rs walk 同序） ----

type LitLine = { kind: 'int' | 'float' | 'string'; value: string; suffix: string; raw: string };

function suffixMark(suffix?: string): string {
  return suffix ? `u ${suffix}` : '';
}

function pushLit(l: A.Lit, out: LitLine[]): void {
  switch (l.t) {
    case 'int':
      out.push({ kind: 'int', value: String(l.v), suffix: suffixMark(l.suffix), raw: String(l.v) });
      break;
    case 'float':
      // v0.2.56 L-14：`1f32` 修复后走 float 分支（此前 int kind + f32 后缀漂移）
      out.push({ kind: 'float', value: floatBits(l.v), suffix: l.suffix ?? '', raw: String(l.v) });
      break;
    case 'str':
      out.push({ kind: 'string', value: escapeForDump(l.v), suffix: '', raw: l.v });
      break;
    default:
      break; // char / bool 两端一致跳过（与 dhv collect 的 _ => {} 对齐）
  }
}

function exprLits(e: A.Expr, out: LitLine[]): void {
  switch (e.kind) {
    case 'lit':
      pushLit(e.lit, out);
      break;
    case 'macro':
      // v0.2.56：表达式级宏 token 树叶子字面量（与 dhv walk_token_tree 同序）——
      // 第五轮语料宏内无字面量未暴露，第六轮 string 扩展后失配实锤
      tokenTreeLits(e.tree, out);
      break;
    case 'binary':
      exprLits(e.lhs, out);
      exprLits(e.rhs, out);
      break;
    case 'unary':
      exprLits(e.operand, out);
      break;
    case 'call':
      exprLits(e.callee, out);
      for (const a of e.args) exprLits(a, out);
      break;
    case 'method':
      exprLits(e.recv, out);
      for (const a of e.args) exprLits(a, out);
      break;
    case 'field':
      exprLits(e.recv, out);
      break;
    case 'index':
      exprLits(e.recv, out);
      exprLits(e.index, out);
      break;
    case 'slice':
      exprLits(e.recv, out);
      if (e.lo) exprLits(e.lo, out);
      if (e.hi) exprLits(e.hi, out);
      break;
    case 'cast':
      exprLits(e.expr, out);
      break;
    case 'assign':
      exprLits(e.target, out);
      exprLits(e.value, out);
      break;
    case 'if':
      exprLits(e.cond, out);
      stmtsLits(e.then, out);
      if (e.else) exprLits(e.else, out);
      break;
    case 'match': {
      exprLits(e.expr, out);
      for (const arm of e.arms) {
        patternLits(arm.pattern, out);
        if (arm.guard) exprLits(arm.guard, out);
        exprLits(arm.body, out);
      }
      break;
    }
    case 'block':
      stmtsLits(e.stmts, out);
      if (e.tail) exprLits(e.tail, out);
      break;
    case 'loop':
    case 'while':
      exprLits(e.body, out);
      break;
    case 'whilelet':
      patternLits(e.pat, out);
      exprLits(e.expr, out);
      exprLits(e.body, out);
      break;
    case 'for':
      patternLits(e.pat, out);
      exprLits(e.iter, out);
      exprLits(e.body, out);
      break;
    case 'closure':
      for (const p of e.params) patternLits(p.pat, out);
      exprLits(e.body, out);
      break;
    case 'array':
    case 'tuple':
      for (const it of e.items) exprLits(it, out);
      break;
    case 'struct':
      for (const f of e.fields) exprLits(f.value, out);
      break;
    default:
      break;
  }
}

function patternLits(p: A.Pattern, out: LitLine[]): void {
  switch (p.kind) {
    case 'literal':
      if (typeof p.value === 'bigint' || typeof p.value === 'number') {
        out.push({ kind: 'int', value: String(p.value), suffix: '', raw: String(p.value) });
      } else if (typeof p.value === 'string') {
        // v0.2.56：字符串模式位（`match s { "abc" => ... }`）与 dhv pattern
        // Literal→Str 分支对齐
        out.push({ kind: 'string', value: escapeForDump(p.value), suffix: '', raw: p.value });
      }
      break;
    case 'tuple':
      for (const it of p.items) patternLits(it, out);
      break;
    case 'struct':
      for (const f of p.fields) patternLits(f.pat, out);
      break;
    case 'or':
      for (const alt of p.alts) patternLits(alt, out);
      break;
    case 'range':
      patternLits(p.lo, out);
      patternLits(p.hi, out);
      break;
    case 'binding':
      if (p.sub) patternLits(p.sub, out);
      break;
    default:
      break;
  }
}

function tokenTreeLits(tt: A.TokenTree, out: LitLine[]): void {
  if (tt.t === 'delim') {
    for (const it of tt.items) tokenTreeLits(it, out);
    return;
  }
  const tok = tt.tok;
  if (tok.kind === 'int') {
    out.push({ kind: 'int', value: String(tok.value), suffix: suffixMark(tok.suffix), raw: tok.text });
  } else if (tok.kind === 'float') {
    out.push({ kind: 'float', value: floatBits(Number(tok.value)), suffix: tok.suffix ?? '', raw: tok.text });
  } else if (tok.kind === 'string') {
    out.push({ kind: 'string', value: escapeForDump(String(tok.value)), suffix: '', raw: tok.text });
  }
}

function stmtsLits(stmts: A.Stmt[], out: LitLine[]): void {
  for (const st of stmts) {
    switch (st.kind) {
      case 'let': {
        patternLits(st.pat, out);
        if (st.init) exprLits(st.init, out);
        if (st.elseBlock) stmtsLits(st.elseBlock, out);
        break;
      }
      case 'expr':
        // 对齐 dhv 口径：带分号语句位置的宏调用 = macro_invocation_semi →
        // Item::MacroCall（dhv walk 跳过）；表达式位置（let init / match 臂 /
        // 尾表达式）的宏 = Expr::Macro（dump token 树）。
        if (st.expr.kind !== 'macro') exprLits(st.expr, out);
        break;
      default:
        break;
    }
  }
}

function itemLits(item: A.Item, out: LitLine[]): void {
  switch (item.kind) {
    case 'fn':
      if (item.fn.body) stmtsLits(item.fn.body, out);
      break;
    case 'const':
      exprLits(item.value, out);
      break;
    case 'enum':
      for (const v of item.variants) {
        // dhv-ts 判别式字段名为 discr（dhv 为 discriminant —— dump 行口径统一为值）
        if (v.discr !== undefined) {
          out.push({ kind: 'int', value: String(v.discr), suffix: '', raw: String(v.discr) });
        }
      }
      break;
    case 'graph':
      for (const gs of item.graph.body) {
        if (gs.t === 'node' && gs.decl.init) exprLits(gs.decl.init, out);
        if (gs.t === 'edge') {
          if (gs.decl.guardExpr) exprLits(gs.decl.guardExpr, out);
          if (gs.decl.guardPattern) patternLits(gs.decl.guardPattern, out);
        }
        if (gs.t === 'stmt') stmtsLits([gs.stmt], out);
        if (gs.t === 'item') itemLits(gs.item, out);
      }
      break;
    case 'export':
      itemLits(item.item, out);
      break;
    default:
      break;
  }
}

function dhvTsValues(file: string): LitLine[] {
  const src = fs.readFileSync(file, 'utf-8');
  const ast = parseFileSource(src, file);
  const out: LitLine[] = [];
  for (const item of ast.items) itemLits(item, out);
  return out;
}

// ---- dhv 侧：parse --dump-values 输出解析 ----

function dhvValues(file: string): LitLine[] {
  const stdout = execFileSync(DHV, ['parse', file, '--dump-values'], {
    encoding: 'utf-8', timeout: 60_000,
  }) as string;
  const out: LitLine[] = [];
  for (const line of stdout.split('\n')) {
    if (line.startsWith('int\t')) {
      // `int\t<raw>\t<value>[u 后缀]`
      const cols = line.split('\t');
      const rest = cols[2] ?? '';
      const m = /^(-?[0-9]+)(u .*)?$/.exec(rest);
      if (!m) continue;
      out.push({ kind: 'int', value: m[1]!, suffix: m[2] ?? '', raw: cols[1] ?? '' });
    } else if (line.startsWith('float\t')) {
      // `float\t<raw>\t<bits 16hex>\t<suffix>`
      const cols = line.split('\t');
      const bits = cols[2] ?? '';
      if (!/^[0-9a-f]{16}$/.test(bits)) continue;
      out.push({ kind: 'float', value: bits, suffix: cols[3] ?? '', raw: cols[1] ?? '' });
    } else if (line.startsWith('string\t')) {
      // `string\t<raw>\t<escaped>`（escaped 不含真实 tab —— 已统一转义）
      const cols = line.split('\t');
      if (cols.length < 3) continue;
      out.push({ kind: 'string', value: cols.slice(2).join('\t'), suffix: '', raw: cols[1] ?? '' });
    }
  }
  return out;
}

// ---- 比对 ----

let pass = 0;
let fail = 0;
const diffs: string[] = [];

function fmtLits(lits: LitLine[]): string {
  return lits.map((l) => `${l.kind}:${l.raw}→${l.value}${l.suffix ? ` [${l.suffix}]` : ''}`).join(', ') || '（无字面量）';
}

if (!fs.existsSync(DHV)) {
  console.error('dhv 未构建（先 cargo build --release）');
  process.exit(1);
}
const files = fs.existsSync(VALDIR)
  ? fs.readdirSync(VALDIR).filter((f) => f.endsWith('.hsl')).sort()
  : [];

console.log(`== 值级一致性（${files.length} 个用例 × 双编译器逐字面量比对：int / float / string）==`);
for (const f of files) {
  const full = path.join(VALDIR, f);
  let a: LitLine[];
  let b: LitLine[];
  try {
    a = dhvValues(full);
  } catch {
    fail++;
    diffs.push(`${f}: dhv parse 失败`);
    continue;
  }
  try {
    b = dhvTsValues(full);
  } catch {
    fail++;
    diffs.push(`${f}: dhv-ts parse 失败`);
    continue;
  }
  const aStr = a.map((l) => `${l.kind}\t${l.value}\t${l.suffix}`).join('\n');
  const bStr = b.map((l) => `${l.kind}\t${l.value}\t${l.suffix}`).join('\n');
  if (a.length !== b.length || aStr !== bStr) {
    fail++;
    diffs.push(
      `${f}: 字面量序列不一致\n  dhv    (${a.length}): ${fmtLits(a)}\n  dhv-ts (${b.length}): ${fmtLits(b)}`,
    );
  } else {
    pass++;
    const kinds = a.reduce<Record<string, number>>((acc, l) => { acc[l.kind] = (acc[l.kind] ?? 0) + 1; return acc; }, {});
    const summary = ['int', 'float', 'string'].map((k) => kinds[k] ? `${k}×${kinds[k]}` : null).filter(Boolean).join(' ');
    console.log(`  ✓ ${f.padEnd(40)} ${summary || '（无字面量）'} 全部一致`);
  }
}

console.log('');
if (fail > 0) {
  console.log('不一致清单：');
  for (const d of diffs) console.log(`  ✗ ${d}`);
}
console.log(`值级一致性: ${pass} 通过 · ${fail} 失败`);
process.exit(fail > 0 ? 1 : 0);
