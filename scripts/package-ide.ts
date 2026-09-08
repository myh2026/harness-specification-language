#!/usr/bin/env bun
// ============================================================================
// scripts/package-ide.ts — HSL IDE vsix 打包（v0.2.0）
// ----------------------------------------------------------------------------
// 产出：dist-ide/hsl-ide-<version>.vsix（GitHub Release 安装包形态）
// 流程：
//   1. 捆绑：toolchain/dhv-ts（纯 TS 零依赖解释器）→ ide/dhv-ts/
//      （vsix 内置工具链 —— 用户装完扩展即得完整 check/run/emit 能力，
//       唯一运行时前置是 bun）
//   2. bunx @vscode/vsce package --no-dependencies（ide/ 为打包根）
//   3. 清理捆绑副本（工作树零污染；开发态由工作区 toolchain/ 解析）
// 本地运行：bun scripts/package-ide.ts
// CI：ci.yml ide-vsix job（产物上传 artifact → release.yml 附到 Release）
// ============================================================================
import * as fs from "node:fs";
import * as path from "node:path";

const ROOT = path.resolve(import.meta.dir, "..");
const IDE = path.join(ROOT, "ide");
const TS_SRC = path.join(ROOT, "toolchain", "dhv-ts");
const BUNDLED = path.join(IDE, "dhv-ts");

const version = (JSON.parse(fs.readFileSync(path.join(IDE, "package.json"), "utf-8")) as { version: string }).version;
const outDir = path.join(ROOT, "dist-ide");
const vsix = path.join(outDir, `hsl-ide-${version}.vsix`);

// ---- 1. 捆绑 dhv-ts（解释器源码树；tests 不随包 —— 体积与职责） ----
if (!fs.existsSync(path.join(TS_SRC, "src", "main.ts"))) {
  console.error(`✗ 找不到工具链源：${TS_SRC}/src/main.ts`);
  process.exit(1);
}
fs.rmSync(BUNDLED, { recursive: true, force: true });
fs.cpSync(TS_SRC, BUNDLED, { recursive: true });
fs.rmSync(path.join(BUNDLED, "tests"), { recursive: true, force: true });
const bundledFiles = (() => {
  let n = 0;
  const walk = (d: string): void => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      if (e.isDirectory()) walk(path.join(d, e.name));
      else n += 1;
    }
  };
  walk(BUNDLED);
  return n;
})();
console.log(`① 捆绑 dhv-ts → ide/dhv-ts（${bundledFiles} 文件）`);

// ---- 2. vsce package ----
fs.rmSync(vsix, { force: true });
fs.mkdirSync(outDir, { recursive: true });
console.log(`② vsce package → ${path.relative(ROOT, vsix)}`);
const proc = Bun.spawnSync(
  ["bunx", "@vscode/vsce", "package", "--no-dependencies", "-o", vsix],
  { cwd: IDE, stdout: "inherit", stderr: "inherit" },
);

// ---- 3. 清理捆绑副本（无论成败） ----
fs.rmSync(BUNDLED, { recursive: true, force: true });

if (proc.exitCode !== 0) {
  console.error(`✗ vsce 打包失败（退出码 ${proc.exitCode}）`);
  process.exit(1);
}
const size = fs.statSync(vsix).size;
console.log(`✓ hsl-ide-${version}.vsix（${(size / 1024).toFixed(0)} KB · 含捆绑 dhv-ts）`);
console.log(`  安装：code --install-extension ${path.relative(ROOT, vsix)}`);
