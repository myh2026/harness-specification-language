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
//   每个整数字面量在两端的解析值」逐条比对（raw / 十进制值 / 后缀），
//   parse 层静默损坏（归零、舍入、后缀吞字）无处遁形。
//
// 机制：
//   dhv      parse <file> --dump-values   → `int\t<raw>\t<value>[u 后缀]` 行
//   dhv-ts   本脚本内嵌 AST 遍历（同序：项 → 语句 → 模式/表达式 → 宏 token）
//   比对 = 逐行严格相等（值串规范化：十进制、后缀 `u <ty>` 标记）
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
// ---- dhv-ts 侧：AST 遍历收集整数字面量（与 dhv main.rs walk 同序） ----

interface IntLit { raw: string; value: string; suffix: string }

function suffixMark(suffix?: string): string {
  return suffix ? `u ${suffix}` : '';
}

function exprLits(e: A.Expr, out: IntLit[]): void {
  switch (e.kind) {
    case 'lit':
      if (e.lit.t === 'int') {
        out.push({ raw: String(e.lit.v), value: String(e.lit.v), suffix: suffixMark(e.lit.suffix) });
      }
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

function patternLits(p: A.Pattern, out: IntLit[]): void {
  switch (p.kind) {
    case 'literal':
      if (typeof p.value === 'bigint' || typeof p.value === 'number') {
        out.push({ raw: String(p.value), value: String(p.value), suffix: '' });
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

function stmtsLits(stmts: A.Stmt[], out: IntLit[]): void {
  for (const st of stmts) {
    switch (st.kind) {
      case 'let': {
        patternLits(st.pat, out);
        if (st.init) exprLits(st.init, out);
        if (st.elseBlock) stmtsLits(st.elseBlock, out);
        break;
      }
      case 'expr':
        exprLits(st.expr, out);
        break;
      default:
        break;
    }
  }
}

function itemLits(item: A.Item, out: IntLit[]): void {
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
          out.push({ raw: String(v.discr), value: String(v.discr), suffix: '' });
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

function dhvTsValues(file: string): IntLit[] {
  const src = fs.readFileSync(file, 'utf-8');
  const ast = parseFileSource(src, file);
  const out: IntLit[] = [];
  for (const item of ast.items) itemLits(item, out);
  return out;
}

// ---- dhv 侧：parse --dump-values 输出解析 ----

function dhvValues(file: string): IntLit[] {
  const stdout = execFileSync(DHV, ['parse', file, '--dump-values'], {
    encoding: 'utf-8', timeout: 60_000,
  }) as string;
  const out: IntLit[] = [];
  for (const line of stdout.split('\n')) {
    if (!line.startsWith('int\t')) continue;
    const [_, raw, rest] = line.split('\t');
    // rest = "<value>[u 后缀]" —— 拆出 value 与 suffix（u 标记与空格）
    const m = /^(-?[0-9]+)(u .*)?$/.exec(rest ?? '');
    if (!m) continue;
    out.push({ raw: raw ?? '', value: m[1]!, suffix: m[2] ?? '' });
  }
  return out;
}

// ---- 比对 ----

let pass = 0;
let fail = 0;
const diffs: string[] = [];

function fmtLits(lits: IntLit[]): string {
  return lits.map((l) => `${l.raw}→${l.value}${l.suffix ? ` [${l.suffix}]` : ''}`).join(', ') || '（无字面量）';
}

if (!fs.existsSync(DHV)) {
  console.error('dhv 未构建（先 cargo build --release）');
  process.exit(1);
}
const files = fs.existsSync(VALDIR)
  ? fs.readdirSync(VALDIR).filter((f) => f.endsWith('.hsl')).sort()
  : [];

console.log(`== 值级一致性（${files.length} 个用例 × 双编译器逐字面量比对）==`);
for (const f of files) {
  const full = path.join(VALDIR, f);
  let a: IntLit[];
  let b: IntLit[];
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
  const aStr = a.map((l) => `${l.value}\t${l.suffix}`).join('\n');
  const bStr = b.map((l) => `${l.value}\t${l.suffix}`).join('\n');
  if (a.length !== b.length || aStr !== bStr) {
    fail++;
    diffs.push(
      `${f}: 字面量序列不一致\n  dhv    (${a.length}): ${fmtLits(a)}\n  dhv-ts (${b.length}): ${fmtLits(b)}`,
    );
  } else {
    pass++;
    console.log(`  ✓ ${f.padEnd(40)} ${a.length} 个整数字面量全部一致`);
  }
}

console.log('');
if (fail > 0) {
  console.log('不一致清单：');
  for (const d of diffs) console.log(`  ✗ ${d}`);
}
console.log(`值级一致性: ${pass} 通过 · ${fail} 失败`);
process.exit(fail > 0 ? 1 : 0);
