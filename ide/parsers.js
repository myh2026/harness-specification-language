// ============================================================================
// ide/parsers.js — HSL IDE 纯解析层（v0.2.0）
// ----------------------------------------------------------------------------
// 从 extension.js 抽出的无依赖纯函数（不 require vscode —— 便于
// ide/tests/validate.js 直接单测）。三条解析通道：
//   1. parseCheckErrors：dhv-ts check 输出的 error[code]: msg (file:line:col)
//   2. parseTargets：dhv-ts targets 输出的 38 后端注册表（含 Tier 分组）
//   3. parseRunArtifacts：run 产物 run.json + report.md → 摘要（verdict 等）
// CommonJS（VS Code 扩展加载面）。
// ============================================================================

'use strict';

// ---- 1. check 错误行 → 诊断对象 ----
// 形态（实测）：error[E-0]: 无法解析的模式 "{" (punct) (/tmp/bad.hsl:1:13)
// 末组 (file:line:col) 锚定行尾；消息贪婪匹配 + 行尾锚 → 消息内部可含括号
// （懒惰消息会把 "(punct) (/path)" 的前半截进 file 分组 —— 实测陷阱）。
const ERROR_LINE = /^error\[([^\]]+)\]:\s+(.*)\s*\(([^()]*?):(\d+):(\d+)\)\s*$/;

/**
 * 解析 check 输出 → 错误数组。
 * @param {string} stdout dhv-ts check 的完整输出
 * @returns {Array<{code: string, message: string, file: string, line: number, col: number, raw: string}>}
 *   无位置的错误行 → { file: '', line: 0, col: 0 }（调用方决定挂靠目标）
 */
function parseCheckErrors(stdout) {
  const out = [];
  for (const raw of String(stdout || '').split('\n')) {
    const line = raw.trimEnd();
    if (!line.startsWith('error[')) continue;
    const m = ERROR_LINE.exec(line);
    if (m) {
      out.push({
        code: m[1],
        message: m[2].replace(/\s+$/, ''),
        file: m[3],
        line: Math.max(1, Number(m[4]) || 1),
        col: Math.max(1, Number(m[5]) || 1),
        raw: line,
      });
    } else {
      // 无位置形态：error[E-x]: message
      const m2 = /^error\[([^\]]+)\]:\s*(.*)$/.exec(line);
      if (m2) out.push({ code: m2[1], message: m2[2], file: '', line: 0, col: 0, raw: line });
    }
  }
  return out;
}

/** check 总结行（“dhv-ts check: 0 error(s), 0 warning(s)”）→ {errors, warnings}。 */
function parseCheckSummary(stdout) {
  const m = /dhv-ts check:\s*(\d+)\s*error\(s\),\s*(\d+)\s*warning\(s\)/.exec(String(stdout || ''));
  if (!m) return null;
  return { errors: Number(m[1]), warnings: Number(m[2]) };
}

// ---- 2. targets 注册表 → 分组目标列表 ----
// 形态（实测）：
//   Tier 1 · Harness 核心（活体/语句子集翻译优先）
//     python        Python        .py   full 活体翻译       native 可执行 · 语法校验:python3
const TIER_LINE = /^\s{2}Tier\s(\d+)\s*·\s*(.+?)\s*$/;
const TARGET_LINE = /^\s{4}(\S+)\s{2,}(\S.*?)\s{2,}\.(\w+)\s+(.+?)\s*$/;

/**
 * 解析 targets 输出 → [{tier, tierName, id, name, ext, note}]。
 * 解析失败（输出形态变化）→ 空数组（调用方回退到原始 QuickPick）。
 */
function parseTargets(stdout) {
  const out = [];
  let tier = 0;
  let tierName = '';
  for (const raw of String(stdout || '').split('\n')) {
    const t = TIER_LINE.exec(raw);
    if (t) { tier = Number(t[1]); tierName = t[2]; continue; }
    const m = TARGET_LINE.exec(raw);
    if (m) out.push({ tier, tierName, id: m[1], name: m[2], ext: m[3], note: m[4] });
  }
  return out;
}

// ---- 3. run 产物 → 摘要 ----
/**
 * 从 run 输出目录的产物提取摘要（全部容错：缺文件 → null 字段）。
 * @param {{readFile: (p: string) => string}} io 文件读取注入（测试替身）
 */
function parseRunArtifacts(outDir, io) {
  const summary = { ok: null, elapsedMs: null, model: null, verdict: null, turns: null, reportPath: null, events: null };
  try {
    const runJson = JSON.parse(io.readFile(`${outDir}/run.json`));
    summary.ok = typeof runJson.ok === 'boolean' ? runJson.ok : null;
    summary.elapsedMs = typeof runJson.elapsed_ms === 'number' ? runJson.elapsed_ms : null;
    summary.model = runJson.model || null;
    summary.events = typeof runJson.events === 'number' ? runJson.events : null;
  } catch { /* run.json 缺失或坏 JSON */ }
  try {
    const report = io.readFile(`${outDir}/report.md`);
    summary.reportPath = `${outDir}/report.md`;
    const v = /-\s*verdict:\s*(\S+)/.exec(report);
    if (v) summary.verdict = v[1];
    const t = /-\s*turns:\s*(\d+)/.exec(report);
    if (t) summary.turns = Number(t[1]);
  } catch { /* report.md 缺失 */ }
  return summary;
}

/** k 缩写（8400 → 8.4k）—— 与 CLI 侧 fmt_k 同形。 */
function fmtK(n) {
  if (typeof n !== 'number' || !isFinite(n)) return String(n);
  if (Math.abs(n) >= 1000) return `${(n / 1000).toFixed(1)}k`;
  return String(n);
}

module.exports = { parseCheckErrors, parseCheckSummary, parseTargets, parseRunArtifacts, fmtK, ERROR_LINE };
