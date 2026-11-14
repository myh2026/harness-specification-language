# UPSTREAM-FEEDBACK — 用 HSL 写一个真实 harness 的实测反馈

> 本文是**给上游（语言/工具链作者）的反馈清单**，不是使用教程。
> 全部条目来自 `toolchain/examples/sentry/`（12 模块 / 1334 行 HSL，事故分诊 harness）的完整实操：
> `check` → `run`（scripted + fixture v2）→ `emit` 全流程。
> 每条都附**可复现命令**或**错误原文**；没有实测的不写。

## 环境

| 项 | 值 |
| --- | --- |
| OS | Windows 11（PowerShell 7） |
| Bun | 1.4.3（`C:\Users\86186\.bun\bin\bun.exe`） |
| 解释器 | `toolchain/dhv-ts/src/main.ts`（`dhv-ts v0.2.72`） |
| 语言 | HSL v0.2.72 / BNF v1.5.0，commit `04f8f7b` |
| 参考 | `toolchain/examples/dsh`（模板）、`guide/HSL-GUIDE.md`、`host.ts` |
| Rust 编译器 | 未参与本次实测（仅用 Bun 跑 dhv-ts；`cargo` 已就绪） |

---

## A. 问题清单（按严重度）

编号用于引用，不要求对应 issue 编号。P0 = 影响可用性/误导 AI；P1 = 明显摩擦；P2 = 文档/体验。

### A1 [P0] `$host.dhv.*` 在 Bun 上挂起，且无超时/NotImplemented

```hsl
let ok: bool = native typescript {
    const r = await $host.dhv.check({ file: "examples/sentry/sentry.hsl" });
    return r.ok;
};
```

- 现象：Bun 1.4.3 下**挂起 >60 s 不返回**，也不抛错；同样影响 `$host.dhv.run`。
- 影响：AI 会一直等/重试，浪费 token 与轮次（实测一次挂起即打断整个 run）。
- 建议：未实现就抛 `NotImplemented`；已实现则给内部超时（如 30 s）+ `AbortError`。
  退路：文档在 `dhv-ts/README.md` 的 host 表里明确标 "仅 Rust 侧可用"。

### A2 [P0] 单位类型 `()` 投射到 TS 非法

```hsl
fn write_artifacts(...) -> Result<(), HarnessError> { ... }
```

- 现象：`emit` 后 TS 语法校验失败，返回类型被渲染成字面量 `()`（TS 里是语法错误）。
- 规避（本项目已采用）：`main -> Result<bool, ...>`、`write_artifacts -> Result<u32, ...>`。
- 建议：`emit` 把 unit 类型渲染为 `void`（`Result<void, E>` / `Promise<void>`），
  或至少给 **warning 指明"unit 返回类型在 TS 目标非法，请改用 bool/u32 等哨兵值"**。
- **修复状态：已在本次工作区修复** —— `dhv-ts/src/backends/typrint.ts`：空元组
  （unit `()`）改按 `lang.types.unit` 映射（TS → `void`，rust → `()`）。探针
  `fn health() -> ()` emit 2/2 语法✓（`export function health(): void`），Rust 引擎
  emit 同形；回归 sentry emit 56/56、backends-demo emit 214/214 语法✓ 零告警。

### A3 [P0] Windows 上 `$host.shell.run` 依赖真 bash，失败信息不可读

- 现象链：`shell.run` 实现是 `execFile("bash", ["-c", command])`。Windows 上若
  `C:\Windows\System32\bash.exe`（WSL 存根）先被命中，会返回 UTF-16 乱码错误
  （如 `syntax error near unexpected token ...`），被计为 `failures`，AI 无法判断是
  命令错还是环境错。把 Git Bash 的 `bin` 加到 PATH 最前即恢复（本机 `D:\学习\py学习\Git\bin`）。
- 建议：spawn 前检测 `bash` 来源（版本/路径），失败时给出明确提示
  （"未找到 POSIX bash；Windows 请把 Git Bash 加入 PATH 最前"）；或允许 `--shell` 配置。
- 旁证：正因如此，sentry 的「工具瞬时失败重试」demo 第一条路曾失败 3 次才定位到环境问题。

### A4 [P0] 部分 S 严格性规则只在 run 阶段生效（`check` 绿 ≠ 可跑）

```hsl
let failed = false;
while ... { failed = true; }   // check 通过
```

- 现象：`check` 0 error，run 才报
  `✗ 运行期错误：不能对不可变绑定 "failed" 赋值（S4：不可变优先，请用 let mut）`。
- 建议：能静态判定的 S 规则尽量前移到 `check`（S1/S4 尤其重要）；不能前移的在
  check 输出里 warn（"该模式需 let mut，run 时才会报"）。
- 依据：本次 1334 行里几乎全部返工都发生在 run 阶段（S4、S1、enum 匹配、`format!` 等）。

### A5 [P1] 注解/属性完全不校验（`#[capability]`、`#[derive]`、doc 注释）

- 现象：`#[capability(file_read)]` 写错拼写、`#[derive(Debug)]` 写不可 derive 的类型、
  属性参数乱写，`check` 均 0 error 0 warning——"写了就以为生效"。
- 影响：对 AI 尤其危险，会把未生效的约束当契约。
- 建议：至少校验**已知属性名的参数形状**；未知属性 → warning；`capability` 的名字与
  `$host` 命名空间对应起来（如 `file_read` ⇄ `$host.fs.read`）。

### A6 [P1] `check` 无结构化/机读诊断

- 现状：文本输出带规则号（S/G/P/N/X）与 `file:line:col`，可 grep。
- 建议：`check --json` 输出 `[{severity, rule, file, line, col, message, quickfix?}]`；
  对 CI / AI 自循环价值极大（本次靠 `run.json` + `events.jsonl` 做自验证，缺对应的 check 侧）。

### A7 [P1] `format!` 说明符子集未文档化，且部分不支持

- 现象：`{:.1}`、`{:>4}`、`{:04}` 这类需要实测才能确认；不支持时的报错未指向"说明符表"。
- 建议：在 guide 里放一张**支持矩阵**；不支持的说明符在 check 阶段明确报"仅支持 {} / {{ }} / {:.N}"。

### A8 [P1] `HashSet` 是伪实现

- 现象：`HashSet` 能 check、能写，但语义不是集合（去重/成员判定不符合预期）。
- 建议：实现真语义（BTreeSet 也可），或 check 阶段**直接拒绝**并给出 `Vec` 替代写法。

### A9 [P1] `AgentLoop` 的 stop 必须逐变体显式匹配，`_` 不支持

- 现象：stop 枚举有 8 个变体就必须写 8 个 `Variant => {...}`；写 `_ => ...` 报错。
  典型 AI 写法会直接用通配，导致反复改。
- 建议：报错文案附一个"最小正确写法"示例；长远支持 `_`（末尾）。
- 同样问题：多分支 `match` 需要覆盖全部变体（枚举新增变体时，跨模块破坏性大），
  建议 guide 增加"枚举演进"小节。

### A10 [P1] `emit` 跨语言投射的告警风暴，缺少"允许/契约 stub"选项

- 现象：`rules { fn -> : python }` 与其他模块的 TS 类型/函数互引时，`X-1`（类型引用未投射）、
  `X-5`（函数/变体未投射）大量告警；本项目最终回退为 **全 TypeScript 投射**（56 文件、
  0 告警）。跨语言引用时 `emit` 只有"告警"这一种反馈，没有"自动生成契约 stub"的选项。
- 建议：`emit --cross-language=stub`（生成 `declare`/`interface` 骨架）或文档明确
  "projection 图必须语言闭合"的约束与推荐做法。
- 另：`--no-validate` 仍打印 "N 个通过语法校验"，易被 CI 误读；建议改为 "语法校验: 跳过"。

### A11 [P2] `emit` warning 即 exit 1（设计合理，但缺 summary 语义）

- 建议：exit 1 的同时输出 `errors/warnings/files` 汇总行（本次靠解析 stdout 判断，脆）。

### A12 [P2] BOM 未容错：checker 报 `期望项定义，得到 "﻿" (file:1:2)`

- 复现：PowerShell `Set-Content -Encoding UTF8` 存 `.hsl` → 文件带 BOM → check 报上述错误（列号还偏差 1）。
- 建议：词法器入口 strip U+FEFF（很多语言/编译器都这么做）；或报一行 "检测到 UTF-8 BOM，
  请以无 BOM 保存"。
- 顺带：错误信息里的 `"﻿"` 是不可见字符，人/AI 都难排查。
- **修复状态：已在本次工作区修复** —— `dhv-ts/src/lexer.ts` 与 `dhv/src/parser.rs`
  入口同码 strip U+FEFF；带 BOM 探针 `check` 双引擎（dhv-ts 0 error 0 warning、
  dhv Rust ✓ 校验通过）均通过。

### A13 [P2] `dhv sync` 默认 `--root` 与直觉不符

- 现象：回写源码时若不带 `--root`，按"生成文件所在目录"解析，导致找不到 target/写错位置；
  建议默认值按 `manifest.json` 的源路径解析，或在错误里直接提示加 `--root`。

### A14 [P2] `graph`/`node` 的 state 生命周期与初始化时机文档不足

- 现象：node 间 state 是按值还是按引用、node 入口是否重新求值，需反复 run 才能确认；
  guide 的描述偏抽象。建议补一节 + 最小可运行示例（node 里改 state 后下游看到什么）。

---

## B. 给"AI 写 harness"场景的能力建议（优先级排序）

1. **可机读诊断**：`check --json`（含 `rule/code/quickfix`）；AI 才能自循环修复。
2. **把"看着能跑其实不行"的语义差异前移**：`HashSet`、`format!` 说明符、`AgentLoop` 形态、
   未校验的注解——要么实现、要么 check 阶段明确拒绝并给修复示例。
3. **host API 稳定性**：任何未实现 API 立刻 `NotImplemented`，不要挂起；全部给超时。
4. **`run --json` 摘要**：现在 `run.json` 只有 `ok/error`；建议补 `verdict/turns/tool_calls/
   artifacts[]/events_counts`，AI 做"写→跑→改"闭环时不用再解析事件流。
5. **错误码 → 修复示例索引**：`guide/` 之外加一页 "S/G/P/N/X 速查 + 正确写法"，
   每码 3-5 行示例即可；这是 AI 最缺的文档形态。
6. **fixture v2 能力上首页**：tracks + faults + scripted model + `run.json`/`events.jsonl`
   是 HSL 与其它 agent 框架最差异化的点，也应成为"AI 自验证"卖点。

---

## C. 已验证可用、值得信任的能力（建议保留）

- `fixture v2`：`tracks`（命名轨道 + 耗尽抛错）、`faults`（按 `target` + `nth` 注入
  `error|deny|empty|corrupt|slow`）、`scripted` 模型确定性回放——实测一次注入 `shell.run`
  瞬时错误，HSL 侧重试成功，事件流正确出现 `fault_injected`。
- `run` 产物：`run.json`、`events.jsonl`、`transcript.jsonl`、`artifacts.write` 落 `--out`；
  `fs.*` 严格限制在 `--workspace`（越狱抛错）。
- `emit`：`project{}` + `rules{}`（v1.5 R1-R6）实测 0 告警；`manifest.json` 逐文件
  `lang/tier/bytes/items/contract_fallbacks/syntax_check`，纯 HSL 函数 `full` 活体翻译、
  `native` 函数进 `contract_fallbacks` 契约骨架——对"一份 spec 多语言"很实用。
- `$host.make("Entity", {...})`、`$host.json.fields`、`$host.artifacts.write`、`$host.events.emit`
  按文档工作；`--scale microkernel|monolith` 只影响事件 `scale` 字段与 emit 脚手架（不会改变拓扑）。
- 诊断信息本身质量高（规则编号 + 中文说明 + file:line），S-1..S-19 有明确编号可查。

---

## D. 复现步骤（本项目实测命令）

```powershell
cd D:\下载\hsl\toolchain

# 1) 静态检查（0/0 即通过）
bun dhv-ts/src/main.ts check examples/sentry/sentry.hsl

# 2) 确定性回放（复制 workspace 后跑）
bun dhv-ts/src/main.ts run examples/sentry/sentry.hsl `
  --workspace D:\temp\sentry-ws --task "checkout 支付链路自 02:13 起大面积超时" `
  --model scripted --fixture examples/sentry/fixtures/triage-demo.json --out D:\temp\sentry-out

# 3) 投射（exit 0 才是通过）
bun dhv-ts/src/main.ts emit examples/sentry/sentry.hsl --out D:\temp\sentry-emit

# 4) 一键（check → 回放 → 预算耗尽 → emit）
powershell -ExecutionPolicy Bypass -File examples/sentry/scripts/demo.ps1
```

预期：check `0 error(s), 0 warning(s)`；run `verdict: accepted / SEV-1 / turns 9`；
emit `56 个文件（56 通过语法校验）`；demo `exit 0`。

---

## E. 与上游文档的对应位置

| 反馈 | 相关文件 |
| --- | --- |
| host API / fixture v2 / shell.run / fs jail | `toolchain/dhv-ts/src/host.ts` |
| run 产物 / 事件 / artifacts | `toolchain/dhv-ts/src/main.ts`、`host.ts` |
| emit / rules{} / manifest | `toolchain/dhv-ts/src/emit.ts`、`manifest.json` |
| 语言限制与陷阱 | `guide/HSL-GUIDE.md`、`toolchain/hsl-spec/BNF.md` |
| 双实现差异 | `toolchain/hsl-spec/COMPLIANCE.md` |
| 本次逐条实测明细 | 同目录 `RUNTIME-NOTES.md`（438 行）、`LANG-NOTES.md`（579 行） |

> 若上游跟进：本目录的 `sentry` 示例本身就是"12 模块 + fixture 故障注入 + 双模型审计 + 全量
> 投射"的现成回归用例，可直接接入 CI 的示例测试矩阵（像 dsh 那样加一条 check+run 冒烟即可）。
