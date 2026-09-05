#!/usr/bin/env bash
# ============================================================================
# HSL 工具链双编译器一致性回归（dhv Rust ↔ dhv-ts TypeScript）
#
# 用法: bash tests/run_conformance.sh
#
# 内容:
#   1. dhv(cargo build) 与 dhv-ts(bun) 对同一用例集分别 check
#   2. 用例集 = 内置示例 + tests/fixtures（parse/check/errors/modules）
#   3. 断言双方「通过/失败」结论一致（允许诊断文案差异），且与期望一致
#   4. 值级一致性：fixtures/values 逐字面量比对解析值（L-11 教训）
#   5. 行为级一致性：fixtures/emit 生成物真实运行 vs interp（L-15 教训）
#   任一不一致 → 退出码 1
#
# 原则: **修复一个 bug，就锁定一个用例**（见 dhv/tests/conformance.rs）。
# ============================================================================
set -uo pipefail

TOOLCHAIN_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$TOOLCHAIN_DIR"

DHV="dhv/target/release/dhv"
DHV_TS=(bun dhv-ts/src/main.ts)
FIX="dhv/tests/fixtures"

PASS_COUNT=0
FAIL_COUNT=0
declare -a DISAGREEMENTS=()

if [[ ! -x "$DHV" ]]; then
  echo "== cargo build (release) =="
  (cd dhv && cargo build --release) || { echo "dhv 构建失败"; exit 1; }
fi

# 结论: pass / fail（仅看退出码，忽略诊断文案差异）
conclusion_dhv() { "$DHV" check "$1" >/dev/null 2>&1 && echo pass || echo fail; }
conclusion_ts()  { "${DHV_TS[@]}" check "$1" >/dev/null 2>&1 && echo pass || echo fail; }

record() { # name expect actual_dhv actual_ts
  local name="$1" expect="$2" a="$3" b="$4"
  if [[ "$a" != "$b" ]]; then
    FAIL_COUNT=$((FAIL_COUNT + 1))
    DISAGREEMENTS+=("不一致: $name 期望[$expect] dhv=$a dhv-ts=$b")
  elif [[ "$expect" != "$a" ]]; then
    FAIL_COUNT=$((FAIL_COUNT + 1))
    DISAGREEMENTS+=("期望落空: $name 期望[$expect] 实际=$a")
  else
    PASS_COUNT=$((PASS_COUNT + 1))
    printf '  ✓ %-70s [%s]\n' "$name" "$a"
  fi
}

echo "== 1/5 内置示例（必须双端 pass）=="
for ex in examples/nova/nova.hsl examples/dsh/dsh.hsl \
          examples/backends-demo/agent.hsl examples/backends-demo/model.hsl; do
  record "$ex" pass "$(conclusion_dhv "$ex")" "$(conclusion_ts "$ex")"
done

echo "== 2/6 单文件 fixtures（parse|check → pass；errors → fail）=="
for f in "$FIX"/parse/*.hsl "$FIX"/check/*.hsl; do
  record "$(basename "$f")" pass "$(conclusion_dhv "$f")" "$(conclusion_ts "$f")"
done
for f in "$FIX"/errors/*.hsl; do
  record "$(basename "$f")" fail "$(conclusion_dhv "$f")" "$(conclusion_ts "$f")"
done

echo "== 3/6 多模块工程（pass_* → pass；fail_* → fail）=="
for d in "$FIX"/modules/*/; do
  [[ -f "${d}root.hsl" ]] || continue
  name="$(basename "$d")"
  case "$name" in
    pass_*) expect=pass ;;
    fail_*) expect=fail ;;
    *) continue ;;
  esac
  record "modules/$name" "$expect" "$(conclusion_dhv "${d}root.hsl")" "$(conclusion_ts "${d}root.hsl")"
done

# ---- v0.2.54 值级一致性（L-11 教训）：逐字面量比对解析值 ----
# 结论对拍看不见 parse 层静默损坏（dhv 曾把 250u8 解析为 0 且 check 双端全绿）。
echo "== 4/6 值级一致性（fixtures/values 逐字面量比对）=="
if VALUE_OUT=$(bun tests/run_value_conformance.ts 2>&1); then
  VALUE_PASS=$(printf '%s' "$VALUE_OUT" | grep -oE '值级一致性: [0-9]+ 通过' | grep -oE '[0-9]+')
  echo "  ✓ values/ 语料 $VALUE_PASS 个文件值级全一致"
  PASS_COUNT=$((PASS_COUNT + 1))
else
  FAIL_COUNT=$((FAIL_COUNT + 1))
  DISAGREEMENTS+=("值级一致性失败：见上方明细（value conformance 报不一致）")
  printf '%s\n' "$VALUE_OUT" | sed 's/^/    /'
fi

# ---- v0.2.55 行为级一致性（L-15 教训）：emit 产物真实运行 vs interp ----
# 语法校验绿灯看不见「生成物运行成功但行为为空」（fn main 只定义不调用，
# 零输出零副作用 exit 0）。本段把第五轮值级对拍延伸到运行行为层：
# interp run ↔ emit→python3/bun 真实运行，emit:: 标记行逐行全等。
echo "== 5/6 行为级一致性（fixtures/emit 生成物真实运行比对）=="
if EMIT_OUT=$(bun tests/run_emit_conformance.ts 2>&1); then
  EMIT_PASS=$(printf '%s' "$EMIT_OUT" | grep -oE '行为级对拍: [0-9]+ 通过' | grep -oE '[0-9]+')
  EMIT_RUNS=$(printf '%s' "$EMIT_OUT" | grep -oE '[0-9]+ 个后端真实运行' | grep -oE '^[0-9]+')
  echo "  ✓ emit/ 语料 $EMIT_PASS 个文件行为级全一致（$EMIT_RUNS 个后端真实运行）"
  PASS_COUNT=$((PASS_COUNT + 1))
else
  FAIL_COUNT=$((FAIL_COUNT + 1))
  DISAGREEMENTS+=("行为级一致性失败：见上方明细（emit conformance 报不一致）")
  printf '%s\n' "$EMIT_OUT" | sed 's/^/    /'
fi

# ---- v0.2.56 S-18 预警对等（#L-22 native 值模型断层）----
# 退出码对拍看不见「警告是否产出」（警告不改变结论）。本段直接比对双端的
# S-18 预警出现性：warn fixture 双端都必须告警且退出 0；legal fixture
# （$host.make 通道在体）双端都必须零告警。注意 dhv 渲染 S-S18 / dhv-ts
# 渲染 S-18，各 grep 各的形态。
echo "== 6/6 S-18 预警对等（#L-22：native 值模型断层，警告级） =="
S18_WARN_FIX="$FIX/check/S18_native_foreign_warn.hsl"
S18_LEGAL_FIX="$FIX/check/S18_host_make_legal.hsl"
s18_case() { # file expect_warn(yes|no)
  local f="$1" expect="$2"
  local out_dhv out_ts rc_dhv rc_ts w_dhv w_ts
  out_dhv=$("$DHV" check "$f" 2>&1); rc_dhv=$?
  out_ts=$("${DHV_TS[@]}" check "$f" 2>&1); rc_ts=$?
  w_dhv=$(printf '%s' "$out_dhv" | grep -c 'WARNING\[S-S18\]' || true)
  w_ts=$(printf '%s' "$out_ts" | grep -c 'warning\[S-18\]' || true)
  if [[ $rc_dhv -ne 0 || $rc_ts -ne 0 ]]; then
    FAIL_COUNT=$((FAIL_COUNT + 1))
    DISAGREEMENTS+=("S-18 parity: $(basename "$f") 退出码非零 dhv=$rc_dhv ts=$rc_ts")
    return
  fi
  if [[ "$expect" == yes ]]; then
    if (( w_dhv >= 1 && w_ts >= 1 )); then
      PASS_COUNT=$((PASS_COUNT + 1))
      echo "  ✓ S-18 双端告警对等: $(basename "$f")（dhv=${w_dhv} ts=${w_ts} 条）"
    else
      FAIL_COUNT=$((FAIL_COUNT + 1))
      DISAGREEMENTS+=("S-18 预警缺失: $f dhv=${w_dhv} ts=${w_ts} 条（期望双端 ≥1）")
    fi
  else
    if (( w_dhv == 0 && w_ts == 0 )); then
      PASS_COUNT=$((PASS_COUNT + 1))
      echo "  ✓ S-18 零误报对等: $(basename "$f")（双端 0 条）"
    else
      FAIL_COUNT=$((FAIL_COUNT + 1))
      DISAGREEMENTS+=("S-18 误报: $f dhv=${w_dhv} ts=${w_ts} 条（期望双端 0）")
    fi
  fi
}
s18_case "$S18_WARN_FIX" yes
s18_case "$S18_LEGAL_FIX" no

echo
echo "== 结果 =="
echo "通过: $PASS_COUNT  失败: $FAIL_COUNT"
if (( FAIL_COUNT > 0 )); then
  printf '%s\n' "${DISAGREEMENTS[@]}"
  exit 1
fi
echo "双编译器一致性: 全部一致 ✓（含值级 + 行为级 + S-18 预警对等）"
