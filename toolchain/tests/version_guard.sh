#!/usr/bin/env bash
# ============================================================================
# 版本守卫（push 前置检查）
#
# 原则（用户要求）：为防服务器/环境重置丢工作，仅当「本地版本比远端 README 记录
# 的版本更新」或「本地有新增实质改动（测试全绿）」时才允许提交推送；否则不动。
#
# 用法: bash tests/version_guard.sh
# 输出: ALLOW_PUSH=1 或 ALLOW_PUSH=0（同时打印原因）
# ============================================================================
set -uo pipefail

TOOLCHAIN_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_DIR="$(dirname "$TOOLCHAIN_DIR")"

local_ver="$(grep -m1 '^version' "$TOOLCHAIN_DIR/dhv/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')"

# 远端 README 中的当前版本（### x.y.z 标题或 🛡️ 当前版本 小节里的 v 号）
remote_ver="$(git -C "$REPO_DIR" show origin/main:README.md 2>/dev/null \
  | grep -oE '\*\*v[0-9]+\.[0-9]+\.[0-9]+\*\*' | head -1 | tr -d 'v*' )"
remote_ver="${remote_ver:-0.0.0}"

ver_ge() { # $1 >= $2 ?
  [ "$(printf '%s\n%s\n' "$2" "$1" | sort -V | head -1)" = "$2" ]
}

# 本地工作区相对 origin/main 是否有差异
if ! git -C "$REPO_DIR" diff --quiet origin/main -- 2>/dev/null; then
  has_diff=1
else
  has_diff=0
fi

if [ "$local_ver" = "$remote_ver" ] && [ "$has_diff" = 0 ]; then
  echo "ALLOW_PUSH=0  # 本地 v$local_ver 与远端一致且无差异 —— 不动"
  exit 0
fi

if [ "$local_ver" = "$remote_ver" ] && [ "$has_diff" = 1 ]; then
  echo "ALLOW_PUSH=1  # 版本持平 v$local_ver 但工作区有新改动（先确认测试全绿再提交）"
  exit 0
fi

if ver_ge "$local_ver" "$remote_ver"; then
  echo "ALLOW_PUSH=1  # 本地 v$local_ver >= 远端 v$remote_ver"
  exit 0
fi

echo "ALLOW_PUSH=0  # 本地 v$local_ver 落后于远端 v$remote_ver —— 拒绝推送"
exit 0
