# DHV — HSL 编译器骨架 v0.1.0

> **HSL 是一门为编写 AI Agent harness 而生的编译型语言。你用 HSL 写逻辑，
> DHV 编译器将其转译为一个包含多语言代码、配置、文档的真实工程仓库。**

## 工程结构

```text
dhv/
├── Cargo.toml
├── src/
│   ├── hsl.pest          # P0 — PEG 文法（与 hsl-spec/BNF.md 逐条对应）
│   ├── ast.rs            # P1 — AST 类型定义（全部语法节点 + Span）
│   ├── parser.rs         # P2 — pest Pair 树 → 强类型 AST
│   ├── typecheck.rs      # 严格性 S1-S8 / 拓扑 G1-G6 / 投射 P1-P7 校验
│   ├── codegen/
│   │   ├── mod.rs        # CodegenBackend trait + 投射驱动
│   │   ├── rust_backend.rs   # P3 — Rust 后端
│   │   ├── python.rs         # P5 — Python 后端
│   │   ├── typescript.rs     # P7 — TypeScript 后端
│   │   └── static_res.rs     # P4 — YAML / Markdown / JSON 后端（已完整可用）
│   ├── sourcemap.rs      # P6 — @dhv:source-map 围栏 + 实时反编译回写
│   ├── diagnostics.rs    # 诊断系统（S/G/P/N/M/L 系列错误码）
│   ├── lib.rs            # 编译管线编排
│   └── main.rs           # CLI（parse / check / emit / watch）
└── tests/
    └── main.hsl          # 全量语法样例（BNF §7 附录）
```

## 构建

```bash
cargo build --release        # 产出 dhv 二进制
cargo test                   # sourcemap 回路测试
```

## 使用

```bash
# 解析并输出 AST 摘要
dhv parse tests/main.hsl

# 解析 + 严格性/拓扑/投射校验
dhv check tests/main.hsl

# 生成工程仓库（按 project {} 投射写入物理文件）
dhv emit tests/main.hsl -o generated/
```

## 编译管线

```text
HSL 源码
  │
  ▼
Parser（pest PEG → AST）
  │
  ▼
Type Check（编译期处决：零隐式转换 / 非空默认 / 强制错误处理 / 不可变优先
            + graph 拓扑校验 + project 投射一致性）
  │
  ▼
Multi-Target Codegen（rust / python / typescript / yaml / markdown / json）
  │
  ▼
Physical Writer + SourceMap（每个文件注入 @dhv:source-map 围栏）
  │
  ▼
工程仓库（多语言杂色项目）
```

## 路线图状态

| 阶段 | 状态 |
|:---|:---|
| P0 PEG 文法 | ✅ 完成（hsl.pest，与 BNF.md 对齐） |
| P1 AST 类型定义 | ✅ 完成（ast.rs 全量节点） |
| P2 Parser | ✅ 核心完成（项/类型/模式/表达式/graph/block/native/project/宏） |
| Type Check 严格性 | ✅ S1/S2/S4/S6/S7/S8 落地（S3 强制错误处理待类型推导）；G1/G2 + P2/P3/P4 |
| P3 Rust Codegen | 🟡 骨架（struct/enum/trait/fn 直译，表达式级主链路） |
| P4 YAML/MD/JSON | ✅ 静态资源转译完整可用 |
| P5 Python Codegen | 🟡 骨架（类型映射 + fn/struct + 表达式级主链路） |
| P6 双向工程 | 🟡 骨架（围栏注入/提取/回写闭环接口就绪） |
| P7 TS Codegen | 🟡 骨架（类型映射 + enum 判别式联合） |
| P8 跨语言胶水 | ⬜ 计划（edge → FFI/IPC/MCP 适配器） |
| P9 Lint 系统 | 🟡 骨架（诊断框架 + L 系列错误码） |
| P10 宏系统 | 🟡 文法完整，展开器计划中 |
| P11 包宇宙 | ⬜ 计划 |

## 严格性检查 v0.1.1（typecheck.rs）

| 规则 | 实现 |
|:---|:---|
| S1 零隐式转换 | ✅ if/while 条件字面量非 bool → 编译错误 |
| S2 非空默认 | ✅ 裸 `.unwrap()` → Lint 警告 |
| S3 强制错误处理 | ⬜ 需返回类型数据流（P3+） |
| S4 不可变优先 | ✅ 对不可变绑定赋值 → 编译错误 |
| S5 `?` 独占传播 | ✅ 文法层（v1.2-A 已删三元） |
| S6 穷尽 match | ✅ AgentLoop 内禁 `_` 通配 + enum 注册表穷尽性校验 |
| S7 未使用即错误 | ✅ let / import / graph node（`_` 前缀与 glob import 豁免） |
| S8 变量遮蔽 | ✅ 同作用域错误 / 跨作用域警告 |

另修复：project{} 对 graph/struct/enum/import 投射目标的 P3 误报；
native 块按 N1 变量捕获语义标记外层符号使用。

## 已知限制（pest 实现取舍）

- block 体内插值定界符 `{{` 后直接跟随 `//`（无空格）时会被行注释规则吸收——建议插值后留空格；
- 泛型约束的完整语义求解（trait bound 检查）在 P3+ 落地，当前骨架解析并保留约束 AST。

## License

MIT
