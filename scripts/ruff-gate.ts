// ============================================================================
// scripts/ruff-gate.ts — HSL python 产物 ruff 门禁（v0.2.64）
// ----------------------------------------------------------------------------
// 「所有产物 ruff 检测均可通过」的可执行门禁（与 ORG v0.5.4 同源设计）：
//   1. 用 dhv-ts 把语料 emit 成 Python 工程（kernel-tour 全特性 +
//      pattern-tour 模式族 + backends-demo/agent 真实 harness 规格 + dsh）；
//   2. 逐目录跑 ruff check（ruff 0.16 默认全规则集）；
//   3. 任何失败 → exit 1 + 逐条列出（生成器回归当场可见）。
//
// ruff 定位：PATH > ~/.local/bin（uv tool install ruff 的默认落点）。
// 无 ruff 环境直接报错退出（门禁不静默跳过 —— 诚实失败优于假绿）。
//
// 用法：bun scripts/ruff-gate.ts [--keep]（--keep 保留产物目录便于排查）
// cwd：仓库根（脚本自动 cd 到 toolchain 执行 emit —— 与 run-all.ts 同约定）
// ============================================================================

import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";

const ROOT = path.resolve(import.meta.dir, "..");
const TOOLCHAIN = path.join(ROOT, "toolchain");
const DHV = "dhv-ts/src/main.ts"; // 相对 toolchain（spawn cwd）

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
const ruff = findRuff();
const outRoot = fs.mkdtempSync(path.join(os.tmpdir(), "hsl-ruff-gate-"));
let failed = 0;

console.log(`ruff gate —— ${CORPUS.length} 份语料 emit → python → ruff check\n`);
console.log(`  ruff: ${ruff}\n`);

for (const c of CORPUS) {
  const src = path.join(TOOLCHAIN, c.hsl);
  if (!fs.existsSync(src)) {
    console.log(`  ⏭ ${c.hsl}（语料缺失，跳过）`);
    continue;
  }
  const dir = path.join(outRoot, path.basename(c.hsl, ".hsl"));
  fs.rmSync(dir, { recursive: true, force: true });
  // emit（生成器自身先过语法校验）
  const emit = Bun.spawnSync([process.execPath, DHV, "emit", c.hsl, "--out", dir], {
    cwd: TOOLCHAIN, stdout: "pipe", stderr: "pipe",
  });
  if (emit.exitCode !== 0) {
    console.log(`  ✗ ${c.hsl} emit 失败：\n${emit.stderr.toString().slice(0, 500)}`);
    failed++;
    continue;
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
    console.log(`  ⏭ ${c.hsl}（无 python 目标，跳过）`);
    continue;
  }
  // ruff check（ruff 0.16 默认全规则集：E4/E7/E9/F/I001/PLR0124…）
  const res = Bun.spawnSync([ruff, "check", "--output-format", "concise", dir], {
    cwd: TOOLCHAIN, stdout: "pipe", stderr: "pipe",
  });
  const outText = (res.stdout.toString() + res.stderr.toString()).trim();
  if (res.exitCode === 0) {
    console.log(`  ✓ ${c.hsl.padEnd(44)} ${pyFiles.length} 个 .py · ${c.note}`);
  } else {
    console.log(`  ✗ ${c.hsl}（${pyFiles.length} 个 .py）—— ruff 失败：\n${outText.split("\n").map((l) => "      " + l).join("\n")}`);
    failed++;
  }
}

if (!keep) fs.rmSync(outRoot, { recursive: true, force: true });
else console.log(`\n  产物保留：${outRoot}`);

if (failed > 0) {
  console.error(`\n✗ ruff gate 失败（${failed} 份语料）—— python 生成器回归，见上`);
  process.exit(1);
}
console.log(`\n✓ ruff gate 全绿（${CORPUS.length} 份语料 · ruff 0.16 默认全规则）`);
