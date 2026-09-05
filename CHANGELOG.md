# 变更日志（CHANGELOG）

本文件记录工具链版本演进；语言规范级变更另见 [toolchain/hsl-spec/BNF.md §8](toolchain/hsl-spec/BNF.md)。

## [0.2.56] (2026-09-05)

**#L-22 native 值模型断层修复批次**（来源：Curator SUT 泛化实验实录 `entities.clone()` 运行期 panic「foreign 没有方法 clone」；先证据链后修复，158 用例回归全绿）：

- **`$host.make` 结构体/枚举变体构造通道**（interp.linkProgram 幂等注入 hostApi，校验与结构体字面量同规则）：native 块返回的 plain object 不带 `__struct`/`__enum` 运行时标记 → foreign 值（字段直通可用，clone/方法/模式派发全失效）。此前唯一出路是「native 拍平字符串 + HSL 侧 split_once 逐字段重建」定式（Curator ~40 行协议代码 + 值不得含 `~`/`|` 的协议保留字约束）。现 `$host.make("Entity", {...})` 直接产出带标记合法值；命名字段变体 `$host.make("Status::Pair", {a, b})`；元组变体走数组 payload；单元变体无 payload；prelude 族（Result::Ok/Err、Option::Some/None）显式镜像。字段完备性/多余性/元组长度校验失败可观测报错。SUT 侧验证：Curator parse_extract 以 $host.make 重写（拍平协议全删），15/15 场景黄金输出逐字节等价。
- **S-18 native 值模型断层预警**（dhv-ts checker + dhv typecheck.rs 双端同口径，conformance 第 6 段「预警对等」锁定 —— 退出码对拍看不见警告是否产出）：`let <含 struct/enum 族名注解> = native <...>` 且体无 `$host.make` → 警告（foreign 值：字段直通可用，clone/方法/模式派发失效）。只判同现场 let 注解 + 初始化器（零误报面窄）；String/数值注解的合法拍平协议不触发。
- **锁定用例**：run-all 149→158（$host.make 结构体/命名字段/元组/单元变体/错误路径 ×5 + S-18 触发 ×2 + 零误报 ×2）；conformance 62→66（check fixtures +2 + S-18 预警对等 ×2）；dhv cargo test 15/15；dsh/nova/backends-demo/Vigil/Curator 零回归。

## [0.2.52] (2026-09-02)

**静态产物真校验批次**（来源：为「38 后端全测」巡检建立产物级断言时触发，先最小化复现再修复，111 用例回归全绿）：

- **静态 json 后端真校验**（backends/validate.ts `validateStaticGeneratedFile` + emit.ts 接线，校验盲区级）：此前静态格式后端的 `syntax_check` 无条件标 `pass`（tool=`embedded`）——任意内容投 `.json` 都绿灯。实测：官方示例 backends-demo 把 **YAML 内容**的同一 block 投到 `config/agent.json/.toml/.ini/.xml`，5 个生成物**格式全部非法**却全部通过校验（manifest 证据：`syntax_check: "pass", syntax_tool: "embedded"`，`JSON.parse` 报 `Unexpected identifier "agent"`）。现 json 静态产物用 `JSON.parse` 真解析（bun 内建，零依赖）：失败 → 输出 `语法✗ json.parse`、manifest 记 `fail`、emit 退出码 1（与编程语言后端校验失败的既有行为一致）；yaml/toml/ini/xml/markdown 宿主无零依赖校验器，如实保持 `embedded`。锁定用例：`run-all.ts`「静态 json 真校验（非法红灯 / 合法绿灯）」——负例断言 exit 1 + manifest fail，正例断言合法 JSON（含 `{{}}` 插值渲染为 24）可真解析。
- **backends-demo 静态块重写**（examples/backends-demo/agent.hsl，示例正确性）：`agent_config` 拆为六个格式各自的合法内容（yaml / json / toml / ini / xml + markdown 独立），同一 agent 配置语义、`{{MAX_TURNS}}` 插值在六种格式中一致渲染为 24。原写法「一个 YAML block 投五种后缀」在上条修复后必然红灯，本条让示例重新成为「六静态后端各自合法」的正例示范。
- **工具链版本串三处硬编码漂移**（新增 `dhv-ts/src/version.ts` 单一来源）：横幅（main.ts）、manifest `dhv` 字段（emit.ts）、生成文件头注释（decls.ts）三处各自硬编码，全部停留在 `0.2.10`（package.json 实为 0.2.5x）——manifest 声称的工具链版本与真实二进制不符。现统一从 package.json 读取（`version.ts`），删一处忘一处不再可能。
- **新增 `toolchain/tests/verify_backends.ts`**：38 后端全量覆盖校验脚本（manifest 全 pass / 零告警 / 注册表 `ALL_LANGS(38)` ↔ 产物语言集合**双向相等** / 静态 json 内容级真解析），供 15 分钟巡检与每日定时任务共用。

## [0.2.51] (2026-09-02)

**外部实测驱动的修复批次**（来源：以独立项目实测 HSL 撰写数学/物理验证 harness 与红蓝对抗 harness 过程中触发的问题，全部先最小化复现再修复，110 用例回归全绿）：

- **整数值浮点除法静默截断**（interp.ts `evalBinary`/`evalCompound`，语义正确性）：JS 中 `1.0 === 1`，整数值的浮点字面量在运行期丢失浮点性，此前的「双整数值 → 截断」启发把 `1.0/7.0` 静默算成 **0**（armorlab 规避率/误报率全为 0.000 的直接根因）。现引入与 backends/body.ts `exprKind` 同构的**静态浮点性探测**（`exprFloaty`：float 字面量 / `as f32|f64` 转换 / 算术二元递归 / 显式 f64·f32 类型注解的绑定——Env 记录 `floatTy`，Rust 语义下类型不可变更故可靠），命中即真实除法；整数截断语义（`7/2=3`）完整保留，110 用例零回归。
- **宏实参重建丢引号**（body.ts `tokensToExpr`，双重隐患）：string/char/rawstr token 的 `text` 是**不带引号的裸内容**，此前直接 join 后重词法化——宏实参里含 `'` 的字符串（SQL 注入签名 `"' or 1=1"`）触发 LexError；不含特殊字符的静默变成**标识符**（错误但不报错）。现按 token 类型重建可词法化源码（string → JSON 转义引号、char → `'…'`、rawstr → `r"…"`/`r#"…"#`）。附带：`'` 开头的 LexError 现在携带后续内容上下文（此前只有行列号，定位困难）。
- **跨文件函数/常量/枚举变体依赖接线**（emit.ts，NameError 族）：类型接线自 v0.2.4 存在，但**函数与常量从未接线**——活体翻译的函数体调用其他投射函数（`inspect_payload` 调 `normalize`）、引用投射常量（`recompute` 用 `G`）、构造/匹配枚举变体（python/ts 展平为裸名 `Blocked`/`PASSED` 单例）时，生成物一律 NameError。现新增 `collectCallableRefs` + `importHeaderForFnDeps`（与类型接线同构：python 同目录 `from X import Y` / ts 相对 import / rust `use crate::` / go 同包免导入 + X-2~X-4 跨目录诚实告警）；变体接线含无负载变体的 snakeUpper 单例名；宏实参内的结构体字面量（`vec![Quantity{…}]`）与函数调用（`ident(` / `Enum::Variant`，注意宏树中 `(`、`{` 是 delim 节点而非 punct token）同步收集。**接线行置于运行期助手 def 之前**（置于其后会在「exec 裸函数体」消费形态下遮蔽消费方提供的同名桩类，tests/hsl 语义级验证实录）。
- **std/math 自由函数与常量活体映射**（body.ts `stdMathFreeCall` + decls.ts prelude）：std 函数调用此前在活体翻译中**裸名直出**（`sin(x)` 生成 `sin(x)`，python NameError / ts 编译错误；`PI` 同样裸奔）。现 python（`math.sin`/`math.pi`，prelude 加 `import math`）与 ts/js（`Math.sin`/`Math.PI`，`Number.isNaN` 等特例）全量直译；rust/go/cpp 的自由函数是方法形态（`(x).sin()`），不可廉价映射 → 抛 TranspileError 触发**诚实 contract 回退**（此前生成必炸的裸名代码却能通过启发式校验）。实测 mathphys 全链生成物（`recompute`/`judge_formula_v`/`formula_result_dim`）与解释器输出逐位一致（40.8163 / 2.0071 / Dim(m=0,l=1,t=0)）。
- **E-2 未知函数调用静态检查**（checker.ts，盲区级）：此前调用未定义/未 import 的函数（如漏写 `import { sort_desc } from "std/collections"` 直接调用）`check` 全绿、`run` 才报 `"sort_desc" 不是可调用项` —— 符号解析是纯静态可判定的，不应留到运行期。现对**单段路径调用**（`f(...)`）做三重豁免后报 `error[E-2]`：局部绑定（闭包值/参数/match 绑定）、本文件可见名（顶层 fn/graph/macrodef + import 别名 + 嵌套 fn 源码行扫描）、预导入构造器白名单（Ok/Err/Some/None）；两段路径（`Type::variant`）保守不查。连带：函数/graph/impl/trait 方法参数进入作用域（`param` 标记豁免 S-7 —— 未使用参数是合法风格，防误报）；**std 导入名校验**（`import { levenshtei }` 拼写错误此前同样要到 run 才炸，现报 E-2 并附候选名提示）。nova 15 模块 / dsh / backends-demo 全部零误报通过。
- **`native python` 块公共缩进去除**（native.ts，正确性）：`scanRawBody` 按源码原样搬运块体，python 路径此前不做 dedent 直接拼进子进程 wrapper —— 块体书写缩进恰好对齐 wrapper 的 for 循环体（4 空格）时"碰巧能跑"（且末表达式会被执行「捕获变量个数」次）；嵌套更深（8 空格）直接 `IndentationError`。现统一 textwrap.dedent 语义去公共缩进，任意嵌套深度语义一致。
- **native python 末表达式语句误判**（native.ts `transformPythonBody`，静默丢值）：`"sqrt2=%.6f" % math.sqrt(2)` 这类末表达式因**字面量内的 `=`** 被赋值探测正则误判为语句，返回值静默丢失（得到 unit）。现语句判定先剥离字符串字面量再进行。
- **`format!` 精度说明符 `{:.N}` 落地**（values.ts `hslFormat` + body.ts `formatString`，静默语义丢失）：此前 `format!("{:.3}", 3.14159)` 在解释器输出 `3.14159`（精度被静默丢弃）、python 活体翻译生成 `f"{x}"`（丢失 `:.3`）—— 按「宁缺毋滥」纪律，被解析器接受却静默改语义是不可接受的。现三端一致实现 `.N` 浮点十进制精度子集：interp `toFixed(N)`（NaN/inf 特判）；python 活体 `f"{(x):.3f}"`（经 exec 语义级验证 3.142）；ts/js `${(x).toFixed(3)}`；rust `{name:.3}` 原生内联捕获 / `{:.3}` 位置参数（真机形态）；go `%.3f`；cpp `std::format("{:.3}")`。`{:?}`/`{}`/`{0}`/`{{` 行为不变。
- **IDE（VSCode 扩展）v0.1.1**：① `hsl.compile` 此前执行 `dhv emit <file>` **缺必填 `--out`**（退出码 2，命令必失败）—— 现新增 `hsl.outDir` 配置（默认 `.hsl-gen`）并拼接 `--out`；② 语法高亮六处修正：投射项语言列表 7 种 → **38 后端全量 + 别名**（`go/cpp/java/vb/zig…` 此前不高亮）；`native` 块语言 3 种 → 32 种编程语言（静态格式后端正确排除）；无后缀浮点 `3.14` / 指数 `1e-9` 此前按整数+点分断高亮，现正确识别为 float；原始字符串零井号形态 `r"..."` 此前不匹配（`r#+` 要求至少一井号）；字符字面量 `'H'` 被标签规则抢吞（`'outer` 无闭引号即命中 label）—— 现 char 规则前移 + 标签加 `(?!')` 负向断言双保险；`block`/`static` 资源块关键字入表，移除 HSL 不存在的 `mod`/`use`。
- **新增 `ide/tests/validate.js`**：IDE 扩展发布级校验脚本（JSON 合法性 / tmLanguage 全 regex 可编译 / 38 后端高亮覆盖 / char-label 冲突回归 / 数字形态矩阵 / native 语言集合 / 扩展入口 `node --check`），`bun ide/tests/validate.js` 一键回归。

## [0.2.50] (2026-09-01)

- **Ruby 专属后端新增**（codegen/ruby_backend.rs，~900 行）：Ruby 是 Tier 2 脚本与动态语言（registry.ts body: 'contract'），此前走通用 contract 后端（纯注释式契约），现生成真实 Ruby 3.x 代码。struct → class + attr_accessor + initialize（named）/ Struct.new()（tuple/unit）；enum（unit）→ module + 常量；enum（data）→ class 继承体系；trait → module（mixin duck typing）；impl → class + include module；fn → 顶层 def；const → 常量（UpperCamelCase）；graph → 顶层执行代码块。30+ 种表达式转译：binary/unary/call/method/field/index/slice/range/assign/compound_assign/if/else/match→case/for→each/while/while-let/loop/closure→lambda/return/break→break/continue→next/array/struct/tuple/block/try→begin/rescue/await/cast/native/macro（println→puts, panic→raise RuntimeError）。90+ Ruby 关键字避让表。类型映射对齐 langs.rs Ruby TypeMap：String/Integer/Float/Boolean/Array/Hash/Set/nilable/Object/nil。50+ std 方法映射。后端注册：ruby 优先于 contract 注册，contains_key 检查排除重复。专属后端数量 16 → 17（rust/go/python/typescript/cpp/java/csharp/kotlin/swift/scala/dart/elixir/haskell/ruby + 3 static），首个 Ruby 语言专属后端，消除 dhv 与 dhv-ts 的 Ruby 能力声明不一致。能力分级：3 Full + 12 Logic + 17 Contract + 6 Raw = 38 后端。langs.rs 能力标签更新（ruby Contract→Logic，family_for 从 Script 移至 OOClass）。

## 0.2.49 (2026-09-01)

- **Haskell 专属后端新增**（codegen/haskell_backend.rs，~1804 行）：Haskell 是 Tier 3 纯函数式语言（registry.ts body: 'contract'），此前走通用 contract 后端（纯注释式契约），现生成真实 Haskell 2010+（GHC）代码。struct → data Name { field :: Type } deriving (Show, Eq)（named）/ type alias（tuple）/ data unit（unit）；enum（unit）→ data Name = V1 | V2 deriving (Show, Eq, Bounded, Enum)；enum（data）→ data Name = V1 T1 T2 | V2 { field :: T } deriving (Show, Eq)；trait → class TraitName a where（type class + 方法签名）；impl → instance TraitName ConcreteType where（实例定义）；fn → 顶层函数签名 + 定义；const → 类型签名 + 值绑定；graph → main :: IO () + main = do ...。完整表达式转译（30+ 种 ExprKind）：binary（&&/||/==/!=/++ 等）/unary（not/negate）/call（函数应用 juxtaposition）/method（std 方法映射）/field（记录字段函数）/index（!!）/slice（take/drop）/range（[lo..hi]/[lo..hi-1]）/assign（let 绑定）/if/if-let/match→case（模式匹配）/for→mapM_/while→递归辅助/loop→forever/closure（\params -> body）/return/break/continue/array/[...]/struct literal（记录语法）/tuple/(,,)/block（let...in...）/try→Control.Exception/await→<- do/cast→fromIntegral/native/macro（println!→putStrLn, panic!→error, format!→concat）。27+ Haskell 关键字避让表（用 prime 后缀转义）。类型映射对齐 langs.rs Haskell TypeMap：String/Char/Bool/Int/Int64/Word32/Word64/Float/Double/[%T]/Map K V/Set T/Maybe T/Either E T。50+ std 方法映射（Vec→List: push→(:)/++、pop→init、len→length、is_empty→null、sort、reverse、map、filter、foldl/foldr、for_each→mapM_、find、any、all、concatMap、contains→elem；String: trim→strip、to_lowercase→map toLower、starts_with→isPrefixOf、ends_with→isSuffixOf、split→splitOn、replace；Option→Maybe: is_some→isJust、is_none→isNothing、unwrap→fromJust、unwrap_or→fromMaybe、and_then→>>=；Result→Either: is_ok→isRight、is_err→isLeft；Map: insert→Map.insert、get→Map.lookup、keys→Map.keys、values→Map.elems、remove→Map.delete、contains→Map.member）。模块头部含 LambdaCase/RecordWildCards pragmas + Data.Map/Data.Set/Data.List/Data.Char/Data.Maybe/Control.Monad/Text.Read imports。专属后端数量 15 → 16（rust/go/python/typescript/cpp/java/csharp/kotlin/swift/scala/dart/elixir/haskell + 3 static），首个纯惰性函数式语言专属后端，消除 dhv 与 dhv-ts 的 Haskell 能力声明不一致。能力分级：3 Full + 11 Logic + 18 Contract + 6 Raw = 38 后端。

## 0.2.48 (2026-09-01)

- **Elixir 专属后端新增**（codegen/elixir_backend.rs，~1610 行）：Elixir 是 Tier 3 函数式语言（registry.ts body: 'contract'），此前走通用 contract 后端（纯注释式契约），现生成真实 Elixir 1.16+ 代码。struct → defmodule + defstruct（named/tuple/unit）+ @type；enum（unit）→ @type 原子联合 + 模块属性常量；enum（data）→ defmodule + 标记元组（{:Variant, field1, field2}）；trait → behaviour + @callback；impl → defmodule implementing behaviour + @impl；fn → def（顶层函数，带 @spec 类型注解）；const → @模块属性；graph → def main()。完整表达式转译（30+ 种 ExprKind）：binary（and/or 关键字）/unary（not）/call/method/field/index/slice/range（.. / ..//）/assign/compound_assign/if/else/else-if/match/case（含 when guard）/for（Enum.each）/while（defp 递归）/while-let/closure（fn -> end）/return/break/continue/array/struct（%Module{key: val}）/tuple/try（case :ok/:error）/await（Task.await）/cast/if-let/native/macro。60+ Elixir 关键字避让表（def/end/do/fn/if/else/case/cond/when/and/or/not/true/false/nil/receive/try/catch/rescue/raise/for/in/unless/with/use/import/require/alias 等）。类型映射对齐 langs.rs Elixir TypeMap：String→String.t()/char→String.t()/bool→boolean/int→integer/uint→non_neg_integer/float→float/Vec→list/HashMap→map/HashSet→MapSet.t/Option→{:ok, T}|:error/Result→{:ok, T}|{:error, E}/unit→:ok。50+ std 方法映射（Vec→List: push→++, pop→Enum.drop, len→length, is_empty→==[], sort→Enum.sort, map→Enum.map, filter→Enum.filter, fold→Enum.reduce, contains→in; String: trim→String.trim, to_lowercase→String.downcase, starts_with→String.starts_with?, split→String.split; Option: unwrap→elem({:ok,v},1), unwrap_or→case; Map: insert→Map.put, get→Map.get, keys→Map.keys）。format!→"...#{expr}..."字符串插值，println!→IO.puts。专属后端数量 14 → 15（rust/go/python/typescript/cpp/java/csharp/kotlin/swift/scala/dart/elixir + 3 static），首个 BEAM 平台语言专属后端，消除 dhv 与 dhv-ts 的 Elixir 能力声明不一致。能力分级：3 Full + 10 Logic + 19 Contract + 6 Raw = 38 后端。

## 0.2.47 (2026-09-01)

- **langs.rs 能力标签同步**：java/csharp/kotlin/swift/scala/dart 六个语言自 v0.2.41–v0.2.46 已实现专属后端（Logic 级），但 `Capability` 枚举仍标记为 `Contract`，现已全部更正为 `Logic`。能力分级现为：3 Full（python/typescript/javascript）+ 9 Logic（rust/go/cpp/java/csharp/kotlin/swift/scala/dart）+ 20 Contract + 6 Raw = 38 后端。同步更新文件头注释（BNF v1.4 → v1.5）与 codegen/mod.rs 注册表注释。

## 0.2.46 (2026-09-01)

- **Dart 专属后端新增**（codegen/dart_backend.rs，~1504 行）：Dart 是 Tier 4 系统与现代语言（registry.ts body: 'contract'），此前走通用 contract 后端（纯注释式契约），现生成真实 Dart 3 代码。struct → class（named, constructor）/ Record（tuple, Dart 3）/ class（unit）；enum（unit）→ sealed class + static const 实例；enum（data）→ sealed class + subclass + factory constructor（Dart 3 模式）；trait → abstract class（支持默认实现）；impl → class extends abstract class；fn → 顶层函数（支持 async）；const → const/final；graph → void main()。完整表达式转译（28 种 ExprKind）：binary/unary/call/method/field/index/slice/range/assign/compound_assign/if/match(switch expression Dart 3)/for/while/while-let/loop/closure/return/break/continue/array/struct/tuple/try/catch/await/cast/if-let/async-block/native/macro。50+ Dart 关键字避让表（用 $ 后缀转义）。Dart 3 类型映射（对齐 registry.ts TypeMap）：String/bool/int/double/List/Map/Set/T?/void/Never/dynamic。50+ std 方法映射（Vec→List: push→add, pop→removeLast, len→length, is_empty→isEmpty, sort→sort, map, where/filter, fold→fold, any→any, all→every, flatMap→expand, contains, forEach; String: toString/trim/toLowerCase/toUpperCase/startsWith/endsWith/split/replaceAll; Option(?): is_some→!=null, is_none→==null, unwrap→!, unwrap_or→??, and_then→andThen; Map: insert→[]=, keys, values）。format! → 字符串插值 ${expr}，println! → print()。专属后端数量 13 → 14（rust/go/python/typescript/cpp/java/csharp/kotlin/swift/scala/dart + 3 static），首个 Dart 语言专属后端，消除 dhv 与 dhv-ts 的 Dart 能力声明不一致。

## 0.2.45 (2026-09-01)

- **Scala 专属后端新增**（codegen/scala_backend.rs，~1376 行）：Scala 是 Tier 3 函数式语言（registry.ts body: 'contract'），此前走通用 contract 后端（纯注释式契约），现生成真实 Scala 3 代码。struct → case class（named/tuple/unit）；enum → sealed abstract class + case object/case class；trait → trait（支持默认实现）；impl → class extends trait；fn → 顶层 def；const → val；graph → @main def main()。完整表达式转译（33 种 ExprKind）：if/else、match/case（含 guard）、for/while/while-let/loop、closure（lambda）、range（to/until）、try/catch/finally、await（Future）、cast（asInstanceOf）、if-let（match + case）等。45+ Scala 关键字避让表（用 $ 后缀转义）。Scala 3 类型映射（对齐 registry.ts TypeMap）：String/Char/Boolean/Int/Long/Float/Double/BigInt/List/Map/Set/Option/Either/Unit/Any/Nothing。50+ std 方法映射（Vec→List: push→:+=, pop→.init, len→.length, is_empty→.isEmpty, sort→.sorted, map, filter, fold→.foldLeft, for_each→.foreach, find, any→.exists, all→.forall, flatMap, contains; String: to_string→.toString, trim, to_lowercase→.toLowerCase, to_uppercase→.toUpperCase, starts_with→.startsWith, ends_with→.endsWith, split, replace, chars→.toCharArray; Option: is_some→.isDefined, is_none→.isEmpty, unwrap→.get, map, and_then→.flatMap, unwrap_or→.getOrElse; Result→Either: is_ok→.isRight, is_err→.isLeft, ok→.toOption; Map: insert→.updated, get, keys, values）。format! → s"..." 字符串插值，println! → println()。专属后端数量 12 → 13（rust/go/python/typescript/cpp/java/csharp/kotlin/swift/scala + 3 static），首个 Scala/JVM 函数式语言专属后端，消除 dhv 与 dhv-ts 的 Scala 能力声明不一致。
- **编译器零警告**：修复 python.rs（4 处）和 typescript.rs（2 处）unreachable pattern 警告（移除枚举已全覆盖的 `_ =>` 兜底分支）；修复 grammar_probe.rs unused variable 警告。dhv --release 编译零警告。

## 0.2.44 (2026-09-01)

- **Swift 专属后端新增**（codegen/swift_backend.rs，~1100 行）：Swift 是 Tier 1 Harness 核心语言，此前走通用 contract 后端（纯注释式契约），现生成真实 Swift 5.9+ 代码。struct → struct (named) / final class (tuple/unit)；enum → enum（unit）/ indirect enum（data，关联值原生支持）；trait → protocol（支持默认实现）；impl → extension；fn → 顶层 func（支持 async）；const → static let；graph → @main struct with static func main()。完整表达式转译（30+ 种 ExprKind）：if/else、switch（match，含 where guard）、for-in/while/while-let/loop（支持 label）、closure（{ x in ... }）、range（..< / ...）、try、await（async）、cast（as!）、if-let（if case let）等。70+ Swift 关键字避让表（用反引号转义）。Swift 类型映射（对齐 registry.ts TypeMap）：String/Character/Bool/Int8-Int64/UInt8-UInt64/Int/Float/Double/[T]/[K:V]/Set<T>/T?/Result<T,E>/Void/Never。专属后端数量 11 → 12（rust/go/python/typescript/cpp/java/csharp/kotlin/swift + 3 static），消除 dhv 与 dhv-ts 的 Swift 能力声明不一致。

## 0.2.43 (2026-09-01)

- **Kotlin 专属后端新增**（codegen/kotlin_backend.rs，~1100 行）：Kotlin 是 Tier 1 Harness 核心语言，此前走通用 contract 后端，现生成真实 Kotlin 代码。struct → data class（named）/ class（tuple/unit）；enum → sealed class + data class/object；trait → interface（支持默认实现）；impl → class implementing interface；fn → 顶层 fun（支持 suspend）；const → const val；graph → class { companion object { @JvmStatic fun main() } }。完整表达式转译（30+ 种 ExprKind）：if/else、when（match）、for/while/while-let/loop（支持 label）、closure（lambda）、range（.. / until）、try/catch、await（suspend）、cast（as）等。60+ Kotlin 关键字避让表。Kotlin 类型映射：String/Boolean/Byte/Short/Int/Long/Float/Double/List/Map/Set/HashMap/HashSet/Pair/Triple/Array/Nothing。专属后端数量 10 → 11（rust/go/python/typescript/cpp/java/csharp/kotlin + 3 static），首个 JVM（非 Java）语言专属后端。

## 0.2.42 (2026-09-01)

- **C# 专属后端新增**（codegen/csharp_backend.rs，~540 行）：C# 是 Tier 1 Harness 核心语言，此前走通用 contract 后端（纯注释式契约），现生成真实 C# 9+ 代码。struct → record（named）/ class（tuple/unit）；enum → sealed abstract record + derived record（data）/ enum（unit）；trait → interface（C# 8+ 默认实现）；impl → class implementing interface；fn → static method；const → const；graph → static void Main()。表达式全覆盖（30+ 种 ExprKind）：binary/unary/call/method/field/index/slice/range/assign/compound_assign/if/if-let/match(switch)/for/while/while-let/loop/closure/return/break/continue/array/struct/tuple/block/try/await/cast/native/macro。新增 80+ C# 关键字避让表、字段首字母大写（C# 惯例）。类型映射对齐 registry.ts TypeMap（string/char/bool/int/long/uint/ulong/nuint/nint/float/double/List<T>/Dictionary<K,V>/HashSet<T>/T?/Func<>）。专属后端数量 9 → 10（rust/go/python/typescript/cpp/java/csharp + 3 static），消除 dhv 与 dhv-ts 的 C# 能力声明不一致。

## 0.2.41 (2026-09-01)

- **Java 专属后端新增**（codegen/java_backend.rs，~1300 行）：Java 是 Tier 1 Harness 核心语言，此前走通用 contract 后端（纯注释式契约），现生成真实 Java 17+ 代码。struct → record（Java 17+）/ class（tuple/unit）；enum → sealed interface + record（data）/ enum（unit）；trait → interface；impl → class implements interface；fn → static method；const → static final；graph → public static void main(String[] args)。表达式全覆盖（30+ 种 ExprKind）：binary/unary/call/method/field/index/slice/range/assign/compound_assign/if/if-let/match(switch)/for/while/while-let/loop/closure/return/break/continue/array/struct/tuple/block/try/await/cast/native/macro。新增 70+ Java 关键字避让表、50+ Java std 方法映射表（Vec→List/Map/Set/Optional）、Java 类型映射（对齐 registry.ts TypeMap）。专属后端数量 8 → 9（rust/go/python/typescript/cpp/java + 3 static），消除 dhv 与 dhv-ts 的 Java 能力声明不一致。

## 0.2.40 (2026-09-01)

- **C++ 专属后端新增**（codegen/cpp_backend.rs，~860 行）：C++ 是 Tier 1 Harness 核心语言，此前走通用 contract 后端，现生成真实 C++17 代码。

## [0.2.37] — 2026-09-01 · 「Rust 后端表达式全面覆盖」版

### 改进
- **Rust 后端从骨架升级为完整表达式覆盖**（codegen/rust_backend.rs）：
  - 表达式种类从 8 种（literal/path/binary/unary/call/method/field/await/try/cast/native）扩展到全部 30+ 种
  - 新增：if/else if/else、match（含 guard）、for、while、while-let、for-in range、loop、closure（含 async/move）、return、break（含 label+value）、continue、array、array-repeat、struct literal（含 spread）、tuple、block expression、async block、assign、compound-assign、index、slice、range、if-let、macro
  - 新增语句级 if/match/for/while/loop 处理（正确缩进与格式，无需 return 包装）
  - 新增 `let mut` 支持（读取 LetStmt.mutable 字段）
  - 赋值语句自动追加分号
  - 新增模式转译函数 `rs_pattern()`（覆盖 ident/wildcard/rest/literal/path/tuple-struct/struct/tuple/or/range）
  - 新增 Const 和 TypeAlias 项支持
- 模块头注释更新：从「P3 骨架」改为「HSL 与 Rust 高度同构，转译接近直译」

## [0.2.36] — 2026-09-01 · 「Contract 后端真实语法输出」版

### 改进
- **Contract 后端从纯注释式升级为真实目标语言语法输出**（codegen/contract.rs）：
  - 31 种 contract 语言不再输出注释式契约，而是生成目标语言可读代码（struct→class/struct、enum→enum、fn→method、const→常量、typealias→using/typealias、trait→interface/protocol）
  - 按语法族（LangFamily）分组生成：OOClass（Java/C#/Kotlin/Swift/…）、CFamily（C++/D/Zig/…）、Script（Ruby/PHP/Lua/…）、Functional（Elixir/Haskell/…）
  - 类型输出使用每语言专属类型映射（i64→long for Java, i64→int for Go, String→str for Python 等）
- **32 语言类型映射表**（langs.rs）：新增 `type_map_for()` 函数，为全部 32 种编程语言提供 HSL→目标类型映射（对齐 dhv-ts backends/registry.ts types: TypeMap），覆盖 String/char/bool/i32~f64/Vec/HashMap/HashSet/Option/Result/Box/unit 等 17 种 HSL 类型
- **语言语法族分类**（langs.rs）：新增 `LangFamily` 枚举（OOClass/CFamily/Script/Functional）和 `family_for()` 函数，32 种编程语言按语法风格归入四族
- 新增回归测试 `type_map_coverage`（每语言必须有 String/i64/bool 映射）和 `family_coverage`（每编程语言必须有语法族）

## [0.2.35] — 2026-09-01 · 「Contract 后端覆盖扩展」版

### 改进
- **Contract 后端覆盖扩展**（codegen/contract.rs）：此前 26 种 contract 语言对 impl/const/type_alias/static_resource 项直接报错。现全部生成类型契约注释：
  - `impl Trait for Type { fn ... }` → 方法签名列表
  - `const NAME: Type = ...` → 常量声明注释
  - `type Alias = ...` → 类型别名注释
  - `static NAME / block NAME（N 部分）` → 静态资源大小注释
- **类型渲染改进**：新增 Array（`[T; _]`）、Slice（`[T]`）、FnPtr（`fn(T) -> R`）、Never（`!`）的正确类型语法输出（此前全部降级为 `Any`）。
- **错误消息改进**：`"暂不支持 {:?} 项"` 改为 `"暂不支持 {} 项"`（显示可读的项类型名，替代 `std::mem::discriminant` 数字）。
- 新增 `fn_sig_text()` 复用函数签名生成（减少 Fn arm 重复代码）。

### 变更
- 版本统一：dhv 0.2.35 · dhv-ts 0.2.35 · BNF v1.5.0。

## [0.2.34] — 2026-09-01 · 「Go 后端 match 解构赋值」版

### 改进
- **Go 后端 match 模式解构赋值**（codegen/go_backend.rs）：此前 Go 后端将所有 match 生成 `switch` 语句，TupleStruct/Struct 带绑定模式输出为注释（如 `/* Some(x) */ true`），无法实际使用。现自动检测解构模式并生成 `if/else if` 链：
  - `Option::Some(x)` → `if v != nil { x := *v; ... }`
  - `Option::None` → `else { ... }`
  - `Enum::Variant { field: binding }` → `if ... { binding := v.Field; ... }`
  - 简单字面/标识符模式保留 `switch`（无回归风险）
  - 对齐 dhv-ts `body.ts matchDispatch` 的 Go 路径（if/else if 链 + 绑定提取）
- 新增辅助函数：`go_arm_info()`（模式→条件+绑定提取）、`emit_match_dispatch()`（统一调度 switch/if-else）、`emit_match_as_if_chain()`、`export_capitalize()`。
- 尾位置 match（函数返回值）正确包装 return（`as_return` 参数）。

### 变更
- 版本统一：dhv 0.2.34 · dhv-ts 0.2.34 · BNF v1.5.0。

## [0.2.33] — 2026-09-01 · 「Parser 错误消息中文化」版

### 改进
- **parser 错误消息中文化**（parser.rs）：pest 原始英文错误消息（如 `expected block_comment or identifier`）转为中文「期望 X，得到 Y」格式（如 `期望 标识符，得到 文件结束`），对齐 dhv-ts 诊断风格。实现：提取 `ErrorVariant::ParsingError` 的 `positives`/`negatives` 规则名 → `rule_friendly_name()` 映射为中文术语（标识符/类型/表达式/函数参数/模式/语句/路径/守卫表达式等 20+ 种）→ 过滤注释/空白规则 → 拼接。span 改用 `Pos/Span` 双分支正确计算错误跨度（此前 Span 分支的 end 被丢弃）。

### 变更
- 版本统一：dhv 0.2.33 · dhv-ts 0.2.33 · BNF v1.5.0。

## [0.2.32] — 2026-09-01 · 「E-1 重复项名检查」版

### 新增
- **E-1 顶层重名检查**（typecheck.rs）：同一文件内出现同名顶层项（fn / struct / enum / trait / const / typealias / graph / static / macrodef）时输出 `ERROR[M-E1]`，附带 note 建议重命名或移除重复项。作用域为每文件独立（不跨模块），首次定义静默接受、后续重复报错。跳过 import / impl / macro_call（无独立项名）。对齐 dhv-ts `checker.ts` E-001。
- 回归用例 `errors/E1_duplicate_top_item.hsl`（双编译器一致：fn 与 struct 同名 → E-1）。

### 变更
- 版本统一：dhv 0.2.32 · dhv-ts 0.2.32 · BNF v1.5.0。
- 双编译器一致性：38 → 39 组用例。

## [0.2.31] — 2026-09-01 · 「G-3 无条件环检测」版

### 新增
- **G-3 无条件环检测**（typecheck.rs）：graph 拓扑中若存在所有边均无 `on Guard` 的环，输出 `ERROR[G-G3]`（编译期可判定死锁）。算法：构建邻接表 → 对每个起点 DFS 寻找回连路径且路径上无 guard → 报错。链式边（`a -> b -> c`）展开为二元边参与检测。对齐 dhv-ts `checker.ts` G-3。
- 回归用例 `errors/G3_unconditional_cycle.hsl`（双编译器一致：无条件环 → G-3）。

### 变更
- `check_graph` Pass B 新增 `edge_list` 收集（为 G-3 提供邻接表数据）
- 版本统一：dhv 0.2.31 · dhv-ts 0.2.31 · BNF v1.5.0。

## [0.2.30] — 2026-09-01 · 「M3 import 未 export 检查」版

### 新增
- **M3 import 未 export 检查**（typecheck.rs + lib.rs）：`import { Secret } from "./helper.hsl"` 若 `Secret` 未被 `helper.hsl` export 则输出 `ERROR[M-M3]`。对齐 dhv-ts `checker.ts` M3。实现：`harvest_module` 新增 `module_path` 参数收集每模块 export 名集合（`module_exports: HashMap<String, HashSet<String>>`）；新增 `check_m3_imports` 方法在 harvest 后对根文件和依赖模块的 import 逐一校验。namespace/glob import 豁免（与 dhv-ts 一致）。
- 回归用例 `modules/fail_M3_not_exported`（双编译器一致：导入未 export 的名 → M3）。

### 变更
- `harvest_module` 签名新增 `module_path: &str` 参数（lib.rs 调用点同步）
- 版本统一：dhv 0.2.30 · dhv-ts 0.2.30 · BNF v1.5.0。

## [0.2.29] — 2026-09-01 · 「诊断信息质量提升」版

### 新增
- **G-4 孤岛节点警告**（typecheck.rs）：graph 体内声明了 `node` 但无任何 `edge` 引用时输出 WARNING[G-G4]，提示可能遗漏了连接。对齐 dhv-ts `checker.ts` G-4。
- **N-1 native 语言标识校验**（typecheck.rs）：`native nonexistent_lang { ... }` 输出 ERROR[N-N1]，此前未注册语言静默通过。对齐 dhv-ts `checker.ts` N-1。首个使用 `DiagCode::NativeSafety` 的实际检查。
- 回归用例 `errors/N1_unregistered_native_lang.hsl`。

### 改进
- **rules 展开诊断补齐 note**（typecheck.rs）：P2 路径冲突、P4 未注册语言、P4 block→静态、P4 代码→静态四处 rules 展开路径的诊断新增 `.note()` 修复建议（此前仅显式投射路径有 note，rules 展开路径缺失，信息质量不一致）。

### 变更
- 版本统一：dhv 0.2.29 · dhv-ts 0.2.29 · BNF v1.5.0。

## [0.2.28] — 2026-09-01 · 「for-range 代码生成补齐」版

### 修复（dhv-ts，全部以回归用例锁定）
- **内联 for-range 代码生成**（body.ts `case 'for'`）：解析器 `parseExprNoStruct()` 会将 `a..b` 消费为 `range` 表达式，导致 `e.range` 始终为 `undefined`，`forRangeLines()` 死代码。现检测 `e.iter?.kind === 'range'` 以激活多语言 for-range 生成（Python `range()` / TS `for(;;)` / Go `for ;;` / C++ `for(auto;;)` / Rust `for .. in`）。
- **Python 值语境 range inclusive bug**（body.ts `case 'range'`）：此前 Python 始终生成 `range(lo, (hi) + (1 if True else 0))`（总是 +1），现正确区分 inclusive（`+ 1`）与 exclusive（不加）。
- **半开 range 代码生成**（`forRangeLines` / `case 'range'`）：支持 `..n`（无下界，默认 0）形态；`n..`（无上界）显式报错（Python 无穷 range 不可表示）。

### 新增
- 回归用例 `check/inline_for_range.hsl`（双编译器一致：内联 `for i in a..b` / `for i in a..=b` / `for i in 0..5`）。
- 双编译器一致性：34 → 35 组用例。

### 变更
- 版本统一：dhv 0.2.28 · dhv-ts 0.2.28 · BNF v1.5.0。

## [0.2.27] — 2026-09-01 · 「Guide 后端注册表更新」版

### 变更
- **guide 附录 A 后端注册表更新**：
  - 来源引用增加 dhv langs.rs，BNF 版本更新至 v1.5
  - 新增「dhv 专属后端实现」说明：7 个专属后端（python/typescript/rust/go + yaml/markdown/json），其余 31 语言走通用契约后端
  - Go 条目备注大幅扩展（v0.2.17 升级 / v0.2.20 函数体覆盖 30+ 种表达式 / 各项映射细节）
- **guide 已知限制 #3 修正**：contract 语言数量 25→26（与实际 38-3-3-6=26 一致）
- **guide 版本同步**：头部版本表、参考实现、尾部署名全部更新至 v0.2.27

### 修复
- 无

## [0.2.26] — 2026-09-01 · 「Release CI Windows 路径修复」版

### 修复
- **release.yml Windows 打包路径**：PowerShell 脚本中 `$src` 路径重复了 `toolchain/dhv/` 前缀（`working-directory` 已设为 `toolchain/dhv`，路径应为 `target/...` 而非 `toolchain/dhv/target/...`），导致 `Compress-Archive` 找不到文件（exit 1）
- **v0.2.25 Release 失败回退**：删除 v0.2.25 tag

### 变更
- 版本统一：dhv 0.2.26 · dhv-ts 0.2.26 · BNF v1.5.0。

## [0.2.25] — 2026-09-01 · 「Release CI Windows 打包再修复」版

### 修复
- **release.yml Windows 打包**：`zip` 命令在 windows-latest Git Bash 中不存在（exit 127）→ 改回 PowerShell `Compress-Archive`（`shell: pwsh`）+ `$env:RUNNER_TEMP` 跨平台临时目录
- **release.yml 统一临时路径**：所有平台打包产物统一使用 `$RUNNER_TEMP`（替代 `/tmp/`，Windows 无 `/tmp/`）
- **v0.2.24 Release 失败回退**：删除 v0.2.24 tag

### 变更
- 版本统一：dhv 0.2.25 · dhv-ts 0.2.25 · BNF v1.5.0。

## [0.2.24] — 2026-09-01 · 「Release CI Windows 修复」版

### 修复
- **release.yml Windows 打包**：`Compress-Archive`（PowerShell，路径和 /tmp 不兼容 Windows）→ `zip`（Git Bash 预装，统一 Unix 路径）
- **v0.2.23 Release 失败回退**：Linux/macOS arm64/macOS x86_64 三平台构建成功但 Windows MSVC 打包失败，删除 v0.2.23 tag

### 变更
- 版本统一：dhv 0.2.24 · dhv-ts 0.2.24 · BNF v1.5.0。

## [0.2.23] — 2026-09-01 · 「Release CI 原生 Runners 重写」版

### 修复
- **release.yml 彻底重写为原生 runner 方案**：
  - Windows：`windows-latest` + `x86_64-pc-windows-msvc`（原生 MSVC，无需 zig/gnu）
  - macOS arm64：`macos-latest` 原生构建
  - macOS x86_64：`macos-latest` 上 Rust 原生交叉编译（Rust 对 apple targets 原生支持，无需外部工具链）
  - Linux：`ubuntu-latest` 原生构建
  - Windows 打包改用 PowerShell `Compress-Archive`（预装，无需 7z）
  - 消除所有 zig / cargo-zigbuild / cargo-binstall 依赖
- **v0.2.22 Release 失败回退**：删除 v0.2.22 tag

### 变更
- 版本统一：dhv 0.2.23 · dhv-ts 0.2.23 · BNF v1.5.0。

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
