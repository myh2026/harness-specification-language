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
            imports: Vec::new(),
            in_agent_loop: 0,
        }
    }

    /// 对整个文件执行全部检查（根文件入口：含 Scale / Project / S 系列）
    pub fn check_file(&mut self, file: &SourceFile) -> &Diagnostics {
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
    // Pass 0: 收集
    // ------------------------------------------------------------------

    /// 模块注册表收集（linker 调用，检查根文件前）：
    /// 仅收集依赖模块 **导出** 的 enum 变体与静态资源 —— 跨模块 S6/P4 语义。
    /// 不动 declared_items（P3 目标存在性仍由根文件自身 + 其 import 决定）。
    pub fn harvest_module(&mut self, file: &SourceFile) {
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

        // Pass A: 注册 node/let 声明（G2 端点表 + S7 追踪）
        for stmt in &graph.body {
            match stmt {
                GraphStmt::Node(n) => {
                    declared_nodes.push(n.name.name.clone());
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
        for stmt in &graph.body {
            match stmt {
                GraphStmt::Edge(edge) => {
                    for ep in &edge.endpoints {
                        let last = ep.last().name.clone();
                        let _ = self.symbols.lookup(&last); // 端点引用即使用
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
                ));
            } else {
                seen_paths.insert(path.clone(), name.clone());
            }
            // P4 层级
            match crate::langs::resolve(rule.lang.name.as_str()) {
                None => {
                    self.diags.push(Diagnostic::error(
                        DiagCode::Projection("P4"),
                        format!(
                            "未注册的后端语言 `{}`（rules 展开，项 {name}；注册表见 dhv targets）",
                            rule.lang.name
                        ),
                        rule.span,
                    ));
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
                        ));
                    }
                    if !is_block && spec.tier == 0 {
                        self.diags.push(Diagnostic::error(
                            DiagCode::Projection("P4"),
                            format!(
                                "代码项 `{name}` 不能投射到静态格式 `{0}`（rules 展开，需编程语言后端）",
                                rule.lang.name
                            ),
                            rule.span,
                        ));
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // 语句 / 表达式遍历（S 系列检查主体）
    // ------------------------------------------------------------------

    fn check_let(&mut self, l: &LetStmt) {
        if let Some(ty) = &l.ty {
            self.walk_type(ty);
        }
        if let Some(init) = &l.init {
            self.walk_expr(init);
        }
        if let Some(els) = &l.else_block {
            self.walk_block_inner(els);
        }
        if let PatternKind::Ident { name, .. } = &l.pattern.kind {
            self.declare_binding(name, l.mutable, SymbolKind::Let);
        } else {
            self.walk_pattern(&l.pattern, SymbolKind::Let);
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
            ExprKind::Literal(_) => {}
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
                for word in nb.code.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
                    if word.is_empty() {
                        continue;
                    }
                    let _ = self.symbols.lookup(word);
                    self.mark_import_used(word);
                }
            }
            ExprKind::Path(p) => self.mark_path_used(p),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.walk_expr(lhs);
                self.walk_expr(rhs);
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
