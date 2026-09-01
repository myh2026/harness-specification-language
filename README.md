<div align="center">

# HSL — Harness Specification Language

**从逻辑到 38 个后端的工程投射 · 为编写 AI Agent harness 而生的编译型语言**

[![License: MIT](https://img.shields.io/badge/License-MIT-informational.svg)](LICENSE)
[![Toolchain](https://img.shields.io/badge/toolchain-v0.2.43-success.svg)](#%EF%B8%8F-当前版本)
[![BNF](https://img.shields.io/badge/BNF-v1.5.0-blue.svg)](toolchain/hsl-spec/BNF.md)
[![Backend](https://img.shields.io/badge/backends-38-orange.svg)](toolchain/hsl-spec/BNF.md)

</div>

---

> **一句话定位**：你用一套带类型、带拓扑校验的源码描述 Agent 的逻辑与编排，
> DHV 工具链把它投射成一个真实的多语言工程仓库（Python / TypeScript / Rust / Go / … / YAML / Markdown），
> 而不是一个黑盒脚手架。

## 目录结构

```
HSL/
├── toolchain/     # DHV 工具链（主产品）
│   ├── dhv/       #   Rust 编译器（PEG 文法 hsl.pest，多后端转译）
│   ├── dhv-ts/    #   TypeScript 参考解释器（check / run / emit / targets / sync / watch）
│   ├── examples/  #   内置示例：nova（研究型 Agent）、dsh（对话型 Agent）、backends-demo（38 后端投射）
│   ├── hsl-spec/  #   BNF v1.5.0 语言规范 + 合规矩阵
│   └── tests/     #   双编译器一致性回归套件（run_conformance.sh）
├── guide/         # HSL 语言完全指南（4042 行，全部示例经实测）
└── ide/           # VSCode 扩展（语法高亮 / 片段 / 主题）
```

## ⚡ 快速开始

```bash
# 0) 依赖：Rust 工具链 + Bun
git clone https://github.com/myh2026/HSL.git && cd HSL/toolchain

# 1) 构建 Rust 编译器
cd dhv && cargo build --release && cd ..

# 2) 校验示例（dhv 与 dhv-ts 双端一致）
./dhv/target/release/dhv check examples/nova/nova.hsl
bun dhv-ts/src/main.ts check examples/nova/nova.hsl

# 3) 跑双编译器一致性回归（31 组用例：示例 + parse/check/errors/modules 矩阵）
bash tests/run_conformance.sh

# 4) 投射出真实工程（38 后端）
bun dhv-ts/src/main.ts emit examples/backends-demo/agent.hsl --out /tmp/backends-demo
```

## 🌟 新语法（BNF v1.5）：投射规则组 `rules {}`

不再逐项手写 `A -> path : lang` 映射 —— 按项类型批量投射，显式映射优先（R1 遮蔽原则）：

```hsl
project {
    Nova -> "src/main.rs" : rust,          // 显式映射（优先于规则）

    rules {                                 // v1.5 规则组：{name} 占位符
        struct -> "src/types/{name}.rs"  : rust,
        enum   -> "src/types/{name}.rs"  : rust,
        fn     -> "src/logic/{name}.rs"  : rust,
        graph  -> "src/graphs/{name}.rs" : rust,
        block  -> "config/{name}.yml"    : yaml,
    }
}
```

规则声明校验：占位符白名单（R2）· 类型唯一（R3）· 类型注册（R4）；
展开池覆盖依赖模块导出项（R5）；展开项与显式项同等参与 P2/P4 校验（R6）。详见 [BNF §3.4](toolchain/hsl-spec/BNF.md)。

## 🧪 测试矩阵

| 套件 | 覆盖 |
|:---|:---|
| `dhv/tests/conformance.rs`（cargo test） | parse / check / errors（S4·S6·S7·S8·P4·P5·M2）/ 多模块 linker / 值语境 range（dhv 独有） |
| `toolchain/tests/run_conformance.sh` | **双编译器一致性**：39 组用例，dhv ↔ dhv-ts「通过/失败」结论逐一对照 |
| `dhv/tests/grammar_probe.rs` | pest 文法探针（raw string） |

回归原则：**修复一个 bug，就锁定一个用例。**

## 📦 版本化发布

预编译二进制随 [GitHub Releases](https://github.com/myh2026/HSL/releases) 分发：

| 平台 | 架构 | 产物 |
|:---|:---|:---|
| Linux | x86_64 (gnu) | `dhv-v{VER}-linux-x86_64.tar.gz` |
| macOS | arm64 (Apple Silicon) | `dhv-v{VER}-macos-aarch64.tar.gz` |
| Windows | x86_64 | `dhv-v{VER}-windows-x86_64.zip` |

## 🛡️ 当前版本

**v0.2.43**（工具链统一版本：dhv 0.2.43 · dhv-ts 0.2.43 · BNF v1.5.0 · 指南 v0.2.35）

- 版本号以 `toolchain/dhv/Cargo.toml` 与 `toolchain/dhv-ts/package.json` 为准，随每次功能/修复递增；
- 详见 [CHANGELOG.md](CHANGELOG.md)。

## 📄 许可证

MIT — 见 [LICENSE](LICENSE)（toolchain / guide / ide 各目录亦持有同许可副本）。
