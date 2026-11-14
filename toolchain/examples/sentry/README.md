# SENTRY — 用 HSL 写一个「事故分诊」Agent Harness

SENTRY 是一个**确定性可验证**的 on-call 事故分诊 harness，用
[HSL（Harness Specification Language）](https://github.com/myh2026/harness-specification-language)
v0.2.72 / BNF v1.5.0 写成，位于 HSL 仓库的 `toolchain/examples/sentry/`。

它不是一个"聊天机器人"：模型可以自由推理，但**严重级别由 HSL 代码计算、报告先过确定性
结构闸门、再交独立审计模型裁决**，任何一步都能在 CI 里零外联、可复现地重放。

> 边界：工作区里的一切（日志、指标、配置、报告）都是**固定夹具**，harness 本身不
> 连真实监控系统；`--model deepseek` 时接真实 LLM（OpenAI 兼容接口），其余全部离线。

---

## 快速开始

前置：`bun`（v0.2.72 的 dhv-ts 是零依赖 TS 实现）。Windows 上 `$host.shell.run`
通过 `bash -c` 执行命令，需要 PATH 中第一个 `bash` 是真正的 bash（Git Bash 可用；
`C:\Windows\System32\bash.exe` 是 WSL 存根，会失败）。

```bash
cd toolchain

# 1) 静态检查（S/G/P/N 规则 + 12 模块链接）
bun dhv-ts/src/main.ts check examples/sentry/sentry.hsl
# → dhv-ts check: 0 error(s), 0 warning(s) ✓ 12 个模块全部通过检查

# 2) 确定性回放：scripted 模型 + v2 fixture（零外联，CI 友好）
cp -r examples/sentry/workspace /tmp/sentry-ws
bun dhv-ts/src/main.ts run examples/sentry/sentry.hsl \
  --workspace /tmp/sentry-ws \
  --task "checkout 支付链路自 02:13 起大面积超时，请分诊并给出复盘报告" \
  --model scripted \
  --fixture examples/sentry/fixtures/triage-demo.json \
  --out /tmp/sentry-out
# → ✓ harness 返回 Ok；verdict: accepted；severity: SEV-1

# 3) 接真实 LLM（需 OPENAI_API_KEY / DEEPSEEK_API_KEY 等，见 dhv-ts README）
bun dhv-ts/src/main.ts run examples/sentry/sentry.hsl \
  --workspace /tmp/sentry-ws --task "..." --model deepseek --max-turns 12

# 4) 投射工程库：project{} + 38 后端代码生成
bun dhv-ts/src/main.ts emit examples/sentry/sentry.hsl --out /tmp/sentry-emit
# → ✓ emit 完成：56 个文件（56 个通过语法校验）+ manifest.json（exit 0）
```

> Windows：把 `cp -r` 换成 `Copy-Item -Recurse` 即可；PowerShell 里务必保证 PATH 中
> 第一个 `bash` 是 Git Bash（见"已知限制"），否则工具调用会被计为失败。
> 也可一键跑通（check → 确定性回放 → emit）：
> `powershell -ExecutionPolicy Bypass -File scripts\demo.ps1`。

`check` / `run` / `emit` 均可带 `--scale monolith|microkernel` 覆盖默认值
（本项目 `scale = microkernel`：边事件走事件总线）。

---

## 它长什么样

```
toolchain/examples/sentry/
├── sentry.hsl                 入口：graph 编排 + main + project{} 投射声明
├── types/                     errors / messages / signals / state
├── config/resources.hsl       block 系统提示词、审计 Rubric、HARNESS_CONFIG、static INCIDENT_SCHEMA
├── providers/incident_model.hsl  IncidentModel trait + DeepSeek / Scripted 实现
├── tools/                     files.hsl（读/写/改/列 + cap_chars）、shell.hsl（bash 工具）
├── logic/analysis.hsl         Analyzer：日志扫描、指标解析、严重级别评分、报告闸门
├── agents/                    executor.hsl（协议解析 + 工具调度 + 重试）、auditor.hsl（独立裁决）
├── workspace/                 事故现场夹具：日志、指标、配置、runbook、verify-report.js
├── fixtures/                  v2 剧本：triage-demo.json（成功路径）+ loop-forever.json（预算耗尽路径）
├── scripts/demo.ps1           Windows 一键演示：check → 确定性回放 → emit
└── docs/                      调研笔记（RUNTIME-NOTES.md / LANG-NOTES.md）
```

数据流（G3 条件环，环上每条边都带 `on` guard）：

```
analyzer ──Signal::EvidenceReady──> model
model    ──Action::Tool───────────> executor
executor ──Event::Observed────────> model
model    ──Action::Done───────────> auditor
auditor  ──Verdict::Revise────────> model   （accept 则落盘退出）
```

模型每回合只能输出**恰好一个 JSON 对象**（`tool` 或 `done`）；解析失败会把错误文本
反馈给模型重试，预算记在 `protocol_violations`。transcript 每回合都回灌给模型，
避免模型"忘记自己做过什么"而重复调用。

---

## 三个可验证关卡（本项目的重点）

1. **确定性侦察（analyzer，不花模型 token）**
   - `scan_logs`：统计总行数、ERROR/WARN 行数、`UPSTREAM_TIMEOUT` 次数、首个 ERROR 时间戳、异常组件；
   - `parse_metrics`：从 `metrics.json` 读出 p99/基线/错误率/QPS/CPU/部署时间；
   - `grade`：**HSL 侧评分矩阵**把事实映射成 SEV-1..4（p99 超基线 3 倍 +2、错误率
     ≥25% +2 且 ≥40% 再 +1、CPU ≥90% +1、15 分钟内有过部署 +1、ERROR 行 ≥50 +1，
     ≥5 分即 SEV-1）。级别是计算结果，不是模型"印象"。
2. **报告闸门 `check_report`（确定性，模型不可绕过）**
   - `report.md` 必须 ≥400 字符、包含 `## 根因/## 证据/## 影响面/## 处置` 四节、
     包含评分矩阵给出的严重级别、`UPSTREAM_TIMEOUT`、`payment-db`、`p99`；
   - 未通过 → 违规项注入 transcript 打回重写，计入 `gate_failures`。
3. **独立审计者 `audit`（同源 trait、独立模型实例、独立对话、独立 fixture 轨道）**
   - 只有 `auditor.act([system(RUBRIC), user(evidence+transcript)])` 返回
     `{"verdict":"accept"}` 才判定完成；`revise` 会带着理由打回。
   - 同时仓库里的 `workspace/verify-report.js` 是**第二套独立实现**，由模型通过
     bash 工具调用，交叉验证 HSL 闸门没写错。

工具瞬时故障由 **HSL 侧重试**（`is_retryable` + 退避，最多 3 次尝试），
预算闸门（`max_turns` / `max_bash_calls` / `max_output_chars`）在运行时强制。

---

## 确定性剧本（fixture v2）

`fixtures/triage-demo.json`：

```json
{
  "tracks": {
    "triage": ["{\"action\":\"tool\",\"tool\":\"list_files\"}", "…", "{\"action\":\"done\",…}"],
    "audit":  ["{\"verdict\":\"accept\"}"]
  },
  "faults": [
    { "target": "shell.run", "nth": 1, "kind": "error",
      "message": "injected transient exec failure (first bash attempt)" }
  ]
}
```

- `tracks`：每个 track 对应 `$host.fixture.next(track)` 的一条独立消费流
  （`triage` 给执行者、`audit` 给审计者），耗尽即抛错，不会静默放行；
- `faults`：第 `nth` 次命中 `target` 时按 `kind` 注入 `error/deny/empty/corrupt/slow`
  故障，本剧本注入一次 `shell.run` 瞬时错误，用来演示重试路径
  （实测 `retries: 1`、`failures: 0`、`attempts=2`，事件流里可见 `fault_injected`）。

剧本模式将完整动作序列写死，因此每个数字都可复现：`turns: 9`、`tool_calls: 9`、
`bash_calls: 1`、`retries: 1`、`transcript_lines: 22`。

---

## 运行产物

`--out` 目录（真实落盘 = CLI 的 `events.jsonl`+`run.json`，加上 harness 里
`$host.artifacts.write` 自己写的文件；CLI 打印的"产物"提示行只是文案）：

| 文件 | 谁写的 | 内容 |
| --- | --- | --- |
| `events.jsonl` | CLI | 事件流：`run_start` / `node` / `edge` / `observed` / `fault_injected` / `run_end` |
| `run.json` | CLI | 运行元信息：`ts` / `ok` / `elapsed_ms` / `model` / `task` / `events` |
| `run-report.md` | harness | 本次运行的裁决、严重级别、预算消耗、确定性侦察摘要 |
| `incident.json` | harness | 机器可读事故摘要（evidence + stats，`ok`=是否真正完成分诊） |
| `transcript.jsonl` | harness | 完整对话流（role/content 每行一条） |
| `report.md` | 模型 | 写入**工作区**的复盘报告（被 `check_report` 闸门校验；不在 `--out`） |

`fs.*` 只能在 `--workspace` 内读写（越狱直接抛错）；`artifacts.write` 落在 `--out`。
`microkernel` 尺度下每条走到的边都发 `edge` 事件（如 `model -> executor on Tool`），
但实测 `--scale monolith` 的事件流除 `scale` 字段外完全相同——尺度真正影响的是
`emit` 出来的工程脚手架，不是运行期拓扑。

---

## 投射工程库（emit）

`sentry.hsl` 末尾的 `project{}` 把编排、CLI、全部类型与函数投射到 TypeScript，
资源投射到静态格式：

```hsl
project {
    Sentry          -> "src/sentry/topology.ts"      : typescript,
    main            -> "src/sentry/cli.ts"           : typescript,
    SENTRY_PROMPT   -> "prompts/triage-agent.md"     : markdown,
    HARNESS_CONFIG  -> "config/sentry.yml"           : yaml,
    INCIDENT_SCHEMA -> "config/incident.schema.json" : json,
    rules {
        struct -> "src/sentry/types/{name}.ts" : typescript,
        enum   -> "src/sentry/types/{name}.ts" : typescript,
        trait  -> "src/sentry/types/{name}.ts" : typescript,
        fn     -> "src/sentry/logic/{name}.ts" : typescript,
    }
}
```

实测：`56 个文件（56 通过语法校验）+ manifest.json`，`X-1`（类型引用未投射）与
`X-5`（函数/枚举变体未投射）检查均为 0 告警。manifest 逐文件记录
`lang` / `tier` / `bytes` / `items` / `contract_fallbacks` / `syntax_check`：
纯 HSL 逻辑的函数（如 `grade`、`scan_logs`）是 `full` 活体翻译，逐语句生成 TS 代码
（`grade.ts` 里能直接看到评分矩阵的 `if/else`）；把计算委派给 `native typescript` 的函数
（如 `parse_action`、`read_file`）会在 manifest 里列入 `contract_fallbacks`，TS 侧保留
类型契约骨架（native 块内容不参与翻译）。语法校验由 bun 的 TS 转译器完成
（`syntax_tool: "bun transpiler (ts)"`）；生成文件带 `@dhv:source-map` /
`@dhv:hsl-mirror` 源码镜像围栏（可被 `dhv sync` 回写）。
这里验证的是 **projection 图闭合 + 各文件语法合法**，不是把 harness 编译成可运行的 TS 副本。

> 小坑：`fn main() -> Result<(), HarnessError>` 投射到 TS 时返回类型会被渲染成
> `()`（TS 里是语法错误）。所以 `main` 返回 `Result<bool, HarnessError>`（成功标志），
> `write_artifacts` 返回 `Result<u32, HarnessError>`（落盘产物数），投射即全部合法；
> 运行结束 CLI 会回显 `true`，属正常。

---

## 实测记录（Windows 11 + bun v1.1x + dhv-ts v0.2.72）

```text
$ bun dhv-ts/src/main.ts check examples/sentry/sentry.hsl
dhv-ts check: 0 error(s), 0 warning(s)
✓ 12 个模块全部通过检查

$ bun dhv-ts/src/main.ts run examples/sentry/sentry.hsl --workspace <copy> --task "…" \
    --model scripted --fixture examples/sentry/fixtures/triage-demo.json --out <out>
[sentry] 工具瞬时失败已重试（attempts=2）
[sentry] model declared done: 定位 payment-db 连接池耗尽导致 UPSTREAM_TIMEOUT，timeout_ms 已调至 8000 止血并产出复盘
[sentry] artifacts: run-report.md / incident.json / transcript.jsonl
[sentry] main 完成：本次落盘 3 个产物
✓ harness 返回 Ok（1.0 s）
# verdict: accepted / severity: SEV-1 / turns 9 / tool_calls 9 / bash_calls 1 / retries 1 / failures 0

$ bun dhv-ts/src/main.ts emit examples/sentry/sentry.hsl --out <emit>
✓ emit 完成：56 个文件（56 个通过语法校验）+ manifest.json
```

事故现场是**故意写好的**：`artifacts/gateway.log` 40 行（10 ERROR / 2 WARN）、
`metrics.json`（p99 1830ms vs 基线 180ms、错误率 0.42）、8 分钟前有过部署——
评分矩阵算出 6 分 → SEV-1，与剧本里模型写出的报告一致。

### 失败路径：预算耗尽（同样可复现）

`fixtures/loop-forever.json` 让 triage 轨道连续 40 次 `read_file`，配合 `--max-turns 3`：

```bash
bun dhv-ts/src/main.ts run examples/sentry/sentry.hsl --workspace <copy> \
  --task "预算耗尽测试" --model scripted \
  --fixture examples/sentry/fixtures/loop-forever.json \
  --max-turns 3 --out <out>
```

实测 121 ms 结束：`verdict: budget_exhausted`、`completed: false`、`turns: 3`，
`incident.json` 里 `"ok": false`。注意此时进程仍以 `Ok(true)` 退出——`main` 的返回值只表示
"harness 脚本正常跑完"，**是否分诊成功由 `incident.json.ok` / `run-report.md` 的
`completed` 表达**，这才是 CI 应该断言的字段。预算闸门只会中断，不会被模型"宣称完成"绕过。

---

## 与官方 dsh 示例的差异

| | `examples/dsh` | `examples/sentry` |
| --- | --- | --- |
| 域 | 改代码的 coding agent | on-call 事故分诊 |
| 确定性分析 | 无（纯模型） | `analyzer` 先算日志/指标/严重级别 |
| 报告校验 | 无 | `check_report` 结构闸门 + `verify-report.js` 运行时交叉验证 |
| 审计 | 单模型 reviewer | 独立审计模型 + 独立 fixture 轨道 |
| 工具故障 | 直接失败计数 | `is_retryable` + 退避重试（HSL 侧） |
| 投射 | 显式 5 个条目 | `project{}` + `rules{}` 全量投射（56 文件） |

---

## 已知限制

- **Windows 上 `$host.shell.run` 需要真 bash**：PATH 里第一次出现的 `bash` 生效。
  若只有 WSL 存根（`C:\Windows\System32\bash.exe`），bash 调用会返回乱码错误并被计为
  `failures`；把 Git Bash 的 `bin` 放到 PATH 最前即可（本机实测
  `D:\学习\py学习\Git\bin`）。
- fixture 轨道耗尽、`json.fields` 解析失败、文件越狱访问都会**抛错终止**而不是静默降级，
  这是刻意的：确定性回放失败必须响亮。
- `--model deepseek` 依赖环境的 LLM 凭据/网络，本目录的实测全部走 scripted 模式。
- emit 的校验只到"每个投射文件语法合法"这一层，不做 TS 类型检查；运行时权威实现始终
  是 `.hsl` 源码 + dhv-ts 解释器（Rust `dhv` 编译器与 dhv-ts 的翻译可能存在细节差异）。
- `emit` 有**任何 warning 就 exit 1**（CI 里应把 `emit exit 0` 当断言）；`--no-validate`
  仍会打印 "N 个通过语法校验"，别信这一行。
- 别用 PowerShell `Set-Content -Encoding UTF8` 改 `.hsl`：它会加 BOM，checker 报
  `期望项定义，得到 "﻿" (file:1:2)`。用 `[IO.File]::WriteAllText($p, $s, [Text.UTF8Encoding]::new($false))`。
- `$host.dhv.check` / `$host.dhv.run` 在 bun 1.4.3 上会**挂起**（本机实测 >60 s），
  harness 内不要用，脚本化验证走 CLI。
- `dhv sync` 默认 `--root` 是生成文件所在目录，回写源码时务必显式传 `--root`。

更细的实测清单见 `docs/RUNTIME-NOTES.md`（438 行，运行期/CLI/fixture/emit 全量）
与 `docs/LANG-NOTES.md`（579 行，语言表面 + S-1..S-19 诊断原文 + TOP-20 陷阱）。

---

## 参考

- HSL 仓库：<https://github.com/myh2026/harness-specification-language>
- 语言总纲：`guide/HSL-GUIDE.md`（4198 行）
- 规格：`toolchain/hsl-spec/BNF.md`、`COMPLIANCE.md`
- 运行时：`toolchain/dhv-ts/src/host.ts`（$host API 与 fixture v2）
- 本项目实测笔记：`docs/RUNTIME-NOTES.md`、`docs/LANG-NOTES.md`
- 上游反馈清单：`docs/UPSTREAM-FEEDBACK.md`（14 条实测问题 + 给 AI 写作场景的能力建议）
- 一键演示：`scripts/demo.ps1`
- 姊妹示例：`toolchain/examples/dsh/`（coding agent）、`toolchain/examples/nova/`（多 Agent 研究）
