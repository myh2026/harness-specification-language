# dhv-ts — HSL 参考解释器与 38 后端投射器

> **Harness Specification Language** 的可执行参考实现。
> 你写 HSL，它真的跑起来——包括接真实 LLM 的 Agent harness；
> 你声明 project{}，它把逻辑真实转译到 38 个后端语言。

```
  ┌─────────────────────────────────────────────────┐
  │  dhv-ts — HSL 参考解释器          v0.2.52       │
  │  Harness Specification Language · BNF v1.4.7    │
  │  38 后端：32 编程语言 + 6 静态格式               │
  └─────────────────────────────────────────────────┘
```

## 这是什么

`dhv`（Rust 编译器）把 HSL 静态投射到六种后端；`dhv-ts` 是它的姊妹项目——一个
**树遍历解释器**，让 HSL 程序在没有 Rust 工具链的环境里直接执行：

| | dhv（Rust） | dhv-ts（本项目） |
|:---|:---|:---|
| 定位 | 生产编译器 | 参考/开发运行时 + 38 后端投射器 |
| 执行方式 | 静态投射 | 逐 AST 解释执行 + emit 多目标生成 |
| 后端数 | 38（注册表 + 核心后端 + 通用契约后端） | 38（full 3 / logic 3 / contract 26 / 静态 6，宿主工具链交叉校验） |
| 类型检查 | 全量（S/P/G 规则 + 类型推导） | 结构级铁律（S1/S2/S4/S6/S7/S8 + G/P/N 子集——v0.2 全部真实生效） |
| `native` 块 | 按目标语言生成胶水（P5） | **运行期真实执行**（typescript 进程内 / python 子进程） |
| 标准库 | std 方法面 | **10 模块**（core/collections/text/math/io/json/time/random/env/iter） |
| 双向工程 | watch（源文件） | **sync 围栏回写 + watch**（@dhv:hsl-mirror 三标记协议） |
| 依赖 | Rust 工具链 | `bun`（零第三方依赖；LLM 网关经 z-ai-web-dev-sdk） |

类比：dhv 之于 dhv-ts，如同 GCC 之于 CPython——一个负责产物，一个负责"现在就能跑"。

## 快速开始

```bash
# 1. 静态检查（S/G/P/N 规则 + 模块链接）
bun dhv-ts/src/main.ts check <entry.hsl>

# 2. 解释执行（入口约定：入口文件里的 fn main()，BNF v1.3 R-1）
bun dhv-ts/src/main.ts run <entry.hsl> [options]

# 3. 投射工程仓库：38 后端真实代码生成 + 静态资源 + manifest + 交叉语法校验
bun dhv-ts/src/main.ts emit <entry.hsl> --out DIR [--scale MODE] [--no-validate]

# 4. 列出全部 38 个后端语言（tier / 能力级 / 扩展名）
bun dhv-ts/src/main.ts targets

# 5. 双向工程：生成文件的围栏 HSL 镜像 → 回写 .hsl 源码
bun dhv-ts/src/main.ts sync <generated-file> [--root DIR]

# 6. watch 模式：.hsl 变化 → 自动 check + emit
bun dhv-ts/src/main.ts watch <entry.hsl> --out DIR
```

**run 的常用参数**：

```
--workspace DIR     Agent 工作区（路径监狱：所有 fs 操作限制在内）
--task TEXT         任务描述
--model NAME        scripted（确定性剧本）| deepseek（真实 LLM）
--fixture FILE      剧本 JSON（scripted 模式）
--temperature F     采样温度（默认 0.2）
--max-turns N       主循环轮数上限（默认 24）
--max-bash N        bash 调用上限（默认 12）
--allow a,b,c       shell 首词白名单（默认 bun,node,ls,cat,grep,diff）
--scale MODE        microkernel | monolith（影响 G6 边事件观测）
--out DIR           产物目录（默认 .hsl-runs/<timestamp>）
--quiet             静默轨迹
```

## 架构

```
.hsl 源文件
   │
   ▼
lexer.ts ──────► token 流（含 §1.9 双模式原始区：native 体 / block 体+{{}}插值）
   │
   ▼
macro.ts ──────► macro_rules! token 级展开（定义先于使用）
   │
   ▼
parser.ts ─────► AST（BNF §2/§3 全构件：graph/edge/node/project/scale/block/native）
   │
   ├──► checker.ts ──► 诊断（S2/S4/S6/S7/S8 · G1-G4 · P3/P4/P6 · N1 · 模块链接）
   │
   ├──► interp.ts ──► 树遍历执行（作用域/模式匹配/闭包/?+From/trait 派发/graph 执行）
   │        │
   │        ├──► builtins.ts ──► std 方法面（BNF 附录 A）
   │        └──► native.ts  ──► 逃生舱桥（附录 B ABI）
   │                 ├── typescript：进程内 new Function + $host + 按名捕获注入
   │                 └── python：python3 子进程 + JSON 编组 + 末表达式变换
   │
   └──► host.ts ──► 宿主 API（llm 网关 / fs 路径监狱 / shell 白名单 / json 桥 /
                     事件总线（G6）/ 剧本装置 / 产物写出）
```

## 已验证的真实运行

| 项目 | 模式 | 结果 |
|:---|:---|:---|
| `examples/dsh`（DS harness 复现：多 Agent 编码助手） | scripted 剧本 | ✅ 50ms 确定性全链路（读文件→修 bug→跑测试→审查→报告） |
| `examples/dsh` | deepseek 真实 LLM | ✅ 13.6s 真实智能体循环（含协议纠错回路与安全闸门拦截记录） |
| `dhv-ts/examples/smoke.hsl` | — | ✅ 14 类构件冒烟（枚举/闭包/宏/native 双语言/进制字面量…） |

真实 LLM 运行的事件流（节选，完整见 `examples/dsh` README）：

```
run_start → node(model/executor/reviewer) → edge(model→executor on Tool)×7
→ observed×7 → capability_denied×2（模型尝试 deno/cd 被白名单拦截）
→ edge(model→reviewer on Done) → run_end(ok)
```

## native 逃生舱（BNF v1.3 附录 B）

```hsl
fn read_file(policy: Policy, path: String) -> Result<ToolResult, HarnessError> {
    let r = native typescript {
        const t0 = Date.now();
        try {
            const content = await $host.fs.read(path);   // 捕获变量按名注入
            return { ok: true, body: content, error: "", ms: Date.now() - t0 };
        } catch (e) {
            return { ok: false, body: "", error: String(e && e.message || e), ms: 0 };
        }
    };
    ...
}
```

- 捕获变量按名注入（`path`、`policy` 直接可用；**self 的字段要写 `self.x`**）
- `$host` 是宿主命名空间（LLM 网关 / 沙箱 fs / 白名单 shell / JSON 桥 / 事件总线）
- 无 `return` 的 typescript 体自动按末表达式返回；python 体自动变换末行

## 已支持构件（BNF 覆盖清单）

- **项**：struct / enum（含判别式与元组变体）/ trait（含默认实现）/ impl（含 `impl Trait for T`、From 特化）/ fn（async/self/mut）/ const / typealias / import（三种形态）/ export / graph / block+static 资源 / macro_rules!
- **表达式**：全优先级链 / struct 字面量（简写、`..base`）/ 闭包 / if-let-else / match（守卫、or-、范围、rest）/ 循环族（标签 break/continue）/ `?`（Result+Option，From 自动转换）/ `.await` / `as` 显式转换 / 切片 / 宏（format!/vec!/println!…）
- **HSL 专属**：graph（参数/node/edge/on Guard/with 属性/AgentLoop）/ project / scale / native 双语言 / `{{}}` 插值 / `#[capability]` 等属性 / G6 边事件追踪
- **铁律（运行期强制）**：S1（if/while 条件必须 bool）/ S4（不可变绑定赋值报错）/ S6（match 不穷尽报错）

## 已知限制（诚实声明）

- 无完整静态类型推导（类型注解在运行期基本忽略；`?` 的 From 转换按标注的返回类型驱动）——这是 dhv Rust 编译器的职责
- impl 方法解析按类型名全局注册（跨模块同名类型会冲突）
- 宏展开支持基础 frag（ident/literal/tt/expr/ty）与重复，复杂 matcher（嵌套重复、多规则优先级）未覆盖
- 赋值索引仅支持字面量/变量下标（`arr[i] += 1` 的 i 为复杂表达式时不支持）
- `native rust` / `native yaml` 等其余后端运行期不可执行（明确报错，指向 dhv 静态投射）

## 目录

```
dhv-ts/
├── src/
│   ├── main.ts     CLI（check / run / emit）
│   ├── lexer.ts    词法（含原始代码区双模式）
│   ├── parser.ts   递归下降解析（BNF 全构件）
│   ├── ast.ts      AST 定义
│   ├── macro.ts    macro_rules! token 级展开
│   ├── interp.ts   解释器核心（graph 执行 + 边追踪）
│   ├── builtins.ts std 方法面（BNF 附录 A）
│   ├── native.ts   native 逃生舱桥（附录 B）
│   ├── host.ts     宿主 API（$host）
│   ├── checker.ts  静态检查（S/G/P/N）
│   └── linker.ts   模块加载（文件即模块，M1-M5）
└── examples/
    └── smoke.hsl   冒烟样例
```

## v0.2.10 变更摘要（历史）

> 当前版本变更见仓库根 [CHANGELOG.md](../../CHANGELOG.md)（版本单一来源：`dhv-ts/package.json`，横幅/manifest/文件头注释均由 `src/version.ts` 读取）。

**真机工具链编译级验证轮**——首次安装 rustc 1.98 / go 1.27 / JDK 21 / kotlinc 2.4.10
对全部生成代码真机编译（此前 16 轮 go/rust/java/kotlin 仅结构断言），修复 7 个
真机编译错误级 bug；dhv Rust 编译器同步完成首次真实构建（25+ 编译错误闭合）。

- **修复：go 后端 `unwrap_or` 生成三元表达式（go 无此语法）**：此前
  `(recv != nil ? *recv : d)` 真机 go build 报 invalid character U+003F；
  现走 `_dhvUnwrapOr[T any](opt *T, def T) T` 泛型助手（表达式位置合法 +
  单次求值 + 副作用接收者安全）
- **修复：go 同 package 多文件顶级助手重复声明**：emit 多个 go 文件时
  `_dhvSome/_dhvPop/...` 每文件重复声明（重定义编译错误）。现跨文件共享状态
  去重——助手仅注入首个 go 文件，其余仅 import
- **修复：go 未使用 import（严格规则）**：import 按正文实际引用裁剪
  （`trimGoImports`；全部未用省略 import 块）
- **修复：go `len()` 与 i64 类型不匹配**：`int64(len(x))` 统一归一（go 内建
  len 返回 int，与 HSL i64 映射混算报 mismatched types）
- **修复：go 尾兜底 return 不可达**：body 尾已是 return 时跳过零值兜底
  （go vet unreachable）
- **修复：rust format! 双重格式化**：此前表达式实参嵌入 `{...}`（如
  `{args.len()}` 内联捕获不支持方法调用）且同时传位置参数——rustc 报 invalid
  format string。现纯标识符 → 内联捕获 `{name}`（不重复传参）、表达式 →
  位置 `{}` + 参数列表
- **修复：rust 头部缺 `use std::collections::HashMap`**（E0425）
- **dhv Rust 编译器首次真实构建**（hsl.pest 语法修复 + 12 处 named 包裹层
  静默丢弃 bug + 尾逗号 + dyn 单 bound + 路径模式守卫等 25+ 错误；
  cargo build 0 warning 0 error，parse/check/emit 全链路真实工作）
- **测试套件 105→109**（+4 全真机编译级）：rust format（rustc）/ rust
  HashMap（rustc）/ go 助手去重+import 裁剪（go build + go vet）/ javac
  编译级；另两既有测试升级真机 go build

## v0.2.9 变更摘要

- **修复：interp `"".parse::<f64>()` 返回 Ok(0) 的 JS 语言怪癖泄漏**：此前经 JS
  `Number("") === 0` 隐式返回 Ok(0)（与整数路径 `"" → Err` 不一致）。现空串统一
  Err（与 Rust 语义一致），整数/浮点两路径行为闭合
- **修复（一整类）：裸 `Option::None`（无注解 let 中转）链式方法在 cpp/go 生成非法代码**：
  `Option::None.map(f)` 此前 cpp 生成 `_dhvOptMap(std::nullopt, f)`
  （`std::nullopt_t` 模板推导失败，编译必炸）、`Option::None.unwrap_or(0)` 生成
  `std::nullopt.value_or(0)`（nullopt_t 无成员）、go 生成 `*nil`（untyped nil
  解引用非法）—— 均通过启发式平衡校验但真机必炸（v1.4.8 测试全用带注解 let 故未暴露）。
  现 None 字面量接收者专门派发：`None.unwrap_or(d) ≡ d`、`None.or(alt) ≡ alt`、
  `None.is_some/is_ok ≡ false`、`None.is_none/is_err ≡ true`（单次求值精确）；
  cpp map/and_then/filter/unwrap_or_else 用 `_dhvNoneT` 链式包装器（恒等化简 +
  `operator std::optional<T>` 隐式转换）；unwrap/expect 与 go 闭包族诚实回退 contract
- **修复：exprKind 两段 builtin 路径值返回 unknown**：`Option::None.filter(f)` 此前
  走 Vec 分支回退 contract。现 `case 'path'` 识别两段 `Option::*`/`Result::*`
- **String::parse::<T>() turbofish 泛型实参首次接线**（此前全部语言回退 contract，
  含原生支持的 rust）：rust 原生 `.parse::<i64>()` 直投；python
  `_dhv_parse_int(s, ty)`（严格正则 + u 型拒负；float 手工实现 JS Number 语义子集）；
  ts/js `_dhvParseInt/_dhvParseFloat`（与 interp 同为 JS 实现，天然同源）；
  cpp `_dhvParse<T>` 模板助手（stoll/stod + 全消耗检查 + unsigned 拒负）；
  go `_dhvParseInt(s, unsigned)/_dhvParseFloat`。生成端非 rust 语言采用
  Option-flavored Result 表示（Err → None/null/nullopt/nil），链式消费
  （unwrap_or/unwrap/expect/is_ok/is_err）与 Option 映射无缝衔接；
  **is_ok/is_err 首次进入 METHOD_TABLE**（rust 原生）
- **Option::filter 新增**（interp builtin + rust 原生 + py/ts/js/cpp 助手；全部
  单次求值，副作用接收者安全；go 诚实回退 contract）
- **Vec::sort_by 扩至 cpp/go**：cpp `std::stable_sort` + 泛型 lambda comparator
  （key 语义；稳定序与 interp/rust 同源，断言禁止退化非稳定 std::sort）；
  go **闭包体内联替换**技术首秀 —— `sort.SliceStable(v, func(i, j int) bool
  { return v[i].score < v[j].score })`（substParam 深克隆表达式树，把闭包参数
  引用替换为 `v[i]`/`v[j]`，绕过 go func literal 需显式类型的结构性限制）
- **char 谓词 is_alphabetic/is_numeric 扩至 ts/js/cpp/go**：ts/js 与 interp 同源
  正则；cpp/go `_dhvIsAlpha/_dhvIsDigit`（UTF-8 首字节 ≥ 0x80 判非 ASCII，
  与 interp 正则语义精确对齐）
- **测试套件 96→105 用例**（+9：parse turbofish 全语言活体 / cpp parse g++ 语义级
  11251 / python parse exec 101151 / interp 空串修复回归 111 / Option::filter
  interp+py+cpp / cpp 裸 None 链 g++ 113 + 回归断言 / cpp sort_by 稳定序 1020/2131 /
  go sort_by 闭包内联替换结构 / char 谓词 g++ 11/111 含 UTF-8 é）
- **已知限制清单 43→48 条**（+5：空串 parse 修复 / 裸 None 链修复 / exprKind 两段
  路径修复 三条已闭合；新增 #47 parse Option-flavored Result 表示边界、#48
  parse 浮点接受面 cpp/go 边缘差异 —— 均诚实文档化）

## v0.2.8 变更摘要

- **修复：vec! 宏 / 数组字面量在 cpp 生成 lambda 捕获非法语法**（编译必炸的隐藏 bug）：
  `vec![1, 2]` 此前 cpp 生成 `[1, 2]`（C++ 中是 lambda 捕获表达式，非数组字面量）——
  g++ 报 "expected identifier before numeric constant"。现 cpp 用 CTAD `std::vector{1, 2}`
  （C++17 class template arg deduction 自动推导 `std::vector<int>`）；go 同步从 `[1, 2]`
  （固定数组）升级为 `[]any{1, 2}` 切片字面量（与 Vec 切片头语义对齐）
- **修复：cpp 闭包缺外层变量捕获导致编译错误**：`Option::Some(first).map(|x| x + last)`
  此前 cpp 生成 `[](auto x) { return x + last; }` —— `last` 未捕获，编译错误。现 cpp
  闭包用 `[&](auto x) { return ...; }`（按引用捕获所有外层变量，与 Rust 闭包默认行为一致）
- **修复：cpp extend/append 临时变量迭代器不同源导致 `vector::_M_range_insert` length_error**：
  `v.extend(vec![5])` 此前 cpp 内联 `(std::vector{5}).begin(), (std::vector{5}).end()` ——
  两个 `std::vector{5}` 临时对象不同源，迭代器非法范围 → 运行期 `std::length_error`。
  现 cpp 用 `_dhvExtend(v, arg)` 模板助手（const ref 绑定临时，保证 begin/end 同源）
- **修复：exprKind 不能识别 Option::Some/None、Result::Ok/Err、Vec::from、HashMap::new、vec! 宏**：
  `Option::Some(first).map(|x| x * 10)` 此前因 `Option::Some(first)` 的 kind 为 unknown
  → map 走 Vec 分支 → cpp/go 无映射 → 退化为 contract。现 exprKind 新增 `case 'call'`
  识别 `Option::Some/None`/`Result::Ok/Err`/`Vec::from`/`HashMap::new` 路径调用的
  返回 kind；`case 'macro'` 识别 `vec!` 宏返回 'vec' kind —— 后续方法分发可正确感知
- **Option 链式家族扩至 cpp/go**（Task 21 移交建议①）：map / and_then / or /
  unwrap_or_else / expect 五方法。cpp 用模板助手 `_dhvOptMap`（`std::optional<decltype(f(*opt)>`）
  / `_dhvOptAndThen`（lambda 须返回 std::optional）/ `_dhvOptOr`（同型选择）/ 
  `_dhvOptUnwrapOrElse`（零参 lambda）/ `_dhvOptExpect`（throw std::runtime_error）；
  go 仅扩非闭包方法 `_dhvOptOr` / `_dhvOptExpect`（HSL 闭包无类型注解，go func literal
  需显式类型 —— 诚实回退 contract；map/and_then/unwrap_or_else 暂不映射 go）
- **Vec::sort/is_sorted/clear/extend/append 扩至 cpp/go**（Task 21 移交建议②）：
  cpp sort → `std::sort(v.begin(), v.end())` / is_sorted → `std::is_sorted(...)` /
  clear → `v.clear()` / extend+append → `_dhvExtend(v, arg)` 模板助手；go sort →
  `slices.Sort(v)` / is_sorted → `slices.IsSorted(v)` / clear → `v = nil` / extend+append →
  `v = append(v, (arg)...)`（与 interp spread 语义对齐）
- **测试套件 88→96 用例**（+8：cpp Option::map/and_then g++ 编译+运行 10/8/-1 /
  cpp Option::or/unwrap_or_else/expect g++ 运行 42/99/7 / go Option or/expect 助手族结构 /
  cpp Vec::sort/is_sorted/clear/extend/append g++ 编译+运行 105/5/0 / go Vec 助手族结构 /
  cpp vec! 宏字面量 CTAD 修复 g++ 运行 60/7 / cpp Option::or 链 + unwrap_or g++ 运行 42/-7 /
  cpp 综合场景 Option 链 + Vec 方法族 g++ 运行 13/6）
- **已知限制清单 38→43 条**（+5 新增，全部 v0.2.8 已修复：vec! cpp lambda 捕获非法 /
  cpp 闭包缺外层捕获 / cpp extend 临时迭代器不同源 / exprKind 路径调用盲区 / Option 链式家族
  cpp/go 缺映射）—— 全部带 g++ 编译+链接+运行实测级验证

## v0.2.7 变更摘要

- **修复：String::contains 在 cpp/go 生成编译错误代码（类型感知分发缺口）**：
  `s.contains("x")` 此前 cpp 走 Vec 表生成 `std::find(s.begin(), s.end(), "x")`
  （char 与 const char* 比较 = g++ 编译错误，实测复现）、go 生成
  `slices.Contains(s, "x")`（string 非切片 = 编译错误）—— 均通过启发式平衡校验
  但真机编译必炸。现 contains 类型感知分发（str → 子串查找 / Vec → 迭代器查找）
- **修复：let 块初始化全语言能力缺口**：`let base = if b { 100 } else { 0 };` 在
  interp 一直可用，但生成端全部 7 语言回退 contract。现走「声明 + 分支尾赋值」
  模式（python 分支内赋值 / ts-js `let x;` / rust `let x;` 延迟初始化 / cpp-go
  按分支值推导类型或需注解）；asValue 机制扩展为 boolean | string 全链路贯通
- **Vec::insert / Vec::remove 活体映射扩至 cpp/go**：cpp `_dhvInsert`（越界
  clean throw）/ `_dhvRemoveAt`（返回被删元素）；go `_dhvInsert(&v,...)` /
  `_dhvRemoveAt(&v,...)`（指针副作用通道）
- **HashMap 全表面活体映射扩至 cpp/go**：insert / contains_key / keys / values /
  get / remove 六方法全部活体（cpp 模板助手 + go 泛型助手）；go remove 从匿名函数
  `any`（链式 unwrap_or 解引用 any 是编译错误）升级为 `_dhvMapRemove` 返回 *V
- **Vec::get / HashMap::get Option 语义扩至 cpp/go（关闭 v1.4.3 遗留「下标近似」）**：
  kind 感知分发 —— vec 越界 / map 缺键 → nullopt/nil（与 interp None 对齐；
  此前 cpp 越界 UB / go 缺键零值）
- **String 方法族活体映射扩至 cpp（12 方法全表面）**：trim / to_lower / to_upper /
  starts_with / ends_with（C++20 原生）/ replace / split / split_whitespace /
  lines / char_count（UTF-8 码点计数）/ repeat / join —— C++ 标准库无这些
  便捷函数 → 内联助手族
- **translator-tour 扩容**：新增 vec_surgery（insert/remove/get 链）+ map_census
  （HashMap 全表面 + let 块初始化）函数 + main 巡览段
- **测试套件 82→88 用例**（+6：String::contains 类型感知 g++ 编译运行 / cpp
  Vec::insert+remove+HashMap 全表面 g++ 运行 / cpp-go get Option 语义 / let 块
  初始化全活体+python exec / cpp String 方法族 12 方法 g++ 运行 / go 助手族结构断言）

## v0.2.6 变更摘要

- **修复：matchDispatch 副作用 scrutinee 未 hoist**（match 路径的同类问题，与 v0.2.2 #16
  while-let hoistScrut 同源）：`match v.pop() { Some(x) => ..., None => ... }` 此前
  cpp/python 路径在每臂 cond 与 binds 都引用 scrut（pop 被多次求值，破坏副作用语义）；
  现 matchDispatch 入口对 python/cpp 路径 hoist 非标识符 scrut 到 `_m_N`。rust/ts/js/go
  路径仅在 match/switch 头求值一次（原生语义保证），无需 hoist
- **修复：validate balanceCheck 误判 `(*ptr)` 解引用为注释**：`(*v)[n]`（go/cpp 解引用 +
  下标）此前被误报为 OCaml `(* *)` 块注释未闭合；现仅 ocaml/fsharp/pascal 方言识别 `(*`
  为注释，go/cpp/ts/js/python 等正确视为解引用表达式
- **Vec::pop / Vec::first / Vec::last / clone 活体映射扩至 cpp/go**（v0.2.5 #2 的 5 方法 →
  4 方法 × 7 语言）：cpp 模板助手 `_dhvPop/_dhvFirst/_dhvLast`（std::optional 语义 +
  include guard）；go 泛型助手 `_dhvPop/_dhvFirst/_dhvLast`（指针副作用通道，1.18+）；
  clone 为 cpp 拷贝构造 / go slice header 拷贝（与 interp 浅拷贝语义对齐）。`while let
  Some(x) = v.pop() { ... }` drain 场景从 contract 回退升级为活体翻译
- **Option 方法族扩至 cpp/go**：unwrap_or（cpp `value_or` / go `(recv != nil ? *recv : d)`）/
  unwrap（cpp `*recv` / go `*recv`）/ is_some（cpp `has_value()` / go `!= nil`）/ is_none
  （cpp `!has_value()` / go `== nil`）。链式 `v.pop().unwrap_or(d)` 现可活体翻译
- **C# 生成物结构合法化**（v0.2.5 #5 Java 的同构修复）：旧版 C# 把 fn/const 投射为顶层
  `public static T F(...)` / `internal const T K = ...`（C# 顶层函数/常量非法）。新结构：
  类型项顶层声明（同命名空间裸名互见）+ fn/const 包装进 `internal static class Dhv<文件stem>`
  （与 Java `class Dhv<Stem>` 同构；static class 所有成员必须 static，与 fn/const 投射
  形态一致；防实例化；每文件唯一防重名冲突）。C# 同步加入 X-1 告警
- **pattern-tour drain → cpp/go 扩面**：v0.2.5 新增的 drain 函数（Vec::pop while-let 累加）
  此前未投至 cpp/go（因 pop 缺映射）；本轮扩面后 pattern-tour 30→32 文件
- **dhv Rust codegen 一致性 review**（Task 19 移交⑥）：dhv/src/codegen/{mod.rs,
  contract.rs,static_res.rs} 与 langs.rs 38 后端注册表逐项核对一致 —— mod.rs 注册 6 命名
  后端（rust/python/typescript/yaml/markdown/json）+ 循环 LANGS 注册其余 32 为
  ContractBackend（共 38）；ContractBackend emit_item 处理 struct/enum/trait/fn/graph 签名
  契约（与 dhv-ts decls.ts 同构）
- **测试套件 74→82 用例**（新增 8 例：cpp Vec::pop g++ 编译+链接+运行 / cpp-go first/
  last/clone + 编译级 / matchDispatch 副作用 hoist / cpp pop 副作用对接收者可见 /
  balanceCheck (*ptr) 不误判 / C# 宿主类合法化 / Kotlin-Swift contract 结构 / 宏 token 树
  嵌套 delim 类型收集）

## v0.2.5 变更摘要

- **if-let / while-let 活体翻译扩展至 cpp/go**（5→7 语言）：cpp 走
  `std::holds_alternative<V>` + `auto& _v = std::get<V>`；go 走类型断言 init-statement
  `if _ifv_N, _ok_M := s.(V); _ok_M`（无绑定 blank 标识符防 unused）。while-let：
  cpp `while (true) { if (!cond) break; }` / go `for { ... }`（scrutinee 每迭代求值一次，
  Rust 语义同源）。Result::Ok/Err 对 cpp/go 诚实回退 contract（类型映射无变体通道）
- **修复 4 个生成端非法代码 bug**（if-let 活体化暴露的预存缺陷）：① cpp/go Option 条件
  `v != null`（非法）→ `has_value()` / `!= nil`；② 绑定 `const x = v`（非法）→
  `auto x = *v;` / `x := *v`；③ go 变体字段大小写错位（`s.f0` vs 声明 `F0`）→ capitalize；
  ④ 裸 `None` 值在 cpp/go/ts/js 输出非法字面 `None` → nullopt/nil/null
- **Some 构造映射（cpp/go）**：cpp 模板助手 `_dhvSome`（类型推导 + include guard）；
  go 泛型助手 `_dhvSome[T any]`（1.18+）；附带修复 cpp `String::to_string()` 字符串
  接收者生成非法 `std::to_string("x")` → `std::string(recv)`
- **Java 生成物结构合法化**（重构）：类型项顶层声明（同包裸名互见，跨文件引用无需限定）；
  fn/const/impl 宿主 `class Dhv<文件stem>`（每文件唯一；旧版全项嵌 public class 含
  双非法点：public 类名不匹配文件名 + 同模块多文件 wrapper 重名）；Java 加入 X-1 告警
- **M3 静态化**：check 阶段校验 import 名未被源模块 export（此前只在 run/emit 报错；
  nova 实录 8 个缺 export 定义致 run/emit 双失败而 check 全绿）
- **修复：dhv Rust `hsl.pest` 引用未定义规则**（编译失败级）：`expression_with_block`
  此前被引用但从未定义 —— pest_derive 无法编译。现补全（块表达式块尾自终止，
  与 dhv-ts v1.4.2 #4 守卫同源）+ parser.rs Pair 树契约同步
- **修复：nova 示例 8 个定义缺 export**（NovaError/ProviderError/ToolError/PolicyViolation +
  4 个 graph + 4 个 block/static 资源）——nova run/emit 双双复活
- **cpp 编译级验证扩面**：pattern-tour describe/classify/count_down g++ 编译+链接运行
  语义与解释器对齐；Some/None 构造链路同验；pattern-tour 扩容 count_down +
  classify→go + count_down→5 语言（24→30 文件）
- **测试套件 66→74 用例**（新增 8 例：cpp Some/None g++ 编译+运行 / cpp-go if-let+while-let
  结构断言+python exec / cpp Option match / cpp to_string / go 字段大小写 / Java 结构 /
  M3 静态正反 / nova emit 回归）

## v0.2.4 变更摘要

- **跨文件类型依赖自动接线**：emit 追踪投射项的用户类型引用，按语言接线——python
  `from <stem> import T` / ts·js 相对 `import { T } from './<stem>'` / rust
  `use crate::<mod>::{T}` / go 同包免导入 / **cpp 内联 ODR 兼容类型声明**（未投射类型从
  AST 兜底内联）；X-1~X-4 诚实告警（未投射类型 / 跨目录 / 非法模块路径 / go 跨包）
  使 emit 退出码 1
- **修复：元组下标 `t.0` 生成端非法语法**（python/ts 的 SyntaxError）；现除 rust 外一律 `t[0]`
- **修复：副作用接收者双重求值**：`m.remove(k).unwrap_or(d)` 此前双重 `pop`（键删两次）；
  Option 组合子家族（unwrap_or/unwrap_or_else/and_then/or/pop/clone/is_sorted/strip_prefix/
  strip_suffix/find）统一走 prelude 助手，参数恰好求值一次（ts/js 同构修复）
- **方法映射表 66→75 项**：strip_prefix/strip_suffix/find（Option 语义）· position ·
  enumerate · cloned · Vec::insert/Vec::remove 与 HashMap 同名方法类型感知分发 ·
  ts/js `_dhvRemove`（Map::remove Option 语义）
- **显式类型 let**：rust `let n: i64 = 0` / cpp `int64_t n = 0` / go `var n int64 = 0`
  （注解在场时照实投射，i64 大值不再静默截断为 i32/int）
- **cpp 后端首次编译级验证**：backends-demo 6 文件 g++ -std=c++23 全过 + 链接可执行输出
  与解释器逐字一致；pattern-tour 4 文件（含内联 Shape）全过；测试套件在 g++ 环境自动执行
- **dhv Rust 一致性**：hsl.pest 宏定义尾 `!` 容错（v1.4.2）· typecheck.rs S-6 通配语义
  （v1.4.1）同步
- **测试套件 57→66 用例**（新增 9 例：新方法语义级 / 元组下标三语言 / 跨文件接线断言 /
  X-1 告警回归 / cpp 编译级 ×2 / ts remove bun exec / 副作用单次求值 / dhv Rust 源码守护）

## v0.2.3 变更摘要

- **if-let 尾位置值语义**：带 else 的 `if let ... {} else {}` 在函数尾产出 return（与
  match/if 对齐）；值语义块（match 臂/if 分支）尾部的嵌套 if/match 同步修复
  （此前静默丢失 return）；`else if let` / `else if` 链可活体翻译
- **Vec::get / HashMap::get 生成端 Option 语义**：python `_dhv_get` 助手（越界/缺键 →
  None）；ts/js 类型感知分发（`v[i] ?? null` / `m.get(k) ?? null`）；rust 原生 `.get()`
  ——与解释器语义完全对齐（闭合 v1.4.2 已知限制）
- **类型感知同名方法分发**：`map` 按接收者静态类型分发 Vec::map / Option::map
  （此前 Option::map 静默生成 Vec 风格代码）
- **方法映射表 46→66 项**：pow/sqrt/floor/ceil/round/clamp · any/all/fold/for_each/extend ·
  sort_by(key 语义)/sort_desc · as_str/trim_start/trim_end/char_at/is_alphabetic/is_numeric ·
  or/unwrap_or_else · rust 链拼块 iter/collect/cloned
- **contract 后端声明质量**：kotlin/swift/scala 参数类型后置冒号（此前误用 C 风格）；
  Java 全部顶层项包进 `public class <Module> {}`
- **macro_rules! 定义名尾 `!` 容错**：与 Rust 习惯迁移兼容
- **测试套件 57 用例全绿**（新增 8 例：if-let 尾值语义 / 嵌套 if 值语义回归 / else-if-let
  链 / 新方法语义级验证 / Option::map 分发 / 三语言声明质量 / ts get 类型感知 / macro 尾 !）

## v0.2.2 变更摘要

- if-let/while-let 模式扩展至 9 类（枚举 tuple/struct/无负载变体 + Result::Ok/Err +
  binding + 通配）；块表达式二元 LHS 解析器修正；测试 49 用例

## v0.2.1 变更摘要（开源发布版）

- **38 后端投射**：`emit` 从"只投射静态资源"升级为全量代码生成（examples/backends-demo
  182 文件 / 38 语言实测全部通过语法校验；python 生成代码经 exec 语义级验证）
- **双向工程**：`@dhv:source-map` / `@dhv:hsl-mirror` / `@dhv:end-source-map` 三标记围栏
  协议；`sync` 按名回写 + 回写后解析校验 + 失败回滚；`watch` File Watcher
- **标准库 10 模块**：`import { f } from "std/<mod>";`（BNF v1.4 附录 C）——
  core/collections/text/math/io/json/time/random/env/iter，约 60 函数
- **能力分级诚实边界**：full（python/ts/js 活体翻译）/ logic（rust/go/cpp 语句子集）/
  contract（26 语言类型契约 + 围栏 HSL 镜像）/ 静态（6 格式原文 + 插值），写入 manifest
- **修复 `?` 的 From 转换**：类型路径解析保留泛型实参，`impl From<E1> for E2` 经 `?`
  真实转换（此前为死代码）
- **S 规则全部真实生效**：修复 bun 转译器静默丢弃语句位置 `declare(...)` 调用的宿主陷阱
  （该 bug 曾使 S-7/S-8 从未执行）；宏实参下探 / native 捕获标记 / 子作用域使用上溯 /
  `_` 通配豁免
- **测试套件**：`tests/hsl/run-all.ts` 57 用例（回归 / 检查规则正反例 / emit / 解析 /
  sync 闭环 / 模糊 / CLI / 压力）全绿，随发行物分发

完整语言教程：`docs/HSL-GUIDE.md`（hsl-guide 独立下载包含同文）。规范：`hsl-spec/BNF.md`
（v1.4）+ `hsl-spec/COMPLIANCE.md`（总纲合规对照表）。
