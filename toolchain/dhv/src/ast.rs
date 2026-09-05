//! # HSL AST — 类型定义（P1）
//!
//! 本文件是 HSL 语言所有语法节点的权威 AST 定义，与 `hsl-spec/BNF.md`（v1.0）
//! 逐条对应。DHV 编译器的 Parser（P2）、Type Check、Codegen、Lint 均以本文件为基础。
//!
//! 设计要点：
//! - **带 Span 的包装结构体**：`Expr { span, kind }` 风格（rustc 式），便于错误定位与 Lint。
//! - **定义与投射分离**：`TopLevel::Item`（定义层）与 `TopLevel::Project` / `TopLevel::Scale`
//!   （投射层）在文件级区分（BNF §2.1）。
//! - **原始代码区**：`native` 块与 `block/static` 体保留原文（`RawCode` / `RawContentPart`），
//!   HSL 不解析其内部（BNF §1.9 模式 A/B）。
//! - 决策 D1-D7（无 lifetime / 无 unsafe / 无三元 / static 专用于资源块 / label 专用 `'ident` /
//!   graph-loop 同形 / as 唯一转换）在类型层面均已体现。

use std::fmt;

// ============================================================================
// 位置信息
// ============================================================================

/// 源文件标识（由编译驱动分配，`0` 通常表示主文件）
pub type FileId = u32;

/// 源码区间 [start, end)，字节偏移
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub file: FileId,
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(file: FileId, start: usize, end: usize) -> Self {
        Span { file, start, end }
    }
    pub fn merge(a: Span, b: Span) -> Self {
        debug_assert_eq!(a.file, b.file);
        Span { file: a.file, start: a.start.min(b.start), end: a.end.max(b.end) }
    }
}

impl Default for Span {
    fn default() -> Self {
        Span { file: 0, start: 0, end: 0 }
    }
}

// ============================================================================
// 标识符与路径
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

impl Ident {
    pub fn new(name: impl Into<String>, span: Span) -> Self {
        Ident { name: name.into(), span }
    }
}

impl fmt::Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

/// 路径：`a::b::c`（`leading_colon` 表示 `::a::b` 全局路径）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    pub leading_colon: bool,
    pub segments: Vec<Ident>,
    pub span: Span,
}

impl Path {
    pub fn is_ident(&self, name: &str) -> bool {
        !self.leading_colon
            && self.segments.len() == 1
            && self.segments[0].name == name
    }
    pub fn last(&self) -> &Ident {
        self.segments.last().expect("path has at least one segment")
    }
}

// ============================================================================
// 源文件与顶层
// ============================================================================

/// 一个 `.hsl` 文件的完整 AST
#[derive(Debug, Clone, Default)]
pub struct SourceFile {
    pub items: Vec<TopLevel>,
    pub span: Span,
}

/// 文件顶层：定义层项 + 投射层声明（BNF §2.1）
#[derive(Debug, Clone)]
pub enum TopLevel {
    /// 定义层：struct / enum / trait / impl / fn / graph / block / import ...
    Item(Item),
    /// 投射层：`scale = monolith | microkernel;`（BNF §3.5）
    Scale(ScaleDecl),
    /// 投射层：`project { ... }`（BNF §3.4）
    Project(ProjectBlock),
}

/// `scale = <mode>;`
#[derive(Debug, Clone)]
pub struct ScaleDecl {
    pub mode: ScaleMode,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScaleMode {
    Monolith,
    Microkernel,
    /// 未来扩展模式（`serverless` 等），编译器按注册表校验
    Custom(String),
}

impl fmt::Display for ScaleMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScaleMode::Monolith => f.write_str("monolith"),
            ScaleMode::Microkernel => f.write_str("microkernel"),
            ScaleMode::Custom(s) => f.write_str(s),
        }
    }
}

/// `project { Target -> "path" : lang, ... }`
#[derive(Debug, Clone)]
pub struct ProjectBlock {
    pub projections: Vec<Projection>,
    /// §2.15（BNF v1.5）投射规则组：按项类型批量映射，显式映射优先（R1 遮蔽原则）
    pub rules: Vec<ProjectionRule>,
    pub span: Span,
}

/// 单条投射规则：项类型 -> 物理路径模板（唯一占位符 {name}）: 目标语言
#[derive(Debug, Clone)]
pub struct ProjectionRule {
    /// 项类型：graph / fn / struct / enum / trait / const / type / block|static
    pub kind: String,
    /// 路径模板，如 "src/types/{name}.rs"
    pub path: String,
    pub lang: Ident,
    pub span: Span,
}

/// 单条投射：逻辑项 → 物理文件 : 目标语言
#[derive(Debug, Clone)]
pub struct Projection {
    pub target: Path,
    /// 物理文件路径（相对工程根）
    pub path: String,
    /// 目标语言后端标识（rust / python / typescript / yaml / markdown / json / toml）
    pub lang: Ident,
    pub span: Span,
}

// ============================================================================
// 项（Items）— BNF §2.2
// ============================================================================

#[derive(Debug, Clone)]
pub enum Item {
    Struct(StructDef),
    Enum(EnumDef),
    Trait(TraitDef),
    Impl(ImplDef),
    Fn(FnDef),
    Const(ConstDef),
    TypeAlias(TypeAliasDef),
    /// HSL 专属：`graph` 拓扑（BNF §3.1）
    Graph(GraphDef),
    /// HSL 专属：`block` / `static` 静态资源（BNF §3.2）
    StaticResource(StaticResourceDef),
    Import(ImportDecl),
    /// `export <item>` — 导出修饰（BNF §2.3）
    Export(Box<ExportItem>),
    MacroRules(MacroRulesDefinition),
    /// 语句级宏调用项：`impl_tool!(...)`（BNF §2.2 MacroInvocationSemi）
    MacroCall { path: Path, args: MacroArgs },
}

impl Item {
    pub fn span(&self) -> Span {
        match self {
            Item::Struct(x) => x.span,
            Item::Enum(x) => x.span,
            Item::Trait(x) => x.span,
            Item::Impl(x) => x.span,
            Item::Fn(x) => x.span,
            Item::Const(x) => x.span,
            Item::TypeAlias(x) => x.span,
            Item::Graph(x) => x.span,
            Item::StaticResource(x) => x.span,
            Item::Import(x) => x.span,
            Item::Export(x) => x.item.span(),
            Item::MacroRules(x) => x.span,
            Item::MacroCall { args, .. } => args.span,
        }
    }

    pub fn name(&self) -> Option<&Ident> {
        match self {
            Item::Struct(x) => Some(&x.name),
            Item::Enum(x) => Some(&x.name),
            Item::Trait(x) => Some(&x.name),
            Item::Impl(_) => None,
            Item::Fn(x) => Some(&x.name),
            Item::Const(x) => Some(&x.name),
            Item::TypeAlias(x) => Some(&x.name),
            Item::Graph(x) => Some(&x.name),
            Item::StaticResource(x) => Some(&x.name),
            Item::Import(_) => None,
            Item::Export(x) => x.item.name(),
            Item::MacroRules(x) => Some(&x.name),
            Item::MacroCall { .. } => None,
        }
    }

    pub fn attrs(&self) -> &[Attribute] {
        match self {
            Item::Struct(x) => &x.attrs,
            Item::Enum(x) => &x.attrs,
            Item::Trait(x) => &x.attrs,
            Item::Impl(_) => &[],
            Item::Fn(x) => &x.attrs,
            Item::Const(x) => &x.attrs,
            Item::TypeAlias(x) => &x.attrs,
            Item::Graph(x) => &x.attrs,
            Item::StaticResource(x) => &x.attrs,
            Item::Import(_) => &[],
            Item::Export(x) => x.item.attrs(),
            Item::MacroRules(_) => &[],
            Item::MacroCall { .. } => &[],
        }
    }
}

/// `export <item>`
#[derive(Debug, Clone)]
pub struct ExportItem {
    pub item: Item,
}

// ---------------------------------------------------------------------------
// 属性 — BNF §2.2
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Attribute {
    pub path: Path,
    pub args: Option<AttrArgs>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum AttrArgs {
    /// `#[derive(Debug, Clone)]` — token 树参数
    Tokens(Vec<TokenTree>),
    /// `#[cfg(lang: rust)]` — 键值式 cfg 参数（保留 token 树，语义层解析）
    Assign(Literal),
}

// ---------------------------------------------------------------------------
// 结构体 / 枚举 — BNF §2.4
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct StructDef {
    pub attrs: Vec<Attribute>,
    pub name: Ident,
    pub generics: GenericParams,
    pub kind: StructKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StructKind {
    /// `struct S { a: T }`
    Named(Vec<FieldDef>),
    /// `struct S(T, U);`
    Tuple(Vec<FieldDef>),
    /// `struct S;`
    Unit,
}

/// 具名字段（named）或元组字段（tuple；name 为位置编号）
#[derive(Debug, Clone)]
pub struct FieldDef {
    pub attrs: Vec<Attribute>,
    pub name: Option<Ident>, // None = 元组字段
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub attrs: Vec<Attribute>,
    pub name: Ident,
    pub generics: GenericParams,
    pub variants: Vec<VariantDef>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct VariantDef {
    pub attrs: Vec<Attribute>,
    pub name: Ident,
    pub fields: StructKind,
    /// `= <判别式>`
    pub discriminant: Option<Literal>,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// trait / impl — BNF §2.5
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TraitDef {
    pub attrs: Vec<Attribute>,
    pub name: Ident,
    pub generics: GenericParams,
    /// supertrait 约束 `trait A: B + C`
    pub supertraits: Vec<TypeBound>,
    pub items: Vec<TraitItem>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum TraitItem {
    /// 无默认实现的签名（带分号）
    FnSig(FnSig),
    Const(ConstDef),
    TypeAlias(TypeAliasDef),
    /// 带默认实现的函数
    Fn(FnDef),
}

/// 函数签名（trait 声明 / 函数指针等场景）
#[derive(Debug, Clone)]
pub struct FnSig {
    pub is_async: bool,
    pub name: Ident,
    pub generics: GenericParams,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ImplDef {
    pub attrs: Vec<Attribute>,
    /// `impl Trait for Type` 的 Trait 侧（None = inherent impl）
    pub trait_ty: Option<Type>,
    pub self_ty: Type,
    pub generics: GenericParams,
    pub items: Vec<ImplItem>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ImplItem {
    Fn(FnDef),
    Const(ConstDef),
    TypeAlias(TypeAliasDef),
}

// ---------------------------------------------------------------------------
// 函数 — BNF §2.6
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FnDef {
    pub attrs: Vec<Attribute>,
    pub is_async: bool,
    pub name: Ident,
    pub generics: GenericParams,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub where_clause: Vec<WherePredicate>,
    /// None = 无体的外部签名（trait 声明）
    pub body: Option<BlockExpr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub kind: ParamKind,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ParamKind {
    /// 普通（模式绑定）参数
    Pattern(Pattern),
    /// `self` / `&self` / `&mut self` / `mut self`
    Self_(SelfKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfKind {
    Value,
    Mut,
    Ref,
    RefMut,
}

// ---------------------------------------------------------------------------
// 常量 / 类型别名 — BNF §2.7
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ConstDef {
    pub attrs: Vec<Attribute>,
    pub name: Ident,
    pub ty: Type,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypeAliasDef {
    pub attrs: Vec<Attribute>,
    pub name: Ident,
    pub generics: GenericParams,
    pub ty: Type,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// 泛型 — BNF §2.8
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct GenericParams {
    pub type_params: Vec<TypeParam>,
    pub const_params: Vec<ConstParam>,
}

impl GenericParams {
    pub fn is_empty(&self) -> bool {
        self.type_params.is_empty() && self.const_params.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct TypeParam {
    pub name: Ident,
    pub bounds: Vec<TypeBound>,
    pub default: Option<Type>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ConstParam {
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}

/// `T: Bound1 + Bound2`（单个谓词）
#[derive(Debug, Clone)]
pub struct WherePredicate {
    pub ty: Type,
    pub bounds: Vec<TypeBound>,
    pub span: Span,
}

/// trait 约束（D1：无 lifetime bound，恒为类型约束）
#[derive(Debug, Clone)]
pub struct TypeBound {
    pub ty: Type,
    pub span: Span,
}

// ============================================================================
// 类型 — BNF §2.9
// ============================================================================

#[derive(Debug, Clone)]
pub struct Type {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum TypeKind {
    /// 命名类型 / 泛型应用：`i32`、`Vec<String>`、`hsl::collections::HashMap<K,V>`
    Path(PathType),
    /// `&T` / `&mut T`
    Ref { mutable: bool, inner: Box<Type> },
    /// `(A, B)`
    Tuple(Vec<Type>),
    /// `[T; N]`
    Array { elem: Box<Type>, len: ConstArg },
    /// `[T]`（切片，仅作为 `&[T]` 内层出现）
    Slice(Box<Type>),
    /// `!`
    Never,
    /// `(T)` 的去糖结果由 Parser 归一；此变体保留原始括号信息供 lint
    Paren(Box<Type>),
    /// `fn(A, B) -> C`
    FnPtr { params: Vec<Type>, ret: Option<Box<Type>> },
    /// `dyn Trait + Send`
    DynTrait(Vec<TypeBound>),
    /// `impl Trait`（参数/返回位置）
    ImplTrait(Vec<TypeBound>),
    /// `_`
    Infer,
}

/// 带泛型实参的路径类型
#[derive(Debug, Clone)]
pub struct PathType {
    pub path: Path,
    pub generic_args: Vec<GenericArg>,
}

#[derive(Debug, Clone)]
pub enum GenericArg {
    Type(Type),
    Const(ConstArg),
}

/// 常量泛型实参：字面量或 const 块表达式
#[derive(Debug, Clone)]
pub struct ConstArg {
    pub kind: ConstArgKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ConstArgKind {
    Literal(Literal),
    Block(Box<BlockExpr>),
}

// ============================================================================
// 模式 — BNF §2.10
// ============================================================================

#[derive(Debug, Clone)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum PatternKind {
    /// `1`、`"x"`、`true`、`-3.14`
    Literal(Literal),
    /// `x` / `mut x` / `x @ pat`
    Ident { mutable: bool, name: Ident, sub: Option<Box<Pattern>> },
    /// `_`
    Wildcard,
    /// `..`（解构剩余）
    Rest,
    /// `a..b` / `a..=b`
    Range { lo: Box<Pattern>, hi: Box<Pattern>, inclusive: bool },
    /// `Some(x)`、`Action::CallTool { name, args }`
    Struct { path: Path, fields: Vec<StructPatternField>, rest: bool },
    /// `Some(x)` / `Action::Respond(text)`（元组结构/枚举变体模式）
    TupleStruct { path: Path, elems: Vec<Pattern>, rest_at: Option<usize> },
    /// `(a, b)` / `(a, .., c)`
    Tuple { elems: Vec<Pattern>, rest_at: Option<usize> },
    /// 单元 `()` 按空元组处理
    /// `(pat)` 的去糖结果由 Parser 归一
    Path(Path),
    /// `p1 | p2 | p3`（顶层 or-pattern 已在 Parser 展开）
    Or(Vec<Pattern>),
}

#[derive(Debug, Clone)]
pub struct StructPatternField {
    pub name: Ident,
    pub pattern: Option<Pattern>, // None = 简写 `name`
    pub span: Span,
}

// ============================================================================
// 字面量 — BNF §1.5
// ============================================================================

#[derive(Debug, Clone)]
pub struct Literal {
    pub kind: LiteralKind,
    /// 原始文本（保留进制写法，用于 SourceMap 回写保真）
    pub raw: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum LiteralKind {
    /// value：i128 域内的值；overflow=true 表示源字面量超出 i128 容量
    /// （v0.2.54 L-10：此前 parse 失败静默归零 —— 值损坏比溢出更糟，
    /// dhv-ts BigInt 精确而 dhv 归零的双端漂移实录。S-16 静态拒绝）
    Int { value: i128, suffix: Option<IntSuffix>, overflow: bool },
    Float { value: f64, suffix: Option<FloatSuffix> },
    /// 已去除引号与转义的字符串值
    Str { value: String, raw_string: bool },
    Char(char),
    Bool(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntSuffix {
    I8, I16, I32, I64, I128, Isize,
    U8, U16, U32, U64, U128, Usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatSuffix {
    F32, F64,
}

// ============================================================================
// 表达式 — BNF §2.11
// ============================================================================

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Literal(Literal),
    /// 路径表达式（含枚举变体构造 `Action::Stop`）
    Path(Path),
    /// 二元运算：`a + b`（含 `&&`/`||`）
    Binary { op: BinaryOp, lhs: Box<Expr>, rhs: Box<Expr> },
    /// 一元运算：`-x`、`!x`、`*x`、`&x`、`&mut x`
    Unary { op: UnaryOp, operand: Box<Expr> },
    /// `f(a, b)`
    Call { callee: Box<Expr>, args: Vec<Expr> },
    /// `x.method::<T>(a, b)`
    MethodCall { receiver: Box<Expr>, method: Ident, generic_args: Vec<GenericArg>, args: Vec<Expr> },
    /// `x.field` / `tup.0`
    Field { base: Box<Expr>, field: FieldIndex },
    /// `a[i]`
    Index { base: Box<Expr>, index: Box<Expr> },
    /// `a[1..3]` 切片
    Slice { base: Box<Expr>, range: RangeExpr },
    /// `0..5` / `a..=b` / `n..` / `..n` 范围表达式（值语境；for-in 迭代与 let 绑定）
    Range(Box<RangeExpr>),
    /// `expr?` — 错误传播（后缀，BNF §2.11.3）
    Try(Box<Expr>),
    /// `expr.await` — 异步等待（后缀）
    Await(Box<Expr>),
    /// `expr as T` — 显式转换（唯一转换通道，D7）
    Cast { expr: Box<Expr>, ty: Type },
    /// `x = y`
    Assign { lhs: Box<Expr>, rhs: Box<Expr> },
    /// `x += y` 等
    CompoundAssign { op: BinaryOp, lhs: Box<Expr>, rhs: Box<Expr> },
    /// 闭包：`|x| x + 1` / `move |x: u8| { .. }` / `async || ..`
    Closure { is_move: bool, is_async: bool, params: Vec<Param>, ret: Option<Type>, body: Box<Expr> },
    /// `if cond { a } else { b }`（else-if 链由 Parser 嵌套展开）
    If { cond: Box<Expr>, then: BlockExpr, else_: Option<Box<Expr>> },
    /// `if let pat = expr { .. } else { .. }`
    IfLet { pattern: Pattern, expr: Box<Expr>, then: BlockExpr, else_: Option<Box<Expr>> },
    /// `match expr { arms }`
    Match { scrutinee: Box<Expr>, arms: Vec<MatchArm> },
    /// `loop { .. }`（graph 体内即为 AgentLoop，AST 同形，D6）
    Loop { label: Option<Ident>, body: BlockExpr },
    /// `while cond { .. }`
    While { label: Option<Ident>, cond: Box<Expr>, body: BlockExpr },
    /// `while let pat = expr { .. }`
    WhileLet { label: Option<Ident>, pattern: Pattern, expr: Box<Expr>, body: BlockExpr },
    /// `for pat in iter { .. }`
    For { label: Option<Ident>, pattern: Pattern, iter: Box<Expr>, body: BlockExpr },
    /// `{ stmts; tail }`
    Block(BlockExpr),
    /// `async { .. }` / `async move { .. }`
    AsyncBlock { is_move: bool, body: BlockExpr },
    /// `break [label] [value]`
    Break { label: Option<Ident>, value: Option<Box<Expr>> },
    /// `continue [label]`
    Continue { label: Option<Ident> },
    /// `return [value]`
    Return(Option<Box<Expr>>),
    /// `(a, b)` / `()`
    Tuple(Vec<Expr>),
    /// `[a, b, c]`
    Array(Vec<Expr>),
    /// `[elem; count]`
    ArrayRepeat { elem: Box<Expr>, count: Box<Expr> },
    /// `Struct { field: value, ..base }`
    Struct { path: Path, fields: Vec<StructExprField>, spread: Option<Box<Expr>> },
    /// 宏调用：`path!(...)`
    Macro { path: Path, args: MacroArgs },
    /// HSL 专属：`native <lang> { ... }` 逃生舱（BNF §3.3）
    Native(NativeBlock),
}

#[derive(Debug, Clone)]
pub enum FieldIndex {
    Named(Ident),
    /// 元组下标 `tup.0`
    Index(u32, Span),
}

/// 范围表达式（仅索引 / 切片位置与模式位置出现）
#[derive(Debug, Clone)]
pub struct RangeExpr {
    pub lo: Option<Box<Expr>>,
    pub hi: Option<Box<Expr>>,
    pub inclusive: bool,
}

#[derive(Debug, Clone)]
pub struct StructExprField {
    pub name: FieldIndex,
    /// None = 简写 `field`
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub attrs: Vec<Attribute>,
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// 块与语句 — BNF §2.12
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BlockExpr {
    pub stmts: Vec<Stmt>,
    /// 尾表达式（无分号；块的值）
    pub tail: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    /// `let pat [: ty] [= init] [else { .. }];`
    Let(LetStmt),
    /// 局部项
    Item(Item),
    /// 表达式语句；has_semi 影响类型（块值语义）
    Expr { expr: Expr, has_semi: bool },
    /// 空语句 `;`
    Empty(Span),
}

#[derive(Debug, Clone)]
pub struct LetStmt {
    pub attrs: Vec<Attribute>,
    pub mutable: bool,
    pub pattern: Pattern,
    pub ty: Option<Type>,
    pub init: Option<Expr>,
    /// `let ... else { .. }`（发散块）
    pub else_block: Option<BlockExpr>,
    pub span: Span,
}

// ============================================================================
// HSL 专属：graph / edge — BNF §3.1
// ============================================================================

#[derive(Debug, Clone)]
pub struct GraphDef {
    pub attrs: Vec<Attribute>,
    pub name: Ident,
    pub generics: GenericParams,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub where_clause: Vec<WherePredicate>,
    pub body: Vec<GraphStmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum GraphStmt {
    /// `node planner: Planner = Planner::new();`
    Node(NodeDecl),
    /// `edge a -> b on Action::CallTool with retry(3);`
    Edge(EdgeDecl),
    /// `let ...`
    Let(LetStmt),
    /// 其他语句（含 AgentLoop：`loop { .. }`）
    Stmt(Stmt),
    /// graph 体内局部项
    Item(Item),
}

#[derive(Debug, Clone)]
pub struct NodeDecl {
    pub mutable: bool,
    pub name: Ident,
    pub ty: Type,
    pub init: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EdgeDecl {
    /// 端点序列：`a -> b -> c`（链式边，语义 = 多条二元边）
    pub endpoints: Vec<Path>,
    /// `on <guard>`：条件边（模式或表达式）
    pub on: Option<EdgeGuard>,
    /// `with attr = value, ...`
    pub attrs: Vec<EdgeAttr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum EdgeGuard {
    /// `on Action::CallTool` — 枚举变体模式
    Pattern(Pattern),
    /// `on expr` — 布尔守卫
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct EdgeAttr {
    pub name: Ident,
    pub value: Option<Literal>,
    pub span: Span,
}

// ============================================================================
// HSL 专属：block / static 静态资源 — BNF §3.2
// ============================================================================

#[derive(Debug, Clone)]
pub struct StaticResourceDef {
    pub attrs: Vec<Attribute>,
    /// `block` 或 `static`（同义；风格由 Lint 决定）
    pub kind: ResourceKind,
    pub name: Ident,
    /// 原始内容（文本 + 编译期插值交错）
    pub content: Vec<RawContentPart>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Block,
    Static,
}

#[derive(Debug, Clone)]
pub enum RawContentPart {
    /// 原始文本（保留逐字原文，供 SourceMap 回写保真）
    Text(String),
    /// `{{ expr }}` 编译期插值（类型必须实现 ToString，BNF §5.5 N4）
    Interpolation { expr: Expr, span: Span },
}

// ============================================================================
// HSL 专属：native 逃生舱 — BNF §3.3
// ============================================================================

#[derive(Debug, Clone)]
pub struct NativeBlock {
    /// 目标语言（rust / python / typescript / ...，编译器注册表校验）
    pub lang: Ident,
    /// 原始代码（逐字保留；含内部换行与缩进）
    pub code: String,
    pub span: Span,
}

// ============================================================================
// import / export — BNF §2.3
// ============================================================================

#[derive(Debug, Clone)]
pub struct ImportDecl {
    pub spec: ImportSpec,
    /// 模块路径（相对当前 .hsl 文件），如 "../models/types.hsl"
    pub from: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ImportSpec {
    /// `import { A, B as C } from "..";`
    Named(Vec<ImportItem>),
    /// `import * as m from "..";`
    Namespace { alias: Ident },
    /// `import A as B from "..";`
    Single(ImportItem),
}

#[derive(Debug, Clone)]
pub struct ImportItem {
    pub name: Ident,
    pub alias: Option<Ident>,
}

// ============================================================================
// 宏 — BNF §2.13
// ============================================================================

#[derive(Debug, Clone)]
pub struct MacroRulesDefinition {
    pub name: Ident,
    pub rules: Vec<MacroRule>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MacroRule {
    pub matcher: Vec<MacroMatch>,
    pub transcriber: Vec<MacroTranscribe>,
}

#[derive(Debug, Clone)]
pub enum MacroMatch {
    /// `$name:frag`
    Fragment { name: Ident, frag: MacroFragSpec },
    /// `$( ... )SEP* / + / ?`
    Repetition { pattern: Vec<MacroMatch>, separator: Option<String>, op: RepetitionOp },
    /// 任意 token / 定界树（字面匹配）
    Token(TokenTree),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroFragSpec {
    Ident, Path, Expr, Ty, Pat, Stmt, Block, Item, Literal, Tt, Meta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepetitionOp {
    ZeroOrMore, // *
    OneOrMore,  // +
    ZeroOrOne,  // ?
}

#[derive(Debug, Clone)]
pub enum MacroTranscribe {
    /// `$name`
    Var(Ident),
    /// `$( ... )SEP*`
    Repetition { pattern: Vec<MacroTranscribe>, separator: Option<String>, op: RepetitionOp },
    Token(TokenTree),
}

#[derive(Debug, Clone)]
pub struct MacroArgs {
    pub delim: Delimiter,
    pub tokens: Vec<TokenTree>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delimiter {
    Paren,
    Bracket,
    Brace,
}

/// 宏 / 属性用的 token 树
#[derive(Debug, Clone)]
pub enum TokenTree {
    Token(Token, Span),
    Delimited { delim: Delimiter, tokens: Vec<TokenTree>, span: Span },
}

/// 单 token（保留原始文本）
#[derive(Debug, Clone)]
pub enum Token {
    Ident(String),
    RawIdent(String),
    Literal(Literal),
    Label(String),
    Punct(String),
}

// ============================================================================
// 运算符
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Rem,
    BitAnd, BitOr, BitXor, Shl, Shr,
    Eq, Ne, Lt, Gt, Le, Ge,
    And, Or,
}

impl BinaryOp {
    pub fn precedence(self) -> u8 {
        use BinaryOp::*;
        match self {
            Or => 3, And => 4, BitOr => 5, BitXor => 6, BitAnd => 7,
            Eq | Ne => 8, Lt | Gt | Le | Ge => 9,
            Shl | Shr => 10, Add | Sub => 11, Mul | Div | Rem => 12,
        }
    }
    pub fn as_str(self) -> &'static str {
        use BinaryOp::*;
        match self {
            Add => "+", Sub => "-", Mul => "*", Div => "/", Rem => "%",
            BitAnd => "&", BitOr => "|", BitXor => "^", Shl => "<<", Shr => ">>",
            Eq => "==", Ne => "!=", Lt => "<", Gt => ">", Le => "<=", Ge => ">=",
            And => "&&", Or => "||",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
    Deref,
    Ref,
    RefMut,
}

impl UnaryOp {
    pub fn as_str(self) -> &'static str {
        match self {
            UnaryOp::Neg => "-",
            UnaryOp::Not => "!",
            UnaryOp::Deref => "*",
            UnaryOp::Ref => "&",
            UnaryOp::RefMut => "&mut",
        }
    }
}

// ============================================================================
// Visitor 基础设施（Lint / TypeCheck / Codegen 共用）
// ============================================================================

/// AST 遍历 trait —— P2 之后各 pass 实现此接口
pub trait AstVisitor {
    fn visit_file(&mut self, _file: &SourceFile) {}
    fn visit_item(&mut self, _item: &Item) {}
    fn visit_graph(&mut self, _graph: &GraphDef) {}
    fn visit_expr(&mut self, _expr: &Expr) {}
    fn visit_type(&mut self, _ty: &Type) {}
    fn visit_pattern(&mut self, _pat: &Pattern) {}
    fn visit_block(&mut self, _block: &BlockExpr) {}
}

/// 对 AST 节点的浅分类（快速谓词）
pub fn item_kind_name(item: &Item) -> &'static str {
    match item {
        Item::Struct(_) => "struct",
        Item::Enum(_) => "enum",
        Item::Trait(_) => "trait",
        Item::Impl(_) => "impl",
        Item::Fn(_) => "fn",
        Item::Const(_) => "const",
        Item::TypeAlias(_) => "type",
        Item::Graph(_) => "graph",
        Item::StaticResource(_) => "block",
        Item::Import(_) => "import",
        Item::Export(_) => "export",
        Item::MacroRules(_) => "macro_rules",
        Item::MacroCall { .. } => "macro_call",
    }
}
