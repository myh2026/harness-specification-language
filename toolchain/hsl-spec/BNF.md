# HSL 语言规范 —— BNF 文法（正式版 v1.5.0）

> **HSL — Harness Specification Language**
> 一门为编写 AI Agent harness 而生的编译型语言。本文件是 HSL 的**规范语法定义**，
> 是 DHV 编译器（Parser / Type Check / Codegen / Lint）的唯一权威语法依据。
>
> 本 BNF 覆盖两大部分：
> 1. **通用语言全量构件**——类型系统、函数、控制流、模式匹配、闭包、宏、模块……（现代编译型语言的完备集）
> 2. **HSL 专属构件**——`graph`/`edge` 拓扑、graph 内 `loop`、`project {}` 投射、`scale` 尺度、`block{}`/`static{}` 静态资源、`native lang {}` 逃生舱、`#[capability]` 能力域。
>
> 文法之外的强约束（类型规则、严格性、拓扑校验、投射一致性）在第 5 章「静态语义」中定义。

---

## 0. 记法约定

| 记法 | 含义 |
|:---|:---|
| `::=` | 定义为 |
| `A \| B` | 选择：A 或 B |
| `( ... )` | 分组 |
| `X*` | 重复 0 次或多次 |
| `X+` | 重复 1 次或多次 |
| `X?` | 可选（0 次或 1 次） |
| `"..."` | 字面终结符（串/符号） |
| `(* ... *)` | 文法注释 |
| `ε` | 空串 |
| `U+XXXX` | Unicode 码点 |
| `lookahead(!X)` | 负向预查：接下来的 token 不是 X |

产生式命名采用 `UpperCamelCase`。本规范为 **EBNF**（扩展巴科斯范式）；
第 6 章给出其到 pest PEG（DHV 实际实现）的映射约定。

**设计决策（本文档具有约束力的取舍）：**

| 决策 | 内容 | 理由 |
|:---|:---|:---|
| D1 | **无生命周期（lifetime）系统** | HSL 转译目标为 Python/TS/YAML 等无借用检查的语言；Rust 后端由编译器自动推导借用，源级不暴露 lifetime 语法 |
| D2 | **无裸指针、无 `unsafe`、无 `extern`** | 严格性铁律：编译期处决不安全状态；跨语言互操作一律走 `native` 逃生舱 |
| D3 | **无三元运算符 `?:`** | 用 `if expr { a } else { b }` 表达式替代（总纲第三条铁律） |
| D4 | **`static` 专用于静态资源块** | Rust 风格 `static VAR` 常量被移除，编译期常量统一用 `const` |
| D5 | **标签（label）使用 `'ident` 形式** | 仅供 `break`/`continue`/`loop` 嵌套跳出；这是 `'ident` 词法 token 的唯一用途（弥补 D1 删除 lifetime 后的空缺） |
| D6 | **`graph` 内 `loop` 与普通 `loop` 同形** | 语义约束（graph 必含 AgentLoop、match 穷尽性）由静态语义保证，而非另设语法 |
| D7 | **`as` 显式转换是唯一的类型转换通道** | 零隐式转换铁律的语法体现 |

---

## 1. 词法文法（Lexical Grammar）

### 1.1 源文件与字符集

```bnf
SourceFile   ::= BOM? Shebang? ItemOrProjection* EOF
Shebang      ::= "#!" (!NEWLINE ANY)* NEWLINE          (* 可选，仅文件首行 *)
BOM          ::= U+FEFF
EOF          ::= 文件结束
```

### 1.2 空白与注释

```bnf
Whitespace   ::= (U+0020 | U+0009 | U+000A | U+000D | U+000C)+
LineComment  ::= "//" (!NEWLINE ANY)*
BlockComment ::= "/*" (BlockComment | !"*/" ANY)* "*/"  (* 块注释可嵌套 *)
Comment      ::= LineComment | BlockComment
Trivia       ::= Whitespace | Comment
NEWLINE      ::= U+000A | U+000D U+000A | U+000D
```

注释可出现在任何 token 之间，不参与语法结构。

### 1.3 标识符

```bnf
Identifier   ::= NonRawIdentifier | RawIdentifier
NonRawIdentifier ::= IdentStart IdentContinue* (lookahead(!IdentContinue | !Keyword))
RawIdentifier    ::= "r#" Identifier
IdentStart   ::= "XID_Start" | "_"
IdentContinue::= "XID_Continue"
```

- `XID_Start` / `XID_Continue`：Unicode UAX#31 定义的标识符字符集。
- `r#` 前缀允许使用关键字作标识符（如 `r#type`）。
- 禁止由单个 `_` 构成普通标识符（`_` 是通配符模式）。

### 1.4 关键字

**严格关键字（不可作标识符，除非 `r#` 前缀）：**

```
as async await block break const continue dyn edge else enum export
false fn for from graph if impl import in let loop match mod mut
native on project return r# scale static struct trait true type
while where move use
```

**上下文关键字（仅特定位置具有语法含义，其他位置可作普通标识符）：**

```
monolith microkernel node with                  (* scale 值 / graph 声明 *)
rust python typescript yaml markdown json toml  (* 语言标识，native/project 中 *)
```

### 1.5 字面量

```bnf
Literal          ::= IntegerLiteral | FloatLiteral
                 | StringLiteral | RawStringLiteral
                 | CharLiteral | BooleanLiteral

IntegerLiteral   ::= (DecLiteral | HexLiteral | OctLiteral | BinLiteral) IntegerSuffix?
DecLiteral       ::= DEC_DIGIT (DEC_DIGIT | "_")*
HexLiteral       ::= "0x" (HEX_DIGIT | "_")+ (lookahead(!HEX_DIGIT))
OctLiteral       ::= "0o" ([0-7] | "_")+ (lookahead(![0-7]))
BinLiteral       ::= "0b" ([01] | "_")+ (lookahead(![01]))
IntegerSuffix    ::= ("i8"|"i16"|"i32"|"i64"|"i128"|"isize"
                 |  "u8"|"u16"|"u32"|"u64"|"u128"|"usize")

FloatLiteral     ::= DecLiteral "." DEC_DIGIT (DEC_DIGIT|"_")* FloatExp? FloatSuffix?
                 |  DecLiteral "."? FloatExp FloatSuffix?
                 |  DecLiteral FloatSuffix
FloatExp         ::= ("e"|"E") ("+"|"-")? (DEC_DIGIT|"_")+
FloatSuffix      ::= "f32" | "f64"
FloatLiteral     ::= ... (lookahead(!IdentifierStart))  (* 1.e3 与 1e3 均合法；1.max(2) 中 "1." 不是浮点 *)

CharLiteral      ::= "'" (CharChar | Escape) "'"
CharChar         ::= 除 "'"、"\\"、NEWLINE 外的任意 Unicode 字符

StringLiteral    ::= '"' (StringChar | Escape)* '"'
StringChar       ::= 除 '"'、"\\"、NEWLINE 外的任意 Unicode 字符
Escape           ::= "\\n" | "\\r" | "\\t" | "\\\\" | "\\0"
                 |  "\\x" HEX_DIGIT HEX_DIGIT
                 |  "\\u{" (HEX_DIGIT | "_")+ (lookahead(!" "_or_empty) "}")   (* \u{1F600} *)
                 |  "\\'" | "\\\""

RawStringLiteral ::= "r" HASH* '"' RawStringChar* '"' HASH*
RawStringChar    ::= 任意字符（直到匹配同等数量 # 的右引号）
HASH             ::= "#"

BooleanLiteral   ::= "true" | "false"
DEC_DIGIT        ::= [0-9]
HEX_DIGIT        ::= [0-9a-fA-F]
```

### 1.6 标签 token（唯一用途：循环标签，见 D5）

```bnf
LabelToken       ::= "'" Identifier
```

### 1.7 运算符与标点

```bnf
OperatorOrPunct ::= "+"  | "-"  | "*"  | "/"  | "%"  | "^"
                 | "!"  | "&"  | "&&" | "|"  | "||" | "<<"
                 | ">>" | "==" | "!=" | "<"  | ">"  | "<="
                 | ">=" | "="  | "+=" | "-=" | "*=" | "/="
                 | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>="
                 | "->" | "=>" | "::" | "."  | ".." | "..="
                 | ","  | ";"  | ":"  | "?"  | "@"  | "#"
                 | "$"  | "("  | ")"  | "["  | "]"  | "{"
                 | "}"  | "{{" | "}}"
```

`{{` / `}}` 为插值定界符（仅在 block/static 体中进入此模式，见 §1.9）。

### 1.8 词法歧义消解规则

| 规则 | 说明 |
|:---|:---|
| L1 | 最长匹配（maximal munch）优先：`>>` 是右移，不是两个泛型闭括号（泛型场景由 parser 层重排 token） |
| L2 | `1..3` 解析为整数 `1` + 范围 `..` + 整数 `3`（`.` 后无数字时不并入浮点） |
| L3 | `r#` 后必须跟 Identifier；`r"..."` 是原始字符串 |
| L4 | 嵌套泛型 `Vec<Vec<i32>>` 合法：parser 对 `>>` 做拆分 |
| L5 | `?` 在类型位置是 trait bound 的一部分已移除（D1），在表达式位置恒为 try 后缀 |

### 1.9 原始代码区词法模式（Lexical Mode Switching）

HSL 有两类**原始代码区**，词法器进入特殊模式：

**模式 A：`block {}` / `static {}` 体（静态资源）**

```bnf
RawBlockBody     ::= (RawText | Interpolation)*   (* 至深度归零的 "}" 为止 *)
Interpolation    ::= "{{" Trivia* Expression Trivia* "}}"
RawText          ::= 任意字符序列，但 "{{" 触发插值模式，"{" / "}" 参与深度计数
```

- 大括号深度计数：`block` 体中的 `{` 使深度 +1，`}` 使深度 -1；深度归零的 `}` 终结 block 体。
- 字符串字面量（YAML/JSON 中的 `"..."`）内的大括号**不**参与计数（按文本处理）。

**模式 B：`native lang {}` 体（逃生舱代码）**

```bnf
RawNativeBody    ::= 任意字符序列，大括号深度计数（含各目标语言的字符串/注释感知），至深度归零的 "}" 终结
```

- 词法器按目标语言的字符串定界规则（Python `"""`、Rust `r#""#`、TS 模板串等）跳过字符串内的大括号。
- 模式 B 下**不做任何 HSL 解析**，内容原样搬运至 Codegen。

---

## 2. 语法文法（Syntactic Grammar）—— 通用语言部分

### 2.1 程序与文件结构

一个 `.hsl` 文件分为**定义层**（纯净逻辑）与**投射层**（物理映射）：

```bnf
ItemOrProjection ::= Item | ScaleDecl | ProjectBlock
```

静态约束：每个文件**至多一个** `ProjectBlock`、**至多一个** `ScaleDecl`（见 §5.4）。

### 2.2 项（Items）

```bnf
Item            ::= OuterAttributes? VisItem
VisItem         ::= StructDef | EnumDef | TraitDef | ImplDef
                |  FnDef | ConstDef | TypeAliasDef
                |  GraphDef                       (* HSL 专属，见 §4.1 *)
                |  StaticResourceDef             (* HSL 专属，见 §4.2 *)
                |  ImportDecl | ExportItem
                |  MacroRulesDefinition
                |  MacroInvocationSemi           (* 语句级宏调用项 *)

MacroInvocationSemi ::= SimplePath "!" DelimTokenTree ";"

OuterAttributes ::= OuterAttribute+
OuterAttribute  ::= "#" "[" AttrPath AttrArgs? "]"
AttrPath        ::= SimplePath
AttrArgs        ::= "(" TokenTree* ")" | "=" Literal
```

### 2.3 模块：import / export（文件即模块）

```bnf
ImportDecl      ::= "import" ImportSpec "from" ModulePath ";"
ImportSpec      ::= "{" ImportItem ("," ImportItem)* ","? "}"
                |  "*" "as" Identifier
                |  Identifier ("as" Identifier)?
ImportItem      ::= Identifier ("as" Identifier)?
ModulePath      ::= StringLiteral                (* 相对路径，如 "../models/types.hsl" *)

ExportItem      ::= "export" Item                (* 导出修饰：export struct ... / export fn ... *)
```

- 默认私有；仅 `export` 的项可被其他 `.hsl` 文件 import。
- `import * as m from "..."` 引入命名空间 `m`，访问形如 `m.Prompt`。

### 2.4 结构体 / 枚举

```bnf
StructDef       ::= "struct" Identifier GenericParams? StructBody
StructBody      ::= NamedFieldsDef | TupleStructBody | ";"
TupleStructBody ::= TupleFieldsDef ";"
NamedFieldsDef  ::= "{" (NamedField ("," NamedField)* ","?)? "}"
NamedField      ::= OuterAttributes? Identifier ":" Type
TupleFieldsDef  ::= "(" (TupleField ("," TupleField)*)? ")"
TupleField      ::= OuterAttributes? Type

EnumDef         ::= "enum" Identifier GenericParams? "{" (EnumVariant ("," EnumVariant)* ","?)? "}"
EnumVariant     ::= OuterAttributes? Identifier
                   (TupleFieldsDef | NamedFieldsDef)?
                   ("=" IntegerLiteral)?         (* 判别式 *)
```

### 2.5 trait 与 impl

```bnf
TraitDef        ::= "trait" Identifier GenericParams? TraitSuper? "{" TraitItem* "}"
TraitSuper      ::= ":" TypeBound ("+" TypeBound)*
TraitItem       ::= TraitFnSig ";"
                |  ConstDef
                |  TypeAliasDef
                |  FnDef                          (* 默认实现 *)

TraitFnSig      ::= "async"? "fn" Identifier GenericParams? FnParams ("->" Type)?

ImplDef         ::= "impl" GenericParams? ImplTarget WhereClause? "{" ImplItem* "}"
ImplTarget      ::= Type ("for" Type)?            (* impl Trait for Type 或 impl Type *)
ImplItem        ::= FnDef | ConstDef | TypeAliasDef
```

### 2.6 函数

```bnf
FnDef           ::= "async"? "fn" Identifier GenericParams? FnParams
                   ("->" Type)? WhereClause? FnBody
FnBody          ::= BlockExpression | ";"
FnParams        ::= "(" (FnParam ("," FnParam)*)? ")"
FnParam         ::= OuterAttributes? (SelfParam | ("mut")? Pattern ":" Type)
SelfParam       ::= "&" "mut"? "self" | "mut"? "self"
```

- 无 `unsafe` / 无 `extern "C"`（D2）：外部调用一律 `native` 块。

### 2.7 常量与类型别名

```bnf
ConstDef        ::= "const" Identifier ":" Type "=" Expression ";"
TypeAliasDef    ::= "type" Identifier GenericParams? "=" Type ";"
```

### 2.8 泛型参数与约束

```bnf
GenericParams   ::= "<" (GenericParam ("," GenericParam)*)? ">"
GenericParam    ::= TypeParam | ConstParam
TypeParam       ::= Identifier (":" TypeBound ("+" TypeBound)*)? ("=" Type)?
ConstParam      ::= "const" Identifier ":" Type
TypeBound       ::= Type                          (* 仅 trait bound；无 lifetime bound，D1 *)
WhereClause     ::= "where" (WherePredicate ("," WherePredicate)*)?
WherePredicate  ::= Type ":" TypeBound ("+" TypeBound)*
```

### 2.9 类型

```bnf
Type            ::= TypeNoBounds | ImplTraitType | TraitObjectType | InferType
TypeNoBounds    ::= ParenthesizedType | NeverType | PathType | TupleType
                |  ArrayType | SliceType | ReferenceType | FnPtrType
InferType       ::= "_"

PathType        ::= TypePath
TypePath        ::= ("::")? TypePathSegment ("::" TypePathSegment)*
TypePathSegment ::= "::"? Identifier GenericArgs?
GenericArgs     ::= "<" (GenericArg ("," GenericArg)*)? ">"
GenericArg      ::= Type | ConstArg
ConstArg        ::= Literal | BlockExpression      (* [u8; N] 中的 N *)

TupleType       ::= "(" ")" | "(" Type ("," Type)* ","? ")"
                |  "(" Type "," Type ("," Type)* ")"      (* 单元素须尾逗号 *)
ArrayType       ::= "[" Type ";" ConstArg "]"
SliceType       ::= "[" Type "]"
ReferenceType   ::= "&" "mut"? TypeNoBounds
NeverType       ::= "!"
ParenthesizedType ::= "(" Type ")"
FnPtrType       ::= "fn" FnParams ("->" Type)?     (* 函数指针类型 *)
TraitObjectType ::= "dyn" TypeBound ("+" TypeBound)+ ("+" AutoTrait)*
AutoTrait       ::= "Send" | "Sync"                (* 编译期标记，语义由后端映射 *)
ImplTraitType   ::= "impl" TypeBound ("+" TypeBound)+    (* 仅参数/返回位置 *)
```

**内建类型（静态语义 §5.2 定义其精确位宽与各后端映射）：**
`bool char i8 i16 i32 i64 i128 isize u8 u16 u32 u64 u128 usize f32 f64
String str Vec<T> HashMap<K,V> HashSet<T> Option<T> Result<T,E> Box<T>`

### 2.10 模式（Patterns）

```bnf
Pattern         ::= OrPattern
OrPattern       ::= SinglePattern ("|" SinglePattern)*
SinglePattern   ::= RangePattern
                |  LiteralPattern | IdentifierPattern | WildcardPattern
                |  RestPattern | StructPattern | TupleStructPattern
                |  TuplePattern | GroupedPattern | PathPattern

LiteralPattern  ::= BooleanLiteral | CharLiteral | StringLiteral
                |  IntegerLiteral | FloatLiteral
                |  "-" IntegerLiteral | "-" FloatLiteral

IdentifierPattern ::= "mut"? Identifier ("@" Pattern)?
WildcardPattern ::= "_"
RestPattern     ::= ".."
RangePattern    ::= RangePatternBound (".." | "..=") RangePatternBound
RangePatternBound ::= LiteralPattern | PathPattern | "-" IntegerLiteral

StructPattern   ::= PathPattern "{" (StructPatternElem ("," StructPatternElem)* ","?)? "}"
StructPatternElem ::= Identifier ":" Pattern
                  |  "mut"? Identifier
                  |  RestPattern

TupleStructPattern ::= PathPattern "(" TupleStructItems? ")"
TupleStructItems   ::= Pattern ("," Pattern)* RestPattern?  |  RestPattern

TuplePattern    ::= "(" ")" | "(" Pattern ("," Pattern)* ","? ")"
                |  "(" Pattern "," ")"                (* 单元素须尾逗号 *)
GroupedPattern  ::= "(" Pattern ")"
PathPattern     ::= ("::")? PathSegmentPattern ("::" PathSegmentPattern)*
PathSegmentPattern ::= Identifier GenericArgs?
```

- 无 `ref` / `ref mut` 模式（D1：无借用语义，按值绑定规则由静态语义 §5.1 定义）。

### 2.11 表达式（Expressions）

#### 2.11.1 总入口与含块表达式

```bnf
Expression      ::= ExpressionWithBlock | ExpressionWithoutBlock

ExpressionWithBlock ::= BlockExpression | AsyncBlockExpression
                   |  IfExpression | IfLetExpression
                   |  MatchExpression
                   |  LoopExpression | WhileExpression | WhileLetExpression
                   |  ForExpression
                   |  NativeBlockExpression           (* HSL 专属，§4.3 *)
```

#### 2.11.2 运算符优先级链（自低向高；结合性见 §5.6 总表）

```bnf
ExpressionWithoutBlock ::= AssignmentExpression | OrExpression

AssignmentExpression ::= OrExpression ("=" | CompoundAssignOp) AssignmentExpression
CompoundAssignOp    ::= "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>="
                      (* 右结合；左操作数须为 place 表达式（§5.1） *)

OrExpression       ::= AndExpression ("||" AndExpression)*
AndExpression      ::= BitOrExpression ("&&" BitOrExpression)*
BitOrExpression    ::= BitXorExpression ("|" BitXorExpression)*
BitXorExpression   ::= BitAndExpression ("^" BitAndExpression)*
BitAndExpression   ::= EqualityExpression ("&" EqualityExpression)*
EqualityExpression ::= RelationalExpression (("==" | "!=") RelationalExpression)*
RelationalExpression ::= ShiftExpression (("<" | ">" | "<=" | ">=") ShiftExpression)*
ShiftExpression    ::= AdditiveExpression (("<<" | ">>") AdditiveExpression)*
AdditiveExpression ::= MultiplicativeExpression (("+" | "-") MultiplicativeExpression)*
MultiplicativeExpression ::= CastExpression (("*" | "/" | "%") CastExpression)*
CastExpression     ::= UnaryExpression ("as" Type)*
UnaryExpression    ::= ("-" | "!" | "*" | "&" | "&" "mut") UnaryExpression
                   |  PostfixExpression
```

#### 2.11.3 后缀表达式（含 HSL 的 `?` 与 `.await`）

```bnf
PostfixExpression ::= PrimaryExpression PostfixOp*
PostfixOp         ::= "?"                            (* try / 错误传播后缀 *)
                   |  "." "await"                    (* 异步等待后缀 *)
                   |  "." FieldAccess
                   |  "." MethodCall
                   |  "(" CallArgs? ")"              (* 调用 *)
                   |  "[" IndexOrRange "]"           (* 索引 / 切片 *)

FieldAccess       ::= Identifier | IntegerLiteral    (* 元组下标 tup.0 *)
MethodCall       ::= Identifier GenericArgs? "(" CallArgs? ")"
CallArgs          ::= Expression ("," Expression)* ","?
IndexOrRange      ::= Expression | RangeExpr
RangeExpr         ::= Expression? (".." | "..=") Expression?
```

#### 2.11.4 基础表达式

```bnf
PrimaryExpression ::= LiteralExpression
                  |  PathExpression
                  |  GroupedExpression
                  |  TupleExpression
                  |  ArrayExpression
                  |  StructExpression
                  |  ClosureExpression
                  |  IfExpression | IfLetExpression
                  |  MatchExpression
                  |  BlockExpression | AsyncBlockExpression
                  |  LoopExpression | WhileExpression | WhileLetExpression | ForExpression
                  |  BreakExpression | ContinueExpression | ReturnExpression
                  |  MacroInvocation
                  |  NativeBlockExpression

LiteralExpression ::= Literal
PathExpression    ::= PathInExpr
GroupedExpression::= "(" Expression ")"
TupleExpression  ::= "(" ")" | "(" (Expression ",")+ Expression? ")"
ArrayExpression  ::= "[" ArrayElements? "]"
ArrayElements    ::= Expression ("," Expression)* ","?
                 |  Expression ";" Expression         (* 重复展开 [0; 256] *)

StructExpression ::= PathInExpr "{" StructExprFields? "}"
StructExprFields ::= StructExprField ("," StructExprField)* ","?
StructExprField  ::= Identifier ":" Expression
                 |  IntegerLiteral | Identifier      (* 简写：字段名 = 变量名 *)
                 |  ".." Expression                  (* 功能更新语法 ..base *)

PathInExpr       ::= ("::")? PathExprSegment ("::" PathExprSegment)*
PathExprSegment  ::= "::"? Identifier GenericArgs?
SimplePath       ::= ("::")? Identifier ("::" Identifier)*
```

#### 2.11.5 闭包

```bnf
ClosureExpression ::= "move"? "async"? "||" ClosureParams? ("->" Type)? BlockExpression
                 |   "move"? "async"? "|" ClosureParams "|" (BlockExpression | ExpressionWithoutBlock)
ClosureParams    ::= ClosureParam ("," ClosureParam)* ","?
ClosureParam     ::= Pattern (":" Type)?
```

#### 2.11.6 条件表达式

```bnf
IfExpression     ::= "if" Expression BlockExpression
                   ("else" (IfExpression | IfLetExpression | BlockExpression))?

IfLetExpression  ::= "if" "let" Pattern "=" Expression BlockExpression
                   ("else" (IfExpression | IfLetExpression | BlockExpression))?
```

- `if` 条件表达式类型必须为 `bool`（零隐式转换，§5.1）。

#### 2.11.7 match 表达式

```bnf
MatchExpression  ::= "match" Expression "{" (MatchArm (","? MatchArm)*)? "}"
MatchArm         ::= OuterAttributes? MatchArmPattern ("if" Guard)? "=>" MatchArmBody
MatchArmPattern  ::= Pattern
Guard            ::= Expression
MatchArmBody     ::= Expression | ","
```

- 穷尽性与不可达 arm 由静态语义 §5.1 强制（match 是表达式，各 arm 类型必须一致）。

#### 2.11.8 循环表达式

```bnf
LoopExpression   ::= LoopLabel? "loop" BlockExpression
WhileExpression  ::= LoopLabel? "while" Expression BlockExpression
WhileLetExpression ::= LoopLabel? "while" "let" Pattern "=" Expression BlockExpression
ForExpression    ::= LoopLabel? "for" Pattern "in" Expression BlockExpression
LoopLabel        ::= LabelToken ":"                       (* 'outer: loop {...} *)
```

#### 2.11.9 跳转表达式

```bnf
BreakExpression  ::= "break" LabelToken? Expression?
ContinueExpression ::= "continue" LabelToken?
ReturnExpression ::= "return" Expression?
```

#### 2.11.10 块与异步块

```bnf
BlockExpression  ::= "{" Statement* "}"
AsyncBlockExpression ::= "async" "move"? BlockExpression
```

### 2.12 语句（Statements）

```bnf
Statement        ::= ";"
                |  Item
                |  LetStatement
                |  ExpressionStatement

LetStatement     ::= OuterAttributes? "let" "mut"? Pattern (":" Type)?
                   ("=" Expression)? WhereClause? ("else" BlockExpression)? ";"
                   (* let ... else：模式不匹配时发散 *)

ExpressionStatement ::= Expression ";"
                   |  ExpressionWithoutBlock            (* 块表达式结尾可省分号并作为块的值 *)
                   |  ExpressionWithBlock ";"           (* 有分号则为语句，值为 () *)
```

### 2.13 宏系统（macro_rules!）

```bnf
MacroRulesDefinition ::= "macro_rules" "!" Identifier "{" MacroRuleSemi+ "}"
MacroRuleSemi    ::= "(" MacroMatcher ")" "=>" MacroTranscriber ";"
MacroMatcher     ::= MacroMatch*
MacroMatch       ::= Token
                 |  DelimTokenTree
                 |  "$" Identifier ":" MacroFragSpec
                 |  MacroRep
MacroRep         ::= "$" "(" MacroMatch+ ")" MacroRepOp Separator? MacroRepOp2
MacroRepOp       ::= ("*" | "+" | "?")
Separator        ::= Token                                (* 重复项之间的分隔 token *)
MacroRepOp2      ::= ε                                    (* 见 §6 pest 映射的等价写法 *)

MacroTranscriber ::= MacroTranscribe*
MacroTranscribe  ::= Token
                 |  "$" Identifier
                 |  DelimTokenTree
                 |  "$" "(" MacroTranscribe+ ")" MacroRepOp "$"?

MacroFragSpec    ::= "ident" | "path" | "expr" | "ty" | "pat"
                 |  "stmt" | "block" | "item" | "literal" | "tt" | "meta"

MacroInvocation  ::= SimplePath "!" DelimTokenTree
DelimTokenTree   ::= "(" TokenTree* ")" | "[" TokenTree* "]" | "{" TokenTree* "}"
TokenTree        ::= Token | DelimTokenTree
Token            ::= Identifier | Literal | OperatorOrPunct | LabelToken | RawIdentifier
```

---

## 3. 语法文法 —— HSL 专属构件

### 3.1 graph：Agent 拓扑（一等公民）

```bnf
GraphDef         ::= OuterAttributes? "graph" Identifier GenericParams?
                   GraphParams? ("->" Type)? WhereClause? "{" GraphBody "}"

GraphParams      ::= "(" (GraphParam ("," GraphParam)*)? ")"
GraphParam       ::= "mut"? Identifier ":" Type

GraphBody        ::= GraphStmt*
GraphStmt        ::= NodeDecl
                 |  EdgeDecl
                 |  LetStatement
                 |  Statement
                 |  Item                                    (* 允许局部项 *)

NodeDecl         ::= "node" "mut"? Identifier ":" Type ("=" Expression)? ";"
EdgeDecl         ::= "edge" EdgeEndpoint ("->" EdgeEndpoint)+
                   ("on" Guard)? EdgeAttrs? ";"
EdgeEndpoint     ::= PathInExpr
EdgeAttrs        ::= "with" EdgeAttr ("," EdgeAttr)*
EdgeAttr         ::= Identifier ("=" Literal)?
Guard            ::= Expression | Pattern                   (* on Action::CallTool *)
```

**AgentLoop（graph 体内的核心循环）：**

```bnf
AgentLoop        ::= LoopLabel? "loop" BlockExpression      (* 与普通 loop 同形，见 D6 *)
```

- 静态约束（详见 §5.3）：graph body 必须**恰含至少一个** AgentLoop；
  loop 内的 `match action` 必须穷尽所有 `Action` 变体；
  `edge` 引用的端点必须在 graph body 中已由 `node`/`let` 声明。

### 3.2 block / static：静态资源原生块

```bnf
StaticResourceDef ::= ResourceKind Identifier "{" RawBlockBody "}"
ResourceKind     ::= "block" | "static"                     (* 同义，风格偏好由 lint 决定 *)
RawBlockBody     ::= (RawText | Interpolation)*
Interpolation    ::= "{{" Trivia* Expression Trivia* "}}"
```

- 体内容为**原始文本**（配置/文档/提示词），不做 HSL 解析；
- `{{ expr }}` 为编译期插值，表达式必须可 `ToString`（§5.5）；
- 大括号深度计数规则见 §1.9 模式 A。

### 3.3 native：跨语言逃生舱

```bnf
NativeBlockExpression ::= "native" LangIdent "{" RawNativeBody "}"
LangIdent        ::= Identifier                             (* rust | python | typescript | ... 注册后端 *)
```

- 作为**表达式**使用：可出现在 `let x: T = native python { ... };` 或函数体任意表达式位置；
- 块内为原始目标语言代码（§1.9 模式 B），HSL 编译器不做解析，原样搬运；
- 返回值：块内**最后一个无分号表达式**的值（如示例中 Python 的 `response.choices[0].message.content`）；
- 捕获变量：块内引用的外部 HSL 变量按名字映射到目标语言（§5.5 安全规则）。

### 3.4 project：物理投射声明

```bnf
ProjectBlock     ::= "project" "{" (ProjectionItem | RulesBlock)* "}"
ProjectionItem   ::= ProjectionTarget "->" StringLiteral ":" LangIdent ","?
ProjectionTarget ::= PathInExpr                             (* 指向本文件定义的逻辑项 *)

(* —— v1.5 新增：投射规则组 —— *)
RulesBlock       ::= "rules" "{" RulesItem* "}"
RulesItem        ::= ItemKind "->" PathTemplate ":" LangIdent ","?
ItemKind         ::= "graph" | "fn" | "struct" | "enum" | "trait"
                  |  "const" | "type" | "block" | "static"
PathTemplate     ::= StringLiteral                           (* 唯一占位符 {name} *)
```

- `逻辑项 → 物理文件 : 目标语言`；
- 静态约束（§5.4）：同一物理路径不得被两个投射项占据；目标项必须存在且可见；
  `block/static` 只能投射到 yaml/markdown/json/toml 等静态后端；
  函数/impl/graph 可投射到 rust/python/typescript。

**v1.5 投射规则组语义（R1-R6）**：为免逐项手写映射，`rules` 按项类型批量投射：

- **R1（遮蔽原则）**：显式单项映射优先；未显式映射的命名项按其类型匹配唯一规则展开，`{name}` 替换为项名。
- **R2（占位符白名单）**：路径模板 v1 仅支持 `{name}`；其他占位符 → 诊断 **P5**。
- **R3（唯一性）**：同一规则类型只允许声明一条；重复 → **P5**。
- **R4（类型注册）**：规则类型限 `graph/fn/struct/enum/trait/const/type/block/static`（block 与 static 同义，均指 StaticResourceDef）；未知类型 → **P5**。
- **R5（展开池）**：展开池 = 本文件命名项 + import 依赖模块的导出命名项；`impl`（匿名）、import、宏调用不参与。
- **R6（一致性）**：展开项与显式项同等参与 P2（路径唯一）/ P4（后端层级）校验。

示例：

```hsl
project {
    Nova -> "src/main.rs" : rust,          // 显式映射（优先）

    rules {
        struct -> "src/types/{name}.rs"  : rust,
        enum   -> "src/types/{name}.rs"  : rust,
        fn     -> "src/logic/{name}.rs"  : rust,
        graph  -> "src/graphs/{name}.rs" : rust,
        block  -> "config/{name}.yml"    : yaml,
    }
}
```

### 3.5 scale：尺度声明

```bnf
ScaleDecl        ::= "scale" "=" ScaleMode ";"
ScaleMode        ::= "monolith" | "microkernel" | Identifier (* 扩展模式经编译器注册 *)
```

### 3.6 HSL 内建属性（完整清单见 §5.7）

```bnf
(* 例： *)
(* #[capability(file_write, net_connect)] —— 能力域声明，编译期处决越界 *)
(* #[cfg(lang: rust)]                     —— 条件编译：仅特定后端保留 *)
(* #[derive(Debug, Clone, Serialize)]     —— 派生实现 *)
(* #[doc("...")]                           —— 文档 *)
```

---

## 4. 完整产生式索引（按字母序）

```bnf
AdditiveExpression        ::= (见 §2.11.2)
AndExpression             ::= (见 §2.11.2)
ArrayExpression           ::= (见 §2.11.4)
ArrayType                 ::= (见 §2.9)
AsmClause                 ::= ε   (* 保留：HSL 无内联汇编（D2） *)
AssignmentExpression      ::= (见 §2.11.2)
AsyncBlockExpression      ::= (见 §2.11.10)
AttrArgs                  ::= (见 §2.2)
AttrPath                  ::= (见 §2.2)
Attribute                 ::= OuterAttribute
AwaitPostfix              ::= (见 §2.11.3 PostfixOp)
BinLiteral                ::= (见 §1.5)
BitAndExpression          ::= (见 §2.11.2)
BitOrExpression           ::= (见 §2.11.2)
BitXorExpression          ::= (见 §2.11.2)
BlockComment              ::= (见 §1.2)
BlockExpression           ::= (见 §2.11.10)
BooleanLiteral            ::= (见 §1.5)
BreakExpression           ::= (见 §2.11.9)
CallArgs                  ::= (见 §2.11.3)
CastExpression            ::= (见 §2.11.2)
CharLiteral               ::= (见 §1.5)
ClosureExpression         ::= (见 §2.11.5)
Comment                   ::= (见 §1.2)
CompoundAssignOp          ::= (见 §2.11.2)
ConstArg                  ::= (见 §2.9)
ConstDef                  ::= (见 §2.7)
ConstParam                ::= (见 §2.8)
ContinueExpression        ::= (见 §2.11.9)
DecLiteral                ::= (见 §1.5)
DelimTokenTree            ::= (见 §2.13)
EdgeDecl                  ::= (见 §3.1)
EdgeAttrs                 ::= (见 §3.1)
EdgeEndpoint              ::= (见 §3.1)
EnumDef                   ::= (见 §2.4)
EnumVariant               ::= (见 §2.4)
EqualityExpression        ::= (见 §2.11.2)
Escape                    ::= (见 §1.5)
Expression                ::= (见 §2.11.1)
ExpressionStatement       ::= (见 §2.12)
ExportItem                ::= (见 §2.3)
FieldAccess               ::= (见 §2.11.3)
FloatLiteral              ::= (见 §1.5)
FnBody                    ::= (见 §2.6)
FnDef                     ::= (见 §2.6)
FnParam                   ::= (见 §2.6)
FnParams                  ::= (见 §2.6)
FnPtrType                 ::= (见 §2.9)
ForExpression             ::= (见 §2.11.8)
GenericArg                ::= (见 §2.9)
GenericArgs               ::= (见 §2.9)
GenericParams             ::= (见 §2.8)
GraphDef                  ::= (见 §3.1)
GraphBody                 ::= (见 §3.1)
GraphParam                ::= (见 §3.1)
GraphParams               ::= (见 §3.1)
GroupedExpression         ::= (见 §2.11.4)
GroupedPattern            ::= (见 §2.10)
Guard                     ::= (见 §3.1)
HexLiteral                ::= (见 §1.5)
Identifier                ::= (见 §1.3)
IdentifierPattern        ::= (见 §2.10)
IfExpression              ::= (见 §2.11.6)
IfLetExpression           ::= (见 §2.11.6)
ImplDef                   ::= (见 §2.5)
ImplItem                  ::= (见 §2.5)
ImplTarget                ::= (见 §2.5)
ImplTraitType             ::= (见 §2.9)
ImportDecl                ::= (见 §2.3)
ImportItem                ::= (见 §2.3)
ImportSpec                ::= (见 §2.3)
InferType                 ::= (见 §2.9)
IntegerLiteral            ::= (见 §1.5)
Interpolation             ::= (见 §3.2)
Item                      ::= (见 §2.2)
ItemOrProjection          ::= (见 §2.1)
LabelToken                ::= (见 §1.6)
LangIdent                 ::= (见 §3.3)
LetStatement              ::= (见 §2.12)
LineComment               ::= (见 §1.2)
Literal                   ::= (见 §1.5)
LiteralExpression         ::= (见 §2.11.4)
LiteralPattern            ::= (见 §2.10)
LoopExpression            ::= (见 §2.11.8)
LoopLabel                 ::= (见 §2.11.8)
MacroInvocation           ::= (见 §2.13)
MacroInvocationSemi       ::= (见 §2.2)
MacroMatch                ::= (见 §2.13)
MacroMatcher              ::= (见 §2.13)
MacroRulesDefinition      ::= (见 §2.13)
MacroTranscriber          ::= (见 §2.13)
MatchArm                  ::= (见 §2.11.7)
MatchArmBody              ::= (见 §2.11.7)
MatchArmPattern           ::= (见 §2.11.7)
MatchExpression           ::= (见 §2.11.7)
MethodCall                ::= (见 §2.11.3)
ModulePath                ::= (见 §2.3)
MultiplicativeExpression  ::= (见 §2.11.2)
NamedField                ::= (见 §2.4)
NamedFieldsDef            ::= (见 §2.4)
NativeBlockExpression     ::= (见 §3.3)
NeverType                 ::= (见 §2.9)
NodeDecl                  ::= (见 §3.1)
OctLiteral                ::= (见 §1.5)
OperatorOrPunct           ::= (见 §1.7)
OrExpression              ::= (见 §2.11.2)
OuterAttribute            ::= (见 §2.2)
OuterAttributes           ::= (见 §2.2)
ParenthesizedType         ::= (见 §2.9)
PathExprSegment           ::= (见 §2.11.4)
PathInExpr                ::= (见 §2.11.4)
PathPattern               ::= (见 §2.10)
PathSegmentPattern        ::= (见 §2.10)
PathType                  ::= (见 §2.9)
Pattern                   ::= (见 §2.10)
PostfixExpression         ::= (见 §2.11.3)
PostfixOp                 ::= (见 §2.11.3)
PrimaryExpression         ::= (见 §2.11.4)
ProjectBlock              ::= (见 §3.4)
ProjectionItem            ::= (见 §3.4)
RulesBlock                ::= (见 §3.4)
RulesItem                 ::= (见 §3.4)
ItemKind                  ::= (见 §3.4)
PathTemplate              ::= (见 §3.4)
ProjectionTarget          ::= (见 §3.4)
RangeExpr                 ::= (见 §2.11.3)
RangePattern              ::= (见 §2.10)
RangePatternBound         ::= (见 §2.10)
RawBlockBody              ::= (见 §3.2)
RawIdentifier             ::= (见 §1.3)
RawNativeBody             ::= (见 §1.9)
RawStringLiteral          ::= (见 §1.5)
RelationalExpression      ::= (见 §2.11.2)
RestPattern               ::= (见 §2.10)
ReturnExpression          ::= (见 §2.11.9)
ReferenceType             ::= (见 §2.9)
ScaleDecl                 ::= (见 §3.5)
ScaleMode                 ::= (见 §3.5)
SelfParam                 ::= (见 §2.6)
Shebang                   ::= (见 §1.1)
ShiftExpression           ::= (见 §2.11.2)
SimplePath                ::= (见 §2.11.4)
SliceType                 ::= (见 §2.9)
SourceFile                ::= (见 §1.1)
Statement                 ::= (见 §2.12)
StaticResourceDef         ::= (见 §3.2)
StringLiteral             ::= (见 §1.5)
StructDef                 ::= (见 §2.4)
StructExprField           ::= (见 §2.11.4)
StructExprFields          ::= (见 §2.11.4)
StructExpression          ::= (见 §2.11.4)
StructPattern             ::= (见 §2.10)
StructPatternElem         ::= (见 §2.10)
TraitDef                  ::= (见 §2.5)
TraitFnSig                ::= (见 §2.5)
TraitItem                 ::= (见 §2.5)
TraitObjectType           ::= (见 §2.9)
TraitSuper                ::= (见 §2.5)
TranslateBound            ::= ε   (* 保留占位 *)
Trivia                    ::= (见 §1.2)
TryPostfix                ::= (见 §2.11.3 PostfixOp "?")
TupleExpression           ::= (见 §2.11.4)
TupleField                ::= (见 §2.4)
TupleFieldsDef            ::= (见 §2.4)
TuplePattern              ::= (见 §2.10)
TupleStructBody           ::= (见 §2.4)
TupleStructPattern        ::= (见 §2.10)
TupleType                 ::= (见 §2.9)
Type                      ::= (见 §2.9)
TypeAliasDef              ::= (见 §2.7)
TypeBound                 ::= (见 §2.8)
TypeNoBounds              ::= (见 §2.9)
TypeParam                 ::= (见 §2.8)
TypePath                  ::= (见 §2.9)
TypePathSegment           ::= (见 §2.9)
UnaryExpression           ::= (见 §2.11.2)
VisItem                   ::= (见 §2.2)
WhereClause               ::= (见 §2.8)
WherePredicate            ::= (见 §2.8)
WhileExpression           ::= (见 §2.11.8)
WhileLetExpression        ::= (见 §2.11.8)
Whitespace                ::= (见 §1.2)
WildcardPattern           ::= (见 §2.10)
```

---

*（第 5 章「静态语义」与第 6 章「pest 映射」见本文档下半部分。）*

---

## 5. 静态语义（BNF 之外的强约束）

### 5.1 严格性铁律（编译期处决）

| 编号 | 规则 | 检测时机 |
|:---|:---|:---|
| S1 | **零隐式转换**：任何类型转换必须显式 `as`。字面量推断除外（`let x: u8 = 1` 合法）。`if x {}` 中 x 非 bool → 编译错误 | TypeCheck |
| S2 | **非空默认**：变量默认不可空。可空必须 `Option<T>`，访问必须 `match`/`if let`/`unwrap_or*`（裸 `unwrap` 产生 lint 警告） | TypeCheck + Lint |
| S3 | **强制错误处理**：返回 `Result<T,E>` 的调用必须被 `?`、`match`、`let Ok(..) = .. else` 之一处理，或显式标注 `#[allow(unhandled)]` | TypeCheck |
| S4 | **不可变优先**：`let` 默认不可变；`mut` 必须显式；对不可变绑定赋值 → 编译错误 | TypeCheck |
| S5 | **`?` 独占错误传播**：`?` 只能用于 `Result`/`Option`；三元运算符不存在（语法层面即无） | Parser |
| S6 | **穷尽 match**：所有 match 必须穷尽；graph AgentLoop 中对 `Action` 的 match 不允许 `_` 通配兜底（必须显式列出所有变体，逼你直面新分支）——`#[cfg]` 变体除外 | TypeCheck |
| S7 | **未使用即错误**：未使用的 `let` 绑定、`import`、graph node → Lint 错误（`_` 前缀豁免） | Lint |
| S8 | **变量遮蔽**：同作用域遮蔽 → Lint 错误；跨作用域遮蔽 → Lint 警告 | Lint |

### 5.2 内建类型与后端映射表

| HSL 类型 | Rust 后端 | Python 后端 | TypeScript 后端 |
|:---|:---|:---|:---|
| `bool` | `bool` | `bool` | `boolean` |
| `i32` 等 | 同名 | `int` | `number` |
| `f64` | `f64` | `float` | `number` |
| `String` | `String` | `str` | `string` |
| `&str` | `&str` | `str` | `string` |
| `Vec<T>` | `Vec<T>` | `list[T]` | `T[]` |
| `HashMap<K,V>` | `std::collections::HashMap` | `dict[K,V]` | `Map<K,V>` |
| `Option<T>` | `Option<T>` | `T \| None` | `T \| undefined` |
| `Result<T,E>` | `Result<T,E>` | `T` + 异常封装 | `T` + 异常封装 |
| `Box<dyn Trait>` | `Box<dyn Trait>` | 协议类实例 | 接口实例 |
| `[T; N]` | `[T; N]` | `tuple[T,...]` | `readonly [T,...]` |

泛型采用**编译期单态化**（monomorphization）：HSL 编译器为每个具体实例化生成目标语言特化代码；
`dyn Trait` 采用 vtable 方案（Python/TS 天然鸭子类型，Rust 生成 trait object）。

#### 5.2.1 后端语言注册表（v1.4 —— 38 后端）

`project {}` 的 `<lang-id>` 取值来自本注册表（封闭集合；别名归一：`ts→typescript`、
`js→javascript`、`py→python`、`md→markdown`、`yml→yaml`、`c++→cpp`、`sh→bash`）。

**Tier 1 · Harness 核心（10）**

| id | 语言 | 扩展名 | 能力级 |
|:--|:--|:--|:--|
| `python` | Python | .py | full 活体翻译（native 运行期可执行） |
| `typescript` | TypeScript | .ts | full 活体翻译（native 运行期可执行） |
| `javascript` | JavaScript | .js | full 活体翻译（native 运行期可执行） |
| `rust` | Rust | .rs | logic 语句子集 |
| `go` | Go | .go | logic 语句子集 |
| `cpp` | C++ | .cpp | logic 语句子集 |
| `java` | Java | .java | contract 类型契约（sealed interface + record） |
| `csharp` | C# | .cs | contract（abstract record） |
| `kotlin` | Kotlin | .kt | contract（sealed class + data class） |
| `swift` | Swift | .swift | contract（enum 关联值） |

**Tier 2 · 脚本与动态（8）**：`ruby` `php` `lua` `perl` `bash` `powershell` `r` `julia`（均 contract）

**Tier 3 · 函数式（6）**：`scala` `elixir` `erlang` `haskell` `ocaml` `fsharp`（contract；
Scala/Haskell/OCaml/F#/Erlang 原生和类型）

**Tier 4 · 系统与现代（8）**：`zig` `nim` `crystal` `dart` `groovy` `objectivec` `d` `vb`
（contract；Zig/Nim 原生 tagged union / object variants）

**静态格式（6）**：`yaml` `markdown` `json` `toml` `ini` `xml`（block/static 原文 + `{{}}` 插值渲染）

**能力级语义（诚实边界，写入 manifest.json）**

| 能力级 | 生成物 | 说明 |
|:--|:--|:--|
| full | 活体语句翻译 | 函数体真实转译为宿主语言（let/if/while/for/match 分发/format!/常用 std 方法映射表）；不可翻译构件触发整函数回退 contract（绝不输出半翻译代码） |
| logic | 语句子集翻译 | 同 full 的语句子集；rust/go/cpp |
| contract | 类型契约 | struct/enum/trait/impl/fn 签名真实翻译；函数体 = 围栏 HSL 源镜像 + 目标语言显式未实现标记（NotImplementedError/todo!/panic 等） |
| raw | 静态资源原文 | block/static 体 + `{{}}` 编译期插值 |

完整类型映射表（38 语言 × 17 内建类型）见 `dhv-ts/src/backends/registry.ts`（单一事实源）与
`dhv/src/langs.rs`。宿主语法校验：python 经 `python3 -m py_compile`、ts/js 经 bun 转译器、
bash 经 `bash -n`（Lint 第 2 层，emit 时自动执行）。

### 5.3 拓扑校验规则（graph / edge）

| 编号 | 规则 |
|:---|:---|
| G1 | graph body 必须恰含 ≥1 个 `AgentLoop`（顶层直接子节点中） |
| G2 | `edge` 的每个端点必须是 body 中已声明的 `node` / `let` 标识符（声明先于 edge） |
| G3 | 拓扑图（节点=graph 内可执行单元，边=edge）不得出现**编译期可判定的死锁**：即无显式 `edge` 环（`a -> b -> a`），除非环上至少一条边带 `on Guard` 条件（条件打破环） |
| G4 | 每个 `node` 必须可达（存在入边或被 loop 体内引用），否则 Lint 警告「孤岛节点」 |
| G5 | `edge ... on Pattern` 中的 Pattern 必须是与端点产出类型相关的枚举变体模式（通常为 `Action` 变体） |
| G6 | microkernel 尺度下，每条 edge 编译为事件总线订阅；monolith 尺度下编译为直接调用——语义等价性由 codegen 保证 |

### 5.4 投射一致性规则（project / scale）

| 编号 | 规则 |
|:---|:---|
| P1 | 每文件至多一个 `project {}` 块、至多一个 `scale = ...` 声明 |
| P2 | 同一物理文件路径在**整个工程**内只能被一个投射项占据（跨文件路径冲突 → 编译错误） |
| P3 | 投射目标项必须在本文件定义（或 import 引入）且可见；`project` 不得引用未导出的私有项 |
| P4 | `block`/`static` 只能投射到 `yaml`/`markdown`/`json`/`toml` 后端；`fn`/`impl`/`struct`/`graph` 只能投射到 `rust`/`python`/`typescript` |
| P5 | 投射到某语言的项，其内部 `native` 块的语言若与目标语言不一致 → 编译器必须生成 FFI 胶水（P8），否则编译错误 |
| P6 | `scale` 影响整个工程的架构形态，仅允许出现在被标记为**入口**的文件（含 `graph` 的文件），否则 Lint 警告 |
| P7 | `graph` 投射后，其 `-> Result<T,E>` 返回类型编译为目标语言的入口函数签名（如 Python 的 `def main() -> ...`） |

### 5.5 native 块与插值安全

| 编号 | 规则 |
|:---|:---|
| N1 | `native` 块引用的外部 HSL 变量必须：已声明、类型在目标语言有映射（§5.2）、且被 `#[allow]` 显式标记或类型为可平凡传递（bool/数值/String/Vec/Option） |
| N2 | `native` 块返回值：由上下文显式标注（`let x: T = native ...`）或由块尾表达式推断失败时报错——**禁止**依赖目标语言动态类型穿透 |
| N3 | 嵌套逃逸：`native python {}` 内不得再出现 HSL 语法；目标语言字符串内的 `{}` 按目标语言语义处理 |
| N4 | `block` 插值 `{{ expr }}`：表达式类型必须实现 `ToString`（数值/bool/String/枚举），`Vec`/`struct` → 编译错误 |
| N5 | 插值在**编译期**求值（const 上下文或字面量组合），运行期状态引用（如 `{{state.current_goal}}`）在生成时以占位符 + 注入点形式落地（YAML/MD 模板由运行时 harness 填充） |

### 5.6 运算符优先级与结合性总表

| 优先级 | 运算符 | 结合性 | 类别 |
|:---:|:---|:---|:---|
| 1（最低） | `=` `+=` `-=` `*=` `/=` `%=` `&=` `|=` `^=` `<<=` `>>=` | 右结合 | 赋值 |
| 2 | `..` `..=` | 不结合（仅范围位置） | 范围 |
| 3 | `\|\|` | 左结合 | 逻辑或 |
| 4 | `&&` | 左结合 | 逻辑与 |
| 5 | `\|` | 左结合 | 按位或 |
| 6 | `^` | 左结合 | 按位异或 |
| 7 | `&` | 左结合 | 按位与 |
| 8 | `==` `!=` | 不结合 | 相等比较 |
| 9 | `<` `>` `<=` `>=` | 不结合 | 关系比较 |
| 10 | `<<` `>>` | 左结合 | 移位 |
| 11 | `+` `-` | 左结合 | 加减 |
| 12 | `*` `/` `%` | 左结合 | 乘除模 |
| 13 | `as` | 左结合（后缀链） | 显式转换 |
| 14 | `-` `!` `*` `&` `&mut` | 右结合（一元） | 一元 |
| 15（最高） | `?` `.await` `.` `()` `[]` | 左结合（后缀链） | 后缀 |

比较运算符**不链式**（`a < b < c` 语法错误）；范围运算符仅出现在 `[..]` 与模式位置。

### 5.7 内建属性清单（封闭枚举 v1）

| 属性 | 参数 | 作用 | 违反后果 |
|:---|:---|:---|:---|
| `#[capability(name,...)]` | 能力名：`file_read file_write net_connect process_spawn secret_access` | 声明项所需能力域；`native` 块内调用超出能力域的 API → 编译错误 | 编译错误（编译期处决） |
| `#[cfg(lang: rust)]` | `lang:` / `scale:` / `feature:` | 条件编译：不满足条件的项被剥离 | — |
| `#[cfg_attr(...)]` | 同 cfg + 属性 | 条件属性 | — |
| `#[derive(...)]` | `Debug Clone PartialEq Eq Hash Serialize Deserialize` | 派生 trait 实现 | 不支持的派生 → 编译错误 |
| `#[doc("...")]` | 字符串 | 文档（投射为目标语言 docstring / 注释） | — |
| `#[allow(lint)]` | lint 名 | 豁免 Lint | — |
| `#[deny(lint)]` | lint 名 | 升级 Lint 为错误 | — |
| `#[tool(name, desc)]` | 预留 | Agent 工具注册元数据（P8 胶水生成） | — |

未知属性 → 编译错误（属性空间封闭，v1 不开放过程宏）。

### 5.8 名字解析与模块系统

| 编号 | 规则 |
|:---|:---|
| M1 | 文件即模块：`models/types.hsl` 的模块名是路径字符串本身 |
| M2 | import 路径为相对路径（相对当前 .hsl 文件）；仅 `.hsl` 后缀 |
| M3 | 传递导出禁止：`export` 只作用于本文件定义；re-export 必须显式 `export import` |
| M4 | 标准库前缀 `hsl::`（如 `hsl::collections::HashMap`）保留 |
| M5 | graph 内局部项的作用域规则与函数体一致（词法作用域） |

### 5.9 错误处理语义

- `?` 后缀：`Result<T,E>` → 提前返回 `Err(e→From::from)`；`Option<T>` → 提前返回 `None`；函数返回类型必须匹配。
- `From` trait 实现 `fn from(src: E1) -> E2`，用于 `?` 的自动包装（唯一的「隐式」转换，且是显式声明的）。
- `panic!` 宏保留但 Lint 警告（生产代码应使用 Result）。

---

## 6. 与 pest PEG 的映射（DHV 实现约定）

DHV 编译器使用 [pest](https://pest.rs/) 实现 P0 文法层。本 BNF → pest 的映射约定：

| BNF 构件 | pest 对应 |
|:---|:---|
| `"literal"` | `"literal"`（字符串字面量规则） |
| `X*` | `x*`；`X+` → `x+`；`X?` → `x?` |
| `A \| B` | `a \| b`（PEG 有序选择） |
| `lookahead(!X)` | `!x`（负向谓词）；`lookahead(X)` → `&x` |
| 任意字符 | `ANY`；非换行 `!("\\n" \| "\\r")` |
| Unicode 类 | `XID_Start` / `XID_Continue` 内置 ASCII 规则 + 自定义 Unicode 规则 |
| 原始代码区（§1.9） | pest 内置的 push/pop 栈：`PUSH("}")` 实现深度配对 |

pest 规则命名约定：`WHITESPACE` / `COMMENT` 为 pest 隐式规则；
语法规则采用 `snake_case`（如 `struct_def`、`graph_def`、`edge_decl`）；
每个语法规则带 `Span`，Parser 层（P2）负责把 pest Pair 树组装为强类型 AST（P1，见 `dhv/src/ast.rs`）。

**文法验收基准（32 组样例）**：BNF 的每个产生式组必须被 `dhv/tests/` 下的样例覆盖——
字面量/类型/模式/表达式优先级/控制流/graph/edge/project/scale/block/native/宏/属性/注释嵌套。

---

## 7. 规范附录：完整示例（本例通过全部静态语义校验）

```hsl
// models/types.hsl —— 契约层：纯类型，不投射
export struct Prompt { system: String, user: String }

export enum Action {
    CallTool { name: String, args: HashMap<String, String> },
    Respond(String),
    Stop,
}

export trait LLMProvider {
    async fn generate(prompt: Prompt) -> Result<String, Error>;
}

export trait ToolExecutor {
    fn execute(action: Action) -> Result<String, Error>;
}

// plugins/bash_tool.hsl —— 实现层
import { Action, ToolExecutor } from "../models/types.hsl";

#[capability(process_spawn)]
struct BashTool { timeout_ms: u64 }

impl ToolExecutor for BashTool {
    fn execute(action: Action) -> Result<String, Error> {
        let Action::CallTool { name, args } = action else {
            return Err(Error::invalid_action("expected CallTool"));
        };
        let out: String = native rust {
            std::process::Command::new(name)
                .args(args.iter().collect::<Vec<_>>())
                .output()?
                .stdout
                .iter().map(|b| *b as char).collect()
        };
        Ok(out)
    }
}

project { BashTool -> "src/tools/bash.rs" : rust }

// main.hsl —— 编排层（入口）
import { Prompt, Action, LLMProvider, ToolExecutor } from "./models/types.hsl";
import { BashTool } from "./plugins/bash_tool.hsl";

struct AgentState { history: Vec<String>, current_goal: String }
struct DeepSeekClient { model: String }

impl LLMProvider for DeepSeekClient {
    async fn generate(prompt: Prompt) -> Result<String, Error> {
        let truncated = truncate(prompt.user, 4000)?;
        let raw: String = native python {
            import openai
            client = openai.Client()
            response = client.chat.completions.create(
                model="deepseek-v4",
                messages=[{"role": "user", "content": truncated}]
            )
            response.choices[0].message.content
        };
        Ok(raw)
    }
}

graph MyAgent -> Result<(), Error> {
    let planner: Box<dyn LLMProvider> = DeepSeekClient::new();
    let tool: Box<dyn ToolExecutor> = BashTool::new();
    let mut state = AgentState { history: Vec::new(), current_goal: "run tests" };

    edge planner -> tool on Action::CallTool;

    loop {
        let action = planner.generate(state.clone()).await?;
        match action {
            Action::CallTool { .. } => {
                let result = tool.execute(action)?;
                state.history.push(result);
            },
            Action::Respond(text) => {
                println(text);
                break;
            },
            Action::Stop => break,
        }
    }
}

block agent_config {
    agent:
      name: MiniAgent
      version: 0.1
      max_retries: {{MAX_RETRIES}}
}

block agent_instructions {
    # Agent Instructions
    你是一个执行助手。当前目标：{{state.current_goal}}
}

scale = microkernel;

project {
    MyAgent           -> "src/main.rs"        : rust,
    DeepSeekClient    -> "src/deepseek.py"    : python,
    agent_config      -> "config/agent.yml"   : yaml,
    agent_instructions-> ".harness/AGENTS.md" : markdown,
}
```

（示例中对 `Action` 的 match 无 `_` 通配——符合 S6：graph AgentLoop 强制显式穷尽。）

---

## 8. 变更记录

| 版本 | 日期 | 变更 |
|:---|:---|:---|
| v1.5.0 | 2026-08 | **工程化扩展版**（dhv Rust 编译器与 dhv-ts 参考解释器双端对齐实现）。全部为**增量扩展 + 修正性澄清**，不破坏 v1.4 语法：

  1. **§3.4 投射规则组（rules）正式化**：`rules { kind -> "path/{name}.ext" : lang }` 按项类型批量投射，显式映射优先（R1）；占位符白名单（R2）、类型唯一（R3）、类型注册（R4）、展开池（R5）、诊断一致性（R6）；新增诊断系列 **P5**。
  2. **§2.11.7 无结构体字面量语境成文**：if/while 条件、if let/while let 的 `=` 右侧、match 待匹配对象、for 迭代对象中禁用结构体字面量（Rust 规则）。此前 PEG 的贪婪匹配使 `if x < lo { 1 }` 中 `lo { 1 }` 被解析为元组结构体字面量吞掉 if 块——dhv 实现已按本规则重构（ns_* 阶梯），并与 dhv-ts `parseExprNoStruct` 行为对齐；结构体字面量字段形态与 dhv-ts `looksLikeStructLiteral` 启发式对齐（移除裸整数字段）。
  3. **方法泛型 turbofish 正式化**：MethodCall ::= identifier ("::" GenericArgs)? "(" CallArgs? ")" —— `.collect::<Vec<String>>()` / `.parse::<f64>()` 为合法形态。
  4. **export / impl 前导属性正式化**：`OuterAttributes` 允许出现在 `export` 与 `impl` 之前（`#[derive(..)] export struct X`、`#[cfg(lang: rust)] impl Trait for Type`）；导出项属性归并到内部项。
  5. **§1.9 词法澄清（native 原子性）**：pest 的全局 `COMMENT` 规则同样参与隐式空白注入，非原子规则内字符串中的 `//`、`/*` 会被误判为注释（`base_url="https://api..."` 类代码崩坏）。native_string / native_text / native_body 必须为原子（`@`/`$`）规则；实现者注意。
  6. **词法形状前瞻澄清**：`block` / `static` 关键字仅在 `block NAME {` / `static NAME {` 形态进入原始资源区模式；`rules { block -> ... }` 中的规则类型不触发（dhv-ts lexer 形状前瞻）。
  7. **模块链接器（dhv）**：dhv check 引入最小链接器（BFS + 环检测），import 依赖模块的导出 enum / 静态资源进入跨模块注册表——S6 穷尽性校验与 P4 静态资源判定跨模块可见；新增诊断 **M2**（模块加载失败）。
  8. **S6 触发条件澄清**：AgentLoop 内 `_` 通配兜底仅对**已注册用户枚举**的 match 禁止；Option/Result/字符串字面量匹配的 `_` 兜底合法（与 dhv-ts enumArms 门控一致）。
  9. **check ≠ emit**：check 命令不驱动代码生成（dhv-ts 既有行为成文）；codegen 能力缺口不阻塞校验。
  10. **已知限制（诚实边界）**：值语境 range（`let r = a..b;` 作为一等值）由 **dhv** 完整实现（v0.2.12）；dhv-ts 当前支持 for-in 与切片位置的 range（含 `..=` 闭区间，v1.5 修复单 token 词法解析），值语境 range 作为绑定值暂不解析——回归用例见 dhv tests/conformance.rs 内嵌用例。|
| v1.0 | 2025-06 | 首个正式版：全量 BNF + 静态语义 + pest 映射约定。由总纲文档直接推导，P0 pest 文法以本文件为准对齐重写。 |
| v1.2 | 2025-08 | 词法歧义消解规则（L1-L5）与原始代码区词法模式（§1.9 模式 A/B）成文；文法验收基准明确为 32 组样例。 |
| v1.3 | 2025-08 | **参考实现驱动修订**（dhv-ts 解释器实现 NOVA/DSH 两个真实项目过程中发现的规范缺口，全部为澄清性/修正性变更，不破坏 v1.2 语法）：

  1. **`from` 降级为上下文关键字**（§1.4 修正）：`from` 作为严格关键字与 `String::from(...)` 及 `From` trait 生态直接冲突（NOVA 代码已在使用）。v1.3 起 `from` 仅在 `import ... from "..."` 子句位置具有语法含义，其余位置为普通标识符。
  2. **graph 调用约定正式化**（§3.1 语义注记）：graph 的调用形式为 `GraphName::run(args)`（或入口文件内裸 `GraphName(args)`），与 P7「graph 投射为目标语言入口函数」对齐。
  3. **闭包参数模式禁用顶层 or-**（§2.11.5 注记）：`|x|` 的收尾 `|` 与 or-模式分隔符产生不可消解歧义。闭包参数模式不得包含顶层 `|`；需要 or-模式时加括号 `|(a | b)|`。（与 Rust 一致。）
  4. **带块表达式语句分号规则澄清**（§2.12 注记）：`ExpressionWithBlock` 单独作为语句时**可省略分号**（`ExpressionStatement ::= Expression ";" | ExpressionWithBlock`）；块尾无分号的任意表达式为块的值。
  5. **模式 A（资源块）字符串定界规则明确**（§1.9 模式 A 注记）：资源块体内**只有双引号字符串**参与大括号跳过计数；单引号是 Markdown 文本中的撇号（如 `user's`），不是定界符。
  6. **枚举变体结构模式的变体名校验**（§5.1 语义澄清）：`Enum::Variant { fields }` 模式匹配必须同时校验枚举名与**变体名**——仅按字段形状匹配是错误的（不同变体可能共享同名字段）。dhv-ts 参考实现曾在此出错（`ToolCall::ReadFile { path }` 误匹配了 `EditFile` 值），已修复并回归。
  7. **G6 运行期观测语义**（§5.3 注记）：microkernel 尺度下，AgentLoop 内 match 所选分支若与某条 `edge ... on Variant` 的变体一致，运行时向事件总线发射 `edge {from, to, on}` 事件；monolith 尺度下等价为直接调用轨迹。语义等价性由 codegen/运行时共同保证。
  8. **R-1 运行入口约定**：`dhv run <entry.hsl>` 调用入口文件中名为 `main` 的 `fn main() -> ...`（入口文件内可见即可，无需 export）。
  9. **附录 A（std 预导入库方法面）与附录 B（native 运行时 ABI）** 成文（见下）。
| v1.4 | 2025-08 | **开源发布版**：38 后端注册表、标准库 10 模块、双向工程围栏协议、运行期修复。变更全部为**增量扩展 + 修正性修复**，不破坏 v1.3 语法：

  1. **§5.2.1 后端语言注册表成文**：32 编程语言（4 tier）+ 6 静态格式 = 38 后端；能力分级 full/logic/contract/raw 与诚实边界（写入 manifest）。`dhv-ts targets` / `dhv targets` 打印注册表。原 P-4 仅允许 3 编程语言的规定废止。
  2. **附录 C：std 标准库（10 模块）成文**：`import { f } from "std/<mod>";` 语法正式化 —— std/core、std/collections、std/text、std/math、std/io、std/json、std/time、std/random、std/env、std/iter（共约 60 函数 + 2 常量）。虚拟模块解析（不触文件系统）；std/io 走宿主路径监狱并以 `Result` 返回；std/random 为可复现 PRNG（mulberry32，默认种子 42，`seed(n)` 重置）。
  3. **双向工程围栏协议正式化**：三标记协议 `@dhv:source-map: <module>:<line>, block: <name> [(live)]` / `@dhv:hsl-mirror` / `@dhv:end-source-map`。live 围栏 = [活体翻译区（内核，重编译覆盖）] + [HSL 源镜像（可编辑，`dhv sync` 回写依据）]；contract 围栏仅含镜像。回写按 block 名定位，回写后重新解析校验，失败回滚。
  4. **运行期修复（? 的 From 转换接线）**：v1.3 中 `?` 的 `impl From<E1> for E2` 转换因类型解析器丢弃泛型实参而从未生效（规范 §5.9 声明的语义与实现不符）。v1.4 修复：类型路径解析保留末段泛型实参，`From` 特化注册表生效，`?` 传播真实执行转换（回归用例见 tests/hsl）。
  5. **宿主运行时陷阱记录（工程注记）**：bun 转译器会静默丢弃**语句位置**的 `declare(...)` 调用（TypeScript ambient 关键字冲突）——曾导致检查器 S-7/S-8 规则从未真正执行。任何以 bun 为宿主的 HSL 工具链实现禁止在语句位置调用名为 `declare` 的函数。此类宿主陷阱是「编译器自举前必须跑真实测试套件」的直接论据。
  6. **S 规则运行期修正**：S-7 使用标记沿作用域祖先链向上传播（子块中的使用对父块声明可见）；S-7 对宏实参下探（println!/format! 内的使用不再误报）；S-7 对 native 块按名词法捕获语义标记；S-8 豁免 `_` 通配重复绑定（Rust 语义）。
  7. **CLI 扩面**：`check` / `run` / `emit` / `targets` / `sync` / `watch` 六命令（emit 支持 38 后端真实代码生成 + 交叉语法校验 python3/bun/bash -n + manifest.json；watch 为总纲 §6 File Watcher 的实现）。
| v1.4.1 | 2025-08 | **语义修正轮**（测试驱动发现，4 项修正 + 1 项扩面）：

  1. **S-6 通配语义修正（§5.1）**：`_` arm 在普通函数内视为穷尽覆盖（与 Rust 一致，v1.4 前的实现比规范更严）；**graph AgentLoop 内仍拒绝 `_` 兜底**（铁律不变——Agent 核心决策循环必须显式直面每个新变体）。
  2. **构造器补全**：`String::new()` / `HashSet::new()` / `Vec::with_capacity()` / `String::with_capacity()` / `HashMap::with_capacity()` 运行期可用（与 `Vec::new` / `HashMap::new` 对齐）。
  3. **std 方法面扩充（附录 A/C）**：Vec（clear/is_sorted/sort_desc）、HashMap（clear）、String（char_count）、Option（expect 生成端映射 + 预置助手 `_dhv_expect`/`_dhvExpect`）。活体翻译器方法映射表新增 12 项（pop/clear/sort/is_sorted/remove/char_count/split_whitespace/lines/repeat/expect/and_then），其中 position/find 因 Option 返回语义在生成端不映射（诚实回退 contract）。
  4. **测试套件扩至 43 用例**（tests/hsl/run-all.ts）：新增 translator-tour 巡览 / 构造器回归 / S-6 正反边界 3 例 / 新方法映射生成代码语义级验证（python exec 实测断言）。
  5. 教程（HSL-GUIDE.md）同步更新 S-6 与 String::new 相关章节/FAQ/已知限制清单。
| v1.4.2 | 2025-08 | **模式匹配扩面轮**（活体翻译器能力增强，6 项变更，无语法破坏）：

  1. **if-let / while-let 模式扩展（§2.11.6 / §2.11.8）**：原活体翻译器仅支持 `Option::Some(x)` 单一模式，本轮扩展至：(a) 枚举 tuple 变体 `Enum::V(a, b)`、(b) 枚举 struct 变体 `Enum::V { x, y }`、(c) 无负载变体 `Enum::Unit`、(d) `Result::Ok(v)` / `Result::Err(e)`、(e) binding 与 `_` 通配（恒真条件 + 单纯绑定）。Rust 后端走原生 `if let` / `while let` 语法；python/ts/js 后端合成 `isinstance` / `.kind ===` 比较 + 临时变量绑定。
  2. **Some(x) / Ok(x) / None / Err(e) 单段简写归一化**：用户可写 `if let Some(x) = ...` 或 `if let Option::Some(x) = ...`，二者编译期归一为同一 AST（Pattern 归一化 helper `normalizePattern`），下游 armInfo / rustPattern 无需重复特化。
  3. **Scrutinee 表达式 hoist（避免副作用多次求值）**：if-let / while-let 中若 scrutinee 表达式含运算符或方法调用（如 `cur.pop()` / `results.get(i)`），活体翻译器先缓存到 `_scrut_N` / `_wl_N` 临时变量，再用于条件检查与字段绑定，避免 `next()` 等副作用表达式在 cond 检查与字段绑定中被多次求值。
  4. **解析器修正（§2.11.2）：块表达式不能作为二元 LHS**：原解析器的所有二元层（parseOr/parseAnd/parseBitOr/parseBitXor/parseBitAnd/parseEquality/parseRelational/parseShift/parseAdditive/parseMultiplicative/parseCast/parseAssign）会在拿到块表达式（if/match/while/loop/for/whilelet/block/iflet/native）LHS 后继续消费运算符，导致 `while let Some(x) = next() { ... } -1` 被错误解析为 `binary(whilelet, '-', 1)`。本轮在所有二元层入口加 `if (exprIsWithBlock(lhs)) return lhs;` 守卫，与 Rust 语义一致（块表达式不能作二元运算的 LHS）。
  5. **std 方法面：get(i) 在生成端映射为 subscript**（pre-existing，本轮通过 while-let 副作用测试覆盖）；position/find 仍诚实回退 contract（Option 语义在生成端无法廉价复现）。
  6. **测试套件扩至 49 用例**（tests/hsl/run-all.ts）：新增 pattern-tour run / pattern-tour check+emit / if-let 枚举变体语义级验证（python exec）/ while-let 副作用 scrutinee 验证 / 单段简写归一化等价验证 / 解析器块-尾表达式分离回归（共 6 例）。所有生成代码均经 python3 真实 exec 实测断言。
| v1.4.3 | 2025-08 | **值语义完备 + 类型感知映射轮**（活体翻译器 3 处 bug 修复 + 扩面，无语法破坏）：

  1. **if-let 尾位置值语义（§2.11.6）**：`if let PAT = EXPR { A } else { B }` 在函数尾位置现为值语义（分支产出 `return`，与 match/if 对齐）；无 else 的 if-let 在值语境诚实回退 contract（Rust 语义：无 else 的 if let 表达式类型为 `()`）。同时修复：值语义块（match 臂 / if 分支）尾部的嵌套 if/match 此前被降级为语句导致**静默丢失 return**（生成代码返回 None 而非分支值——比回退更危险的错误类）。
  2. **else if let / else if 链**：`if let ... {} else if let ... {} else {}` 现可活体翻译（python `elif` 改写 / brace 语言 `} else if` 改写 / Rust 原生）；此前 else 分支为 if/iflet 表达式时直接回退 contract。
  3. **Vec::get / HashMap::get 生成端 Option 语义（闭合 v1.4.2 #5）**：python 映射为 `_dhv_get(c, k)` 助手（try/except 越界/缺键 → None）；ts/js 按静态类型感知分发（Vec → `v[i] ?? null`，HashMap → `m.get(k) ?? null`，未知 → `instanceof Map` 多态）；rust 用原生 `.get()`。生成端与解释器语义对齐：`v.get(0)` 越界返回 None 而非 panic。
  4. **类型感知同名方法分发**：翻译器类型环境扩展容器/包装类 kind（vec/map/option/result），`map` 按接收者分发为 Vec::map（列表推导）或 Option::map（None 短路）——此前 Option::map 会**静默生成 Vec 风格代码**（对 Some([1,2]) 迭代负载——错误类：静默语义偏移）。
  5. **方法映射表 +20 项**（46→66）：数值 pow/sqrt/floor/ceil/round/clamp（python round 用 `int((x+0.5)//1)` 避免 banker's rounding 偏差）；Vec any/all（python 生成器表达式）/fold（参数序适配：HSL `fold(init, f)` → python `reduce(f, v, init)`）/for_each（ts/js/rust）/extend/iter/collect/cloned（rust 链拼块）；String as_str/trim_start/trim_end/char_at（python 切片 `[i:i+1]` 天然 OOB→'' 对齐 interp）/is_alphabetic/is_numeric；Option or/unwrap_or_else；Vec sort_by（key 语义：python `sort(key=f)` / rust `sort_by_key`——interp 实现即按 f(x) 数值装饰排序）/sort_desc。first/last 的 ts 映射升级为 `(v.length > 0 ? v[0] : null)`（Option 精确）。
  6. **contract 后端声明质量（java/kotlin/swift）**：kotlin/swift/scala 函数参数改为类型后置冒号语法（`fun f(x: T)` / `func f(x: T)`，此前误用 C 风格 `f(T x)` 生成非法 Kotlin/Swift）；Java 全部顶层项包进 `public class <Module> {}`（顶层函数/常量在 Java 非法）。
  7. **macro_rules! 定义名尾 `!` 容错（§2.13）**：`macro_rules! name! { ... }` 与 `macro_rules! name { ... }` 等价（Rust 习惯迁移）；parser 与 token 级展开器（macro.ts）同步容错。
  8. **测试套件扩至 57 用例**：新增 if-let 尾值语义（含 OOB → None）/ match 臂嵌套 if 值语义回归（防静默丢失 return）/ else-if-let 链 / 新方法映射 6 函数语义级验证（python exec 12 断言）/ Option::map 类型感知分发 / java-kotlin-swift 声明质量断言 / ts get 类型感知 / macro 尾 ! 双形态展开（共 8 例）。


| v1.4.4 | 2025-08 | **跨文件类型依赖 + 单次求值轮**（物理层依赖解析 + 双 bug 修复 + 扩面，无语法破坏）：

  1. **跨文件类型依赖解析（总纲 §4 物理层）**：emit 现在追踪每个投射项引用的用户类型（类型路径根 + 多段路径表达式 + 模式首段 + 宏 token 树），当类型被投射到同语言的另一物理文件时按目标语言导入机制自动接线：python 同目录平铺 `from <stem> import A, B`；typescript/javascript 相对路径 `import { A } from './<rel>';`；rust 模块组装约定 `use crate::<目录链>::{A, B};`；go 同包免导入（全部产物 `package hsl`）；**cpp 内联类型声明**（ODR 兼容：与被投射文件中的定义逐字一致，多 TU 安全；类型未投射到 cpp 时从全程序 AST 兜底内联）。contract 级语言不接线（围栏纪律）。诚实告警协议：X-1 类型未投射（生成物引用未定义名）/ X-2 python 跨目录导入需手动接线 / X-3 rust 非法模块路径 / X-4 go 跨目录跨包。告警使 emit 退出码 1（与 P 系列同权）。
  2. **修复：元组下标访问生成端非法语法**：`t.0` / `t.1` 在 python/ts/js/go 生成端此前输出 `t.0`（**SyntaxError**——py_compile 校验从未覆盖此形态）；现除 rust（原生 `t.0`）外一律映射为下标 `t[0]`，与解释器 readField（元组=数组）语义对齐。
  3. **修复：副作用接收者双重求值（方法映射表系统性缺陷）**：`m.remove(k).unwrap_or(d)` 此前生成 `m.pop(k, None) if m.pop(k, None) is not None else d`（pop 调用**两次**——键被删两次、第二次得 None → TypeError/静默语义漂移）。修复：Option 组合子家族（unwrap_or / unwrap_or_else / and_then / or / pop / clone / is_sorted / strip_prefix / strip_suffix / find）统一走 prelude 助手函数（参数求值恰好一次）；ts/js 同构修复 and_then / unwrap_or_else / pop / clone。这与 v1.4.2 #3 的 scrutinee hoist 是同一类问题在方法映射表中的复发，本轮系统性闭合。
  4. **方法映射表 +9 项（66→75）**：String strip_prefix / strip_suffix / find（Option 语义三元/助手映射——v1.4.2 曾因"Option 无法廉价复现"不映射，助手模式使低成本正确映射成为可能）；Vec position（Option 语义：python `next(genexp, None)`）/ enumerate（python `list(enumerate(v))` / ts `map((x, i) => [i, x])`，元组下标访问对齐）；Option cloned。Vec::insert / Vec::remove 与 HashMap::insert / HashMap::remove 同名不同义，按接收者类型感知分发（Vec insert 按下标插入 / Vec remove 返回被删元素；Map remove 返回 Option 旧值——ts/js 新增 `_dhvRemove` 助手，python `pop(k, None)` 天然对齐，go 匿名函数立即调用）。
  5. **显式类型 let（i64 语义保真）**：rust `let n: i64 = 0;` / cpp `int64_t n = 0;` / go `var n int64 = 0`——有类型注解时照实投射（此前 rust 推断 i32 / cpp `auto` 推断 int，大值场景静默截断）。
  6. **cpp 后端首次达到真实编译级验证**：backends-demo 全 6 个 cpp 文件 g++ -std=c++23 编译通过 + 链接可执行程序输出与解释器逐字一致（`stop|call grep with 2 args|24|1 / 2 工具调用成功`）；pattern-tour 4 个 cpp 文件（含内联 Shape variant 结构）全部编译通过。测试套件在 g++ 可用环境自动执行编译级断言（无 g++ 时跳过不失败）。
  7. **dhv Rust 源码一致性修复（2 处）**：hsl.pest 宏定义接受可选尾 `!`（v1.4.2 容错同步：`macro_rules! name! { ... }`）；typecheck.rs S-6 实现 AgentLoop 外 `_` 通配 = 穷尽覆盖（v1.4.1 修正同步）。
  8. **测试套件扩至 66 用例**：新增 v1.4.4 方法映射语义级验证（python exec 12 断言）/ 元组下标修复（py/ts/rs 三语言）/ 跨文件类型依赖接线断言（py from-import + ts import + rs use + cpp 内联）/ X-1 告警回归 / cpp g++ 编译级验证 ×2 / ts Map::remove Option 语义（bun exec）/ 副作用单次求值（drain 双重求值探针）/ dhv Rust 源码一致性守护（共 9 例）。
| v1.4.5 | 2025-08 | **cpp/go 模式活体化 + Java 结构合法化 + dhv 语法补全轮**（4 个高危 bug 修复，无语法破坏）：

  1. **修复：dhv Rust `hsl.pest` 引用未定义规则（编译失败级）**：`expression = { expression_with_block | assignment_expression }` 引用的 `expression_with_block` 此前**从未定义** —— pest_derive 无法编译（源码全树扫描确认仅此一处）。现补全定义：`expression_with_block = { block_primary ~ postfix_op* }`，block_primary 覆盖全部 10 类含块表达式（block/async/native/if/iflet/match/loop/while/whilelet/for）。**语义同时达成 v1.4.2 #4 对齐**：PEG 有序选择使块表达式块尾自终止 —— `while let Some(x) = e { } - 1` 的 `- 1` 不再被吞进二元链（与 dhv-ts 二元层 `exprIsWithBlock` 守卫同源）；后缀链（.field/.method()/?/.await/[i]）仍允许跟块后。parser.rs Pair 树契约同步（block_primary + apply_postfix 循环）。已知分歧（记录）：带括号块-LHS 二元 `(if c {1} else {2}) + 3` —— dhv-ts 拒绝（守卫不区分括号），pest 接受（实现比规格多，符合"只能多不能少"）。
  2. **if-let / while-let 活体翻译扩展至 cpp/go**（v1.4.2 #1 的 5 语言 → 7 语言）：cpp 走 `std::holds_alternative<V>(s)` + `auto& _v = std::get<V>(s)`（与 matchDispatch 同构）；go 走类型断言 init-statement `if _ifv_N, _ok_M := s.(V); _ok_M {`（无绑定时 blank 标识符防 go unused 编译错误）。while-let：cpp `while (true) { if (!(cond)) break; … }` / go `for { … }`——scrutinee 需 hoist 时在循环体内每迭代求值一次（Rust 语义同源）。模式覆盖：用户 enum tuple/struct/无负载变体 + Option::Some/None + binding + 通配；**Result::Ok/Err 对 cpp/go 诚实回退 contract**（类型映射无变体通道：cpp Result→%T 裸值 / go→(T, error) 多返回值——宁缺毋滥纪律）。
  3. **修复 4 个生成端非法代码 bug**（if-let 活体化暴露的预存缺陷，此前被 contract 回退掩盖）：① cpp/go Option 条件 `v != null`（cpp 无 null / go 用 nil）→ `has_value()` / `!= nil`；② cpp/go Option 绑定 `const x = v`（双非法）→ `auto x = *v;` / `x := *v`（解引用）；③ go 变体字段大小写错位（binds 生成 `s.f0` 但 decls 声明 `F0` → 编译失败）→ armInfo go 分支 capitalize；④ 裸 `None` 值所有语言输出字面 `None`（cpp/go/ts/js 全非法）→ py None / ts-js null / rust None / cpp std::nullopt / go nil。
  4. **Some 构造映射（cpp/go）**：cpp 模板助手 `template<typename T> std::optional<T> _dhvSome(const T&)`（类型推导 + include guard 兜底单 TU 拼接）；go 泛型助手 `func _dhvSome[T any](v T) *T`（go 1.18+）。附带修复：cpp `String::to_string()` 字符串接收者此前生成 `std::to_string("x")`（非法——只接受数值）→ `std::string(recv)`。
  5. **Java contract 生成物结构合法化**（重构）：旧版全项嵌进 `public class <模块名>` 含两个非法点 —— ① public 类名必须匹配文件名（`ToolResult.java` 内 `public class Model` = javac 报错）；② 同模块多文件共享 wrapper 名（同包重名冲突）。新结构：类型项（record/sealed interface/interface）**顶层声明**（同包裸名互见 → 跨文件引用无需限定名）；仅 fn/const/impl 需宿主 `class Dhv<文件stem>`（每文件唯一 + 前缀防碰撞 + package-private 免文件名约束）。Java 同步加入 X-1 告警（被引用未投射类型）。
  6. **M3 静态化（check 阶段 import 校验）**：`import { X } from "..."` 中 X 未被源模块 export 此前只在 run/emit 期报错（check 全绿的盲区——nova 项目实录：8 个定义缺 export 导致 run/emit 双失败）。现 checkProgram 新增静态 M3：import 名未被源模块 export 即报 `error[M3]`（标准库虚拟模块豁免）。
  7. **cpp 后端编译级验证扩面**：pattern-tour describe/classify/count_down（新活体能力）g++ 编译 + 链接运行语义与解释器对齐（`describe(Circle 3)="circle r=3"` / `classify(Point)="point (1,2)"` / `count_down(4)=10` / `count_down(None)=0`）；Some/None 构造语义级（make(5)→Some(10) / use_opt(Some(42))=43 / use_opt(None)=-1）。
  8. **测试套件扩至 74 用例**：新增 cpp Some/None g++ 编译+链接+运行 / cpp-go if-let 变体链+while-let 循环结构断言+python exec / cpp Option match has_value / cpp to_string 字符串接收者 / go 变体字段大小写 / Java 顶层类型+Dhv 宿主结构 / M3 静态检查正反 / nova emit 回归（共 8 例）；pattern-tour 扩容 count_down 函数 + classify→go + count_down→5 语言（24→30 文件）。
| v1.4.6 | 2025-08 | **cpp/go Vec 方法扩面 + matchDispatch 副作用 hoist + C# 宿主类合法化轮**（2 个真实 bug 修复 + 5 项扩面，无语法破坏）：

  1. **修复：matchDispatch 副作用 scrutinee 未 hoist（match v.pop() 多次求值）**：match 分发链（python/cpp if-elif 路径）每臂 cond 与 binds 都引用 scrut；`match v.pop() { Some(x) => ..., None => ... }` 此前生成 `_dhvPop(v).has_value() { auto x = *_dhvPop(v); ... }`（pop 调用 **2 次**——第二次得 None 或不同值，破坏副作用语义）。修复：matchDispatch 入口对 python/cpp 路径 hoist 非标识符 scrut 到 `_m_N`（与 while-let hoistScrut 同源，Task 17 引入；match 路径本轮补齐）。rust/ts/js/go 路径仅在 match/switch 头求值一次（原生语义保证），无需 hoist。
  2. **修复：validate balanceCheck 误判 `(*ptr)` 解引用为注释**：balanceCheck 之前对所有语言把 `(*` 视作块注释起始（OCaml/FSharp 方言）—— go/cpp 生成代码的 `(*v)[n]`（解引用 + 下标）被误报 "(* ... *) 注释未闭合"。修复：仅 ocaml/fsharp/pascal 方言识别 `(*` 为注释；go/cpp/ts/js/python 等的 `(*ptr)` 正确视为解引用表达式。pop/first/last 等 cpp/go 助手函数（含 `(*v)[n]`）此前会被误判语法失败，本轮闭合。
  3. **Vec::pop / Vec::first / Vec::last / clone 活体映射扩至 cpp/go**（v1.4.5 #2 的 5 方法 → 4 方法 × 7 语言）：cpp 模板助手 `_dhvPop<T>(std::vector<T>&) → std::optional<T>` / `_dhvFirst/_dhvLast`（include guard 兜底单 TU 拼接）；go 泛型助手 `_dhvPop[T any](*[]T) → *T` / `_dhvFirst/_dhvLast`（指针传递副作用通道，go 1.18+）。clone 为 cpp 拷贝构造 / go slice header 拷贝（共享 backing array，与 interp 浅拷贝语义对齐）。`while let Some(x) = v.pop() { ... }` drain 场景（Task 19 移交建议 #2）从 contract 回退升级为活体翻译。
  4. **Option 方法族扩至 cpp/go**：unwrap_or（cpp `std::optional::value_or` / go `(recv != nil ? *recv : d)`）/ unwrap（cpp `*recv` / go `*recv`）/ is_some（cpp `recv.has_value()` / go `recv != nil`）/ is_none（cpp `!recv.has_value()` / go `recv == nil`）。链式 `v.pop().unwrap_or(d)` 在 cpp/go 现可活体翻译（此前因 unwrap_or 缺失而回退 contract）。
  5. **C# contract 生成物结构合法化**（v1.4.5 #5 Java 的同构修复）：旧版 C# 把 fn/const 投射为顶层 `public static T F(...)` / `internal const T K = ...` —— C# 顶层函数/常量非法（必须属于 class）。新结构：类型项（struct/enum/trait/typealias，`internal record`/`abstract record`/`interface`/`using`）**顶层声明**（同命名空间裸名互见，跨文件引用无需限定名）；仅 fn/const 包装进 `internal static class Dhv<文件stem>`（与 Java `class Dhv<Stem>` 同构；static class：所有成员必须 static，与 fn/const 投射形态一致；防实例化；每文件唯一防重名冲突）。
  6. **跨文件类型告警 X-1 扩展至 C#**：Java v1.4.5 #5 的 warnJavaTypeRefs 重构为 warnTopLevelTypeRefs，覆盖 java/csharp。被引用未投射到 C# 的类型诚实报 X-1（与 Java 同纪律）。
  7. **pattern-tour drain → cpp/go 扩面**：v1.4.5 新增的 drain 函数（Vec::pop while-let 累加）此前未投至 cpp/go（因 pop 缺映射）；本轮扩面后 pattern-tour 30 → 32 文件。
  8. **测试套件扩至 82 用例**：新增 cpp Vec::pop g++ 编译+链接+运行（drain=15/0/7）/ cpp-go first/last/clone + 结构断言 + g++ 编译级 / matchDispatch 副作用 hoist（match v.pop() 单次求值，cpp+python exec）/ cpp pop 副作用对接收者可见（pop+peek=302/-100）/ balanceCheck (*ptr) 解引用不误判（go/cpp/ocaml 三方言）/ C# 宿主类合法化（internal static class Dhv<Stem>）/ Kotlin-Swift contract 结构断言（sealed class/enum case）/ 宏 token 树嵌套 delim 类型收集（vec![Tool{...}] 跨文件接线 + 零 X-1）（共 8 例）。
  9. **dhv Rust codegen 一致性 review**（Task 19 移交⑥）：dhv/src/codegen/{mod.rs,contract.rs,static_res.rs,python.rs,rust_backend.rs,typescript.rs} 与 langs.rs 38 后端注册表逐项核对一致 —— mod.rs 注册 6 命名后端（rust/python/typescript/yaml/markdown/json）+ 循环 LANGS 注册其余 32 为 ContractBackend（共 38）；ContractBackend emit_item 处理 struct/enum/trait/fn/graph 签名契约（与 dhv-ts decls.ts 同构）；static_res 三个命名后端（yaml/markdown/json）覆盖静态资源，toml/ini/xml 经默认 emit_static_resource 实现（原文 + 编译期插值）。
| v1.4.7 | 2025-08 | **String::contains 类型感知修复 + cpp/go Vec/HashMap/String 全表面 + let 块初始化轮**（1 个编译错误级 bug 修复 + 1 个全语言能力缺口闭合 + 4 项扩面，无语法破坏）：

  1. **修复：String::contains 在 cpp/go 生成编译错误代码（类型感知分发缺口）**：`s.contains("x")` 此前 cpp 走 Vec 表生成 `std::find(s.begin(), s.end(), "x")`（char 与 const char* 比较 = g++ 编译错误，已实测复现）、go 生成 `slices.Contains(s, "x")`（string 非切片 = 编译错误）—— 均通过启发式平衡校验但真机编译必炸。修复：contains 增加类型感知分发（kind === 'str' 时 cpp `(s.find(x) != std::string::npos)` / go `strings.Contains(s, x)`；Vec 路径保持不变）。与 v1.4.5 #3 to_string 字符串接收者同源的「同名方法跨类型错配」类缺陷。
  2. **修复：let 块初始化（let x = if/match/if-let）全语言能力缺口**：`let base = if b { 100 } else { 0 };` 在 interp 一直可用，但生成端全部 7 语言回退 contract（expr() 遇块表达式直接 throw）。修复：新增「声明 + 分支尾赋值」模式 —— python 免声明（分支内首次赋值）；ts/js `let x;`；rust `let x;`（延迟初始化合法）；cpp/go 需显式类型（注解在场照实投射，否则按分支尾 kind 推导基元类型 int64_t/double/std::string/bool —— 宽松策略取首个可推导分支钉住类型，iflet 的 then 分支常引用模式绑定 kind unknown 但 else 字面量可推导）。值语义分支必须齐全（无 else 的 if/if-let 类型为 ()，诚实 throw 回退）。asValue 机制从 boolean 扩展为 `boolean | string`（true → return；string → 分支尾赋值；false → 表达式语句），ifChain/ifLet/matchDispatch/armBlock/blockIntoValue 全链路贯通。
  3. **Vec::insert / Vec::remove 活体映射扩至 cpp/go**（v1.4.6 Task 21 移交①）：cpp `_dhvInsert`（越界 clean throw，Rust 同语义）/ `_dhvRemoveAt`（返回被删元素 —— std::vector::erase 返回 iterator 非元素）；go `_dhvInsert(&v, i, x)`（三语句 append+copy+赋值不可内联）/ `_dhvRemoveAt(&v, i)`（append 切片拼接；越界 panic 与 Rust 同语义）。
  4. **HashMap 全表面活体映射扩至 cpp/go**（Task 21 移交②）：insert（cpp `m[k] = v`）/ contains_key（cpp `m.find(k) != m.end()`，map/unordered_map 通用）/ keys/values（cpp 模板 `_dhvKeys/_dhvValues`（M::key_type/mapped_type 推导）；go 泛型 `_dhvKeys/_dhvValues`）/ remove（Option 旧值：cpp `_dhvMapRemove` 模板助手；go 从匿名函数 `func() any` **升级**为 `_dhvMapRemove` 泛型助手 —— 旧版返回 any，链式 `.unwrap_or(d)` 解引用 any 是编译错误；新返回 *V 与 Option 指针表示一致）。
  5. **Vec::get / HashMap::get Option 语义扩至 cpp/go（关闭 v1.4.3 遗留「下标近似」）**：kind 感知分发 —— vec → `_dhvVecGet`（越界 nullopt/nil）；map → `_dhvMapGet`（缺键 nullopt/nil）；unknown kind → 维持下标近似（静态类型无运行时分发通道，围栏纪律保障）。此前 cpp 越界 UB / go 缺键零值，与 interp None 语义漂移。
  6. **String 方法族活体映射扩至 cpp（Task 21 移交⑥）**：trim/_dhvTrim（C++ 标准库无 trim）/ to_lowercase/to_uppercase/_dhvToLower/_dhvToUpper（std::transform）/ starts_with/ends_with（C++20 原生）/ replace/_dhvReplaceAll（std::string::replace 是下标式非查找式）/ split/_dhvSplit（py 语义：含空串分隔）/ split_whitespace/_dhvSplitWS / lines/_dhvSplit(s, "\n")（py 语义含尾空串）/ char_count/_dhvCharCount（UTF-8 前导字节计数 —— std::string::size 是字节数非码点数）/ repeat/_dhvRepeat / join/_dhvJoin（if constexpr 分发 string/数值元素）。
  7. **测试套件扩至 88 用例**（+6）：String::contains 类型感知（g++ 编译+运行 1 0 1 0）/ cpp Vec::insert+remove+HashMap 全表面（g++ 编译+链接+运行 ir=10/mo=107 与 interp 对齐）/ cpp-go get Option 语义（OOB/缺键 → 默认值 6 -1 -1 1 -7）/ let 块初始化（py/rs/ts/cpp 全活体 + python exec 语义级）/ cpp String 方法族（12 方法 g++ 编译+运行 hello hsl|3|3|3|9|3|a-b-c 与 interp 逐字对齐）/ go HashMap+Vec 助手族结构断言（含「不应再有 func() any」回归断言）。
  8. **translator-tour 扩容**：新增 vec_surgery（insert/remove/get 链）+ map_census（HashMap 全表面 + let 块初始化 if）函数 + main 巡览段（vec_surgery=109 / map_census=106 / let_block=110）。
| v1.4.8 | 2025-08 | **Option 链式家族 + cpp/go Vec 全方法 + 闭包翻译 + 数组字面量修复轮**（4 个编译错误级 bug 修复 + 1 个类型感知盲区闭合 + 2 项能力扩面，无语法破坏）：

  1. **修复：vec! 宏 / 数组字面量在 cpp 生成 lambda 捕获非法语法**（编译必炸的隐藏 bug）：`vec![1, 2]` 此前 cpp 生成 `[1, 2]`（C++ 中是 lambda 捕获表达式 `[1, 2]{...}`，非数组字面量）—— g++ 报 "expected identifier before numeric constant"。修复：cpp 用 CTAD `std::vector{1, 2}`（C++17 class template arg deduction 自动推导 `std::vector<int>`）；go 同步从 `[1, 2]`（固定数组 `[2]int`）升级为 `[]any{1, 2}` 切片字面量（与 Vec 切片头语义对齐）。
  2. **修复：cpp 闭包缺外层变量捕获导致编译错误**：`Option::Some(first).map(|x| x + last)` 此前 cpp 生成 `[](auto x) { return x + last; }` —— `last` 未捕获，g++ 报 "'last' is not captured"。修复：cpp 闭包用 `[&](auto x) { return ...; }`（按引用捕获所有外层变量，与 Rust 闭包默认行为一致）。
  3. **修复：cpp extend/append 临时变量迭代器不同源导致 `vector::_M_range_insert` length_error**：`v.extend(vec![5])` 此前 cpp 内联 `(std::vector{5}).begin(), (std::vector{5}).end()` —— 两个 `std::vector{5}` 临时对象不同源，迭代器非法范围 → 运行期 `std::length_error`。修复：cpp 用 `_dhvExtend(v, arg)` 模板助手（const ref 绑定临时，保证 begin/end 同源）。
  4. **修复：exprKind 不能识别 Option::Some/None、Result::Ok/Err、Vec::from、HashMap::new、vec! 宏**：`Option::Some(first).map(|x| x * 10)` 此前因 `Option::Some(first)` 的 kind 为 unknown → map 走 Vec 分支 → cpp/go 无映射 → 退化为 contract。修复：exprKind 新增 `case 'call'` 识别 `Option::Some/None`/`Result::Ok/Err`/`Vec::from`/`HashMap::new` 路径调用的返回 kind；`case 'macro'` 识别 `vec!` 宏返回 'vec' kind —— 后续方法分发可正确感知。
  5. **Option 链式家族扩至 cpp/go**（Task 21 移交建议①）：map / and_then / or / unwrap_or_else / expect 五方法。cpp 用模板助手 `_dhvOptMap`（`std::optional<decltype(f(*opt))>`）/ `_dhvOptAndThen`（lambda 须返回 std::optional）/ `_dhvOptOr`（同型选择）/ `_dhvOptUnwrapOrElse`（零参 lambda）/ `_dhvOptExpect`（throw std::runtime_error）；go 仅扩非闭包方法 `_dhvOptOr` / `_dhvOptExpect`（HSL 闭包无类型注解，go func literal 需显式类型 —— 诚实回退 contract；map/and_then/unwrap_or_else 暂不映射 go）。
  6. **Vec::sort/is_sorted/clear/extend/append 扩至 cpp/go**（Task 21 移交建议②）：cpp sort → `std::sort(v.begin(), v.end())` / is_sorted → `std::is_sorted(...)` / clear → `v.clear()` / extend+append → `_dhvExtend(v, arg)` 模板助手；go sort → `slices.Sort(v)` / is_sorted → `slices.IsSorted(v)` / clear → `v = nil` / extend+append → `v = append(v, (arg)...)`（与 interp spread 语义对齐）。
  7. **测试套件扩至 96 用例**（+8）：cpp Option::map/and_then 链式 g++ 编译+运行（10/8/-1）/ cpp Option::or/unwrap_or_else/expect g++ 运行（42/99/7）/ go Option or/expect 助手族结构 / cpp Vec::sort/is_sorted/clear/extend/append g++ 编译+运行（105/5/0）/ go Vec 助手族结构 / cpp vec! 宏 CTAD 修复 g++ 运行（60/7）/ cpp Option::or 链 + unwrap_or g++ 运行（42/-7）/ cpp 综合场景（Option 链 + Vec 方法族）g++ 运行（13/6）。

| v1.4.10 | 2025-08 | **真机工具链编译级验证轮**（安装 rustc 1.98 / go 1.27 / JDK 21 / kotlinc 2.4.10 实测全部生成代码；修复 7 个真机编译错误级 bug + dhv Rust 编译器首次真实构建闭合 25+ 编译错误，无语法破坏）：

  1. **修复：go 后端 `unwrap_or` 生成三元表达式（go 无此语法，编译必炸）**：`opt.unwrap_or(d)` 此前生成 `(recv != nil ? *recv : d)` —— go 不存在三元运算符，真机 `go build` 报 invalid character U+003F。修复：新增 `_dhvUnwrapOr[T any](opt *T, def T) T` 泛型助手（表达式位置合法 + 单次求值 + 副作用接收者安全）。
  2. **修复：go 后端同 package 多文件顶级助手重复声明**：emit 多个 go 文件时 `_dhvSome/_dhvPop/...` 等助手在每个文件重复声明（同 package 重定义编译错误）。修复：跨文件共享状态去重 —— 助手仅注入首个 go 文件，其余文件仅 import（`goHelpersState` 贯穿 emit 流程）。
  3. **修复：go 后端未使用 import（go 严格规则）**：盲目 import fmt/strings/slices/sort/strconv 五包在多数文件触发 imported and not used。修复：emit 尾部按正文实际引用裁剪 import（`trimGoImports` 文本后处理；全部未用时省略 import 块）。
  4. **修复：rust 后端 format! 双重格式化（invalid format string）**：此前把表达式实参直接嵌入 `{...}`（如 `{args.len()}` —— rust 内联捕获不支持方法调用）且同时传位置参数。修复：纯标识符 → 内联捕获 `{name}`（不重复传参）；表达式 → 位置 `{}` + 参数列表。
  5. **修复：rust 后端 HashMap 未导入（E0425）**：`HashMap<String, T>` 非 prelude 类型，此前无 use 声明真机必炸。修复：rust 文件头统一 `use std::collections::HashMap;`。
  6. **修复：go 后端 len() 类型不匹配（mismatched types int/int64）**：go 内建 len 返回 int 与 HSL i64 映射（int64）混算报错。修复：`len(x)` → `int64(len(x))` 统一归一到 HSL 语义类型。
  7. **修复：go 后端尾兜底 return 不可达（go vet unreachable）**：body 尾已是 return 时再补零值兜底触发 vet 警告。修复：尾 return 判定（`goBodyEndsWithReturn`）跳过兜底。
  8. **dhv Rust 编译器首次真实构建**（此前 16 轮仅源码级 review）：修复 hsl.pest 语法错误 ×3（`[0-7]` POSIX 风格 bracket class → `'0'..'7'`；`ASCII_HEXDIGIT` → `ASCII_HEX_DIGIT`；`macro_transcriber` 无界 token 流贪婪吞分隔符 → 限定 delim token tree）+ operator_or_punct 括号字符移除（token 树边界与 rustc 模型对齐）+ 25 个 Rust 编译错误（LineColLocation 匹配 / Pair move-after-borrow / generic_params peek 不消耗 / fn_body·struct_body·trait_item·impl_item·graph_stmt·call_args·tuple_tail·index_or_range·import_spec·item_or_projection·block_element 等 12 处 named 包裹层静默丢弃 bug）+ EOI 过滤 + literal compound-atomic + transcriber keyword 分支 + where/generic/fn_params/graph_params 尾逗号 + `dyn Trait` 单 bound + identifier_pattern 守卫（`Action::CallTool` 路径模式）+ typecheck or-pattern 分离 + export 解包投射 + const 插值编译期求值（N5）。`cargo build` 0 warning 0 error · `cargo test` 3/3 · `dhv parse/check/emit` 全链路首次真实工作（tests/main.hsl 16 项 + 1 graph + 2 静态资源 + 4 投射 + scale）。
  9. **测试套件扩至 109 用例**（+4，全部真机编译级）：rust format 内联捕获修复（rustc 编译 + 内联/位置双形态断言）/ rust HashMap 导入（rustc 编译）/ go 多文件助手去重 + import 裁剪（go build + go vet）/ javac 编译级（java 后端首次真机编译）。另将 go HashMap 助手族 / if-let 变体链两测试升级真机 go build。

| v1.4.9 | 2025-08 | **parse turbofish 接线 + Option::filter + sort_by cpp/go + 裸 None 链修复轮**（1 个 interp 语义修复 + 2 个编译错误级 bug 类修复 + 4 项能力扩面，无语法破坏）：

  1. **修复：interp String::parse::<f64> 空串返回 Ok(0) 的 JS 语言怪癖泄漏**：`"".parse::<f64>()` 此前经 JS `Number("") === 0` 隐式返回 `Ok(0)`（与整数路径 `"".parse::<i64>() → Err` 不一致）。修复：空串统一 `Err`（与 Rust 语义一致）；整数/浮点两路径行为闭合。
  2. **修复（一整类）：裸 `Option::None`（无注解 let 中转）链式方法在 cpp/go 生成非法代码**：`Option::None.map(f)` 此前 cpp 生成 `_dhvOptMap(std::nullopt, f)`（`std::nullopt_t` 模板推导失败，编译必炸）、`Option::None.unwrap_or(0)` 生成 `std::nullopt.value_or(0)`（nullopt_t 无成员）、go 生成 `*nil`（untyped nil 解引用非法）—— 均通过启发式平衡校验但真机必炸（v1.4.8 测试全用带注解 let `a: Option<i64> = Option::None` 故未暴露）。修复：None 字面量接收者专门派发 —— 语义化简（单次求值精确）：`None.unwrap_or(d) ≡ d`、`None.or(alt) ≡ alt`、`None.is_some/is_ok ≡ false`、`None.is_none/is_err ≡ true`；cpp 的 map/and_then/filter/unwrap_or_else 用 `_dhvNoneT` 链式包装器（成员恒等化简，支撑后续 `.value_or` 链 + `operator std::optional<T>` 隐式转换）；unwrap/expect（interp 运行期即中止）与 go 闭包族（untyped nil 无类型通道）诚实回退 contract。
  3. **修复：exprKind 两段 builtin 路径值返回 unknown**：`Option::None.filter(f)` 此前因 kind 为 unknown → filter 走 Vec 分支 → 回退 contract。修复：`case 'path'` 识别两段 `Option::*` → 'option' / `Result::*` → 'result'（与 v1.4.8 case 'call' 识别同源对齐）。
  4. **String::parse::<T>() turbofish 泛型实参首次接线**（此前全部语言回退 contract，含原生支持的 rust）：body.ts method() 透传 `e.generics`；rust 原生 `.parse::<i64>()` 直投；python `_dhv_parse_int(s, ty)`（严格 `[+-]?\d+` 正则 + u 型拒负；float 助手手工实现 JS Number 语义子集：Infinity/NaN 接受，空串/下划线/裸 inf 拒绝）；ts/js `_dhvParseInt/_dhvParseFloat`（与 interp 同为 JS 实现，语义天然同源）；cpp `_dhvParse<T>` 模板助手（stoll/stod + pos 全消耗检查 + unsigned 拒负 + catch(...) → nullopt）；go `_dhvParseInt(s, unsigned)/_dhvParseFloat`（TrimSpace + 手写数字检查 + ParseInt/ParseFloat）。生成端非 rust 语言采用 **Option-flavored Result 表示**（Err → None/null/nullopt/nil，错误消息不可观察，已知限制）：链式消费（unwrap_or/unwrap/expect/is_ok/is_err）复用既有 Option 映射，表示同构无缝衔接；**is_ok/is_err 首次进入 METHOD_TABLE**（与 is_some/is_none 同构，rust 原生）。
  5. **Option::filter 新增**（interp builtin + rust 原生 + py `_dhv_filter` + ts/js `_dhvFilter` + cpp `_dhvOptFilter` 模板助手；全部单次求值，副作用接收者安全；go 表达式位置无三元 + 闭包需显式类型 → 诚实回退 contract）。Vec::filter 同名不同义走通用表不变。
  6. **Vec::sort_by 扩至 cpp/go**：cpp `std::stable_sort` + 泛型 lambda comparator（key 语义 `f(a) < f(b)`；稳定序与 interp Array.prototype.sort / rust sort_by_key 同源，断言禁止退化非稳定 std::sort）；go **闭包体内联替换**技术首秀 —— go func literal 需显式参数类型无法直投闭包值，但 `sort.SliceStable(v, func(i, j int) bool { return v[i].score < v[j].score })` 只需把闭包体中参数引用替换为 `v[i]`/`v[j]` 即可内联合成（substParam 深克隆表达式树，不支持的形态诚实 throw 回退 contract）。
  7. **char 谓词 is_alphabetic/is_numeric 扩至 ts/js/cpp/go**：ts/js 与 interp 同源正则 `/[A-Za-z\u0080-\uFFFF]/.test(c)` / `/[0-9]/.test(c)`；cpp/go `_dhvIsAlpha/_dhvIsDigit` 助手（UTF-8 首字节 ≥ 0x80 判非 ASCII，与 interp 正则语义精确对齐 —— 非 ASCII 字符在 UTF-8 下首字节 ≥ 0x80，与 \u0080-\uFFFF 匹配等价）。
  8. **测试套件扩至 105 用例**（+9）：parse turbofish 全语言活体（5 后端结构断言）/ cpp parse g++ 编译+运行语义级（11251 与 interp 逐字对齐）/ python parse python3 exec 语义级（101151）/ interp 空串修复回归（111）/ Option::filter interp+py exec+cpp 结构 / cpp 裸 None 链 g++ 编译+运行（113）+ 修复回归断言 / cpp sort_by 稳定序 g++（1020/2131 —— 2131 验证稳定序 21 在 31 前）/ go sort_by 闭包内联替换结构 / char 谓词 g++ 编译+运行（11/111 含 UTF-8 é 精确对齐）。

---

## 附录 A：std 预导入库方法面（v1.3）

参考实现（dhv-ts `src/builtins.ts`）提供的标准方法集。编译目标语言（dhv Rust）投射时映射到各后端等价物。

**String**：`len` `is_empty` `push_str`* `as_str` `clone` `to_string` `trim` `trim_start` `trim_end` `contains` `starts_with` `ends_with` `replace` `split` `split_whitespace` `lines` `to_lowercase` `to_uppercase` `chars` `repeat` `strip_prefix`→Option `strip_suffix`→Option `find`→Option `parse::<T>`→Result `take`（*为变异方法，要求 place 接收者）

**Vec**：`len` `is_empty` `push`* `pop`→Option `clone` `first`/`last`→Option `get`→Option `contains` `join` `iter` `map` `filter` `for_each` `any` `all` `fold` `enumerate` `take` `skip` `rev` `sort`* `sort_by`* `append`* `extend`* `insert`* `remove`* `sum` `position` `collect::<T>()`

**HashMap**：`insert`* `get`→Option `contains_key` `len` `is_empty` `remove`→Option `keys` `values` `clone`

**Option**：`unwrap` `expect` `unwrap_or` `unwrap_or_else` `is_some` `is_none` `map` `and_then` `ok_or` `or` `cloned` `filter`（v1.4.9）

**Result**：`unwrap` `expect` `is_ok` `is_err` `ok` `err` `map` `map_err` `unwrap_or` `and_then` `or_else`

**数值**：`to_string` `abs` `pow` `sqrt` `floor` `ceil` `round` `min` `max` `clamp`　**char**：`to_string` `is_alphabetic` `is_numeric`

**宏（预导入）**：`format!`（`{}`/`{0}`/`{:?}`/`{{`转义）`vec!` `println!` `print!` `eprintln!` `panic!` `assert!` `assert_eq!` `dbg!`

## 附录 B：native 运行时 ABI（v1.3）

解释器（`dhv run --interpret`）与静态投射（dhv codegen）共用以下约定：

1. **`$host` 注入**：native 块内可访问 `$host`（宿主 API 命名空间：`llm`/`fs`/`shell`/`json`/`artifacts`/`events`/`fixture`/`log`/`env`/`config`）。宿主 API 是运行时能力，不属于语言语义。
2. **捕获变量按名映射（N1）**：native 体内引用的外层 HSL 变量按名字注入块作用域。**self 的字段必须写 `self.field`**（self 本身是捕获变量，字段不是）。
3. **返回值**：块内显式 `return`；或末表达式（解释器对无 return 的 typescript 体自动包裹 `return (...)`；python 体自动变换末行为 `__hsl_result__ = (...)`）。
4. **类型纪律（N2）**：进/出 native 的值应为平凡类型（bool/数值/String/Vec/Option/HashMap）；`$host.json.fields` 把 JSON 对象顶层字段字符串化为 `HashMap<String, String>`，保持 HSL 侧零动态对象穿透。
5. **后端可用性**：解释器运行期支持 `native typescript`（进程内）与 `native python`（python3 子进程，JSON 编组）；其余语言由 dhv 静态投射（P5 FFI 胶水）。

---

## 附录 C：std 标准库（10 模块，v1.4）

C++ 式多库组织。`import { 函数 } from "std/<模块>";` 显式引入（虚拟模块，不触文件系统；
dhv-ts 参考实现 `src/std.ts`）。常量：`std/math` 的 `PI`、`E`。

### std/core —— 身份/断言/哈希

| 函数 | 签名 | 说明 |
|:--|:--|:--|
| `identity` | `<T>(x: T) -> T` | 恒等 |
| `todo` | `(msg?: String) -> !` | 未实现占位（panic） |
| `unreachable` | `(msg?: String) -> !` | 不可达分支（panic） |
| `type_name` | `(v: Any) -> String` | 运行期类型名 |
| `hash` | `(v: Any) -> i64` | FNV-1a 64（Debug 表示上求值） |

### std/collections —— Vec 构建与变换

| 函数 | 签名 | 说明 |
|:--|:--|:--|
| `vec` | `(...items) -> Vec<T>` | 变参构造 |
| `repeat_vec` | `(v: T, n: i64) -> Vec<T>` | 重复填充 |
| `zip` | `(a: Vec<A>, b: Vec<B>) -> Vec<(A, B)>` | 拉链（取短） |
| `chunk` | `(v: Vec<T>, n: i64) -> Vec<Vec<T>>` | 分块 |
| `dedup` | `(v: Vec<T>) -> Vec<T>` | 去除**连续**重复 |
| `unique` | `(v: Vec<T>) -> Vec<T>` | 保序去重 |
| `flatten` | `(v: Vec<Vec<T>>) -> Vec<T>` | 拍平（一层） |
| `sort_desc` | `(v: Vec<f64>) -> Vec<f64>` | 降序排序（新 Vec） |
| `reverse` | `(v: Vec<T>) -> Vec<T>` | 反转（新 Vec） |
| `swap_remove` | `(v: Vec<T>, i: i64) -> T` | 交换删除（O(1)，返回被删值） |

### std/text —— 字符串工具

| 函数 | 签名 | 说明 |
|:--|:--|:--|
| `split_once` | `(s: String, sep: String) -> Option<(String, String)>` | 首次切分 |
| `rsplit_once` | `(s: String, sep: String) -> Option<(String, String)>` | 末次切分 |
| `split_at` | `(s: String, i: i64) -> (String, String)` | 按索引切分 |
| `to_snake` / `to_camel` / `to_pascal` / `to_kebab` | `(String) -> String` | 命名风格转换 |
| `pad_start` / `pad_end` | `(s, n, ch?) -> String` | 填充 |
| `capitalize` | `(String) -> String` | 首字母大写 |
| `count` | `(s, sub) -> i64` | 子串计数 |
| `is_alpha` / `is_numeric` / `is_alphanumeric` | `(String) -> bool` | 字符类别 |
| `truncate` | `(s, n, ell?) -> String` | 截断加省略号（默认 `…`，按字符计） |
| `levenshtein` | `(a, b) -> i64` | 编辑距离 |

### std/math —— 数学

`PI`、`E` 常量；`sin cos tan asin acos atan atan2 exp ln log2 log10 pow sqrt`；
`gcd(a,b) lcm(a,b) signum(x) isqrt(n) div_ceil(a,b) div_floor(a,b) rem_euclid(a,b) hypot(a,b)
is_nan(x) is_infinite(x) inf()`。

### std/io —— 文件（宿主路径监狱）

| 函数 | 签名 | 说明 |
|:--|:--|:--|
| `read_file` | `(path) -> Result<String, String>` | 读文本（≤2MB） |
| `write_file` | `(path, content) -> Result<i64, String>` | 写（建目录），返回字节数 |
| `append_file` | `(path, content) -> Result<i64, String>` | 追加 |
| `list_dir` | `(dir) -> Result<Vec<String>, String>` | 目录列表（两层深） |

宿主不可用（如 emit 模式）时返回 `Err`。路径越狱被宿主拒绝（capability 语义）。

### std/json —— JSON（本地确定性实现）

| 函数 | 签名 | 说明 |
|:--|:--|:--|
| `parse` | `(s) -> Result<Any, String>` | 解析为运行期对象（Vec/HashMap/标量） |
| `stringify` | `(v) -> String` | 序列化（struct/enum/Vec/HashMap 原生支持） |
| `get` | `(obj, key) -> Option<Any>` | 对象字段取值 |

### std/time —— 时间

`now_ms() -> i64`、`now_iso() -> String`、`duration_desc(ms) -> String`（如 `1.2s`）。

### std/random —— 可复现随机

mulberry32 PRNG，**默认种子 42**（同种子同序列 —— 确定性测试友好）。
`seed(n)` 重置；`random() -> f64`、`int_in(lo, hi) -> i64`（含端点）、
`choice(v) -> Option<T>`、`shuffle(v) -> Vec<T>`（新 Vec）、`uuid_v4() -> String`。

### std/env —— 环境与配置

`env_get(name) -> Option<String>`、`task_text() -> String`（宿主任务）、
`model_name() -> String`、`workspace() -> String`。

### std/iter —— 迭代工具

`range(lo, hi)`、`range_step(lo, hi, step)`（上界 10⁶ 防炸）、
`enumerate(v) -> Vec<(i64, T)>`、`chain(a, b)`、`take(v, n)`、`skip(v, n)`、
`min_of(v) -> Option<T>`、`max_of(v) -> Option<T>`。
