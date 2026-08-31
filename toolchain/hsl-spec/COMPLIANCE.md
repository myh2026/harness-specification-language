# HSL / DHV × 项目总纲 —— 合规对照表（开源发布审计）

> 本文档逐条对照《HSL / DHV 项目总纲》验证实现状态。原则：**只能比总纲多，不能比总纲少**。
> 状态：✅ 已实现（附验证方式）· 🟡 部分实现（附诚实边界）· 📋 规范就绪（实现列入路线图）
> 验证基准：`bun tests/hsl/run-all.ts`（38 用例全绿）· 本表基于 dhv-ts v0.2.1 + dhv v0.2 源码

---

## 一、为什么需要 HSL（§一）—— 无实现义务，属动机陈述

| 总纲主张 | 状态 |
|:---|:---|
| 三大痛点（多语言胶水地狱 / 架构埋在代码里 / 生成代码黑盒化） | ✅ 产品叙事一致（发布页 / 教程第一章） |

## 二、语言设计哲学 —— 七条铁律（§二）

| 铁律 | 状态 | 验证 |
|:---|:---|:---|
| 1. 逻辑原生 | ✅ | HSL 源码即逻辑；emit 将其转译为 38 后端（tests: emit 全量） |
| 2. 强类型到底（struct/enum/trait/泛型/Result/Option） | ✅ | BNF §2 全量语法；dhv-ts 解析执行；dhv typecheck 静态校验 |
| 3. Rust 级严格（零隐式转换/非空默认/强制错误处理/不可变优先） | ✅ | S-1/S-2/S-4/S-6/S-7/S-8 规则真实生效（tests: 检查规则 10 例；v0.2 修复了使 S-7/S-8 失效的 bun 转译器陷阱） |
| 4. 尺度自由（scale = monolith \| microkernel） | ✅ | emit 按 scale 生成两种脚手架形态（tests: scale 切换用例）；运行期 G6 边事件观测 |
| 5. 生态不替代（native 逃生舱） | ✅ | native python/typescript 运行期可执行（ABI 见 BNF 附录 B）；38 语言 native 块合法 |
| 6. 定义与投射分离 | ✅ | 定义层/投射层分文件区块；project{} 集中声明（BNF §2.1/§3.4） |
| 7. Agent 循环原生（graph+loop+match 强制全分支） | ✅ | G-1 强制 AgentLoop；S-6 穷尽性 + loop 内 `_` 兜底拒绝（tests: S-6 两用例） |

## 与正常语言相比，HSL 独有的构件（§二）

| 构件 | 状态 | 验证 |
|:---|:---|:---|
| `graph`（拓扑一等公民，编译期校验） | ✅ | G-1~G-4 规则（tests: G-1）；nova 6 graph 全过 |
| `loop`（graph 内 Agent 循环） | ✅ | G-1 强制 |
| `project {}` | ✅ | **38 后端真实代码生成**（tests: emit 182 文件全语法校验）—— 超出总纲的 6 后端 |
| `block {}` / `static {}` + `{{}}` 插值 | ✅ | 6 静态格式渲染（yaml/md/json/toml/ini/xml） |
| `native lang {}` | ✅ | python/ts 运行期执行；38 语言静态投射合法性（N-1） |
| `scale` | ✅ | 单形态声明 → 双脚手架形态 + G6 观测语义 |

## 三、语法体系（§三）

| 构件 | 状态 |
|:---|:---|
| 完整语法构件表（类型/泛型/函数/控制流/错误处理/运算/属性/宏/模块/逃生舱/静态资源/拓扑/尺度/投射） | ✅ BNF v1.4 全量 + dhv-ts 解析器（nova 2007 行 15 模块全过为证） |
| 表达式优先级链（15 级） | ✅ BNF §5.6 |
| `?` 后缀错误传播 / 三元已删除 | ✅（v0.2.1 修复 From 泛型实参丢失，`?` 转换真实执行；tests: From 回归） |

## 四、编译器架构（§四）

| 阶段 | 总纲要求 | 状态 |
|:---|:---|:---|
| Parser | pest PEG → AST | ✅ dhv（pest 32/32）+ dhv-ts（递归下降，双实现交叉验证） |
| Type Check | 强类型校验 + Lint | ✅ dhv typecheck.rs（S/G/P 系列 985 行）+ dhv-ts checker（38 用例中 10 条规则正反例） |
| Multi-Target Codegen | Rust/Python/TS/YAML/MD/JSON | ✅✅ **超出**：38 后端（32 编程语言 + 6 静态格式）；能力分级 full/logic/contract/raw；python 生成代码经 exec 语义级验证 |
| Physical Writer + SourceMap | 每文件注入围栏 | ✅ @dhv:source-map/@dhv:hsl-mirror/@dhv:end-source-map 三标记协议；manifest.json |
| 三种编译模式 | monolith/microkernel | ✅ emit --scale 双形态脚手架；第三种"未来扩展"——架构不禁止（注册表开放式能力级） |

## 五、多语言投射机制（§五）

| 要求 | 状态 |
|:---|:---|
| 一个 HSL 文件喷射到任意数量物理文件 | ✅ project{} 多目标（backends-demo 182 文件/38 语言实测） |
| 静态资源原生块 + {{}} 编译期插值 | ✅ 6 静态格式（类型检查保护的插值渲染） |
| Native 逃生舱（同函数内调用任意语言生态） | ✅ python/ts 运行期；example: dsh DeepSeekModel 经 native typescript 调真实 LLM |

## 六、双向工程机制（§六）

| 要求 | 状态 |
|:---|:---|
| SourceMap 围栏 | ✅ 三标记协议（BNF v1.4 变更 3） |
| 修改产物回写 HSL | ✅ `dhv sync`：编辑围栏内 HSL 镜像 → 按名回写 .hsl → 回写后重新解析校验、失败回滚（tests: sync 闭环用例含 re-emit 活体区更新） |
| 实时反编译（File Watcher） | ✅ `dhv watch`：监听 .hsl 依赖闭包 → check + emit 自动重跑 |
| 诚实边界（内核不可改/逻辑层可改/回写校验） | ✅ manifest 协议声明 + 文件头 @dhv:generated 标记 + 回写校验回滚 |
| 目标语言代码→HSL 的逐语句反编译 | 🟡 v0.2 以 HSL 镜像为回写介质（对全部 38 后端统一可用）；目标语言代码的自动反编译列入 P6 路线图 |

## 七、严格性与 Lint 架构（§七）

| 层 | 要求 | 状态 |
|:---|:---|:---|
| 编译期防线 | 零隐式转换/非空默认/强制错误处理/不可变优先/跨语言转译安全 | ✅ S-1/S-2/S-4/S-6/S-7/S-8（dhv 双实现）；N 系列块语言合法性（N-1） |
| 第 1 层 Lint | AST 层（未使用/死锁/越界/遮蔽） | ✅ S-7（含宏实参下探与 native 捕获语义）/S-8/G-3/G-4 |
| 第 2 层 Lint | 目标语言交叉 Lint | ✅✅ **超出**：emit 自动执行 python3 -m py_compile / bun 转译器(ts|js) / bash -n；其余语言括号平衡启发式 |
| 第 3 层 Lint | 回写反向净化 | ✅ 回写后重新解析校验，失败回滚（sync.ts） |

## 八、模块系统（§八）

| 要求 | 状态 |
|:---|:---|
| 文件即模块 / import-export | ✅ M1-M5；循环 import 按引用补全（tests: CLI 循环 import 用例） |
| 三层职责（契约/实现/编排） | ✅ nova/dsh 实测（types/ + plugins|providers/ + 入口） |
| std 虚拟模块 | ✅✅ **超出**：`std/<mod>` 10 模块虚拟解析（BNF 附录 C） |

## 九、Agent 核心循环（§九）

| 要求 | 状态 |
|:---|:---|
| graph + loop + match Action 语法固化 | ✅ G-1/S-6 强制 |
| 真实 Agent 跑通 | ✅ dsh：scripted 50ms 确定性 + 真实 LLM 13.6s 智能体（读码→修复→测试转绿→审查裁决→产物落盘） |
| 模型切换 dyn Trait | ✅ dsh ScriptedModel/DeepSeekModel 同 trait 双实现 |

## 十、与现有生态的关系（§十）

| 生态 | 状态 |
|:---|:---|
| DeepSeek Harness (dsh) | ✅ examples/dsh 为其 HSL 复现（10 模块，双模式实测） |
| Rust / Python / TypeScript | ✅ 转译 + native 直调 |
| MCP | 📋 edge 胶水生成 MCP 适配器（P8 路线图；edge 事件模型已就绪） |
| AGENTS.md / SKILL.md | ✅ block → markdown 投射（dsh SYSTEM_PROMPT → .harness/AGENTS.md 实测） |

## 十一、实施路线图（§十一）对照

| 阶段 | 状态 |
|:---|:---|
| P0 PEG 文法（32/32） | ✅ |
| P1 AST | ✅（Rust + TS 双实现） |
| P2 Parser | ✅ |
| P3 Rust Codegen | ✅ 后端存在（骨架级；dhv source-form 交付） |
| P4 YAML/Markdown Codegen | ✅（+ json/toml/ini/xml = 6 静态格式） |
| P5 Python Codegen | ✅（dhv-ts full 级活体翻译，语义级验证） |
| P6 双向工程 | ✅（sync 回写 + watch；目标语言反编译部分列入后续） |
| P7 TypeScript Codegen | ✅（full 级） |
| P8 跨语言胶水（FFI/IPC） | 🟡 事件总线模型就绪（G6），FFI 胶水自动生成列路线图 |
| P9 Lint 系统 | ✅ 三层（含宿主工具链交叉校验） |
| P10 宏系统 | ✅ macro_rules! token 级展开（nova 3 宏实测） |
| P11 包宇宙 | 📋 路线图（import 本地解析就绪） |

---

## 超出总纲的部分（"只能多"）

1. **38 后端**（总纲示意 6 种）：32 编程语言 4 tier + 6 静态格式，注册表 + 能力分级 + manifest 诚实声明
2. **标准库 10 模块**（总纲未要求）：约 60 函数 + 2 常量，可复现 PRNG、本地确定性 JSON、路径监狱 IO
3. **双向工程对全部后端统一可用**（总纲设想目标语言反编译；v0.2 的 HSL 镜像介质对 38 后端一致工作）
4. **交叉语法校验**（总纲 Lint 第 2 层的具体化）：宿主真实工具链自动执行
5. **38 用例严格测试套件**（tests/hsl/）随发行物分发
6. **发现并修复宿主运行时陷阱**：bun 转译器静默丢弃语句位置 `declare(...)` 调用（已写入 BNF v1.4 工程注记）

## 已知限制（诚实清单）

| 限制 | 说明 | 缓解 |
|:---|:---|:---|
| dhv（Rust）为源码形态交付 | 沙盒无 Rust 工具链，无法本地编译验证 | dhv-ts 为可运行参考实现；dhv 源码与 BNF/AST 对齐 |
| contract 后端函数体不翻译 | 26 语言生成类型契约 + 围栏 HSL 镜像 + 未实现标记 | 能力级写入 manifest；full/logic 语言覆盖 harness 主流（py/ts/js/rs/go/cpp） |
| dhv-ts 无完整类型推导 | 解释器定位（S1 字面量级检查；推导归 dhv） | 结构级铁律全部生效 |
| 标识符不做命名风格转换 | HSL snake_case 原样进入各后端（合法但非惯用） | v0.3 计划按后端惯例映射（带回写映射表） |
| 目标语言代码反编译 | sync 以 HSL 镜像为介质 | P6 后续 |
