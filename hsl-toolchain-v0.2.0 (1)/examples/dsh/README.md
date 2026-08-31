# DSH — DS Harness 的 HSL 复现

> 用 HSL（Harness Specification Language）写的**多 Agent 编码助手 harness**，
> 复现 DeepSeek 风格 agent harness 的核心循环：**planner → tool → observe → reviewer**。
> 它不是伪代码——`dhv-ts` 解释器让它真实运行，接真实 LLM，修真实的 bug。

## 拓扑：一次会话的状态机

```
                 ┌──────────────────────────────────────┐
                 │                                      │
                 ▼         on Action::Tool              │ on Event::Observed
  ┌─────────┐  edge  ┌───────────┐  edge   ┌─────────┐ │
  │  model  │ ─────► │ executor  │ ──────► │  model  │─┘
  └────┬────┘        └───────────┘         └─────────┘
       │                                            ▲
       │ on Action::Done                            │ on Verdict::Revise
       ▼                                            │
  ┌───────────┐   verdict: Accept → break（出环）    │
  │ reviewer  │ ────────────────────────────────────┘
  └───────────┘
```

四条边全部带 `on Guard`（G3 条件环合法）；每轮工具执行后 `observed` 事件入总线（G6）。

## 项目结构（10 个 HSL 模块）

```
dsh/
├── dsh.hsl                  入口：Dsh 主 graph + main + project + scale + bump! 宏
├── types/
│   ├── messages.hsl         Message / ToolCall（5 工具封闭枚举）/ Action / Verdict / Event
│   ├── state.hsl            Policy / SessionStats / SessionState / Report + 进制常量
│   └── errors.hsl           ProviderError / HarnessError + From×2（? 的转换通道）
├── providers/
│   └── model.hsl            ModelProvider trait + DeepSeekModel（真实 LLM）+ ScriptedModel（剧本）
├── tools/
│   ├── workspace.hsl        read/write/edit/list（#[capability(file_read,file_write)]，输出封顶）
│   └── shell.hsl            bash（#[capability(process_spawn)]，白名单+超时）
├── agents/
│   ├── executor.hsl         严格 JSON 协议解析 + Toolkit 工具分发（S6 穷尽 match）
│   └── reviewer.hsl         审查闸门（.await? 后缀链 + From 跨类型转换）
├── config/
│   └── resources.hsl        SYSTEM_PROMPT / REVIEW_RUBRIC / HARNESS_CONFIG / EVENTS_SCHEMA
│                            （block+static 双形态 + {{}} 编译期插值）
├── workspace/               待修复的任务工作区（stats.ts 带 bug + 失败测试）
└── fixtures/fix-variance.json   ScriptedModel 剧本（确定性 CI 运行）
```

## 运行

### 模式一：确定性剧本（CI 可复现，零外联）

```bash
cp -r examples/dsh/workspace /tmp/dsh-ws
bun dhv-ts/src/main.ts run examples/dsh/dsh.hsl \
  --workspace /tmp/dsh-ws \
  --task "stats.ts 中 variance() 的分母用错了（应为样本方差 n-1），且 median() 尚未实现。请修复并让 stats.test.ts 全部通过。" \
  --model scripted \
  --fixture examples/dsh/fixtures/fix-variance.json \
  --out /tmp/dsh-run
```

**实测记录**：✅ 50ms，5 turns，5 tool_calls，0 failures，verdict=accepted。
剧本序列：list_files → read 测试 → read 实现 → edit_file（真的改文件）→ bash（真的跑测试）→ done → review accept。运行后 `bun stats.test.ts` 全 PASS。

### 模式二：真实 LLM（DeepSeek 风格，经 z-ai 网关）

```bash
bun dhv-ts/src/main.ts run examples/dsh/dsh.hsl \
  --workspace /tmp/dsh-llm-ws \
  --task "同上" \
  --model deepseek --max-turns 10 --out /tmp/dsh-llm-run
```

**实测记录**：✅ 13.6s，verdict=accepted，工作区测试全 PASS。真实决策链（transcript.jsonl）：

```
assistant: {"tool": "read_file", "path": "stats.ts"}
assistant: {"tool": "read_file", "path": "stats.test.ts"}        ← 先读实现再读测试
assistant: {"tool": "edit_file", ...old_text: "// BUG: 样本方差应为 n-1 分母..."}   ← 修分母
assistant: {"tool": "edit_file", ...old_text: "// TODO: 缺失 median 实现"}          ← 补 median
assistant: {"tool": "bash", "command": "cd /tmp/... && deno test stats.test.ts"}   ← 被安全闸门拦截
assistant: {"tool": "bash", "command": "deno test stats.test.ts"}                  ← 再被拦截
assistant: {"tool": "bash", "command": "node stats.test.ts"}                       ← 收敛到白名单
assistant: {"action": "done", "summary": "修复了 stats.ts 中的两个问题..."}
```

事件总线（events.jsonl，microkernel 尺度）记录了完整拓扑观测：
`node(model/executor/reviewer)` → `edge(model→executor on Tool)`×7 → `observed`×7 →
`capability_denied`×2（**运行时安全闸门真实拦截**）→ `edge(model→reviewer on Done)` → `run_end`。

### 静态投射

```bash
bun dhv-ts/src/main.ts emit examples/dsh/dsh.hsl --out /tmp/dsh-emit
# emit .harness/AGENTS.md      ← SYSTEM_PROMPT（markdown，插值已渲染）
# emit config/harness.yml      ← HARNESS_CONFIG（yaml，{{DEFAULT_MAX_TURNS}} → 24）
# emit config/events.schema.json ← EVENTS_SCHEMA（合法 JSON schema）
```

## Harness 的六道防线（为什么这是"真 harness"而非玩具）

| # | 防线 | 实现 | 实测触发 |
|:---|:---|:---|:---|
| 1 | 编译期能力域 | `#[capability(file_read, file_write, process_spawn, net_connect)]` 声明式划分工具权限 | check 校验 |
| 2 | 运行时路径监狱 | `$host.fs` 所有操作限制在 workspace 内 | 越界即抛错 |
| 3 | shell 白名单 | 首词白名单 + 超时 + 输出封顶 | LLM 运行中拦截 `cd`/`deno` 两次 |
| 4 | 预算闸门 | max_turns / max_bash_calls / max_output_chars（HSL 侧判定） | 剧本与 LLM 双模式生效 |
| 5 | 协议纠错回路 | 模型输出违反 JSON 协议时，错误反馈重试（预算内） | LLM 运行中触发 1 次后恢复 |
| 6 | 审查闸门 | done 之后独立 reviewer 裁决 Accept/Revise，Revise 带笔记回环 | 拓扑完整，accept 出环 |

## 设计决策（真实工程发现）

1. **模型回合必须入转录**：早期版本只记录工具观察、不记录 assistant 消息，导致真实 LLM
   看不到自己已读过文件而无限重读。修复后转录为标准 `system/user/assistant/tool` 四角色流。
2. **严格 JSON 协议 + 纠错回路**：解析失败不直接崩溃，把错误作为 user 消息反馈，
   模型在 1-2 轮内回到协议（真实运行中观察到 `action: "read_file"` 误用与 markdown
   围栏两种违规，均被纠正）。
3. **`$host.json.fields` 的类型纪律**：JSON 顶层字段字符串化为 `HashMap<String, String>`
   进入 HSL，而不是让动态对象图穿透到 HSL 侧（N2 精神）。
4. **native 只做效应，逻辑留在 HSL**：prompt 组装、协议校验、预算判定、裁决分发全部是
   纯 HSL；native 块只做 API 调用与文件/进程效应（N1 纪律）。
5. **ScriptedModel 是一等公民**：同一条 HSL 代码路径，插上剧本是确定性 CI，插上
   DeepSeek 是真实智能体——这是 harness 可测试性的关键。

## BNF 特性覆盖矩阵（本项目用到的构件）

| 构件 | 用在哪 | 证明了什么 |
|:---|:---|:---|
| graph + mut 参数 + 返回类型 | dsh.hsl `graph Dsh(mut state) -> Result<Report, E>` | 主编排即类型化状态机 |
| node 声明（带初始化 + `?`） | model/executor/reviewer 三节点 | 节点=可执行单元，失败即早退 |
| edge + on Guard 条件环 | 四条边 | G3 合法环 + G6 运行期观测 |
| AgentLoop + S6 穷尽 match | Action/ToolCall/Verdict/Option 四处 | 新增变体=编译期处决，直面分支 |
| macro_rules! | `bump!(state.stats.tool_calls, 1)` | token 级展开链路 |
| GraphName::run 调用约定 | `Dsh::run(state)` | BNF v1.3 正式化的约定 |
| native typescript / python | llm 网关 / fs / shell / json | 双逃生舱真实执行 |
| block + static + `{{}}` 插值 | 四个资源块（提示词/YAML/JSON schema） | 模式 A 词法 + 编译期插值 |
| `?` + From 跨类型 | ProviderError→HarnessError、String→HarnessError | 唯一显式错误通道 |
| turbofish `parse::<u32>()` | Policy::from_config | 零隐式转换（S1） |
| trait + 默认方法 + 双 impl | ModelProvider 双实现 | 多态提供方插拔 |
| 进制/分隔字面量 | `0x0C` `4_000` | §1.5 全进制覆盖 |
| project + scale | 5 目标投射 + microkernel | P3/P4 合法 + emit 落地 |

## 产物清单（每次运行）

```
<outdir>/
├── report.md         运行报告（任务/裁决/预算消耗，HSL format! 渲染）
├── transcript.jsonl  完整对话转录（四角色流）
├── events.jsonl      事件总线（node/edge/observed/capability_denied/run_*）
└── run.json          机器可读摘要（ok/elapsed/model/task/events）
```
