#!/usr/bin/env bun
// ============================================================================
// tests/run_emit_conformance.ts — emit 产物行为级对拍（v0.2.55 第七轮护栏）
// ----------------------------------------------------------------------------
// 背景（L-15 教训）：emit 的「语法校验绿灯」只能证明生成物是合法目标语言
// 代码，不能证明它与 interp 行为一致 —— 投射 `fn main` 曾只生成函数定义、
// 无入口调用：生成物运行「成功」（exit 0）但零输出零副作用。
//
// 本护栏 = 行为级闭环（第五轮值级对拍的自然延伸：解析值 → 运行行为）：
//   1. interp run <fixture>        → 提取 emit:: 标记行（参考语义）
//   2. emit <fixture> --out <tmp>  → 解析 project{} 中 main 的投射目标
//   3. 逐后端真实运行生成物（python3 / bun）→ 提取 emit:: 行
//   4. 三方逐行比对（行序即语义序；任何漂移 = RED）
//
// 语料约定：tests/fixtures/emit/*.hsl，全部 println! 以 "emit::" 前缀标记
// （防 interp 横幅/banner 混入比对）；main 返回 0（进程退出码三端一致）。
// 每语料自带 project{} —— 投射矩阵由语料自决（如 bigint 语料只投 python，
// 因 js 的 >2^53 舍入是 L-9c 已声明投射差异而非回归）。
// ============================================================================

import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';
import { execFileSync } from 'node:child_process';

const ROOT = path.resolve(import.meta.dir, '..'); // toolchain/
const DHV_TS = path.join(ROOT, 'dhv-ts/src/main.ts');
const FIXTURES = path.join(ROOT, 'tests/fixtures/emit');
const MARKER = /^emit::/;

interface RunResult { code: number; stdout: string; stderr: string }

function run(cmd: string, args: string[], opts: { cwd?: string; timeout?: number } = {}): RunResult {
  try {
    const stdout = execFileSync(cmd, args, {
      cwd: opts.cwd ?? ROOT,
      encoding: 'utf-8',
      timeout: opts.timeout ?? 60_000,
      maxBuffer: 16 * 1024 * 1024,
    }) as string;
    return { code: 0, stdout, stderr: '' };
  } catch (e) {
    const err = e as { status?: number; stdout?: string; stderr?: string };
    return { code: err.status ?? 1, stdout: err.stdout ?? '', stderr: err.stderr ?? '' };
  }
}

/** 提取 emit:: 标记行（保序；空行/横幅免疫） */
function markerLines(out: string): string[] {
  return out.split('\n').filter((l) => MARKER.test(l));
}

/** 解析 project{} 中 main 的投射目标：[{path, lang}]（语料自决投射矩阵） */
function mainProjections(src: string): Array<{ file: string; lang: string }> {
  const out: Array<{ file: string; lang: string }> = [];
  const re = /main\s*->\s*"([^"]+)"\s*:\s*([A-Za-z_][A-Za-z0-9_]*)/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(src)) !== null) out.push({ file: m[1]!, lang: m[2]! });
  return out;
}

/** 可真实运行的生成后端 → 命令行（其余语言 tier 不支持本护栏执行） */
function runnable(lang: string): { cmd: string; arg: (abs: string) => string[] } | null {
  switch (lang) {
    case 'python': return { cmd: 'python3', arg: (abs) => [abs] };
    case 'javascript': return { cmd: 'bun', arg: (abs) => [abs] };
    case 'typescript': return { cmd: 'bun', arg: (abs) => [abs] };
    default: return null;
  }
}

const fixtures = fs.readdirSync(FIXTURES).filter((f) => f.endsWith('.hsl')).sort();
if (fixtures.length === 0) {
  console.error('emit 语料缺失：tests/fixtures/emit/*.hsl');
  process.exit(1);
}

let pass = 0;
const failures: string[] = [];
let backendRuns = 0;

for (const fx of fixtures) {
  const abs = path.join(FIXTURES, fx);
  const src = fs.readFileSync(abs, 'utf-8');
  const projections = mainProjections(src);
  if (projections.length === 0) {
    failures.push(`${fx}: project{} 未投射 main —— 语料契约破坏`);
    continue;
  }

  // 1. interp 参考语义
  const interp = run('bun', [DHV_TS, 'run', abs]);
  const interpLines = markerLines(interp.stdout);
  if (interp.code !== 0 || interpLines.length === 0) {
    failures.push(`${fx}: interp run 失败（code=${interp.code}，标记行 ${interpLines.length}）\n${interp.stderr.slice(0, 400)}`);
    continue;
  }

  // 2. emit
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'dhv-emit-'));
  const emit = run('bun', [DHV_TS, 'emit', abs, '--out', tmp]);
  if (emit.code !== 0) {
    failures.push(`${fx}: emit 失败\n${emit.stdout.slice(-400)}${emit.stderr.slice(0, 200)}`);
    continue;
  }

  // 3. 逐后端运行 + 比对
  let allMatch = true;
  const detail: string[] = [];
  for (const proj of projections) {
    const rr = runnable(proj.lang);
    if (!rr) continue; // 不可执行后端（contract/static）不在行为级护栏范围
    const genAbs = path.join(tmp, proj.file);
    if (!fs.existsSync(genAbs)) {
      failures.push(`${fx}: 生成物缺失 ${proj.file}（${proj.lang}）`);
      allMatch = false;
      continue;
    }
    const ran = run(rr.cmd, rr.arg(genAbs));
    backendRuns++;
    if (ran.code !== 0) {
      // 退出码非 0 = 生成物运行期崩溃（ReferenceError/SyntaxError 等）
      const errTxt = `${ran.stdout}${ran.stderr}`.split('\n').filter((l) => /Error|error|panic/i.test(l)).slice(0, 3).join(' | ');
      failures.push(`${fx} [${proj.lang}]: 生成物运行崩溃（exit=${ran.code}）${errTxt}`);
      allMatch = false;
      continue;
    }
    const genLines = markerLines(ran.stdout);
    if (genLines.length !== interpLines.length) {
      failures.push(`${fx} [${proj.lang}]: 标记行数不一致（interp ${interpLines.length} vs ${proj.lang} ${genLines.length}）`);
      allMatch = false;
      continue;
    }
    for (let i = 0; i < interpLines.length; i++) {
      if (interpLines[i] !== genLines[i]) {
        failures.push(`${fx} [${proj.lang}] 行 ${i + 1}: interp 「${interpLines[i]}」 vs ${proj.lang} 「${genLines[i]}」`);
        allMatch = false;
      }
    }
    if (detail !== null) detail.push(proj.lang);
  }
  if (allMatch) {
    pass++;
    console.log(`  ✓ ${fx}（${projections.map((p) => p.lang).join('+')}）`);
  }
  fs.rmSync(tmp, { recursive: true, force: true });
}

console.log(`\nemit 行为级对拍: ${pass} 通过 · ${failures.length} 失败 · ${backendRuns} 个后端真实运行`);
if (failures.length > 0) {
  console.log('\n不一致明细：');
  for (const f of failures) console.log(`  ✗ ${f}`);
  process.exit(1);
}
console.log('emit 产物行为级一致性: 三方逐行全等 ✓（interp ↔ 生成物真实运行）');
