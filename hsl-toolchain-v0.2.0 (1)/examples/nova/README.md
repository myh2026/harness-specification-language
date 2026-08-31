# NOVA — 多 Agent 深度研究系统

> 用 **HSL** 编写的生产级示例项目：给定一个研究问题，规划 → 并行研究 → 代码验证 → 证据审查 → 综合报告。
>
> 本项目是 HSL 语言的"压力测试场"：**2,000+ 行真实规模源码**，全量压榨 BNF v1.2 的表达力，验证这门语言面对真实工程时的完备性。

## 它做什么

```
用户问题
   │
   ▼
┌─────────────────────────────────────────────────────────┐
│  Nova（主编排 graph）                                     │
│                                                          │
│  Planner ──► 检索任务 ──► Researcher ──► Finding          │
│                │                          │              │
│                ▼                          ▼              │
│          验证任务 ──► Verifier      SafetyPolicy(闸门)    │
│                │                          │              │
│                ▼                          ▼              │
│          审查任务 ──► Critic ────► Synthesizer ──► Report │
│                                │                         │
│                                ▼                         │
│                          InMemoryStore（记忆固化）          │
└─────────────────────────────────────────────────────────┘
```

五个 agent 各自是一个独立 `graph`，由主编排 `graph Nova` 经条件 edge 调度。
`scale = microkernel` 时每条 edge 编译为事件总线订阅（G6）；切到 `monolith`
则编译为直接调用——**同一份源码，两种架构形态，零改动**。

## 目录结构

```
nova/
├── nova.hsl                 # 入口：主编排 graph + scale + project（11 目标投射）
├── types/
│   ├── actions.hsl          # Action 枚举家族（编排/研究/验证词汇 + 事件 + 进度）
│   ├── state.hsl            # ResearchState 全局状态 + Task DAG + Finding/Citation
│   └── errors.hsl           # NovaError 错误树 + From 自动包装（? 的唯一通道）
├── providers/
│   ├── llm.hsl              # LLMProvider trait + DeepSeek(native python)
│   │                        #   + Ollama(#[cfg(lang: rust)] 条件编译)
│   ├── tools.hsl            # ToolExecutor trait + WebSearch(python)/CodeRunner(rust 沙盒)
│   │                        #   + Recall(faiss) + 'retry 循环标签 + 泛型重试执行器
│   └── embed.hsl            # EmbeddingProvider + BGE-M3(native python, Vec<f64> 映射)
├── memory/
│   └── store.hsl            # MemoryStore trait + 倒排索引 + 记忆压缩（泛型 consolidate）
├── policy/
│   └── safety.hsl           # 安全策略：能力域白名单 + 预算水位 + 内容闸门（双保险）
├── agents/
│   ├── planner.hsl          # 规划 graph：问题 → 任务 DAG（node/edge/局部 item）
│   ├── researcher.hsl       # 研究 graph：检索→阅读→笔记 子循环（三跳条件环）
│   ├── coder.hsl            # 验证 graph：断言生成 → 沙盒运行 → 判定回填
│   ├── critic.hsl           # 审查 graph：证据强度评分 → Accept/Revise/Reject
│   └── synthesizer.hsl      # 综合 graph：置信度排序 → 逐章生成报告
├── config/
│   └── resources.hsl        # block×3 + static×1：YAML 配置/AGENTS.md/JSON schema/事件表
└── README.md
```

## 运行

```bash
# 需要 dhv v0.1+（Rust 工具链编译后）
cd nova
dhv check nova.hsl          # 静态检查：G/P/N/S 规则全量校验
dhv emit nova.hsl           # 投射生成：
#   src/main.rs                (rust)      主编排入口
#   src/agents/*.rs            (rust)      五个 agent graph
#   src/providers/deepseek.py  (python)    LLM 客户端
#   src/tools/web_search.py    (python)    检索工具
#   src/tools/code_runner.rs   (rust)      沙盒工具
#   config/nova.yml            (yaml)      运行配置（插值已求值）
#   .harness/AGENTS.md         (markdown)  Agent 纪律提示词
#   config/task.schema.json    (json)      任务 schema
#   docs/events.md             (markdown)  事件总线文档
```

环境变量：`DEEPSEEK_API_KEY`、`TAVILY_API_KEY`（仅研究任务需要）。

## HSL 特性覆盖矩阵

本项目存在的意义之一：**逐条验证 BNF v1.2 的每个构件在真实工程里都有用武之地。**

| BNF 构件 | 条款 | 用在 | 证明的价值 |
|---|---|---|---|
| `graph` + AgentLoop | §3.1 G1 | 全部 6 个 graph | 拓扑即代码：agent 协作结构可静态校验 |
| `node` 声明（含无初始化） | §3.1 | planner/critic（类型锚点） | microkernel 尺度下的插件注入位 |
| `edge` 多跳 + `on Guard` | §3.1 G3/G5 | researcher（三跳条件环） | 条件环合法性的真实用例 |
| `edge ... with attrs` | §3.1 | nova（backpressure/durable） | 边属性承载部署语义 |
| graph 泛型参数 | §3.1 | researcher(task, question) | graph 作为可参数化单元 |
| graph 体内局部 item | §3.1 M5 | planner（seed_tasks/expand） | 拓扑私有辅助，不污染模块 |
| `block` / `static` | §3.2 | resources.hsl ×4 | 四种静态资源（YAML/MD/JSON/表格） |
| `{{}}` 编译期插值 | §3.2 N4 | nova_config ×3 处 | 常量注入配置，改一处全量更新 |
| `native python` | §3.3 | llm/tools/embed | 直接吃 Python 生态（openai/tavily/faiss） |
| `native rust` | §3.3 | coder 沙盒 | 子进程隔离 + 超时击杀，零 FFI |
| `project` 11 目标 | §3.4 P1-P7 | nova.hsl | rust/python 双活 + 3 静态后端 |
| `scale = microkernel` | §3.5 P6 | nova.hsl（唯一入口） | 事件总线架构一键切换 |
| `#[capability(...)]` | §3.6 | 5 处 | 能力域编译期处决（net_connect/process_spawn/file_read） |
| `#[cfg(lang: rust)]` | §3.6 | OllamaClient | 条件编译：python 投射时剥离 |
| `#[derive(...)]` | §3.6 | 全部数据类型 | Debug/Clone/PartialEq 派生 |
| `#[deny]` / `#[doc]` | §3.6 | safety/llm | lint 升级 + 文档投射 docstring |
| `macro_rules!` | §2.13 | macros.hsl ×3 | impl_tool/guard_check/with_retry 样板消除 |
| import 三态 | §2.3 | nova.hsl | `{a,b}` / `* as m` / 全部真实使用（S7） |
| export 可见性 | §2.3 | 全部公共项 | 模块边界即 API 契约 |
| 泛型 + where | §2.8 | tools/store | `T: ToolExecutor + Clone` 多约束 |
| trait 默认实现 | §2.5 | LLMProvider | complete_within 预算裁剪 |
| impl 固有方法 | §2.5 | 8 处 | 关联函数工厂 + 决策方法 |
| From 错误转换 | §5.9 | errors.hsl ×3 | `?` 的唯一显式通道 |
| 循环标签 | §2.12 D5 | tools 'retry | 标签唯一用途的示范 |
| 全进制字面量 | §1.5 | state.hsl | 0x20 / 0xF_4240 / 0b… / 8_000 |
| 数据枚举 + 判别式 | §2.4 | AgentRole 位掩码 | 判别式即事件路由位 |

### S 铁律自检（dhv check 应零告警）

- **S1** 零隐式转换：全部条件显式比较或 bool 字段（`len() == 0`、`>=`、`verified`）
- **S2** 非空默认：零裸 `unwrap`——Option 一律 `match`/`cloned().unwrap_or` 兜底
- **S4** 不可变优先：`mut` 仅 17 处，全部真实变更（计数器/游标/缓冲）
- **S6** 穷尽 match：所有 enum match 带穷尽分支（含 `_` 兜底的显式分类）
- **S7** 未使用即错误：每个 import 与绑定都被消费（静态扫描确认）
- **S8** 变量遮蔽：同名绑定换用新名（`next_id`/`working`/`sorted`）

## 设计决策记录

| 决策 | 理由 |
|---|---|
| graph 调用约定 `GraphName::run(args)` | 与 P7 对齐：graph 投射为函数，调用语法与 trait 关联函数同形 |
| 安全双闸门 | `#[capability]` 编译期处决 + `SafetyPolicy.gate_finding` 运行时审查 |
| native 块只做 IO | N3 纪律：解析/判定留在 HSL 侧，保证跨后端语义一致 |
| 无初始化 node | critic 的 scanner/scorer 为 microkernel 插件位（运行时注入实现） |
| 兜底 Finding | 任务无法产出时保持拓扑闭合，避免编排层 None 传染 |

## 已知边界（诚实声明）

- 沙盒无 Rust 工具链，本项目以源码形态交付，`dhv check/emit` 语句为**设计目标接口**；
- LLM 结构化输出解析（parse_tasks）为骨架实现，生产需 schema 校验闭环；
- InMemoryStore 为演示存储，trait 边界已为 Redis/Postgres 预留。

— NOVA · MIT · 与 [HSL 工具链](../..) 一起发行
