# LANG-NOTES — HSL v0.2.72 / BNF v1.5.0 实测参考（dhv-ts 解释器）

> 写给"sentry 这类 1500+ 行、多模块、带 graph/投射/native"的 harness。
> 全部结论在 `toolchain/dhv-ts`（v0.2.72）上实测；证据文件在 `docs/probe/`（下文 `pNN`/`vN` 即探测文件名）。
> 命令（cwd = 仓库根 `D:\下载\hsl`）：
>
> ```bash
> bun toolchain/dhv-ts/src/main.ts check <entry.hsl>          # 静态检查
> bun toolchain/dhv-ts/src/main.ts run   <entry.hsl> --quiet  # 解释执行（entry 里要有 fn main）
> bun toolchain/dhv-ts/src/main.ts emit  <entry.hsl> --out DIR [--scale monolith|microkernel] [--no-validate]
> bun toolchain/dhv-ts/src/main.ts targets                     # 列后端
> bun toolchain/dhv-ts/src/main.ts sync <generated-file> --root DIR
> ```
>
> `check` 只做浅层检查：**类型注解基本不校验、S-1/S-5 等语义要到 run 才炸**。写完必须真的 `run` 一遍。
> 文档与实现冲突时以本文件（实测）为准。

---

## 1. 程序形状 / 入口 / 模块

```hsl
// 单文件最小程序
fn main() -> Result<(), String> {
    println!("{}", 1 + 1);
    Ok(())
}
```

- 入口：`run <file>` 调用该文件里的 `fn main`，**无需 `export`**（R-1）。已验证三种签名：`fn main()`、`fn main() -> i64`、`fn main() -> Result<(), String>`。
- 没有 `main` 的入口文件直接失败：
  `✗ 运行期错误：入口文件没有 fn main()（运行约定：BNF v1.3 §R-1）`
  （`toolchain/examples/nova/nova.hsl` 是 emit-only 示例，本身不可 run。）
- 顶层项顺序（惯例，实现不强制）：`import` → `const`/`typealias`/`struct`/`enum`/`trait`/`impl` → `fn` → `graph`；`project{}`/`scale` 放文件末尾。
- 文件即模块（M1）。跨模块必须 `export` 目标项（M3 检查，见 §6）。
- `.hsl` 源文件必须 **UTF-8 无 BOM**。BOM 会让第一行首字节被当成 token：
  `error[E-0]: 期望项定义，得到 "﻿" (ident) (...:1:2)`
  用 write 工具或 `[IO.File]::WriteAllText($p,$s,(New-Object Text.UTF8Encoding($false)))` 写；不要用 PowerShell `Set-Content -Encoding utf8`（会加 BOM）。
- run 产物：`.hsl-runs/<ISO 时间戳>/` 下 `report.md`、`transcript.jsonl`、`events.jsonl`、`run.json`。

推荐的大 harness 布局（sentry 就是这么分的）：`types/*.hsl`（struct/enum/const/typealias）、`logic/*.hsl`（纯函数+impl）、`agents/*.hsl`、`providers/*.hsl`、`config/resources.hsl`（block/static）、`sentry.hsl`（import 汇总 + 主编排 graph + `fn main` + `scale` + `project{}`）。

---

## 2. 类型系统表面

### 2.1 标量

`i8..i128 / isize / u8..u128 / usize / f32 / f64 / bool / char / String`。
字面量支持 `0xFF`、`0b1010`、`4_000`、`1.5`、`r"raw"`、char `'x'`。

- **解释器整数是 BigInt：任意精度、不回绕**（13.3 #64）。`i64` 溢出不会报错，会静默变大数。
- 整数除法截断：`7 / 2 == 3`；`%` 同 Rust。
- **没有引用/生命周期类型**：`fn f() -> &'static str` →
  `error[E-0]: 无法解析的类型 "static" (label)`。一律用 `String`（必要时 `.clone()`）。
- 显式转换只用 `as`；`int + float` 是 `error[S-14]`（见 §4）。
- 注解域检查只对**静态字面量**生效：`error[S-13]` / `error[S-16]`（`s13_domain.hsl` / `s16_big.hsl`）。运行期 `let x: u8 = 300;` 不拦。

### 2.2 复合

- `Vec<T>`、`HashMap<K,V>`、`Option<T>`、`Result<T,E>`、`Box<T>` / `Box<dyn Trait>`、tuple `(A, B)`（`t.0`/`t.1`，`let (a,b) = t` 解构）。
- `typealias`、`const NAME: T = ...;`（String const 可用，也能进 `{{}}` 插值）。
- `HashSet<T>` **不可用**（见 §14 陷阱 2）。
- 泛型 `struct Holder<T> { v: T }`、`fn f<T>(x: T) -> T` 能解析、能 run（T 在解释器被擦除，仅 emit 有意义）。但泛型函数**不做类型校验**，实测 `ident_generic<T>(9)` 返回 9。
- **类型注解在 dhv-ts 里基本不校验**：
  ```hsl
  let x: i64 = "a string";   // check 0 error
  println!("{}", x.len());   // run → 8，不报错
  ```
  结构体字段同理。别指望注解兜底；正确性靠 run 与 emit。
- `Self` 关键字能过 check，但 run 报 `✗ 运行期错误：未知的结构体 "Self"`。impl 的返回类型/构造统一写具体类型名。

### 2.3 struct / enum

```hsl
struct Point { x: i64, y: i64 }
impl Point {
    fn new(x: i64, y: i64) -> Point { Point { x, y } }   // 字段简写
    fn shift(mut self, dx: i64, dy: i64) -> Point { self.x += dx; self.y += dy; self }
    fn norm2(self) -> i64 { self.x * self.x + self.y * self.y }
}

enum Shape { Circle { r: f64 }, Rect(f64, f64), Empty }   // struct 变体 / tuple 变体 / 单元变体
```

- 结构体更新：`Point { x: 99, ..base }` 可用。
- `{:?}` 打印 `Point { x: 0, y: 0 }`；String 字段带引号 `P { s: "a" }`。
- 结构体按字段做相等比较，无需 derive；`.clone()` 可用。
- 变体构造/模式可用简写：`Some/None/Ok/Err`。
- tuple 变体模式：`Shape::Rect(w, h)`；struct 变体：`Shape::Circle { r }`。
- **enum 必须穷尽 match**（S-6，见 §4）。

---

## 3. 函数 / impl / trait / 闭包 / async

```hsl
async fn fetch(x: i64) -> Result<i64, String> { Ok(x + 1) }

fn main() -> Result<(), String> {
    let v = fetch(1).await?;              // async 调用要 .await
    let f = |a: i64, b: i64| a + b;       // 闭包：参数可带注解，捕获按名字复制
    let g = f(1, 2);
    println!("{} {}", v, g);
    Ok(())
}
```

- 函数体最后一个不带分号的表达式即返回值；也可 `return x;`。
- `?` 只用于 `Result` / `Option`：`Ok` 取内值、`Err` 提前返回；Option 的 `None` 提前返回 `None`。`?` 在别的类型上 check 放过、run 报 `✗ 运行期错误："?" 只能用于 Result/Option（S5），得到 int`。
- `impl From<A> for B` 会被 `?` 用来做错误转换（实测 `p05`：`A` 出错自动变 `B`）。多模块 harness 里给每种错误实现 `impl From<X> for HarnessError` 是最省事的写法。
- recursive fn 正常（`fact(5)=120`）。
- 闭包捕获是**按值复制**，不是移动：闭包里改捕获变量不会影响外部。
- `fn f(...)` 名字可作为值传递。
- 泛型 fn 可写（见 §2.2）。
- `mut self` 方法**未必回写调用方**：`p06` 里 `fn bump(mut self) -> i64` 返回新值，但 `c.get()` 仍是旧值。需要回写就返回新值并赋值；不要依赖 `mut self` 就地改。
- trait：默认方法、`impl Trait for T`、重载、`Box<dyn Trait>` 动态派发都可用：

```hsl
trait Provider { fn apply(self, p: i64) -> i64; fn name(self) -> String { String::from("base") } }
struct Double {}
impl Provider for Double { fn apply(self, p: i64) -> i64 { p * 2 } }

fn dispatch_demo() -> i64 {
    let boxed: Vec<Box<dyn Provider>> = vec![Box::new(Double {})];   // 单元 struct 必须带 {}
    let mut total = 0;
    for p in boxed { total += p.apply(21); }
    total
}
```

坑：`Box::new(Double)`（单元 struct 省略 `{}`）能过 check，run 报 `✗ 运行期错误：foreign 没有方法 "apply"`（apply 换成你的 trait 方法名）。**始终写 `Box::new(Double {})`**。
坑：`Trait::method(obj)` 静态调用语法能过 check，但返回 `()` 垃圾值。**一律用 `obj.method()`**。
坑：跨模块时实现方法按"类型名"全局注册（13.3 #6）⇒ **全项目类型名唯一**；同名 struct 在两个模块里会互串方法。

---

## 4. S 规则：check 期 vs run 期（含诊断原文）

| 规则 | 时机 | 诊断原文（实测） | 修法 |
|---|---|---|---|
| S-1 零隐式转换 | **run** | `✗ 运行期错误：if 条件必须是 bool（S1：零隐式转换），得到 1`；`✗ 运行期错误：while 条件必须是 bool（S1），得到 0` | 条件写 `==`/`!=`/显式 bool |
| S-2 裸 unwrap | check(warning，退出 0) | `warning[S-2]: 裸 .unwrap()：非空默认建议使用 unwrap_or / match / ?（S2）` | 用 `unwrap_or`/`?`/match；或用 expect 说明 |
| S-4 不可变赋值 | check | `error[S-4]: 对不可变绑定 "x" 赋值（请用 let mut）`；graph 里 `✗ 运行期错误：不能对不可变绑定 "phase" 赋值（S4：不可变优先，请用 let mut）` | `let mut`；graph 节点 `node mut` |
| S-5 `?` 类型错 | **run** | `✗ 运行期错误："?" 只能用于 Result/Option（S5），得到 int` | 确认函数返回 Result/Option |
| S-6 match 不穷尽 | check | `error[S-6]: match 不穷尽：Color 缺少变体 Blue` | 补臂或 `_`（普通函数）|
| S-6 AgentLoop 内 | check | `error[S-6]: graph AgentLoop 内的枚举 match 不允许 _ 通配兜底（必须显式穷尽，直面新分支）` | 在 graph 的 loop 里**逐变体列出**，不许 `_` |
| S-7 未使用 | check | `error[S-7]: 绑定 "unused" 声明后未使用（_ 前缀可豁免）`；`error[S-7]: import "helper" 未使用` | `_name`；`let _ = f()`；真正用到再 import |
| S-8 同作用域遮蔽 | check | `error[S-8]: 同作用域遮蔽："x" 已声明（S8）` | 换名或包进 `{ }` 内部作用域 |
| S-13 字面量超域 | check | `error[S-13]: 整数字面量 300 超出 u8 域 [0, 255]（rustc 后端将拒绝编译，python/js 后端静默放行 —— 跨后端漂移；显式截断请用 as）` | 用 `as` 或换类型 |
| S-14 二元混算 | check | `error[S-14]: 二元运算类型不匹配：int + float（int 与 float 混算需显式 as 转换（S1 零隐式转换）；rustc 后端编译期拒绝，python/typescript 后端静默放行或产生垃圾值 —— 跨后端漂移）` | `(a as f64) + b` |
| S-16 超 i128 | check | `error[S-16]: 整数字面量超出 i128 静态容量：170141...728（dhv(Rust) parse 静默归零为 0 —— 值损坏；... 静态域分析以 i128 为容量上界 ...）` | 拆小/用 f64 |
| S-18 native 产物带标记 | check(warning) | `warning[S-18]: native 块产物进入含结构体/枚举族 "Payload" 的注解绑定，但 native 返回的 plain object 不带运行时 __struct/__enum 标记（foreign 值：字段直通可用，.clone()/方法/模式派发失效，#L-22）...` | native 内 `$host.make(...)`；或拍平成 JSON 字符串 |
| S-19 方法不存在 | check(0.2.67 起 error) | `error[S-19]: 接收者类型为 String，但运行期方法面 STRING_METHODS 没有 "missing_method" —— run 将报「String 没有方法 "missing_method"」（#13 ...）` | 只用 §7 里的方法 |

补充实测：
- S-1/S-5 是 run 期才发现 → **不跑就不知道**。
- S-7 的 import 检查是源码行扫描（13.3 #12）：字符串里出现同名也算用过；`import * as m` 豁免但 std 场景不可用（见陷阱）。
- G-4 的 span 有 bug：同一 graph 多个孤岛节点会重复同样位置告警，别被行号骗。
- P-1 实际上**未强制**：一个文件里写两个 `project{}`、两条 `scale` 声明，check 都过（P-3/P-6 另行检查）。但仍建议一个文件一个。

---

## 5. 控制流

```hsl
if a > 0 { ... } else if a == 0 { ... } else { ... }       // if 是表达式，可绑定
let k = if a > 0 { String::from("+") } else { String::from("-") };
if let Some(v) = opt { ... } else if let Ok(w) = r { ... }
while i < 3 { i += 1; }
while let Some(v) = it.pop() { ... }
for x in 0..5 { ... }            // 不含 5
for x in 1..=3 { ... }           // 含 3
for x in v { ... }               // 遍历 Vec 的快照（实测循环跑完后原 Vec 内容不变）
for (k, v) in m.iter() { ... }   // HashMap：插入序（JS Map），不是哈希序
loop { break; }                  // 可做表达式：let r = loop { break 42; };
'outer: for ... { break 'outer; continue 'outer; }
```

- `break` 可以带值：`let r = loop { break 42; }; // 42`。
- 没有三元 `?:`；用 if 表达式。但**不要**直接把它塞进算术：`(if c {1} else {2}) + 3` 会被 reject（13.3 #26），先 `let t = if ...;` 再算。
- `break`/`continue` 只作用于循环；graph 的 AgentLoop 里 `break` 退出该 loop。

---

## 6. 导入 / 导出 / std 模块

三种形式（路径相对**当前 .hsl 文件**，必须带 `.hsl`）：

```hsl
import { LibCfg, LibTrait, lib_double } from "./lib.hsl";   // ① 具名
import lib_double as dbl from "./lib.hsl";                  // ② 别名
import * as lib from "./lib.hsl";                           // ③ 命名空间
```

- ① 最完整：结构体字面量、trait 类型注解、函数都能用。
- ② 只适合单个符号。
- ③ **只能调函数/读 const**：`lib.lib_double(5)`、`lib.LIB_LIMIT` 可以；`lib.LibCfg { ... }` 结构体字面量报
  `error[E-0]: 期望 ";"，得到 "{" (punct)`。要用结构体就改具名 import。
- 文件不存在：`error[E-0]: import 路径不存在："./no_such_lib.hsl"（from <绝对路径>）`
- 导入未 export 的符号：`error[M3]: import 失败："private_helper" 未被 p17_import_lib.hsl export`
- 未使用 import 是 **error**（S-7）。只被"方法调用隐式用到"的 trait 也算未使用；把它用在类型位置（`Box<dyn LibTrait>`）才算用过。
- `export` 是编译期概念：同文件内无需 import 自己的项。
- std 模块共 10 个，**必须具名 import**：`import { to_snake, PI } from "std/text"`。
  `import * as text from "std/text"` check 能过，run 每个调用都炸：
  `✗ 运行期错误：foreign 没有方法 "to_snake"`。

| 模块 | 导出（实测全可用） |
|---|---|
| std/core | identity, todo, unreachable, type_name, hash |
| std/collections | vec, repeat_vec, zip, chunk, dedup(相邻), unique(全局), flatten, sort_desc, reverse, swap_remove |
| std/text | split_once, rsplit_once, split_at, to_snake, to_camel, to_pascal, to_kebab, pad_start, pad_end, capitalize, count, is_alpha, is_numeric, is_alphanumeric, truncate, levenshtein |
| std/math | PI, E, sin, cos, tan, asin, acos, atan, atan2, exp, ln, log2, log10, pow, sqrt, gcd, lcm, signum, isqrt, div_ceil, div_floor, rem_euclid, hypot, is_nan, is_infinite, inf |
| std/io | read_file, write_file, append_file, list_dir（都返回 Result；仅 run 模式有宿主） |
| std/json | parse, stringify, get（无宿主依赖） |
| std/time | now_ms, now_iso, duration_desc |
| std/random | seed, random, int_in, choice, shuffle, uuid_v4（mulberry32，默认 seed 42，可复现） |
| std/env | env_get, task_text, model_name, workspace |
| std/iter | range, range_step, enumerate, chain, take, skip, min_of, max_of |

实测样例：`to_snake("HelloWorld")=="hello_world"`、`pad_start("7",3,"0")=="007"`、`truncate("abcdef",4)=="abc…"`、`levenshtein("kitten","sitting")==3`、`split_once("a:b",":")` → `Some(["a","b"])`、`gcd(12,18)==6`、`isqrt(17)==4`、`range(0,5)==[0,1,2,3,4]`、`now_iso()` 24 字符、`json.stringify(vec![1,2])=="[1,2]"`。`workspace()` == 进程 cwd；`task_text()==""`、`model_name()=="scripted"`（默认）。

---

## 7. 可调用方法面（builtins.ts 注册表）

> 名单来自 builtins.ts 注册表（S-19 就按它判"方法是否存在"）；其中大部分已逐个 run 验证，个别只验证了注册存在。方法名不在此名单 → `error[S-19]`；不在方法面的调用在 run 期报 `没有方法 "x"`。

- **String**：len, is_empty, push_str, as_str, clone, to_string, trim, trim_start, trim_end, contains, starts_with, ends_with, replace, split, split_whitespace, lines, to_lowercase, to_uppercase, chars, repeat, strip_prefix, strip_suffix, find, parse, char_at, char_count, take, join, split_once, rsplit_once, clear, truncate, retain, insert, remove
  - **不能下标**：`s[0]` → `✗ 运行期错误：String 不支持索引`。用 `s.chars()` / `char_at`。
  - `"42".parse::<i64>()` 可用；`"x".parse::<i64>().unwrap_or(0)` 常用。
- **Vec**：len, is_empty, push, pop, clone, first, last, get, contains, join, iter, iter_mut, into_iter, to_vec, next, map, filter, for_each, any, all, fold, enumerate, take, skip, rev, sort, sort_by, append, extend, sum, collect, clear, is_sorted, sort_desc, position, insert, remove, find, filter_map, flat_map, flatten, count, min, max, zip, chain, step_by, reverse, dedup, retain, truncate, chunks
  - `v[i] = ...` 下标赋值可用；`v[0]` 读可用；`v.iter().map(...).collect::<Vec<String>>().join("-")` 可用。
  - `sort()` 升序；**`sort_by` 是"键函数"语义**：`v.sort_by(|x| -x)` 得到降序（不是 Rust 的 cmp 语义！）。
  - `sum()` 需要 turbofish：`v.iter().sum::<i64>()`；`min()/max()/first()/last()/get()/position()` 返回 Option。
- **HashMap**：insert, get, contains_key, len, is_empty, remove, clear, keys, values, clone, iter
  - `m["a"]` 下标读可用；`m.iter()` 迭代 `(k,v)`；**顺序 = 插入序**（JS Map）。`remove` 返回 Option。
  - 键类型任意（`HashMap<i64, String>` 可用），不限于 String。
- **Option**：unwrap, expect, unwrap_or, unwrap_or_else, is_some, is_none, map, and_then, ok_or, or, and, cloned, clone, filter
- **Result**：unwrap, expect, is_ok, is_err, ok, err, map, map_err, unwrap_or, clone, and_then, or_else, unwrap_or_else, unwrap_err
- **数字**：to_string, abs, pow, sqrt, floor, ceil, round, min, max, clamp, clone
- **char**：to_string, is_alphabetic, is_numeric, is_ascii, is_ascii_*（系列）, clone
- **自由函数**：min, max, abs, to_string, str

比较/相等：`==`/`!=` 对 struct 按字段比较（无需 derive，实测）；enum/Vec 也能直接 `==`。`<`/`>` 对数字与 String 可用。

---

## 8. `format!` / `println!` / `vec!` / 断言

只实测这几种格式说明符有效：

| 写法 | 结果 | 备注 |
|---|---|---|
| `{}` | Display | String 不加引号；struct 打印成 `P { s: "a" }` |
| `{0}` `{1}` | 位置参数 | 可重复引用 |
| `{:?}` | Debug | String/Option 带引号；struct 同 `{}` 但字符串带引号；foreign 值打印 JSON |
| `{:.2}` | 浮点小数位 | 3.1416 |
| `{{` `}}` | 字面量花括号 | `format!("{{}}")=="{}"` |

**被静默忽略（不报错、不生效）**：`{:>4}` `{:<4}` `{:^4}` `{:05}` `{:x}` `{:o}` `{:b}` `{:#x}` `{:#?}`。
实测 `{:05}` on 42 → `"42"`；`{:x}` on 255 → `"255"`；`{:#?}` == `{:?}`。需要补零/进制/对齐就**自己写 HSL 函数**（如 `pad_start(v.to_string(), 3, "0")`、手写 hex）。

宏参数坑：`println!("{:?}", Ok::<i64, String>(5))` check 过、run 报
`✗ 运行期错误：期望标识符，得到 "<" (punct) (<macro>:2:27)`
→ 带逗号的多参数 turbofish 不要直接放进宏实参；先 `let x = Ok::<i64,String>(5);` 再打印。单参 turbofish（`collect::<Vec<i64>>()`）没问题。

其他预置宏：`vec![...]`、`print!`、`eprintln!`、`assert!`、`assert_eq!`、`panic!`、`dbg!`。

---

## 9. graph / node / edge / AgentLoop（G-1..G-4 + 事件/scale）

```hsl
enum Phase { Plan, Act, Done }

graph Worker(mut state: State) -> Result<i64, String> {
    node mut phase: Phase = Phase::Plan;    // 节点默认不可变，要改就 node mut
    node mut count: i64 = 0;
    node items: Vec<i64> = vec![];
    edge phase -> count on Phase::Act;      // 端点须先声明（G-2）
    edge count -> items on Phase::Done;

    let start = 1;
    loop {                                   // G-1：graph body 必须恰含至少一个 loop（AgentLoop）
        match phase {                        // 必须逐变体列出，禁止 _（S-6）
            Phase::Plan => { phase = Phase::Act; state.count = state.count + 1; }
            Phase::Act  => { count = count + start; phase = Phase::Done; }
            Phase::Done => { break; }
        }
    }
    Ok(count)
}

fn main() -> Result<(), String> {
    let mut st = State { count: 0 };
    println!("{}", Worker::run(st)?);
    Ok(())
}
```

- 调用：`G::run(args)`（与 trait 关联函数同形；跨模块时 `import { Worker } from "./agents/worker.hsl"` 再 `Worker::run(...)` —— guide/nova 示例如此写法，单模块已实测）。
- 节点在 loop 里的 match 臂中赋值；`node mut x`；`let` 可出现在 graph body 与 loop 内。
- 早返回：loop 里 `return Ok(n * 10);` 可用（`v1`）。
- 边属性：`edge a -> b on Guard with durable = true;`；链式 `edge a -> b -> c on Guard with durable = true;`；or-guard `on P::A | P::B`（`v1`/`v2` 实测 check+run 都过）。
- 环必须至少一条边带 `on Guard`，否则：
  `error[G-3]: 拓扑存在无条件环：a -> ... -> a（环上至少一条边需 on Guard 打破，G3）`
- `error[G-1]: graph NoLoop 缺少 AgentLoop（graph body 必须恰含至少一个 loop）`
- `error[G-2]: edge 端点 "ghost" 未在 graph body 中声明（node/let，且声明须先于 edge）`
- `warning[G-4]: 节点 "p" 没有任何 edge（孤岛节点；若为插件注入位请加注释说明）`
- **G-5/G-6（实测形态）**：node 初始化事件按声明序写入 events.jsonl；guard 变体命中时写 edge 事件。
- **边事件**：runtime 在 AgentLoop 的 match 里命中"作为某条边 guard 的变体"时写事件；所以**要用 `match phase { ... }` 显式写每个 guard 变体**，否则没有 edge 事件。
- **state 传参是引用语义**（实测 `v5`/`v6`）：`Worker::run(st)` 会把 graph 内对 `state` 的修改**回写到调用方的 `st`**（run 后 `st.count==1`）；`Worker::run(st.clone())` 则不会（`st.count==0`，返回 1）。想隔离状态就显式 `.clone()`；想回写就直接传。
- `events.jsonl` 实测字段：
  - `{"name":"run_start","data":{"entry":...,"model":"scripted","task":"","scale":"microkernel"}}`
  - `{"name":"node","data":{"graph":"Worker","node":"phase","initialized":true}}`（按声明序，每节点一次）
  - `{"name":"edge","data":{"graph":"Worker","from":"phase","to":"count","on":"Act","scale":"microkernel"}}`
  - `{"name":"run_end","data":{"elapsed_ms":21,"ok":true}}`
  - `run.json` = `{ts, ok, elapsed_ms, model, task, events}`。
  - 还可通过 native 里 `$host.events.emit(name, data)` 追加自定义事件。
- `scale`：`scale = "monolith"` / `"microkernel"`（默认 microkernel）。声明要放在含 graph 的入口文件，否则
  `warning[P-6]: scale 声明应出现在含 graph 的入口文件`。
  `emit --scale` 只影响头部文案/字节数，**文件布局与路径完全相同**（实测）。
- graph 里 `state` 字段修改在 graph 内可见；跨 graph 是否回写取决于是否传引用（见上）。

---

## 10. project{} / rules{}（BNF v1.5）/ emit / sync

```hsl
scale = "microkernel";

project {
    TriageState -> "src/types/state.rs" : rust,
    TriageReport -> "src/types/report.rs" : rust,
    audit       -> "src/logic/audit.py"   : python,
    SENTRY_PROMPT -> "resources/prompt.md" : markdown,

    rules {
        struct -> "src/gen/{name}.rs" : rust,
        enum   -> "src/gen/{name}.rs" : rust,
        fn     -> "src/gen/{name}.py" : python,
    }
}
```

- `project{}` 语法：`Item -> "相对路径.ext" : lang`，**每项必须真实存在（本地声明或已 import）**，否则
  `error[P-3]: 投射目标 "Ghost" 未定义（或未 import）`。
- **`rules{}` 只能嵌套在 `project{}` 里面**。写在顶层：
  `error[E-0]: 期望项定义，得到 "rules" (ident)`。
- 规则类型恰好 9 种：`graph/fn/struct/enum/trait/const/type/block/static`。多余/拼错：
  `error[P-5]: 投射规则类型 "widget" 未注册（支持 graph/fn/struct/enum/trait/const/type/block/static）`
- 路径占位符只支持 `{name}`：
  `error[P-5]: 投射规则路径占位符 {wrong} 未注册（v1 仅支持 {name}）`
- 同一类型只能一条规则（R3）：
  `error[P-5]: 投射规则类型 "struct" 重复声明（R3：同一类型只允许一条规则）`
- 展开池（R5）= 本文件命名的项 + 已 import 模块的 **exported** 命名项（impl/import/宏调用不展开）。实测 `mm_rules_entry.hsl`：规则自动把 import 来的 `MPhase` 投到 `gen/enums/MPhase.rs`。
- 显式单条映射优先（R1）：`MItem` 既有显式又落进 struct 规则 → 只生成显式路径。实测只出 `src/m/item.rs`。
- 同一路径 + 同一语言的多项会**合并进同一个文件**（P-2），按 type→trait→impl→fn→graph 顺序。
- 代码项只能投到 32 个编程语言之一，投到 markdown 等静态格式：
  `error[P-4]: struct P4S 只能投射到编程语言后端（得到 markdown；注册表：python/typescript/javascript/rust/go/cpp/java/csharp/kotlin/swift/ruby/php/lua/perl/bash/powershell/r/julia/scala/elixir/erlang/haskell/ocaml/fsharp/zig/nim/crystal/dart/groovy/objectivec/d/vb）`
- `block`/`static` 只能投到 6 种静态格式之一，投到 rust：
  `error[P-4]: 静态资源 P4B 只能投射到 yaml/markdown/json/toml/ini/xml（得到 rust）`
- 语言别名：`ts→typescript`、`js→javascript`、`py→python`、`md→markdown`、`yml→yaml`、`c++→cpp`、`sh/bash→bash`。
- 后端分档（`targets`）：python/typescript/javascript 是 full 活体翻译；rust/go/cpp 只翻"logic 语句子集"；其余 25 个是"contract 类型契约"（只保证契约/字段，不保证语义）。
- emit 产物：每个文件带 `@dhv:source-map` 围栏 + 头注释 `本文件由 DHV v0.2.72 ... @dhv:generated`；`manifest.json` 记录 `{dhv, entry, scale, backends, generated_at, files:[{path,lang,tier,bytes,items,syntax_check,...}], warnings, protocol}`。
- python 产物会用 `py_compile` 校验（out 下会留 `__pycache__`）；rust 用括号平衡启发式。
- **X-1 / X-5 警告意味着生成物会引用未定义名**，且 emit 此时**退出码 1**（虽然打印"✓ emit 完成"）：
  `warning[X-1]: 类型 X 被 <file> 引用但未投射到 <lang>（生成物引用未定义名）`
  `warning[X-5]: 引用枚举变体 IDLE 未投射到 python（生成物将引用未定义名，运行期报错 —— 请在 project{} 中补投该枚举）`
  → CI 里把 emit 当断言用：0 exit 才算投射完整。
- `sync` 根据生成文件里的 `@dhv:source-map` 围栏把修改写回 HSL 镜像；`--root DIR` 指定根。

---

## 11. block / static（资源内嵌）

```hsl
const LIMIT: i64 = 7;
fn helper(x: i64) -> i64 { x * 2 }

block SYSTEM_PROMPT {
你是 SENTRY 的分诊审计员。
上限 = {{LIMIT}}，示例 = {{helper(3)}}。
JSON 示例：{"verdict":"accept"}     // 单花括号原样保留，不做插值
}

static HARNESS_CONFIG {
{
  "max_turns": 9,
  "model": "scripted"
}
}
```

- `block` 与 `static` 是同一个 `StaticResourceDef`，只有投射格式约束不同（都要在 6 种静态格式内）。
- 正文里：**单花括号 `{...}` 原样输出**（写 JSON/YAML 安全）；只有 `{{表达式}}` 才求值。`{{"{"}}` 可输出 `{`。
- 表达式可引用 const、函数调用、`{:?}` 等（`{{LIMIT}}`→`7`、`{{helper(3)}}`→`6`）。
- **只能插值"值"**，不能插类型/typealias：`{{Name}}`（别名）→ run 报 `✗ 运行期错误：未绑定变量 "Name"`。
- 正文缩进原样保留（"写到最左列，WYSIWYG"）；emit 时按目标格式渲染。首行前导换行也会保留。

---

## 12. native（typescript / python）+ `$host`

```hsl
struct Decision { accept: bool, reason: String }

fn read_cfg() -> Result<String, String> {
    let p = String::from("README.md");
    let txt = native typescript {
        $host.fs.read(p)                        // 捕获 p；最后一个表达式自动作为返回值
    };
    Ok(txt)
}

fn judge(x: i64) -> String {
    native python {
        "high" if x > 5 else "low"             // python：顶层 exec，最后表达式即返回值
    }
}
```

规则（全部实测）：

- `native typescript { ... }`：**同步体内进程执行**（`new Function` 包 async IIFE）。
  - 无 `return` 时自动包成 `return (最后表达式);`；有 `return` 则按写的来；`await` 可用。
  - 作用域捕获：body 里出现的、HSL 环境里存在的标识符按值注入（`$` 开头的不注入）；**字符串字面量里出现同名标识符也会被捕获**（实现是文本扫描）。
  - `Ok`/`Err`/`Some`/`None` 已注入 shim；`return null` 视为 unit/undefined。
  - 空分组 `()` 非法（check N-6 会提示 `Ok(())` 的写法问题）。
- `native python { ... }`：**顶层 exec**（不是函数体）。
  - 写 `return` 会 `SyntaxError: 'return' outside function`：
    `✗ 运行期错误：native python 块执行失败：File "<string>", line 6 SyntaxError: 'return' outside function`
  - 返回值 = **最后一条表达式**（实现自动 `__hsl_result__ = (...)`），或显式赋 `__hsl_result__`。
  - 捕获变量通过 JSON 注入 globals（`HSL_NATIVE_CTX`）；dict 回来是 foreign 对象。
  - 启动慢一点（约 1s+，每次调起 python3 子进程），别在热循环里用。
- 除 ts/python 外的语言：check 能过（按 N-1 只校验是否注册），run 报
  `✗ 运行期错误：native <lang> 后端未接入解释器（dhv-ts 运行期支持 typescript / python；...P5）`
  （它们只对 emit 有意义。）
- 注册校验：`error[N-1]: native 语言 "cobol" 未注册（已注册：rust/python/typescript/...）`。
- `$host` **只能在 native 块里用**。在普通 HSL 里写 `$host.fs.read(...)`：
  `error[E-0]: 无法解析的表达式起点 "$" (punct)`。
- 值跨界：bool/number/String/Vec/Option/HashMap 直通；struct→plain object；enum→`{__variant,...}`；bigint→Number 精度会掉。
- **plain object / array 回来是 "foreign" 值**：字段直通可读，但 `.clone()`、方法调用、模式匹配都失效（#L-22）。
  - 想要"真正的" struct/enum，用 `$host.make`：
    ```hsl
    native typescript {
        $host.make("Decision", { accept: true, reason: "ok" })          // 结构体：字段必须齐全、不许有多余字段
        $host.make("Phase::Act", null)                                  // 单元变体
        $host.make("Result::Ok", [1])                                   // tuple 变体 payload 是数组
    }
    ```
  - `let d: Decision = native ... {...}` 不给 `$host.make` 会触发 S-18 warning（见 §4）。
- `$host` 表面（host.ts 暴露面；下表中 **✔** 的已实跑，其余按源码签名/文档）：
  - ✔ `$host.config`：{model, workspace, task, temperature, maxTurns, maxBashCalls, maxOutputChars, allow, scale, outdir, quiet}（实跑读到 `model=="scripted"`）
  - `$host.llm.complete(prompt, opts?)`
  - ✔ `$host.fs.read(path)`、✔ `fs.write(path, content)→字节数`、`fs.edit(path, old, new)`、✔ `fs.list(dir?, depth?)→按行字符串`（路径受 jail，会解析 symlink）
  - `$host.shell.run(cmd, {cwd, timeoutMs})→{stdout, stderr}`（白名单默认 bun/node/ls/cat/grep/diff；可用 `--allow` 扩展）
  - ✔ `$host.json.parse/stringify(pretty 2)/fields`（`fields` 返回 `HashMap<String,String>`：**所有值都转成字符串**，`1` → `"1"`）
  - ✔ `$host.artifacts.write(name, content)→路径`、✔ `$host.events.emit(name, data)`、`$host.log`
  - `$host.fixture.{next, nextAct, nextReview, actsLeft}`（scripted 模式）
  - `$host.env`、`$host.dhv.run`
- 实测 `$host.shell.run("bun --version")` 在本机返回 wsl.exe 帮助文本（bun 是 wsl shim）——别依赖 shell 输出格式。

---

## 13. macro_rules!（只支持定长）

```hsl
macro_rules! bump {
    ($field:expr, $by:expr) => { $field += $by; };
}

macro_rules! square {
    ($x:expr) => { $x * $x };
}

// 用法
let mut count = 3;
bump!(count, 2);            // count == 5
println!("{}", square!(4));    // 16
println!("{}", square!(2+3));  // 11 ← 原始 token 代入，不自动加括号！！
```

- 定长宏（固定数目、固定片段）实测 OK：`bump!`、`square!`、`add!`、`greet!`、`call_thrice!`。
- 定义必须在使用之前；宏体是 token 级替换：**调用处传表达式要自己加括号**（`square!(2+3)` = 11 的坑）。
- 内建宏（format/vec/println/print/eprintln/panic/assert/assert_eq/dbg）不走 macro_rules!，直接用。
- **重复展开 `$(...)*` / `$(...),*` 在 0.2.72 是坏的**：
  - `$($x:literal),*` 调用 → `error[E-0]: 宏规则期望 "("`
  - `$($x:expr)*` 调用 → `error[E-0]: 定界树未闭合`
  - `$($x:tt)*` check 过、run 报 `✗ 运行期错误：无法解析的表达式起点 "," (punct) (<macro>:5:32)`
  - nova 里 `($name:ident, [$($cap:literal),*])` 这种写法**调用即失败**：`error[E-0]: 宏调用与任何规则都不匹配：ClockTool , [ ]`
  - 结论：**别用重复宏**；要"批量"就写普通函数（`fn add_all(v: Vec<i64>)`）或逐个展开。
- 宏实参里有带逗号多参数 turbofish 会坏（见 §8）。

---

## 14. TOP-20 陷阱（诊断原文 + 修法）

> 按"写大 harness 时踩到会卡最久"排序。诊断原文均可直接 grep 定位。

1. **UTF-8 BOM** → `error[E-0]: 期望项定义，得到 "﻿" (ident) (...:1:2)`
   修：用无 BOM UTF-8 写 `.hsl`（write 工具 / `[IO.File]::WriteAllText` + `UTF8Encoding($false)`）。
2. **`HashSet` 是假的** → run `✗ 运行期错误：HashMap 没有方法 "contains"`
   原因：`HashSet::new()` 返回 `new Map()`，MAP_METHODS 无 `contains`。修：用 `HashMap<K,bool>` 或 `Vec`+`contains`/去重再 `unique`。
3. **`import * as m from "./x.hsl"` 后构造结构体** → `error[E-0]: 期望 ";"，得到 "{" (punct)`
   修：结构体/trait 用具名 import；命名空间只用于函数/const。
4. **std 模块用命名空间导入** → check 过、run `✗ 运行期错误：foreign 没有方法 "to_snake"`
   修：`import { to_snake } from "std/text"`（具名，永远）。
5. **引用类型** → `error[E-0]: 无法解析的类型 "static" (label)`（写 `&'static str` 时）
   修：返回 `String`；短生命周期用 clone，别想借。
6. **native 返回 plain object 当结构体用** → `warning[S-18]: ...（foreign 值：字段直通可用，.clone()/方法/模式派发失效，#L-22）`，之后 `.clone()`/match 报错。
   修：native 里 `$host.make("Type", {...})`；或返回 JSON 字符串在 HSL 侧重建。
7. **native python 里写 `return`** → `✗ 运行期错误：native python 块执行失败：File "<string>", line N SyntaxError: 'return' outside function`
   修：python 块是顶层 exec；用最后表达式或 `__hsl_result__ = ...`。
8. **在普通 HSL 里调 `$host`** → `error[E-0]: 无法解析的表达式起点 "$" (punct)`
   修：`$host` 只在 `native typescript { ... }` / `native python { ... }` 里出现。
9. **`format!` 写了 Rust 风格说明符**（`{:>4}` `{:05}` `{:x}`）→ 不报错但不生效：`{:05}` 42→"42"、`{:x}` 255→"255"
   修：只信 `{}` `{N}` `{:?}` `{:.N}` `{{}}`；补零用 `std/text::pad_start`，hex 手写。
10. **macro 重复展开** → `error[E-0]: 宏调用与任何规则都不匹配：...` / `error[E-0]: 定界树未闭合`
    修：0.2.72 别用 `$(...)*`；宏只写定长；批量逻辑用函数。
11. **graph 节点忘写 `node mut`** → run `✗ 运行期错误：不能对不可变绑定 "phase" 赋值（S4：不可变优先，请用 let mut）`
    修：要改的节点 `node mut`；`let` 同理 `let mut`（否则 check 期 `error[S-4]`）。
12. **graph 的 loop 里用 `_` 兜底 match** → `error[S-6]: graph AgentLoop 内的枚举 match 不允许 _ 通配兜底（必须显式穷尽，直面新分支）`
    修：逐变体列出；新增变体时故意让 check 炸（这是 spec 想要的行为）。
13. **match 漏变体** → `error[S-6]: match 不穷尽：Color 缺少变体 Blue`
    修：补臂；普通函数可用 `_`，AgentLoop 内不行。
14. **未使用绑定/导入**（S-7 是 error 不是 warning）→ `error[S-7]: 绑定 "unused" 声明后未使用（_ 前缀可豁免）` / `error[S-7]: import "helper" 未使用`
    修：`let _ = ...`、变量加 `_` 前缀；trait 只在方法调用隐式用到时把 import 删掉或在类型位置用一次。
15. **同作用域同名** → `error[S-8]: 同作用域遮蔽："x" 已声明（S8）`
    修：换名，或包进 `{ let x = ...; }`。
16. **`if 1 {}` / `while 0 {}`** → check 过，run 才炸 `✗ 运行期错误：if 条件必须是 bool（S1：零隐式转换），得到 1`
    修：条件必须是 bool；写完必 run。
17. **`?` 用在非 Result/Option 上** → check 过，run `✗ 运行期错误："?" 只能用于 Result/Option（S5），得到 int`
    修：函数签名对齐；Option/Result 才能 `?`。
18. **`Self` 当返回类型** → check 过，run `✗ 运行期错误：未知的结构体 "Self"`
    修：impl 里写具体类型名。
19. **`Box::new(UnitStruct)` 省略 `{}`** → check 过，run `✗ 运行期错误：foreign 没有方法 "apply"`
    修：一律 `Box::new(Double {})`。
20. **`Trait::method(obj)` 静态调用** → 能过 check，返回值是 `()` 垃圾
    修：只用 `obj.method()` 方法调用形式。

**额外高频**：
- `String` 下标 → `✗ 运行期错误：String 不支持索引`；用 `chars()`/`char_at`。
- `(if c {1} else {2}) + 3` → 解析失败（13.3 #26）；先 `let t = if ...;`。
- `block` 里插值 typealias → `✗ 运行期错误：未绑定变量 "Name"`；只能插值。
- `project{}` 投射到不存在的目标 → `error[P-3]: 投射目标 "Ghost" 未定义（或未 import）`。
- `rules{}` 写在 `project{}` 外 → `error[E-0]: 期望项定义，得到 "rules" (ident)`。
- 生成物 dangling 引用 → `warning[X-1]` / `warning[X-5]`，且 **emit 退出码 1**。
- 整数溢出**不报错**（BigInt 任意精度）；要限域就自己 clamp / 写显式校验。

---

## 15. dhv-ts 已知限制（写大 harness 必看，来自 guide 13.3 + 实测）

1. 无完整类型推断/校验：注解基本被忽略（`let x: i64 = "s"` 能 run）。别把类型注解当安全网；`run` + `emit` 才是验证。
2. impl 方法按"类型名"全局注册 ⇒ **全项目类型名唯一**；跨模块同名 struct 会串方法。
3. 下标赋值只支持简单 `v[i] = x`。
4. S-7 import 检查是源码行扫描：字符串里出现同名符号也算"用过"（可能漏报未使用）。
5. static/block 正文 emit 时近乎原样，缩进即排版。
6. 解释器整数是 BigInt：不溢出、不回绕，和 rustc 后端行为不同。
7. graph 的 `state` 实参是引用语义（见 §9），传之前想清楚要不要 `.clone()`。
8. 宏重复展开不可用；定长宏是 token 替换（注意括号）。
9. 跨后端漂移：S-14/S-13 这类问题在 python/js 后端"静默放行/产生垃圾"，只有 rust 拒绝；要保证可移植就按 check 的严格要求写。

---

## 16. 写 1500 行 harness 的实操清单

- [ ] 模块布局：types/（纯数据）/ logic/（纯函数+impl）/ agents/ / providers/ / config/resources.hsl（block/static）/ 入口（import + graph + main + scale + project）。
- [ ] 每个 enum 在 graph 的 AgentLoop 里**逐变体 match**，不用 `_`。
- [ ] 需要改的状态用 `mut state: S` + `node mut`；需要跨调用隔离就 `.clone()`。
- [ ] 错误统一 `HarnessError`，各模块 `impl From<X> for HarnessError` 后用 `?`。
- [ ] 只用 §7 列出的方法与 §6 列出的 std 导出；std 一律具名 import。
- [ ] 格式化只用 `{}` `{N}` `{:?}` `{:.N}`。
- [ ] 宏只用定长；重复逻辑写函数。
- [ ] native 只用于 `$host` 调用；返回结构体必须 `$host.make`。
- [ ] `check` 0 error 之后必须 `run`（S-1/S-5 只有 run 才炸）；再 `emit` 并确保退出码 0（X-1/X-5 视为失败）。
- [ ] 写文件用 UTF-8 无 BOM。
