# 变更日志（CHANGELOG）

本文件记录工具链版本演进；语言规范级变更另见 [toolchain/hsl-spec/BNF.md §8](toolchain/hsl-spec/BNF.md)。

## [0.2.15] — 2026-09-01 · 「Guide §5.8 rules 章节」版

### 新增
- **HSL-GUIDE.md §5.8 投射规则组 `rules {}`（BNF v1.5）**：
  完整文档化 rules 语法、R1-R6 语义六条、完整示例（含显式遮蔽演示）、
  跨模块展开示例。所有示例经 dhv check 与 dhv-ts check 双编译器实测通过。
- 目录新增 5.7（补录）与 5.8 条目。

### 变更
- 版本统一：dhv 0.2.15 · dhv-ts 0.2.15 · BNF v1.5.0。

## [0.2.14] — 2026-08-31 · 「值语境 range 对齐」版

### 新增
- **dhv-ts 值语境 range**（对齐 dhv，消除 BNF v1.5 已知限制 #10）：
  `let r = a..b;` / `let s = a..=b;` 现在在 dhv-ts 中作为一等值表达式正确解析、检查、解释执行。
  AST 新增 `Expr.kind = 'range'`；parser 新增 `parseRange()` 优先级层（assignment 与 or 之间）。
  解释器产 Range 描述对象，`for i in r`（r 为 range 变量）可正确迭代。
- 回归用例 `check/value_context_range.hsl`（双编译器一致：a..b / a..=b / 0..n 三种值语境 range）。

### 变更
- 版本统一：dhv 0.2.14 · dhv-ts 0.2.14 · BNF v1.5.0。
- 双编译器一致性：32 → 33 组用例（新增 `value_context_range`）。
- `value_context_ranges_dhv_only` 测试注释更新：不再标记为 dhv 独有能力。

## [0.2.13] — 2026-08-31 · 「模块体 S 检查」版

### 新增
- **dhv 依赖模块体级 S 系列检查**（对齐 dhv-ts「先链接后逐文件检查」）：
  `TypeChecker::check_module_body()` 对每个依赖模块重置每文件状态（symbols / imports / declared_items），
  共享跨模块注册表（enums / static_resources / module_items），降入 fn / graph / impl 体执行 S4/S6/S7/S8。
- **多文件诊断渲染**：`Diagnostic::file_hint` + `CompileResult::module_sources` + 链接器保存模块源码；
  CLI `cmd_check` 按诊断来源查找正确源码，显示准确的文件名 / 行列 / 源码摘录。
- 回归用例 `modules/fail_S7_module_body`（双编译器一致：依赖模块体未使用绑定 → S7）。

### 修复（dhv，全部以回归用例锁定）
- S7 导入使用标记遗漏：`block`/`static` 体内 `{{expr}}` 插值（如 `{{MAX_ITERATIONS}}`）未遍历 → 导入误报未使用。
  → `check_item` 遍历 `StaticResourceDef.content` 中的 `RawContentPart::Interpolation`。
- S7 导入使用标记遗漏：`graph` 参数类型和返回类型中的路径引用未遍历。
  → `check_graph` 新增参数/返回类型的 `walk_type` 调用（对齐 `check_fn`）。
- S7 导入使用标记遗漏：闭包参数类型注解（`|c: Citation| ...`）未遍历。
  → `walk_expr` 的 `Closure` 分支新增 `walk_type(&p.ty)`。

### 变更
- 版本统一：dhv 0.2.13 · dhv-ts 0.2.13 · BNF v1.5.0。
- 双编译器一致性：31 → 32 组用例（新增 `fail_S7_module_body`）。

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
