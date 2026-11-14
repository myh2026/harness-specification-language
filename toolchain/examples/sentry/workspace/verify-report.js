#!/usr/bin/env node
// ============================================================================
// sentry/workspace/verify-report.js — 工作区自检脚本（模型通过 bash 调用）
// 与 harness 侧的 HSL 报告闸门独立实现：双实现交叉验证，防止闸门自身是 bug。
// ============================================================================
const fs = require("fs");

const failures = [];

function must(cond, message) {
  if (!cond) failures.push(message);
}

const reportPath = "report.md";
const configPath = "configs/gateway.yaml";

must(fs.existsSync(reportPath), "report.md 不存在");
const report = fs.existsSync(reportPath) ? fs.readFileSync(reportPath, "utf8") : "";

must(report.length >= 400, `report.md 过短（${report.length} 字符，至少 400）`);
for (const section of ["## 根因", "## 证据", "## 影响面", "## 处置"]) {
  must(report.includes(section), `report.md 缺少章节 ${section}`);
}
must(report.includes("SEV-1"), "report.md 缺少严重级别 SEV-1");
must(report.includes("UPSTREAM_TIMEOUT"), "report.md 缺少错误码 UPSTREAM_TIMEOUT");
must(report.includes("payment-db"), "report.md 缺少根因组件 payment-db");
must(report.includes("p99"), "report.md 缺少 p99 指标引用");
must(report.includes("1830"), "report.md 缺少 p99=1830 的事实引用");
must(report.includes("8000"), "report.md 未记录 mitigations（timeout_ms=8000）");

must(fs.existsSync(configPath), "configs/gateway.yaml 不存在");
const config = fs.existsSync(configPath) ? fs.readFileSync(configPath, "utf8") : "";
must(config.includes("timeout_ms: 8000"), "configs/gateway.yaml 未把 timeout_ms 调整为 8000");

if (failures.length > 0) {
  console.error("verify-report: FAILED");
  for (const f of failures) console.error(" - " + f);
  process.exit(1);
}
console.log("verify-report: OK — report.md 结构与事实引用通过，configs/gateway.yaml 已止血");
