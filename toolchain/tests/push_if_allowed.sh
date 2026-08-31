#!/usr/bin/env bash
# ============================================================================
# 测试 → 版本守卫 → 提交推送（定时任务调用入口）
#
# 流程:
#   1. cargo test（dhv 回归矩阵）
#   2. tests/run_conformance.sh（双编译器一致性 31 组用例）
#   3. tests/version_guard.sh（版本比对方：本地 vs 远端 README）
#   4. 有实质差异且守卫放行 → commit + push origin main（凭据取自 ~/.git-credentials）
# 任何一步失败 → 不推送，退出码 1。
# ============================================================================
set -uo pipefail

TOOLCHAIN_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_DIR="$(dirname "$TOOLCHAIN_DIR")"
source "$HOME/.cargo/env" 2>/dev/null || true

echo "== 1/4 cargo test =="
(cd "$TOOLCHAIN_DIR/dhv" && cargo test --release --quiet) || { echo "测试失败，不推送"; exit 1; }

echo "== 2/4 双编译器一致性 =="
bash "$TOOLCHAIN_DIR/tests/run_conformance.sh" >/tmp/conformance.log 2>&1 \
  || { echo "一致性回归失败:"; tail -20 /tmp/conformance.log; exit 1; }
tail -2 /tmp/conformance.log

echo "== 3/4 版本守卫 =="
guard_out="$(bash "$TOOLCHAIN_DIR/tests/version_guard.sh")"
echo "$guard_out"
case "$guard_out" in
  ALLOW_PUSH=1*) : ;;
  *) echo "守卫不放行 —— 不动"; exit 0 ;;
esac

echo "== 4/4 提交推送 =="
cd "$REPO_DIR"
if git diff --quiet && git diff --cached --quiet && [ -z "$(git status --porcelain)" ]; then
  echo "工作区干净 —— 无需提交"
  exit 0
fi
git add -A
git commit -q -m "chore: 定时任务自动提交（测试全绿 + 版本守卫放行）$(date '+%Y-%m-%d %H:%M %Z')"
git push origin main || { echo "推送失败"; exit 1; }
echo "✓ 已推送 $(git log --oneline -1)"
