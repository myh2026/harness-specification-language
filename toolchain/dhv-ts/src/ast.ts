// ============================================================================
// dhv-ts/src/ast.ts — HSL AST（对齐 BNF v1.2 §2/§3 与 dhv/src/ast.rs 命名）
// ============================================================================

export interface Span {
  line: number;
  col: number;
  file: string;
}

// ---- 类型（解释器运行期大多忽略，仅 fn 返回类型参与 `?` 的 From 转换）----
export type HType =
  | { kind: 'path'; segs: string[]; args?: HType[] }
  | { kind: 'ref'; mut: boolean; inner: HType }
  | { kind: 'tuple'; items: HType[] }
  | { kind: 'array'; elem: HType; len?: Expr }
  | { kind: 'slice'; elem: HType }
  | { kind: 'fnptr'; params: HType[]; ret?: HType }
  | { kind: 'dyn'; bounds: string[] }
  | { kind: 'implt'; bounds: string[] }
  | { kind: 'infer' }
  | { kind: 'never' }
  | { kind: 'paren'; inner: HType };

// ---- 属性 ----
export interface Attribute {
  path: string[]; // ["capability"] / ["cfg"] / ["derive"] / ["doc"] / ["allow"] / ["deny"] / ["tool"]
  argsTokens: TokenTree[]; // 原始参数 token 树
  argsRaw: string;
}

// ---- 模式 ----
export type Pattern =
  | { kind: 'wildcard'; span: Span }
  | { kind: 'literal'; value: number | bigint | string | boolean; span: Span }
  | { kind: 'binding'; name: string; mut: boolean; sub?: Pattern; span: Span }
  | { kind: 'path'; segs: string[]; sub?: TuplePat | StructPat; span: Span } // 枚举变体/常量路径
  | { kind: 'tuple'; items: Pattern[]; rest: boolean; span: Span }
  | { kind: 'struct'; segs: string[]; fields: { name: string; pat: Pattern }[]; rest: boolean; span: Span }
  | { kind: 'or'; alts: Pattern[]; span: Span }
  | { kind: 'range'; lo: Pattern; hi: Pattern; inclusive: boolean; span: Span }
  | { kind: 'rest'; span: Span };

export interface TuplePat {
  kind: 'tuple';
  items: Pattern[];
  rest: boolean;
}
export interface StructPat {
  kind: 'struct';
  fields: { name: string; pat: Pattern }[];
  rest: boolean;
}

// ---- 表达式 ----
export type Expr =
  | { kind: 'lit'; lit: LitVal; span: Span }
  | { kind: 'path'; segs: string[]; span: Span }
  | { kind: 'binary'; op: string; lhs: Expr; rhs: Expr; span: Span }
  | { kind: 'unary'; op: string; operand: Expr; span: Span }
  | { kind: 'assign'; op: string; target: Expr; value: Expr; span: Span }
  | { kind: 'call'; callee: Expr; args: Expr[]; span: Span }
  | { kind: 'method'; recv: Expr; name: string; generics?: HType[]; args: Expr[]; span: Span }
  | { kind: 'field'; recv: Expr; name: string | number; span: Span }
  | { kind: 'index'; recv: Expr; index: Expr; span: Span }
  | { kind: 'slice'; recv: Expr; lo?: Expr; hi?: Expr; inclusive: boolean; span: Span }
  | { kind: 'try'; expr: Expr; span: Span }
  | { kind: 'await'; expr: Expr; span: Span }
  | { kind: 'cast'; expr: Expr; ty: HType; span: Span }
  | { kind: 'tuple'; items: Expr[]; span: Span }
  | { kind: 'array'; items: Expr[]; span: Span }
  | { kind: 'arrayrep'; value: Expr; count: Expr; span: Span }
  | { kind: 'struct'; segs: string[]; fields: StructExprField[]; span: Span }
  | { kind: 'closure'; params: { pat: Pattern; ty?: HType }[]; ret?: HType; body: Expr; isAsync: boolean; span: Span }
  | { kind: 'if'; cond: Expr; then: Expr; els?: Expr; span: Span }
  | { kind: 'iflet'; pat: Pattern; expr: Expr; then: Expr; els?: Expr; span: Span }
  | { kind: 'match'; expr: Expr; arms: MatchArm[]; span: Span }
  | { kind: 'block'; stmts: Stmt[]; span: Span }
  | { kind: 'asyncblock'; stmts: Stmt[]; span: Span }
  | { kind: 'loop'; label?: string; body: Expr; span: Span }
  | { kind: 'while'; label?: string; cond: Expr; body: Expr; span: Span }
  | { kind: 'whilelet'; label?: string; pat: Pattern; expr: Expr; body: Expr; span: Span }
  | { kind: 'for'; label?: string; pat: Pattern; iter: Expr; range?: { lo: Expr; hi: Expr; inclusive: boolean }; body: Expr; span: Span }
  | { kind: 'break'; label?: string; value?: Expr; span: Span }
  | { kind: 'continue'; label?: string; span: Span }
  | { kind: 'return'; value?: Expr; span: Span }
  | { kind: 'macro'; path: string[]; tree: TokenTree; span: Span }
  | { kind: 'native'; lang: string; body: string; span: Span }
  | { kind: 'unit'; span: Span };

export interface StructExprField {
  name: string;
  value?: Expr; // None = 简写
  base?: Expr; // ..base 功能更新
}

export interface MatchArm {
  pattern: Pattern;
  guard?: Expr;
  body: Expr;
  span: Span;
}

export type LitVal =
  | { t: 'int'; v: number | bigint; suffix?: string }
  | { t: 'float'; v: number; suffix?: string }
  | { t: 'str'; v: string }
  | { t: 'char'; v: string }
  | { t: 'bool'; v: boolean };

// ---- 语句 ----
export type Stmt =
  | { kind: 'let'; mut: boolean; pat: Pattern; ty?: HType; init?: Expr; elseBlock?: Stmt[]; span: Span }
  | { kind: 'expr'; expr: Expr; hasSemi?: boolean; span: Span }
  | { kind: 'item'; item: Item; span: Span }
  | { kind: 'empty'; span: Span };

// ---- 函数 ----
export interface FnParam {
  self?: 'value' | 'mutvalue' | 'ref' | 'refmut';
  mut?: boolean;
  pat: Pattern;
  ty?: HType;
}

export interface FnDef {
  name: string;
  params: FnParam[];
  ret?: HType;
  body?: Stmt[]; // None = 无体（trait 声明）
  isAsync: boolean;
  generics?: string[];
  attrs?: Attribute[];
  span: Span;
}

// ---- 项 ----
export interface FieldDef {
  name: string;
  ty: HType;
  attrs: Attribute[];
}

export interface VariantDef {
  name: string;
  fields?: { named: FieldDef[] } | { tuple: HType[] };
  discr?: number | bigint;
  attrs: Attribute[];
}

export interface TraitItem {
  kind: 'sig' | 'fn' | 'const' | 'type';
  fn?: FnDef;
  name: string;
  ty?: HType;
  value?: Expr;
}

export type Item =
  | { kind: 'struct'; name: string; fields: FieldDef[]; tupleFields?: HType[]; attrs: Attribute[]; exported: boolean; span: Span }
  | { kind: 'enum'; name: string; variants: VariantDef[]; attrs: Attribute[]; exported: boolean; span: Span }
  | { kind: 'trait'; name: string; supers: string[]; items: TraitItem[]; attrs: Attribute[]; exported: boolean; span: Span }
  | { kind: 'impl'; traitSegs?: string[]; traitArgs?: HType[]; typeName: string; methods: FnDef[]; attrs: Attribute[]; exported: boolean; span: Span }
  | { kind: 'fn'; fn: FnDef; attrs: Attribute[]; exported: boolean; span: Span }
  | { kind: 'const'; name: string; ty?: HType; value: Expr; attrs: Attribute[]; exported: boolean; span: Span }
  | { kind: 'typealias'; name: string; value: HType; exported: boolean; span: Span }
  | { kind: 'graph'; graph: GraphDef; attrs: Attribute[]; exported: boolean; span: Span }
  | { kind: 'blockres'; name: string; resKind: 'block' | 'static'; parts: import('./lexer.ts').RawPart[]; attrs: Attribute[]; exported: boolean; span: Span }
  | { kind: 'import'; spec: ImportSpec; path: string; exported: boolean; span: Span }
  | { kind: 'macrodef'; name: string; rules: MacroRule[]; span: Span }
  | { kind: 'macrocallitem'; path: string[]; tree: TokenTree; span: Span };

export interface MacroRule {
  matcher: TokenTree[];
  transcriber: TokenTree[];
}

export type ImportSpec =
  | { t: 'items'; items: { name: string; alias?: string }[] }
  | { t: 'glob'; alias: string }
  | { t: 'single'; name: string; alias?: string };

// ---- graph ----
export interface GraphParam {
  mut: boolean;
  name: string;
  ty: HType;
}

export interface NodeDecl {
  name: string;
  mut: boolean;
  ty: HType;
  init?: Expr;
  span: Span;
}

export interface EdgeDecl {
  endpoints: string[]; // 每个 endpoint 是 path 的 segs 用 :: 连接
  guardExpr?: Expr;
  guardPattern?: Pattern;
  attrs: { name: string; value?: string | number | boolean }[];
  span: Span;
}

export type GraphStmt =
  | { t: 'node'; decl: NodeDecl }
  | { t: 'edge'; decl: EdgeDecl }
  | { t: 'stmt'; stmt: Stmt }
  | { t: 'item'; item: Item };

export interface GraphDef {
  name: string;
  params: GraphParam[];
  ret?: HType;
  body: GraphStmt[];
  span: Span;
}

// ---- project / scale ----
export interface ProjectionItem {
  target: string[]; // 逻辑项路径
  path: string; // 物理文件路径
  lang: string;
  span: Span;
}

export interface ProjectionRule {
  kind: string; // graph / fn / struct / enum / trait / const / type / block|static
  path: string; // 路径模板，唯一占位符 {name}
  lang: string;
  span: Span;
}

export interface ProjectBlock {
  items: ProjectionItem[];
  /** §2.15（BNF v1.5）投射规则组：按项类型批量映射，显式映射优先（R1 遮蔽原则） */
  rules: ProjectionRule[];
  span: Span;
}

export interface File {
  file: string;
  items: Item[];
  project?: ProjectBlock;
  scale?: { mode: string; span: Span };
}

// ---- token 树（宏用）----
export type TokenTree =
  | { t: 'tok'; tok: { kind: string; text: string; value?: unknown; line: number; col: number } }
  | { t: 'delim'; open: string; close: string; items: TokenTree[] };
