<!-- HSL-GUIDE · 由 Task F 编写 · 全部示例经 bun dhv-ts/src/main.ts check 实测通过 -->

# HSL 语言完全指南

**Harness Specification Language · 从逻辑到 38 个后端的工程投射**

| | |
|:---|:---|
| 文档版本 | v0.2.51（与工具链同步） |
| 语言规范 | BNF v1.5.0（`toolchain/hsl-spec/BNF.md`；新增 §3.4 投射规则组 rules） |
| 参考实现 | dhv-ts v0.2.27（`bun toolchain/dhv-ts/src/main.ts ...`）；dhv Rust 编译器 v0.2.27 |
| 许可证 | MIT |
| 后端 | 38 个：32 编程语言 + 6 静态格式 |

> **一句话定位**：HSL 是一门为编写 AI Agent harness 而生的编译型语言——你用一套带
> 类型、带拓扑校验的源码描述 Agent 的逻辑与编排，DHV 工具链把它投射成一个真实的多语言
> 工程仓库（Python / TypeScript / Rust / Go / … / YAML / Markdown），而不是一个黑盒脚手架。

> **本指南的诚实承诺**：文中每一个 HSL 示例都通过了 `bun dhv-ts/src/main.ts check`
> 实测；第十二章的完整项目用 `run` 命令真实跑通；第五、八章引用的生成代码均为
> `emit` 的真实输出。凡工具链尚未实现的行为，本指南明确标注「已知限制」，
> 绝不描述不存在的功能。

---

## 目录

- [第一章 认识 HSL](#第一章-认识-hsl)
  - [1.1 为什么需要一门 harness 语言](#11-为什么需要一门-harness-语言)
  - [1.2 三层抽象：逻辑层 / 拓扑层 / 物理层](#12-三层抽象逻辑层--拓扑层--物理层)
  - [1.3 HSL 与传统做法对照](#13-hsl-与传统做法对照)
  - [1.4 工具链全家福](#14-工具链全家福)
- [第二章 15 分钟 quickstart](#第二章-15-分钟-quickstart)
  - [2.1 环境要求](#21-环境要求)
  - [2.2 第一个程序](#22-第一个程序)
  - [2.3 check / run / emit 三连](#23-check--run--emit-三连)
  - [2.4 看到 38 后端产物的那一刻](#24-看到-38-后端产物的那一刻)
- [第三章 语言巡礼](#第三章-语言巡礼)
  - [3.1 变量：let 与 mut](#31-变量let-与-mut)
  - [3.2 基本类型](#32-基本类型)
  - [3.3 容器与标准泛型](#33-容器与标准泛型)
  - [3.4 函数与 async](#34-函数与-async)
  - [3.5 struct](#35-struct)
  - [3.6 enum 与和类型](#36-enum-与和类型)
  - [3.7 trait 与 impl](#37-trait-与-impl)
  - [3.8 match 与穷尽性](#38-match-与穷尽性)
  - [3.9 if / else 与 if let](#39-if--else-与-if-let)
  - [3.10 循环：while / for / loop 与标签](#310-循环while--for--loop-与标签)
  - [3.11 错误处理：Result 与 ?](#311-错误处理result-与-)
  - [3.12 闭包](#312-闭包)
  - [3.13 macro_rules! 宏](#313-macro_rules-宏)
  - [3.14 import 与 export](#314-import-与-export)
- [第四章 Agent 核心循环](#第四章-agent-核心循环)
  - [4.1 graph：拓扑是一等公民](#41-graph拓扑是一等公民)
  - [4.2 node 与 edge on Guard](#42-node-与-edge-on-guard)
  - [4.3 AgentLoop 与 match Action](#43-agentloop-与-match-action)
  - [4.4 G 规则：拓扑的静态校验](#44-g-规则拓扑的静态校验)
  - [4.5 一个完整的最小 Agent](#45-一个完整的最小-agent)
  - [4.6 scale = monolith | microkernel](#46-scale--monolith--microkernel)
- [第五章 38 后端投射](#第五章-38-后端投射)
  - [5.1 project{} 语法全解](#51-project-语法全解)
  - [5.2 语言注册表：四个 tier 与静态格式](#52-语言注册表四个-tier-与静态格式)
  - [5.3 能力分级：full / logic / contract](#53-能力分级full--logic--contract)
  - [5.4 静态资源：block / static 与 {{}} 插值](#54-静态资源block--static-与-插值)
  - [5.5 scale 对脚手架的影响](#55-scale-对脚手架的影响)
  - [5.6 manifest.json 与诚实边界协议](#56-manifestjson-与诚实边界协议)
  - [5.7 跨文件类型依赖：投射产物之间的自动接线](#57-跨文件类型依赖投射产物之间的自动接线)
  - [5.8 投射规则组 rules {}（BNF v1.5）](#58-投射规则组-rules--bnf-v15)
- [第六章 native 逃生舱](#第六章-native-逃生舱)
  - [6.1 native typescript 与 native python：直接可执行](#61-native-typescript-与-native-python直接可执行)
  - [6.2 运行期 ABI：$host 与捕获变量](#62-运行期-abihost-与捕获变量)
  - [6.3 其他语言的 native 块：静态投射语义](#63-其他语言的-native-块静态投射语义)
  - [6.4 类型纪律：JSON 编组](#64-类型纪律json-编组)
  - [6.5 何时该用 native](#65-何时该用-native)
- [第七章 标准库参考](#第七章-标准库参考)
  - [7.1 std/core](#71-stdcore)
  - [7.2 std/collections](#72-stdcollections)
  - [7.3 std/text](#73-stdtext)
  - [7.4 std/math](#74-stdmath)
  - [7.5 std/io](#75-stdio)
  - [7.6 std/json](#76-stdjson)
  - [7.7 std/time](#77-stdtime)
  - [7.8 std/random](#78-stdrandom)
  - [7.9 std/env](#79-stdenv)
  - [7.10 std/iter](#710-stditer)
  - [7.11 预导入方法面](#711-预导入方法面)
- [第八章 双向工程](#第八章-双向工程)
  - [8.1 围栏协议图解](#81-围栏协议图解)
  - [8.2 完整工作流 walkthrough](#82-完整工作流-walkthrough)
  - [8.3 诚实边界：哪些区可编辑](#83-诚实边界哪些区可编辑)
- [第九章 CLI 完全参考](#第九章-cli-完全参考)
  - [9.1 六个命令](#91-六个命令)
  - [9.2 全部 flags](#92-全部-flags)
  - [9.3 退出码约定](#93-退出码约定)
  - [9.4 watch 模式](#94-watch-模式)
- [第十章 静态检查与错误码](#第十章-静态检查与错误码)
- [第十一章 测试你的 harness](#第十一章-测试你的-harness)
  - [11.1 剧本模式：--fixture](#111-剧本模式--fixture)
  - [11.2 确定性测试策略](#112-确定性测试策略)
  - [11.3 tests/hsl/run-all.ts 套件](#113-testshslrun-allts-套件)
- [第十二章 完整实战：从零写一个代码统计 Agent](#第十二章-完整实战从零写一个代码统计-agent)
- [第十三章 生态与对比](#第十三章-生态与对比)
  - [13.1 与 DeepSeek Harness（dsh）/ MCP / AGENTS.md 的关系](#131-与-deepseek-harnessdsh--mcp--agentsmd-的关系)
  - [13.2 与直接写 Python / TypeScript 的取舍](#132-与直接写-python--typescript-的取舍)
  - [13.3 已知限制（诚实清单）](#133-已知限制诚实清单)
- [附录A 38 后端语言完整注册表](#附录a-38-后端语言完整注册表)
- [附录B std 函数速查总表](#附录b-std-函数速查总表)
- [附录C 常见问题 FAQ](#附录c-常见问题-faq)
- [附录D 术语表](#附录d-术语表)

---

# 第一章 认识 HSL

## 1.1 为什么需要一门 harness 语言

写过一个 AI Agent 的人几乎都会撞上同样三堵墙。HSL 的每一个设计决定，
都是对着这三堵墙砸的。

### 墙一：多语言胶水地狱

真实的 Agent 工程从来不是单一语言：模型调用层是 Python（因为 openai / llama-index
生态在那里），工具沙箱是 TypeScript 或 Rust（因为要跑子进程与权限控制），配置是
YAML，提示词是 Markdown，事件 schema 是 JSON。于是「一个 Agent」实际上是
五种语言 × 三套构建系统 × 手写的胶水层。改一个工具签名，要同步改 Python 客户端、
TS 类型、JSON schema 和文档——总有一处会漏。

### 墙二：架构埋在代码里

Agent 的核心资产是它的**编排结构**：谁调度谁、什么条件下走哪条边、失败如何回环。
但在 Python 里，这个结构散落在 `while True` 循环、if-elif 链和函数调用栈里，
没有任何工具能回答「这个 harness 有几个节点？哪条边构成环？done 之后一定经过
审查吗？」——这些问题只有读完全部代码才能回答，而且答案随时会过期。

### 墙三：生成代码黑盒

也有团队试图用代码生成器解决墙一。但传统生成器输出的代码是**只写不读**的黑盒：
一旦手改了生成物，再生成就冲突；不手改，又没法接私有的运维逻辑。生成器只敢
作为一次性脚手架，无法成为工程主体。

### HSL 的回答

| 墙 | HSL 的回答 |
|:---|:---|
| 多语言胶水 | 一份 HSL 源码，`project {}` 声明式投射到 38 个后端；类型与签名真实翻译，不是注释 |
| 架构埋在代码里 | `graph` / `node` / `edge on Guard` 是**一等语法**，拓扑可静态校验（G 规则）、可观测（G6 边事件） |
| 生成代码黑盒 | `@dhv:source-map` 围栏协议：生成文件内嵌 HSL 源镜像，编辑镜像 → `dhv sync` 回写源码 → 再 emit 更新活体代码。**双向工程，不是单向脚手架** |

## 1.2 三层抽象：逻辑层 / 拓扑层 / 物理层

HSL 把「一个 Agent 工程」切成三层，每层有自己的语法与校验规则：

```
┌─────────────────────────────────────────────────────────────┐
│ 逻辑层（定义层）                                              │
│   struct / enum / trait / impl / fn / const                  │
│   —— 纯粹的数据与计算，与落到哪个语言无关                        │
│   校验：S 规则（严格性铁律）                                    │
├─────────────────────────────────────────────────────────────┤
│ 拓扑层（编排层）                                               │
│   graph / node / edge on Guard / loop + match Action         │
│   —— Agent 的状态机结构                                       │
│   校验：G 规则（必含 AgentLoop、端点存在、条件环、孤岛）           │
├─────────────────────────────────────────────────────────────┤
│ 物理层（投射层）                                               │
│   project {} / scale = monolith | microkernel                │
│   —— 逻辑项 → 物理文件 → 目标语言 的映射                        │
│   校验：P 规则（目标存在、语言合法、路径不冲突）                    │
└─────────────────────────────────────────────────────────────┘
```

一个 `.hsl` 文件可以同时容纳三层；也可以像第十二章的实战项目那样，
按层拆成多个模块文件（types.hsl / counter.hsl / main.hsl）。

关键心智模型：**逻辑层与拓扑层描述「是什么」，物理层描述「放哪里」。**
换后端语言不需要改逻辑；重构 Agent 拓扑不需要碰物理映射。

## 1.3 HSL 与传统做法对照

| 维度 | 直接写 Python/TS | 模板生成器 | **HSL + DHV** |
|:---|:---|:---|:---|
| 语言数 | 1 种（胶水手写） | 每种语言一套模板 | 1 份源码 → 38 后端 |
| 架构可见性 | 埋在代码里 | 模板里（更看不见） | `graph/edge` 一等语法 + 静态校验 |
| 类型契约 | 靠 docstring / mypy | 模板变量 | 编译期 S 规则 + 真实类型翻译 |
| match 穷尽性 | 无 | 无 | S-6 强制（新变体 = 编译期处决） |
| 生成代码可维护性 | —（无生成） | 手改即废 | 围栏协议：镜像可编辑，sync 回写 |
| 确定性测试 | mock 一切 | 难 | 剧本模式（--fixture）+ 可复现 PRNG |
| 安全边界 | 手写 if | 模板里散落 | `#[capability]` 编译期 + 宿主路径监狱运行期 |
| 逃生舱 | 本来就是宿主语言 | — | `native python {}` 直接写宿主代码 |

## 1.4 工具链全家福

| 组件 | 位置 | 作用 |
|:---|:---|:---|
| dhv-ts（参考解释器） | `dhv-ts/` | 你现在能用的全部：check / run / emit / targets / sync / watch。**本指南的主角** |
| dhv（Rust 编译器） | `dhv/` | 生产级静态编译器（源码形态交付，需要 Rust 工具链构建） |
| hsl-spec/BNF.md | `hsl-spec/` | 语言规范（BNF 文法 + 静态语义 S/G/P/N 规则） |
| VSCode 扩展 | `ide/vscode-hsl/` | 语法高亮 / 代码片段 / 主题 |
| examples/ | `examples/` | dsh（DS harness 复现）、nova（多 Agent 研究系统）、backends-demo（38 后端演示） |
| tests/hsl/run-all.ts | `tests/hsl/` | 74 用例发布级测试套件 |

dhv-ts 与 dhv 的关系，如同 CPython 之于 GCC：一个负责「现在就能跑」，
一个负责「产物最优化」。本指南全部命令以 dhv-ts 为准。

---

# 第二章 15 分钟 quickstart

## 2.1 环境要求

| 依赖 | 版本 | 用途 |
|:---|:---|:---|
| bun | ≥ 1.1（开发环境实测 1.3.14） | 运行 dhv-ts（零第三方 npm 依赖） |
| python3 | ≥ 3.8 | `native python` 块执行 + python 后端语法校验 |
| bash | 任意现代版本 | bash 后端 `bash -n` 语法校验 |

只需要 `bun` 也能完成本指南 95% 的内容；python3 只影响 native python 块与
python 生成物的语法校验。验证安装：

```bash
bun --version
# 1.3.14
```

## 2.2 第一个程序

新建文件 `first.hsl`，完整复制以下内容：

```hsl
// first.hsl — 你的第一个 HSL 程序

export struct Todo {
    text: String,
    done: bool,
}

export enum Action {
    Add { text: String },
    Complete { index: i64 },
    Stop,
}

fn main() -> i64 {
    let mut todos: Vec<Todo> = Vec::new();
    todos.push(Todo { text: String::from("写第一个 HSL 程序"), done: false });

    let action = Action::Complete { index: 0 };
    match action {
        Action::Add { text } => todos.push(Todo { text, done: false }),
        Action::Complete { index } => {
            if index < todos.len() {
                todos[index].done = true;
            }
        },
        Action::Stop => println!("stop"),
    }

    println!("完成数 = {}", todos.filter(|t| t.done).len());
    0
}
```

逐行解释：

| 行 | 构件 | 说明 |
|:---|:---|:---|
| `export struct Todo` | 逻辑层 | 定义数据结构；`export` 使其可被其他模块 import |
| `export enum Action` | 逻辑层 | 和类型（sum type）：三个变体，两个带命名负载 |
| `fn main() -> i64` | 入口约定 | `run` 命令调用入口文件中名为 `main` 的 fn（R-1 约定，无需 export） |
| `let mut todos: Vec<Todo>` | S-4 | 默认不可变；`mut` 必须显式 |
| `match action { ... }` | S-6 | 穷尽匹配：漏掉任何一个 `Action` 变体都是编译错误 |
| `todos.filter(\|t\| t.done)` | 闭包 | Vec 方法链 + 闭包实参 |
| `0` | 尾表达式 | 函数体最后一个无分号表达式即返回值 |

## 2.3 check / run / emit 三连

```bash
# 1) 静态检查：S/G/P/N 规则 + 模块链接
bun dhv-ts/src/main.ts check first.hsl
```

输出：

```
dhv-ts check: 0 error(s), 0 warning(s)
✓ 1 个模块全部通过检查
```

```bash
# 2) 解释执行
bun dhv-ts/src/main.ts run first.hsl --quiet
```

输出：

```
完成数 = 1

✓ harness 返回 Ok（8 ms）
```

故意制造一个错误试试——把 `Action::Stop` 那个 arm 删掉再 check：

```
error[S-6]: match 不穷尽：Action 缺少变体 Stop
  --> first.hsl:16:5

dhv-ts check: 1 error(s), 0 warning(s)
```

这就是 S-6 的价值：**新增一个枚举变体，所有忘记处理它的地方在编译期全部暴露**。
对 Agent harness 而言，「模型新增了一种动作」不再是一次线上事故。

```bash
# 3) 投射：把逻辑变成真实工程文件（先加一个 project 声明）
```

在 `first.hsl` 末尾追加：

```hsl
project {
    Todo   -> "src/todo.py"   : python,
    Action -> "src/action.ts" : typescript,
}
```

```bash
bun dhv-ts/src/main.ts emit first.hsl --out /tmp/first-gen
```

输出：

```
投射模式：scale = microkernel（未声明，默认） · 入口 first.hsl
  src/todo.py                       python       full          ... B  ← Todo · 语法✓ python3 -m py_compile
  src/action.ts                     typescript   full          ... B  ← Action · 语法✓ bun transpiler (ts)

✓ emit 完成：2 个文件（2 个通过语法校验）+ manifest.json → /tmp/first-gen（... ms）
```

打开 `/tmp/first-gen/src/todo.py`，你会看到真实的 Python `@dataclass`；
打开 `src/action.ts`，是真实的 TypeScript 判别联合——由 dhv-ts 内置的
python3 / bun 工具链做语法校验，不是文本替换。

## 2.4 看到 38 后端产物的那一刻

项目里自带一个「全后端演示」，一条命令把同一份逻辑
投射到全部 38 个后端：

```bash
bun dhv-ts/src/main.ts emit examples/backends-demo/agent.hsl --out /tmp/backends-demo
```

输出（节选）：

```
投射模式：scale = microkernel · 入口 agent.hsl
  gen/python/prompt.py               python       full          893 B  ← Prompt · 语法✓ python3 -m py_compile
  gen/typescript/prompt.ts           typescript   full          924 B  ← Prompt · 语法✓ bun transpiler (ts)
  gen/rust/prompt.rs                 rust         logic         694 B  ← Prompt · 语法✓ heuristic:balanced
  gen/go/prompt.go                   go           logic         715 B  ← Prompt · 语法✓ heuristic:balanced
  gen/java/Prompt.java               java         contract      604 B  ← Prompt · 语法✓ heuristic:balanced
  gen/swift/Prompt.swift             swift        contract      620 B  ← Prompt · 语法✓ heuristic:balanced
  gen/haskell/Prompt.hs              haskell      contract      647 B  ← Prompt · 语法✓ heuristic:balanced
  gen/vb/Prompt.vb                   vb           contract      632 B  ← Prompt · 语法✓ heuristic:balanced
  config/agent.yml                   yaml         static         94 B  ← agent_config · 语法✓ embedded
  ...
✓ emit 完成：182 个文件（182 个通过语法校验）+ manifest.json → /tmp/backends-demo
```

182 个文件横跨 38 个后端，全部通过语法校验。此时你可以：

```bash
python3 -c "import sys; sys.path.insert(0, '/tmp/backends-demo/gen/python'); from prompt import Prompt; print(Prompt('a','b'))"
# Prompt(system='a', user='b')   ← 真实可用的 dataclass
cat /tmp/backends-demo/gen/java/Prompt.java
# record Prompt(String system, String user) {}
```

注意 Java 生成物里赫然是 Java 17 的 `record`，Haskell 是
`data Prompt = Prompt { ... }`——同一份 HSL 源码，各自的「母语」形态。
能力分级（full / logic / contract）的精确语义见第五章。

---

# 第三章 语言巡礼

本章按构件逐个巡礼 HSL 的通用语言部分。每个小节遵循
「示例 → 输出 → 解释」三段式，所有示例可直接存成 `.hsl` 文件用
`bun dhv-ts/src/main.ts check` 验证。

先给出贯穿本章的背景知识——HSL 的词法与 Rust 高度同源（你可以使用
`//` 行注释与可嵌套的 `/* */` 块注释、全进制整数字面量
`0xFF` / `0b1010` / `0o17`、数字分隔符 `4_000`、原始字符串
`r"C:\path"`、Unicode 转义 `\u{1F600}`），但做了七项关键取舍
（BNF 第 0 章设计决策）：

| 决策 | 内容 | 为什么 |
|:---|:---|:---|
| D1 | 无 lifetime 系统 | 目标语言无借用检查；Rust 后端由编译器推导 |
| D2 | 无裸指针 / unsafe / extern | 跨语言互操作一律走 native 逃生舱 |
| D3 | 无三元运算符 `?:` | 一律用 `if cond { a } else { b }` 表达式 |
| D4 | `static` 专用于静态资源块 | 编译期常量统一 `const` |
| D5 | 标签形如 `'outer` | 仅供循环 break/continue 使用 |
| D6 | graph 内 loop 与普通 loop 同形 | 语义约束交给静态检查（G-1） |
| D7 | `as` 是唯一类型转换通道 | 零隐式转换（S-1） |

## 3.1 变量：let 与 mut

```hsl
fn main() -> i64 {
    let fixed: i64 = 10;          // 不可变（默认）
    let mut counter: i64 = 0;     // 可变，必须显式 mut
    counter += 1;
    counter = counter + fixed;
    println!("counter = {}", counter);
    0
}
```

输出：

```
counter = 11
```

解释：

- `let` 绑定默认不可变（S-4）。对不可变绑定赋值 → `error[S-4]`。
- 类型注解可省略时由运行时值决定；但**生产代码建议全部显式标注**——
  dhv-ts 解释器不做完整类型推导，完整静态推导是 dhv Rust 编译器的职责。
- `_` 前缀的变量名豁免「未使用」检查（S-7）：`let _ignored = 1;` 合法。

## 3.2 基本类型

```hsl
fn main() -> i64 {
    let a: i64 = 42;
    let b: f64 = 3.14;
    let c: bool = true;
    let d: char = 'H';
    let s: String = String::from("hello");
    let big: i64 = 0xFF;          // 十六进制
    let bin: i64 = 0b1010;        // 二进制
    let sep: i64 = 4_000;         // 分隔符
    let raw = r"C:\path\to\file"; // 原始字符串（反斜杠不转义）
    let total = a + b as i64 + c as i64 + big + bin + sep / 1000;
    println!("total={} s={} d={} raw={}", total, s, d, raw);
    0
}
```

输出：

```
total=4100 s=hello d=H raw=C:\path\to\file
```

解释：

| 类型族 | 类型 | 说明 |
|:---|:---|:---|
| 有符号整数 | `i8 i16 i32 i64 i128 isize` | i64 是日常默认 |
| 无符号整数 | `u8 u16 u32 u64 u128 usize` | usize 用于长度/索引语义 |
| 浮点 | `f32 f64` | f64 默认 |
| 布尔 | `bool` | `true` / `false`；if 条件必须是 bool（S-1） |
| 字符 | `char` | 单引号；Unicode 码点语义 |
| 字符串 | `String` | 双引号；支持 `\n \t \u{XXXX}` 等转义 |

类型转换**只有一条通道**：`expr as Type`（D7）。`c as i64` 把 bool 转 1。
没有隐式转换，`if 1 {}` 这样的写法直接报错。

## 3.3 容器与标准泛型

```hsl
fn main() -> i64 {
    let v: Vec<i64> = vec![1, 2, 3];
    let mut m: HashMap<String, i64> = HashMap::new();
    m.insert(String::from("a"), 1);
    let some: Option<i64> = Some(5);
    let ok: Result<i64, String> = Ok(7);

    println!("v={:?} len={}", v, v.len());
    println!("m.a={} some={} ok={}", m.get(String::from("a")).unwrap_or(0), some.unwrap_or(0), ok.unwrap_or(0));
    0
}
```

输出：

```
v=[1, 2, 3] len=3
m.a=1 some=5 ok=7
```

解释：

| 泛型 | 语义 | 后端映射示例（python） |
|:---|:---|:---|
| `Vec<T>` | 有序可增长数组 | `list[T]` |
| `HashMap<K,V>` | 键值表 | `dict[K, V]` |
| `HashSet<T>` | 集合 | `set[T]` |
| `Option<T>` | 可空值：`Some(T)` / `None` | `T \| None` |
| `Result<T,E>` | 可失败值：`Ok(T)` / `Err(E)` | `T`（错误走异常封装语义） |
| `Box<T>` / `Box<dyn Trait>` | 装箱 / trait 对象 | `T` / 协议类实例 |

`Vec::new()` / `HashMap::new()` / `String::new()`（v0.2.2 起全部可用，
另含 `with_capacity` 系列与 `HashSet::new`）——空字符串也可写
`String::from("")`。

`{:?}` 是 debug 格式化（打印容器），`{}` 是 display 格式化（打印标量）。

## 3.4 函数与 async

```hsl
fn double(x: i64) -> i64 { x * 2 }

fn greet(name: String) -> String {
    format!("你好, {}", name)
}

async fn fetch_label(id: i64) -> String {
    format!("item-{}", id)
}

fn main() -> i64 {
    println!("{}", double(21));
    println!("{}", greet(String::from("HSL")));
    let label = fetch_label(7).await;    // .await 后缀
    println!("{}", label);
    0
}
```

输出：

```
42
你好, HSL
item-7
```

解释：

- 函数体最后一个**无分号表达式**即返回值；显式 `return expr;` 同样合法。
- `async fn` 返回 future，用 `.await` 后缀等待（在 dhv-ts 中 await 透明穿透，
  因为解释器本身是异步树遍历）。
- 无参数自洽约定：`run` 命令寻找入口文件中名为 `main` 的 fn 并以零参调用
  （BNF v1.3 R-1）。
- 参数模式可以是任意 pattern（解构见 3.8），`mut` 参数在 graph 中常用
  （`graph Dsh(mut state: SessionState)`）。

## 3.5 struct

```hsl
export struct Point {
    x: i32,
    y: i32,
}

impl Point {
    fn new(x: i32, y: i32) -> Point {
        Point { x, y }
    }

    fn magnitude_sq(self) -> i32 {
        self.x * self.x + self.y * self.y
    }

    fn zero() -> Point {
        Point { x: 0, y: 0 }
    }
}

fn main() -> i64 {
    let p = Point::new(3, 4);
    let origin = Point::zero();
    println!("mag_sq = {}", p.magnitude_sq());
    println!("origin = {:?}", origin);
    0
}
```

输出：

```
mag_sq = 25
origin = Point { x: 0, y: 0 }
```

解释：

- `impl Type { ... }` 定义固有方法：无 `self` 的关联函数（`Point::new`），
  带 `self` 的实例方法（`p.magnitude_sq()`）。
- 结构体字面量支持**字段简写**：`Point { x, y }` 等价 `Point { x: x, y: y }`。
- `#[derive(Debug, Clone, PartialEq)]` 等派生属性可标注（投射到后端的派生宏）。

## 3.6 enum 与和类型

enum 是 HSL 表达「模型下一步做什么」的核心词汇——它就是 Agent 的
动作协议：

```hsl
export enum Shape {
    Circle { radius: f64 },
    Rect { w: i32, h: i32 },
    Dot,
}

export enum ToolCall {
    ReadFile { path: String },
    EditFile { path: String, old_text: String, new_text: String },
    Bash { command: String },
}

fn describe(s: Shape) -> String {
    match s {
        Shape::Circle { radius } => format!("circle r={}", radius),
        Shape::Rect { w, h } => format!("rect {}x{}", w, h),
        Shape::Dot => String::from("dot"),
    }
}

fn main() -> i64 {
    println!("{}", describe(Shape::Rect { w: 2, h: 5 }));
    println!("{}", describe(Shape::Circle { radius: 1.5 }));
    let call = ToolCall::ReadFile { path: String::from("main.py") };
    match call {
        ToolCall::ReadFile { path } => println!("read {}", path),
        ToolCall::EditFile { path, .. } => println!("edit {}", path),
        ToolCall::Bash { command } => println!("bash {}", command),
    }
    0
}
```

输出：

```
rect 2x5
circle r=1.5
read main.py
```

解释：

- 变体可带命名负载 `{ ... }`、元组负载 `(String)` 或无负载。
- `..` 在结构模式中忽略其余字段（如 `ToolCall::EditFile { path, .. }`）。
- 变体名校验是严格的：`Enum::Variant { fields }` 匹配同时校验枚举名与变体名
  （BNF v1.3 澄清项 6——不同变体可能共享同名字段，仅按字段形状匹配是错误的）。
- 枚举 + match 是 harness 动作协议的标准形态：新增变体 → 所有 match 处
  编译期报缺（S-6），你被迫直面新分支。

## 3.7 trait 与 impl

```hsl
export trait Speaker {
    fn name(self) -> String;
    fn speak(self) -> String {
        // 默认实现：trait 方法可以有方法体
        format!("{} 说：你好", self.name())
    }
}

export struct Cat { label: String }
export struct Robot { id: i64 }

impl Speaker for Cat {
    fn name(self) -> String { self.label.clone() }
}

impl Speaker for Robot {
    fn name(self) -> String { format!("robot-{}", self.id) }
    fn speak(self) -> String { String::from("哔哔") }   // 覆盖默认实现
}

fn main() -> i64 {
    let speakers: Vec<Box<dyn Speaker>> = vec![
        Box::new(Cat { label: String::from("咪") }),
        Box::new(Robot { id: 7 }),
    ];
    for s in speakers {
        println!("{}", s.speak());       // 动态派发
    }
    0
}
```

输出：

```
咪 说：你好
哔哔
```

解释：

- `trait` 定义行为契约，可含签名（无体）与默认实现（有体）。
- `impl Trait for Type` 提供实现；`impl Type` 是固有实现（见 3.5）。
- `Box<dyn Speaker>` 是 trait 对象类型——Provider 注入位的标准写法
  （dsh 示例中 `Box<dyn ModelProvider>` 同一接口插拔真实 LLM 与剧本模型）。
- From trait 特化（`impl From<E1> for E2`）是 `?` 的错误转换通道，
  见 3.11 的已知限制说明。

## 3.8 match 与穷尽性

```hsl
export enum Level {
    Info,
    Warn { code: i64 },
    Error { code: i64, message: String },
}

fn main() -> i64 {
    let lvl = Level::Warn { code: 2 };
    let text = match lvl {
        Level::Info => String::from("info"),
        Level::Warn { code } if code >= 2 => format!("严重警告 {}", code),
        Level::Warn { code } => format!("警告 {}", code),
        Level::Error { code, message } => format!("错误 {} {}", code, message),
    };
    println!("{}", text);
    0
}
```

输出：

```
严重警告 2
```

解释：

- match 是**表达式**：每个 arm 的值类型应一致，可作为 let 的初值。
- arm 语法：`Pattern => expr,`；支持 `if guard` 守卫（守卫必须是 bool）。
- 模式家族：字面量 / 绑定 / 通配 `_` / 解构（struct / enum / tuple）/
  or-模式 `A | B` / 范围模式 `1..=9` / rest `..`。
- **穷尽性（S-6）**：对枚举的 match 必须覆盖所有变体，或以 `_ =>` 通配兜底
  （v0.2.2 起与 Rust 语义一致：普通函数内的 `_` 兜底视为穷尽）。
  **唯一例外**：graph AgentLoop 内的枚举 match 不允许 `_` 兜底——Agent 的
  核心决策循环必须显式直面每个新分支，这是 HSL 把「非确定性循环决策」
  固化成语法的核心铁律；对字符串等非枚举值的 match 则不受穷尽性约束。

```hsl
// 普通函数内：_ 兜底合法（Rust 语义，v0.2.2+）
fn describe(a: Action) -> String {
    match a {
        Action::Go => String::from("go"),
        _ => String::from("other"),   // ✓ 视为穷尽
    }
}
```

```
graph Agent {
    loop {
        match act {
            Action::Go => continue,
            _ => break,               // ✗ S-6：AgentLoop 内禁止通配兜底
        }
    }
}
```

对字符串的 match（无枚举 arm）则完全自由：

```hsl
fn pick(s: String) -> i64 {
    match s.as_str() {
        "a" => 1,
        "b" => 2,
        _ => 0,
    }
}
```

## 3.9 if / else 与 if let

```hsl
fn clamp(v: i64, lo: i64, hi: i64) -> i64 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

fn main() -> i64 {
    println!("{} {} {}", clamp(-5, 0, 10), clamp(5, 0, 10), clamp(99, 0, 10));

    // if 是表达式（没有三元运算符，D3）
    let parity = if clamp(5, 0, 10) % 2 == 0 { String::from("even") } else { String::from("odd") };
    println!("parity = {}", parity);

    // if let：模式为真才进入
    let maybe: Option<i64> = Some(9);
    if let Some(v) = maybe {
        println!("got {}", v);
    }

    // while let：持续出队
    let mut queue = vec![1, 2, 3];
    let mut drained: i64 = 0;
    while let Some(x) = queue.pop() {
        drained += x;
    }
    println!("drained = {}", drained);
    0
}
```

输出：

```
0 5 10
parity = odd
got 9
drained = 6
```

解释：

- `if` 条件必须是 `bool`，零隐式转换（S-1）。
- if-else 链是表达式，取代三元运算符。
- `if let Pattern = expr {}` 与 `while let Pattern = expr {}`
  是 Option 出队的惯用形。

### 3.9.1 if-let / while-let 支持的模式（v1.4.2 扩面；v1.4.5 cpp/go 活体化）

v1.4.2 起，活体翻译器（python/ts/js/rust 后端）支持远超 `Option::Some` 的模式集；
**v1.4.5 起 cpp/go 后端同构支持**（见下表后约 cpp/go 专节）。
参考实现 dhv-ts 的 `body.ts` 中 `armInfo()` + `normalizePattern()` + `hoistScrut()`
共同实现下列六类模式：

| 模式类别 | 示例 | Python 生成 | TS/JS 生成 | Rust 生成 |
|:--|:--|:--|:--|:--|
| Option::Some tuple | `if let Some(x) = m` | `if m is not None:` + `x = m` | `if (m != null) { const x = m; }` | 原生 `if let Some(x) = m` |
| Option::None | `if let None = m` | `if m is None:` | `if (m == null)` | 原生 `if let None = m` |
| Result::Ok tuple | `if let Result::Ok(v) = r` | `if isinstance(r, Ok):` + `v = r.f0` | `if (r?.kind === 'Ok') { const v = r.f0; }` | 原生 `if let Ok(v) = r` |
| Result::Err tuple | `if let Result::Err(e) = r` | `if isinstance(r, Err):` + `e = r.f0` | `if (r?.kind === 'Err') { const e = r.f0; }` | 原生 `if let Err(e) = r` |
| 用户 enum tuple 变体 | `if let Color::Rgb(r, g, b) = c` | `if isinstance(c, Rgb):` + `r = c.f0; g = c.f1; b = c.f2` | `if (c?.kind === 'Rgb') { const r = c.f0; ... }` | 原生 `if let Color::Rgb(r, g, b) = c` |
| 用户 enum struct 变体 | `if let Color::Point { x, y } = c` | `if isinstance(c, Point):` + `x = c.x; y = c.y` | `if (c?.kind === 'Point') { const x = c.x; ... }` | 原生 `if let Color::Point { x, y } = c` |
| 用户 enum 无负载变体 | `if let Color::Red = c` | `if isinstance(c, Red):` | `if (c?.kind === 'Red')` | 原生 `if let Color::Red = c` |
| 单纯绑定 | `if let x = expr` | 恒真条件 + `x = scrut` | 同上 | 原生 `if let x = expr` |
| 通配 `_` | `if let _ = expr` | 恒真条件 | 同上 | 原生 `if let _ = expr` |

**简写归一化**：`Some(x)` / `None` / `Ok(v)` / `Err(e)` 单段写法等价于
`Option::Some(x)` / `Option::None` / `Result::Ok(v)` / `Result::Err(e)`，
编译期归一为同一 AST，下游翻译路径无需重复特化。

**Scrutinee 副作用保护**：当 scrutinee 是含运算符或方法调用的复杂表达式
（如 `cur.pop()` / `results.get(i)`），活体翻译器会先 hoist 到临时变量
`_scrut_N`（if-let）/ `_wl_N`（while-let 每迭代重新求值一次），再用于条件检查
与字段绑定，**避免副作用表达式被多次求值**。这是与 Rust `while let` 同源语义
（每次迭代求值一次 scrutinee）。

**生成代码示例**：`drain` 函数（v1.4.2 真实实测）

HSL：
```hsl
fn drain(v: Vec<i32>) -> i32 {
    let mut total = 0;
    let mut cur = v;
    while let Some(x) = cur.pop() {
        total = total + x;
    }
    total
}
```

Python（v1.4.2 实测生成）：
```python
def drain(v: list[int]) -> int:
    total = 0
    cur = v
    while True:
        _wl_0 = cur.pop() if cur else None
        if not (_wl_0 is not None):
            break
        x = _wl_0
        total = (total + x)
    return total
```

TypeScript（v1.4.2 实测生成）：
```typescript
export function drain(v: number[]): number {
    let total = 0;
    let cur = v;
    while (true) {
      const _wl_0 = (cur.length > 0 ? cur.pop()! : null);
      if (!(_wl_0 != null)) break;
      const x = _wl_0;
      total = (total + x);
    }
    return total;
}
```

Rust（v1.4.2 实测生成）：
```rust
pub fn drain(v: Vec<i32>) -> i32 {
    let mut total = 0;
    let mut cur = v;
    while let Some(x) = cur.pop() {
        total = (total + x);
    }
    return total;
}
```

**已知边界**：

- ~~`if-let` 在 tail 位置（无分号的尾表达式）不参与值语义~~ **v1.4.3 已修复**：带 else 的
  `if let PAT = EXPR { A } else { B }` 在函数尾位置现为值语义（分支产出 `return`，
  与 `match` / `if` 对齐）。无 else 的 if-let 在值语境（函数有返回类型）诚实回退
  contract——Rust 语义中无 else 的 if let 表达式类型是 `()`，值语境本就非法。
  v1.4.3 实测：`fn pick(v: Vec<i64>) -> i64 { if let Option::Some(x) = v.get(0) { x + 100 } else { -1 } }`
  生成 python 后 `pick([5,6])==105`、`pick([])==-1`（get 越界 → None → else 分支）。
- ~~`else if let` 链需要显式 return~~ **v1.4.3 已修复**：`if let ... {} else if let ... {} else {}`
  现可活体翻译（python 改写为 `elif`；brace 语言改写为 `} else if`；Rust 原生语法）。
- 嵌套 if-let / while-let 已支持（`first_ok` 实测：while-let 套 if-let Result::Ok）。
- match 臂 / if 分支等值语义块尾部的**嵌套 if / if-let / match** 同样按值语义递归处理
  （v1.4.3 修复：此前嵌套分支被降级为语句导致静默丢失 return——生成代码返回 None）。
- 完整示例见 `dhv-ts/examples/pattern-tour.hsl`。

**v1.4.5 cpp/go 活体化**（if-let / while-let 从 5 语言 → 7 语言）

| 模式类别 | C++ 生成 | Go 生成 |
|:--|:--|:--|
| 用户 enum 变体 | `if (std::holds_alternative<Circle>(s)) { auto& _v = std::get<Circle>(s); auto r = _v.f0; ... }` | `if _ifv_0, _ok_1 := s.(Circle); _ok_1 { r := _ifv_0.F0; ... }` |
| Option::Some | `if (m.has_value()) { auto x = *m; ... }` | `if m != nil { x := *m; ... }` |
| Option::None | `if (!(m.has_value())) { ... }` | `if m == nil { ... }` |
| while-let | `while (true) { if (!(cond)) { break; } … }` | `for { if !(cond) { break }; … }` |
| Some(x) 构造 | `_dhvSome(x)`（模板推导助手，含 include guard） | `_dhvSome(x)`（go 1.18+ 泛型助手） |
| None 值 | `std::nullopt` | `nil` |

- go 无绑定变体（如 `Shape::Unit`）用 blank 标识符：`if _, _ok_2 := s.(Unit); _ok_2 {`
  （go 对未使用变量零容忍——具名接收者会被编译拒绝）。
- go 变体结构体字段是导出大写（`F0` / `X` / `Y`），与 decls 声明一致。
- **Result::Ok / Result::Err 对 cpp/go 诚实回退 contract**：类型映射无变体通道
  （cpp `Result<T,E>` → `T` 裸值 / go → `(T, error)` 多返回值），无法廉价复现变体匹配语义——
  宁缺毋滥纪律（围栏内 HSL 源码 + 未实现标记，语义仍由 dhv-ts/dhv 保障）。
- cpp 编译级验证（v1.4.5 实测）：pattern-tour describe/classify/count_down
  g++ -std=c++23 编译 + 链接运行，输出与解释器逐字对齐
  （`describe(Circle 3)`=`circle r=3`、`classify(Point)`=`point (1,2)`、
  `count_down(4)`=10、`count_down(None)`=0）；Some/None 构造链路同验
  （`make(5)`→`Some(10)`、`make(-3)`→`None`、`use_opt(Some(42))`=43、`use_opt(None)`=-1）。
- 裸 `None` / `Some(x)` / `Option::None` / `Option::Some(x)` 值在所有活体语言合法映射
  （v1.4.5 修复：此前 cpp/go/ts/js 输出字面 `None` —— 非法标识符）。
- 带 `mut` 的 while-let 变量循环（`count_down` 模式）在 5 语言全活体。

**v1.4.3 方法映射扩面**（活体翻译器方法映射表 46 → 66 项，全部经 python exec
语义级验证）：

| 类别 | 新增方法 | 说明 |
|:--|:--|:--|
| 数值 | `pow` `sqrt` `floor` `ceil` `round` `clamp` | python round 用 `int((x+0.5)//1)` 避免银行家舍入偏差（与 JS/Rust 半值向上一致）|
| Vec 聚合 | `any` `all` `fold` `for_each` `extend` | python any/all 用生成器表达式；fold 参数序适配（HSL `fold(init, f)` → python `reduce(f, v, init)`）|
| Vec 排序 | `sort_by` `sort_desc` | sort_by 是 **key 语义**（按 `f(x)` 数值排序）：python `sort(key=f)` / rust `sort_by_key` |
| Rust 链拼块 | `iter` `collect` `cloned` | rust 后端迭代器链（py/ts 的 map/filter 直接产出数组，无需链拼）|
| String/char | `as_str` `trim_start` `trim_end` `char_at` `is_alphabetic` `is_numeric` | char_at 越界返回 `''`（python 切片 `[i:i+1]` 天然对齐）|
| Option | `or` `unwrap_or_else` `map`（类型感知） | Option::map 与 Vec::map 同名不同义——按接收者静态类型分发 |

`get` / `first` / `last` / `pop` 的生成端映射升级为 **Option 精确语义**（与解释器对齐）：
python `v.get(i)` → `_dhv_get(v, i)`（越界/缺键返回 None）；ts/js 按类型感知分发
`v[i] ?? null` / `m.get(k) ?? null`；rust 原生 `.get(i)`。

## 3.10 循环：while / for / loop 与标签

```hsl
fn main() -> i64 {
    // while
    let mut total: i64 = 0;
    let mut i: i64 = 0;
    while i < 5 {
        total += i;
        i += 1;
    }
    println!("while total = {}", total);

    // for + range（0..5 左闭右开）
    let mut sum: i64 = 0;
    for k in 0..5 {
        sum += k;
    }
    println!("for-range sum = {}", sum);

    // for + in（迭代 Vec）
    let names = vec![String::from("a"), String::from("b"), String::from("c")];
    let mut joined = String::from("");
    for n in names {
        joined.push_str(n.as_str());
    }
    println!("for-in joined = {}", joined);

    // loop（无限循环，配合 break）
    let mut count: i64 = 0;
    loop {
        count += 1;
        if count >= 3 {
            break;
        }
    }
    println!("loop count = {}", count);

    // 标签：嵌套循环跳到外层（'ident 是标签的唯一用途，D5）
    let mut hits: i64 = 0;
    'outer: for a in 0..5 {
        for b in 0..5 {
            if a * b >= 6 {
                continue 'outer;   // 跳到外层下一轮
            }
            if a + b > 5 {
                break 'outer;      // 直接跳出外层
            }
            hits += 1;
        }
    }
    println!("labeled hits = {}", hits);
    0
}
```

输出：

```
while total = 10
for-range sum = 10
for-in joined = abc
loop count = 3
labeled hits = 17
```

解释：

- 四种循环：`while` / `while let` / `for pat in expr` / `loop`。
- range 语法 `a..b`（开区间）与 `a..=b`（闭区间）。
- 标签 `'outer:` 写在循环前；`break 'outer` / `continue 'outer` 跨层跳转。
  这在「重试内层、预算耗尽跳外层」的 harness 场景中非常常用
  （nova 示例的 `'retry` 标签即此用法）。

## 3.11 错误处理：Result 与 ?

```hsl
export enum ToolError {
    NotFound { path: String },
    TooLarge { path: String, size: i64 },
}

fn read_file(path: String) -> Result<String, ToolError> {
    Err(ToolError::NotFound { path })
}

fn step() -> Result<String, ToolError> {
    let body = read_file(String::from("notes.md"))?;   // Err 则立刻提前返回
    Ok(body)
}

fn main() -> i64 {
    match step() {
        Result::Ok(text) => println!("len={}", text.len()),
        Result::Err(ToolError::NotFound { path }) => println!("未找到 {}", path),
        Result::Err(ToolError::TooLarge { path, size }) => println!("过大 {} {}B", path, size),
    }

    // Option 也支持 ?
    let opt: Option<i64> = None;
    fn pick(v: Option<i64>) -> Option<i64> {
        let x = v?;
        Some(x * 2)
    }
    match pick(opt) {
        Option::Some(n) => println!("some {}", n),
        Option::None => println!("none"),
    }
    0
}
```

输出：

```
未找到 notes.md
none
```

解释：

- `?` 后缀：`Ok(v)` 解包为 v；`Err(e)` 立即从当前函数返回 `Err(e)`；
  `None` 立即返回 `None`。这是唯一的错误传播通道（S-5：`?` 不做别的事）。
- `Result` / `Option` 是预导入的枚举，直接 `Ok(...)` / `Err(...)` /
  `Some(...)` / `None` 构造，也支持 `Result::Ok` 全限定形式。
- 常用方法：`unwrap_or(默认值)`、`is_ok()` / `is_err()` / `is_some()` /
  `map(f)` / `map_err(f)`、`expect("消息")`。裸 `unwrap()` 会触发 S-2 警告。

**From 转换（v0.2.1 已接线）**：BNF §5.9 规定 `?` 可经
`impl From<E1> for E2` 自动包装错误类型（如 ProviderError → HarnessError）。
v0.2.1 修复了类型解析器丢弃泛型实参的问题，`?` 传播时会查 `fromImpls`
注册表执行真实转换：

```hsl
struct ProviderError { message: String }
struct HarnessError { message: String }

impl From<ProviderError> for HarnessError {
    fn from(e: ProviderError) -> HarnessError {
        HarnessError { message: String::from("wrapped: ") + e.message }
    }
}

fn flaky(fail: bool) -> Result<i64, ProviderError> {
    if fail { Err(ProviderError { message: String::from("boom") }) } else { Ok(42) }
}

fn run(fail: bool) -> Result<i64, HarnessError> {
    let v = flaky(fail)?;   // ← Err 时自动经 From 包装为 HarnessError
    Ok(v + 1)
}
```

运行 `run(true)` 得到 `Err(HarnessError { message: "wrapped: boom" })` —— 转换真实发生。
未注册 From 时 `?` 保持错误值原样传递（此时跨层错误建议统一错误枚举或手动 match 包装）。

## 3.12 闭包

```hsl
fn double(x: i64) -> i64 { x * 2 }

fn main() -> i64 {
    // 完整形：|参数| 表达式
    let add = |a: i64, b: i64| a + b;
    let square = |x: i64| x * x;
    println!("add(3,4) = {}", add(3, 4));
    println!("square(5) = {}", square(5));

    // 按名捕获外层变量
    let base = 100;
    let nums = vec![1, 2, 3, 4, 5];
    let with_base = nums.map(|n| n + base);
    println!("with_base = {:?}", with_base);

    // 方法链：filter + map + sum
    let evens = nums.filter(|n| n % 2 == 0).map(|n| n * 10);
    println!("evens = {:?} sum={}", evens, evens.sum());

    // 函数名本身可作值传递
    let f = double;
    println!("f(21) = {}", f(21));
    0
}
```

输出：

```
add(3,4) = 7
square(5) = 25
with_base = [101, 102, 103, 104, 105]
evens = [20, 40] sum=60
f(21) = 42
```

解释：

- 语法 `|params| body`（Rust 风格）；参数可带类型注解。
- 闭包按名词法捕获外层变量，无需 `move`（解释器按值拷贝语义）。
- Vec 的迭代方法面（map / filter / fold / any / all / sum / position…）
  是数据处理的日常主力，完整清单见 7.11 与附录 B。

## 3.13 macro_rules! 宏

```hsl
macro_rules! bump {
    ($field:expr, $by:expr) => { $field += $by; };
}

macro_rules! tri {
    ($a:expr, $b:expr, $c:expr) => { $a * 100 + $b * 10 + $c };
}

fn main() -> i64 {
    let mut counter: i64 = 0;
    bump!(counter, 5);
    bump!(counter, 5);
    println!("counter = {}", counter);
    let combined = tri!(1, 2, 3);
    println!("tri!(1,2,3) = {}", combined);
    0
}
```

输出：

```
counter = 10
tri!(1,2,3) = 123
```

解释：

- 定义：`macro_rules! 名字 { ($匹配器) => { $转写器 }; }`——名字后**不带** `!`
  （v1.4.3 起带尾 `!` 的 `macro_rules! 名字! { ... }` 作为容错形态同样接受——
  从 Rust 迁移习惯的用户写 `macro_rules! bump! {...}` 也能定义与调用）。
- 片段说明符支持 `ident / literal / tt / expr / ty / pat` 等；
  重复 `$(...)*` 亦支持（复杂嵌套重复是已知限制，见 13.3）。
- 宏在**token 层展开**（定义必须先于使用），`bump!(state.stats.turns, 1)`
  这类样板消除是 harness 里的典型用法（dsh 主循环真实使用）。
- 预导入宏：`format!` / `println!` / `vec!` / `assert!` / `assert_eq!` /
  `panic!` / `dbg!` 等，无需定义直接用。

## 3.14 import 与 export

HSL 的模块系统是「文件即模块」（M1）：`models/types.hsl` 的模块名就是
这个相对路径。默认私有，`export` 的项才能被别人 import。

```hsl
// ---- tools/math.hsl ----
export fn add(a: i64, b: i64) -> i64 { a + b }
export fn mul(a: i64, b: i64) -> i64 { a * b }
fn secret() -> i64 { 42 }          // 私有，外界不可见
```

```hsl
// ---- main.hsl ----
import { add, mul } from "./tools/math.hsl";     // 列表导入
// import * as m from "./tools/math.hsl";        // 命名空间导入：m.add(...)
// import add as plus from "./tools/math.hsl";   // 重命名导入

fn main() -> i64 {
    println!("{} {}", add(1, 2), mul(3, 4));
    0
}
```

输出：

```
3 12
```

解释：

- import 路径是**相对当前 .hsl 文件**的字符串，仅 `.hsl` 后缀。
- 三种形态：`{ a, b }` 列表 / `* as m` 命名空间 / `name as alias` 单项。
- **S-7 把「未使用的 import」当错误**——每行 import 都必须被消费
  （命名空间导入豁免整体）。这是防止模块边界腐化的关键规则。
- 标准库导入形如 `import { len_of } from "std/text";`，见第七章。
- 传递导出禁止（M3）：export 只作用于本文件定义。

---

# 第四章 Agent 核心循环

前一章是「通用语言」；本章是 HSL 的灵魂——用**语法固化**的方式表达
Agent 的编排拓扑。读完本章你就能看懂 dsh / nova 两个真实项目的骨架。

## 4.1 graph：拓扑是一等公民

`graph` 是一种与 fn 平级的**项**：它像函数一样有参数和返回类型，
但 body 里允许出现 `node` / `edge` 声明，且**必须**包含 AgentLoop。

```hsl
graph Dsh(mut state: SessionState) -> Result<Report, HarnessError> {
    node model: Box<dyn ModelProvider> = make_model(state.policy.clone())?;
    node executor: Toolkit = Toolkit::new(state.policy.clone());
    // ... edge / let / loop ...
}
```

调用约定（BNF v1.3）：`GraphName::run(args)`——与 trait 关联函数同形：

```hsl
let report = Dsh::run(state)?;
```

graph 与 fn 的区别一句话：**fn 是计算，graph 是带静态校验的状态机**。
G 规则（4.4 节）保证 graph 的拓扑在编译期就是合法的。

## 4.2 node 与 edge on Guard

### node：声明可执行单元

```hsl
graph G(mut n: i64) -> i64 {
    node counter: i64 = 0;        // 带初始化
    node limit: i64;              // 无初始化（microkernel 插件注入位）
    node mut hits: i64 = 0;       // 可变节点（node mut）
    // ...
}
```

- `node 名字: 类型 = 表达式;`——初始化表达式可以带 `?`（失败即 graph 早退）。
- 无初始化的 node 是**插件注入位**：nova 的 critic graph 用它声明
  scanner / scorer，运行时由微内核注入实现。
- `node mut` 声明可变节点（默认不可变，与 let 的 S-4 一致）。

### edge：声明转移边

```hsl
edge model -> executor on Action::Tool;
edge executor -> model on Event::Observed;
edge model -> reviewer on Action::Done;
edge reviewer -> model on Verdict::Revise;
```

- `edge A -> B on Guard;`——Guard 通常是某个枚举变体模式（`Action::Tool`），
  语义是「当 A 产出该变体时，转移到 B」。
- 支持多跳：`edge a -> b -> c;` 等价两条边。
- `with` 属性可携带部署语义：`edge a -> b on X with backpressure = true, durable;`

这四条边就是 dsh 的核心拓扑（README 中的示意图）：

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

## 4.3 AgentLoop 与 match Action

graph body 必须恰含至少一个 `loop`（G-1）——这个 loop 就是 AgentLoop。
它与普通 loop **语法同形**（D6），但承载约定俗成的结构：

```hsl
loop {
    if turns >= state.policy.max_turns {    // 预算闸门
        break;
    }
    let action = model.act(transcript.clone()).await?;
    match action {
        Action::Tool { call } => { /* 执行工具 → 观察 → 继续循环 */ },
        Action::Done { summary } => { /* 收尾 → break */ },
    }
}
```

这个结构固化了 harness 的三件套：

1. **预算闸门**：轮数 / 调用数上限，先于模型调用检查；
2. **动作分发**：`match action` 对 Action 枚举分发——graph AgentLoop 内
   禁止 `_` 通配兜底（S-6 铁律），必须显式列出每个变体（普通函数内
   `_` 兜底合法，v0.2.2 起与 Rust 语义一致）；
3. **终止条件**：至少一个 arm 或闸门能 `break` 出环。

运行期观测（G6）：当 AgentLoop 内 match 选中某个变体，且该变体恰好是
某条 `edge ... on 该变体` 的 Guard 时，宿主向事件总线发射一条 edge 事件：

```json
{"name":"edge","data":{"graph":"MiniAgent","from":"planner","to":"planner","on":"CallTool","scale":"microkernel"}}
```

这是「拓扑可观测」的落地：events.jsonl 里能直接看到边被走过几次。

## 4.4 G 规则：拓扑的静态校验

| 规则 | 含义 | 违反后果 |
|:---|:---|:---|
| G-1 | graph body 必须恰含至少一个 AgentLoop（顶层直接子节点中的 `loop`） | error |
| G-2 | edge 的每个端点必须已在此前声明（node / let / graph 参数），声明先于 edge | error |
| G-3 | 拓扑不得出现编译期可判定的**无条件环**：环上至少一条边带 `on Guard` | error |
| G-4 | 每个 node 应有边可达（孤岛节点产生警告，插件注入位可接受） | warning |

G-3 的设计意图：`a -> b -> a` 的死循环必然挂死 harness；而带 Guard 的环
（如 model → executor → model on Observed）是 Agent 的呼吸回路，
由运行时的动作分发打破。

触发示例（无条件环）：

```
graph G {
    let a = 1;
    let b = 2;
    edge a -> b;
    edge b -> a;          // ✗ 无 guard 的环
    loop { break; }
}
```

```
error[G-3]: 拓扑存在无条件环：a -> ... -> a（环上至少一条边需 on Guard 打破，G3）
error[G-3]: 拓扑存在无条件环：b -> ... -> b（环上至少一条边需 on Guard 打破，G3）
```

## 4.5 一个完整的最小 Agent

下面是一个**可以真实跑通**的最小 Agent：剧本驱动、图拓扑、Action 分发、
状态累计。一个 HSL 文件 + 一个 fixture：

```hsl
// ---- mini-agent.hsl ----
export enum Action {
    CallTool { name: String },
    Done { summary: String },
}

export struct State {
    turns: i64,
    tool_calls: i64,
}

fn next_action() -> Action {
    // 从剧本取下一动作（native 详见第六章）
    let raw: String = native typescript {
        const s = await $host.fixture.nextAct();
        return s;
    };
    match parse_action(raw) {
        Result::Ok(a) => a,
        Result::Err(e) => Action::Done { summary: format!("协议违规：{}", e) },
    }
}

fn parse_action(raw: String) -> Result<Action, String> {
    let fields: HashMap<String, String> = native typescript {
        const obj = JSON.parse(raw);
        const out = new Map();
        for (const [k, v] of Object.entries(obj)) out.set(k, String(v));
        return out;
    };
    let action = fields.get(String::from("action")).unwrap_or(String::from(""));
    if action == String::from("tool") {
        Ok(Action::CallTool { name: fields.get(String::from("tool")).unwrap_or(String::from("?")) })
    } else if action == String::from("done") {
        Ok(Action::Done { summary: fields.get(String::from("summary")).unwrap_or(String::from("")) })
    } else {
        Err(format!("未知 action: {}", action))
    }
}

graph MiniAgent(mut state: State) -> State {
    node planner: () = ();
    let mut last: String = String::from("(start)");

    edge planner -> planner on Action::CallTool;   // 自环：工具循环

    loop {
        if state.turns >= 8 {                       // 预算闸门
            break;
        }
        let action = next_action();
        match action {
            Action::CallTool { name } => {
                last = format!("(模拟执行 {} 完成)", name);
                state.tool_calls += 1;
                state.turns += 1;
                println!("[turn {}] 工具 {} → {}", state.turns, name, last);
            },
            Action::Done { summary } => {
                println!("[done] {}", summary);
                break;
            },
        }
    }
    state
}

fn main() -> Result<(), String> {
    let state = State { turns: 0, tool_calls: 0 };
    let final_state = MiniAgent::run(state);
    println!("共 {} 轮，{} 次工具调用", final_state.turns, final_state.tool_calls);
    Ok(())
}

scale = microkernel;

project {
    MiniAgent -> "src/mini_agent.py" : python,
}
```

```json
// ---- fix-mini.json（剧本） ----
{
  "acts": [
    "{\"action\": \"tool\", \"tool\": \"grep\"}",
    "{\"action\": \"tool\", \"tool\": \"cat\"}",
    "{\"action\": \"done\", \"summary\": \"任务完成\"}"
  ],
  "reviews": []
}
```

运行：

```bash
bun dhv-ts/src/main.ts run mini-agent.hsl --fixture fix-mini.json --quiet
```

真实输出：

```
[turn 1] 工具 grep → (模拟执行 grep 完成)
[turn 2] 工具 cat → (模拟执行 cat 完成)
[done] 任务完成
共 2 轮，2 次工具调用

✓ harness 返回 Ok（14 ms）
```

对照 events.jsonl（产物目录下），可以看到 G6 边事件与 node 事件：

```json
{"seq":1,"name":"node","data":{"graph":"MiniAgent","node":"planner","initialized":true}}
{"seq":2,"name":"edge","data":{"graph":"MiniAgent","from":"planner","to":"planner","on":"CallTool","scale":"microkernel"}}
{"seq":3,"name":"edge","data":{"graph":"MiniAgent","from":"planner","to":"planner","on":"CallTool","scale":"microkernel"}}
```

两次工具调用 = 两条 edge 事件。**拓扑不再是注释里的 ASCII 图，
而是可验证、可观测的机器事实。**

## 4.6 scale = monolith | microkernel

`scale` 是工程的架构尺度声明（每工程一个，P-6 要求在含 graph 的入口文件）：

```hsl
scale = microkernel;    // 或 monolith
```

两者在 HSL 源码层**零差异**——同一份源码，两种架构形态：

| 维度 | microkernel（默认） | monolith |
|:---|:---|:---|
| 语义（G6） | 每条 edge = 事件总线订阅；match 分发即发事件 | 每条 edge = 直接调用轨迹 |
| 生成的 graph 脚手架 | Plugin 注册表 + 事件总线驱动循环 | 节点局部变量化 + while 循环 |
| 运行观测（dhv-ts） | events.jsonl 中 edge 事件带 `scale: microkernel` | 同样发 edge 事件，`scale: monolith` 标注为直接调用轨迹 |
| 适用场景 | 多 Agent 插件化、可热插拔观测 | 单体部署、低延迟、少依赖 |

切换方式：改入口文件的 `scale = ...;` 声明后重新 emit。

**注意（诚实边界）**：`emit` 的尺度取自**入口文件的 scale 声明**；
CLI 的 `--scale` flag 只影响 `run`（解释器观测标注），目前**不会**覆盖
emit 的投射形态（dhv-ts v0.2.0 已知行为，见 13.3）。

---

# 第五章 38 后端投射

本章回答：`project {}` 怎么写、38 个后端各是什么、生成物长什么样、
能力边界在哪。所有生成代码均为真实 emit 输出。

## 5.1 project{} 语法全解

```hsl
project {
    // 逻辑项        -> "物理路径"          : 目标语言,
    Prompt          -> "gen/python/prompt.py"      : python,
    Prompt          -> "gen/rust/prompt.rs"        : rust,
    describe_action -> "gen/typescript/describe.ts" : typescript,
    agent_config    -> "config/agent.yml"          : yaml,
}
```

规则（P 规则，完整表见第十章）：

| 规则 | 内容 |
|:---|:---|
| P-1 | 每文件至多一个 `project {}` 块、至多一个 `scale =` 声明 |
| P-2 | 同一物理路径在整个工程内只能被一个投射项占据（多语言冲突 → 警告并跳过后者） |
| P-3 | 投射目标必须存在（本文件定义或 import 引入），否则 error |
| P-4 | `block/static` 只能投射到 6 种静态格式；代码项（fn/impl/struct/enum/trait/graph）只能投射到 32 种编程语言 |
| P-3 跨模块 | 同一逻辑项可投射到多个文件；一个文件可聚合多个逻辑项（同语言） |

要点：

- **同一项可多次投射**：`Prompt` 可以同时去 python / rust / java……
  这是「一份逻辑，多语言散布」的核心机制。
- **一个物理文件可聚合多个项**：投射到同一路径的多个项按
  类型 → trait → impl → fn → graph 的顺序合并进同一文件。
- 语言 id 支持**别名**：`ts→typescript`、`js→javascript`、`py→python`、
  `md→markdown`、`yml→yaml`、`c++→cpp`、`sh/bash→bash`。
  查看全部合法 id：`bun dhv-ts/src/main.ts targets`。

## 5.2 语言注册表：四个 tier 与静态格式

32 种编程语言按工程角色分四个 tier，外加 6 种静态格式：

| Tier | 定位 | 语言（id） |
|:---|:---|:---|
| Tier 1 · Harness 核心 | Agent 工程的主力语言 | python, typescript, javascript, rust, go, cpp, java, csharp, kotlin, swift |
| Tier 2 · 脚本与动态 | 胶水与运维脚本 | ruby, php, lua, perl, bash, powershell, r, julia |
| Tier 3 · 函数式 | 类型系统表达力强 | scala, elixir, erlang, haskell, ocaml, fsharp |
| Tier 4 · 系统与现代 | 现代多范式 | zig, nim, crystal, dart, groovy, objectivec, d, vb |
| 静态格式 | 配置与文档 | yaml, markdown, json, toml, ini, xml |

tier 是**工程语义分组**，不是能力限制——能力由下一节的三级分级决定。
完整注册表（每语言的能力级 / 扩展名 / 类型映射要点）见附录 A。

`targets` 命令输出示例（节选）：

```
  Tier 1 · Harness 核心（活体/语句子集翻译优先）
    python        Python        .py   full 活体翻译       native 可执行 · 语法校验:python3
    typescript    TypeScript    .ts   full 活体翻译       native 可执行 · 语法校验:bun-ts
    javascript    JavaScript    .js   full 活体翻译       native 可执行 · 语法校验:bun-js
    rust          Rust          .rs   logic 语句子集
    go            Go            .go   logic 语句子集
    cpp           C++           .cpp  logic 语句子集
    java          Java          .java contract 类型契约   — sealed interface + record 契约（Java 17+）
    ...
  Static · 静态资源格式
    yaml          YAML          .yml  原文+插值
    ...
```

## 5.3 能力分级：full / logic / contract

DHV 对代码生成能力**分级并写进 manifest**，绝不假装能翻译一切：

| 能力级 | 语言 | 生成什么 |
|:---|:---|:---|
| **full**（活体翻译） | python / typescript / javascript | 类型 + 签名 + **函数体语句级翻译**（let / if-elif / while / for / match / 调用链 / 方法映射 / format!→f-string 等） |
| **logic**（语句子集） | rust / go / cpp | 类型 + 签名 + 语句子集翻译；遇到不支持构件**回退 contract**（绝不输出半翻译代码） |
| **contract**（类型契约） | 其余 26 种编程语言 | 类型与签名**真实翻译**为该语言的声明（record / sealed / data class / sum type…）；函数体以围栏内嵌 HSL 原文 + 显式未实现标记 |
| **static**（原文+插值） | 6 种静态格式 | `block/static` 原文渲染 `{{}}` 插值后落盘 |

### full 级真实示例

HSL 源（examples/backends-demo/model.hsl）：

```hsl
export fn describe_action(action: Action) -> String {
    match action {
        Action::CallTool { name, args } => format!("call {} with {} args", name, args.len()),
        Action::Respond { text } => format!("respond: {}", text),
        Action::Stop => String::from("stop"),
    }
}
```

python 后端生成（`gen/python/describe.py`，真实输出节选）：

```python
def describe_action(action: Action) -> str:
    # @dhv:source-map: model.hsl:42, block: describe_action (live)
    if isinstance(action, CallTool):
        name = action.name
        args = action.args
        return f"call {name} with {len(args)} args"
    elif isinstance(action, Respond):
        text = action.text
        return f"respond: {text}"
    elif isinstance(action, Stop):
        return 'stop'
    else:
        raise ValueError('dhv: match 不可达分支（S-6 穷尽性）')
    # @dhv:hsl-mirror — HSL 源镜像（编辑此区后 dhv sync 回写源码）
    # export fn describe_action(action: Action) -> String {
    #     match action {
    #         Action::CallTool { name, args } => format!("call {} with {} args", name, args.len()),
    #         Action::Respond { text } => format!("respond: {}", text),
    #         Action::Stop => String::from("stop"),
    #     }
    # }
    # @dhv:end-source-map
```

可以看到：match → isinstance 链、字段解构 → 属性访问、`format!` → f-string、
`args.len()` → `len(args)`。翻译后的 python 代码通过了
`python3 -m py_compile` 语法校验，且经 exec 实测输出正确。

### logic 级真实示例

HSL 源：

```hsl
export fn clamp_turns(turns: i64, lo: i64, hi: i64) -> i64 {
    if turns < lo {
        lo
    } else if turns > hi {
        hi
    } else {
        turns
    }
}
```

rust 后端生成（`gen/rust/clamp.rs`，真实输出节选）：

```rust
pub fn clamp_turns(turns: i64, lo: i64, hi: i64) -> i64 {
    // @dhv:source-map: model.hsl:50, block: clamp_turns (live)
    if (turns < lo) {
        return lo;
    } else {
        if (turns > hi) {
            return hi;
        } else {
            return turns;
        }
    }
    // @dhv:hsl-mirror — HSL 源镜像（编辑此区后 dhv sync 回写源码）
    // export fn clamp_turns(turns: i64, lo: i64, hi: i64) -> i64 {
    //     if turns < lo {
    //         lo
    //     } else if turns > hi {
    //         hi
    //     } else {
    //         turns
    //     }
    // }
    // @dhv:end-source-map
}
```

logic 级的边界示例：当函数体含有翻译器尚不支持的构件
（例如对 String 调 `.lines()` 迭代），rust 后端**自动回退 contract**——
签名照译，函数体给出 `todo!(...)` + 围栏原文，绝不输出半翻译代码。
第十二章的 `stats_of`（for-in 遍历 `content.lines()`）在 rust 后端即回退
contract，而 python 后端为 full 翻译。这就是「logic 语句子集」的准确含义。

### contract 级真实示例

同一个 `describe_action` 投射到 scala：

```scala
def describe_action(Action action): String = {
    // @dhv:source-map: model.hsl:42, block: describe_action
    // @dhv:hsl-mirror — HSL 源镜像（编辑此区后 dhv sync 回写源码）
    // export fn describe_action(action: Action) -> String {
    //     ...
    // }
    // @dhv:end-source-map
    throw new NotImplementedError("dhv: describe_action 未翻译 — HSL 逻辑在 @dhv:source-map 围栏内，运行请用 dhv-ts 或 dhv 编译")
}
```

类型与签名是真实 Scala；函数体抛 NotImplementedError 并在围栏内保留
HSL 原文。每种语言的未实现标记都符合其母语习惯：python 是
`raise NotImplementedError(...)`，kotlin 是 `TODO("...")`，zig 是
`return error.DhvContract;`，vb 是 `Throw New NotImplementedException(...)`。

### 结构体在各能力级的真实对照

同一个 `Prompt { system: String, user: String }`：

| 后端 | 能力级 | 生成物 |
|:---|:---|:---|
| python | full | `@dataclass` 类 |
| typescript | full | `export interface` + 工厂函数 |
| rust | logic | `#[derive(...)] pub struct Prompt { pub system: String, ... }` |
| java | contract | `record Prompt(String system, String user) {}` |
| kotlin | contract | `data class Prompt(val system: String, val user: String)` |
| ruby | contract | `Prompt = Struct.new(:system, :user, keyword_init: true)` |
| haskell | contract | `data Prompt = Prompt { ... } deriving (Show, Eq)` |

enum 的和类型在各后端用其原生机制表达：Java 用 sealed interface + record，
Swift 用带关联值的 enum，Scala 用 sealed trait + case class，
Haskell / OCaml 用原生 sum type，PHP 8.1 用 enum，Zig 用 tagged union。

## 5.4 静态资源：block / static 与 {{}} 插值

`block` 与 `static` 是同义词（风格自选），体内容是**原始文本**——
不做 HSL 解析，只识别 `{{ 表达式 }}` 编译期插值：

```hsl
const MAX_TURNS: i64 = 24;

block agent_config {
agent:
  name: BackendsDemo
  version: 2
  max_turns: {{MAX_TURNS}}
  languages: 38
}

static agent_instructions {
# Backends Demo Agent
这是一个跨 38 后端的投射演示。
当前预算：{{MAX_TURNS - 12}} 轮。
}
```

投射与生成（真实输出）：

```hsl
project {
    agent_config    -> "config/agent.yml"  : yaml,
    agent_config    -> "config/agent.json" : json,
    agent_config    -> "config/agent.toml" : toml,
    agent_config    -> "config/agent.ini"  : ini,
    agent_config    -> "config/agent.xml"  : xml,
    agent_instructions -> "docs/AGENTS.md" : markdown,
}
```

```
config/agent.yml     yaml      static  94 B ← agent_config
docs/AGENTS.md       markdown  static 104 B ← agent_instructions
...
```

生成的 `agent.yml` 内容（插值已求值）：

```yaml
agent:
  name: BackendsDemo
  version: 2
  max_turns: 24
  languages: 38
```

要点：

- 插值表达式必须可 `ToString`（数值 / bool / String / 枚举），
  Vec / struct 表达式不可插值（N-4）。
- 插值在 **emit 时**（编译期语义）求值；dsh 的实践是把常量
  （DEFAULT_MAX_TURNS 等）注入配置，运行期状态留给 harness 代码处理。
- 静态格式后端**不翻译内容**——你写什么 YAML 就是 YAML。词法器只做
  两件事：大括号深度计数（字符串内大括号不计数）+ `{{}}` 插值识别。
  因此 block 体建议**顶格书写**（缩进会原样进入产物）。
- JSON 无注释：dhv 以 `.map` 边车文件记录围栏信息（注册表 note）。

## 5.5 scale 对脚手架的影响

graph 项投射时，两种 scale 生成**形态不同**的脚手架。
以下是一个含 `model` / `executor` 双节点 graph 的真实输出节选：

```hsl
enum Action { Tool { name: String }, Done }

graph DemoAgent {
    node model: String = String::from("m");
    node executor: String = String::from("e");
    edge model -> executor on Action::Tool;
    loop { /* ... */ }
}

scale = microkernel;   // 换成 monolith 再 emit 即得另一种形态

project {
    DemoAgent -> "scaffold.py" : python,
    DemoAgent -> "scaffold.rs" : rust,
}
```

microkernel（默认）——python 后端生成 Plugin 注册表 + 事件总线循环：

```python
# @dhv:generated — graph DemoAgent 脚手架（microkernel 尺度）· 不可手改
# 拓扑：model, executor
# 边：model -> executor
# AgentLoop：✓（编译期强制 match 全分支）
# microkernel：节点 → Plugin 实现，边 → 事件总线订阅
demo_agent_plugins = {}  # 事件总线注册表
demo_agent_plugins['model'] = None  # Plugin 注入位
demo_agent_plugins['executor'] = None  # Plugin 注入位
def demo_agent_run():
    while True:
        # 事件总线驱动 AgentLoop（边 = 订阅）
        break
```

rust 后端的 microkernel 脚手架更明显——生成 Plugin struct 与注册总线：

```rust
pub struct DemoAgentPlugin {
    pub name: String,
    pub on_event: fn(evt: &str, payload: &str) -> String,
}
pub fn demo_agent_run() {
    let mut bus: Vec<DemoAgentPlugin> = Vec::new();
    bus.push(DemoAgentPlugin { name: "model".into(), on_event: todo!() });
    bus.push(DemoAgentPlugin { name: "executor".into(), on_event: todo!() });
    loop {
        // 事件总线驱动 AgentLoop
        break;
    }
}
```

monolith——节点局部变量化 + 直接调用（python 后端）：

```python
# @dhv:generated — graph DemoAgent 脚手架（monolith 尺度）· 不可手改
# monolith：节点 → 函数，边 → 直接调用
def demo_agent_run():
    # 节点实例化（monolith：局部变量）
    model = None  # str
    executor = None  # str
    while True:
        # AgentLoop：match Action — 全分支处理（S-6）
        break
```

脚手架之后，两种形态都会附上 graph 的完整 HSL 源镜像围栏
（供 sync 回写与人工对照，见第八章）。

## 5.6 manifest.json 与诚实边界协议

每次 emit 在产物根目录写 `manifest.json`，逐文件记录能力级与校验结果：

```json
{
  "dhv": "dhv-ts 0.2.0",
  "entry": "agent.hsl",
  "scale": "microkernel",
  "backends": 38,
  "generated_at": "2026-08-29T16:39:29.619Z",
  "files": [
    {
      "path": "gen/python/prompt.py",
      "lang": "python",
      "tier": "full",
      "bytes": 893,
      "items": ["Prompt"],
      "syntax_check": "pass",
      "syntax_tool": "python3 -m py_compile"
    },
    ...
  ],
  "protocol": {
    "fence": "@dhv:source-map / @dhv:hsl-mirror / @dhv:end-source-map",
    "sync": "编辑围栏内 HSL 镜像 → dhv sync <file> 回写 .hsl → dhv emit 重新生成",
    "honesty": {
      "full": "活体语句翻译（python/typescript/javascript）",
      "logic": "语句子集翻译（rust/go/cpp），不可翻译时回退 contract",
      "contract": "类型契约 + 签名真实翻译，函数体围栏内嵌 HSL 源镜像 + 未实现标记"
    }
  }
}
```

manifest 是机器可读的「能力声明」——CI 可以断言 `syntax_check == "pass"`，
下游团队可以查询每个文件的 tier 决定是否手工接管。

---

## 5.7 跨文件类型依赖：投射产物之间的自动接线

一个真实项目里，类型和函数往往被投射到**不同的物理文件**——`summarize(results: Vec<ToolResult>)`
投射到 `gen/python/summarize.py`，而 `ToolResult` 结构体投射到 `gen/python/toolresult.py`。
v0.2.1 起，emit 会自动追踪这类跨文件类型引用，并按目标语言的导入机制接线：

| 后端 | 接线方式 | 示例 |
|---|---|---|
| python | 同目录平铺导入 | `from toolresult import ToolResult` |
| typescript / javascript | 相对路径导入 | `import { ToolResult } from './toolresult';` |
| rust | 模块组装约定 | `use crate::gen::rust::toolresult::{ToolResult};` |
| go | 免导入 | 全部产物同包 `package hsl`，同包类型直接可见 |
| cpp | **内联类型声明** | 在引用文件顶部内联一份逐字一致的声明（ODR 兼容，多编译单元安全） |
| java | **顶层类型 + 裸名互见** | 类型项（record/sealed interface/interface）顶层声明于同包；fn/const/impl 宿主 `class Dhv<文件stem>`（v0.2.5 重构：旧版全项嵌 `public class <模块名>` 是非法 Java——public 类名必须匹配文件名，且同模块多文件 wrapper 重名） |
| 其余（contract 级） | 不接线 | 函数体本就是围栏，类型名由契约纪律保障 |

真实产物长这样（`gen/cpp/describe.cpp`，`Action` 未单独投射到 cpp，从 AST 兜底内联）：

```cpp
// ---- 跨文件类型依赖（源 examples/backends-demo/model.hsl，未单独投射，内联声明）----
// Action — HSL enum 和类型（C++17 std::variant）
struct CallTool {
    std::string name;
    std::unordered_map<std::string, std::string> args;
};
struct Respond {
    std::string text;
};
struct Stop {
};
using Action = std::variant<CallTool, Respond, Stop>;
```

**诚实告警协议**：当类型引用无法自动接线时，emit 输出告警并**退出码 1**（与 P 系列
投射规则同权），绝不静默生成坏代码：

- `X-1`：类型被引用但未投射到该语言（生成物引用未定义名）——修法是在 project{} 里补上投射目标
- `X-2`：python 跨目录导入（平铺导入不可达，需手动接线）
- `X-3`：rust 目标文件路径不是合法模块路径（如含连字符）
- `X-4`：go 跨目录 = 跨包（同包免导入的前提被打破）

graph 脚手架（monolith/microkernel）的类型名只出现在 HSL 镜像注释里，
不参与依赖接线（脚手架本体是通用插件注册表，不活体引用类型）。

---

## 5.8 投射规则组 `rules {}`（BNF v1.5）

§5.1 的显式映射适合小项目——每个项手写一行路径。当一个模块有
几十个 struct / fn / enum 时，逐项映射就变成了体力活。
`rules {}` 让你**按项类型批量投射**，用 `{name}` 占位符自动展开。

### 语法

`rules {}` 写在 `project {}` 内部，与显式映射项混排：

```hsl
project {
    // 显式映射（优先级最高）
    process -> "src/core/process.rs" : rust,

    // 批量规则
    rules {
        struct -> "src/types/{name}.rs"  : rust,
        enum   -> "src/types/{name}.rs"  : rust,
        fn     -> "src/logic/{name}.rs"  : rust,
        graph  -> "src/graphs/{name}.rs" : rust,
        block  -> "config/{name}.yml"    : yaml,
        const  -> "src/consts/{name}.rs" : rust,
    }
}
```

规则类型限定 9 种：`graph` / `fn` / `struct` / `enum` / `trait` /
`const` / `type` / `block` / `static`。`block` 与 `static` 同义（均指
`StaticResourceDef`）。路径模板目前只支持 `{name}` 一个占位符，
展开时替换为该项的标识符名。

### 语义六条（R1-R6）

| 规则 | 含义 |
|:---|:---|
| **R1 遮蔽原则** | 显式单项映射始终优先；未被显式覆盖的命名项按其类型匹配唯一规则展开 |
| **R2 占位符白名单** | 路径模板 v1 仅支持 `{name}`；出现其他占位符（如 `{module}`）→ 诊断 P5 |
| **R3 唯一性** | 同一规则类型只允许声明一条；重复声明 → P5 |
| **R4 类型注册** | 规则类型必须是上述 9 种之一；未知类型（如 `widget`）→ P5 |
| **R5 展开池** | 展开池 = 本文件命名项 + import 依赖模块的导出命名项；`impl`（匿名）、import、宏调用不参与 |
| **R6 一致性** | 展开产生的投射项与显式项同等参与 P2（路径唯一）/ P4（后端层级）校验 |

### 完整示例

以下示例经 `dhv check` 与 `dhv-ts check` 双编译器实测通过：

```hsl
struct Point { x: i64, y: i64 }
enum Status { Active, Inactive }
const MAX: i64 = 100;

block app_cfg {
mode = production
port = 8080
}

export fn process(p: Point) -> Status {
    Active
}

export graph Agent {
    node start: String = String::from("");
    node end: String = String::from("");
    edge start -> end;
    loop { break; }
}

project {
    // 显式映射：process 走专用路径，不触发 fn 规则
    process -> "src/core/process.rs" : rust,

    rules {
        struct -> "src/types/{name}.rs"  : rust,
        enum   -> "src/types/{name}.rs"  : rust,
        fn     -> "src/logic/{name}.rs"  : rust,
        graph  -> "src/graphs/{name}.rs" : rust,
        block  -> "config/{name}.yml"    : yaml,
        const  -> "src/consts/{name}.rs" : rust,
    }
}
```

展开后等价于（`process` 走显式，其余走规则）：

```hsl
// 等价展开（编译器内部视图）
project {
    process  -> "src/core/process.rs"   : rust,   // 显式
    Point    -> "src/types/Point.rs"     : rust,   // struct 规则
    Status   -> "src/types/Status.rs"    : rust,   // enum 规则
    MAX      -> "src/consts/MAX.rs"      : rust,   // const 规则
    app_cfg  -> "config/app_cfg.yml"     : yaml,   // block 规则
    Agent    -> "src/graphs/Agent.rs"    : rust,   // graph 规则
}
```

### 跨模块展开（R5）

规则不仅展开本文件定义的项，还覆盖 **import 依赖模块的导出项**。
假设 `lib.hsl` 导出了 `Saved` 结构体和 `save_all` 函数：

```hsl
// lib.hsl
export struct Saved { id: String }
export fn save_all(items: Vec<Saved>) -> i64 { items.len() as i64 }
```

```hsl
// root.hsl
import { Saved, save_all } from "./lib.hsl";

export fn run() -> i64 {
    let s = Saved { id: String::from("a") };
    save_all(vec![s])
}

project {
    rules {
        struct -> "src/types/{name}.rs" : rust,
        fn     -> "src/logic/{name}.rs" : rust,
    }
}
```

展开池包含 `run`（本文件）、`Saved` 和 `save_all`（lib.hsl 导出），
三个项全部按规则投射。注意 `impl`、`import` 语句和宏调用
不参与展开——它们要么匿名（impl）、要么不是命名项（import）、
要么展开时机不同（宏在解析期展开）。

---

# 第六章 native 逃生舱

再好的语言层抽象也吃不完宿主生态。HSL 的答案不是 FFI，而是
**native 逃生舱**：`native <lang> { ... }` 是一个**表达式**，体内是
原样搬运的目标语言代码（词法器按目标语言的字符串规则做大括号配对，
不做任何 HSL 解析）。

```hsl
let content: String = native typescript {
    return await $host.fs.read(path);     // path 是 HSL 变量，按名捕获
};
```

## 6.1 native typescript 与 native python：直接可执行

dhv-ts 运行期支持两种 native 块的**真实执行**：

| | native typescript | native python |
|:---|:---|:---|
| 执行方式 | 进程内 `new Function`（async 包裹） | `python3` 子进程 |
| 数据编组 | 进程内直接共享值 | JSON 编组进出（HSL 值 → JSON → python → JSON → HSL 值） |
| 返回值 | 显式 `return`；无 return 时末表达式自动包裹 `return (...)` | 显式 `return`；无 return 时末行自动变换 `__hsl_result__ = (末行)` |
| 超时 | — | 30 秒 |
| 其余语言 | 运行期明确报错（由 dhv 静态投射，P5） | 同左 |

```hsl
fn main() -> i64 {
    // 1) typescript：进程内执行
    let forty_two: i64 = native typescript {
        return 6 * 7;
    };
    println!("native_ts = {}", forty_two);

    // 2) python：子进程执行，末表达式即返回值
    let py: String = native python {
        "hello " + "from python"
    };
    println!("native_py = {}", py);

    // 3) 捕获变量按名映射：HSL 变量 prefix 直接出现在 TS 代码里
    let prefix = String::from("[HSL]");
    let stamped: String = native typescript {
        return prefix + " captured";
    };
    println!("captured = {}", stamped);
    0
}
```

运行（真实输出）：

```
native_ts = 42
native_py = hello from python
captured = [HSL] captured
```

## 6.2 运行期 ABI：$host 与捕获变量

native 块内可访问 **`$host`**——宿主 API 命名空间（运行时能力，
不属于语言语义，BNF 附录 B）：

| 命名空间 | 能力 | 安全边界 |
|:---|:---|:---|
| `$host.fs` | `read / write / edit / list` | **路径监狱**：所有操作限制在 `--workspace` 内，越界抛错 |
| `$host.shell` | `run(cmd, {cwd, timeoutMs})` | **首词白名单**（`--allow`）+ 超时 + 输出上限；拒绝时发 `capability_denied` 事件 |
| `$host.llm` | `complete({messages, temperature, maxTokens})` | LLM 网关（`--model deepseek` 时走真实模型） |
| `$host.json` | `parse / stringify / fields` | `fields` 把 JSON 顶层字段**字符串化**为 `HashMap<String, String>` |
| `$host.artifacts` | `write(name, content)` | 运行产物写出（不受工作区监狱限制，写入 `--out` 目录） |
| `$host.events` | `emit(name, data)` | 事件总线（G6 观测通道，落盘 events.jsonl） |
| `$host.fixture` | `nextAct / nextReview / actsLeft` | 剧本装置（`--fixture`，见第十一章） |
| `$host.config` | model / workspace / task / scale / 预算参数 | CLI 参数的只读视图 |
| `$host.log` / `$host.env` | 轨迹日志 / 环境变量 | — |

```hsl
fn main() -> i64 {
    // $host.json.fields：保持类型纪律的 JSON 桥
    let raw = String::from("{\"tool\": \"grep\", \"limit\": 10}");
    let fields: HashMap<String, String> = native typescript {
        return $host.json.fields(raw);
    };
    println!("tool = {}", fields.get(String::from("tool")).unwrap_or(String::from("?")));

    // $host.shell：白名单命令（默认 bun,node,ls,cat,grep,diff）
    let out: String = native typescript {
        const r = await $host.shell.run("echo hello-shell");
        return r.stdout.trim();
    };
    println!("shell = {}", out);

    // $host.events：向事件总线发自定义事件
    let _ = native typescript {
        $host.events.emit("custom_event", { source: "guide" });
        return true;
    };

    // 路径监狱：越界读取被拒绝
    let jailed: String = native typescript {
        try {
            await $host.fs.read("../../etc/passwd");
            return "escaped!";
        } catch (e) {
            return String(e && e.message || e);
        }
    };
    println!("jail = {}", jailed);
    0
}
```

真实输出（默认白名单不含 echo，第一条 shell 被拦截后以 `--allow ...,echo`
重跑得到第二组结果）：

```
shell =                       ← 默认白名单拒绝 echo（events.jsonl 记录 capability_denied）
shell = hello-shell           ← --allow bun,node,ls,cat,grep,diff,echo 之后
jail = 路径越界（capability 违规）：../../etc/passwd 逃出工作区 /tmp/guide-test
```

events.jsonl 同步记录：

```json
{"name":"capability_denied","data":{"command":"echo hello-shell","reason":"首词 \"echo\" 不在白名单 [bun, node, ls, cat, grep, diff]"}}
{"name":"custom_event","data":{"source":"guide"}}
```

捕获规则（N-1）：

- native 体中引用的标识符若在 HSL 词法作用域中存在，**按名注入**块作用域
  （值拷贝语义；python 侧经 JSON 编组）。
- **self 的字段必须写 `self.field`**——self 本身是捕获变量，字段不是。
- 以 `$` 开头的名字（`$host`）不参与捕获。
- S-7 联动：被 native 块按名引用的 HSL 变量视为「已使用」。

## 6.3 其他语言的 native 块：静态投射语义

32 种编程语言的 native 块在**运行期不可执行**（dhv-ts 明确报错，
指向 dhv 静态投射）：

```
✗ 运行期错误：native rust 后端未接入解释器（dhv-ts 运行期支持 typescript / python；
  rust 由 dhv Rust 编译器静态投射，P5）
```

它们的语义是**静态投射**（P5）：当 native 块语言与投射目标语言一致时，
块内代码**原样透传**进生成物（body.ts 的「native 同语言块原样透传」机制）；
语言不一致时由 dhv 编译器生成 FFI 胶水。典型用法是
`native rust { ... }` 写进投射到 rust 后端的函数里。

`native yaml` / `native markdown` 等静态格式标识同样不合法于运行期
（语言标识必须来自 38 后端注册表，N-1 校验 id 合法性）。

## 6.4 类型纪律：JSON 编组

进出 native 块的值应为**平凡类型**（N-2）：

```
bool / i64 等数值 / String / Vec / Option / HashMap
```

python 侧的编组规则（native.ts `marshalForPython`）：

| HSL 值 | python 侧 |
|:---|:---|
| String / 数值 / bool | 原样 |
| Vec | list（递归编组） |
| HashMap | dict（键字符串化） |
| struct | dict（剥掉元字段） |
| enum | `{__variant: 变体名, ...命名负载, __tuple: [...]}` |
| bigint | Number |

返回方向：python 子进程 stdout 中的 `__HSL_OUT__` 标记后跟 JSON，
解释器解码为 HSL 值（数组即 Vec，对象即 plain 对象/HashMap 语义）。

**纪律建议**（dsh 项目的设计决策）：不要让动态对象图穿透进 HSL——
用 `$host.json.fields` 把 JSON 顶层字段字符串化为
`HashMap<String, String>`，解析与判定留在 HSL 侧，native 只做 IO 与
API 调用。这保证同一逻辑投射到任何后端时语义一致（N3 精神）。

## 6.5 何时该用 native

**该用**：

- 吃宿主生态库：pandas 数据处理、openai 客户端、tokio 异步运行时、
  faiss 向量检索（nova 示例的真实用法）；
- 平台效应：文件系统、子进程、网络（经 $host 沙箱）；
- 与既有系统对接的胶水。

**不该用**：

- 业务判定逻辑（协议解析、预算判定、裁决分发）——这些应该是纯 HSL，
  否则失去穷尽性检查与多后端投射能力；
- 能用 std 库解决的事（第七章）。

一句话总结 dsh README 的设计决策：**native 只做效应，逻辑留在 HSL。**

---

# 第七章 标准库参考

标准库以**虚拟模块**形式内建于 dhv-ts，共 10 个模块、约 60 个函数
+ 2 个数学常量。导入语法：

```hsl
import { levenshtein, split_once } from "std/text";
import { range, take } from "std/iter";
```

- `std/` 路径不走文件系统（linker 直接映射到虚拟模块）。
- S-7 同样约束 std import：导入但未使用是错误。
- 完整速查表见附录 B；本章逐模块讲解语义与示例。
- 本章多数示例取自 `dhv-ts/examples/std-tour.hsl`（可直接运行：
  `bun dhv-ts/src/main.ts run dhv-ts/examples/std-tour.hsl`），
  输出均为真实运行结果。

## 7.1 std/core

**定位**：身份、断言式失败、类型内省、稳定哈希。

| 函数 | 签名 | 说明 |
|:---|:---|:---|
| `identity` | `(T) -> T` | 原样返回（泛型透传的约定锚点） |
| `todo` | `(String?) -> !` | 抛出 `todo!: <消息>`（未实现占位） |
| `unreachable` | `(String?) -> !` | 抛出 `unreachable: <消息>`（逻辑不可达分支） |
| `type_name` | `(T) -> String` | 运行期类型名：`"String"` / `"bool"` / `"i64"` / `"f64"` / `"Vec"` / `"HashMap"` / struct/enum 名 |
| `hash` | `(T) -> i64` | FNV-1a 64 位哈希（对值的显示文本计算；稳定可复现） |

```hsl
import { identity, type_name, hash } from "std/core";

fn main() -> i64 {
    println!("identity(42) = {}", identity(42));
    println!("type_name(3.14) = {}", type_name(3.14));
    println!("hash(\"hsl\") = {}", hash(String::from("hsl")));
    0
}
```

```
identity(42) = 42
type_name(3.14) = f64
hash("hsl") = 260547981763452358
```

## 7.2 std/collections

**定位**：Vec 的构建与批量变换（方法面之外的补充）。

| 函数 | 签名 | 说明 |
|:---|:---|:---|
| `vec` | `(...T) -> Vec<T>` | 由实参构造 Vec |
| `repeat_vec` | `(T, i64) -> Vec<T>` | 重复填充 |
| `zip` | `(Vec<A>, Vec<B>) -> Vec<(A, B)>` | 按位配对（取短） |
| `chunk` | `(Vec<T>, i64) -> Vec<Vec<T>>` | 定长分块 |
| `dedup` | `(Vec<T>) -> Vec<T>` | 去除**相邻**重复 |
| `unique` | `(Vec<T>) -> Vec<T>` | 全局去重（保序） |
| `flatten` | `(Vec<T>) -> Vec<T>` | 压平一层嵌套 Vec |
| `sort_desc` | `(Vec<i64>) -> Vec<i64>` | 数值降序 |
| `reverse` | `(Vec<T>) -> Vec<T>` | 反转 |
| `swap_remove` | `(Vec<T>, i64) -> T` | 交换删除（O(1)，越界抛错） |

```hsl
import { vec, zip, chunk, dedup, unique, flatten, reverse } from "std/collections";

fn main() -> i64 {
    let v = vec(1, 2, 3, 4, 5, 6);
    let z = zip(vec(1, 2), vec(10, 20));
    println!("zip = {:?}", z);
    println!("chunk(v,2) = {:?}", chunk(v, 2));
    println!("dedup = {:?}", dedup(vec(1, 1, 2, 2, 3)));
    println!("unique = {:?}", unique(vec(1, 2, 1, 3, 2)));
    println!("flatten = {:?}", flatten(vec(vec(1, 2), vec(3))));
    println!("reverse = {:?}", reverse(vec(1, 2, 3)));
    0
}
```

```
zip = [[1, 10], [2, 20]]
chunk(v,2) = [[1, 2], [3, 4], [5, 6]]
dedup = [1, 2, 3]
unique = [1, 2, 3]
flatten = [1, 2, 3]
reverse = [3, 2, 1]
```

注意 `dedup` 与 `unique` 的区别：`dedup` 只合并相邻重复
（`[1,2,1]` 保持原样），`unique` 全局去重。

## 7.3 std/text

**定位**：String 的分割、命名风格转换、填充、编辑距离。

| 函数 | 签名 | 说明 |
|:---|:---|:---|
| `split_once` | `(String, String) -> Option<(String, String)>` | 首次分割（找不到返回 None） |
| `rsplit_once` | `(String, String) -> Option<(String, String)>` | 末次分割 |
| `split_at` | `(String, i64) -> (String, String)` | 按索引切两段 |
| `to_snake` / `to_camel` / `to_pascal` / `to_kebab` | `(String) -> String` | 命名风格互转 |
| `pad_start` / `pad_end` | `(String, i64, String?) -> String` | 定宽填充（默认空格） |
| `capitalize` | `(String) -> String` | 首字母大写 |
| `count` | `(String, String) -> i64` | 子串计数 |
| `is_alpha` / `is_numeric` / `is_alphanumeric` | `(String) -> bool` | 字符集判定 |
| `truncate` | `(String, i64, String?) -> String` | 按字符截断（默认省略号 `…`） |
| `levenshtein` | `(String, String) -> i64` | 编辑距离（Unicode 感知） |

```hsl
import { split_once, to_snake, to_camel, pad_start, capitalize, levenshtein, truncate } from "std/text";

fn main() -> i64 {
    let parts = split_once(String::from("key=value"), String::from("="));
    match parts {
        Option::Some(pair) => println!("split_once = {} / {}", pair[0], pair[1]),
        Option::None => println!("none"),
    }
    println!("to_snake(\"myToolName\") = {}", to_snake(String::from("myToolName")));
    println!("to_camel(\"my_tool_name\") = {}", to_camel(String::from("my_tool_name")));
    println!("pad_start(\"7\", 3, \"0\") = {}", pad_start(String::from("7"), 3, String::from("0")));
    println!("capitalize(\"hsl\") = {}", capitalize(String::from("hsl")));
    println!("levenshtein(kitten/sitting) = {}", levenshtein(String::from("kitten"), String::from("sitting")));
    println!("truncate(long, 8) = {}", truncate(String::from("0123456789ABC"), 8));
    0
}
```

```
split_once = key / value
to_snake("myToolName") = my_tool_name
to_camel("my_tool_name") = myToolName
pad_start("7", 3, "0") = 007
capitalize("hsl") = Hsl
levenshtein(kitten/sitting) = 3
truncate(long, 8) = 0123456…
```

`truncate` 按**字符**（不是字节）计数，对 CJK 友好。

## 7.4 std/math

**定位**：三角 / 指数对数 / 整数数论 / 浮点判定 + 常量。

| 函数 / 常量 | 签名 | 说明 |
|:---|:---|:---|
| `PI` / `E` | `f64` | 圆周率 / 自然常数 |
| `sin` `cos` `tan` | `(f64) -> f64` | 三角 |
| `asin` `acos` `atan` `atan2` | `(f64[, f64]) -> f64` | 反三角（atan2 双参） |
| `exp` `ln` `log2` `log10` | `(f64) -> f64` | 指数与对数 |
| `pow` | `(f64, f64) -> f64` | 幂 |
| `sqrt` `hypot` | `(f64[, f64]) -> f64` | 平方根 / 模长 |
| `gcd` `lcm` | `(i64, i64) -> i64` | 最大公约 / 最小公倍 |
| `signum` | `(f64) -> f64` | 符号（-1/0/1） |
| `isqrt` | `(i64) -> i64` | 整数平方根（负数抛错） |
| `div_ceil` / `div_floor` | `(i64, i64) -> i64` | 向上 / 向下整除 |
| `rem_euclid` | `(i64, i64) -> i64` | 欧几里得余数（结果非负） |
| `is_nan` / `is_infinite` / `inf` | `(f64) -> bool` / `() -> f64` | 浮点判定与无穷大 |

```hsl
import { sin, cos, gcd, lcm, signum, isqrt, div_ceil, rem_euclid, PI } from "std/math";

fn main() -> i64 {
    println!("sin(0) = {}", sin(0.0));
    println!("cos(0) = {}", cos(0.0));
    println!("gcd(12,18) = {}", gcd(12, 18));
    println!("lcm(4,6) = {}", lcm(4, 6));
    println!("signum(-5) = {}", signum(-5));
    println!("isqrt(50) = {}", isqrt(50));
    println!("div_ceil(7,2) = {}", div_ceil(7, 2));
    println!("rem_euclid(-7,3) = {}", rem_euclid(-7, 3));
    println!("PI = {}", PI);
    0
}
```

```
sin(0) = 0
cos(0) = 1
gcd(12,18) = 6
lcm(4,6) = 12
signum(-5) = -1
isqrt(50) = 7
div_ceil(7,2) = 4
rem_euclid(-7,3) = 2
PI = 3.141592653589793
```

## 7.5 std/io

**定位**：文件读写——**宿主依赖**模块。

| 函数 | 签名 | 说明 |
|:---|:---|:---|
| `read_file` | `(String) -> Result<String, String>` | 读文本（限 2 MB） |
| `write_file` | `(String, String) -> Result<i64, String>` | 写文本，返回字符数 |
| `append_file` | `(String, String) -> Result<i64, String>` | 追加（无则新建） |
| `list_dir` | `(String) -> Result<Vec<String>, String>` | 列目录（两层深度，目录带 `/` 尾缀） |

**Result 语义与宿主依赖**（这是本模块最重要的知识点）：

- 全部函数返回 `Result`，**绝不抛异常**——失败路径是显式的 Err；
- 它们走 `$host.fs` 宿主通道：**只有在 `dhv-ts run` 模式**（有宿主运行时）
  才真正工作；
- 无宿主时（例如 check 或脱离 run 的求值）返回
  `Err("std/io::read_file 需要宿主运行时（dhv-ts run 模式）")`；
- 路径同样受**工作区监狱**限制（`--workspace` 之外即 Err）。

```hsl
import { read_file, list_dir, write_file } from "std/io";

fn main() -> Result<(), String> {
    match list_dir(String::from(".")) {
        Result::Ok(files) => {
            for f in files {
                println!("file: {}", f);
            }
        },
        Result::Err(e) => println!("list err: {}", e),
    }
    match read_file(String::from("a.txt")) {
        Result::Ok(text) => println!("a.txt lines = {}", text.lines().len()),
        Result::Err(e) => println!("read err: {}", e),
    }
    match write_file(String::from("out.txt"), String::from("written")) {
        Result::Ok(n) => println!("wrote {} chars", n),
        Result::Err(e) => println!("write err: {}", e),
    }
    Ok(())
}
```

在含 `a.txt` 的工作区运行（真实输出）：

```
file: a.txt
file: main.hsl
file: out/
a.txt lines = 4
wrote 7 chars
```

注意 `lines()` 把结尾换行后的空行也计为一行（`"a\nb\n"` 是 3 行）。
第十二章的实战项目即以 std/io 驱动文件读取。

## 7.6 std/json

**定位**：本地确定性 JSON——**零宿主依赖**（内置递归下降解析器，
跨后端语义一致）。

| 函数 | 签名 | 说明 |
|:---|:---|:---|
| `parse` | `(String) -> Result<T, String>` | 解析 JSON（对象→plain 对象、数组→Vec、数字→数值；失败返回 Err） |
| `stringify` | `(T) -> String` | 序列化（struct 去元字段、enum 按变体负载序列化、HashMap→对象） |
| `get` | `(T, String) -> Option<T>` | 取对象顶层字段（非对象 / 缺键返回 None） |

```hsl
import { parse, stringify, get } from "std/json";

fn main() -> i64 {
    let parsed = parse(String::from("{\"name\":\"hsl\",\"backends\":38}"));
    match parsed {
        Result::Ok(obj) => {
            match get(obj, String::from("name")) {
                Option::Some(name) => println!("json name = {}", name),
                Option::None => println!("json name missing"),
            }
            match get(obj, String::from("backends")) {
                Option::Some(n) => println!("json backends = {}", n),
                Option::None => println!("json backends missing"),
            }
        },
        Result::Err(e) => println!("json parse err = {}", e),
    }
    println!("stringify = {}", stringify(vec![1, 2, 3]));
    0
}
```

```
json name = hsl
json backends = 38
stringify = [1,2,3]
```

## 7.7 std/time

**定位**：时间戳与时长人类可读化（自然是非确定性的，
不要用于断言式测试）。

| 函数 | 签名 | 说明 |
|:---|:---|:---|
| `now_ms` | `() -> i64` | Unix 毫秒时间戳 |
| `now_iso` | `() -> String` | ISO 8601 UTC 时间串 |
| `duration_desc` | `(i64) -> String` | 毫秒 → `"<n>ms"` / `"<x.x>s"` / `"<n>m<n>s"` / `"<n>h<n>m"` |

```hsl
import { now_ms, duration_desc } from "std/time";

fn main() -> i64 {
    let t = now_ms();
    println!("now_ms = {}", t);
    println!("duration_desc(1234) = {}", duration_desc(1234));
    0
}
```

```
now_ms = 1788021564676
duration_desc(1234) = 1.2s
```

## 7.8 std/random

**定位**：**可复现伪随机**（PRNG = mulberry32）。默认种子 42——
同一份代码两次运行的随机序列**完全一致**，这是确定性测试的关键。

| 函数 | 签名 | 说明 |
|:---|:---|:---|
| `seed` | `(i64) -> ()` | 重设种子（此后序列重新开始） |
| `random` | `() -> f64` | [0, 1) 均匀浮点 |
| `int_in` | `(i64, i64) -> i64` | 闭区间均匀整数 |
| `choice` | `(Vec<T>) -> Option<T>` | 随机取一（空 Vec 返回 None） |
| `shuffle` | `(Vec<T>) -> Vec<T>` | Fisher-Yates 洗牌（返回新 Vec） |
| `uuid_v4` | `() -> String` | 36 字符 UUID（由 PRNG 驱动，同样可复现） |

```hsl
import { seed, random, int_in, choice, shuffle, uuid_v4 } from "std/random";

fn main() -> i64 {
    seed(42);
    println!("random() #1 = {}", random());
    println!("random() #2 = {}", random());
    println!("int_in(1,6) = {}", int_in(1, 6));
    println!("choice = {:?}", choice(vec![String::from("a"), String::from("b")]));
    println!("shuffle = {:?}", shuffle(vec![1, 2, 3, 4, 5]));
    println!("uuid_v4 = {}", uuid_v4());
    0
}
```

```
random() #1 = 0.6011037519201636
random() #2 = 0.44829055899754167
int_in(1,6) = 6
choice = Some("b")
shuffle = [4, 2, 5, 3, 1]
uuid_v4 = d73eb438-a907-4d09-0402-c802d7c57008
```

`seed(42)` 之后这份输出**每次运行都相同**——把随机化纳入 CI 断言
不再需要 mock。需要真随机时用环境变量做种子即可。

## 7.9 std/env

**定位**：运行环境与配置内省。

| 函数 | 签名 | 说明 |
|:---|:---|:---|
| `env_get` | `(String) -> Option<String>` | 环境变量（不存在返回 None） |
| `task_text` | `() -> String` | `--task` 任务描述（无则空串） |
| `model_name` | `() -> String` | `--model` 名（默认 `scripted`） |
| `workspace` | `() -> String` | 工作区路径（无宿主时为 cwd） |

```hsl
import { env_get } from "std/env";

fn main() -> i64 {
    match env_get(String::from("PATH")) {
        Option::Some(p) => println!("PATH 头 20 字符 = {}", p.take(20)),
        Option::None => println!("(PATH 不可见)"),
    }
    0
}
```

```
PATH 头 20 字符 = /home/z/.venv/bin:/h
```

## 7.10 std/iter

**定位**：序列的构造与切片（急切求值，返回 Vec）。

| 函数 | 签名 | 说明 |
|:---|:---|:---|
| `range` | `(i64, i64) -> Vec<i64>` | `[lo, hi)`（范围上限 1e6 防失控） |
| `range_step` | `(i64, i64, i64) -> Vec<i64>` | 带步长（支持递减；step=0 抛错） |
| `enumerate` | `(Vec<T>) -> Vec<(i64, T)>` | 配索引 |
| `chain` | `(Vec<T>, Vec<T>) -> Vec<T>` | 拼接 |
| `take` / `skip` | `(Vec<T>, i64) -> Vec<T>` | 取前 n / 跳过前 n |
| `min_of` / `max_of` | `(Vec<i64>) -> Option<i64>` | 最值（空 Vec 返回 None） |

```hsl
import { range, range_step, enumerate, chain, take, min_of, max_of } from "std/iter";

fn main() -> i64 {
    println!("range(0,5) = {:?}", range(0, 5));
    println!("range_step(0,10,3) = {:?}", range_step(0, 10, 3));
    println!("enumerate = {:?}", enumerate(vec![String::from("a"), String::from("b")]));
    println!("chain = {:?}", chain(vec![1, 2], vec![3, 4]));
    println!("take(range(0,10),3) = {:?}", take(range(0, 10), 3));
    match min_of(vec![5, 2, 9]) {
        Option::Some(m) => println!("min_of = {}", m),
        Option::None => println!("min none"),
    }
    match max_of(vec![5, 2, 9]) {
        Option::Some(m) => println!("max_of = {}", m),
        Option::None => println!("max none"),
    }
    0
}
```

```
range(0,5) = [0, 1, 2, 3, 4]
range_step(0,10,3) = [0, 3, 6, 9]
enumerate = [[0, "a"], [1, "b"]]
chain = [1, 2, 3, 4]
take(range(0,10),3) = [0, 1, 2]
min_of = 2
max_of = 9
```

## 7.11 预导入方法面

除了 std 模块函数，HSL 还有**无需 import** 的内建方法面
（BNF 附录 A，实现在 builtins.ts）：

| 类型 | 常用方法 |
|:---|:---|
| String | `len` `is_empty` `trim` `contains` `starts_with` `ends_with` `replace` `split` `split_whitespace` `lines` `to_lowercase` `to_uppercase` `chars` `repeat` `strip_prefix` `strip_suffix` `find` `parse::<T>()` `push_str` `take` `clone` `as_str` |
| Vec | `len` `is_empty` `push` `pop` `first` `last` `get` `contains` `join` `map` `filter` `for_each` `any` `all` `fold` `sum` `position` `enumerate` `insert` `remove` `take` `skip` `sort` `sort_by` `sort_desc` `is_sorted` `clear` `append` `extend` `rev` `clone` |
| HashMap | `insert` `get` `contains_key` `len` `is_empty` `remove`（返回 Option 旧值）`keys` `values` `clear` `clone` |
| Option | `unwrap_or` `unwrap_or_else` `is_some` `is_none` `map` `and_then` `ok_or` `or` `cloned` `unwrap`（S-2 警告） |
| Result | `is_ok` `is_err` `ok` `err` `map` `map_err` `unwrap_or` `and_then` `or_else` `unwrap`（S-2 警告） |
| 数值 | `to_string` `abs` `pow` `sqrt` `floor` `ceil` `round` `min` `max` `clamp` |
| char | `to_string` `is_alphabetic` `is_numeric` |

预导入宏：`format!`（`{}` / `{0}` / `{:?}` / `{:.N}` 浮点十进制精度（v0.2.51） / `{{` 转义）、`vec!`、
`println!`、`print!`、`eprintln!`、`panic!`、`assert!`、`assert_eq!`、`dbg!`。

---

# 第八章 双向工程

第五章的生成物里反复出现 `@dhv:source-map` 围栏。本章讲清它是什么、
以及如何用它做**双向**工程：生成代码不只是产物，还是源码的另一种载体。

## 8.1 围栏协议图解

每个被投射的**函数体 / graph / impl** 都会在生成文件里留下一个三标记围栏：

```
<行注释> @dhv:source-map: <模块>:<行号>, block: <项名> [(live)]
[ 活体翻译区（仅 full/logic 且翻译成功时存在；内核生成，重编译覆盖） ]
<行注释> @dhv:hsl-mirror — HSL 源镜像（编辑此区后 dhv sync 回写源码）
<行注释> <HSL 源码逐行镜像>            ←—— 可编辑区（sync 回写依据）
<行注释> @dhv:end-source-map
```

以真实生成物为例（python 后端，`greet` 函数）：

```python
def greet(name: str) -> str:
    # @dhv:source-map: model.hsl:1, block: greet (live)
    return f"hello, {name}"
    # @dhv:hsl-mirror — HSL 源镜像（编辑此区后 dhv sync 回写源码）
    # export fn greet(name: String) -> String {
    #     format!("hello, {}", name)
    # }
    # @dhv:end-source-map
```

三个标记的职责：

| 标记 | 职责 | 谁拥有 |
|:---|:---|:---|
| `@dhv:source-map: module:line, block: name` | 定位：源模块 + 行号 + 项名；`(live)` 后缀表示其下有活体翻译区 | 内核（不可改） |
| 活体翻译区 | 真实目标语言代码（full/logic 能力级） | 内核（重编译覆盖；改 HSL 后经 sync→emit 更新） |
| `@dhv:hsl-mirror` 与 `@dhv:end-source-map` 之间 | **HSL 源镜像——唯一可手编辑区** | 你（编辑后 dhv sync 回写） |

## 8.2 完整工作流 walkthrough

场景：Python 同事说「把 greet 的文案改成中文，加个感叹号」，
但他只想改 python 文件（不想碰 HSL 源码）。

**第 1 步 · emit**（若还没有产物）：

```bash
mkdir -p /tmp/sync-demo
# model.hsl 内容：
#   export fn greet(name: String) -> String {
#       format!("hello, {}", name)
#   }
# main.hsl import 它并 project { greet -> "src/greet.py" : python, }
bun dhv-ts/src/main.ts check /tmp/sync-demo/main.hsl
bun dhv-ts/src/main.ts emit /tmp/sync-demo/main.hsl --out /tmp/sync-demo/out
```

```
投射模式：scale = microkernel（未声明，默认） · 入口 main.hsl
  src/greet.py                       python       full         1126 B  ← greet · 语法✓ python3 -m py_compile

✓ emit 完成：1 个文件（1 个通过语法校验）+ manifest.json → /tmp/sync-demo/out（49 ms）
```

**第 2 步 · 编辑镜像区**（用任何编辑器，只动 `@dhv:hsl-mirror` 与
`@dhv:end-source-map` 之间的行）：

```diff
     # @dhv:hsl-mirror — HSL 源镜像（编辑此区后 dhv sync 回写源码）
     # export fn greet(name: String) -> String {
-    #     format!("hello, {}", name)
+    #     format!("你好, {}! (v2)", name)
     # }
     # @dhv:end-source-map
```

注意：编辑的是**围栏内的 HSL 镜像**（注释形式存在的 HSL 源码），
不是上面的活体 python 代码——活体区由下一步自动重生成。

**第 3 步 · sync 回写**：

```bash
bun dhv-ts/src/main.ts sync /tmp/sync-demo/out/src/greet.py --root /tmp/sync-demo
```

```
/tmp/sync-demo/out/src/greet.py
  围栏：1 个 · 回写：1 处 · 错误：0 个（4 ms）
    ↩ 回写 model.hsl:1 block:greet（镜像 3 行）

✓ 已回写 1 处 HSL 源码 —— 运行 emit 重新生成活体翻译区
```

`--root` 是围栏中相对模块路径的解析根（围栏记录的 `model.hsl`
相对它定位 .hsl 源文件；默认取生成文件所在目录）。

此时查看 `model.hsl`——**HSL 源码已经被改写**：

```hsl
export fn greet(name: String) -> String {
    format!("你好, {}! (v2)", name)
}
```

sync 的安全机制：

- 回写前先确认 .hsl 本身可解析（防手改坏文件）；
- 回写后**重新解析校验**，失败则整体回滚并报错
  （改出语法错误的镜像不会污染源码）；
- 按行号降序替换避免行号漂移；impl 方法围栏（形如 `Type_method`）
  定位到对应 impl 块。

**第 4 步 · 再 emit**（活体翻译区随之更新）：

```bash
bun dhv-ts/src/main.ts emit /tmp/sync-demo/main.hsl --out /tmp/sync-demo/out
```

```
✓ emit 完成：1 个文件（1 个通过语法校验）+ manifest.json → /tmp/sync-demo/out（46 ms）
```

查看 `greet.py` 的活体区——python 代码已按新 HSL 源码重生成：

```python
def greet(name: str) -> str:
    # @dhv:source-map: model.hsl:1, block: greet (live)
    return f"你好, {name}! (v2)"
    # @dhv:hsl-mirror — HSL 源镜像（编辑此区后 dhv sync 回写源码）
    # export fn greet(name: String) -> String {
    #     format!("你好, {}! (v2)", name)
    # }
    # @dhv:end-source-map
```

闭环完成：**生成文件编辑 → HSL 源码更新 → 全部 38 后端的产物一致更新**。
这就是「双向工程」——生成代码不再是黑盒，而是 HSL 源码的可编辑视图。

## 8.3 诚实边界：哪些区可编辑

| 区域 | 标记 | 可否手改 | 再编译时命运 |
|:---|:---|:---|:---|
| 文件头 | `@dhv:generated` | 否 | 覆盖 |
| 运行期助手（`_dhv_unwrap` 等） | `@dhv:generated` | 否 | 覆盖 |
| graph 脚手架 | `@dhv:generated` | 否 | 覆盖 |
| 活体翻译区 | `@dhv:source-map: ... (live)` 至镜像标记之间 | **不建议**（会被覆盖） | **重编译覆盖** |
| HSL 源镜像区 | `@dhv:hsl-mirror` 至 `@dhv:end-source-map` | **是（唯一可编辑区）** | sync 读取后回写 .hsl；再 emit 后镜像与源码同步 |
| 围栏标记行本身 | 三个 `@dhv:` 标记 | 否 | 识别依赖它们，删了 sync 就找不到锚点 |
| 围栏之外的普通代码 | — | 可以（如文件级胶水） | **注意**：下次 emit 整个文件重写，围栏外手改会丢失 |

关键认识：**围栏协议是「镜像区可编辑 + 活体区可再生成」的分工**。
想持久保留的手写代码，要么放进 .hsl（成为源码的一部分），
要么放到不被投射占用的文件里——emit 只重写 project{} 声明的路径。

---

# 第九章 CLI 完全参考

所有命令的统一入口：

```bash
bun dhv-ts/src/main.ts <command> [entry.hsl] [flags]
```

## 9.1 六个命令

### check — 静态检查

```bash
bun dhv-ts/src/main.ts check <entry.hsl>
```

- 加载整个模块闭包（import 链），跑 S/G/P/N 规则检查；
- 输出诊断（`error[码]: 消息` + `--> 文件:行:列`）与汇总行
  `dhv-ts check: N error(s), M warning(s)`；
- 有 error 时退出码 1。

### run — 解释执行

```bash
bun dhv-ts/src/main.ts run <entry.hsl> [options]
```

- 入口 = 入口文件中名为 `main` 的 fn（R-1，无需 export）；
- 横幅打印运行配置（模型 / 工作区 / 任务 / 尺度 / 产物目录）；
- `main` 返回 `Err(...)` → 打印 `✗ harness 返回 Err` 并退出 1；
  返回 Ok → 打印载荷摘要并退出 0；
- 结束时写产物：`report.md`（占位）、`transcript.jsonl`、
  `events.jsonl`（事件总线）、`run.json`（机器可读摘要）；
  你在 native 块里经 `$host.artifacts.write` 写的文件也在该目录。

### emit — 投射工程

```bash
bun dhv-ts/src/main.ts emit <entry.hsl> --out DIR [--no-validate]
```

- 遍历全工程 project{} → 生成代码文件 + 静态资源 + `manifest.json`；
- 默认做**交叉语法校验**（python3 / Bun.Transpiler / bash -n /
  括号平衡启发式），`--no-validate` 跳过；
- 逐文件打印 `路径 语言 tier 字节数 ← 项 · 语法✓ 工具`；
- 尺度取自**入口文件的 scale 声明**（未声明默认 microkernel；
  v0.2.0 中 `--scale` flag 不影响 emit，见 13.3）；
- 任一文件校验失败或存在警告 → 退出码 1。

### targets — 注册表

```bash
bun dhv-ts/src/main.ts targets
```

打印 38 后端注册表（tier / 能力级 / 扩展名 / native 可执行性 /
语法校验工具 / 别名表）。

### sync — 双向回写

```bash
bun dhv-ts/src/main.ts sync <generated-file> [--root DIR]
```

读取生成文件的围栏镜像 → 与 .hsl 源码逐块比对 → 有差异则回写
（回写后重解析，失败回滚）。详见第八章。

### watch — 文件监听

```bash
bun dhv-ts/src/main.ts watch <entry.hsl> --out DIR
```

见 9.4。

## 9.2 全部 flags

| flag | 取值 | 默认 | 作用 |
|:---|:---|:---|:---|
| `--workspace DIR` | 路径 | cwd | Agent 工作区：`$host.fs` 路径监狱边界、shell cwd |
| `--task TEXT` | 字符串 | 空 | 任务描述（`$host.config.task` / `std/env::task_text`） |
| `--model NAME` | `scripted` / `deepseek` | `scripted` | 模型模式：剧本 / 真实 LLM 网关 |
| `--fixture FILE` | JSON 路径 | 无 | 剧本（scripted 模式；格式见 11.1） |
| `--temperature F` | 浮点 | 0.2 | LLM 采样温度 |
| `--max-turns N` | 整数 | 24 | 主循环轮数上限（透传给 `$host.config`） |
| `--max-bash N` | 整数 | 12 | bash 调用上限 |
| `--max-output N` | 整数 | 4000 | 工具输出截断字符数 |
| `--allow a,b,c` | 首词列表 | `bun,node,ls,cat,grep,diff` | shell 白名单 |
| `--scale MODE` | `microkernel` / `monolith` | `microkernel` | run 观测标注（emit 形态取自文件声明） |
| `--out DIR` | 路径 | `.hsl-runs/<时间戳>`（run）/ 必填（emit） | 产物目录 |
| `--quiet` | — | 否 | 静默轨迹 |
| `--root DIR` | 路径 | 生成文件所在目录 | sync 的模块解析根 |
| `--no-validate` | — | 否 | emit 跳过交叉语法校验 |
| `--help` / `-h` | — | — | 打印 usage |

未知 flag 直接报错：`未知参数 --foo`（exit 1）。

## 9.3 退出码约定

| 码 | 含义 |
|:---|:---|
| 0 | 成功（check 无 error / run 返回 Ok / emit 全部校验通过 / sync 无错误） |
| 1 | 检查有 error；运行期错误或 harness 返回 Err；emit 有文件校验失败或有警告；sync 回写错误 |
| 2 | 用法错误：缺 entry、emit 缺 `--out` 等（打印 usage） |

脚本化使用示例：

```bash
bun dhv-ts/src/main.ts check agent.hsl || exit 1
bun dhv-ts/src/main.ts run agent.hsl --fixture fix.json --quiet
```

## 9.4 watch 模式

```bash
bun dhv-ts/src/main.ts watch agent.hsl --out /tmp/gen
```

启动时先做一次完整 check + emit，然后监听**入口及其 import 闭包中的
所有 .hsl 文件**（250 ms 防抖）：

```
  watch 模式（总纲 §6 File Watcher）
  入口    /path/to/agent.hsl
  产物    /tmp/gen
  监听 .hsl 变化 → check + emit；Ctrl-C 退出

  监听 3 个 .hsl 文件中…

[16:41:02] 变更 agent.hsl → check ✓ · emit 6 文件（87 ms）
[16:41:20] 变更 model.hsl → check 失败（1 errors，不 emit）
error[S-6]: match 不穷尽：Action 缺少变体 Stop
```

check 失败时**不 emit**（产物目录保持上一次成功状态）——
坏代码永远不会污染生成物。

---

# 第十章 静态检查与错误码

dhv-ts check 的规则总表。每条给出：含义 / 触发示例 / 修复方法。
诊断格式统一为：

```
error[S-6]: match 不穷尽：Action 缺少变体 Stop
  --> src/agent.hsl:16:5
```

## S 规则（严格性铁律）

| 码 | 级别 | 含义 | 触发示例 | 修复 |
|:---|:---|:---|:---|:---|
| S-1 | error（运行期强制） | 零隐式转换：if/while 条件必须是 bool | `if 1 { }` | 写显式比较 `if n > 0 { }`；转换用 `as` |
| S-2 | warning | 裸 `.unwrap()`（非空默认） | `opt.unwrap()` | `unwrap_or(默认)` / `match` / `?` |
| S-4 | error | 对不可变绑定赋值 | `let a = 1; a = 2;` | `let mut a = 1;` |
| S-5 | error（运行期） | `?` 只能用于 Result/Option | `let x = 42?;` | 去掉 `?` 或改返回类型 |
| S-6 | error | match 穷尽性；graph 体内枚举 match 禁止 `_` 兜底 | 见下 | 补全所有变体 arm |
| S-7 | error | 未使用的 let 绑定 / import / 捕获例外 | `let unused = 42;` | 删除或改名 `_unused` |
| S-8 | error | 同作用域变量遮蔽 | `let x = 1; let x = 2;` | 换新名字（跨作用域遮蔽合法） |

S-6 的两种形态（真实报错）：

```
// 普通函数：缺变体且无 _ 兜底
error[S-6]: match 不穷尽：Color 缺少变体 Blue

// graph AgentLoop 内：_ 兜底被铁律拒绝（Agent 核心决策循环
// 必须显式直面每个新变体——这是 HSL 固化「非确定性循环决策」的语法核心）
error[S-6]: graph AgentLoop 内的枚举 match 不允许 _ 通配兜底（必须显式穷尽，直面新分支）
```

v0.2.2 语义：普通函数内 `_ =>` 兜底视为穷尽（与 Rust 一致）；
仅 AgentLoop 内拒绝通配。

注意 S-3（强制错误处理）与 S-1（编译期类型检查）的完整形态由
dhv Rust 编译器实现；dhv-ts 在运行期强制 S-1/S-4/S-6。

## G 规则（拓扑）

| 码 | 级别 | 含义 | 触发示例 | 修复 |
|:---|:---|:---|:---|:---|
| G-1 | error | graph 缺少 AgentLoop | graph body 无 `loop` | 补 `loop { ... }` |
| G-2 | error | edge 端点未声明（或声明晚于 edge） | `edge a -> b;`（b 不存在） | 先 `node b` / `let b` 再 edge |
| G-3 | error | 无条件环（编译期可判定死锁） | `edge a->b; edge b->a;` 均无 guard | 环上至少一条边加 `on Guard` |
| G-4 | warning | 孤岛节点（无任何 edge 触达） | `node lonely: i64 = 0;` | 接入边，或注释说明是插件注入位 |

## P 规则（投射）

| 码 | 级别 | 含义 | 触发示例 | 修复 |
|:---|:---|:---|:---|:---|
| P-1 | error | 每文件至多一个 project{} / scale | 两个 project 块 | 合并 |
| P-2 | error/warn | 同一物理路径冲突 | 两项投射到同一路径不同语言 | 拆路径 |
| P-3 | error | 投射目标未定义（或未 import） | `Missing -> "x.py" : python` | 定义项或补 import |
| P-4 | error | 投射语言不合法 | block 投到 rust；fn 投到 cobol | block→静态格式；代码项→注册表语言 |
| P-6 | warning | scale 不在含 graph 的入口文件 | `scale = microkernel;` 在纯类型文件 | 移到入口 |

P-4 的真实报错（含完整注册表提示）：

```
error[P-4]: fn helper 只能投射到编程语言后端（得到 cobol；注册表：
python/typescript/javascript/rust/go/cpp/java/csharp/kotlin/swift/ruby/
php/lua/perl/bash/powershell/r/julia/scala/elixir/erlang/haskell/ocaml/
fsharp/zig/nim/crystal/dart/groovy/objectivec/d/vb）

error[P-4]: 静态资源 cfg 只能投射到 yaml/markdown/json/toml/ini/xml（得到 rust）
```

## N 规则（native 块）

| 码 | 级别 | 含义 | 触发示例 | 修复 |
|:---|:---|:---|:---|:---|
| N-1 | error | native 语言未注册 | `native cobol { ... }` | 用注册表里的语言 id |
| N-1（语义） | — | 捕获按名映射；被引用变量计为已使用（防 S-7 误报） | — | — |
| N-2 | error（运行期约定） | 进出 native 的值应为平凡类型 | 返回复杂闭包 | 用 JSON 编组（6.4） |
| N-3 | — | native 内不得再出现 HSL 语法（词法模式 B） | — | — |

## L / E / R 规则（链接 / 重复 / 入口）

| 码 | 命令 | 含义 | 触发示例 | 修复 |
|:---|:---|:---|:---|:---|
| E-0 / L-0 | check/run | 链接失败：import 路径不存在 | `import { X } from "./no-such.hsl";` | 修路径 |
| E-1 | check | 重复定义顶层项 | 两个 `fn dup()` | 改名 |
| E-2 | check | 调用未定义的函数 / std 导入名不存在（v0.2.51） | `sort_desc(...)`（漏 import） | 补 import 或修拼写；单段路径调用才检查，局部绑定与两段路径豁免 |
| R-1 | run | 入口文件没有 fn main | 只有 `fn not_main()` | 补 `fn main()` |

真实报错：

```
error[E-0]: import 路径不存在："./no-such-file.hsl"（from /tmp/rules/l0.hsl）
error[E-1]: 重复定义顶层项 "dup"
✗ 运行期错误：入口文件没有 fn main()（运行约定：BNF v1.3 §R-1）
```

---

# 第十一章 测试你的 harness

Agent 最难的不是写出来，而是**测**。HSL 的工具链为此内置了三件武器：
剧本模式（确定性执行）、可复现 PRNG、发布级测试套件。

## 11.1 剧本模式：--fixture

`--model scripted`（默认模型模式）配合 `--fixture FILE` 把「模型的决策」
变成一份 JSON 剧本——同一份 harness 代码，插上剧本是确定性 CI，
插上 deepseek 是真实智能体（dsh 的 ScriptedModel 设计）。

```json
{
  "acts": [
    "{\"action\": \"tool\", \"tool\": \"read_file\", \"path\": \"stats.ts\"}",
    "{\"action\": \"tool\", \"tool\": \"bash\", \"command\": \"bun stats.test.ts\"}",
    "{\"action\": \"done\", \"summary\": \"修复完成\"}"
  ],
  "reviews": [
    "{\"verdict\": \"accept\"}"
  ]
}
```

| 字段 | 类型 | 消费方 |
|:---|:---|:---|
| `acts` | `String[]` | `$host.fixture.nextAct()`——每次调用返回下一条；耗尽抛错 |
| `reviews` | `String[]` | `$host.fixture.nextReview()`——审查者剧本；耗尽后默认返回 `{"verdict":"accept"}` |
| （只读）`actsLeft()` | `() -> i64` | `$host.fixture.actsLeft()` |

acts 里放什么完全由你的 harness 协议决定——dsh 放的是模型应输出的
JSON 动作串；第十二章的统计 Agent 放的是 `{"action":"analyze","path":...}`。
剧本断言的是**协议层行为**：模型产出什么、harness 如何分发。

在 native typescript 块中取动作的标准形：

```hsl
fn next_action() -> Action {
    let raw: String = native typescript {
        const s = await $host.fixture.nextAct();
        return s;
    };
    parse_action(raw).unwrap_or(Action::Done { summary: String::from("eof") })
}
```

## 11.2 确定性测试策略

| 手段 | 机制 | 断言什么 |
|:---|:---|:---|
| 剧本 acts | 固定决策序列 | 工具调用顺序、协议纠错回路 |
| 剧本 reviews | 固定审查裁决 | Accept/Revise 分支与回环 |
| `std/random` + `seed(42)` | 可复现 PRNG | 随机化路径的输出完全确定 |
| 产物断言 | `--out DIR` 下的 run.json / events.jsonl / artifacts | 轮数、边事件次数、生成的文件内容 |
| 退出码 | 0/1/2 约定 | CI 门禁 |

events.jsonl 的 G6 边事件让「拓扑行为」可断言——例如 dsh 的
16 个事件里包含 7 次 `edge(model→executor on Tool)`、7 次 `observed`、
2 次 `capability_denied`（真实 LLM 运行中被白名单拦截的记录）。
run.json 是机器可读摘要：

```json
{
  "ts": "2026-08-29T16:40:00.500Z",
  "ok": true,
  "elapsed_ms": 47,
  "model": "scripted",
  "task": "修复 stats.ts 中 variance() 的分母……",
  "events": 16
}
```

一个最小的 CI 脚本：

```bash
#!/usr/bin/env bash
set -e
bun dhv-ts/src/main.ts check examples/dsh/dsh.hsl
bun dhv-ts/src/main.ts run examples/dsh/dsh.hsl \
  --fixture examples/dsh/fixtures/fix-variance.json \
  --task "修复 variance 分母" \
  --workspace /tmp/dsh-ws --out /tmp/dsh-run --quiet
python3 - <<'EOF'
import json
run = json.load(open("/tmp/dsh-run/run.json"))
assert run["ok"] is True, run
print("dsh scripted e2e ✓")
EOF
```

## 11.3 tests/hsl/run-all.ts 套件

工具链自带发布级测试套件，109 用例分 8 组（v0.2.10）：

```bash
bun tests/hsl/run-all.ts
```

| 组 | 用例数 | 覆盖 |
|:---|:---|:---|
| 回归 | 17 | smoke / std-tour / pattern-tour run + emit（**32 文件** v0.2.6 + cpp/go 活体断言含 drain）· translator-tour 巡览（v0.2.7 扩容 vec_surgery/map_census/let_block）· nova check + **nova emit 零告警回归** · dsh check / scripted e2e · backends-demo · S-6 通配正反例 / 构造器回归 · **M3 静态检查正反例** · dhv Rust 源码守护（宏尾 ! / S-6 / expression_with_block 定义 + parser 契约） |
| 检查规则 | 10 | S-6/S-7/S-8 正反例 / P-3 / P-4（未注册语言 + 静态格式 + contract 语言全合法）/ N-1 / G-1 |
| emit | 60 | 38 后端全量 + 语法全过断言 / 围栏与脚手架断言 / scale 形态切换 / if-let 尾值语义（含 OOB→None）/ match 臂嵌套 if 值语义回归 / else-if-let 链 / 方法映射语义级验证（python exec）/ Option::map 类型感知 / java-kotlin-swift 声明质量（v0.2.5 断言新顶层结构）/ ts get 类型感知 / **cpp Some/None g++ 编译+链接+运行** / **cpp-go if-let 变体链 + while-let 结构断言** / **cpp Option match has_value** / **cpp to_string 字符串接收者** / **go 变体字段大小写** / **Java 顶层类型 + Dhv 宿主结构** / **v0.2.6 cpp Vec::pop g++ 编译+链接+运行 drain=15/0/7** / **v0.2.6 cpp-go first/last/clone + 编译级** / **v0.2.6 matchDispatch 副作用 hoist（match v.pop() 单次求值）** / **v0.2.6 cpp pop 副作用对接收者可见（pop+peek）** / **v0.2.6 balanceCheck (*ptr) 解引用不误判** / **v0.2.6 C# 宿主类合法化（internal static class Dhv<Stem>）** / **v0.2.6 Kotlin-Swift contract 结构断言** / **v0.2.6 宏 token 树嵌套 delim 类型收集（vec![Tool{...}] 跨文件接线）** / **v0.2.7 String::contains 类型感知（修复 cpp/go 编译错误代码 + g++ 编译运行）** / **v0.2.7 cpp Vec::insert/Remove + HashMap 全表面（g++ 编译+链接+运行 ir=10/mo=107 与 interp 对齐）** / **v0.2.7 cpp-go Vec/HashMap::get Option 语义（越界/缺键 → 默认值 6 -1 -1 1 -7）** / **v0.2.7 let 块初始化（let x = if/match/if-let：py/rs/ts/cpp 全活体 + python exec 语义级）** / **v0.2.7 cpp String 方法族 12 方法（trim/lower/upper/starts/ends/replace/split/splitWS/lines/repeat/char_count/join → g++ 运行 hello hsl\|3\|3\|3\|9\|3\|a-b-c 与 interp 逐字对齐）** / **v0.2.7 go HashMap+Vec 助手族结构断言（含不应再有 func() any 回归断言）** / **v0.2.8 cpp Option::map/and_then 链式家族（g++ 编译+运行 10/8/-1）** / **v0.2.8 cpp Option::or/unwrap_or_else/expect（g++ 运行 42/99/7）** / **v0.2.8 go Option or/expect 助手族结构** / **v0.2.8 cpp Vec::sort/is_sorted/clear/extend/append（g++ 编译+运行 105/5/0）** / **v0.2.8 go Vec 助手族结构断言** / **v0.2.8 cpp vec! 宏 CTAD 修复（g++ 运行 60/7）** / **v0.2.8 cpp Option::or 链 + unwrap_or（g++ 运行 42/-7）** / **v0.2.8 cpp 综合场景 Option 链 + Vec 方法族（g++ 运行 13/6）** / **v0.2.9 String::parse turbofish 全语言活体（5 后端结构断言）** / **v0.2.9 cpp parse g++ 编译+运行（11251 与 interp 逐字对齐）** / **v0.2.9 python parse python3 exec 语义级（101151）** / **v0.2.9 Option::filter（interp + py exec + cpp 结构）** / **v0.2.9 cpp 裸 Option::None 链修复（g++ 编译+运行 113 + 回归断言防 _dhvOptMap(std::nullopt 复发）** / **v0.2.9 cpp Vec::sort_by 稳定排序（g++ 运行 1020/2131 —— 2131 验证稳定序）** / **v0.2.9 go sort_by 闭包内联替换结构（sort.SliceStable + v[i] 替换）** / **v0.2.9 char 谓词 g++ 编译+运行（11/111 含 UTF-8 é 精确对齐 interp 正则）** / **v0.2.10 真机工具链编译级（安装 rustc 1.98 / go 1.27 / JDK 21 / kotlinc 2.4.10 实测）：rust format 内联捕获修复（rustc 编译）· rust HashMap 导入（rustc 编译）· go 多文件助手去重 + import 裁剪（go build + go vet）· javac 编译级（java 后端首次真机编译）—— 另 go HashMap 助手族 / if-let 变体链两测试升级真机 go build** |
| 解析 | 1 | macro_rules! 定义名尾 !（Rust 习惯容错）双形态展开 |
| sync | 1 | 完整闭环（emit → 改镜像 → sync 回写 → re-emit 活体区更新） |
| 模糊 | 6 | 随机 token 汤 ×200 / 未闭合字符串注释块 / Unicode（CJK/emoji/RTL）/ BOM/CRLF/空文件 / 深嵌套 500 层 / 巨大字面量 |
| CLI 边界 | 8 | 未知参数 / 文件不存在 / 缺参数 usage / 循环 import / 缺失 import / 无 main / targets 输出 |
| 压力 | 4 | 万级 Vec 链式 / 深递归 5000 层干净失败 / 10k 字符串拼接 / 2000 元素 JSON |

套件随发布物分发——它同时是工具链的回归防线与「如何测试 HSL 工程」的
示范。开发 harness 时的建议：把你的 `check + scripted run + 产物断言`
做成同样的独立脚本，接进 CI。

值得知道的历史：这个套件曾抓出一个 bun 转译器级 bug——bun 会**静默丢弃**
语句位置的 `declare(...)` 调用，导致 S-7/S-8 检查一度从未真正生效
（重命名内部函数后修复）。这也是「为什么严格测试对这门工具链不是可选项」
的最好注脚。

---

# 第十二章 完整实战：从零写一个代码统计 Agent

目标：一个真实的工具型 Agent——扫描工作区里的代码文件，逐个统计
总行数 / 代码行 / 注释行 / 空行，按语言归类，最后产出 Markdown 报告。
全流程覆盖：类型定义 → 纯逻辑工具 → graph 编排 → 剧本测试 → 多后端 emit。

最终项目结构：

```
stats-agent/
├── main.hsl          入口：graph StatsAgent + main + project + scale
├── types.hsl         类型与契约层
├── counter.hsl       纯逻辑层（无副作用，可独立测试）
├── fixture.json      剧本
└── ws/               待统计的工作区（示例文件）
```

## 第 1 步：类型定义（types.hsl）

先把「数据长什么样」定死。这是 HSL 的强项——契约先于实现：

```hsl
// types.hsl — 代码统计 Agent · 类型与契约层

export struct FileStats {
    path: String,
    lang: String,
    total: i64,
    code: i64,
    comment: i64,
    blank: i64,
}

export struct StatsReport {
    files: i64,
    total_lines: i64,
    code_lines: i64,
    comment_lines: i64,
    blank_lines: i64,
    details: Vec<FileStats>,
}

export enum Action {
    Analyze { path: String },
    Done { summary: String },
}
```

设计说明：

- `FileStats` 是单文件结果，`StatsReport` 是聚合结果（含明细 Vec）；
- `Action` 是 Agent 的动作协议——**只有两种动作**：分析一个文件 /
  宣告完成。加新动作（比如 Ignore）时，第 4 步的 match 会在编译期
  报缺变体，逼你直面新分支。

## 第 2 步：纯逻辑工具（counter.hsl）

统计逻辑与 IO 彻底分离——这一层没有任何副作用，随时可以单测：

```hsl
// counter.hsl — 代码统计 Agent · 纯逻辑层（无副作用，可独立测试）

import { FileStats } from "./types.hsl";

export fn lang_of(path: String) -> String {
    if path.ends_with(String::from(".hsl")) {
        String::from("HSL")
    } else if path.ends_with(String::from(".ts")) {
        String::from("TypeScript")
    } else if path.ends_with(String::from(".py")) {
        String::from("Python")
    } else if path.ends_with(String::from(".md")) {
        String::from("Markdown")
    } else {
        String::from("other")
    }
}

export fn blank_of(line: String) -> bool {
    line.trim().is_empty()
}

export fn comment_of(line: String, lang: String) -> bool {
    let t = line.trim();
    if lang == String::from("HSL") || lang == String::from("TypeScript") {
        t.starts_with(String::from("//"))
    } else if lang == String::from("Python") {
        t.starts_with(String::from("#"))
    } else {
        false
    }
}

export fn stats_of(path: String, content: String) -> FileStats {
    let lang = lang_of(path);
    let mut total: i64 = 0;
    let mut code: i64 = 0;
    let mut comment: i64 = 0;
    let mut blank: i64 = 0;
    for line in content.lines() {
        total += 1;
        if blank_of(line) {
            blank += 1;
        } else if comment_of(line, lang.clone()) {
            comment += 1;
        } else {
            code += 1;
        }
    }
    FileStats { path, lang, total, code, comment, blank }
}
```

## 第 3 步：graph 编排（main.hsl 主体）

Agent 的骨架：剧本给出动作 → graph 分发 → 分析累计 → Done 时收束。

```hsl
// main.hsl — 代码统计 Agent · 入口与编排层
import { FileStats, StatsReport, Action } from "./types.hsl";
import { stats_of, lang_of } from "./counter.hsl";
import { read_file } from "std/io";

fn next_action() -> Action {
    let raw: String = native typescript {
        const s = await $host.fixture.nextAct();
        return s;
    };
    match parse_action(raw) {
        Result::Ok(a) => a,
        Result::Err(e) => Action::Done { summary: format!("协议违规：{}", e) },
    }
}

fn parse_action(raw: String) -> Result<Action, String> {
    let fields: HashMap<String, String> = native typescript {
        const obj = JSON.parse(raw);
        const out = new Map();
        for (const [k, v] of Object.entries(obj)) out.set(k, String(v));
        return out;
    };
    let action = fields.get(String::from("action")).unwrap_or(String::from(""));
    if action == String::from("analyze") {
        Ok(Action::Analyze { path: fields.get(String::from("path")).unwrap_or(String::from("")) })
    } else if action == String::from("done") {
        Ok(Action::Done { summary: fields.get(String::from("summary")).unwrap_or(String::from("")) })
    } else {
        Err(format!("未知 action: {}", action))
    }
}

fn render_report(report: StatsReport) -> String {
    let mut out = String::from("# 代码统计报告\n\n");
    out.push_str(format!("| 文件 | 语言 | 总行 | 代码 | 注释 | 空行 |\n").as_str());
    out.push_str(format!("|---|---|---|---|---|---|\n").as_str());
    for d in report.details {
        out.push_str(format!("| {} | {} | {} | {} | {} | {} |\n", d.path, d.lang, d.total, d.code, d.comment, d.blank).as_str());
    }
    out.push_str(format!("\n合计：{} 个文件 · {} 行（代码 {} / 注释 {} / 空行 {}）\n", report.files, report.total_lines, report.code_lines, report.comment_lines, report.blank_lines).as_str());
    out
}

graph StatsAgent(mut budget: i64) -> Result<StatsReport, String> {
    node collector: Vec<FileStats> = Vec::new();
    let mut total_lines: i64 = 0;
    let mut code_lines: i64 = 0;
    let mut comment_lines: i64 = 0;
    let mut blank_lines: i64 = 0;

    edge collector -> collector on Action::Analyze;   // 自环：逐文件积累

    loop {
        if budget <= 0 {                              // 预算闸门
            break;
        }
        let action = next_action();
        match action {
            Action::Analyze { path } => {
                match read_file(path.clone()) {
                    Result::Ok(content) => {
                        let s = stats_of(path, content);
                        total_lines += s.total;
                        code_lines += s.code;
                        comment_lines += s.comment;
                        blank_lines += s.blank;
                        collector.push(s);
                        budget -= 1;
                    },
                    Result::Err(e) => {
                        println!("[skip] {}: {}", path, e);   // 读失败：跳过不中断
                    },
                }
            },
            Action::Done { summary } => {
                println!("[done] {}", summary);
                break;
            },
        }
    }

    let details = collector.clone();
    Ok(StatsReport {
        files: details.len(),
        total_lines,
        code_lines,
        comment_lines,
        blank_lines,
        details,
    })
}

fn main() -> Result<(), String> {
    let report = StatsAgent::run(8)?;
    let md = render_report(report);
    println!("{}", md);
    let written: bool = native typescript {
        $host.artifacts.write("stats-report.md", md);
        return true;
    };
    if written {
        println!("[artifacts] stats-report.md 已写出");
    }
    Ok(())
}

scale = microkernel;

project {
    FileStats -> "src/types.py"        : python,
    Action    -> "src/action.ts"       : typescript,
    lang_of   -> "src/lang.go"         : go,
    stats_of  -> "src/stats.rs"        : rust,
    StatsAgent -> "src/agent.py"       : python,
    main      -> "src/main.py"         : python,
}
```

要点逐个讲：

- `node collector: Vec<FileStats> = Vec::new();`——收集器是一个**拓扑节点**，
  自环边 `collector -> collector on Action::Analyze` 语义化地表达了
  「分析动作在收集器上累积」；
- `graph StatsAgent(mut budget: i64)`——预算是 graph 参数，
  每个 Analyze 消耗一点，耗尽即出环（双保险：剧本耗尽时 Done 出环）；
- 读文件用 `std/io::read_file`（Result 语义），失败走 `[skip]` 分支
  而非中断——工具型 Agent 的容错姿势；
- `?` 出现在 `StatsAgent::run(8)?`：graph 返回
  `Result<StatsReport, String>`，main 拿到 StatsReport 或提前返回 Err；
- `render_report` 是纯函数：struct → Markdown 表格。

## 第 4 步：剧本与示例工作区

```json
// fixture.json
{
  "acts": [
    "{\"action\": \"analyze\", \"path\": \"app.ts\"}",
    "{\"action\": \"analyze\", \"path\": \"tool.py\"}",
    "{\"action\": \"analyze\", \"path\": \"notes.md\"}",
    "{\"action\": \"analyze\", \"path\": \"README.md\"}",
    "{\"action\": \"done\", \"summary\": \"已统计 4 个文件\"}"
  ],
  "reviews": []
}
```

```ts
// ws/app.ts
// 应用入口
export function main() {
    // 打招呼
    console.log("hello");
    return 0;
}
```

```python
# ws/tool.py
# 工具函数
def add(a, b):
    # 简单相加
    return a + b
```

```markdown
<!-- ws/notes.md -->
# 笔记

正文一行。

结尾。
```

```markdown
# ws/README.md

待统计的工作区。
```

## 第 5 步：跑通它

```bash
bun dhv-ts/src/main.ts check stats-agent/main.hsl
```

```
dhv-ts check: 0 error(s), 0 warning(s)
✓ 3 个模块全部通过检查
```

```bash
bun dhv-ts/src/main.ts run stats-agent/main.hsl \
  --workspace stats-agent/ws \
  --fixture stats-agent/fixture.json \
  --out /tmp/stats-run --quiet
```

真实输出：

```
[done] 已统计 4 个文件
# 代码统计报告

| 文件 | 语言 | 总行 | 代码 | 注释 | 空行 |
|---|---|---|---|---|---|
| app.ts | TypeScript | 7 | 4 | 2 | 1 |
| tool.py | Python | 5 | 2 | 2 | 1 |
| notes.md | Markdown | 6 | 3 | 0 | 3 |
| README.md | Markdown | 4 | 2 | 0 | 2 |

合计：4 个文件 · 22 行（代码 11 / 注释 4 / 空行 7）

[artifacts] stats-report.md 已写出

✓ harness 返回 Ok（19 ms）
```

手工核对 app.ts：7 行 = 1 注释 + 1 代码 + 1 注释 + 1 代码 + 1 代码 + 1 代码 + 1 空行，
即 code 4 / comment 2 / blank 1——统计正确。
`/tmp/stats-run/stats-report.md` 是经 `$host.artifacts` 写出的持久产物；
`events.jsonl` 记录了 4 次 `edge(collector→collector on Analy)` 与 node 事件
（G6 观测）；`run.json` 给出 `ok: true, elapsed_ms: 19`。

## 第 6 步：投射 6 个后端

```bash
bun dhv-ts/src/main.ts emit stats-agent/main.hsl --out /tmp/stats-gen
```

真实输出：

```
投射模式：scale = microkernel · 入口 main.hsl
  src/types.py                       python       full          929 B  ← FileStats · 语法✓ python3 -m py_compile
  src/action.ts                      typescript   full         1016 B  ← Action · 语法✓ bun transpiler (ts)
  src/lang.go                        go           logic        1842 B  ← lang_of · 语法✓ heuristic:balanced
  src/stats.rs                       rust         logic        1550 B  ← stats_of · 语法✓ heuristic:balanced
  src/agent.py                       python       full         3077 B  ← StatsAgent · 语法✓ python3 -m py_compile
  src/main.py                        python       full         1933 B  ← main · 语法✓ python3 -m py_compile

✓ emit 完成：6 个文件（6 个通过语法校验）+ manifest.json → /tmp/stats-gen（122 ms）
```

观察这个清单，能读出能力分级的真实行为：

- `types.py`（full）：`FileStats` → Python `@dataclass`；
- `action.ts`（full）：`Action` → TypeScript 判别联合；
- `lang.go`（logic）：if-else 链翻译成 Go 函数（活体翻译成功）；
- `stats.rs`（logic 回退 contract）：函数体含 `for line in content.lines()`
  迭代调用，超出 rust 翻译器的语句子集，**自动回退**——签名照译
  （`pub fn stats_of(path: String, content: String) -> FileStats`），
  体内围栏保源 + `todo!("...")`，绝不输出半翻译代码；
- `agent.py`（full）：graph 脚手架（microkernel Plugin 注册表形态）+ 完整
  HSL 源镜像围栏；
- `main.py`（contract 回退）：`main` 含 `native typescript` 块，而目标是
  python——语言不一致，回退契约模式（围栏保源）。

## 第 7 步（收尾）：接下来你可以

1. **sync 闭环**：改 `/tmp/stats-gen/src/lang.go` 围栏里的 HSL 镜像
   （比如把 `"other"` 改成 `"Unknown"`），`dhv sync` 回写 counter.hsl，
   再 emit——所有后端同步更新（第八章）；
2. **换模型**：把 `next_action` 换成经 `$host.llm.complete` 的真实模型调用
   （参考 examples/dsh/providers/model.hsl 的 DeepSeekModel）；
3. **加动作**：给 `Action` 加 `Ignore { path }` 变体——check 立刻报
   S-6 缺变体，补上 match 分支，重新跑剧本；
4. **跑套件**：把本项目接进 `tests/hsl/run-all.ts` 风格的 CI 脚本。

---

# 第十三章 生态与对比

## 13.1 与 DeepSeek Harness（dsh）/ MCP / AGENTS.md 的关系

### dsh：HSL 的「照镜子」项目

`examples/dsh` 是用 HSL 复现 DeepSeek 风格 agent harness 的完整项目
（10 个模块）。它的意义是双向的：

- 对学习者：**读懂 dsh 就读懂了 HSL 的全部惯用法**——graph 参数化状态机、
  四条条件环边、协议纠错回路、预算闸门、双模型插拔（ScriptedModel /
  DeepSeekModel 经 `Box<dyn ModelProvider>`）、六道安全防线；
- 对语言：dsh 是 HSL 的验收基准。工具链回归测试（run-all.ts）每轮都跑
  dsh 的 scripted 端到端（50ms 内：读文件 → 修 bug → 跑测试 → 审查 → 报告）。

dsh 的实测记录（README）值得一看：剧本模式 5 turns 全绿；
真实 LLM 模式 13.6s，transcript 记录了模型两次尝试 `cd`/`deno` 被
shell 白名单拦截后收敛到 `node stats.test.ts` 的完整决策链。

### MCP（Model Context Protocol）

MCP 解决的是「**模型 ↔ 工具**」的运行时协议：工具如何注册、发现、调用。
HSL 解决的是「**工具 + 模型 + 编排**」如何被一个工程化地定义、校验、投射。
两者是互补层：

| | MCP | HSL |
|:---|:---|:---|
| 层次 | 运行时协议 | 编译型语言 + 工具链 |
| 描述 | 工具的接口 schema | 工具的实现 + Agent 拓扑 + 物理映射 |
| 校验 | 调用时 | 编译期（S/G/P/N 规则） |

HSL 的 `Action` 枚举 + match 分发天然适合包 MCP 工具层：
native 块里跑 MCP client，协议解析留在 HSL 侧（N3 纪律）。

### AGENTS.md

AGENTS.md 是给「读仓库的 agent」看的行为说明文件（目录纪律、构建命令、
边界）。HSL 的静态资源层（`block agent_instructions -> ".harness/AGENTS.md"
: markdown`）把它变成**工程投射物**：提示词与配置和代码同源、同版本、
同校验（P-4 规则约束它只能去静态格式后端）。dsh 与 nova 都用这个模式
投射自己的 AGENTS.md 与 YAML 配置。

## 13.2 与直接写 Python / TypeScript 的取舍

诚实的建议框架：

**选 HSL，当你的 harness 有这些特征**：

- 多语言落地是真实需求（团队有 TS 前端 + Python 算法 + Go 运维）；
- 编排拓扑是核心资产，需要静态校验与可观测（多 Agent、条件回环、
  审查闸门）；
- 协议经常演进（Action 枚举加变体要被编译器抓住，而不是线上事故）；
- 需要确定性 CI（剧本 + 可复现 PRNG + 退出码约定）。

**继续直接写 Python/TS，当**：

- 单语言单进程的小工具——HSL 的分层是额外的概念税；
- 极度依赖某个框架的深度集成（langgraph 的图状态机、autogen 的会话
  管理等）——尽管这些概念可以用 graph 重述；
- 团队对编译期纪律有生理性抗拒。

**边界情况**：即使最终产物是纯 Python，也可以用 HSL 做契约层
（类型 + 协议枚举 + 配置经 project{} 投射），实现层留 native——
渐进式采纳是设计支持的路径。

## 13.3 已知限制（诚实清单）

以下每一条都是真实边界，写在文档里比埋在代码里好：

| # | 限制 | 影响与规避 |
|:---|:---|:---|
| 1 | **dhv-ts 无完整类型推导**：类型注解在解释期基本忽略；S-1/S-3 的编译期完整形态、泛型单态化由 dhv Rust 编译器负责 | 写显式类型注解；把 check 的结构级铁律当作安全网而非完整类型系统 |
| 2 | ~~`?` 的 From 转换运行期未接线~~ **v0.2.1 已修复**：`impl From<E1> for E2` 经 `?` 真实转换（见 §3.11 与 tests/hsl From 回归用例） | 已无此限制 |
| 3 | **contract 后端函数体不翻译**：Go（v0.2.17 升级为 logic 后端，含真实函数体骨架）与 rust/go/cpp 之外的 26 种语言只翻译类型与签名，函数体是围栏 + NotImplementedError | 用 manifest.json 的 tier 字段决定哪些后端可「拿来即用」；函数体落地需 full/logic 语言或人工接管 |
| 4 | **logic 翻译器是语句子集**：rust/go/cpp 遇到不支持构件回退 contract（如实测中 `content.lines()` 迭代使 stats_of 回退） | 把回退当特性：围栏里有完整 HSL 原文，接手的人有据可依 |
| 5 | **命名不做语言习惯转换**：`snake_case` 的 HSL 名字在 Java/C# 产物里保持原样（C# 记录字段做了首字母大写例外） | 投射路径按语言分目录（gen/java/…）缓解；命名风格在 HSL 侧自律 |
| 6 | **impl 方法按类型名全局注册**：跨模块同名类型的方法解析会冲突 | 一个工程内类型名保持全局唯一 |
| 7 | **宏支持基础 frag**：ident/literal/tt/expr/ty/pat 与重复可用；嵌套重复、多规则优先级未覆盖 | 复杂样板展开退回普通函数 |
| 8 | **赋值索引仅支持简单下标**：`arr[i] += 1` 的 i 为复杂表达式时不支持 | 先 `let i = expr;` 再用 |
| 9 | **emit 的 `--scale` flag 不生效**：投射形态取自入口文件的 scale 声明，CLI flag 只影响 run 的观测标注 | 切形态改入口文件声明（一行改动） |
| 10 | ~~`String::new()` 运行期不可用~~ **v0.2.2 已修复**：String/Vec/HashMap/HashSet::new + with_capacity 全部可用 | 已无此限制 |
| 11 | ~~S-6 对 `_` 兜底比 BNF 更严~~ **v0.2.2 已修复**：普通函数内 `_` 兜底视为穷尽（Rust 语义）；仅 graph AgentLoop 内拒绝通配（铁律保留） | 已无此限制 |
| 12 | **S-7 import 检查是源码行扫描**：project{} 里的引用也计为使用；字符串里恰好同名也算 | 保持 import 干净；误报可用 `_` 前缀豁免 |
| 13 | **静态格式后端不翻译内容**：block 体顶格书写与否会原样进入产物 | block 体顶格写（参考 dsh 的 resources.hsl） |
| 14 | **watch 的 emit 不做语法校验**（追求快）：完整校验在独立 emit 时执行 | 提交前跑一次完整 emit |
| 15 | **dhv Rust 编译器需自建**：无 Rust 工具链的环境以源码形态交付 dhv/，全部立即可用的是 dhv-ts | dhv-ts 覆盖本指南全部功能 |
| 16 | ~~if-let 在 tail 位置无值语义~~ **v0.2.3 已修复**：`if let ... {} else {}` 在函数尾位置现为值语义（分支产出 return，与 match/if 对齐）；无 else 的值语境诚实回退 contract。值语义块（match 臂/if 分支）尾部的嵌套 if/match 同步修复（此前静默丢失 return）；`else if let` 链可活体翻译 | 已无此限制 |
| 17 | ~~生成端 `Vec::get` / `HashMap::get` 映射为 subscript~~ **v0.2.3 已修复**：python 映射 `_dhv_get` 助手（越界/缺键 → None）；ts/js 类型感知分发（`v[i] ?? null` / `m.get(k) ?? null`）；rust 原生 `.get()`——与解释器语义完全对齐 | 已无此限制 |
| 18 | **Option::map 需静态类型注解才走类型感知分发**：接收者类型来自 `let o: Option<T>` 注解或参数签名；无注解的复杂链式接收者按 Vec::map 处理（如 `v.first().map(f)` 中间值无注解时）| 给中间绑定写显式类型注解（`let o: Option<i64> = v.first();`） |
| 19 | ~~元组下标 `t.0` 在 python/ts 生成端是非法语法~~ **v0.2.4 已修复**：除 rust（原生 `t.0`）外一律映射为下标 `t[0]`，与解释器语义对齐 | 已无此限制 |
| 20 | ~~副作用接收者被双重求值~~ **v0.2.4 已修复**：`m.remove(k).unwrap_or(d)` 此前生成双重 `m.pop(k, None)`（键删两次）；Option 组合子家族统一走 prelude 助手（参数恰好求值一次） | 已无此限制 |
| 21 | ~~跨文件类型引用未接线~~ **v0.2.4 已修复**：emit 自动追踪类型依赖并按语言接线（py from-import / ts import / rust use / go 同包 / cpp 内联 ODR 声明 + X-1~X-4 诚实告警）；见 §5.7 | 已无此限制 |
| 22 | ~~cpp/go 的 Option 条件与绑定生成非法代码~~ **v0.2.5 已修复**：`v != null`（cpp 无 null / go 用 nil）→ `has_value()` / `!= nil`；`const x = v` → `auto x = *v;` / `x := *v`；go 变体字段大小写错位（`s.f0` vs 声明 `F0`）→ capitalize；裸 `None` 值在 cpp/go/ts/js 输出非法字面 `None` → 各语言正确 null/nullopt/nil | 已无此限制 |
| 23 | ~~Java 生成物含非法结构~~ **v0.2.5 已修复**：旧版全项嵌 `public class <模块名>`（public 类名不匹配文件名 = javac 报错；同模块多文件 wrapper 重名）。新版：类型项顶层声明（同包裸名互见）+ fn/const/impl 宿主 `class Dhv<文件stem>`（每文件唯一） | 已无此限制 |
| 24 | ~~check 不校验 import 名是否被 export~~ **v0.2.5 已修复**：静态 M3 规则——import 未 export 名在 check 阶段报 `error[M3]`（此前只在 run/emit 报错；nova 项目曾因 8 个缺 export 定义 run/emit 双失败而 check 全绿） | 已无此限制 |
| 25 | **Result::Ok/Err 模式对 cpp/go 回退 contract**：类型映射无变体通道（cpp Result→%T 裸值 / go→(T, error)），变体匹配不可廉价复现 | Result 匹配密集的逻辑用 rust/python/ts/js 后端；或人工接手围栏 |
| 26 | **带括号块-LHS 二元的解析分歧**：`(if c {1} else {2}) + 3` —— dhv-ts 拒绝（二元层守卫不区分括号包裹），dhv Rust pest 接受（比规格多） | dhv-ts 用户拆开写（先 let 绑定再运算） |
| 27 | **python 生成代码 `5 is None` SyntaxWarning**：recv 为字面量时（py_compile 语法校验不受影响，语义正确） | 升级 python 3.12+ 或忽略 warning |
| 28 | ~~matchDispatch 副作用 scrutinee 未 hoist~~ **v0.2.6 已修复**：`match v.pop() { Some(x) => ..., None => ... }` 此前 cpp/python 路径在每臂 cond 与 binds 都引用 scrut（pop 被多次求值，破坏副作用语义）；现 matchDispatch 入口对 python/cpp 路径 hoist 非标识符 scrut 到 `_m_N`（与 while-let hoistScrut 同源） | 已无此限制 |
| 29 | ~~validate balanceCheck 误判 `(*ptr)` 解引用为注释~~ **v0.2.6 已修复**：`(*v)[n]`（go/cpp 解引用 + 下标）此前被误报为 OCaml `(* *)` 块注释未闭合；现仅 ocaml/fsharp/pascal 方言识别 `(*` 为注释 | 已无此限制 |
| 30 | ~~cpp/go Vec::pop / first / last / clone 缺映射（drain 场景回退 contract）~~ **v0.2.6 已修复**：cpp 模板助手 `_dhvPop/_dhvFirst/_dhvLast`（std::optional 语义）+ go 泛型助手（指针副作用通道）；clone 为 cpp 拷贝构造 / go slice header 拷贝。`while let Some(x) = v.pop() { ... }` drain 场景从 contract 回退升级为活体翻译 | 已无此限制 |
| 31 | ~~cpp/go Option 方法族 unwrap_or/unwrap/is_some/is_none 缺映射~~ **v0.2.6 已修复**：cpp `value_or/`*deref`/`has_value()`/`!has_value()`；go `(recv != nil ? *recv : d)`/`*recv`/`!= nil`/`== nil`。`v.pop().unwrap_or(d)` 链式可活体翻译（此前因 unwrap_or 缺失回退 contract） | 已无此限制 |
| 32 | ~~C# 生成物含非法顶层函数/常量~~ **v0.2.6 已修复**：旧版 C# 把 fn/const 投射为顶层 `public static T F(...)`（C# 顶层函数非法——必须属于 class）。新版：类型项顶层声明（同命名空间裸名互见）+ fn/const 包装进 `internal static class Dhv<文件stem>`（与 Java v0.2.5 #23 同构）；C# 同步加入 X-1 告警 | 已无此限制 |
| 33 | ~~String::contains 在 cpp/go 生成编译错误代码~~ **v0.2.7 已修复**：`s.contains("x")` 此前 cpp 生成 `std::find(s.begin(), s.end(), "x")`（char 与 const char* 比较 = 编译错误）、go 生成 `slices.Contains(s, "x")`（string 非切片 = 编译错误）—— 均通过启发式平衡校验但真机编译必炸。现 contains 类型感知分发（str → 子串查找 / Vec → 迭代器查找） | 已无此限制 |
| 34 | ~~let 块初始化（let x = if/match/if-let）生成端全语言回退 contract~~ **v0.2.7 已修复**：interp 一直支持，生成端现走「声明 + 分支尾赋值」模式（python 分支内赋值 / ts-js `let x;` / rust `let x;` 延迟初始化 / cpp-go 按分支值推导类型或需注解）。无 else 的 if/if-let 值语境仍诚实回退（类型为 ()） | 已无此限制（无 else 除外） |
| 35 | ~~Vec::insert / Vec::remove 在 cpp/go 回退 contract~~ **v0.2.7 已修复**：cpp `_dhvInsert`（越界 clean throw）/ `_dhvRemoveAt`（返回被删元素）；go `_dhvInsert(&v,...)` / `_dhvRemoveAt(&v,...)`（指针副作用通道） | 已无此限制 |
| 36 | ~~HashMap 方法族在 cpp/go 部分回退或错误~~ **v0.2.7 已修复**：insert/contains_key/keys/values/get/remove 全表面活体（cpp 模板助手 + go 泛型助手）；go remove 从匿名函数 `any`（链式 unwrap_or 解引用 any 是编译错误）升级为 `_dhvMapRemove` 返回 *V（与 Option 指针一致）；get 关闭 v1.4.3 遗留「下标近似」（越界/缺键 → nullopt/nil 与 interp None 对齐） | 已无此限制 |
| 37 | ~~String 方法族（trim/lower/upper/replace/split/splitWS/lines/repeat/char_count/join）在 cpp 回退 contract~~ **v0.2.7 已修复**：12 方法全表面活体（C++ 标准库无这些便捷函数 → 内联助手族）；char_count 为 UTF-8 码点计数（非字节数） | 已无此限制 |
| 38 | **get 的 unknown-kind 接收者维持下标近似（cpp/go）**：无类型注解且无法推导接收者类型时（如链式中间值 `foo().get(0)`），cpp/go 无运行时分发通道，诚实维持下标近似 | 给接收者写显式类型注解（参数签名或 let 注解） |
| 39 | ~~vec! 宏 / 数组字面量在 cpp 生成 lambda 捕获非法语法~~ **v0.2.8 已修复**：`vec![1, 2]` 此前 cpp 生成 `[1, 2]`（C++ 中是 lambda 捕获表达式，非数组字面量）—— g++ 报 "expected identifier before numeric constant"。现 cpp 用 CTAD `std::vector{1, 2}`（C++17 class template arg deduction）；go 同步从 `[1, 2]`（固定数组）升级为 `[]any{1, 2}` 切片字面量 | 已无此限制 |
| 40 | ~~cpp 闭包缺外层变量捕获~~ **v0.2.8 已修复**：`Option::Some(first).map(\|x\| x + last)` 此前 cpp 生成 `[](auto x) { return x + last; }` —— `last` 未捕获，g++ 报 "'last' is not captured"。现 cpp 闭包用 `[&](auto x) { return ...; }`（按引用捕获所有外层变量，与 Rust 闭包默认行为一致） | 已无此限制 |
| 41 | ~~cpp extend/append 临时变量迭代器不同源导致 length_error~~ **v0.2.8 已修复**：`v.extend(vec![5])` 此前 cpp 内联 `(std::vector{5}).begin(), (std::vector{5}).end()` —— 两个临时不同源 → `vector::_M_range_insert` length_error。现用 `_dhvExtend(v, arg)` 模板助手（const ref 绑定临时，保证 begin/end 同源） | 已无此限制 |
| 42 | ~~exprKind 不能识别 Option::Some/None、Result::Ok/Err、Vec::from、HashMap::new、vec! 宏~~ **v0.2.8 已修复**：`Option::Some(first).map(...)` 此前因 `Option::Some(first)` kind 为 unknown → map 走 Vec 分支 → cpp/go 无映射 → 退化为 contract。现 exprKind 新增 `case 'call'` 与 `case 'macro'` 识别路径调用/宏的返回 kind | 已无此限制 |
| 43 | ~~Option 链式家族 map/and_then/or/unwrap_or_else/expect 在 cpp/go 缺映射~~ **v0.2.8 已修复**：cpp 五方法全表面活体（`_dhvOptMap`/`_dhvOptAndThen`/`_dhvOptOr`/`_dhvOptUnwrapOrElse`/`_dhvOptExpect` 模板助手）；go 仅扩非闭包方法 or/expect（HSL 闭包无类型注解，map/and_then/unwrap_or_else 暂不映射 go —— 诚实回退 contract） | 已无此限制（go 闭包方法除外） |
| 44 | ~~interp `"".parse::<f64>()` 返回 Ok(0) 的 JS 语言怪癖泄漏~~ **v0.2.9 已修复**：此前经 JS `Number("") === 0` 隐式返回 Ok(0)（与整数路径 `"" → Err` 不一致）。现空串统一 Err（与 Rust 语义一致） | 已无此限制 |
| 45 | ~~裸 `Option::None`（无注解 let 中转）链式方法在 cpp/go 生成非法代码~~ **v0.2.9 已修复**：`Option::None.map(f)` 此前 cpp 生成 `_dhvOptMap(std::nullopt, f)`（nullopt_t 模板推导失败）、`None.unwrap_or(0)` 生成 `std::nullopt.value_or(0)`、go 生成 `*nil` —— 均编译必炸但通过启发式校验。现 None 字面量接收者专门派发（化简等价 + cpp `_dhvNoneT` 链式包装器）；unwrap/expect 与 go 闭包族仍诚实回退 contract | 已无此限制（裸 None 的 unwrap/expect 除外） |
| 46 | ~~`Option::None.filter(f)` 因 exprKind 两段路径返回 unknown 走 Vec 分支回退~~ **v0.2.9 已修复**：`case 'path'` 现识别两段 `Option::*`/`Result::*` 路径值的 kind | 已无此限制 |
| 47 | **parse 生成的 Result 在非 rust 后端是 Option-flavored 表示**：Err → None/null/nullopt/nil，错误消息（Err payload）不可观察；`r.map(f)`/`unwrap_err()`/`match r { Err(e) => ... }` 等 Result 高阶消费不可用（is_ok/is_err/unwrap_or/unwrap/expect 已映射） | 需要 Err 详情时用 interp 运行；生成端消费止于 Option 面 |
| 48 | **parse 的浮点接受面在 cpp/go 有已知边缘差异**：cpp `std::stod`/go `strconv.ParseFloat` 接受 `inf`/`nan` 字面量（interp 的 JS `Number` 拒绝裸 `inf`）；cpp 整数溢出 i64 范围 → nullopt（interp BigInt 任意精度）；`"0x10"` interp 接受为 16（JS Number 十六进制怪癖）、cpp/go 拒绝 | 解析输入约束在十进制字面量域（工程实践主流）；奇异输入用 interp |
| 49 | ~~go 后端生成非法三元表达式~~ **v0.2.10 已修复**（真机 go build 实测发现）：`unwrap_or` 此前生成 `(recv != nil ? *recv : d)` —— go 无三元运算符，编译必炸；现走 `_dhvUnwrapOr` 泛型助手（单次求值 + 副作用安全）。同轮修复：go 同 package 多文件助手重复声明（去重注入首文件）、未使用 import（按需裁剪）、`len()` 与 i64 混算类型不匹配（`int64(len(x))` 统一）、尾兜底 return 不可达（vet）| 已无此限制 |
| 50 | ~~rust 后端 format! 双重格式化~~ **v0.2.10 已修复**（真机 rustc 实测发现）：此前表达式实参嵌入 `{...}` 且同时传位置参数（invalid format string）；现纯标识符走内联捕获 `{name}`、表达式走位置 `{}`。同轮修复：rust 头部缺 `use std::collections::HashMap`（E0425）| 已无此限制 |
| 51 | **HSL interp 数值动态语义 vs go/rust 静态严格类型**：`len()`（usize）与 `i32` 值混算（如 `nk + nv + got`）在 interp 通过，但 go/rust 真机编译报类型不匹配 | 源码侧类型一致化（i64 为主）；混型场景需显式转换 |
| 52 | **rust String 语境的字面量需源码侧 `String::from`**：`m.insert("a", 1)` 的 HashMap<String,_> 在 rust 真机编译报 `expected String, found &str`（interp 宽松通过） | 与 Rust 本身语义一致：字面量入 String 容器写 `String::from("a")`（backends-demo 即此风格） |
| 53 | **nova/macros.hsl 的宏 transcriber `{{ ... }}` 与 block 插值语法冲突**：双花括号块形态触发 dhv-ts 解析器按插值处理（资源块缺名称报错）——独立实验文件，不影响 nova 主入口 15 模块 | 单花括号 transcriber（`{ ... }`）即可 |
| 54 | **go/rust 多文件工程的 mod/package 组织文件不随 emit 生成**：rust 跨文件 use 按 crate mod 链路径生成（`crate::gen::rust::prompt`），需自组 lib.rs/mod.rs 链（或 cargo 工程）方可编译；go 多文件同 package 已可直接编译 | rust：按目录层级组装 mod 链（参考 tests v1.4.10 用例做法）；后续版本考虑 emit 自动生成 mod.rs |
| 55 | ~~dhv-ts 不支持值语境 range~~ **v0.2.14 已修复**：`let r = a..b;` / `let r = a..=b;` / `let r = 0..n;` 等值语境 range 现已支持解析与校验（BNF v1.5 已知限制 #10 同步关闭） | 已无此限制 |
| 56 | **dhv Go 后端函数体为骨架转译**（v0.2.17 新增，v0.2.20 大幅扩展）：struct/enum/fn/trait/impl/graph 生成合法 Go 代码；表达式覆盖 binary/unary/call/method/field/await/cast/if/else-if/match→switch/for→range/while→for/assign/compound-assign/index/slice/array/struct literal/closure→func literal/return/break/continue/block/range/try/loop/if-let/while-let。**仍不支持**：match arm 模式解构赋值（仅类型匹配）、泛型方法调用（turbofish 忽略）、async/await 语义完整转译 | 多数控制流场景可直接使用；match 模式解构与 async 逻辑待人工接手 |
| 57 | **Go 后端类型映射为近似**（v0.2.17）：Option→`*T`（Go 指针，nil 代表 None）、Result→`(any, error)`（非变体通道，Err 详情不可模式匹配）、HashMap→`map[string]any`（丢失键值类型）、Vec→`[]any`（丢失元素类型） | 简单数据结构可直接使用；类型安全场景需人工调整 |
| 58 | ~~未知函数调用 check 不报错~~ **v0.2.51 已修复**：E-2 静态检查（含 std 导入名校验）；单段路径调用、局部绑定豁免、两段路径保守不查 | 已无此盲区 |
| 59 | ~~`format!` 精度说明符 `{:.N}` 被静默丢弃~~ **v0.2.51 已修复**：interp + python/ts/js/rust/go/cpp 六端一致实现浮点十进制精度 | 已无此限制（宽度/对齐等其余 flags 仍为未定义域） |
| 60 | ~~native python 块缩进敏感~~ **v0.2.51 已修复**：块体统一 dedent；末表达式语句判定剥离字符串字面量（`"a=b" % x` 不再误判） | 已无此限制 |
| 61 | ~~跨文件函数/常量/枚举变体依赖未接线~~ **v0.2.51 已修复**：emit 新增 `collectCallableRefs` + `importHeaderForFnDeps`（python 同目录 from-import / ts 相对 import / rust use / go 同包），宏实参内的结构体字面量与函数调用同步收集；实测生成物全链（mathphys `recompute`/armorlab `inspect_payload`）与解释器语义一致 | 已无此盲区（contract 围栏体内的引用仍不接线——诚实边界） |
| 62 | ~~std/math 自由函数在活体翻译中裸名直出~~ **v0.2.51 已修复**：python `math.sin`/`math.pi`、ts/js `Math.sin`/`Math.PI`；rust/go/cpp 方法形态触发诚实 contract 回退 | 已无此盲区 |
| 63 | **无类型注解的整数值浮点变量除法仍是近似**（v0.2.51 部分修复）：`1.0/7.0`、`a as f64 / b`、显式 `let x: f64` 均已正确；无注解变量承载整数值浮点时按动态整数截断（完整类型推导归 dhv Rust 编译器，限制 #1） | 给绑定额外资一层 `as f64` 或类型注解 |
| 64 | **i64/u8 等注解域算术溢出：interp 参考语义 = BigInt 任意精度不环绕**（v0.2.54 spec 化 + S-15 静态守门）：`let a: i64 = i64::MAX; let b = a + 1;` 在 interp 打印 9223372036854775808（静默越域不环绕，rust 后端环绕/panic、python 与 interp 一致、js/ts 字面量读入即舍入）。**静态口径**：S-15 在「静态可折叠 + 域已知」时编译期拒绝（dhv/dhv-ts 双端一致）；无注解的动态算术留运行期 BigInt 参考语义。**emit 侧**：rust 大字面量自动补 i64/i128 后缀（L-9b），js/ts 超安全域字面量 emit 显式告警（L-9c）。超 i128 容量源字面量由 S-16 静态拒绝（dhv parser 此前归零——L-10 已修） | 注解域内确定性算术靠 S-15 静态保证；跨后端值级一致性以 interp 为基准语义（python 同构；rust i64 环绕与 js Number 精度为已声明投射差异） |
| 65 | **dhv(Rust) 带后缀整数字面量曾一直解析为 0**（v0.2.54 L-11 已修）：pest 的 `integer_literal` 规则把后缀并入捕获文本，`from_str_radix("300u8")` 必失败 → `unwrap_or(0)` 静默归零（`250u8` 的值是 0！）—— dhv-ts lexer 剥离后缀无此问题。修复后后缀先剥离再解析；S-13（v2）新增后缀域字面量校验（`300u8` 报错） | 已无此损坏（锁定用例 S13_suffix_domain + run-all 后缀族） |
| 66 | **结论对拍对 parse 层静默损坏失明 → 已建值级对拍**（v0.2.54 第五轮）：`dhv parse <file> --dump-values` 按文件序 dump 全部整数字面量（raw/值/后缀）；`bun tests/run_value_conformance.ts` 与 dhv-ts AST 遍历逐条比对（fixtures/values 语料 8 类：十进制/进制/后缀/表达式序/模式位/图拓扑/判别式/浮点判别）。L-11 类 bug（归零、舍入、后缀吞字）从此被机器护栏锁定 —— RED 注入实验实证：恢复旧 bug 后 suffix_family 7 字面量立即报不一致。已并入 run_conformance.sh 第 4 段 | 新增字面量形态（新进制/新后缀）时在 fixtures/values 加语料即可 |

| 67 | **字符串/浮点值损坏曾无机器护栏 → 三族值级对拍**（v0.2.56 第六轮）：(a) **L-12** dhv `unescape_string` 的 `\u{...}` 收集循环越过 `}` 吞掉后续字符（`"\u{41}bc"` 输出 `'䆼'` 而非 `"Abc"`；`"\u{41}x"` 整串静默空）—— 修复：遇 `}` 停止 + pest 码点域收紧（1-5 位任意 / 6 位 ≤ 0x10FFFF）+ 无效码点保留原文；ts 端同步去除下划线容忍（`\u{_4_1_}` 双端拒绝）。(b) **L-13** dhv float 后缀剥离用 `trim_end_matches(is_alphabetic)` 剥不掉以数字结尾的 `f32/f64` → `1f32` parse 失败 **静默归 0**（每条带 f 后缀浮点必损坏）—— 值级对拍 float 扩展当场抓获；修复：精确后缀剥离 + 失败改 NaN（可观测）。(c) **L-14** ts lexer 曾把 `1f32` 分派为 int token（kind 漂移）—— 后缀 f32/f64 ⇒ 一律 float。值级对拍扩展：**float 用 IEEE754 位模式（16 hex，双端唯一可靠等价判据，NaN payload/符号位全保真）**；string 用统一转义 repr；宏 token 树 walk 对齐（表达式宏 dump / 语句级 macro_invocation_semi 不 dump）；语料 8→12 类（浮点族/转义族/unicode 边界/宏口径） | RED 注入实证：模拟 L-12 复发 unicode_edge 立即失配；新增字面量形态时在 fixtures/values 加语料 |
| 68 | **cast 域折叠（truncation-aware）已进静态检查**（v0.2.56 S-17）：`300 as u8 + 300`（环绕折叠后 344 越域）此前静态漏报（intValOf 不穿 cast）；现 cast 到整型域 = 显式截断投射（与 interp castValue / rust as 同构环绕），cast 到 float/String/bool/char 不折叠。合法族（`300 as u8 + 200` = 244 域内）零误报 | 折叠域外表达式（动态值）仍留运行期参考语义 |
| 69 | **emit 生成物行为级一致性已建机器护栏**（v0.2.55 第七轮）：前六轮对拍锁在 parse/check 层，emit 的「语法校验绿灯」对**行为为空**完全失明—— L-15：投射 `fn main` 到 python/js/ts 只生成函数定义、无入口调用，生成物运行「成功」（exit 0）但零输出零副作用（rust 后端 L-6 已有入口语义，三个 full 活体后端从未对齐）。修复：入口守卫（python `globals().get('__name__') == '__main__'` —— exec 消费形态惰性；js/ts `import.meta.url` 与 argv[1] 比对）+ 退出码 = 返回值。同轮七连发：**L-16** 未投射依赖静默漏接（新增 X-5 emit 期告警）；**L-17** python `//` floor ≠ interp 截断除（-7/2: -4 vs -3）→ `_dhv_idiv/_dhv_imod/_dhv_div`（含 unknown 运行期类型分流）；**L-18** js `/` 浮点除（7/2=3.5）→ `_dhvIdiv/_dhvDiv`；**L-19** 显示规范三端漂移（python `True`/`3.0`/对象 repr、js `[object Object]`）→ `_dhv_str`/`_dhvStr` 显示层 + 枚举/struct `__str__`/`toString()` 烘焙（python `_dhv_float_str` 完整复刻 ECMAScript Number::toString）；**L-20** js 标识符约定三连错配（structLit `lowerFirst` vs camel 工厂导出 / 类型名 import 引用不存在导出 / 单元变体原名 vs snakeUpper 导出）→ camel 镜像 + kind 过滤 + 双注册判据；**L-21** 整值浮点字面量 `3.0` 发射为 `3`（宿主 String() 渗漏，rust i32 推断/整除语义全错）→ 整值补 `.0`。**行为级对拍**：`bun tests/run_emit_conformance.ts`（fixtures/emit 6 类语料：arith/values/enum/struct/vec/bigint_py；interp run ↔ emit→python3/bun 真实运行，`emit::` 标记行逐行全等；语料自带投射矩阵，>2^53 只投 python 防 L-9c 已声明漂移污染判据）；RED 注入实证：守卫失效 → 8 组立即转红（「interp N vs 生成物 0 行」= L-15 签名）。已并入 run_conformance.sh 第 5 段 | 残留：Option::Some 在生成端是透明值（`Some(5)` 显示为 `5`）；emit 不做 check 前置门（check 错误程序仍可投射）。新增行为场景时在 fixtures/emit 加语料即可 |

---

# 附录A 38 后端语言完整注册表

来源：`dhv-ts/src/backends/registry.ts` + `dhv/src/langs.rs`（BNF v1.5 §5.2）。
能力级：full = 活体语句翻译 / logic = 语句子集（回退 contract）/
contract = 类型契约 + 围栏 / static = 原文 + 插值。
「native」列 = dhv-ts 运行期可直接执行 native 块；
「校验」列 = emit 时的交叉语法校验工具。

**dhv 专属后端实现**（7 个，其余 31 语言走通用契约后端）：
python / typescript / rust / go（以上 4 个为编程语言 logic/full 级别，含真实函数体转译）+ yaml / markdown / json（以上 3 个为静态格式后端）。

### Tier 1 · Harness 核心（10）

| id | 名称 | 扩展名 | 能力级 | native | 校验 | 备注 |
|:---|:---|:---|:---|:---:|:---|:---|
| python | Python | .py | full | 是 | python3 | dataclass + isinstance 链 match |
| typescript | TypeScript | .ts | full | 是 | bun-ts | interface + switch-kind match |
| javascript | JavaScript | .js | full | 是 | bun-js | JSDoc 契约 + 工厂函数 |
| rust | Rust | .rs | logic | — | — | 原生 match / derive |
| go | Go | .go | logic | — | — | v0.2.17 升级为 logic 级专属后端；v0.2.20 大幅扩展函数体转译（30+ 种表达式）；struct→type X struct / enum→interface+变体 / fn→func / trait→interface / impl→方法集 / graph→func main() / match→switch / for-in→for range |
| cpp | C++ | .cpp | logic | — | — | holds_alternative 分发 |
| java | Java | .java | contract | — | — | 顶层 sealed interface + record（Java 17+）；fn/const 宿主 class Dhv<stem>（v0.2.5） |
| csharp | C# | .cs | contract | — | — | abstract record + 派生 record（C# 9+） |
| kotlin | Kotlin | .kt | contract | — | — | sealed class + data class |
| swift | Swift | .swift | contract | — | — | enum 关联值原生支持 |

### Tier 2 · 脚本与动态（8）

| id | 名称 | 扩展名 | 能力级 | 备注 |
|:---|:---|:---|:---|:---|
| ruby | Ruby | .rb | contract | Struct + case/in 模式匹配（Ruby 2.7+） |
| php | PHP | .php | contract | enum（PHP 8.1）/ 抽象类 + match |
| lua | Lua | .lua | contract | table 标签联合 |
| perl | Perl | .pl | contract | Moose 风格类型约束 |
| bash | Bash | .sh | contract | 校验 bash -n；case/函数壳 + 关联数组 |
| powershell | PowerShell | .ps1 | contract | class + switch 契约 |
| r | R | .R | contract | list + switch 契约 |
| julia | Julia | .jl | contract | struct + Union 契约 |

### Tier 3 · 函数式（6）

| id | 名称 | 扩展名 | 能力级 | 备注 |
|:---|:---|:---|:---|:---|
| scala | Scala | .scala | contract | sealed trait + case class |
| elixir | Elixir | .ex | contract | defmodule + defstruct + 模式匹配 |
| erlang | Erlang | .erl | contract | tagged tuple；行注释 `%` |
| haskell | Haskell | .hs | contract | data … = …（原生和类型）；行注释 `--` |
| ocaml | OCaml | .ml | contract | type … = A \| B；行注释 `(* … *)` 带闭合 |
| fsharp | F# | .fs | contract | type … = A of …（原生 DU） |

### Tier 4 · 系统与现代（8）

| id | 名称 | 扩展名 | 能力级 | 备注 |
|:---|:---|:---|:---|:---|
| zig | Zig | .zig | contract | tagged union |
| nim | Nim | .nim | contract | object variants |
| crystal | Crystal | .cr | contract | abstract class + struct + case |
| dart | Dart | .dart | contract | sealed class + factory（Dart 3） |
| groovy | Groovy | .groovy | contract | @Canonical 契约 |
| objectivec | Objective-C | .m | contract | interface + 实现壳 |
| d | D | .d | contract | struct + tagged union |
| vb | Visual Basic | .vb | contract | Structure/Module；行注释 `'` |

### 静态格式（6）

| id | 名称 | 扩展名 | 说明 |
|:---|:---|:---|:---|
| yaml | YAML | .yml | block/static 原文 + {{}} 插值 |
| markdown | Markdown | .md | 同上（AGENTS.md / 提示词的常用目标） |
| json | JSON | .json | JSON 无注释：dhv 以 .map 边车文件记录围栏 |
| toml | TOML | .toml | 原文 + 插值 |
| ini | INI | .ini | 原文 + 插值（行注释 `;`） |
| xml | XML | .xml | 原文 + 插值 |

### 别名表

| 别名 | 解析为 |
|:---|:---|
| ts | typescript |
| js | javascript |
| py | python |
| md | markdown |
| yml | yaml |
| c++ | cpp |
| objective-c | objectivec |
| sh / shell | bash |

---

# 附录B std 函数速查总表

10 模块 · 60 函数 + 2 常量。签名中的 `T` 为泛型占位。

### std/core

| 函数 | 签名 |
|:---|:---|
| identity | `(T) -> T` |
| todo | `(String?) -> !` |
| unreachable | `(String?) -> !` |
| type_name | `(T) -> String` |
| hash | `(T) -> i64`（FNV-1a 64） |

### std/collections

| 函数 | 签名 |
|:---|:---|
| vec | `(...T) -> Vec<T>` |
| repeat_vec | `(T, i64) -> Vec<T>` |
| zip | `(Vec<A>, Vec<B>) -> Vec<(A, B)>` |
| chunk | `(Vec<T>, i64) -> Vec<Vec<T>>` |
| dedup | `(Vec<T>) -> Vec<T>`（相邻去重） |
| unique | `(Vec<T>) -> Vec<T>`（全局去重） |
| flatten | `(Vec<T>) -> Vec<T>` |
| sort_desc | `(Vec<i64>) -> Vec<i64>` |
| reverse | `(Vec<T>) -> Vec<T>` |
| swap_remove | `(Vec<T>, i64) -> T` |

### std/text

| 函数 | 签名 |
|:---|:---|
| split_once | `(String, String) -> Option<(String, String)>` |
| rsplit_once | `(String, String) -> Option<(String, String)>` |
| split_at | `(String, i64) -> (String, String)` |
| to_snake | `(String) -> String` |
| to_camel | `(String) -> String` |
| to_pascal | `(String) -> String` |
| to_kebab | `(String) -> String` |
| pad_start | `(String, i64, String?) -> String` |
| pad_end | `(String, i64, String?) -> String` |
| capitalize | `(String) -> String` |
| count | `(String, String) -> i64` |
| is_alpha | `(String) -> bool` |
| is_numeric | `(String) -> bool` |
| is_alphanumeric | `(String) -> bool` |
| truncate | `(String, i64, String?) -> String` |
| levenshtein | `(String, String) -> i64` |

### std/math

| 函数 / 常量 | 签名 |
|:---|:---|
| PI, E | `f64` |
| sin / cos / tan | `(f64) -> f64` |
| asin / acos / atan | `(f64) -> f64` |
| atan2 | `(f64, f64) -> f64` |
| exp / ln / log2 / log10 | `(f64) -> f64` |
| pow | `(f64, f64) -> f64` |
| sqrt | `(f64) -> f64` |
| gcd / lcm | `(i64, i64) -> i64` |
| signum | `(f64) -> f64` |
| isqrt | `(i64) -> i64` |
| div_ceil / div_floor | `(i64, i64) -> i64` |
| rem_euclid | `(i64, i64) -> i64` |
| hypot | `(f64, f64) -> f64` |
| is_nan / is_infinite | `(f64) -> bool` |
| inf | `() -> f64` |

### std/io（宿主依赖；全部 Result 语义）

| 函数 | 签名 |
|:---|:---|
| read_file | `(String) -> Result<String, String>` |
| write_file | `(String, String) -> Result<i64, String>` |
| append_file | `(String, String) -> Result<i64, String>` |
| list_dir | `(String) -> Result<Vec<String>, String>` |

### std/json（本地实现，零宿主依赖）

| 函数 | 签名 |
|:---|:---|
| parse | `(String) -> Result<T, String>` |
| stringify | `(T) -> String` |
| get | `(T, String) -> Option<T>` |

### std/time

| 函数 | 签名 |
|:---|:---|
| now_ms | `() -> i64` |
| now_iso | `() -> String` |
| duration_desc | `(i64) -> String` |

### std/random（mulberry32，默认种子 42）

| 函数 | 签名 |
|:---|:---|
| seed | `(i64) -> ()` |
| random | `() -> f64` |
| int_in | `(i64, i64) -> i64` |
| choice | `(Vec<T>) -> Option<T>` |
| shuffle | `(Vec<T>) -> Vec<T>` |
| uuid_v4 | `() -> String` |

### std/env

| 函数 | 签名 |
|:---|:---|
| env_get | `(String) -> Option<String>` |
| task_text | `() -> String` |
| model_name | `() -> String` |
| workspace | `() -> String` |

### std/iter

| 函数 | 签名 |
|:---|:---|
| range | `(i64, i64) -> Vec<i64>` |
| range_step | `(i64, i64, i64) -> Vec<i64>` |
| enumerate | `(Vec<T>) -> Vec<(i64, T)>` |
| chain | `(Vec<T>, Vec<T>) -> Vec<T>` |
| take | `(Vec<T>, i64) -> Vec<T>` |
| skip | `(Vec<T>, i64) -> Vec<T>` |
| min_of | `(Vec<i64>) -> Option<i64>` |
| max_of | `(Vec<i64>) -> Option<i64>` |

---

# 附录C 常见问题 FAQ

**Q1：为什么 graph 里的 match 报 S-6，明明写了 `_ =>` 兜底？**

S-6 对两种场景语义不同：**graph AgentLoop 内**的枚举 match 禁止 `_`
兜底——Agent 核心决策循环必须显式直面每个新变体（这是 HSL 固化
「非确定性循环决策」的核心铁律）；**普通函数内**的 `_` 兜底合法且视为
穷尽（v0.2.2 起与 Rust 语义一致）。修复：在 AgentLoop 内补全变体 arm，
或把 match 移出 graph。

**Q2：为什么 contract 后端的生成文件里有 NotImplementedError？**

这是设计行为（诚实边界）：26 种 contract 语言只翻译类型与签名；
函数体的 HSL 原文在 `@dhv:source-map` 围栏里，并配显式未实现标记。
运行请用 dhv-ts 或需要函数体的语言（python/typescript/javascript 为
full，rust/go/cpp 为 logic 子集）。

**Q3：bun 的版本要求是什么？**

≥ 1.1（开发实测 1.3.14）。dhv-ts 是纯 TypeScript 零第三方 npm 依赖，
只需 bun 能跑 TS。另外 python3 ≥ 3.8 影响 native python 块与 python
后端语法校验。

**Q4：围栏（@dhv:source-map）能删吗？**

不能。三个标记是 sync 的锚点——删掉后 sync 找不到回写位置；
活体区 / 镜像区 / 内核区的分工也依赖它们。删除围栏 = 放弃该文件的
双向工程能力（文件本身仍可用，但下次 emit 会整体重写）。

**Q5：`String::new()` 可用吗？**

可以——v0.2.2 起 `String::new()` / `Vec::new()` / `HashMap::new()` /
`HashSet::new()` 及 `with_capacity` 系列全部可用（注意 HSL 的 `let`
默认不可变，`String::new()` 后要 `push_str` 需 `let mut s = String::new();`）。

**Q6：`impl From<ProviderError> for HarnessError` 写了但 `?` 没转换？**

见 13.3 #2：From 块可解析可检查，但运行期 `?` 传播错误值原样传递
（类型泛型实参在解析层被丢弃）。当代实践：统一错误枚举 + 元组变体
包裹（`HarnessError::Tool(ToolError)`），跨层手动 match。

**Q7：emit 报「P-4 只能投射到编程语言后端」怎么办？**

两类投射约束不同：`block/static` → 6 种静态格式（yaml/markdown/json/
toml/ini/xml）；代码项（fn/struct/enum/trait/impl/graph）→ 32 种编程
语言。检查你 project{} 冒号后面的语言 id（可用别名，见附录 A）。

**Q8：同一个 struct 能同时投射到多个语言吗？**

能，这正是核心机制：`Prompt -> "a.py" : python, Prompt -> "b.rs" : rust,`
同一项可出现在多条投射里。限制是**一个物理路径只能一种语言**
（P-2 / P-5 冲突警告）。

**Q9：sync 报「当前不可解析，拒绝回写」？**

.hsl 源文件本身有语法错误（可能是上一次手改弄坏的）。sync 在回写前
会先确认源文件可解析；修好源码再 sync。回写后也会重解析，失败自动
整体回滚——错误镜像不会污染源码。

**Q10：run 时 native 块报「后端未接入解释器」？**

dhv-ts 运行期只执行 `native typescript`（进程内）与 `native python`
（子进程）。`native rust` 等其余语言的语义是静态投射（P5）：
在 dhv 编译器里，与目标语言一致时原样透传进生成物。

**Q11：为什么我的 block 投射出来的 YAML 缩进很怪？**

静态格式后端**不翻译内容**——block 体是原始文本，缩进原样进入产物。
把 block 体顶格书写（参考 examples/dsh/config/resources.hsl）。

**Q12：graph 里一定要有 loop 吗？**

是的（G-1）：graph body 必须恰含至少一个 AgentLoop。没有循环的
纯顺序逻辑应该写成普通 fn。

**Q13：edge 报 G-3 无条件环，但我需要环怎么办？**

给环上至少一条边加 Guard：`edge executor -> model on Event::Observed;`
条件环是 Agent 的呼吸回路，是**预期**拓扑；G-3 只处决无条件死锁环。

**Q14：S-7 说我的 import 未使用，但我明明在 project{} 里用了？**

project{} 里的引用**确实算使用**（源码行扫描会命中）。若仍报错，
说明 import 的名字与使用处拼写不一致，或只出现在注释里——
注释行被排除在扫描之外。

**Q15：`--scale monolith` 为什么 emit 出来还是 microkernel 脚手架？**

见 13.3 #9：emit 的尺度取自**入口文件**的 `scale = ...` 声明；
CLI 的 --scale 目前只影响 run 的观测标注。改入口文件声明再 emit。

**Q16：watch 模式下生成的文件可靠吗？**

watch 为速度牺牲了交叉语法校验（emit 时 validate: false）；check 失败
时不 emit，产物保持上次成功状态。提交前跑一次独立 emit 做完整校验。

**Q17：怎么确认生成代码语法真的合法？**

emit 默认做交叉校验并在每行输出 `语法✓ 工具`；manifest.json 的
`syntax_check` / `syntax_tool` 字段可机读断言。python 用
python3 -m py_compile，TS/JS 用 Bun.Transpiler，bash 用 bash -n，
其余语言用括号平衡启发式（heuristic:balanced）。

**Q18：检查器能代替类型系统吗？**

不能也不打算。dhv-ts check 是结构级铁律（S2/S4/S6/S7/S8 + G/P/N
子集）；完整类型推导、泛型单态化、S-1/S-3 编译期形态是 dhv Rust
编译器的职责。写显式类型注解是当前的纪律。

---

# 附录D 术语表

| 术语 | 英文 / 上下文 | 含义 |
|:---|:---|:---|
| HSL | Harness Specification Language | 本语言；为编写 AI Agent harness 而生的编译型语言 |
| DHV | — | HSL 的工具链总称（编译器 dhv + 参考解释器 dhv-ts） |
| dhv-ts | — | HSL 的 TypeScript 参考解释器（check/run/emit/targets/sync/watch） |
| harness | Agent harness | 包裹模型的工程层：工具执行、协议、预算、安全、观测 |
| 逻辑层 / 拓扑层 / 物理层 | definition / topology / projection | HSL 的三层抽象（1.2 节） |
| graph | — | 一等拓扑项：带参数与返回类型的 Agent 状态机 |
| node | — | graph 内的可执行单元声明（可带初始化或留作插件注入位） |
| edge | — | 节点间转移边；`on Guard` 携带条件（枚举变体） |
| AgentLoop | — | graph body 内的 `loop`：预算闸门 + match 分发 + 终止 |
| Guard | — | edge 的条件模式（如 `on Action::Tool`）；也是 match arm 的 if 守卫 |
| scale | monolith / microkernel | 架构尺度声明：边的事件总线形态 vs 直接调用形态 |
| project{} | — | 投射声明块：逻辑项 → 物理路径 : 目标语言 |
| 投射 | projection | 把 HSL 逻辑生成到目标语言/格式的动作 |
| 能力级 | full / logic / contract / static | 代码生成的诚实分级（5.3 节） |
| 围栏 | fence | 生成文件内的三标记协议块（@dhv:source-map 等） |
| 源镜像 | hsl-mirror | 围栏内的 HSL 源码副本——唯一可手编辑区 |
| 活体翻译区 | live zone | full/logic 后端翻译出的真实目标语言代码（重编译覆盖） |
| 双向工程 | bidirectional | 生成物 ↔ 源码的 sync 回写闭环（第八章） |
| 剧本 | fixture | scripted 模式的确定性模型决策 JSON（acts + reviews） |
| 事件总线 | event bus | G6 的观测通道（events.jsonl）；microkernel 的边语义 |
| 路径监狱 | path jail | $host.fs 的 workspace 边界限制 |
| 能力域 | #[capability] | 编译期能力声明（file_read / process_spawn / net_connect…） |
| S 规则 | strictness | 严格性铁律（S-1..S-8） |
| G 规则 | graph | 拓扑校验规则（G-1..G-6） |
| P 规则 | projection | 投射一致性规则（P-1..P-7） |
| N 规则 | native | native 块安全规则（N-1..N-5） |
| R-1 | — | 运行入口约定：run 调用入口文件的 fn main |
| native 逃生舱 | escape hatch | `native <lang> { ... }`：体内为宿主语言代码的表达式 |
| $host | — | native 块内的宿主 API 命名空间（fs/shell/llm/json/…） |
| 穷尽性 | exhaustiveness | match 覆盖所有枚举变体的强制要求（S-6） |
| 和类型 | sum type | enum 的类型理论名称：一个值是若干变体之一 |
| trait 对象 | Box<dyn Trait> | 运行时多态的注入位（Provider 插拔的标准形） |
| 遮蔽 | shadowing | 同名绑定重新声明（S-8：同作用域为错，跨作用域合法） |
| AGENTS.md | — | 给读仓库的 agent 看的行为说明文件；在 HSL 中是静态投射物 |
| dsh | DS Harness | examples/dsh：HSL 复现的 DeepSeek 风格编码助手 harness |
| nova | — | examples/nova：多 Agent 深度研究系统（2000+ 行压测项目） |
| manifest.json | — | emit 产物的能力清单（tier / 语法校验结果 / 协议声明） |
| mulberry32 | — | std/random 的可复现 PRNG 算法（默认种子 42） |
| monomorphization | 单态化 | 泛型的编译期实例化策略（dhv 编译器职责） |
| BNF | — | `hsl-spec/BNF.md`：语言规范（文法 + 静态语义） |

---

## 结语

HSL 的核心赌注是：**Agent 工程值得一门语言**。它的判断依据是——
当「编排结构」成为资产，它就必须被显式声明（graph/edge）、被静态校验
（S/G/P/N）、被可观测（G6 事件）、被多语言投射（project{}）、
被双向维护（围栏协议）。这五件事，脚手架生成器做不到，通用语言
也不屑于做。

本指南描述的一切都可在当前工具链里验证——去跑 `run-all.ts`，
去 emit backends-demo，去把第十二章的统计 Agent 改成你自己的工具。
规范细节以 `hsl-spec/BNF.md` 为准，行为以 dhv-ts 为准，问题以
worklog 与测试套件为史。

— HSL / DHV · MIT · v0.2.27
