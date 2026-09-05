//! # 类型检查与严格性校验
//!
//! 实现编译期绝对防线（BNF §5.1 的 S1-S8）与拓扑/投射校验（G/P 系列）。
//!
//! v0.1 → v0.1.1 演进：
//! - 【修复】项名注册缺失导致 project{} 对 graph/struct/enum 等投射目标误报 P3
//! - 【修复】SymbolTable::declare 丢失 name 字段
//! - 【修复】check_graph 重复 match 分支
//! - 【新增】S1 零隐式转换（if/while 条件字面量非 bool）
//! - 【新增】S2 裸 .unwrap() Lint 警告
//! - 【新增】S4 不可变绑定赋值检查
//! - 【新增】S6 AgentLoop 内 match 禁 `_` 通配 + 枚举穷尽性校验（enum 注册表）
//! - 【新增】S7 未使用即错误（let 绑定 / import / graph node；`_` 前缀与 glob import 豁免）
//! - 【新增】S8 变量遮蔽（同作用域错误 / 跨作用域警告）
//! - 完整类型推导与 trait 求解仍留待 P3+（S3 强制错误处理需要返回类型数据流）

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::diagnostics::{DiagCode, Diagnostic, Diagnostics};

/// v0.2.54 S-15：12 种整型域边界（i128 容量内判定；u128 以 i128 上界为保守子集）
pub fn int_domain_limits(name: &str) -> Option<(i128, i128)> {
    Some(match name {
        "i8" => (-128, 127),
        "i16" => (-32768, 32767),
        "i32" => (-2147483648, 2147483647),
        "i64" | "isize" => (i64::MIN as i128, i64::MAX as i128),
        "i128" => (i128::MIN, i128::MAX),
        "u8" => (0, 255),
        "u16" => (0, 65535),
        "u32" => (0, 4294967295),
        "u64" | "usize" => (0, u64::MAX as i128),
        "u128" => (0, i128::MAX), // i128 上界即为 u128 域的保守子集判定（超界必非法）
        _ => return None,
    })
}

/// v0.2.54 S-15：字面量后缀 → 域名（250u8 → "u8"）
fn int_suffix_domain(s: IntSuffix) -> Option<&'static str> {
    Some(match s {
        IntSuffix::I8 => "i8",
        IntSuffix::I16 => "i16",
        IntSuffix::I32 => "i32",
        IntSuffix::I64 => "i64",
        IntSuffix::I128 => "i128",
        IntSuffix::Isize => "isize",
        IntSuffix::U8 => "u8",
        IntSuffix::U16 => "u16",
        IntSuffix::U32 => "u32",
        IntSuffix::U64 => "u64",
        IntSuffix::U128 => "u128",
        IntSuffix::Usize => "usize",
    })
}

/// v0.2.54 S-15：单段整型注解 → 域名（非整型/多段/泛型 → None）
fn annotation_domain(ty: &Type) -> Option<String> {
    let TypeKind::Path(pt) = &ty.kind else { return None };
    if pt.path.leading_colon || pt.path.segments.len() != 1 || !pt.generic_args.is_empty() {
        return None;
    }
    let n = &pt.path.segments[0].name;
    if int_domain_limits(n).is_some() {
        Some(n.clone())
    } else {
        None
    }
}

/// 绑定来源（S7 报告文案 / 豁免策略使用）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    /// `let` 绑定与模式解构绑定 —— S7 追踪
    Let,
    /// 函数/闭包参数 —— S7 豁免（签名即契约）
    Param,
    /// graph 体内 node/let 声明的拓扑节点 —— S7 追踪
    GraphNode,
}

/// 符号表：作用域栈
#[derive(Debug, Default)]
pub struct SymbolTable {
    scopes: Vec<HashMap<String, Symbol>>,
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub mutable: bool,
    pub ty: TypeKind,
    pub used: bool,
    pub kind: SymbolKind,
    pub span: Span,
    /// v0.2.53 S-14：静态字面量类型事实（let 声明处记录，二元运算检查用；
    /// None = 动态值/不可判，保守放行 —— 与 dhv-ts 的 LitTy 口径一致）
    pub lit_ty: Option<SymbolLitTy>,
    /// v0.2.54 S-15：静态可折叠整数值（i128；None = 不可折叠/动态值）
    pub lit_val: Option<i128>,
    /// v0.2.54 S-15：整型域事实（注解名，如 "u8"/"i64"；来源：let 注解/字面量后缀/cast）
    pub dom: Option<String>,
}

/// S-14 字面量类型域（与 dhv-ts LitTy 同构）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolLitTy {
    Int,
    Float,
    Bool,
    Str,
    Char,
}

/// import 符号（S7 追踪；glob `import * as m` 豁免）
#[derive(Debug, Clone)]
struct ImportSym {
    name: String,
    span: Span,
    used: bool,
}

impl SymbolTable {
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }
    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }
    pub fn declare(
        &mut self,
        name: &str,
        mutable: bool,
        ty: TypeKind,
        kind: SymbolKind,
        span: Span,
    ) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(
                name.to_string(),
                Symbol {
                    name: name.to_string(),
                    mutable,
                    ty,
                    used: false,
                    kind,
                    span,
                    lit_ty: None,
                    lit_val: None,
                    dom: None,
                },
            );
        }
    }
    /// 查找并标记使用
    pub fn lookup(&mut self, name: &str) -> Option<&mut Symbol> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(sym) = scope.get_mut(name) {
                sym.used = true;
                return Some(sym);
            }
        }
        None
    }
    /// 只查不改（不标记 used）
    pub fn peek(&self, name: &str) -> Option<&Symbol> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }
    /// v0.2.53 S-14：只查 lit_ty 不标记 used（类型检查非使用语义）
    pub fn peek_lit_ty(&self, name: &str) -> Option<SymbolLitTy> {
        self.scopes.iter().rev().find_map(|s| s.get(name)).and_then(|sym| sym.lit_ty)
    }
    /// v0.2.53 S-14：更新当前作用域符号的 lit_ty（let 声明处调用）
    pub fn set_lit_ty(&mut self, name: &str, lit_ty: SymbolLitTy) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(sym) = scope.get_mut(name) {
                sym.lit_ty = Some(lit_ty);
                return;
            }
        }
    }
    /// v0.2.54 S-14（v3）：重赋值更新字面量事实（None = 清除，保守放行）
    pub fn set_lit_facts(
        &mut self,
        name: &str,
        lit_ty: Option<SymbolLitTy>,
        lit_val: Option<i128>,
        dom: Option<String>,
    ) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(sym) = scope.get_mut(name) {
                sym.lit_ty = lit_ty;
                sym.lit_val = lit_val;
                sym.dom = dom;
                return;
            }
        }
    }
    /// v0.2.54 S-15：只查折叠值/域（不标记 used）
    pub fn peek_lit_val(&self, name: &str) -> Option<i128> {
        self.scopes.iter().rev().find_map(|s| s.get(name)).and_then(|sym| sym.lit_val)
    }
    pub fn peek_dom(&self, name: &str) -> Option<String> {
        self.scopes.iter().rev().find_map(|s| s.get(name)).and_then(|sym| sym.dom.clone())
    }
    pub fn set_lit_val(&mut self, name: &str, v: i128) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(sym) = scope.get_mut(name) {
                sym.lit_val = Some(v);
                return;
            }
        }
    }
    pub fn set_dom(&mut self, name: &str, dom: String) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(sym) = scope.get_mut(name) {
                sym.dom = Some(dom);
                return;
            }
        }
    }
    pub fn current_scope_has(&self, name: &str) -> bool {
        self.scopes.last().map(|s| s.contains_key(name)).unwrap_or(false)
    }
    pub fn outer_scope_has(&self, name: &str) -> bool {
        if self.scopes.len() < 2 {
            return false;
        }
        self.scopes[..self.scopes.len() - 1]
            .iter()
            .rev()
            .any(|s| s.contains_key(name))
    }
}

/// 编译会话：跨文件符号 + 诊断
pub struct TypeChecker {
    pub diags: Diagnostics,
    pub symbols: SymbolTable,
    /// 文件内定义（含 import）的项名 —— P3 投射目标存在性检查
    pub declared_items: Vec<String>,
    /// enum 注册表：enum 名 → 变体名列表（S6 穷尽性校验；linker 注入跨模块条目）
    enums: HashMap<String, Vec<String>>,
    /// 静态资源清单（block/static 名）：P4 跨模块投射合法性（linker 注入）
    static_resources: HashSet<String>,
    /// 依赖模块导出项（名, 类型串）：§2.15 rules 跨模块展开（linker 注入）
    module_items: Vec<(String, String)>,
    /// M3: 模块导出名集合（模块显示路径 → 导出项名集合）
    module_exports: HashMap<String, HashSet<String>>,
    /// 命名 import 符号（S7 追踪）
    imports: Vec<ImportSym>,
    /// graph AgentLoop 嵌套深度（S6 强化上下文）
    in_agent_loop: usize,
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker {
            diags: Diagnostics::new(),
            symbols: SymbolTable::default(),
            declared_items: Vec::new(),
            enums: HashMap::new(),
            static_resources: HashSet::new(),
            module_items: Vec::new(),
            module_exports: HashMap::new(),
            imports: Vec::new(),
            in_agent_loop: 0,
        }
    }

    /// 对整个文件执行全部检查（根文件入口：含 Scale / Project / S 系列）
    pub fn check_file(&mut self, file: &SourceFile) -> &Diagnostics {
        // E-1: 顶层重名检查（对齐 dhv-ts checker.ts E-001）
        self.check_e1_duplicate_items(file);
        // Pass 0: 收集项名 / enum 注册表 / import 符号
        for top in &file.items {
            if let TopLevel::Item(item) = top {
                self.collect_item(item);
            }
        }
        // Pass 1: 逐项检查
        self.symbols.push_scope();
        for top in &file.items {
            match top {
                TopLevel::Item(item) => self.check_item(item),
                TopLevel::Scale(decl) => self.check_scale(decl),
                TopLevel::Project(proj) => self.check_project(proj, file),
            }
        }
        self.symbols.pop_scope();
        // S7: 未使用 import
        self.report_unused_imports();
        &self.diags
    }

    /// 对依赖模块执行体级 S 系列检查（对齐 dhv-ts：先链接后逐文件检查）。
    ///
    /// 跨模块注册表（enums / static_resources / module_items / diags）保持共享，
    /// 每文件状态（symbols / imports / declared_items / in_agent_loop）独立重置。
    /// 不检查 Scale / Project（仅根文件拥有）。
    pub fn check_module_body(&mut self, module_name: &str, file: &SourceFile) {
        // 重置每文件状态
        self.symbols = SymbolTable::default();
        self.imports = Vec::new();
        self.declared_items = Vec::new();
        self.in_agent_loop = 0;

        let diag_start = self.diags.items.len();

        // E-1: 顶层重名检查（对齐 dhv-ts checker.ts E-001）
        self.check_e1_duplicate_items(file);

        // Pass 0: 收集项名 / import 符号（enum 已由 harvest_module 收集）
        for top in &file.items {
            if let TopLevel::Item(item) = top {
                self.collect_item(item);
            }
        }
        // Pass 1: 逐项检查（降入 fn / graph / impl 体）
        self.symbols.push_scope();
        for top in &file.items {
            if let TopLevel::Item(item) = top {
                self.check_item(item);
            }
        }
        self.symbols.pop_scope();
        // S7: 该模块的未使用 import
        self.report_unused_imports();

        // 为本模块产生的诊断标记文件名（多文件渲染需要）
        for d in &mut self.diags.items[diag_start..] {
            d.file_hint = module_name.to_string();
        }
    }

    // ------------------------------------------------------------------
    // E-1: 顶层重名检查（对齐 dhv-ts checker.ts E-001）
    // ------------------------------------------------------------------

    /// 检查同一文件内是否存在重复的顶层项名。
    ///
    /// 作用域：每文件独立（不跨模块），与 dhv-ts 行为一致。
    /// 首次定义静默接受，后续重复定义报错。
    /// 跳过 import / impl / macro_call（无独立项名）。
    fn check_e1_duplicate_items(&mut self, file: &SourceFile) {
        let mut seen: HashSet<String> = HashSet::new();
        for top in &file.items {
            if let TopLevel::Item(item) = top {
                let name = match item {
                    Item::Import(_) | Item::Impl(_) | Item::MacroCall { .. } => continue,
                    Item::Export(e) => e.item.name(),
                    _ => item.name(),
                };
                let ident = match name {
                    Some(n) => n,
                    None => continue,
                };
                if seen.contains(&ident.name) {
                    self.diags.push(
                        Diagnostic::error(
                            DiagCode::NameResolution("E1"),
                            format!("重复定义顶层项 \"{}\"", ident.name),
                            ident.span,
                        )
                        .note("首个定义已在此文件前方声明，移除此重复项或重命名"),
                    );
                } else {
                    seen.insert(ident.name.clone());
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Pass 0: 收集
    // ------------------------------------------------------------------

    /// 模块注册表收集（linker 调用，检查根文件前）：
    /// 仅收集依赖模块 **导出** 的 enum 变体与静态资源 —— 跨模块 S6/P4 语义。
    /// 不动 declared_items（P3 目标存在性仍由根文件自身 + 其 import 决定）。
    /// M3: 模块导出名收集（linker 调用，检查根文件前）
    pub fn harvest_module(&mut self, module_path: &str, file: &SourceFile) {
        // M3: 收集该模块的 export 名集合
        let mut exported: HashSet<String> = HashSet::new();
        for top in &file.items {
            if let TopLevel::Item(item) = top {
                if let Item::Export(exp) = item {
                    if let Some(name) = exp.item.name() {
                        exported.insert(name.name.clone());
                    }
                }
            }
        }
        self.module_exports.insert(module_path.to_string(), exported);

        for top in &file.items {
            if let TopLevel::Item(item) = top {
                self.harvest_module_item(item);
            }
        }
    }

    fn harvest_module_item(&mut self, item: &Item) {
        match item {
            Item::Export(exp) => self.harvest_module_item(&exp.item),
            Item::Enum(e) => {
                let variants: Vec<String> =
                    e.variants.iter().map(|v| v.name.name.clone()).collect();
                self.enums.entry(e.name.name.clone()).or_insert(variants);
                self.module_items.push((e.name.name.clone(), "enum".to_string()));
            }
            Item::StaticResource(r) => {
                self.static_resources.insert(r.name.name.clone());
                self.module_items.push((r.name.name.clone(), "block".to_string()));
            }
            Item::Fn(fn_) => self.module_items.push((fn_.name.name.clone(), "fn".to_string())),
            Item::Struct(st) => self.module_items.push((st.name.name.clone(), "struct".to_string())),
            Item::Trait(t) => self.module_items.push((t.name.name.clone(), "trait".to_string())),
            Item::Const(c) => self.module_items.push((c.name.name.clone(), "const".to_string())),
            Item::TypeAlias(a) => self.module_items.push((a.name.name.clone(), "type".to_string())),
            Item::Graph(g) => self.module_items.push((g.name.name.clone(), "graph".to_string())),
            _ => {}
        }
    }

    /// M3: import 名必须被源模块 export（对齐 dhv-ts checker.ts M3）
    pub fn check_m3_imports(&mut self, file_name: &str, file: &SourceFile) {
        let root_dir = std::path::Path::new(file_name)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("."));

        for top in &file.items {
            if let TopLevel::Item(Item::Import(imp)) = top {
                if imp.from.is_empty() {
                    continue; // std virtual modules
                }

                // 解析模块路径（与 linker::resolve 同逻辑）
                let resolved = match root_dir.join(&imp.from).canonicalize() {
                    Ok(p) => p.display().to_string(),
                    Err(_) => continue, // M2 already reported by linker
                };

                let exported = match self.module_exports.get(&resolved) {
                    Some(s) => s,
                    None => continue, // module not in export map
                };

                // 收集导入的原始名（跳过 namespace/glob import）
                let names: Vec<&Ident> = match &imp.spec {
                    ImportSpec::Named(items) => items.iter().map(|it| &it.name).collect(),
                    ImportSpec::Single(it) => vec![&it.name],
                    ImportSpec::Namespace { .. } => vec![], // glob: skip
                };

                for name in names {
                    if !exported.contains(&name.name) {
                        let module_short = std::path::Path::new(&resolved)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| imp.from.clone());
                        self.diags.push(
                            Diagnostic::error(
                                DiagCode::NameResolution("M3"),
                                format!(
                                    "import 失败：`{}` 未被 {} export",
                                    name.name, module_short
                                ),
                                name.span,
                            )
                            .note(format!(
                                "在 {} 中添加 `export {}` 使其对本模块可见",
                                module_short, name.name
                            )),
                        );
                    }
                }
            }
        }
    }

    fn collect_item(&mut self, item: &Item) {
        match item {
            Item::Enum(e) => {
                self.declared_items.push(e.name.name.clone());
                let variants: Vec<String> =
                    e.variants.iter().map(|v| v.name.name.clone()).collect();
                self.enums.insert(e.name.name.clone(), variants);
            }
            Item::Import(imp) => {
                // `A as B` → B 入表；S7 追踪别名
                let items: Vec<&ImportItem> = match &imp.spec {
                    ImportSpec::Named(items) => items.iter().collect(),
                    ImportSpec::Single(it) => vec![it],
                    ImportSpec::Namespace { .. } => vec![],
                };
                for it in items {
                    let bound = it.alias.as_ref().unwrap_or(&it.name);
                    self.declared_items.push(bound.name.clone());
                    self.imports.push(ImportSym {
                        name: bound.name.clone(),
                        span: bound.span,
                        used: false,
                    });
                }
                if let ImportSpec::Namespace { alias } = &imp.spec {
                    // glob import：注册名（P3 可见）但豁免 S7
                    self.declared_items.push(alias.name.clone());
                }
            }
            _ => {
                if let Some(name) = item.name() {
                    self.declared_items.push(name.name.clone());
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Pass 1: 项级检查
    // ------------------------------------------------------------------

    fn check_item(&mut self, item: &Item) {
        match item {
            Item::Graph(g) => self.check_graph(g),
            Item::Fn(f) => self.check_fn(f),
            Item::Impl(i) => {
                if let Some(t) = &i.trait_ty {
                    self.walk_type(t);
                }
                self.walk_type(&i.self_ty);
                for it in &i.items {
                    match it {
                        ImplItem::Fn(f) => self.check_fn(f),
                        ImplItem::Const(c) => {
                            self.walk_type(&c.ty);
                            self.walk_expr(&c.value);
                        }
                        ImplItem::TypeAlias(a) => {
                            self.walk_type(&a.ty);
                        }
                    }
                }
            }
            Item::Struct(s) => {
                if let StructKind::Named(fields) | StructKind::Tuple(fields) = &s.kind {
                    for f in fields {
                        self.walk_type(&f.ty);
                    }
                }
            }
            Item::Enum(e) => {
                for v in &e.variants {
                    if let StructKind::Named(fields) | StructKind::Tuple(fields) = &v.fields {
                        for f in fields {
                            self.walk_type(&f.ty);
                        }
                    }
                }
            }
            Item::Trait(t) => {
                for b in &t.supertraits {
                    self.walk_type(&b.ty);
                }
                for it in &t.items {
                    match it {
                        TraitItem::FnSig(sig) => {
                            for p in &sig.params {
                                self.walk_type(&p.ty);
                            }
                            if let Some(ret) = &sig.ret {
                                self.walk_type(ret);
                            }
                        }
                        TraitItem::Fn(f) => self.check_fn(f),
                        TraitItem::Const(c) => {
                            self.walk_type(&c.ty);
                            self.walk_expr(&c.value);
                        }
                        TraitItem::TypeAlias(a) => {
                            self.walk_type(&a.ty);
                        }
                    }
                }
            }
            Item::Const(c) => {
                self.walk_type(&c.ty);
                self.walk_expr(&c.value);
                // v0.2.53 S-13：const 整型域字面量校验（与 let 同规则；双编译器一致）
                self.check_int_literal_range(&c.ty, &c.value);
            }
            Item::TypeAlias(a) => {
                self.walk_type(&a.ty);
            }
            Item::Export(exp) => self.check_item(&exp.item),
            Item::StaticResource(r) => {
                // block/static 体内 {{expr}} 插值 — 标记导入使用（对齐 dhv-ts regex 扫描）
                for part in &r.content {
                    if let RawContentPart::Interpolation { expr, .. } = part {
                        self.walk_expr(expr);
                    }
                }
            }
            Item::Import(_) | Item::MacroRules(_) => {}
            // 语句级宏调用（println!(..); 等）：实参 token 树里的标识符按名使用，
            // 防 S7 误报（agent.hsl `println!("...", p.total_len())` 类形态）
            Item::MacroCall { args, .. } => {
                let mut words = Vec::new();
                collect_token_idents(&args.tokens, &mut words);
                for word in words {
                    let _ = self.symbols.lookup(&word);
                    self.mark_import_used(&word);
                }
            }
        }
    }

    /// S1/S2/S4/S7/S8: 函数体检查
    fn check_fn(&mut self, f: &FnDef) {
        self.symbols.push_scope();
        for p in &f.params {
            self.walk_type(&p.ty);
            if let ParamKind::Pattern(pat) = &p.kind {
                self.walk_pattern(pat, SymbolKind::Param);
            }
        }
        if let Some(ret) = &f.ret {
            self.walk_type(ret);
        }
        if let Some(body) = &f.body {
            self.walk_block_inner(body);
        }
        self.pop_scope_report();
    }

    /// G1-G6 拓扑校验 + graph 体内 S 系列检查
    fn check_graph(&mut self, graph: &GraphDef) {
        let span = graph.span;
        // 遍历参数类型和返回类型（S7 导入使用标记）
        for p in &graph.params {
            self.walk_type(&p.ty);
            if let ParamKind::Pattern(pat) = &p.kind {
                self.walk_pattern(pat, SymbolKind::Param);
            }
        }
        if let Some(ret) = &graph.ret {
            self.walk_type(ret);
        }
        self.symbols.push_scope();
        let mut has_agent_loop = false;
        let mut declared_nodes: Vec<String> = Vec::new();
        let mut node_spans: HashMap<String, Span> = HashMap::new();

        // Pass A: 注册 node/let 声明（G2 端点表 + S7 追踪）
        for stmt in &graph.body {
            match stmt {
                GraphStmt::Node(n) => {
                    declared_nodes.push(n.name.name.clone());
                    node_spans.insert(n.name.name.clone(), n.name.span);
                    self.symbols.declare(
                        &n.name.name,
                        n.mutable,
                        n.ty.kind.clone(),
                        SymbolKind::GraphNode,
                        n.name.span,
                    );
                    if let Some(init) = &n.init {
                        self.walk_expr(init);
                    }
                    self.walk_type(&n.ty);
                }
                GraphStmt::Let(l) => {
                    if let PatternKind::Ident { name, .. } = &l.pattern.kind {
                        declared_nodes.push(name.name.clone());
                    }
                    self.check_let(l);
                }
                _ => {}
            }
        }

        // Pass B: 边与语句
        let mut edge_nodes: HashSet<String> = HashSet::new();
        let mut edge_list: Vec<(String, String, bool)> = Vec::new(); // (from, to, guarded) — G-3 用
        // G-8（v0.2.53）：重复边声明判重 —— 同 (from, to, 守卫指纹) 二次声明报错。
        // 实证：同一条 edge 复制两遍静默通过，拓扑统计（边数/覆盖率分母/变异基线）
        // 直接翻倍污染。守卫指纹：pattern 用 Debug 序列化；expr 守卫用源码位置
        // （同位置 + 同端点 ≡ 复制粘贴）—— 保守口径，不误报合法的同向多守卫并行边。
        let mut edge_seen: std::collections::HashMap<String, Span> = std::collections::HashMap::new();
        for stmt in &graph.body {
            match stmt {
                GraphStmt::Edge(edge) => {
                    let guarded = edge.on.is_some();
                    for i in 0..edge.endpoints.len().saturating_sub(1) {
                        let from = edge.endpoints[i].last().name.clone();
                        let to = edge.endpoints[i + 1].last().name.clone();
                        let guard_fp = match &edge.on {
                            Some(EdgeGuard::Pattern(p)) => format!("pat:{}", pattern_fingerprint(&p.kind)),
                            Some(EdgeGuard::Expr(e)) => format!("expr@{}:{}", e.span.start, e.span.end),
                            None => "unguarded".to_string(),
                        };
                        let key = format!("{from}->{to}|{guard_fp}");
                        if edge_seen.contains_key(&key) {
                            self.diags.push(
                                Diagnostic::error(
                                    DiagCode::Topology("G8"),
                                    format!("重复边声明：{from} -> {to}（拓扑统计将翻倍污染；同向多守卫请用不同 Guard 变体）"),
                                    edge.span,
                                )
                                .note("同 (from, to, 守卫) 的边重复声明会使拓扑统计/覆盖率分母翻倍"),
                            );
                        } else {
                            edge_seen.insert(key, edge.span);
                        }
                        edge_list.push((from, to, guarded));
                    }
                    for ep in &edge.endpoints {
                        let last = ep.last().name.clone();
                        let _ = self.symbols.lookup(&last); // 端点引用即使用
                        edge_nodes.insert(last.clone());
                        if !declared_nodes.contains(&last) {
                            self.diags.push(
                                Diagnostic::error(
                                    DiagCode::Topology("G2"),
                                    format!("edge 端点 `{last}` 未在 graph 体内声明（需先 node/let 声明）"),
                                    ep.span,
                                )
                                .note(format!("在此 graph 内添加声明：`node {last}: <类型>;`")),
                            );
                        }
                    }
                    match &edge.on {
                        Some(EdgeGuard::Pattern(p)) => {
                            self.walk_pattern(p, SymbolKind::Let);
                        }
                        Some(EdgeGuard::Expr(e)) => self.walk_expr(e),
                        None => {}
                    }
                }
                GraphStmt::Stmt(Stmt::Expr { expr, .. }) => {
                    if let ExprKind::Loop { body, .. } = &expr.kind {
                        has_agent_loop = true;
                        self.in_agent_loop += 1;
                        self.walk_block_inner(body);
                        self.in_agent_loop -= 1;
                    } else {
                        self.walk_expr(expr);
                    }
                }
                GraphStmt::Stmt(s) => self.walk_stmt(s),
                GraphStmt::Item(i) => self.check_item(i),
                GraphStmt::Node(_) | GraphStmt::Let(_) => {} // Pass A 已处理
            }
        }

        // G-3: 无条件环检测（编译期可判定死锁）
        let mut adj: HashMap<String, Vec<(String, bool)>> = HashMap::new();
        for (from, to, guarded) in &edge_list {
            adj.entry(from.clone()).or_default().push((to.clone(), *guarded));
        }
        for start in adj.keys() {
            // DFS 寻找回连 start 的路径，且路径上无 guard
            let mut stack: Vec<(String, bool)> = vec![(start.clone(), false)];
            let mut visited: HashSet<String> = HashSet::new();
            while let Some((node, any_guard)) = stack.pop() {
                // 回到起点但路径上有 guard → 跳过（不是无条件环）
                if node == *start && any_guard && stack.is_empty() && !visited.is_empty() {
                    continue;
                }
                if let Some(edges) = adj.get(&node) {
                    for (to, guarded) in edges {
                        if to == start {
                            if !any_guard && !guarded {
                                self.diags.push(
                                    Diagnostic::error(
                                        DiagCode::Topology("G3"),
                                        format!("拓扑存在无条件环：{} -> ... -> {}", start, start),
                                        span,
                                    )
                                    .note("无条件环意味着运行时必然死锁；给环上至少一条 edge 添加 `on Guard` 条件"),
                                );
                                stack.clear();
                                break;
                            }
                            continue;
                        }
                        let key = format!("{}->{}", node, to);
                        if visited.contains(&key) {
                            continue;
                        }
                        visited.insert(key);
                        stack.push((to.clone(), any_guard || *guarded));
                    }
                }
            }
        }

        // G4: 孤岛节点警告（声明了 node 但无任何 edge 引用）
        for n in &declared_nodes {
            if !edge_nodes.contains(n) {
                if let Some(&sp) = node_spans.get(n) {
                    self.diags.push(
                        Diagnostic::warning(
                            DiagCode::Topology("G4"),
                            format!("节点 `{n}` 没有任何 edge（孤岛节点）"),
                            sp,
                        )
                        .note("若为插件注入位请添加注释说明；否则检查是否遗漏了连接此节点的 edge"),
                    );
                }
            }
        }

        // G1: 必须至少一个 AgentLoop
        if !has_agent_loop {
            self.diags.push(
                Diagnostic::error(
                    DiagCode::Topology("G1"),
                    format!("graph `{}` 必须包含至少一个 `loop`（Agent 核心循环）", graph.name),
                    span,
                )
                .note("Agent 循环是 graph 的语义核心：loop + match Action 强制处理所有分支"),
            );
        }
        self.pop_scope_report();
    }

    fn check_scale(&mut self, decl: &ScaleDecl) {
        if let ScaleMode::Custom(mode) = &decl.mode {
            self.diags.push(
                Diagnostic::warning(
                    DiagCode::Projection("P6"),
                    format!("scale 模式 `{mode}` 未在编译器注册，将回退为 monolith"),
                    decl.span,
                )
                .note("已注册模式: monolith | microkernel"),
            );
        }
    }

    /// P1-P4 投射一致性
    fn check_project(&mut self, proj: &ProjectBlock, file: &SourceFile) {
        let mut seen_paths: HashMap<String, String> = HashMap::new();
        for p in &proj.projections {
            // P2: 路径唯一
            if let Some(existing) = seen_paths.get(&p.path) {
                self.diags.push(
                    Diagnostic::error(
                        DiagCode::Projection("P2"),
                        format!(
                            "物理路径 `{}` 被两个投射项占据：`{existing}` 与 `{}`",
                            p.path,
                            p.target.last().name
                        ),
                        p.span,
                    )
                    .note(format!("为 `{}` 选择不同的目标路径以避免冲突", p.target.last().name)),
                );
            } else {
                seen_paths.insert(p.path.clone(), p.target.last().name.clone());
            }
            // P3: 目标项必须存在
            let target_name = p.target.last().name.clone();
            if !self.declared_items.contains(&target_name) {
                self.diags.push(
                    Diagnostic::error(
                        DiagCode::Projection("P3"),
                        format!("投射目标 `{target_name}` 在本文件中未定义（或未 import）"),
                        p.span,
                    )
                    .note(format!("确认 `{target_name}` 已在本文件定义，或通过 `import` 引入")),
                );
            } else {
                self.mark_import_used(&target_name);
            }
            // P4: block 只能投射静态后端；逻辑项只能投射代码后端
            // 静态资源判定：本文件直接定义 + 依赖模块导出（linker 注入注册表）
            let is_block_target = self.static_resources.contains(&target_name)
                || file
                    .items
                    .iter()
                    .any(|t| matches!(t, TopLevel::Item(Item::StaticResource(r)) if r.name.name == target_name));
            let lang = p.lang.name.as_str();
            // P-4：后端合法性查 38 语言注册表（BNF v1.4 §5.2）
            match crate::langs::resolve(lang) {
                None => {
                    self.diags.push(
                        Diagnostic::error(
                            DiagCode::Projection("P4"),
                            format!("未注册的后端语言 `{lang}`（注册表见 dhv targets / BNF v1.4 §5.2，共 38 个）"),
                            p.span,
                        )
                        .note("运行 `dhv targets` 查看全部合法后端 id"),
                    );
                }
                Some(spec) => {
                    if is_block_target && spec.tier != 0 {
                        self.diags.push(
                            Diagnostic::error(
                                DiagCode::Projection("P4"),
                                format!("block/static 资源 `{target_name}` 只能投射到静态格式后端 yaml/markdown/json/toml/ini/xml（当前: {lang}）"),
                                p.span,
                            )
                            .note(format!("将 `{target_name}` 的目标后端改为 yaml / json / toml / markdown / ini / xml 之一")),
                        );
                    }
                    if !is_block_target && spec.tier == 0 {
                        self.diags.push(
                            Diagnostic::error(
                                DiagCode::Projection("P4"),
                                format!("代码项 `{target_name}` 不能投射到静态格式 `{lang}`（需编程语言后端）"),
                                p.span,
                            )
                            .note(format!("将 `{target_name}` 的目标后端改为编程语言，如 rust / python / typescript 等")),
                        );
                    }
                }
            }
        }

        // ------------------------------------------------------------------
        // §2.15（BNF v1.5）投射规则组：声明校验 + 跨文件展开校验
        // ------------------------------------------------------------------
        self.check_projection_rules(proj, file, &mut seen_paths);
    }

    /// rules 规则组：R2 占位符 / R3 重复类型 / R4 未知类型 / R1 显式遮蔽 / P2 路径唯一 / P4 层级
    fn check_projection_rules(
        &mut self,
        proj: &ProjectBlock,
        file: &SourceFile,
        seen_paths: &mut HashMap<String, String>,
    ) {
        use std::collections::BTreeMap;
        const KNOWN: [&str; 9] = ["graph", "fn", "struct", "enum", "trait", "const", "type", "block", "static"];
        // 声明校验
        let mut kind_rules: BTreeMap<String, &ProjectionRule> = BTreeMap::new();
        for rule in &proj.rules {
            if !KNOWN.contains(&rule.kind.as_str()) {
                self.diags.push(
                    Diagnostic::error(
                        DiagCode::Projection("P5"),
                        format!(
                            "投射规则类型 `{}` 未注册（支持：graph/fn/struct/enum/trait/const/type/block/static）",
                            rule.kind
                        ),
                        rule.span,
                    )
                    .note("R4：规则类型必须是 graph/fn/struct/enum/trait/const/type/block/static 之一（block 与 static 同义）"),
                );
                continue;
            }
            if kind_rules.contains_key(&rule.kind) {
                self.diags.push(
                    Diagnostic::error(
                        DiagCode::Projection("P5"),
                        format!("投射规则类型 `{}` 重复声明（R3：同一类型只允许一条规则）", rule.kind),
                        rule.span,
                    )
                    .note(format!("R3：每种类型只能有一条规则；删除重复的 `{}` 规则或合并路径", rule.kind)),
                );
                continue;
            }
            // R2：占位符白名单 {name}
            let mut rest = rule.path.as_str();
            while let Some(start) = rest.find('{') {
                if let Some(end) = rest[start..].find('}') {
                    let ph = &rest[start + 1..start + end];
                    if ph != "name" {
                        self.diags.push(
                            Diagnostic::error(
                                DiagCode::Projection("P5"),
                                format!("投射规则路径占位符 `{{{ph}}}` 未注册（v1 仅支持 {{name}}）"),
                                rule.span,
                            )
                            .note("R2：路径模板 v1 仅支持 {name} 占位符，其他占位符暂未注册"),
                        );
                    }
                    rest = &rest[start + end + 1..];
                } else {
                    self.diags.push(
                        Diagnostic::error(
                            DiagCode::Projection("P5"),
                            "投射规则路径模板缺少闭合 `}`".to_string(),
                            rule.span,
                        )
                        .note("检查路径模板中的 `{` 是否有对应的 `}`"),
                    );
                    break;
                }
            }
            kind_rules.insert(rule.kind.clone(), rule);
        }
        if kind_rules.is_empty() {
            return;
        }
        // 展开池：根文件项在前，依赖模块导出项在后（同名时根文件优先）
        let mut seen_names: HashSet<String> = HashSet::new();
        let mut pool: Vec<(String, String)> = Vec::new(); // (name, kind)
        for top in &file.items {
            if let TopLevel::Item(item) = top {
                if let (Some(name), Some(kind)) = (item.name(), item_kind(item)) {
                    if seen_names.insert(name.name.clone()) {
                        pool.push((name.name.clone(), kind.to_string()));
                    }
                }
            }
        }
        for (name, kind) in self.module_items.clone() {
            if seen_names.insert(name.clone()) {
                pool.push((name, kind));
            }
        }
        // 显式映射名集合（R1 遮蔽原则：显式映射优先）
        let explicit: HashSet<String> = proj
            .projections
            .iter()
            .map(|p| p.target.last().name.clone())
            .collect();
        for (name, kind) in pool {
            if explicit.contains(&name) {
                continue;
            }
            // impl / import / macro 无名项或不参与自动投射的类型
            let rule_kind = normalize_kind(&kind);
            let Some(rule) = kind_rules.get(&rule_kind) else {
                continue;
            };
            let path = rule.path.replace("{name}", &name);
            // P2 路径唯一
            if let Some(existing) = seen_paths.get(&path) {
                self.diags.push(Diagnostic::error(
                    DiagCode::Projection("P2"),
                    format!(
                        "物理路径 `{}` 被两个投射项占据：`{existing}` 与 `{name}`（rules 展开）",
                        path
                    ),
                    rule.span,
                ).note(format!("为 {name} 或 rules 路径模板选择不同的目标以避免冲突")));

            } else {
                seen_paths.insert(path.clone(), name.clone());
            }
            // P4 层级
            match crate::langs::resolve(rule.lang.name.as_str()) {
                None => {
                    self.diags.push(Diagnostic::error(
                        DiagCode::Projection("P4"),
                        format!(
                            "未注册的后端语言 `{}`（rules 展开，项 {name}）",
                            rule.lang.name
                        ),
                        rule.span,
                    ).note("运行 dhv targets 查看全部合法后端 id"));

                }
                Some(spec) => {
                    let is_block = rule_kind == "block";
                    if is_block && spec.tier != 0 {
                        self.diags.push(Diagnostic::error(
                            DiagCode::Projection("P4"),
                            format!(
                                "block/static 资源 `{name}` 只能投射到静态格式后端（rules 展开，当前: {0}）",
                                rule.lang.name
                            ),
                            rule.span,
                        ).note("将 rules 的 lang 改为 yaml / json / toml / markdown / ini / xml 之一"));

                    }
                    if !is_block && spec.tier == 0 {
                        self.diags.push(Diagnostic::error(
                            DiagCode::Projection("P4"),
                            format!(
                                "代码项 `{name}` 不能投射到静态格式 `{0}`（rules 展开，需编程语言后端）",
                                rule.lang.name
                            ),
                            rule.span,
                        ).note("将 rules 的 lang 改为编程语言，如 rust / python / typescript 等"));

                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // 语句 / 表达式遍历（S 系列检查主体）
    // ------------------------------------------------------------------

    /// v0.2.53 S-13：整型域字面量校验。
    /// rustc 真机对拍实证：`let x: i8 = 300` 在 check 双端放行、emit rust 后
    /// rustc 报 literal out of range —— 跨后端语义漂移（python/js 静默放行）。
    /// 静态拦截：注解为 12 种整型之一且 init 为（可带负号的）整数字面量时，
    /// 值必须落在注解类型域内。非字面量不判（BigInt 任意精度为既定设计）。
    fn check_int_literal_range(&mut self, ty: &Type, init: &Expr) {
        let TypeKind::Path(pt) = &ty.kind else { return };
        if pt.path.leading_colon || pt.path.segments.len() != 1 || !pt.generic_args.is_empty() {
            return;
        }
        let name = &pt.path.segments[0].name;
        let Some((lo, hi)) = int_domain_limits(name) else { return };
        // 展开一元负号（u* 域外负值同样在此拦截）
        let (neg, inner): (bool, &Expr) = match &init.kind {
            ExprKind::Unary { op: UnaryOp::Neg, operand } => (true, &**operand),
            _ => (false, init),
        };
        let ExprKind::Literal(lit) = &inner.kind else { return };
        let LiteralKind::Int { value, overflow, .. } = &lit.kind else { return };
        // v0.2.54 L-10：超 i128 容量字面量（S-16 另拒）此处不再误判域内
        if *overflow { return; }
        let v: i128 = if neg { -(*value as i128) } else { *value as i128 };
        if v < lo || v > hi {
            self.diags.push(
                Diagnostic::error(
                    DiagCode::Strictness("S13"),
                    format!("整数字面量 {v} 超出 {name} 域 [{lo}, {hi}]（rustc 后端将拒绝编译，python/js 后端静默放行 —— 跨后端漂移；显式截断请用 as）"),
                    init.span,
                ),
            );
        }
    }

    /// v0.2.53 S-14：表达式的静态字面量类型（lit / 一元负号包裹 / 显式 cast 目标 /
    /// 单段 path 查符号表 lit_ty）—— 与 dhv-ts litTypeOf 同构。不可判 → None。
    fn expr_lit_ty(&self, e: &Expr) -> Option<SymbolLitTy> {
        match &e.kind {
            ExprKind::Literal(l) => match &l.kind {
                LiteralKind::Int { .. } => Some(SymbolLitTy::Int),
                LiteralKind::Float { .. } => Some(SymbolLitTy::Float),
                LiteralKind::Bool(_) => Some(SymbolLitTy::Bool),
                LiteralKind::Str { .. } => Some(SymbolLitTy::Str),
                LiteralKind::Char(_) => Some(SymbolLitTy::Char),
            },
            ExprKind::Unary { op: UnaryOp::Neg | UnaryOp::Not, operand, .. } => self.expr_lit_ty(operand),
            ExprKind::Cast { ty, .. } => {
                // 显式 cast 目标类型可作为静态事实
                let TypeKind::Path(pt) = &ty.kind else { return None };
                if pt.path.leading_colon || pt.path.segments.len() != 1 {
                    return None;
                }
                match pt.path.segments[0].name.as_str() {
                    "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => Some(SymbolLitTy::Int),
                    "f32" | "f64" => Some(SymbolLitTy::Float),
                    "bool" => Some(SymbolLitTy::Bool),
                    "String" | "str" => Some(SymbolLitTy::Str),
                    "char" => Some(SymbolLitTy::Char),
                    _ => None,
                }
            }
            ExprKind::Path(p) if p.segments.len() == 1 && !p.leading_colon => {
                self.symbols.peek_lit_ty(&p.segments[0].name)
            }
            _ => None,
        }
    }

    /// v0.2.54 S-15：整型域事实（cast 目标 / 带后缀字面量 / 作用域声明）—— 与 dhv-ts intDomainOf 同构
    fn int_domain_of(&self, e: &Expr) -> Option<String> {
        match &e.kind {
            ExprKind::Cast { ty, .. } => {
                let TypeKind::Path(pt) = &ty.kind else { return None };
                if pt.path.leading_colon || pt.path.segments.len() != 1 {
                    return None;
                }
                let n = &pt.path.segments[0].name;
                if int_domain_limits(n).is_some() {
                    return Some(n.clone());
                }
                None
            }
            ExprKind::Literal(l) => match &l.kind {
                // 后缀字面量（250u8）；overflow 字面量无域（S-16 另拒）
                LiteralKind::Int { suffix: Some(s), overflow: false, .. } => {
                    int_suffix_domain(*s).map(|d| d.to_string())
                }
                _ => None,
            },
            ExprKind::Path(p) if p.segments.len() == 1 && !p.leading_colon => {
                self.symbols.peek_dom(&p.segments[0].name)
            }
            _ => None,
        }
    }

    /// v0.2.54 S-15：静态可折叠整数值（i128 checked 运算，防检查器自身溢出 panic）
    fn expr_int_val(&self, e: &Expr) -> Option<i128> {
        match &e.kind {
            ExprKind::Literal(l) => match &l.kind {
                LiteralKind::Int { value, overflow: false, .. } => Some(*value),
                _ => None,
            },
            ExprKind::Unary { op: UnaryOp::Neg, operand, .. } => self.expr_int_val(operand).map(|v| v.checked_neg()).flatten(),
            ExprKind::Path(p) if p.segments.len() == 1 && !p.leading_colon => {
                self.symbols.peek_lit_val(&p.segments[0].name)
            }
            ExprKind::Binary { op, lhs, rhs, .. } => {
                use BinaryOp::*;
                if !matches!(op, Add | Sub | Mul | Div | Rem) {
                    return None;
                }
                let l = self.expr_int_val(lhs)?;
                let r = self.expr_int_val(rhs)?;
                let res = match op {
                    Add => l.checked_add(r),
                    Sub => l.checked_sub(r),
                    Mul => l.checked_mul(r),
                    Div => {
                        if r == 0 { return None; } // 除零不折叠（静态另有专项诊断）
                        l.checked_div(r)
                    }
                    Rem => {
                        if r == 0 { return None; }
                        l.checked_rem(r)
                    }
                    _ => return None,
                };
                res // 折叠自身溢出 i128 → None（保守；dhv-ts BigInt 精确折叠但两侧零误报口径一致 —— 超容量的源字面量已由 S-16 拒绝）
            }
            _ => None,
        }
    }

    /// v0.2.54 S-15：静态折叠 + 域已知 → 溢出/除零检查（与 dhv-ts checkDomainArith 同口径）
    fn check_domain_arith(&mut self, op: BinaryOp, lhs: &Expr, rhs: &Expr, span: Span) {
        use BinaryOp::*;
        let (Some(lv), Some(rv)) = (self.expr_int_val(lhs), self.expr_int_val(rhs)) else { return };
        if matches!(op, Div | Rem) && rv == 0 {
            self.diags.push(
                Diagnostic::error(
                    DiagCode::Strictness("S15"),
                    format!(
                        "静态可证除零：{lv} {} 0（interp 运行期 HRuntimeError，rustc 后端 deny(unconditional_panic) 编译期拒绝；python ZeroDivisionError，js 静默 NaN）",
                        op.as_str()
                    ),
                    span,
                ),
            );
            return;
        }
        let dom = self.int_domain_of(lhs).or_else(|| self.int_domain_of(rhs));
        let Some(dom) = dom else { return };
        let Some((lo, hi)) = int_domain_limits(&dom) else { return };
        let res: Option<i128> = match op {
            Add => lv.checked_add(rv),
            Sub => lv.checked_sub(rv),
            Mul => lv.checked_mul(rv),
            Div => lv.checked_div(rv),
            Rem => lv.checked_rem(rv),
            _ => return,
        };
        if let Some(res) = res {
            if res < lo || res > hi {
                self.diags.push(
                    Diagnostic::error(
                        DiagCode::Strictness("S15"),
                        format!(
                            "注解域算术溢出：{lv} {} {rv} = {res} 超出 {dom} 域 [{lo}, {hi}]（interp BigInt 任意精度静默越域、rust 后端环绕/panic —— 跨后端漂移；显式扩域请用 as）",
                            op.as_str()
                        ),
                        span,
                    ),
                );
            }
        }
    }

    /// v0.2.53 S-14：二元运算类型检查（与 dhv-ts checkBinaryOpTypes 同口径）
    fn check_binary_op_types(&mut self, op: BinaryOp, lt: SymbolLitTy, rt: SymbolLitTy, span: Span) {
        use BinaryOp::*;
        use SymbolLitTy::*;
        let fail = |diags: &mut Diagnostics, note: &str| {
            diags.push(
                Diagnostic::error(
                    DiagCode::Strictness("S14"),
                    format!(
                        "二元运算类型不匹配：{} {} {}（{}；rustc 后端编译期拒绝，python/typescript 后端静默放行或产生垃圾值 —— 跨后端漂移）",
                        lit_ty_name(lt), op.as_str(), lit_ty_name(rt), note
                    ),
                    span,
                ),
            );
        };
        let numeric = |t: SymbolLitTy| matches!(t, Int | Float);
        match op {
            Add => {
                // str+str 拼接合法；其余要求同域数值
                if lt == Str && rt == Str { return; }
                if (lt == Str) != (rt == Str) { fail(&mut self.diags, "str 与数值相加：仅 str+str 拼接与数值加法合法"); return; }
                if !numeric(lt) || !numeric(rt) { fail(&mut self.diags, "加法仅数值加法或 str+str 拼接"); return; }
                if (lt == Float) != (rt == Float) { fail(&mut self.diags, "int 与 float 混算需显式 as 转换（S1 零隐式转换）"); return; }
            }
            Sub | Mul | Div | Rem => {
                if !numeric(lt) || !numeric(rt) { fail(&mut self.diags, "算术运算符要求两侧数值（str 重复/拼接请用显式转换或 str 方法）"); return; }
                if (lt == Float) != (rt == Float) { fail(&mut self.diags, "int 与 float 混算需显式 as 转换（S1 零隐式转换）"); return; }
            }
            Eq | Ne | Lt | Gt | Le | Ge => {
                if lt != rt { fail(&mut self.diags, "比较运算两侧类型不同（数值×字符串等跨类比较在 rustc 拒绝，python/typescript 静默给出错误结果）"); return; }
            }
            And | Or => {
                if lt != Bool || rt != Bool { fail(&mut self.diags, "逻辑运算符要求两侧 bool（js 后端静默真值化是语义漂移源）"); return; }
            }
            BitAnd | BitOr | BitXor | Shl | Shr => {
                // 位运算：仅整型域（与 rustc 一致；python 宽松但静态拦截漂移）
                if lt != Int || rt != Int { fail(&mut self.diags, "位运算符要求两侧整型"); return; }
            }
        }
    }

    fn check_let(&mut self, l: &LetStmt) {
        if let Some(ty) = &l.ty {
            self.walk_type(ty);
        }
        if let Some(init) = &l.init {
            self.walk_expr(init);
        }
        // v0.2.53 S-13：整型域字面量校验（跨后端漂移拦截，与 dhv-ts 同规则）
        if let (Some(ty), Some(init)) = (&l.ty, &l.init) {
            self.check_int_literal_range(ty, init);
        }
        if let Some(els) = &l.else_block {
            self.walk_block_inner(els);
        }
        // 声明先于 lit_ty 记录（set_lit_ty 按名查符号表 —— 符号必须已注册；
        // 首版把 S-14 记录放在 declare_binding 之前 → 查无此名静默失效，
        // dhv-ts 报而 dhv 不报的双端不一致 —— conformance 对拍实录）
        let init_lit_ty = l.init.as_ref().and_then(|init| self.expr_lit_ty(init));
        if let PatternKind::Ident { name, .. } = &l.pattern.kind {
            self.declare_binding(name, l.mutable, SymbolKind::Let);
            // v0.2.53 S-14（v2）：let 声明的静态字面量类型记入符号表 ——
            // 后续 path 引用可判（变量中转场景）。
            // v0.2.54 S-14（v3）+ S-15：赋值更新已追踪（见 Assign 臂）；整型
            // 记录还带折叠值 lit_val 与域 dom（注解/后缀来源）。
            if let Some(t) = init_lit_ty {
                self.symbols.set_lit_ty(&name.name, t);
                if t == SymbolLitTy::Int {
                    if let Some(v) = l.init.as_ref().and_then(|init| self.expr_int_val(init)) {
                        self.symbols.set_lit_val(&name.name, v);
                    }
                    let d = l.ty.as_ref().and_then(annotation_domain)
                        .or_else(|| l.init.as_ref().and_then(|init| self.int_domain_of(init)));
                    if let Some(d) = d {
                        self.symbols.set_dom(&name.name, d);
                    }
                }
            }
        } else {
            self.walk_pattern(&l.pattern, SymbolKind::Let);
        }
        // v0.2.54 S-15（let 层）：注解域 + 可折叠算术 init → 结果域检查。
        // S-13 只判纯字面量；这里下沉到折叠算术（250 + 250 对 u8）——
        // 操作数无域时 binary 层查不到，注解域在 let 处才可见。
        if let (Some(ty), Some(init)) = (&l.ty, &l.init) {
            if let ExprKind::Binary { op, .. } = &init.kind {
                if matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem) {
                    if let Some(name) = annotation_domain(ty) {
                        if let Some((lo, hi)) = int_domain_limits(&name) {
                            if let Some(v) = self.expr_int_val(init) {
                                if v < lo || v > hi {
                                    self.diags.push(
                                        Diagnostic::error(
                                            DiagCode::Strictness("S15"),
                                            format!("注解域算术溢出：折叠结果 {v} 超出 {name} 域 [{lo}, {hi}]（interp BigInt 静默越域，rust 后端环绕/panic —— 跨后端漂移；显式扩域请用 as）"),
                                            init.span,
                                        ),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn walk_stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Let(l) => self.check_let(l),
            Stmt::Item(i) => self.check_item(i),
            Stmt::Expr { expr, .. } => self.walk_expr(expr),
            Stmt::Empty(_) => {}
        }
    }

    /// 进入块级作用域（含 S7 收尾报告）
    fn walk_block_inner(&mut self, b: &BlockExpr) {
        self.symbols.push_scope();
        for s in &b.stmts {
            self.walk_stmt(s);
        }
        if let Some(tail) = &b.tail {
            self.walk_expr(tail);
        }
        self.pop_scope_report();
    }

    fn walk_expr(&mut self, e: &Expr) {
        match &e.kind {
            // v0.2.54 S-16（L-10）：超 i128 容量字面量静态拒绝 —— 此前 parser
            // 静默归零（值损坏比溢出更糟），dhv-ts BigInt 精确而 dhv 归零的
            // 双端分歧实录。表达式位置的字面量一律拒绝（与 dhv-ts 同口径）。
            // S-13（v2）：后缀字面量域检查（300u8 —— 注解路径已有，后缀漏拦）。
            ExprKind::Literal(l) => {
                if let LiteralKind::Int { value, suffix, overflow } = &l.kind {
                    if *overflow {
                        self.diags.push(
                            Diagnostic::error(
                                DiagCode::Strictness("S16"),
                                format!(
                                    "整数字面量超出 i128 静态容量：{}（parse 静默归零为 0 —— 值损坏；dhv-ts BigInt 精确解析、rust 后端 i128 无域 —— 静态域分析以 i128 为容量上界，源字面量必须可精确表示）",
                                    l.raw
                                ),
                                e.span,
                            ),
                        );
                    } else if let Some(s) = suffix {
                        if let Some(dom) = int_suffix_domain(*s) {
                            if let Some((lo, hi)) = int_domain_limits(dom) {
                                if *value < lo || *value > hi {
                                    self.diags.push(
                                        Diagnostic::error(
                                            DiagCode::Strictness("S13"),
                                            format!("整数字面量 {value} 超出后缀 {dom} 域 [{lo}, {hi}]（rustc 后端将拒绝编译，python/js 后端静默放行 —— 跨后端漂移；显式截断请用 as）"),
                                            e.span,
                                        ),
                                    );
                                }
                            }
                        }
                    }
                }
            }
            ExprKind::Macro { args, .. } => {
                // 实参 token 树里的标识符按名使用（println!("...", n) 的 n 等），防 S7 误报
                let mut words = Vec::new();
                collect_token_idents(&args.tokens, &mut words);
                for word in words {
                    let _ = self.symbols.lookup(&word);
                    self.mark_import_used(&word);
                }
            }
            ExprKind::Native(nb) => {
                // N1（v0.1 范围）：native 块按原文引用外层符号（变量捕获语义）。
                // 词法扫描其中的标识符并标记使用，避免 S7 对捕获变量误报；
                // 完整的"已声明 + 类型可平凡传递 + #[allow]"校验在 P3+ 接入类型推导。
                // N1: native 块语言标识校验
                if crate::langs::resolve(&nb.lang.name).is_none() {
                    self.diags.push(
                        Diagnostic::error(
                            DiagCode::NativeSafety("N1"),
                            format!("native 语言 `{}` 未注册（已注册语言见 dhv targets）", nb.lang.name),
                            nb.span,
                        )
                        .note("native 块语言标识必须是已注册后端 id 或 host（harness 宿主语言）"),
                    );
                }
                for word in nb.code.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
                    if word.is_empty() {
                        continue;
                    }
                    let _ = self.symbols.lookup(word);
                    self.mark_import_used(word);
                }
            }
            ExprKind::Path(p) => self.mark_path_used(p),
            ExprKind::Binary { op, lhs, rhs, .. } => {
                self.walk_expr(lhs);
                self.walk_expr(rhs);
                // v0.2.53 S-14：二元运算保守静态类型检查（与 dhv-ts 同口径；
                // 三后端真机对拍实证 L-8：str*int 在 rustc 拒绝 / python abcabcabc /
                // ts NaN 静默垃圾值 —— interp 运行期拒绝，静态提前拦）
                let lt = self.expr_lit_ty(lhs);
                let rt = self.expr_lit_ty(rhs);
                if let (Some(lt), Some(rt)) = (lt, rt) {
                    self.check_binary_op_types(*op, lt, rt, e.span);
                }
                // v0.2.54 S-15：两侧 int 且静态可折叠 → 注解域溢出/除零检查
                if matches!((lt, rt), (Some(SymbolLitTy::Int), Some(SymbolLitTy::Int)))
                    && matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem)
                {
                    self.check_domain_arith(*op, lhs, rhs, e.span);
                }
            }
            ExprKind::Unary { operand, .. } => self.walk_expr(operand),
            ExprKind::Call { callee, args } => {
                self.walk_expr(callee);
                for a in args {
                    self.walk_expr(a);
                }
            }
            ExprKind::MethodCall { receiver, method, args, .. } => {
                // S2: 裸 .unwrap() —— 非空默认铁律下的逃生口滥用
                if method.name == "unwrap" {
                    self.diags.push(
                        Diagnostic::warning(
                            DiagCode::Strictness("S2"),
                            "裸 `.unwrap()` 在生产 harness 中被禁止（S2 非空默认）".to_string(),
                            method.span,
                        )
                        .note("请使用 `unwrap_or*` / `match` / `?` 显式处理 None 情形"),
                    );
                }
                self.walk_expr(receiver);
                for a in args {
                    self.walk_expr(a);
                }
            }
            ExprKind::Field { base, .. } => self.walk_expr(base),
            ExprKind::Index { base, index } => {
                self.walk_expr(base);
                self.walk_expr(index);
            }
            ExprKind::Range(range) => {
                if let Some(lo) = &range.lo { self.walk_expr(lo); }
                if let Some(hi) = &range.hi { self.walk_expr(hi); }
            }
            ExprKind::Slice { base, range } => {
                self.walk_expr(base);
                if let Some(lo) = &range.lo {
                    self.walk_expr(lo);
                }
                if let Some(hi) = &range.hi {
                    self.walk_expr(hi);
                }
            }
            ExprKind::Try(inner) | ExprKind::Await(inner) => self.walk_expr(inner),
            ExprKind::Cast { expr, ty } => {
                self.walk_expr(expr);
                self.walk_type(ty);
            }
            ExprKind::Assign { lhs, rhs } | ExprKind::CompoundAssign { lhs, rhs, .. } => {
                self.check_assign_target(lhs); // S4
                self.walk_expr(lhs);
                self.walk_expr(rhs);
                // v0.2.54 S-14（v3）：重赋值更新字面量事实 —— 消除「先 int 后 str
                // 重赋值」中转的假阴性（e04 实录：let mut x = 1; x = "s"; x * 2
                // 静态漏拦）。字面量赋值 → 记新事实；非字面量 → 清除（保守放行）。
                // 复合赋值 → 折叠更新 + 域检查（与 dhv-ts 同口径）。
                if let ExprKind::Path(p) = &lhs.kind {
                    if p.segments.len() == 1 && !p.leading_colon {
                        let name = &p.segments[0].name;
                        let is_plain = matches!(&e.kind, ExprKind::Assign { .. });
                        if is_plain {
                            let t = self.expr_lit_ty(rhs);
                            let v = self.expr_int_val(rhs);
                            let dom = if t == Some(SymbolLitTy::Int) {
                                self.int_domain_of(rhs).or_else(|| self.symbols.peek_dom(name))
                            } else {
                                None
                            };
                            // v0.2.54 S-15：赋值域检查（let mut x: u8 = 0; x = 300
                            // —— S-13 只判 let 声明，赋值路径此前漏拦）
                            if let (Some(v), Some(dom)) = (v, dom.as_deref()) {
                                if let Some((lo, hi)) = int_domain_limits(dom) {
                                    if v < lo || v > hi {
                                        self.diags.push(
                                            Diagnostic::error(
                                                DiagCode::Strictness("S15"),
                                                format!("赋值域越界：{v} 超出 {dom} 域 [{lo}, {hi}]（interp BigInt 静默越域，rust 后端字面量编译期拒绝 —— 跨后端漂移；显式截断请用 as）"),
                                                e.span,
                                            ),
                                        );
                                    }
                                }
                            }
                            self.symbols.set_lit_facts(name, t, v, dom);
                        } else {
                            // 复合赋值（a += n）：折叠更新 lit_val；域检查
                            let bin_op = match &e.kind {
                                ExprKind::CompoundAssign { op, .. } => *op,
                                _ => BinaryOp::Add,
                            };
                            let base = self.symbols.peek_lit_val(name);
                            let rv = self.expr_int_val(rhs);
                            if let (Some(base), Some(rv)) = (base, rv) {
                                let res: Option<i128> = match bin_op {
                                    BinaryOp::Add => base.checked_add(rv),
                                    BinaryOp::Sub => base.checked_sub(rv),
                                    BinaryOp::Mul => base.checked_mul(rv),
                                    BinaryOp::Div => {
                                        if rv == 0 {
                                            self.diags.push(
                                                Diagnostic::error(
                                                    DiagCode::Strictness("S15"),
                                                    format!("静态可证除零：{base} {}= 0（interp 运行期 HRuntimeError，rustc 后端编译期拒绝；js 静默 NaN）", bin_op.as_str()),
                                                    e.span,
                                                ),
                                            );
                                            None
                                        } else {
                                            base.checked_div(rv)
                                        }
                                    }
                                    BinaryOp::Rem => {
                                        if rv == 0 {
                                            self.diags.push(
                                                Diagnostic::error(
                                                    DiagCode::Strictness("S15"),
                                                    format!("静态可证除零：{base} {}= 0（interp 运行期 HRuntimeError，rustc 后端编译期拒绝；js 静默 NaN）", bin_op.as_str()),
                                                    e.span,
                                                ),
                                            );
                                            None
                                        } else {
                                            base.checked_rem(rv)
                                        }
                                    }
                                    _ => None,
                                };
                                if let Some(res) = res {
                                    self.symbols.set_lit_val(name, res);
                                    if let Some(dom) = self.symbols.peek_dom(name) {
                                        if let Some((lo, hi)) = int_domain_limits(&dom) {
                                            if res < lo || res > hi {
                                                self.diags.push(
                                                    Diagnostic::error(
                                                        DiagCode::Strictness("S15"),
                                                        format!("注解域算术溢出：{base} {}= {rv} = {res} 超出 {dom} 域 [{lo}, {hi}]（interp BigInt 静默越域，rust 后端环绕/panic —— 跨后端漂移）", bin_op.as_str()),
                                                        e.span,
                                                    ),
                                                );
                                            }
                                        }
                                    }
                                }
                            } else {
                                // 动态值复合赋值 → 折叠值事实失效保守清除（类型/域事实保留）
                                let t = self.symbols.peek_lit_ty(name);
                                let dom = self.symbols.peek_dom(name);
                                self.symbols.set_lit_facts(name, t, None, dom);
                            }
                        }
                    }
                }
            }
            ExprKind::Closure { params, ret, body, .. } => {
                self.symbols.push_scope();
                for p in params {
                    self.walk_type(&p.ty); // S7: 类型注解中的导入使用标记
                    if let ParamKind::Pattern(pat) = &p.kind {
                        self.walk_pattern(pat, SymbolKind::Param);
                    }
                }
                if let Some(r) = ret {
                    self.walk_type(r);
                }
                self.walk_expr(body);
                self.pop_scope_report();
            }
            ExprKind::If { cond, then, else_ } => {
                self.check_bool_cond(cond, "if"); // S1
                self.walk_expr(cond);
                self.walk_block_inner(then);
                if let Some(el) = else_ {
                    self.walk_expr(el);
                }
            }
            ExprKind::IfLet { pattern, expr, then, else_ } => {
                self.walk_expr(expr);
                self.symbols.push_scope();
                self.walk_pattern(pattern, SymbolKind::Let);
                for s in &then.stmts {
                    self.walk_stmt(s);
                }
                if let Some(tail) = &then.tail {
                    self.walk_expr(tail);
                }
                self.pop_scope_report();
                if let Some(el) = else_ {
                    self.walk_expr(el);
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee);
                self.check_match_exhaustive(arms); // S6
                for arm in arms {
                    self.symbols.push_scope();
                    self.walk_pattern(&arm.pattern, SymbolKind::Let);
                    if let Some(g) = &arm.guard {
                        self.walk_expr(g);
                    }
                    self.walk_expr(&arm.body);
                    self.pop_scope_report();
                }
            }
            ExprKind::Loop { body, .. } => {
                // 注意：S6 的 in_agent_loop 仅由 graph 体内的 loop（check_graph）置位，
                // 与 dhv-ts 的 inAgentLoop 语义一致 —— 普通 fn 体内的 loop 不触发 S6。
                self.walk_block_inner(body);
            }
            ExprKind::While { cond, body, .. } => {
                self.check_bool_cond(cond, "while"); // S1
                self.walk_expr(cond);
                self.walk_block_inner(body);
            }
            ExprKind::WhileLet { pattern, expr, body, .. } => {
                self.walk_expr(expr);
                self.symbols.push_scope();
                self.walk_pattern(pattern, SymbolKind::Let);
                for s in &body.stmts {
                    self.walk_stmt(s);
                }
                if let Some(tail) = &body.tail {
                    self.walk_expr(tail);
                }
                self.pop_scope_report();
            }
            ExprKind::For { pattern, iter, body, .. } => {
                self.walk_expr(iter);
                self.symbols.push_scope();
                self.walk_pattern(pattern, SymbolKind::Let);
                for s in &body.stmts {
                    self.walk_stmt(s);
                }
                if let Some(tail) = &body.tail {
                    self.walk_expr(tail);
                }
                self.pop_scope_report();
            }
            ExprKind::Block(b) => self.walk_block_inner(b),
            ExprKind::AsyncBlock { body, .. } => self.walk_block_inner(body),
            ExprKind::Break { value, .. } => {
                if let Some(v) = value {
                    self.walk_expr(v);
                }
            }
            ExprKind::Continue { .. } => {}
            ExprKind::Return(v) => {
                if let Some(v) = v {
                    self.walk_expr(v);
                }
            }
            ExprKind::Tuple(elems) | ExprKind::Array(elems) => {
                for el in elems {
                    self.walk_expr(el);
                }
            }
            ExprKind::ArrayRepeat { elem, count } => {
                self.walk_expr(elem);
                self.walk_expr(count);
            }
            ExprKind::Struct { path, fields, spread } => {
                self.mark_path_used(path);
                for f in fields {
                    match &f.value {
                        Some(v) => self.walk_expr(v),
                        // 简写字段 `Report { verdict, .. }`：字段名即对外层绑定的使用
                        // （对齐 dhv-ts：shorthand 字段 markUsed）
                        None => {
                            if let FieldIndex::Named(id) = &f.name {
                                let _ = self.symbols.lookup(&id.name);
                                self.mark_import_used(&id.name);
                            }
                        }
                    }
                }
                if let Some(sp) = spread {
                    self.walk_expr(sp);
                }
            }
        }
    }

    /// S1: if/while 条件为非 bool 字面量 → 零隐式转换错误
    fn check_bool_cond(&mut self, cond: &Expr, kw: &str) {
        if let ExprKind::Literal(lit) = &cond.kind {
            if !matches!(lit.kind, LiteralKind::Bool(_)) {
                self.diags.push(
                    Diagnostic::error(
                        DiagCode::Strictness("S1"),
                        format!(
                            "`{kw}` 条件必须为 bool —— 此处为字面量 `{}`（S1 零隐式转换）",
                            lit.raw
                        ),
                        cond.span,
                    )
                    .note("HSL 没有隐式类型转换：非 bool 条件请显式比较，如 `if x != 0 {}`"),
                );
            }
        }
    }

    /// S4: 对不可变绑定赋值 → 编译错误
    fn check_assign_target(&mut self, lhs: &Expr) {
        if let ExprKind::Path(p) = &lhs.kind {
            if !p.leading_colon && p.segments.len() == 1 {
                let name = p.segments[0].name.clone();
                let immutable = self
                    .symbols
                    .lookup(&name)
                    .map(|s| !s.mutable)
                    .unwrap_or(false);
                if immutable {
                    let span = p.segments[0].span;
                    self.diags.push(
                        Diagnostic::error(
                            DiagCode::Strictness("S4"),
                            format!("不能对不可变绑定 `{name}` 赋值（S4 不可变优先）"),
                            span,
                        )
                        .note(format!("需要可变请声明 `let mut {name}`")),
                    );
                }
            }
        }
    }

    /// S6: 穷尽性 + AgentLoop 禁 `_` 通配
    fn check_match_exhaustive(&mut self, arms: &[MatchArm]) {
        let in_loop = self.in_agent_loop > 0;
        let mut covered: HashMap<String, HashSet<String>> = HashMap::new();
        let mut wildcard_span: Option<Span> = None;
        for arm in arms {
            if pattern_has_wildcard(&arm.pattern.kind) {
                wildcard_span = Some(arm.pattern.span);
            }
            let mut refs: Vec<(String, String)> = Vec::new();
            collect_variant_refs(&arm.pattern.kind, &mut refs);
            for (head, var) in refs {
                covered.entry(head).or_default().insert(var);
            }
        }
        // AgentLoop 内 `_` 通配兜底 → 直面新分支（BND §5.1 S6）
        // 触发条件对齐 dhv-ts：本 match 至少含一个**已注册枚举**头段时才禁止 `_`
        // （Option/Result/字符串字面量匹配的 `_` 兜底合法，不在此列）
        let has_enum_arm = covered.iter().any(|(head, _)| self.enums.contains_key(head));
        if in_loop {
            if let Some(span) = wildcard_span {
                if has_enum_arm {
                    self.diags.push(
                        Diagnostic::error(
                            DiagCode::Strictness("S6"),
                            "graph AgentLoop 内的枚举 match 不允许 `_` 通配兜底（必须显式穷尽，直面新分支）".to_string(),
                            span,
                        )
                        .note("S6 强制显式列出所有变体：新增枚举变体时编译器逼你直面新分支"),
                    );
                }
            }
        }
        // AgentLoop 外 `_` 通配视为穷尽覆盖（Rust 语义，BNF v1.4.1 §S-6 修正）
        if !in_loop && wildcard_span.is_some() {
            return;
        }
        // 注册表内的枚举 → 穷尽性校验
        for (enum_name, vars) in covered {
            if let Some(defined) = self.enums.get(&enum_name) {
                let missing: Vec<&String> =
                    defined.iter().filter(|v| !vars.contains(*v)).collect();
                if !missing.is_empty() {
                    let list = missing
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.diags.push(
                        Diagnostic::error(
                            DiagCode::Strictness("S6"),
                            format!("match 对 `{enum_name}` 不穷尽，缺少变体: {list}"),
                            arms.first().map(|a| a.span).unwrap_or_default(),
                        )
                        .note("S6：所有 match 必须穷尽；新增变体会使此处编译失败（这是特性）"),
                    );
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // 类型 / 模式遍历（import 使用标记）
    // ------------------------------------------------------------------

    fn walk_type(&mut self, t: &Type) {
        match &t.kind {
            TypeKind::Path(pt) => {
                self.mark_path_used(&pt.path);
                for arg in &pt.generic_args {
                    if let GenericArg::Type(ty) = arg {
                        self.walk_type(ty);
                    }
                }
            }
            TypeKind::Ref { inner, .. }
            | TypeKind::Slice(inner)
            | TypeKind::Paren(inner) => self.walk_type(inner),
            TypeKind::Tuple(elems) => {
                for ty in elems {
                    self.walk_type(ty);
                }
            }
            TypeKind::Array { elem, .. } => self.walk_type(elem),
            TypeKind::FnPtr { params, ret } => {
                for ty in params {
                    self.walk_type(ty);
                }
                if let Some(r) = ret {
                    self.walk_type(r);
                }
            }
            TypeKind::DynTrait(bounds) | TypeKind::ImplTrait(bounds) => {
                for b in bounds {
                    self.walk_type(&b.ty);
                }
            }
            TypeKind::Never | TypeKind::Infer => {}
        }
    }

    fn walk_pattern(&mut self, pat: &Pattern, kind: SymbolKind) {
        match &pat.kind {
            PatternKind::Literal(_) | PatternKind::Wildcard | PatternKind::Rest => {}
            PatternKind::Ident { mutable, name, .. } => {
                // 裸 `None` 是预导入单元变体（Some/None/Ok/Err 简写归一化），
                // 不构成绑定 —— 按路径使用处理，防 S7/S8 误报
                if name.name == "None" {
                    self.mark_import_used("None");
                    return;
                }
                self.declare_binding(name, *mutable, kind);
            }
            PatternKind::Range { lo, hi, .. } => {
                self.walk_pattern(lo, kind);
                self.walk_pattern(hi, kind);
            }
            PatternKind::Struct { path, fields, .. } => {
                self.mark_path_used(path);
                for f in fields {
                    if let Some(p) = &f.pattern {
                        self.walk_pattern(p, SymbolKind::Let);
                    }
                }
            }
            PatternKind::TupleStruct { path, elems, .. } => {
                self.mark_path_used(path);
                for p in elems {
                    self.walk_pattern(p, SymbolKind::Let);
                }
            }
            PatternKind::Tuple { elems, .. } => {
                for p in elems {
                    self.walk_pattern(p, SymbolKind::Let);
                }
            }
            PatternKind::Path(p) => self.mark_path_used(p),
            PatternKind::Or(pats) => {
                for p in pats {
                    self.walk_pattern(p, kind);
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // 符号登记 / 使用标记 / 作用域收尾
    // ------------------------------------------------------------------

    /// S8: 同作用域遮蔽错误 / 跨作用域遮蔽警告；随后登记符号
    fn declare_binding(&mut self, name: &Ident, mutable: bool, kind: SymbolKind) {
        if self.symbols.current_scope_has(&name.name) {
            self.diags.push(
                Diagnostic::error(
                    DiagCode::Strictness("S8"),
                    format!(
                        "绑定 `{}` 在同一作用域内重复声明（S8：同作用域遮蔽为错误）",
                        name.name
                    ),
                    name.span,
                )
                .note(format!("重命名其中一个 `{}` 绑定以消除冲突", name.name)),
            );
        } else if kind == SymbolKind::Let && self.symbols.outer_scope_has(&name.name) {
            self.diags.push(Diagnostic::warning(
                DiagCode::Strictness("S8"),
                format!("绑定 `{}` 遮蔽了外层作用域同名绑定（S8：跨作用域遮蔽警告）", name.name),
                name.span,
            ).note(format!(
                "如非有意遮蔽，建议将内层 `{}` 重命名为不同名称以避免混淆",
                name.name
            )));
        }
        self.symbols.declare(&name.name, mutable, TypeKind::Infer, kind, name.span);
    }

    fn mark_path_used(&mut self, path: &Path) {
        if path.segments.is_empty() {
            return;
        }
        let head = path.segments[0].name.clone();
        let _ = self.symbols.lookup(&head);
        self.mark_import_used(&head);
    }

    fn mark_import_used(&mut self, name: &str) {
        for imp in &mut self.imports {
            if imp.name == name {
                imp.used = true;
            }
        }
    }

    /// S7: 作用域收尾 —— 未使用的 let / graph node 即错误（`_` 前缀豁免）
    fn pop_scope_report(&mut self) {
        if let Some(scope) = self.symbols.scopes.last() {
            let unused: Vec<(String, Span, SymbolKind)> = scope
                .values()
                .filter(|s| !s.used && !s.name.starts_with('_'))
                .map(|s| (s.name.clone(), s.span, s.kind))
                .collect();
            for (name, span, kind) in unused {
                let msg = match kind {
                    SymbolKind::GraphNode => format!(
                        "graph 节点 `{name}` 声明后未使用（S7：未使用即错误；`_` 前缀豁免）"
                    ),
                    SymbolKind::Let => format!(
                        "未使用的绑定 `{name}`（S7：未使用即错误；`_` 前缀豁免）"
                    ),
                    SymbolKind::Param => continue,
                };
                self.diags.push(
                    Diagnostic::error(
                        DiagCode::Strictness("S7"),
                        msg,
                        span,
                    )
                    .note(format!("如确认不需要此绑定，请以 `_` 开头命名（如 `_{name}`）以豁免 S7 检查")),
                );
            }
        }
        self.symbols.pop_scope();
    }

    /// S7: 未使用 import（glob 豁免）
    fn report_unused_imports(&mut self) {
        let unused: Vec<(String, Span)> = self
            .imports
            .iter()
            .filter(|i| !i.used)
            .map(|i| (i.name.clone(), i.span))
            .collect();
        for (name, span) in unused {
            self.diags.push(
                Diagnostic::error(
                    DiagCode::Strictness("S7"),
                    format!("import `{name}` 导入后未使用（S7：未使用即错误）"),
                    span,
                )
                .note("删除未使用的导入；如需显式保留请以 `_` 开头命名别名"),
            );
        }
    }
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// 模式分析自由函数（S6）
// ---------------------------------------------------------------------------

/// 顶层（含 or-pattern 分支）是否出现 `_` 通配
fn pattern_has_wildcard(pk: &PatternKind) -> bool {
    match pk {
        PatternKind::Wildcard => true,
        PatternKind::Or(pats) => pats.iter().any(|p| pattern_has_wildcard(&p.kind)),
        _ => false,
    }
}

/// 收集模式引用的枚举变体 (enum_head, variant_name)（仅多段路径）
fn collect_variant_refs(pk: &PatternKind, out: &mut Vec<(String, String)>) {
    match pk {
        PatternKind::Or(pats) => {
            for p in pats {
                collect_variant_refs(&p.kind, out);
            }
        }
        PatternKind::Struct { path, .. } | PatternKind::TupleStruct { path, .. } => {
            if path.segments.len() >= 2 {
                out.push((
                    path.segments[0].name.clone(),
                    path.segments[path.segments.len() - 1].name.clone(),
                ));
            }
        }
        PatternKind::Path(p) => {
            if p.segments.len() >= 2 {
                out.push((
                    p.segments[0].name.clone(),
                    p.segments[p.segments.len() - 1].name.clone(),
                ));
            }
        }
        _ => {}
    }
}


/// 递归收集宏实参 token 树中的标识符（S7 使用标记用）
fn collect_token_idents(tts: &[TokenTree], out: &mut Vec<String>) {
    for tt in tts {
        match tt {
            TokenTree::Token(tok, _) => {
                if let Token::Ident(name) = tok {
                    out.push(name.clone());
                }
            }
            TokenTree::Delimited { tokens, .. } => collect_token_idents(tokens, out),
        }
    }
}

/// §2.15：项 → 规则匹配类型串（ItemKind → rules kind）
fn item_kind(item: &Item) -> Option<&'static str> {
    match item {
        Item::Graph(_) => Some("graph"),
        Item::Fn(_) => Some("fn"),
        Item::Struct(_) => Some("struct"),
        Item::Enum(_) => Some("enum"),
        Item::Trait(_) => Some("trait"),
        Item::Const(_) => Some("const"),
        Item::TypeAlias(_) => Some("type"),
        Item::StaticResource(_) => Some("block"),
        Item::Export(exp) => item_kind(&exp.item),
        _ => None,
    }
}

/// 规则 kind 归一：block / static 同义（StaticResource）
fn normalize_kind(kind: &str) -> String {
    if kind == "static" { "block".to_string() } else { kind.to_string() }
}

// ============================================================================
// v0.2.53 G-8 辅助：守卫模式语义指纹（只取语义字段，剥除 span ——
// 同语义不同位置的 pattern 必须得到相同指纹，Debug 序列化含 span 不可用）
// ============================================================================
fn pattern_fingerprint(p: &PatternKind) -> String {
    match p {
        PatternKind::Literal(l) => match &l.kind {
            LiteralKind::Int { value, .. } => format!("lit:{value}"),
            LiteralKind::Float { value, .. } => format!("litf:{value}"),
            LiteralKind::Str { value, .. } => format!("lits:{value}"),
            LiteralKind::Char(c) => format!("litc:{c}"),
            LiteralKind::Bool(b) => format!("litb:{b}"),
        },
        PatternKind::Ident { name, sub, .. } => {
            format!("bind:{}", name.name.clone()
                + &sub.as_ref().map(|s| format!(":{}", pattern_fingerprint(&s.kind))).unwrap_or_default())
        }
        PatternKind::Wildcard => "_".to_string(),
        PatternKind::Rest => "..".to_string(),
        PatternKind::Range { lo, hi, inclusive } => format!(
            "rng:{}..{}{}",
            pattern_fingerprint(&lo.kind),
            if *inclusive { "=" } else { "" },
            pattern_fingerprint(&hi.kind)
        ),
        PatternKind::Struct { path, fields, rest } => format!(
            "st:{}{{{}}}{}",
            path.segments.iter().map(|s| s.name.clone()).collect::<Vec<_>>().join("::"),
            fields.iter().map(|f| format!(
                "{}={}",
                f.name.name,
                f.pattern.as_ref().map(|p| pattern_fingerprint(&p.kind)).unwrap_or_else(|| "shorthand".into())
            )).collect::<Vec<_>>().join(","),
            if *rest { "+rest" } else { "" }
        ),
        PatternKind::TupleStruct { path, elems, rest_at } => format!(
            "tup:{}[{}]{}",
            path.segments.iter().map(|s| s.name.clone()).collect::<Vec<_>>().join("::"),
            elems.iter().map(|e| pattern_fingerprint(&e.kind)).collect::<Vec<_>>().join(","),
            rest_at.map(|i| format!("+r{i}")).unwrap_or_default()
        ),
        PatternKind::Tuple { elems, rest_at } => format!(
            "tp:[{}]{}",
            elems.iter().map(|e| pattern_fingerprint(&e.kind)).collect::<Vec<_>>().join(","),
            rest_at.map(|i| format!("+r{i}")).unwrap_or_default()
        ),
        PatternKind::Path(path) => format!(
            "path:{}",
            path.segments.iter().map(|s| s.name.clone()).collect::<Vec<_>>().join("::")
        ),
        PatternKind::Or(alts) => format!(
            "or({})",
            alts.iter().map(|a| pattern_fingerprint(&a.kind)).collect::<Vec<_>>().join("|")
        ),
    }
}

/// v0.2.53 S-14：字面量类型名（诊断文案用）
fn lit_ty_name(t: crate::typecheck::SymbolLitTy) -> &'static str {
    use crate::typecheck::SymbolLitTy::*;
    match t {
        Int => "int",
        Float => "float",
        Bool => "bool",
        Str => "str",
        Char => "char",
    }
}
