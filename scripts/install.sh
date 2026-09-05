#!/usr/bin/env bash
# HSL/DHV 一键安装器（macOS / Linux）
#
# 用法：
#   curl -fsSL https://raw.githubusercontent.com/myh2026/harness-specification-language/main/scripts/install.sh | bash
#
# 环境变量：
#   HSL_VERSION   指定版本（如 0.2.56），默认 latest
#   BIN_DIR       安装目录（默认 ~/.local/bin）
#   HSL_REPO      仓库（默认 myh2026/harness-specification-language，测试用）
#   GITHUB_TOKEN  可选——匿名 API 限流时携带（普通下载无需）
set -euo pipefail

REPO="${HSL_REPO:-myh2026/harness-specification-language}"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
VERSION="${HSL_VERSION:-}"

err()  { echo "✗ $*" >&2; exit 1; }
log()  { echo "→ $*"; }
ok()   { echo "✓ $*"; }

# ── 1. 平台 / 架构检测 ────────────────────────────────────
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
case "${OS}-${ARCH}" in
  linux-x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
  darwin-arm64)  TARGET="aarch64-apple-darwin"     ;;
  darwin-x86_64) TARGET="x86_64-apple-darwin"      ;;
  *) err "不支持的平台 ${OS}-${ARCH}（支持 Linux x86_64 / macOS arm64 / macOS x86_64）" ;;
esac
log "平台: ${OS}-${ARCH} → ${TARGET}"

# ── 2. 解析版本（默认 latest）────────────────────────────
api_get() {
  if [ -n "${GITHUB_TOKEN:-}" ]; then
    curl -fsSL -H "Authorization: Bearer ${GITHUB_TOKEN}" "$1"
  else
    curl -fsSL "$1"
  fi
}
if [ -z "$VERSION" ]; then
  log "解析最新版本…"
  VERSION="$(api_get "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep -oP '(?<="tag_name": ")[^"]+' | head -1 | tr -d 'v')" \
    || err "无法获取最新版本（检查网络、指定 HSL_VERSION，或设 GITHUB_TOKEN）"
fi
[ -n "$VERSION" ] || err "版本号为空"
log "版本: v${VERSION}"

ASSET="dhv-v${VERSION}-${TARGET}.tar.gz"
BASE="https://github.com/${REPO}/releases/download/v${VERSION}"

# ── 3. 下载 + sha256 校验 ─────────────────────────────────
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

log "下载 ${ASSET} …"
curl -fSL -o "${TMP}/${ASSET}" "${BASE}/${ASSET}" \
  || err "下载失败（v${VERSION} 可能还没有 ${TARGET} 产物）"

if curl -fsSL -o "${TMP}/sha256sums.txt" "${BASE}/sha256sums.txt"; then
  EXPECT="$(grep " ${ASSET}\$" "${TMP}/sha256sums.txt" | awk '{print $1}' || true)"
  if [ -n "$EXPECT" ]; then
    if command -v sha256sum >/dev/null 2>&1; then
      ACTUAL="$(sha256sum "${TMP}/${ASSET}" | awk '{print $1}')"
    elif command -v shasum >/dev/null 2>&1; then
      ACTUAL="$(shasum -a 256 "${TMP}/${ASSET}" | awk '{print $1}')"
    else
      ACTUAL=""
      echo "⚠ 系统无 sha256 工具，跳过校验"
    fi
    if [ -n "$ACTUAL" ]; then
      [ "$ACTUAL" = "$EXPECT" ] || err "sha256 校验失败（期望 ${EXPECT}，实际 ${ACTUAL}）"
      ok "sha256 校验通过"
    fi
  fi
else
  echo "⚠ 该版本无 sha256sums.txt，跳过校验（v0.2.56 前的版本）"
fi

# ── 4. 安装 ──────────────────────────────────────────────
tar xzf "${TMP}/${ASSET}" -C "${TMP}"
[ -f "${TMP}/dhv" ] || err "tarball 中未找到 dhv 可执行文件"

mkdir -p "${BIN_DIR}"
install -m 0755 "${TMP}/dhv" "${BIN_DIR}/dhv"
ok "已安装 ${BIN_DIR}/dhv"

# ── 5. 自检 + PATH 提示 ──────────────────────────────────
if OUT="$("${BIN_DIR}/dhv" --version 2>&1)"; then
  ok "自检: ${OUT}"
else
  err "安装完成但自检失败: ${OUT}"
fi

case ":${PATH}:" in
  *":${BIN_DIR}:"*) ;;
  *)
    echo "⚠ ${BIN_DIR} 不在 PATH 中，请添加（建议写入 ~/.bashrc 或 ~/.zshrc）:"
    echo "    export PATH=\"${BIN_DIR}:\$PATH\""
    ;;
esac

echo
echo "完成。用法：dhv check <file.hsl> / dhv run / dhv emit --out <dir>"
