# 变更日志（CHANGELOG）

本文件记录工具链版本演进；语言规范级变更另见 [toolchain/hsl-spec/BNF.md §8](toolchain/hsl-spec/BNF.md)。

## [0.2.22] — 2026-09-01 · 「Release CI 再修复」版

### 修复
- **release.yml cargo-zigbuild 安装**：`cargo install cargo-binstall && cargo binstall cargo-zigbuild`（binstall 需先编译，极慢且可能超时）→ `taiki-e/install-action@cargo-zigbuild`（直接下载预编译二进制，Rust 社区标准方式）
- **v0.2.21 Release 失败回退**：删除 v0.2.21 tag（CI 因 binstall 超时失败），改用 v0.2.22 重新触发

### 变更
- 版本统一：dhv 0.2.22 · dhv-ts 0.2.22 · BNF v1.5.0。

## [0.2.21] — 2026-09-01 · 「Release CI 修复」版

### 修复
- **release.yml 交叉编译工具链**：`pip install ziglang`（不可靠）→ `goto-bus-stop/setup-zig@v2` action（固定 0.13.0）；
  `cargo install cargo-zigbuild --locked`（从源编译，慢且脆）→ `cargo binstall cargo-zigbuild`（预编译二进制）
- **release.yml CHANGELOG 发布说明提取**：awk 模式修正为匹配 `## [x.y.z]` 方括号格式（此前无方括号导致永远命中兜底）
- **release.yml 构建验证**：新增 artifact 存在性检查（缺失时立即报错而非静默上传空文件）
- **release.yml 缓存**：交叉编译目标也启用 Cargo 缓存（此前 `if: !matrix.cross` 跳过）

### 变更
- 版本统一：dhv 0.2.21 · dhv-ts 0.2.21 · BNF v1.5.0。

## [0.2.20] — 2026-09-01 · 「Go 后端完整转译」版

### 改进
- **dhv Go 后端函数体大幅扩展**（codegen/go_backend.rs，~750 行，此前 ~370 行）：
  - 新增表达式：if/else（语句级 + 尾位置）、match→switch、for-in→for range、while→for、
    赋值/复合赋值、索引、切片、数组字面量、结构体字面量、闭包→func literal、
    return/break/continue、block/range/try/loop/if-let/while-let/async-block
  - 尾位置 if/else-if/else 链正确生成（递归展开，无闭包包裹）
  - 循环体尾表达式不生成 return（仅语句级输出）
  - Vec 类型映射修复（消除双重泛型括号）
  - match→Go switch + default 兜底
- **v0.2.18 Release 触发**：`git tag v0.2.18` 已推送，GitHub Actions release.yml 自动构建四平台

## [0.2.19] — 2026-09-01 · 「文档与诊断」版

### 改进
- **S8 跨作用域遮蔽警告新增可操作 note**—— 跨作用域同名绑定遮蔽时，诊断消息附带「建议重命名」提示（与同作用域 S8 错误的 note 对齐）
- **Guide §13.3 已知限制清单更新**——
  - #3 更新：Go 后端已升级为 logic 级（v0.2.17）
  - #55 新增（已修复）：dhv-ts 值语境 range（v0.2.14 关闭）
  - #56 新增：dhv Go 后端函数体骨架转译范围说明
  - #57 新增：Go 后端类型映射近似说明
- **BNF v1.5 已知限制 #10 关闭**—— 值语境 range 双编译器均已支持
- **v0.2.18 Release 发布**—— 通过 `git tag v0.2.18` 触发 release.yml，四平台构建 + GitHub Release 自动创建

## [0.2.18] — 2026-09-01 · 「CI/CD」版

### 新增
- **GitHub Actions CI 工作流（.github/workflows/ci.yml）**—— 每次 push main 或 PR 自动运行：
  - `rust-test`：cargo test --release 回归矩阵
  - `conformance`：dhv ↔ dhv-ts 双编译器一致性（34 组用例）
  - `dhv-ts-suite`：dhv-ts 全量测试（fuzzing / 38 backends / CLI / stress）
  - `version-sync`：dhv(Cargo.toml) 与 dhv-ts(package.json) 版本号一致性守卫
  - Cargo 缓存加速、Bun 环境自动安装、并发去重（concurrency group）
- **GitHub Actions Release 工作流（.github/workflows/release.yml）**—— push tag v* 自动：
  - 四平台并行构建（linux-x86_64 / windows-x86_64 / macos-aarch64 / macos-x86_64）
  - 交叉编译通过 zig + cargo-zigbuild 实现
  - 从 CHANGELOG.md 自动提取对应版本发布说明
  - 创建 GitHub Release 并上传全部产物
  - release 后自动跑 cargo test + conformance 验证

## [0.2.17] — 2026-09-01 · 「Go 后端」版

### 新增
- **dhv Go 后端（codegen/go_backend.rs）**—— 专用 Go 代码生成后端（此前 Go 走通用契约后端，只生成注释）
  - struct → `type X struct { Fields... }`（字段首字母大写导出）
  - unit enum → `const (X = iota)`（Go 惯用）
  - 数据 enum → interface + 各变体 struct
  - fn → `func Signature { body }`（Go 函数签名 + 语句级骨架）
  - trait → Go interface（方法签名）
  - impl → Go 方法集（`func (self *T) Method()`）
  - graph → `func main() error {}` 入口函数
  - 类型映射：HSL int/float → Go int/float64，String→string，Vec→[]T，Option→*T，Result→(T, error)
  - 表达式级骨架：binary/unary/call/method/field/await/cast
  - 后端注册优先于通用契约后端，现在 6 个语言有专属后端（rust/go/python/typescript + 3 静态）

### 变更
- 版本统一：dhv 0.2.17 · dhv-ts 0.2.17 · BNF v1.5.0。

## [0.2.16] — 2026-09-01 · 「诊断信息质量提升」版

### 改进
- **dhv 诊断信息全面增加可操作建议提示（= note）**：
  - G2（edge 端点未声明）：建议添加 `node` 声明
  - P2（物理路径冲突）：建议为冲突项选择不同路径
  - P3（投射目标未定义）：建议确认定义或 import
  - P4×3（后端不合法 / block→代码 / 代码→静态）：分别建议 `dhv targets`、改静态后端、改编程语言
  - P5×4（未知规则类型 / 重复类型 / 占位符白名单 / 缺少闭合）：分别引用 R4/R3/R2
  - S7（未使用绑定）：建议 `_` 前缀命名豁免
  - S8（重复绑定）：建议重命名消冲突
  此前仅 S4/S6/S7(import)/G1/P6 有 note 提示，现所有主要诊断均附人类可读的修复建议。

### 变更
- 版本统一：dhv 0.2.16 · dhv-ts 0.2.16 · BNF v1.5.0。

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
