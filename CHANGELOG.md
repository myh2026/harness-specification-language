# 变更日志（CHANGELOG）

本文件记录工具链版本演进；语言规范级变更另见 [toolchain/hsl-spec/BNF.md §8](toolchain/hsl-spec/BNF.md)。

## [0.2.12] — 2026-08-31 · 「工程化与一致性」版

### 新增
- **投射规则组 `rules {}`（BNF v1.5 §3.4）**：`project` 块内按项类型批量投射，
  `{name}` 占位符；显式映射优先（R1）；声明校验 P5（占位符白名单 R2 / 类型唯一 R3 / 类型注册 R4）；
  展开池覆盖依赖模块导出项（R5）；展开项同等参与 P2/P4（R6）。dhv 与 dhv-ts 双端一致实现。
- **dhv 模块链接器（linker.rs）**：check 时解析 `import` 依赖闭包（BFS + 环检测 + 去重）；
  模块导出 enum / 静态资源进入跨模块注册表 → S6 穷尽性、P4 静态资源判定跨模块可见；新增诊断 M2。
- **测试基建**：`tests/run_conformance.sh` 双编译器一致性回归（31 组用例）；
  `dhv/tests/conformance.rs` fixture 驱动矩阵（parse / check / errors / modules + 内嵌用例）。
- dhv codegen：rules 展开驱动 emit；跨模块投射（agent.hsl 投射 model.hsl 导入项）。

### 修复（dhv，全部以回归用例锁定）
- if/while/match/for 头部**结构体字面量歧义**（`if x < lo { 1 }` 的 `lo {1}` 吞块）
  → no-struct 语境阶梯（BNF v1.5 §2.11.7），parser/typecheck 镜像。
- 语句级宏 `println!(..)` 实参不标记使用 → S7 误报。
- 结构体字面量**简写字段**不标记使用 → S7 误报；功能更新 `..base` 被静默丢弃。
- `edge .. on Guard` 守卫因 named 包裹层永不命中 → 导入误报未使用。
- S6 作用域修正：仅 graph 体内（AgentLoop 上下文）用户枚举 match 禁 `_` 兜底；
  普通 fn 体内 loop 不触发；消息去除硬编码 "Action"。
- `#[derive(..)] export struct` / `#[cfg(lang: rust)] impl` 前导属性文法支持。
- turbofish：`.collect::<Vec<String>>()` / `.parse::<f64>()`（method_call 增 `::<T>`）。
- native 块内字符串含 `//`（`https://...`）被全局 COMMENT 隐式空白吞掉
  → native_string/native_text 改原子规则。
- struct_expr_field 裸 integer_literal 形态移除（对齐 dhv-ts 启发式）。
- check ≠ emit：check 命令不再驱动代码生成（E0900 能力缺口不阻塞校验）。

### 修复（dhv-ts）
- `block`/`static` 关键词形状前瞻：`rules { block -> ... }` 不再误入原始资源区模式。
- for-in 与切片位置 `..=` 闭区间（词法单 token）解析支持。

### 变更
- 仓库规范化：目录重构（toolchain/ guide/ ide/）、.gitignore、构建产物出库（1503 → 104 tracked files）。
- 版本统一：dhv 0.2.12 · dhv-ts 0.2.12 · BNF v1.5.0。
- 三平台预编译二进制随 GitHub Releases 分发（linux-x86_64 / macos-aarch64 / windows-x86_64）。

## [0.2.11] — 此前
- range 表达式（0..5 / a..=b / n.. / ..n）进入 dhv 文法 for-in 与切片位置（见 BNF v1.5 §2.11）。

## [0.2.10] — 此前
- dhv-ts 大规模文法/检查器修复（详见 BNF v1.4.x 变更记录）。
