# CHANGELOG

## v0.2.70（2026-09-22）—— ML 语料 I：感知器 + KNN 数字识别（四路径对拍）+ f64 整值 const 投射修复

毕业论文「HSL 语言作为一等公民实现机器学习算法」语料落地：新增
`fixtures/ml/digit-recognizer.hsl`（+ `fixtures/ml/README.md`）—— 5×5 位图
数字（0/1/8）识别，数据集内嵌（12 训练 + 6 测试，含对抗性 1 像素噪声变体），
**全程无 native 块**，与 org 仓 `fixtures/turing/` 同哲学：

- **KNN**（k=3，欧氏距离，多数投票）：平方距离手写循环；3 近邻表保序插入
  维护（不依赖 sort_by，规避后端排序实现差异）；平票按类序确定性破平；
- **感知器**（is-8 二分类，margin 变体）：`w ∈ R^26`（`[0.0; 26]` 数组重复
  字面量初始化），更新条件 `y·score < 1.0`（经典条件的间隔推广，保证测试
  样本决策余量），lr=0.1、上限 30 轮、收敛即停；输出轮数 / 更新数 /
  `||w||` / 逐样本得分；
- **四路径对拍全绿**：check 0 错 → run 黄金输出 42 行 → emit python 经
  ruff 0.16 全绿 + `python3` 真实运行逐行一致 → emit rust 经 `rustc` 真实
  编译运行逐行一致（`KNN acc=6/6, Perceptron acc=6/6`）。跨后端确定性前提：
  训练/推理全为 IEEE 754 double 加/乘（三运行时逐位一致），仅展示用开方走
  `{:.3}/{:.4}` 定点打印。

**修复（ML 语料对拍驱动）**：`const MARGIN: f64 = 1.0` 这类 **f64 整值
const 字面量**，dhv-ts `exprLitText`（backends/decls.ts）经 `String(v)` 归一
成 `1` → rust 后端产出 `pub const MARGIN: f64 = 1;` 触发 rustc E0308
（mismatched types；emit 语法校验是启发式，拦不住）。dhv（Rust）侧用
`lit.raw` 保留 `1.0`，双端本就分歧 —— dhv-ts 侧对齐：无 `.`/`e` 的浮点
表示补 `.0` 后缀（`LitVal` 不存原文，parser 只保留数值）。

**已知边界（诚实记录，非本批引入）**：dhv（Rust）对 `project{}` 强制
P-2 物理路径唯一（一项一文件），dhv-ts 允许多项投射同一文件 —— org 图灵
语料（rule110 / busy-beaver / bf）与本 ML 语料均采用多项单文件形态
（rust 单文件可 rustc 直编），走 dhv-ts 车道；`dhv check` 报 22 × P-P2
（仅此一类，无 S/G/N 违规）。

**版本卫生**：v0.2.69 只 bump 了 dhv-ts（0.2.69），dhv `Cargo.toml` 停在
0.2.68，CI version-sync 守卫处于必红状态 —— 本批双端对齐 bump 到 0.2.70
（README「当前版本」同步）。

**测试**：`tests/hsl/run-all.ts` 201/201；`tests/run_emit_conformance.ts`
6/6；`bash tests/run_conformance.sh` 108/108（dhv 侧 cargo build 后全过）；
`scripts/ruff-gate.ts --ts-only` 4 语料全绿；ML 语料四路径手工对拍实录见
`fixtures/ml/README.md`。

## v0.2.69（2026-09-19）—— 推理型模型预算适配：观测记忆 + 空补全升档重试（B-24）

实测（org 真实车道，deepseek-flash）：工具环任务触发长思考时
completion_tokens=8191 全部是 reasoning_tokens、content 零字符 ——
流被长度截断，用户看到 `empty completion (finish_reason=stream…)`
硬错误，推理型模型在长任务上不可用。

**修复（`host.ts` llmComplete 网关车道，多重优雅降级四层）**：

- **① 观测**：两网关车道（流式/非流式）把 reasoning 需求回报 Host ——
  失败路径（空补全抛错前）与成功路径（usage.completion_tokens_details
  .reasoning_tokens / reasoning_chars÷3 粗估）都计入；
- **② 记忆**：`llmReasoningFloor`（Host 实例字段）记录本 run 观测峰值，
  `reasoningBoosted` 把后续请求的 maxTokens 下限抬到
  `min(峰值×2+4096, 32768)` —— 同一 run 不再逐请求重踩；
- **③ 重试**：空补全且推理耗尽型（finish_reason=length 或
  reasoning_chars>0）→ `retryWithReasoningBudget` 按观测需求升档重试
  一次（预算已够还空 → 判定非预算问题不重试，诚实抛）；
- **④ 诚实**：升档后仍空 → 原样抛可诊断错误（v0.2.59 的诊断信息保留）。

**兼容面**：SDK 直连 / scripted / 零外联车道零变化；非推理型模型
（reasoning 零消耗）零变化（floor 恒 0，boost 是 no-op）；仅网关车道
介入。dhv（Rust 端）无 LLM 网关，不受影响。

## v0.2.68（2026-09-18）—— 诊断码三处分歧对齐（#18：双端码集合一致 + 豁免清零）

issue #18 实录：conformance 第 7 段「诊断码集合一致性」暴露三处已知豁免 ——
同一错误双端报不同的码（或在不同检查层拦截）。本批逐项对齐并以
**guide 第十章码表为单一事实来源**；三个语料移除 codes-exempt 后
`bash tests/run_conformance.sh` 7 段全过、**⊘ 豁免数 0**（100/100）。

**三处对齐**：

- **E-1（顶层重名）**：dhv 此前用 `NameResolution("E1")` 拼接渲染为
  `M-E1`（模块域前缀），dhv-ts 报 `E-1`。码表（guide 第十章 L/E/R 节）：
  `E-1 | check | 重复定义顶层项` —— **dhv 去域前缀**：`DiagCode` 新增
  `Duplicate` 族渲染 `E-{id}`，M- 前缀拼接不再用于重复定义语义。
- **L-12（\u{...} 码点越域）**：dhv 此前在 pest 文法层拒绝 → 通用
  `E0001`；dhv-ts 在 lexer 层报 `E-0`。对齐方向（issue 指定）：**两端
  lexer 层以 L-12 专用码拦截** —— dhv escape 文法放宽为「{ 内任意内容到
  首个 }」，码点域校验（纯十六进制 + ≤ 0x10FFFF）移入 parser 词法域扫描
  （`scan_unicode_escapes`，遍历 pair 树定位全部 string/char 字面量）；
  dhv-ts `LexError` 增设可选 `code`，`\u` 域违例（非十六进制 / 超上限）
  携带 `L-12` 渲染。判定口径双端逐位对齐：空/非十六进制 → 「必须是
  十六进制」；纯十六进制但越域（含超 u32 容量）→ 「超出上限」。
  v0.2.56 L-12 教训语义不变（非法转义仍是硬错误，无静默值损坏），
  只是拦截码从通用升级为专用。
- **P-5（未知规则类型）**：dhv parser 文法层 `item_kind` 封闭枚举拒绝
  裸规则名（`widget -> ...`）→ `E0001`；dhv-ts parser 收下、checker 语义层
  报 `P-5`。对齐方向（issue 推荐 + BNF §3.4 R4「未知类型 → P5」）：
  **都接受 → 语义层 P-5** —— dhv `rules_item` 文法放宽为
  `rule_kind = { item_kind | identifier }`，未知类型由 typecheck 既有的
  KNOWN 表拦截（P-5），保持语义检查层价值。BNF §3.4 同步：`RulesItem
  ::= RuleKind "->" ...` + `RuleKind ::= ItemKind | Identifier`。

**文档联动**：BNF §1.5 Escape 修正陈旧的 `\u{...}` 产生式（旧文仍容忍
下划线，与 v0.2.56 L-12 双端拒绝实况相悖）；§3.4 RuleKind 成文；
guide 第十章码表补 P-5 与 L-12 两行（E-1 原本即码表口径，dhv 侧归位）；
`guide/BNF-v1.5.0.md` 镜像逐字节同步（spec-sync 守卫口径）。

**已知残差（诚实边界，均无语料覆盖、结论级仍一致）**：`\u` 缺 `{`
（dhv `E0001` vs dhv-ts `E-0`）；rules 规则类型为非 item_kind 关键字
（如 `let`，dhv `E0001` vs dhv-ts `P-5`，BNF 文法按 `ItemKind |
Identifier` 收紧故 dhv 行为即规范行为）。

**测试**：conformance 100/100（7 段全过、⊘=0，三个语料豁免解除）；
`cargo test` 15/15；`run-all` 194/194（含 ruff 门禁双车道）；IDE 校验
全过；值级 unicode 边界语料（`\u{41}bc` / `\u{10FFFF}` 等）双端值不变。

## v0.2.67（2026-09-18）—— S-19 方法面收紧（#13 check/run 对齐）+ Vec 迭代器协议三件套

issue #13 实录：`s.as_bytes()` / `s.substring(a, b)` 双端 check 0 error 通过、
`dhv-ts run` 运行期才报「String 没有方法」。根因是双端 checker 对未知方法放行
（宽松策略），而 interp 的方法表不含这些名字。本批按 issue 建议的方向 1 落实
「check 过 = run 不炸」的工具链承诺（与 B-6 修复哲学一致），双端同步。

**S-19 由 warning 升级为 error（dhv-ts checker.ts + dhv typecheck.rs 双端）**：

- **接收者类型可知面扩展**（v0.2.58 B-7 时代仅「单段路径 + let 注解」）：
  let 注解（原口径）/ **无注解绑定的初始化器推断**（`let s = "hello"` →
  String，字面量/构造器 Some·Ok·String::from·vec!·format!·方法链）/
  **fn · graph · 闭包参数注解**（`fn f(s: String)` 是接收者最大聚集面）/
  **字面量接收者**（`"abc".as_bytes()`）/**方法链返回类型追踪**
  （`chars()→Vec → take→Vec → cloned→error`，#13 附带发现的迭代器适配器链）/
  切片·下标·cast·str+str 拼接。静态不可判（native/foreign 值、动态函数
  返回值等）不判 —— 零假阳性优先。
- **char 回退面精确化**：运行期 `builtinMethodFor` 对单字符 String 回退
  CHAR_METHODS（UTF-16 长度 ≤1）；check 对**字面量接收者**按同款长度判定
  （`"ab".is_alphabetic()` check 即拦，`"a".is_alphabetic()` 合法），绑定
  接收者保守取并集。
- **用户 impl 豁免与运行期派发同源**：Option/Result 枚举值运行期经 impls
  注册表派发用户方法（实测 `impl Option { fn doubled }` run 可用）→ check
  豁免；String/Vec/HashMap 是原始值（无 __struct/__enum 标记），impl 永不
  派发（实测 `impl String` 的方法 run 仍报错）→ 不豁免。impl 方法名跨模块
  预收集（根文件 + 依赖闭包），与 registerItem 的注册时序对齐。
- **错误信息与运行期同源**：报文直接引用运行期措辞「run 将报『String 没有
  方法 "as_bytes"』」，错误码沿用 S-19（不引入新码，#18 诊断码体系兼容）。
- **重赋值语义**：注解来源的 stdTy 是契约（重赋值不清洗）；推断来源随 RHS
  更新（可判 → 新类型，不可判 → 清除），防「先 Vec 后 String」陈旧事实。

**运行期方法面补齐三件（Vec，语料在用 / 运行期缺席的同类断层）**：
`into_iter`（与 iter 同为恒等）/ `to_vec`（= clone，Rust slice→owned 对应物）/
`next`（头部取元素并原地推进，pop 的镜像 —— Rust Iterator::next）。S-19 收紧
后由 check 语料（s6_exhaustive_enum_in_loop 等 + range_expression 的切片
to_vec）复现暴露 —— 此前这些语料 check 全绿、真 run 必崩。

**保守边界（诚实清单）**：match/if-let/for 模式绑定不追类型（解构与迭代元素
类型待完整类型推导）；数值方法面（NUM_METHODS）未纳入 S-19（本批聚焦
String/Vec/HashMap/Option/Result 五类，#13 主诉面）；注解违约重赋值
（`let mut s: String = "a"; s = vec![]`）本身仍是静态检查盲区（无完整类型
推导），但其上的方法调用按注解契约判。

验证：run-all 194→**201 全绿**（+7 个 v0.2.67 回归：error 升级/推断/链式/
参数注解/impl 豁免+run 全通/重赋值/RET-方法面自洽守卫）；conformance 100→
**105 全过**（+errors/S19_* 三件码集合双端一致 + check/S19_* 两件零误报双端
绿）；cargo test 15/15；nova（2000+ 行）/dsh/backends-demo/五巡览语料全量
check 零新告警。

## v0.2.66（2026-09-15）—— python 产物 ruff 四修 + rust 后端保真（图灵语料对拍驱动）

> 发布卫生补齐（2026-09-18）：v0.2.66 原提交（7ce294a）只更新了 dhv-ts 侧
> （package.json 0.2.66 + 生成器修复），dhv Cargo.toml / README 徽章 / 本
> CHANGELOG 条目漏更 —— version-sync CI 因此必红。本批补齐发布物料，版本
> 语义不变。

**图灵语料三程序**（org 仓 fixtures/turing/）：Rule 110 元胞自动机（Cook
2004 TC 证明）· 3 态忙海狸 BB(3)（13 转移/14 构型/Σ=6 文献级不变量）·
Brainfuck 解释器（906 指令 Hello World）—— 三程序 × 四语言对拍
（dhv-ts 解释器 + python 产物 ruff + rustc + g++）全部输出一致。

**python 产物 ruff 四修**（dhv-ts 车道，PIE808/SIM114/F541/UP034）：

- **PIE808**：`range(0, N)` → `range(N)`（range 首参 0 恒可省）；
- **SIM114**：match 等值臂合并（同体 `|` 模式臂不再重复生成体）；
- **F541**：无占位符 f-string 不发 `f` 前缀（纯字面量直出）；
- **UP034**：f-string 实参外层括号剥离。

**rust 后端两修**（dhv-ts emit 车道，rustc 实编译驱动）：

- **`as` 转换保真**：此前 cast 一律丢弃 → rustc 拒绝 i32 索引（usize 期望）；
  现按目标类型真实投射 `as usize` / `as i64` …（BB(3) 语料驱动）；
- **`char_at` 映射**：`s.char_at(i)` → `s.chars().nth(i)` 越界 None → 空
  串（与 interp 越界返 '' 对齐；Brainfuck 语料驱动）。

## v0.2.65（2026-09-13）—— dhv(Rust) python 生成器 ruff 全绿（按工具链补齐）

v0.2.64 把 dhv-ts 的 python 产物带到 ruff 全绿；本批按工具链推进到
**dhv(Rust) 编译器**：本地装 Rust 工具链实测基线（四语料 50 项失败）
→ 逐根因修复 → 全绿 → 双车道门禁进 CI。

**基线实测（修复前，四语料 29 个 .py · 50 项失败）**：
22 项语法错误（E999）+ 12 项 F821 + 6 项 I001 + 2 项 UP034 + 2 项 PLC3002。

**根因修复（python.rs · 生成器层）**：

- **format! 宏拼接损坏**（×6 文件）：字面量带引号原样入串 + 整体再包
  一层引号 + 占位符与实参错位 → `""tpl""{}{}"`.format(...) 非法语法。
  重写为 f-string 投射（模板 `{{}}` 转义 / `{}` 消耗实参；实参 token
  级翻译：`x.len()` → `len(x)` · `String::from(x)` → `str(x)`；无占位
  回落普通字面量防 F541）；首逗号不再产生空组（空占位 `{}` 是语法错）；
- **while-let 模式当赋值目标**（×4）：`Some(head) = q.pop()` 非法赋值
  + 条件与绑定各自重求值（副作用双 popping）。改为合成 `while True` +
  循环体内单次求值 + 取反条件 break；模式语义统一走
  `py_match_condition`（与 match/if-let 同源）；
- **`String.from` 关键字方法名**：`from` 是 python 关键字 → `str(x)`
  （字面量直出）；
- **元组下标字段**：`kv.1` 非法属性名 → 下标访问 `kv[1]`；
- **impl 顶层缩进 def**：impl 是独立投射项不在 class 体内 → 顶层 def
  （self 显式首参）；
- **空方法名 `.()`**：py_std_method 把整式塞 receiver 的形态（first/
  last/abs/collect…）此前仍拼 `.()`——直接返回整式；
- **async lambda**：python 无此构造 → 诚实降级注释（此前 E999）；
- **块表达式 IIFE**：纯尾块 `(lambda: x)()` ≡ x —— 直接内联（PLC3002
  消除）；多语句块表达式位诚实降级；
- **Some/None/Option 语义**：`Some(x)` → x · `None` 值不再被 py_ident
  转义成 `None_` · `HashMap::new()` → `{}` 等内置容器构造直译。

**根因修复（跨文件引用层 —— finalize_crossrefs 后置收尾，与 dhv-ts
finalizePython 同思路）**：

- **F821 未定义名**（×12）：一项一文件的裸跨引用 → 全产物注册表
  （顶层名 → 模块 stem）→ 代码视图扫描（剥注释/字符串；**f-string 占位
  表达式保留** —— 引用常藏在里面）→ 注入 `from <module> import <names>`；
- **属性访问不算裸名使用**：`WorkStatus.Running(3)` 不再误导入 Running
  （导入未用 F401）；
- **Result 变体桩类**：`isinstance(x, Ok)` → 注入 `class Ok`（`_fields/
  __getitem__`，与枚举 tuple 变体投射同构）；
- **isort 分区**：stdlib 与本地模块分区（provider.py 实测：typing +
  prompt 混排单区被 I001 拒）+ 导入区后两空行。

**根因修复（语句层）**：

- **println! 语句被吞**：`pass # 局部项` 吞掉语句 → 变量使用点消失
  （F841）+ 块内多余 pass（PIE790）。println! → `print(f-string)` 真实
  投射（main.py 语义恢复：真的打印了）；其它语句级宏诚实注释降级；
- **常量名 snake_case 打碎**：`DEFAULT_PRIORITY` → `d_e_f_a_u_l_t…`
  引用侧不匹配（F821）→ 常量名保持原样（全大写惯例）；
- **混合枚举漏 dataclass 导入**：Named 变体的 `@dataclass` 需要导入
  （此前仅纯 struct 路径导入）；
- **FURB136 夹逼形态**：`b if a > b else a` → `min(a, b)`（gt）/`max`
  （lt）直译；
- **绑定多语句**：`; ` 单行拼接 → 多行（E702 预防）。

**门禁基础设施（双车道）**：

- `scripts/ruff-gate.ts` 按工具链分车道：ts（默认）+ rust（需
  cargo build 产出二进制；`--require-rust` 强制在环）—— 8 份语料·车道
  全绿（修复前 ts 0 / rust 50）；
- CI 新 job `rust-ruff-gate`（Rust 工具链 + cargo build + 双语料门禁，
  `--rust-only --require-rust`）；既有 `ruff-gate` job 显式 `--ts-only`。

**诚实边界**：dhv(Rust) python 产物的运行期 parity（prelude 助手 /
`_dhv_str` 显示语义）不在本批范围 —— 静态投射门禁目标（ruff 全绿）
已达成，活体运行请用 dhv-ts（nativeRuntime 路径）。

验证：dhv(Rust) cargo test 15/15 · 四语料 ruff 全绿（50 → 0）· dhv-ts
194/194 · 双车道门禁 8/8 · CI 9 job（含新 rust-ruff-gate）。

## v0.2.64（2026-09-13）—— python 产物 ruff 全绿 + 门禁基础设施（生成器九修）

「所有产物 ruff 检测均可通过」从口号变为可执行门禁。ORG 侧（v0.5.4）
驱动实测暴露的 python 生成器卫生问题全部回流上游，并以 CI job +
套件用例 + 四语料三层锁定。

**生成器修复（dhv-ts python 车道，九处）**：

- **按需导入**（finalizePython）：头部 `dataclasses/math/typing` 族按
  正文实际用量注入，不再全量倾倒（F401 清零）；
- **导入卫生**：`from X import A, B` 跨文件收编去重、`__future__` 归位；
- **变体桩类**：`Ok/Err/Some` 在被 `isinstance` 匹配的文件内注入纯
  class 桩（`class Ok:`），消除 F821（未定义名）；
- **空行约定**：顶层 def 间恰两空行（E303 清零）；
- **复合赋值算符翻倍**：AST 的 op 已是完整算符（`'+='`），模板再追加
  `'='` 生成 `i +== 1` 非法语法（python `py_compile` 实测抓到；
  emit 一致性语料此前未覆盖复合赋值的活体翻译）；
- **语句位括号剥离**（UP034）：二元/一元表达式全括号化发射在
  return / 调用实参位产生冗余包裹；元组字面量顶层逗号探测保护
  （括号是语义，绝不剥）；
- **镜像注释剥离**（F401 误判）：`@dhv:hsl-mirror` 镜像注释里的名字
  不构成导入用量 —— contract 回退文件引用全在镜像注释里，导入被
  误判「已用」（main.py 实测）；
- **emit ENOENT 兜底**：零投射文件的源 emit 到不存在目录时
  `manifest.json` 先写即崩 —— 统一 mkdir 兜底（幂等）；
- **类型映射补全**：python 车道 int 族（i8/i16/i128/u8/u16/u128）
  此前缺映射，现与 i32/i64 同归 `int`。

**LLM 零外联开关**：`DHV_LLM_DISABLE_SDK=1` 显式禁用 SDK 直连车道
（CI / 离线环境机械保证「测试零外联」；此前只能靠「环境恰好没装
SDK」的偶然前提，装了 SDK 的机器上负例测试反而假阴）。

**门禁基础设施（三层锁定）**：

- `scripts/ruff-gate.ts`：四语料 emit → `ruff check`（0.16 默认全规则
  集），任何失败 exit 1 逐条列出；无 ruff 环境诚实报错（不静默跳过）；
- 语料四份：`kernel-tour.hsl`（全特性，移植自 ORG v0.5.4 基线）+
  `pattern-tour.hsl`（模式族）+ `backends-demo/agent.hsl`（真实
  harness 规格）+ `dsh.hsl` —— 共 29 个 .py 产物；
- CI 新 job `ruff-gate`（uv 装 ruff + 门禁 + 带真 ruff 跑全量套件，
  让套件里「ruff 缺席跳过」的断言在 CI 真实执行）；run-all.ts 新增
  v0.2.64 段 8 用例（复合赋值/按需导入/镜像剥离/桩类/ENOENT/
  零外联开关的结构性回归锁定）。

**本批合并入主线**：v0.2.62（实测驱动六修）、v0.2.63（S-4 双端一致）
随本版本一并进入 main（此前只在 `fix/practical-fixes-0.2.62` 分支 /
PR #19）。

验证：dhv-ts **194/194**（186 + 8 新门禁用例）· ruff 四语料 29 个
.py 全绿（修复前 134 项失败）· 版本联动 dhv(Rust) 0.2.64 = dhv-ts
0.2.64（CI Version sync 守卫）。

## v0.2.63（2026-09-12）—— S-4 参数可变性双端一致（dhv-ts 漏报修复）

外部实测发现的双端语义分歧（worklog 遗留观察 → 最小复现实锤）：

- **现象**：`graph` / `fn` 的**非 mut 参数**在体内被赋值时，dhv-ts 放行
  （exit 0），dhv(Rust) 正确报 `S-S4`（exit 1）—— 双端 exit code 分歧。
- **根因**（checker.ts `declareParam`）：参数进作用域表时恒 `mut: true`，
  无视声明处 `mut`（`graph R(mut task: Task, question: String)` 中
  `question` 被当成可变绑定）。S-4 检查依赖 `hit.mut` → 漏报。
- **修复**：参数默认不可变、显式 `mut` 才可变（与 dhv(Rust) parser 及
  BNF 语义一致）。`declareParam(scope, name, mut)` 按声明传入；graph 参数
  取 `GraphParam.mut`；fn/impl/trait 参数经新增 `fnParamBindings()`
  （self 参数按 kind 映射：mutvalue/refmut 可变，value/ref 不可变）。
- **连锁修正**：v0.2.62 的两个 G-8 回归 fixture 此前依赖此漏报才通过
  （非 mut 参数 `x` 在 body 内 `x = x - 1`；dhv(Rust) 同源码本就报 S-S4），
  参数改 `mut x` 并注明缘由。
- **conformance 语料盲区**：对拍只比「通过/失败」结论，本分歧两端都
  fail 时结论相同不报；且语料无「graph/fn 参数可变性」维度用例 ——
  exit code 分歧的用例（补 loop 后 ts 全过）缺失是未暴露的直接原因。
  后续建议对拍粒度升级为诊断码集合。

验证：dhv-ts 186/186（新增 4 回归：graph/fn × 非 mut 拦截/mut 放行）·
conformance 67/67 · nova 15 模块 / backends-demo / dsh 剧本端到端全过。


## v0.2.62（2026-09-12）—— 实测驱动修复批次（六处 · 每修一 bug 锁一用例）

外部实测（沙盒全链路装机：bun 跑 dhv-ts + cargo 构建 dhv + 双编译器 conformance）
驱动的一致性/健壮性修复。全部用例先复现后修复，回归锁定在
`tests/hsl/run-all.ts` v0.2.62 批次与 `dhv/tests/fixtures/parse/`。

- **G-8 expr 守卫指纹**（checker.ts）：Span 是 `{line,col,file}` 对象，
  此前 `span[0]/span[1]` 索引恒 `undefined` → 所有 expr 守卫的指纹都是
  `expr@undefined:undefined` —— 同端点两条**不同的**合法守卫（`x > 1` /
  `x < 0`，Vigil 惯用法同向多守卫）被 G-8 误杀。改为**表达式结构指纹**
  `exprFingerprint`（位置无关递归序列化）：同结构 ≡ 复制粘贴（G-8 仍拦截），
  不同条件 ≡ 合法并行边（不误报）——两个目标同时满足。
- **dhv（Rust）edge guard expr 形态解析**（hsl.pest）：`edge_guard =
  { pattern | expression }` 的 PEG 有序选择里，pattern 对 `x > 1` 只消费
  绑定模式 `x` 即成功提交（PEG 不回溯），残留 `> 1` 撞 `edge_attrs? ~ ";"` →
  E0001 —— `>` `<` `==` `!=` `&&` 等全部中招，而 dhv-ts 同源码正常
  （conformance 只对拍结论，语料缺 expr 守卫形态故漏网）。修复：pattern
  分支加负向前瞻 `guard_continues`（完整 pattern 后紧跟表达式续接运算符
  → 改走 expression 分支）。回归：`parse/edge_expr_guard_operators.hsl`。
- **下标切片语法 `v[i..j]`**（parser.ts）：值语境 range（v1.5 §2.11.7）
  在下标语境抢先吸收 `1..3`，postfix 的 slice 分支成死代码 → 解析为
  `v[range(1,3)]` → interp 把 range 对象 `Number()` 成 NaN，NaN 绕过越界
  检查静默返回 `undefined`（下游 `.len()` 报误导性「unit 没有方法 len」）。
  与 dhv（pest `index_or_range = { range_full | expression }` 先试 slice）
  语义分歧。修复：下标语境专用 `parseExprNoRange()`，`[i]` / `[i..j]` /
  `[i..]` / `[i..=j]` / `[..j]` 全形态可用。
- **复合赋值算术对齐**（interp.ts）：`evalCompound` 的 `%` 缺除零检查
  （`x %= 0` 静默 NaN —— 垃圾值污染数据流）与 bigint 分支；`*` `/` 缺
  bigint 分支（`x *= 大整数` 误报「int 与 float」）。全部对齐 `evalBinary`
  的 L-9 口径（运行期干净 HRuntimeError）。
- **fn/graph body 内 block 资源块**（parser.ts）：`ITEM_KWS` 缺 `'block'`
  （有 `'static'`）→ `atItemStart` 在函数体内不认识 block，而
  `atIdent('block')` 分支是死代码（block 是 kw token）—— 顶层合法、
  函数体内非法的不对称。补齐后与 static 同权。
- **`\x` 转义十六进制校验**（lexer.ts）：`'\xZi'` 的 `parseInt('Zi',16)=NaN`
  → `String.fromCharCode(NaN)` = NUL 字符**静默入值**（字符串"看起来是空的"
  却 len=1，极难排查）。对齐 `\u` 的 L-12 严格口径（2 位十六进制）。

验证：dhv-ts 182/182（新增 6 回归）· cargo test 全绿（新增 parse fixture）·
双编译器 conformance 67/67（expr 守卫语料修复后新增 1 项通过）。


## v0.2.61（2026-09-11）—— LLM 网关流式车道（逐 token 观测面）

`$host.llm.complete` 补流式输出（`stream: true`，仅网关车道）：SSE 逐块解析
（OpenAI 兼容 `stream` 协议），reasoning_content 与 content **双通道分别归因**
（推理型模型思考/正文分离，DeepSeek 实测），每块立即 append-only 落盘
`<outdir>/llm-stream.jsonl`（`{ts, track, kind, delta}`，行级原子）—— 观测面
（ORG 引擎泵 / chat REPL / Web SSE / TUI）以行级尾随即可**逐 token 渲染**。
返回值仍为完整正文：HSL 语义零变化（观测增强，非语义变更）。

- **流式车道**：POST `stream:true` → ReadableStream 逐块；SSE 帧解析（data:
  行负载 / [DONE] 哨兵 / 冒号注释行跳过 / 单块解析失败不炸整条流）；
- **reset 标记**：每次流式调用开始落一行 `kind:"reset"`（空 delta 特例放行）
  —— 调用侧有界重试会完整重发，观测面收 reset 即清已渲染正文重新开始；
- **llm_stream_done 事件**：尾包带 usage 时发（chars / reasoning_chars /
  elapsed_ms 可观测）；
- **空正文可诊断**：推理吃满预算（finish_reason=length 类场景）抛错带
  reasoning_chars —— 与非流式车道 v0.2.59 的诊断面同构；
- **超时/鉴权/模型路由/思考量**：与非流式车道同构（AbortController + finally
  清理；DHV_LLM_API_KEY / DHV_LLM_MODEL / DHV_LLM_THINKING）；
- **测试补位**：v0.2.58 网关上游化时测试未随行 —— 本版补齐（鉴权头贯通 /
  model 路由 / thinking 映射）+ 流式四例（双通道落盘 / reset / done 事件 /
  空正文诊断面）；172 → **176 全绿**；
- ORG 侧消费：`org chat`（交互式 REPL）与 Web GUI SSE（delta 事件）逐 token
  渲染 —— vendored 同步 v0.4.15。

## v0.2.60（2026-09-11）—— S-20 字面量字段闸门 + 宿主面三修（可观测/安全）

实测发现类型安全闸门缺位：`struct Point { x: i64, y: i64 }` 后构造
`Point { x: 1, y: 2, z: 999 }`（不存在字段 z）—— check 全绿、run 静默收下
（未知字段被写进值对象）；缺字段（`Point { x: 1 }`）check 也全绿（运行期才
兜底报错）。对「把 Agent harness 描述成可靠的工程」的核心主张而言，结构体
字段面是纯静态可判定的事实，不应留到运行期。本版在 dhv-ts check 阶段补齐
（与运行期 `evalStructExpr` 同口径）：

- **S-20 结构体字面量**：未知字段（「结构体 P 没有字段 "z"（已知字段：…）」）·
  缺字段（「结构体字面量 P 缺少字段 "y"」）· 字面量内字段重复 —— 均为
  error 级；
- **S-20 枚举变体命名字面量**：`Shape::Rect { w, h, depth }` 同权校验
  （未知/缺失；元组变体走调用语法不涉及）；import 别名经 L-2 映射归一；
- **`..base` 功能更新不误报**：base 动态携带字段可补齐缺项 → 有 base 时
  跳过缺字段检查（未知字段检查照常 —— 多余名字与 base 无关，仍是类型错）；
- **未登记名不误报**：宏生成/import 别名（struct 无别名归一）等静态解析
  不到的名字静默跳过，留给运行期自然报错 —— 结构级检查不制造假阳性；
- 已知边界（诚实声明）：字段值的**类型**推导（`Point { x: "hello" }`）仍
  归 dhv Rust 编译器管 —— dhv-ts 检查器是结构级铁律（`let x: i64 = "hello"`
  同样不拦，与本修复无关的系统性设计边界）。

宿主面三修（同一纪律：可观测不静默 / 边界即保证）：

- **`nextReview` 耗尽抛错**：此前静默返回 `{"verdict":"accept"}` —— 审查
  判定是验收闸门的证据源，缺省伪造 accept = 闸门静默失效（与 nextAct /
  fixture.next(track) 同口径；需要恒 accept 请显式写足 reviews 条目）；
- **`fs.list` 深度可配**：原两层硬编码（第 3 层起目录仅出条目不展开，深层
  文件对 harness 静默不可见且无法配置）→ 默认 8 层，`$host.fs.list(dir,
  depth)` 可调（上限 32，钳制发 `fs_list_depth_clamped` 事件可观测；超
  20000 条截断带标记）；
- **路径监狱 symlink 实解析**：`path.resolve` 只是词法归一 —— 实测 workspace
  内 `etclink -> /etc` 后 `fs.read("etclink/hostname")` 直达外部内容、
  `fs.write` 可在监狱外落盘（双向量穿越）。修复：已存在路径经
  `realpathSync` 归一后必须仍在（realpath 后的）工作区内；写入不存在的
  路径解析最近存在祖先（父目录可能恰是指向外部的符号链，walk-up 到第一
  个存在节点再实解析；不存在的新名字不可能是符号链）。合法操作（监狱内
  读写、新目录创建）经全量回归零误伤。
- 回归：tests/hsl 163 → **172 用例全绿**（+9：S-20 六例 + nextReview 耗尽
  / fs.list 深度 / 路径监狱 symlink 三例）；dhv Rust 侧无改动（Cargo 版本
  联动例行 bump）。

## v0.2.59（2026-09-10）—— 网关直连服务商：鉴权 + 模型路由 + 超时保护

v0.2.58 的 `DHV_LLM_GATEWAY` 假设「网关自持鉴权/限流」的部署形态；实测直连
DeepSeek 官方 API（`https://api.deepseek.com/v1`，模型 `deepseek-flash`）发现
三个缺口：无 Authorization 头（401）、无 model 字段（网关侧无法路由）、无超时
（挂死 fetch 无限等待）。本版补齐，OpenAI 兼容服务商（DeepSeek / OpenRouter /
vLLM / Ollama …）即插即用：

- **DHV_LLM_API_KEY**：设置后请求携带 `Authorization: Bearer <key>`；缺省不
  发（内网自持鉴权网关行为不变）。
- **DHV_LLM_MODEL**：设置后写入请求体 `model` 字段；缺省不写（网关侧默认
  模型路由行为不变）。
- **DHV_LLM_TIMEOUT_MS**：fetch 超时保护，默认 180000；显式设 0 关闭。此前
  网关无响应时整个 agent run 挂死（无超时的 fetch 在 Bun 默认无限等待）。
- **DHV_LLM_THINKING**：思考量控制 —— `off`/`disabled` → `thinking:{type:
  "disabled"}`；`low`/`medium`/`high` → `reasoning_effort`。实测 DeepSeek
  两者均接受（off 时 reasoning_len=0）。缺省不发送（服务商默认，兼容严格
  校验的网关）。
- **逐调用思考量覆盖（v0.2.59）**：`$host.llm.complete` 请求可带
  `thinking` 字段（同上取值），缺省回落 `DHV_LLM_THINKING` 环境变量 ——
  调用侧退避升级用（如 ORG model.hsl 在 finish_reason=length 空返回后
  关思考重试），非工具链层隐式重试。
- **空 content 可诊断化**：推理型模型 reasoning 吃满 max_tokens 时
  content=""、finish_reason=length —— 此前表现为无信息的 "empty
  completion"（ORG v0.4.13 E2E 实测三连空炸穿 run 的根因）。现空 content
  抛错带 `finish_reason` 与 `usage`，调用侧重试/换参有据可依。
- **超时实现用 AbortController + finally clearTimeout**（不用
  AbortSignal.timeout）：实测 Bun 1.3 的 fetch 完成路径与 timeout 信号
  交互可丢延续 —— 响应已达但 await 不恢复，进程 park 在 sigsuspend 空
  事件循环（HSL 侧反复挂死实测；解释器长链路多调用下间歇复现）；且
  AbortSignal.timeout 每调用泄漏一个未取消的 180s timer。显式 controller
  + finally 清理两头都干净。

验证：ORG 侧新增 `tests/gateway.test.ts` 六例（鉴权头/model 字段贯通 + 缺省
行为不变 + 超时中止 + 4xx 错误体传播 + 空 content 诊断 + 思考量控制），随
ORG v0.4.13 联动；DeepSeek 官方 API 真实 E2E（org ask 直连 3.4s 结构化回答 +
团队模式工厂全链路）。

## v0.2.58（2026-09-10）—— 实测双 bug 修复 + ORG vendored 增量上游化（终止双向漂移）

外部实测（ORG 旗舰应用真实使用）驱动的修复批次。同时把 ORG 仓库 vendored 副本上的
三项私有改动上游化 —— 此前 org 的 B-6/B-7/网关修复只存在于 vendored 副本，上游
0.2.57 与 vendored 0.2.58 双向漂移（上游的 CRLF/Windows 修复也未回流 vendored），
本版合并为单一事实源：

- **位运算 BigInt 语义（实测复现）**：`1i64 << 40` 此前静默返回 256（JS 把移位量
  掩码到 5 位：40&31=8 → 1<<8），`3i64 << 33` 返回 6；bigint 操作数的 `& | ^`
  也被 `Number()` 压回双精度（>2^53 丢失精度、>2^31 被 ToInt32 截断）。修复口径：
  任一操作数为 bigint 或经 `exprWideInt` 静态探测（i64/u64/i128/u128/isize/usize
  后缀字面量 / 宽整型注解绑定 / as 宽整型 cast —— 与 `exprFloaty` 同构，安全整数
  域的宽整型字面量在运行期是 number）→ BigInt 语义（任意精度、无掩码）；双 number
  路径 `& | ^` 保持 ToInt32（对 i32 及更窄类型与真实语义一致），但移位量越界
  （<0 或 ≥32）抛 `HRuntimeError` —— Rust debug 语义为 panic，静默掩码是最差
  结果；宽整型移位 ≥128 同样报错。**跨后端语义漂移的典型新案例**（正是 S-15 系列
  要防的形态）。回归用例：值语义（111111 六断言）+ 双越界错误路径，共 2 例。
- **String::find 码点索引统一（实测复现）**：`find` 此前返回 UTF-16 码元索引
  （`indexOf`），而 `len/char_at/take/chars` 全部是码点口径 —— `"é😊x".find("x")`
  得 3、`char_at(2)` 却是 `"x"`，两套索引空间静默组合必错位
  （`s.char_at(s.find(x)?)` 取错字符）。统一为码点（与既有 `len/char_at` 行为
  一致，破坏面最小的对齐方向）。回归用例：含 astral 字符的 find/char_at/len
  组合断言（11111 五断言）。
- **B-6 方法面上游化（自 ORG vendored 0.2.58）**：`split_once` / `rsplit_once`
  （「k: v」行拆键值的 Rust 最常用写法，此前只能 find+take+native slice 手工绕）
  与 String 就地变形五件套（`clear/truncate/retain/insert/remove`）等 26 个内建
  方法 —— check 全过 / run 全崩的静默断层（ORG 的 BUGFIXES B-6）。上游化补齐
  回归用例（split_once/rsplit_once/truncate）。
- **B-7 S-19 静态预警上游化**：内建值类型（String/Vec/HashMap/Option/Result）
  上调用不存在的方法名 → check 阶段 warning（与运行期方法面同表的静态判别），
  把「check 过 / run 崩」的方法名断层提前到写码时刻（ORG 的 BUGFIXES B-7）。
- **DHV_LLM_GATEWAY 网关路由上游化**：`$host.llm.complete` 检测到该环境变量
  （指向 OpenAI 兼容端点，`<base>/v1` 形态）时走 HTTP，独立部署无需本机安装
  z-ai-web-dev-sdk；缺省直连 SDK（行为不变）—— 此前仅 ORG vendored 副本支持。

验证：run-all 159→163 全绿（Linux 实测）；Windows/macOS 由 dhv-ts-matrix job
守卫；Rust 侧 dhv 无改动（版本号联动 0.2.58 仅为版本同步纪律）。

## v0.2.57（2026-09-08）—— 全链路 IDE（vsix 捆绑工具链）+ 三平台 CI + Windows 兼容修复

三项面向「下载即用」的交付升级（Windows 矩阵首跑抓到 2 个真实兼容 bug，随批修复）：

- **HSL IDE v0.2.0（全链路）**：VS Code 扩展从语法高亮骨架升级为完整 IDE 闭环——
  `HSL: Type Check`（错误行 file:line:col 解析进问题面板诊断）、`HSL: Run`
  （模型/任务/剧本交互收集 + 流式输出 + verdict 摘要 + report.md 一键打开）、
  `HSL: Emit`（38 后端 Tier 分组 QuickPick + 产物揭示 + manifest 打开）、保存自动
  check（防抖静默）。纯解析层抽为 `ide/parsers.js`（无 vscode 依赖，validate.js
  新增 4 组单测：括号消息 / Windows 盘符 / Tier 分组 / 产物容错）。**vsix 捆绑
  dhv-ts**（`scripts/package-ide.ts`：复制 toolchain/dhv-ts 进包 → vsce → 清理；
  223KB/45 文件）——装扩展即得完整工具链，唯一前置是 bun。工具链五级解析
  （配置 → DHV_TS → vsix 捆绑 → 工作区 → 兄弟仓库）。
- **三平台 CI 矩阵**：ci.yml 新增 `dhv-ts-matrix` job（ubuntu/windows/macos 全量
  158 用例；此前 4 job 全在 ubuntu，Windows 路径语义 / 校验器可用性回归零拦截）+
  `ide` job（校验 + vsix 打包门禁）；移除 `paths-ignore: ide/**`（ide 有真实 CI
  价值）。release.yml 新增 `ide-vsix` job → vsix 随四平台 dhv 二进制一同附到
  GitHub Release（sha256sums 同批）。
- **python3→python 跨平台回退 + PYTHONUTF8 注入**（emit 校验 + native python 双通道）：
  validate.ts 与 native.ts 均硬编码 `python3`——Windows 宿主常态只有 `python`，
  emit 的 python 语法校验与 `native python` 逃生舱在 Windows 直接失败。新增
  `execPy` 助手（ENOENT 回退 `python` + 注入 `PYTHONUTF8=1`——Windows 默认
  cp1252 代码页读 UTF-8 生成物 `UnicodeDecodeError`，三平台 CI 实测抓到）。
  run-all.ts 全部 20 处 python 子进程同步注入。
- **fsEdit CRLF 容忍**（host.ts，Windows autocrlf 实测）：git autocrlf 使 Windows
  工作区文件为 `\r\n`，fixture/剧本的 `old_text` 是 LF → `$host.fs.edit` 精确匹配
  静默失败（run 仍绿灯，仅 `failures:1` 留痕——正是三平台 CI windows-latest job
  抓到的形态）。现精确匹配失败时按 LF 归一化重试，写回保持原文件主导行尾风格
  （CRLF 文件不被迫整体转 LF）；新增 CRLF 回归用例（159 用例，全平台复现锁定）。

  验证：run-all 158→159 全绿（Linux 实测）；Windows/macOS 由 dhv-ts-matrix job
  守卫。

## v0.2.12（2026-09-06）—— 嵌入执行面（宿主进程内复用）

ORG（旗舰应用）单二进制分发驱动的三项稳健性升级，CLI 行为零变化：

- `main.ts`：顶层执行重构为 `export async function cliMain(argv): Promise<number>` +
  `import.meta.main` 守卫。重复调用状态隔离由 `loadProgram` 的 fresh Map 保证。
- `host.ts`：新增 `$host.dhv.check(file)` / `$host.dhv.run(args)`——进程内嵌套执行面
  （懒加载 cliMain 规避 main↔host 静态循环；stdout/stderr 捕获后恢复）。无 bun 环境
  （ORG 单二进制）下 HSL 工厂闸门据此自动降级进程内车道。
- `version.ts`：嵌入执行下 `import.meta.dir` 指向打包器虚拟 FS，读不到 package.json ——
  回退 `DHV_VERSION` 环境变量，最终回退 `0.0.0`（单一来源纪律不变，只增稳健性）。

---

# 变更日志（CHANGELOG）

本文件记录工具链版本演进；语言规范级变更另见 [toolchain/hsl-spec/BNF.md §8](toolchain/hsl-spec/BNF.md)。

## [0.2.56] (2026-09-05)

**#L-22 native 值模型断层修复批次**（来源：Curator SUT 泛化实验实录 `entities.clone()` 运行期 panic「foreign 没有方法 clone」；先证据链后修复，158 用例回归全绿）：

- **`$host.make` 结构体/枚举变体构造通道**（interp.linkProgram 幂等注入 hostApi，校验与结构体字面量同规则）：native 块返回的 plain object 不带 `__struct`/`__enum` 运行时标记 → foreign 值（字段直通可用，clone/方法/模式派发全失效）。此前唯一出路是「native 拍平字符串 + HSL 侧 split_once 逐字段重建」定式（Curator ~40 行协议代码 + 值不得含 `~`/`|` 的协议保留字约束）。现 `$host.make("Entity", {...})` 直接产出带标记合法值；命名字段变体 `$host.make("Status::Pair", {a, b})`；元组变体走数组 payload；单元变体无 payload；prelude 族（Result::Ok/Err、Option::Some/None）显式镜像。字段完备性/多余性/元组长度校验失败可观测报错。SUT 侧验证：Curator parse_extract 以 $host.make 重写（拍平协议全删），15/15 场景黄金输出逐字节等价。
- **S-18 native 值模型断层预警**（dhv-ts checker + dhv typecheck.rs 双端同口径，conformance 第 6 段「预警对等」锁定 —— 退出码对拍看不见警告是否产出）：`let <含 struct/enum 族名注解> = native <...>` 且体无 `$host.make` → 警告（foreign 值：字段直通可用，clone/方法/模式派发失效）。只判同现场 let 注解 + 初始化器（零误报面窄）；String/数值注解的合法拍平协议不触发。
- **锁定用例**：run-all 149→158（$host.make 结构体/命名字段/元组/单元变体/错误路径 ×5 + S-18 触发 ×2 + 零误报 ×2）；conformance 62→66（check fixtures +2 + S-18 预警对等 ×2）；dhv cargo test 15/15；dsh/nova/backends-demo/Vigil/Curator 零回归。
- **guide BNF 镜像漂移治理**（docs + CI，来源：用户发现 `guide/BNF-v1.4.10.md` 遗留——权威源已演进至 BNF v1.5.0 且全仓无任何引用指向旧镜像，属孤儿过期文件）：删除 `guide/BNF-v1.4.10.md`，新增 `guide/BNF-v1.5.0.md`（与权威源 `toolchain/hsl-spec/BNF.md` 逐字节一致）；新增 `.github/workflows/spec-sync.yml` 守卫（guide/ 有且仅有一份 BNF-v*.md / 与权威源逐字节一致 / 文件名版本号 == 权威源标题版本号，任一违反即红灯），防止双份 BNF 再次静默漂移。README 文档导航补镜像行。

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
