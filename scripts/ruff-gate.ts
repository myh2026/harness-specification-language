// ============================================================================
// scripts/ruff-gate.ts — HSL python 产物 ruff 门禁（v0.2.64 · v0.2.65 双车道）
// ----------------------------------------------------------------------------
// 「所有产物 ruff 检测均可通过」的可执行门禁（与 ORG v0.5.4 同源设计）：
//   1. 把语料 emit 成 Python 工程（kernel-tour 全特性 + pattern-tour
//      模式族 + backends-demo/agent 真实 harness 规格 + dsh）；
//   2. 逐目录跑 ruff check（ruff 0.16 默认全规则集）；
//   3. 任何失败 → exit 1 + 逐条列出（生成器回归当场可见）。
//
// v0.2.65 起按工具链分车道：
//   · ts 车道（默认）：dhv-ts 解释器 emit；
//   · rust 车道：dhv(Rust) 编译器 emit（需先 cargo build --release 产出
//     toolchain/dhv/target/release/dhv；本地缺席时通知跳过，CI 的
//     rust-ruff-gate job 用 --require-rust 强制在环）。
//
// ruff 定位：PATH > ~/.local/bin（uv tool install ruff 的默认落点）。
// 无 ruff 环境直接报错退出（门禁不静默跳过 —— 诚实失败优于假绿）。
//
// 用法：bun scripts/ruff-gate.ts [--keep] [--ts-only] [--rust-only] [--require-rust]
//   --keep          保留产物目录便于排查
//   --ts-only       仅 dhv-ts 车道（ruff-gate CI job 用）
//   --rust-only     仅 dhv(Rust) 车道（rust-ruff-gate CI job 用）
//   --require-rust  Rust 车道强制在环（二进制缺失 = 失败，不跳过）
// cwd：仓库根（脚本自动 cd 到 toolchain 执行 emit —— 与 run-all.ts 同约定）
// ============================================================================

import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";

const ROOT = path.resolve(import.meta.dir, "..");
const TOOLCHAIN = path.join(ROOT, "toolchain");
const DHV_TS = "dhv-ts/src/main.ts"; // 相对 toolchain（spawn cwd）
const DHV_RUST = path.join(TOOLCHAIN, "dhv", "target", "release", "dhv");

/** 语料：全特性巡览 + 模式族 + 真实 harness 规格（三层覆盖面）。 */
const CORPUS: Array<{ hsl: string; note: string }> = [
  { hsl: "dhv-ts/examples/kernel-tour.hsl", note: "全特性语料（复合赋值/闭包/递归/Result 模式/实参括号）" },
  { hsl: "dhv-ts/examples/pattern-tour.hsl", note: "模式全家族（match/if-let/while-let · Ok/Err/Some 桩）" },
  { hsl: "examples/backends-demo/agent.hsl", note: "真实 harness 规格（Prompt/ToolResult 全家族投射）" },
  { hsl: "examples/dsh/dsh.hsl", note: "dsh 演示规格（多后端联合 · python 车道）" },
];

function findRuff(): string {
  for (const c of process.env.PATH?.split(path.delimiter) ?? []) {
    const p = path.join(c, "ruff");
    if (fs.existsSync(p)) return p;
  }
  const fallback = path.join(os.homedir(), ".local", "bin", "ruff");
  if (fs.existsSync(fallback)) return fallback;
  console.error("✗ 找不到 ruff。安装：uv tool install ruff（或 pip install ruff）");
  process.exit(2);
}

const keep = process.argv.includes("--keep");
const tsOnly = process.argv.includes("--ts-only");
const rustOnly = process.argv.includes("--rust-only");
const requireRust = process.argv.includes("--require-rust");
const ruff = findRuff();
const outRoot = fs.mkdtempSync(path.join(os.tmpdir(), "hsl-ruff-gate-"));
let failed = 0;
let lanes = 0;

const wantTs = !rustOnly;
const wantRust = !tsOnly;
const rustAvailable = fs.existsSync(DHV_RUST);
if (wantRust && !rustAvailable) {
  if (requireRust) {
    console.error(`✗ dhv(Rust) 二进制缺失（${DHV_RUST}）—— 先 cd toolchain/dhv && cargo build --release`);
    process.exit(2);
  }
  console.log(`  ⏭ rust 车道：dhv 二进制缺席（本地未构建）—— 仅 ts 车道在环\n`);
}

console.log(`ruff gate —— ${CORPUS.length} 份语料 emit → python → ruff check\n`);
console.log(`  ruff: ${ruff}\n`);

/** 单语料单车道：emit → 收集 .py → ruff check。返回是否通过。 */
function gateOne(lane: string, hsl: string, note: string, emitCmd: string[]): boolean {
  const src = path.join(TOOLCHAIN, hsl);
  if (!fs.existsSync(src)) {
    console.log(`  ⏭ [${lane}] ${hsl}（语料缺失，跳过）`);
    return true;
  }
  const dir = path.join(outRoot, `${lane}-${path.basename(hsl, ".hsl")}`);
  fs.rmSync(dir, { recursive: true, force: true });
  const emit = Bun.spawnSync(emitCmd, {
    cwd: TOOLCHAIN, stdout: "pipe", stderr: "pipe",
  });
  if (emit.exitCode !== 0) {
    console.log(`  ✗ [${lane}] ${hsl} emit 失败：\n${emit.stderr.toString().slice(0, 500)}`);
    return false;
  }
  // 收集 python 文件
  const pyFiles: string[] = [];
  const walk = (d: string): void => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.name.endsWith(".py")) pyFiles.push(p);
    }
  };
  walk(dir);
  if (pyFiles.length === 0) {
    console.log(`  ⏭ [${lane}] ${hsl}（无 python 目标，跳过）`);
    return true;
  }
  // ruff check（ruff 0.16 默认全规则集：E4/E7/E9/F/I001/PLR0124…）
  const res = Bun.spawnSync([ruff, "check", "--output-format", "concise", dir], {
    cwd: TOOLCHAIN, stdout: "pipe", stderr: "pipe",
  });
  const outText = (res.stdout.toString() + res.stderr.toString()).trim();
  const label = `[${lane}] ${hsl}`.padEnd(52);
  if (res.exitCode === 0) {
    console.log(`  ✓ ${label} ${pyFiles.length} 个 .py · ${note}`);
    return true;
  }
  console.log(`  ✗ [${lane}] ${hsl}（${pyFiles.length} 个 .py）—— ruff 失败：\n${outText.split("\n").map((l) => "      " + l).join("\n")}`);
  return false;
}

for (const c of CORPUS) {
  if (wantTs) {
    lanes++;
    if (!gateOne("ts", c.hsl, c.note, [process.execPath, DHV_TS, "emit", c.hsl, "--out", path.join(outRoot, "ts-" + path.basename(c.hsl, ".hsl"))])) failed++;
  }
}
for (const c of CORPUS) {
  if (wantRust && rustAvailable) {
    lanes++;
    const dir = path.join(outRoot, "rust-" + path.basename(c.hsl, ".hsl"));
    if (!gateOne("rust", c.hsl, c.note, [DHV_RUST, "emit", c.hsl, "--out", dir])) failed++;
  }
}

if (!keep) fs.rmSync(outRoot, { recursive: true, force: true });
else console.log(`\n  产物保留：${outRoot}`);

if (failed > 0) {
  console.error(`\n✗ ruff gate 失败（${failed} 份语料·车道）—— python 生成器回归，见上`);
  process.exit(1);
}
console.log(`\n✓ ruff gate 全绿（${lanes} 份语料·车道 · ruff 0.16 默认全规则）`);
