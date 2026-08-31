//! # P2 — Parser：pest PEG Pair 树 → 强类型 AST
//!
//! 输入：`hsl.pest`（P0）产生的 Pair 树。
//! 输出：`ast::SourceFile`（P1 定义的强类型 AST）。
//!
//! 实现约定：
//! - 所有 `parse_xxx(pair, f)` 函数与 hsl.pest 的同名规则一一对应；
//! - pest 的字符串字面量不出现在 Pair 树中，语义恢复依赖 named 规则
//!   （见 hsl.pest 头部工程约定）；
//! - `block {}` 体与 `native {}` 体按 **span 原文重组** 保证保真
//!   （BNF §1.9 模式 A/B）；
//! - 表达式优先级已由 PEG 分层固化，本层只做树的直接映射。

use pest::iterators::{Pair, Pairs};
use pest::Parser as _;
use pest_derive::Parser;

use crate::ast::*;
use crate::diagnostics::{DiagCode, Diagnostic, Diagnostics};

#[derive(Parser)]
#[grammar = "hsl.pest"]
pub struct HslParser;

// ============================================================================
// 入口
// ============================================================================

pub struct ParseSession<'a> {
    pub file_id: FileId,
    pub src: &'a str,
    pub diags: Diagnostics,
}

/// 解析一个 .hsl 文件。语法错误返回 None 并填充 diagnostics。
pub fn parse(file_id: FileId, src: &str) -> Result<SourceFile, Diagnostics> {
    match HslParser::parse(Rule::source_file, src) {
        Ok(mut pairs) => {
            let mut sess = ParseSession { file_id, src, diags: Diagnostics::new() };
            let root = pairs.next().expect("source_file yields one pair");
            let span = sp(root.as_span(), file_id);
            let items = root
                .into_inner()
                // EOI 为 pest 内置 named 规则，会出现在 pair 树中，需过滤
                .filter(|p| p.as_rule() != Rule::EOI)
                .map(|p| top_level(p, &mut sess))
                .collect::<Vec<_>>();
            if sess.diags.has_errors() {
                return Err(sess.diags);
            }
            Ok(SourceFile { items, span })
        }
        Err(err) => {
            let mut diags = Diagnostics::new();
            let (start, _end) = match err.line_col {
                pest::error::LineColLocation::Pos((l, c)) => byte_offset(src, l, c),
                pest::error::LineColLocation::Span((l, c), _) => byte_offset(src, l, c),
            };
            diags.push(
                Diagnostic::error(DiagCode::Parse, format!("语法错误: {err}"), Span::new(file_id, start, start))
                    .note("文法依据: hsl-spec/BNF.md"),
            );
            Err(diags)
        }
    }
}

fn byte_offset(src: &str, line: usize, col: usize) -> (usize, usize) {
    let mut off = 0;
    let mut cur_line = 1;
    for ch in src.chars() {
        if cur_line >= line {
            break;
        }
        if ch == '\n' {
            cur_line += 1;
        }
        off += ch.len_utf8();
    }
    off += col.saturating_sub(1);
    (off.min(src.len()), off.min(src.len()))
}

// ============================================================================
// 基础工具
// ============================================================================

fn sp(span: pest::Span<'_>, f: FileId) -> Span {
    Span::new(f, span.start(), span.end())
}

fn pair_span(pair: &Pair<'_, Rule>, f: FileId) -> Span {
    let s = pair.as_span();
    Span::new(f, s.start(), s.end())
}

fn ident(pair: Pair<'_, Rule>, f: FileId) -> Ident {
    Ident::new(pair.as_str(), pair_span(&pair, f))
}

/// 只取 Pair 的唯一子 Pair
fn sole(pairs: Pairs<'_, Rule>) -> Pair<'_, Rule> {
    pairs.into_iter().next().expect("rule has exactly one child")
}

// ============================================================================
// 顶层与项
// ============================================================================

fn top_level(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> TopLevel {
    let f = sess.file_id;
    match pair.as_rule() {
        // source_file 的直接子是 item_or_projection（非 silent），解包一层
        Rule::item_or_projection => {
            let inner = sole(pair.into_inner());
            top_level(inner, sess)
        }
        Rule::item => TopLevel::Item(item(pair, sess)),
        Rule::scale_decl => TopLevel::Scale(scale_decl(pair, f)),
        Rule::project_block => TopLevel::Project(project_block(pair, sess)),
        r => unreachable!("unexpected top-level rule: {r:?}"),
    }
}

fn item(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> Item {
    let f = sess.file_id;
    let inner = sole(pair.into_inner());
    match inner.as_rule() {
        Rule::struct_def => Item::Struct(struct_def(inner, sess)),
        Rule::enum_def => Item::Enum(enum_def(inner, sess)),
        Rule::trait_def => Item::Trait(trait_def(inner, sess)),
        Rule::impl_def => Item::Impl(impl_def(inner, sess)),
        Rule::fn_def => Item::Fn(fn_def(inner, sess)),
        Rule::const_def => Item::Const(const_def(inner, sess)),
        Rule::type_alias_def => Item::TypeAlias(type_alias_def(inner, sess)),
        Rule::graph_def => Item::Graph(graph_def(inner, sess)),
        Rule::static_resource_def => Item::StaticResource(static_resource_def(inner, sess)),
        Rule::import_decl => Item::Import(import_decl(inner, f)),
        Rule::export_item => {
            // export_item = { outer_attributes? ~ "export" ~ item }；outer_attributes
            // 为 silent 规则，attribute pair 直接出现在子层 —— 与 struct_def 等一致。
            // 前导属性（#[derive] export struct 形态）归并到内部项。
            let mut children = inner.into_inner();
            let leading_attrs = attributes(&mut children, f);
            let exported = children.next().expect("export item");
            let mut it = item(exported, sess);
            merge_leading_attrs(&mut it, leading_attrs);
            Item::Export(Box::new(ExportItem { item: it }))
        }
        Rule::macro_rules_definition => Item::MacroRules(macro_rules_def(inner, f)),
        Rule::macro_invocation_semi => {
            let mut mi = inner.into_inner();
            let path = simple_path(mi.next().expect("macro path"), f);
            let delim = mi.next().expect("macro args");
            Item::MacroCall { path, args: macro_args(delim, f) }
        }
        r => unreachable!("unexpected item rule: {r:?}"),
    }
}

// ---------------------------------------------------------------------------
// 属性
// ---------------------------------------------------------------------------

/// `#[derive(..)] export item` 的前导属性归并到内部项（保持顺序：前导在前）
fn merge_leading_attrs(item: &mut Item, extra: Vec<Attribute>) {
    if extra.is_empty() {
        return;
    }
    match item {
        Item::Struct(s) => {
            s.attrs.splice(0..0, extra);
        }
        Item::Enum(e) => {
            e.attrs.splice(0..0, extra);
        }
        Item::Trait(t) => {
            t.attrs.splice(0..0, extra);
        }
        Item::Fn(fn_) => {
            fn_.attrs.splice(0..0, extra);
        }
        Item::Const(c) => {
            c.attrs.splice(0..0, extra);
        }
        Item::TypeAlias(a) => {
            a.attrs.splice(0..0, extra);
        }
        Item::Graph(g) => {
            g.attrs.splice(0..0, extra);
        }
        Item::StaticResource(r) => {
            r.attrs.splice(0..0, extra);
        }
        Item::Impl(i) => {
            i.attrs.splice(0..0, extra);
        }
        _ => {}
    }
}

fn attributes(pairs: &mut Pairs<'_, Rule>, f: FileId) -> Vec<Attribute> {
    let mut attrs = Vec::new();
    while let Some(p) = pairs.peek() {
        if p.as_rule() == Rule::attribute {
            let attr = p.clone();
            pairs.next();
            attrs.push(attribute(attr, f));
        } else {
            break;
        }
    }
    attrs
}

fn attribute(pair: Pair<'_, Rule>, f: FileId) -> Attribute {
    let span = pair_span(&pair, f);
    let mut inner = pair.into_inner();
    let path_pair = inner.next().expect("attr path");
    let path = simple_path(path_pair, f);
    let args = inner.next().map(|p| match p.as_rule() {
        Rule::attr_args => {
            let ap = p.into_inner();
            match ap.peek().map(|x| x.as_rule()) {
                Some(Rule::literal) => AttrArgs::Assign(literal(sole(ap), f)),
                _ => {
                    // "(" ~ token_tree* ~ ")"
                    let mut tts = Vec::new();
                    for tt in ap {
                        tts.push(token_tree(tt, f));
                    }
                    AttrArgs::Tokens(tts)
                }
            }
        }
        r => unreachable!("unexpected attr arg rule: {r:?}"),
    });
    Attribute { path, args, span }
}

fn simple_path(pair: Pair<'_, Rule>, f: FileId) -> Path {
    let span = pair_span(&pair, f);
    let mut segments = Vec::new();
    for p in pair.clone().into_inner() {
        if p.as_rule() == Rule::identifier {
            segments.push(ident(p, f));
        }
    }
    // "::"? 前缀由原文判断
    let leading = span.end > span.start && pair.as_str().starts_with("::");
    Path { leading_colon: leading, segments, span }
}

// ---------------------------------------------------------------------------
// struct / enum
// ---------------------------------------------------------------------------

fn struct_def(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> StructDef {
    let f = sess.file_id;
    let span = pair_span(&pair, f);
    let mut inner = pair.clone().into_inner();
    let attrs = attributes(&mut inner, f);
    let name = ident(inner.next().expect("struct name"), f);
    let generics = take_generic_params(&mut inner, sess);
    // struct_body: named_field* | tuple_field* | 空
    let mut named = Vec::new();
    let mut tuple = Vec::new();
    // struct_body 为 named 规则（含 named_field/tuple_field），需解包
    // （v0.2.10 修复：此前直接 match named_field 永不命中，字段被静默丢弃）
    let fields_iter = inner.flat_map(|p| {
        if p.as_rule() == Rule::struct_body { p.into_inner().collect::<Vec<_>>() } else { vec![p] }
    });
    for p in fields_iter {
        match p.as_rule() {
            Rule::named_field => {
                let fs = pair_span(&p, f);
                let mut fi = p.into_inner();
                let fattrs = attributes(&mut fi, f);
                let fname = ident(fi.next().expect("field name"), f);
                let fty = ty(sole(fi), sess);
                named.push(FieldDef { attrs: fattrs, name: Some(fname), ty: fty, span: fs });
            }
            Rule::tuple_field => {
                let fs = pair_span(&p, f);
                let mut fi = p.into_inner();
                let fattrs = attributes(&mut fi, f);
                let fty = ty(sole(fi), sess);
                tuple.push(FieldDef { attrs: fattrs, name: None, ty: fty, span: fs });
            }
            _ => {}
        }
    }
    let kind = if !named.is_empty() {
        StructKind::Named(named)
    } else if !tuple.is_empty() {
        StructKind::Tuple(tuple)
    } else {
        // 由原文判断 unit 还是空 named
        if pair.as_str().trim_end().ends_with(";") { StructKind::Unit } else { StructKind::Named(vec![]) }
    };
    StructDef { attrs, name, generics, kind, span }
}

fn enum_def(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> EnumDef {
    let f = sess.file_id;
    let span = pair_span(&pair, f);
    let mut inner = pair.into_inner();
    let attrs = attributes(&mut inner, f);
    let name = ident(inner.next().expect("enum name"), f);
    let generics = take_generic_params(&mut inner, sess);
    let mut variants = Vec::new();
    for p in inner {
        if p.as_rule() != Rule::enum_variant {
            continue;
        }
        let vs = pair_span(&p, f);
        let mut vi = p.into_inner();
        let vattrs = attributes(&mut vi, f);
        let vname = ident(vi.next().expect("variant name"), f);
        let mut named = Vec::new();
        let mut tuple = Vec::new();
        let mut discriminant = None;
        for q in vi {
            match q.as_rule() {
                Rule::named_field => {
                    let fs = pair_span(&q, f);
                    let mut fi = q.into_inner();
                    let fattrs = attributes(&mut fi, f);
                    let fname = ident(fi.next().expect("field name"), f);
                    let fty = ty(sole(fi), sess);
                    named.push(FieldDef { attrs: fattrs, name: Some(fname), ty: fty, span: fs });
                }
                Rule::tuple_field => {
                    let fs = pair_span(&q, f);
                    let mut fi = q.into_inner();
                    let fattrs = attributes(&mut fi, f);
                    let fty = ty(sole(fi), sess);
                    tuple.push(FieldDef { attrs: fattrs, name: None, ty: fty, span: fs });
                }
                Rule::integer_literal => discriminant = Some(literal(q, f)),
                _ => {}
            }
        }
        let fields = if !named.is_empty() {
            StructKind::Named(named)
        } else if !tuple.is_empty() {
            StructKind::Tuple(tuple)
        } else {
            StructKind::Unit
        };
        variants.push(VariantDef { attrs: vattrs, name: vname, fields, discriminant, span: vs });
    }
    EnumDef { attrs, name, generics, variants, span }
}

// ---------------------------------------------------------------------------
// trait / impl
// ---------------------------------------------------------------------------

fn trait_def(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> TraitDef {
    let f = sess.file_id;
    let span = pair_span(&pair, f);
    let mut inner = pair.into_inner();
    let attrs = attributes(&mut inner, f);
    let name = ident(inner.next().expect("trait name"), f);
    let generics = take_generic_params(&mut inner, sess);
    let mut supertraits = Vec::new();
    let mut items = Vec::new();
    for p in inner.flat_map(|q| {
        // trait_item 为 named 包裹层（含 trait_fn_sig/const_def/...），需解包
        // （v0.2.10 修复：此前直接 match trait_fn_sig 永不命中，trait 全部 items 被静默丢弃）
        if q.as_rule() == Rule::trait_item {
            q.into_inner().collect::<Vec<_>>()
        } else {
            vec![q]
        }
    }) {
        match p.as_rule() {
            Rule::type_bound => supertraits.push(type_bound(p, sess)),
            Rule::trait_fn_sig => {
                let s = pair_span(&p, f);
                let mut ti = p.into_inner();
                let is_async = ti.peek().map(|x| x.as_rule() == Rule::async_kw).unwrap_or(false);
                if is_async {
                    ti.next();
                }
                let fname = ident(ti.next().expect("fn name"), f);
                let generics = take_generic_params(&mut ti, sess);
                let params = fn_params(ti.next().expect("fn params"), sess);
                let ret = ti.next().map(|x| ty(x, sess));
                items.push(TraitItem::FnSig(FnSig { is_async, name: fname, generics, params, ret, span: s }));
            }
            Rule::const_def => items.push(TraitItem::Const(const_def(p, sess))),
            Rule::type_alias_def => items.push(TraitItem::TypeAlias(type_alias_def(p, sess))),
            Rule::fn_def => items.push(TraitItem::Fn(fn_def(p, sess))),
            _ => {}
        }
    }
    TraitDef { attrs, name, generics, supertraits, items, span }
}

fn impl_def(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> ImplDef {
    let f = sess.file_id;
    let span = pair_span(&pair, f);
    let mut inner = pair.into_inner();
    let attrs = attributes(&mut inner, f);
    let generics = take_generic_params(&mut inner, sess);
    // impl_target: type_rule ("for" type_rule)?
    let target = inner.next().expect("impl target");
    let mut ti = target.into_inner();
    let first = ty(ti.next().expect("impl type"), sess);
    let second = ti.next().map(|x| ty(x, sess));
    let (trait_ty, self_ty) = match second {
        Some(s) => (Some(first), s),
        None => (None, first),
    };
    let mut items = Vec::new();
    for p in inner.flat_map(|q| {
        // impl_item 为 named 包裹层（含 fn_def/const_def/type_alias_def），需解包
        // （v0.2.10 修复：此前直接 match fn_def 永不命中，impl 全部 items 被静默丢弃）
        if q.as_rule() == Rule::impl_item {
            q.into_inner().collect::<Vec<_>>()
        } else {
            vec![q]
        }
    }) {
        match p.as_rule() {
            Rule::fn_def => items.push(ImplItem::Fn(fn_def(p, sess))),
            Rule::const_def => items.push(ImplItem::Const(const_def(p, sess))),
            Rule::type_alias_def => items.push(ImplItem::TypeAlias(type_alias_def(p, sess))),
            _ => {}
        }
    }
    ImplDef { attrs, trait_ty, self_ty, generics, items, span }
}

// ---------------------------------------------------------------------------
// fn / const / type alias
// ---------------------------------------------------------------------------

fn fn_def(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> FnDef {
    let f = sess.file_id;
    let span = pair_span(&pair, f);
    let mut inner = pair.into_inner();
    let attrs = attributes(&mut inner, f);
    let is_async = inner.peek().map(|p| p.as_rule() == Rule::async_kw).unwrap_or(false);
    if is_async {
        inner.next();
    }
    let name = ident(inner.next().expect("fn name"), f);
    let generics = take_generic_params(&mut inner, sess);
    let params = fn_params(inner.next().expect("fn params"), sess);
    // ("->" type)? 之后的 type 是返回类型；where_clause / fn_body 按规则名判断
    let mut ret = None;
    let mut where_clause = Vec::new();
    let mut body = None;
    for p in inner {
        match p.as_rule() {
            Rule::type_rule => ret = Some(ty(p, sess)),
            Rule::where_clause => where_clause = where_clause_items(p, sess),
            // fn_body = { block_expression | ";" } 为 named 规则，需解包取 block_expression
            // （v0.2.10 修复：此前直接 match block_expression 永不命中，函数体被静默丢弃）
            Rule::fn_body => {
                let fb = sole(p.into_inner());
                debug_assert_eq!(fb.as_rule(), Rule::block_expression);
                body = Some(block_expr(fb, sess));
            }
            _ => {}
        }
    }
    FnDef { attrs, is_async, name, generics, params, ret, where_clause, body, span }
}

fn fn_params(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> Vec<Param> {
    let f = sess.file_id;
    let mut params = Vec::new();
    for p in pair.into_inner() {
        debug_assert_eq!(p.as_rule(), Rule::fn_param);
        let ps = pair_span(&p, f);
        // fn_param = outer_attributes ~ (self_param | pat_mut? ~ pattern ~ ":" ~ type_rule)
        let nodes: Vec<Pair<'_, Rule>> = p
            .into_inner()
            .filter(|q| q.as_rule() != Rule::attribute) // 参数属性 v1 骨架忽略
            .collect();
        let (kind, ty_pair) = match nodes.first().map(|x| x.as_rule()) {
            Some(Rule::self_param) => {
                let sk = match sole(nodes[0].clone().into_inner()).as_rule() {
                    Rule::self_ref_mut => SelfKind::RefMut,
                    Rule::self_ref => SelfKind::Ref,
                    Rule::self_mut => SelfKind::Mut,
                    Rule::self_value => SelfKind::Value,
                    r => unreachable!("unexpected self rule: {r:?}"),
                };
                (ParamKind::Self_(sk), nodes.get(1))
            }
            _ => {
                // [pat_mut?, pattern, type_rule]
                let has_mut = nodes.first().map(|x| x.as_rule() == Rule::pat_mut).unwrap_or(false);
                let idx = if has_mut { 1 } else { 0 };
                let mut pat = pattern(nodes[idx].clone(), sess);
                // mut 参数（`mut fuel` / `mut state`）：可变性传入模式绑定，S4 赋值检查依赖它
                if has_mut {
                    mutify_pattern(&mut pat);
                }
                (ParamKind::Pattern(pat), nodes.get(idx + 1))
            }
        };
        let ty = ty_pair.map(|x| ty(x.clone(), sess)).unwrap_or(Type {
            kind: TypeKind::Infer,
            span: Span::new(f, ps.end, ps.end),
        });
        params.push(Param { kind, ty, span: ps });
    }
    params
}

/// 递归把模式中的所有标识符绑定标记为可变（`mut` 参数修饰整组绑定）
fn mutify_pattern(pat: &mut Pattern) {
    match &mut pat.kind {
        PatternKind::Ident { mutable, .. } => *mutable = true,
        PatternKind::Tuple { elems, .. } => {
            for p in elems { mutify_pattern(p); }
        }
        PatternKind::Struct { fields, .. } => {
            for f in fields {
                if let Some(p) = &mut f.pattern { mutify_pattern(p); }
            }
        }
        PatternKind::Or(pats) => {
            for p in pats { mutify_pattern(p); }
        }
        _ => {}
    }
}

fn const_def(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> ConstDef {
    let f = sess.file_id;
    let span = pair_span(&pair, f);
    let mut inner = pair.into_inner();
    let attrs = attributes(&mut inner, f);
    let name = ident(inner.next().expect("const name"), f);
    let ty = ty(inner.next().expect("const type"), sess);
    let value = expr(inner.next().expect("const value"), sess);
    ConstDef { attrs, name, ty, value, span }
}

fn type_alias_def(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> TypeAliasDef {
    let f = sess.file_id;
    let span = pair_span(&pair, f);
    let mut inner = pair.into_inner();
    let attrs = attributes(&mut inner, f);
    let name = ident(inner.next().expect("alias name"), f);
    let generics = take_generic_params(&mut inner, sess);
    let ty = ty(inner.next().expect("alias type"), sess);
    TypeAliasDef { attrs, name, generics, ty, span }
}

// ---------------------------------------------------------------------------
// 泛型
// ---------------------------------------------------------------------------

fn generic_params(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> GenericParams {
    let f = sess.file_id;
    let mut gp = GenericParams::default();
    for p in pair.into_inner() {
        let s = pair_span(&p, f);
        match p.as_rule() {
            Rule::type_param => {
                let mut inner = p.into_inner();
                let name = ident(inner.next().expect("type param name"), f);
                let mut bounds = Vec::new();
                let mut default = None;
                for q in inner {
                    match q.as_rule() {
                        Rule::type_bound => bounds.push(type_bound(q, sess)),
                        Rule::type_rule => default = Some(ty(q, sess)),
                        _ => {}
                    }
                }
                gp.type_params.push(TypeParam { name, bounds, default, span: s });
            }
            Rule::const_param => {
                let mut inner = p.into_inner();
                let name = ident(inner.next().expect("const param name"), f);
                let ty_pair = inner.next().expect("const param type");
                gp.const_params.push(ConstParam {
                    name,
                    ty: ty(ty_pair, sess),
                    span: s,
                });
            }
            _ => {}
        }
    }
    gp
}

/// 从 pairs 流头取 generic_params（消耗之）；不存在则默认值。
/// 此前 peek+filter 不消耗 iterator，导致后续 next() 取错 pair（v0.2.10 修复）。
fn take_generic_params(inner: &mut Pairs<'_, Rule>, sess: &mut ParseSession) -> GenericParams {
    if inner.peek().map(|p| p.as_rule()) == Some(Rule::generic_params) {
        let p = inner.next().expect("generic_params checked");
        generic_params(p, sess)
    } else {
        GenericParams::default()
    }
}

fn where_clause_items(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> Vec<WherePredicate> {
    let f = sess.file_id;
    let mut out = Vec::new();
    for p in pair.into_inner() {
        let s = pair_span(&p, f);
        let mut inner = p.into_inner();
        let ty_pair = inner.next().expect("where subject");
        let tyv = ty(ty_pair, sess);
        let bounds = inner.filter(|q| q.as_rule() == Rule::type_bound).map(|q| type_bound(q, sess)).collect();
        out.push(WherePredicate { ty: tyv, bounds, span: s });
    }
    out
}

fn type_bound(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> TypeBound {
    let f = sess.file_id;
    let span = pair_span(&pair, f);
    let inner = sole(pair.into_inner());
    TypeBound { ty: ty(inner, sess), span }
}

// ============================================================================
// 类型
// ============================================================================

fn ty(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> Type {
    let f = sess.file_id;
    let span = pair_span(&pair, f);
    let kind = match pair.as_rule() {
        Rule::type_rule => {
            let inner = sole(pair.into_inner());
            return ty(inner, sess);
        }
        Rule::type_no_bounds => {
            let inner = sole(pair.into_inner());
            return ty(inner, sess);
        }
        Rule::never_type => TypeKind::Never,
        Rule::infer_type => TypeKind::Infer,
        Rule::paren_type => {
            let inner = sole(pair.into_inner());
            TypeKind::Paren(Box::new(ty(inner, sess)))
        }
        Rule::tuple_type => {
            let mut elems = Vec::new();
            for p in pair.into_inner() {
                if p.as_rule() == Rule::type_rule {
                    elems.push(ty(p, sess));
                }
            }
            TypeKind::Tuple(elems)
        }
        Rule::array_type => {
            let mut inner = pair.into_inner();
            let elem = ty(inner.next().expect("array elem"), sess);
            let len_pair = inner.next().expect("array len");
            let len = const_arg(len_pair, sess);
            TypeKind::Array { elem: Box::new(elem), len }
        }
        Rule::slice_type => {
            let inner = sole(pair.into_inner());
            TypeKind::Slice(Box::new(ty(inner, sess)))
        }
        Rule::reference_type => {
            let mut inner = pair.into_inner();
            let mutable = inner.peek().map(|p| p.as_rule() == Rule::pat_mut).unwrap_or(false);
            if mutable {
                inner.next();
            }
            let inner_ty = ty(inner.next().expect("ref target"), sess);
            TypeKind::Ref { mutable, inner: Box::new(inner_ty) }
        }
        Rule::fn_ptr_type => {
            let mut inner = pair.into_inner();
            let params_pair = inner.next().expect("fn params");
            let mut params = Vec::new();
            for p in params_pair.into_inner() {
                if p.as_rule() == Rule::type_rule {
                    params.push(ty(p, sess));
                }
            }
            let ret = inner.next().map(|x| Box::new(ty(x, sess)));
            TypeKind::FnPtr { params, ret }
        }
        Rule::path_type => {
            let inner = sole(pair.into_inner());
            let (path, args) = type_path(inner, sess);
            TypeKind::Path(PathType { path, generic_args: args })
        }
        Rule::trait_object_type => {
            let bounds = pair.into_inner().map(|p| type_bound(p, sess)).collect();
            TypeKind::DynTrait(bounds)
        }
        Rule::impl_trait_type => {
            let bounds = pair.into_inner().map(|p| type_bound(p, sess)).collect();
            TypeKind::ImplTrait(bounds)
        }
        r => unreachable!("unexpected type rule: {r:?}"),
    };
    Type { kind, span }
}

/// type_path → (Path, Vec<GenericArg>)
fn type_path(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> (Path, Vec<GenericArg>) {
    let f = sess.file_id;
    let span = pair_span(&pair, f);
    let mut segments = Vec::new();
    let mut args = Vec::new();
    for p in pair.clone().into_inner() {
        match p.as_rule() {
            Rule::identifier => segments.push(ident(p, f)),
            // type_path 的子是 type_path_segment 包裹层（identifier ~ generic_args?），解包
            Rule::type_path_segment => {
                for q in p.into_inner() {
                    match q.as_rule() {
                        Rule::identifier => segments.push(ident(q, f)),
                        Rule::generic_args => {
                            for g in q.into_inner() {
                                args.push(generic_arg(g, sess));
                            }
                        }
                        _ => {}
                    }
                }
            }
            Rule::generic_args => {
                for g in p.into_inner() {
                    args.push(generic_arg(g, sess));
                }
            }
            _ => {}
        }
    }
    let leading = span.end > span.start && pair.as_str().starts_with("::");
    (Path { leading_colon: leading, segments, span }, args)
}

fn generic_arg(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> GenericArg {
    let inner = sole(pair.into_inner());
    match inner.as_rule() {
        Rule::type_rule => GenericArg::Type(ty(inner, sess)),
        Rule::literal => {
            let s = pair_span(&inner, sess.file_id);
            GenericArg::Const(ConstArg {
                kind: ConstArgKind::Literal(literal(inner, sess.file_id)),
                span: s,
            })
        }
        r => unreachable!("unexpected generic arg: {r:?}"),
    }
}

fn const_arg(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> ConstArg {
    let f = sess.file_id;
    let span = pair_span(&pair, f);
    let inner = sole(pair.into_inner());
    let kind = match inner.as_rule() {
        Rule::literal => ConstArgKind::Literal(literal(inner, f)),
        Rule::block_expression => ConstArgKind::Block(Box::new(block_expr(inner, sess))),
        r => unreachable!("unexpected const arg: {r:?}"),
    };
    ConstArg { kind, span }
}

// ============================================================================
// 模式
// ============================================================================

fn pattern(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> Pattern {
    let f = sess.file_id;
    let span = pair_span(&pair, f);
    let singles: Vec<Pair<'_, Rule>> = pair.into_inner().collect();
    if singles.len() == 1 {
        single_pattern(singles.into_iter().next().unwrap(), sess)
    } else {
        let alts = singles.into_iter().map(|p| single_pattern(p, sess)).collect();
        Pattern { kind: PatternKind::Or(alts), span }
    }
}

fn single_pattern(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> Pattern {
    let f = sess.file_id;
    let span = pair_span(&pair, f);
    let kind = match pair.as_rule() {
        Rule::single_pattern => return single_pattern(sole(pair.into_inner()), sess),
        Rule::literal_pattern => {
            let negative = pair.as_str().trim_start().starts_with('-');
            let inner = sole(pair.into_inner());
            let mut lit = literal(inner, f);
            if negative {
                lit.raw = format!("-{}", lit.raw);
                lit.kind = match lit.kind {
                    LiteralKind::Int { value, suffix } => LiteralKind::Int { value: -value, suffix },
                    LiteralKind::Float { value, suffix } => LiteralKind::Float { value: -value, suffix },
                    other => other,
                };
            }
            PatternKind::Literal(lit)
        }
        Rule::identifier_pattern => {
            let mut inner = pair.into_inner();
            let mutable = inner.peek().map(|p| p.as_rule() == Rule::pat_mut).unwrap_or(false);
            if mutable {
                inner.next();
            }
            let name = ident(inner.next().expect("binding name"), f);
            let sub = inner.next().map(|p| Box::new(pattern(p, sess)));
            PatternKind::Ident { mutable, name, sub }
        }
        Rule::wildcard_pattern => PatternKind::Wildcard,
        Rule::rest_pattern => PatternKind::Rest,
        Rule::range_pattern => {
            let mut inner = pair.clone().into_inner();
            let lo = Box::new(range_bound(inner.next().expect("range lo"), sess));
            let _op = inner.next().expect("range op");
            let hi = Box::new(range_bound(inner.next().expect("range hi"), sess));
            let inclusive = pair.as_str().contains("..=");
            PatternKind::Range { lo, hi, inclusive }
        }
        Rule::struct_pattern => {
            let mut inner = pair.into_inner();
            let path = path_from(inner.next().expect("struct pattern path"), sess);
            let mut fields = Vec::new();
            let mut rest = false;
            for p in inner {
                match p.as_rule() {
                    Rule::struct_pattern_elem => {
                        let fs = pair_span(&p, f);
                        let elem: Vec<Pair<'_, Rule>> = p.into_inner().collect();
                        match elem.as_slice() {
                            [id, pat] if id.as_rule() == Rule::identifier && pat.as_rule() == Rule::pattern => {
                                fields.push(StructPatternField {
                                    name: ident(id.clone(), f),
                                    pattern: Some(pattern(pat.clone(), sess)),
                                    span: fs,
                                });
                            }
                            [id, _] if id.as_rule() == Rule::pat_mut => {
                                // { mut x }：elem = [pat_mut, identifier]
                                if let Some(id2) = elem.get(1) {
                                    fields.push(StructPatternField {
                                        name: ident(id2.clone(), f),
                                        pattern: None,
                                        span: fs,
                                    });
                                }
                            }
                            [id] if id.as_rule() == Rule::identifier => {
                                fields.push(StructPatternField {
                                    name: ident(id.clone(), f),
                                    pattern: None,
                                    span: fs,
                                });
                            }
                            [r] if r.as_rule() == Rule::rest_pattern => rest = true,
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            PatternKind::Struct { path, fields, rest }
        }
        Rule::tuple_struct_pattern => {
            let mut inner = pair.into_inner();
            let path = path_from(inner.next().expect("variant path"), sess);
            let mut elems = Vec::new();
            let mut rest_at = None;
            if let Some(items) = inner.next() {
                for p in items.into_inner() {
                    if p.as_rule() == Rule::rest_pattern {
                        rest_at = Some(elems.len());
                    } else if p.as_rule() == Rule::pattern {
                        elems.push(pattern(p, sess));
                    }
                }
            }
            PatternKind::TupleStruct { path, elems, rest_at }
        }
        Rule::tuple_pattern => {
            let mut elems = Vec::new();
            let mut rest_at = None;
            for p in pair.into_inner() {
                if p.as_rule() == Rule::rest_pattern {
                    rest_at = Some(elems.len());
                } else if p.as_rule() == Rule::pattern {
                    elems.push(pattern(p, sess));
                }
            }
            PatternKind::Tuple { elems, rest_at }
        }
        Rule::grouped_pattern => {
            return pattern(sole(pair.into_inner()), sess);
        }
        Rule::path_pattern => {
            let path = path_from_segments(pair, sess);
            PatternKind::Path(path)
        }
        r => unreachable!("unexpected pattern rule: {r:?}"),
    };
    Pattern { kind, span }
}

fn range_bound(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> Pattern {
    match pair.as_rule() {
        Rule::literal_pattern => single_pattern(pair, sess),
        Rule::path_pattern => single_pattern(pair, sess),
        _ => single_pattern(pair, sess),
    }
}

/// path_in_expr / path_pattern / edge_endpoint → ast::Path
fn path_from(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> Path {
    let f = sess.file_id;
    match pair.as_rule() {
        Rule::path_pattern | Rule::path_in_expr | Rule::path_expression => {
            path_from_segments(pair, sess)
        }
        Rule::identifier => {
            let s = pair_span(&pair, f);
            Path {
                leading_colon: false,
                segments: vec![ident(pair, f)],
                span: s,
            }
        }
        _ => path_from_segments(pair, sess),
    }
}

fn path_from_segments(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> Path {
    let f = sess.file_id;
    let span = pair_span(&pair, f);
    let mut segments = Vec::new();
    for p in pair.clone().into_inner() {
        match p.as_rule() {
            Rule::identifier => segments.push(ident(p, f)),
            // path_in_expr/path_pattern 的子是 segment 包裹层，解包取 identifier
            Rule::path_expr_segment | Rule::path_segment_pattern | Rule::type_path_segment => {
                for q in p.into_inner() {
                    if q.as_rule() == Rule::identifier {
                        segments.push(ident(q, f));
                    }
                }
            }
            Rule::generic_args => { /* 表达式位置泛型实参（turbofish）v1 骨架忽略 */ }
            _ => {}
        }
    }
    let leading = pair.as_str().starts_with("::");
    Path { leading_colon: leading, segments, span }
}

// ============================================================================
// 字面量
// ============================================================================

fn literal(pair: Pair<'_, Rule>, f: FileId) -> Literal {
    // 兼容两种调用形态：literal 规则（compound-atomic，解包取子字面量规则）
    // 或直接传入子字面量规则（integer_literal/boolean_literal/…，literal_pattern 语境）
    let pair = if pair.as_rule() == Rule::literal {
        sole(pair.into_inner())
    } else {
        pair
    };
    let span = pair_span(&pair, f);
    let raw = pair.as_str().to_string();
    let inner = pair;
    let kind = match inner.as_rule() {
        Rule::integer_literal => {
            let text = inner.as_str();
            let (radix, digits): (u32, &str) = if let Some(d) = text.strip_prefix("0x") {
                (16, d)
            } else if let Some(d) = text.strip_prefix("0o") {
                (8, d)
            } else if let Some(d) = text.strip_prefix("0b") {
                (2, d)
            } else {
                (10, text)
            };
            let cleaned: String = digits.chars().filter(|c| *c != '_').collect();
            let value = i128::from_str_radix(&cleaned, radix)
                .unwrap_or_else(|_| 0);
            let suffix = int_suffix(text);
            LiteralKind::Int { value, suffix }
        }
        Rule::float_literal => {
            let text: String = inner.as_str().chars().filter(|c| *c != '_').collect();
            let value: f64 = text.trim_end_matches(|c: char| c.is_alphabetic())
                .parse()
                .unwrap_or(0.0);
            let suffix = if text.ends_with("f32") { Some(FloatSuffix::F32) } else if text.ends_with("f64") { Some(FloatSuffix::F64) } else { None };
            LiteralKind::Float { value, suffix }
        }
        Rule::string_literal => LiteralKind::Str {
            value: unescape_string(inner.as_str()),
            raw_string: false,
        },
        Rule::raw_string_lit => {
            let s = inner.as_str();
            let value = s
                .trim_start_matches('r')
                .trim_start_matches('#')
                .trim_start_matches('"')
                .trim_end_matches('"')
                .trim_end_matches('#')
                .to_string();
            LiteralKind::Str { value, raw_string: true }
        }
        Rule::char_literal => {
            let s = inner.as_str();
            let inner_ch = &s[1..s.len().saturating_sub(1)];
            let ch = if inner_ch.starts_with('\\') {
                unescape_char(inner_ch)
            } else {
                inner_ch.chars().next().unwrap_or('\0')
            };
            LiteralKind::Char(ch)
        }
        Rule::boolean_literal => LiteralKind::Bool(inner.as_str() == "true"),
        r => unreachable!("unexpected literal rule: {r:?}"),
    };
    Literal { kind, raw, span }
}

fn int_suffix(text: &str) -> Option<IntSuffix> {
    const SUFFIXES: &[(&str, IntSuffix)] = &[
        ("i128", IntSuffix::I128), ("i64", IntSuffix::I64), ("i32", IntSuffix::I32),
        ("i16", IntSuffix::I16), ("i8", IntSuffix::I8), ("isize", IntSuffix::Isize),
        ("u128", IntSuffix::U128), ("u64", IntSuffix::U64), ("u32", IntSuffix::U32),
        ("u16", IntSuffix::U16), ("u8", IntSuffix::U8), ("usize", IntSuffix::Usize),
    ];
    for (s, suffix) in SUFFIXES {
        if text.ends_with(s) {
            return Some(*suffix);
        }
    }
    None
}

fn unescape_string(s: &str) -> String {
    let inner = s.trim_start_matches('"').trim_end_matches('"');
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('0') => out.push('\0'),
                Some('\\') => out.push('\\'),
                Some('\'') => out.push('\''),
                Some('"') => out.push('"'),
                Some('x') => {
                    let hex: String = chars.by_ref().take(2).collect();
                    if let Ok(b) = u8::from_str_radix(&hex, 16) {
                        out.push(b as char);
                    }
                }
                Some('u') => {
                    let mut hex = String::new();
                    for c2 in chars.by_ref() {
                        if c2 == '{' || c2 == '}' || c2 == '_' {
                            continue;
                        }
                        hex.push(c2);
                        if hex.len() >= 6 {
                            break;
                        }
                    }
                    if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(cp) {
                            out.push(ch);
                        }
                    }
                }
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn unescape_char(s: &str) -> char {
    let mut chars = s.chars();
    chars.next(); // backslash
    match chars.next() {
        Some('n') => '\n',
        Some('r') => '\r',
        Some('t') => '\t',
        Some('0') => '\0',
        Some('\\') => '\\',
        Some('\'') => '\'',
        Some('"') => '"',
        Some('x') => {
            let hex: String = chars.take(2).collect();
            u8::from_str_radix(&hex, 16).map(|b| b as char).unwrap_or('\0')
        }
        Some(other) => other,
        None => '\0',
    }
}

// ============================================================================
// 表达式
// ============================================================================

fn expr(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> Expr {
    let f = sess.file_id;
    match pair.as_rule() {
        Rule::expression | Rule::no_struct_expression => {
            let child = sole(pair.into_inner());
            expr(child, sess)
        }
        // block_primary 为 named 包裹层（含 if/match/loop/while/for/block），解包
        Rule::block_primary => expr(sole(pair.into_inner()), sess),
        // §2.11.1 含块表达式（BNF v1.4.5 定义补全）：block_primary ~ postfix_op* —— 
        // 结构与 postfix_expression 同构（首个 inner 为块表达式，其余为后缀操作）
        Rule::expression_with_block => {
            let mut inner = pair.into_inner();
            let first = expr(inner.next().expect("block primary"), sess);
            let mut e = first;
            for op in inner {
                e = apply_postfix(e, op, sess);
            }
            e
        }
        Rule::assignment_expression | Rule::ns_assignment_expression => {
            let span = pair_span(&pair, f);
            let mut inner = pair.into_inner();
            let lhs = expr(inner.next().expect("assign lhs"), sess);
            if let Some(op) = inner.next() {
                let rhs = expr(inner.next().expect("assign rhs"), sess);
                let op_str = op.as_str();
                let kind = if op_str == "=" {
                    ExprKind::Assign { lhs: Box::new(lhs), rhs: Box::new(rhs) }
                } else {
                    ExprKind::CompoundAssign {
                        op: compound_op(op_str),
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    }
                };
                Expr { kind, span }
            } else {
                lhs
            }
        }
        // 范围表达式：a..b / a..=b / n.. / ..n；无 range_op 时透传内层表达式
        Rule::range_expression | Rule::ns_range_expression => {
            let span = pair_span(&pair, f);
            let mut lo: Option<Expr> = None;
            let mut hi: Option<Expr> = None;
            let mut inclusive = false;
            let mut has_op = false;
            for p in pair.into_inner() {
                match p.as_rule() {
                    Rule::range_op => {
                        has_op = true;
                        inclusive = p.as_str().contains('=');
                    }
                    Rule::or_expression | Rule::ns_or_expression => {
                        let e = expr(p, sess);
                        if has_op { hi = Some(e); } else { lo = Some(e); }
                    }
                    r => unreachable!("unexpected range child: {r:?}"),
                }
            }
            if !has_op {
                lo.expect("range_expression without op must hold one expression")
            } else {
                Expr {
                    kind: ExprKind::Range(Box::new(RangeExpr {
                        lo: lo.map(Box::new),
                        hi: hi.map(Box::new),
                        inclusive,
                    })),
                    span,
                }
            }
        }
        // 二元链（PEG 分层 → 左结合 fold）
        Rule::or_expression
        | Rule::ns_or_expression
        | Rule::and_expression
        | Rule::ns_and_expression
        | Rule::bit_or_expression
        | Rule::ns_bit_or_expression
        | Rule::bit_xor_expression
        | Rule::ns_bit_xor_expression
        | Rule::bit_and_expression
        | Rule::ns_bit_and_expression
        | Rule::equality_expression
        | Rule::ns_equality_expression
        | Rule::relational_expression
        | Rule::ns_relational_expression
        | Rule::shift_expression
        | Rule::ns_shift_expression
        | Rule::additive_expression
        | Rule::ns_additive_expression
        | Rule::multiplicative_expression
        | Rule::ns_multiplicative_expression => {
            let span = pair_span(&pair, f);
            let mut inner = pair.into_inner();
            let mut lhs = expr(inner.next().expect("binary lhs"), sess);
            while let Some(op) = inner.next() {
                let op_span = pair_span(&op, f);
                let rhs = expr(inner.next().expect("binary rhs"), sess);
                let op = binary_op(op.as_str());
                lhs = Expr {
                    kind: ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                    span: Span::merge(span, op_span),
                };
            }
            lhs
        }
        Rule::cast_expression | Rule::ns_cast_expression => {
            let span = pair_span(&pair, f);
            let mut inner = pair.into_inner();
            let mut e = expr(inner.next().expect("cast operand"), sess);
            while let Some(_p) = inner.next() {
                // as_kw 之后必跟 type_no_bounds
                let ty_pair = inner.next().expect("cast target type");
                let target = ty(ty_pair, sess);
                e = Expr {
                    kind: ExprKind::Cast { expr: Box::new(e), ty: target },
                    span,
                };
            }
            e
        }
        Rule::unary_expression | Rule::ns_unary_expression => {
            let span = pair_span(&pair, f);
            let mut inner = pair.into_inner();
            let first = inner.next().expect("unary first");
            match first.as_rule() {
                Rule::unary_op => {
                    let operand = expr(inner.next().expect("unary operand"), sess);
                    let op = match first.as_str() {
                        "-" => UnaryOp::Neg,
                        "!" => UnaryOp::Not,
                        "*" => UnaryOp::Deref,
                        _ => unreachable!(),
                    };
                    Expr { kind: ExprKind::Unary { op, operand: Box::new(operand) }, span }
                }
                Rule::ref_op => {
                    let mutable = first.into_inner().any(|p| p.as_rule() == Rule::pat_mut);
                    let operand = expr(inner.next().expect("ref operand"), sess);
                    let op = if mutable { UnaryOp::RefMut } else { UnaryOp::Ref };
                    Expr { kind: ExprKind::Unary { op, operand: Box::new(operand) }, span }
                }
                Rule::postfix_expression | Rule::ns_postfix_expression => {
                    // unary_expression 直接包裹 postfix（无 unary op）
                    let mut e = expr(first, sess);
                    let mut ops = inner;
                    while let Some(op) = ops.next() {
                        e = apply_postfix(e, op, sess);
                    }
                    e
                }
                r => unreachable!("unexpected unary rule: {r:?}"),
            }
        }
        Rule::postfix_expression | Rule::ns_postfix_expression => {
            let mut inner = pair.into_inner();
            let first = expr(inner.next().expect("postfix primary"), sess);
            let mut e = first;
            for op in inner {
                e = apply_postfix(e, op, sess);
            }
            e
        }
        Rule::primary_expression | Rule::ns_primary_expression => expr(sole(pair.into_inner()), sess),
        // 具体表达式
        Rule::literal_expression => {
            let span = pair_span(&pair, f);
            let lit = literal(sole(pair.into_inner()), f);
            Expr { kind: ExprKind::Literal(lit), span }
        }
        Rule::path_expression => {
            let span = pair_span(&pair, f);
            let path = path_from_segments(sole(pair.into_inner()), sess);
            Expr { kind: ExprKind::Path(path), span }
        }
        Rule::grouped_expression => {
            expr(sole(pair.into_inner()), sess)
        }
        Rule::tuple_expression => {
            let span = pair_span(&pair, f);
            let mut elems = Vec::new();
            // tuple_tail 为 named 包裹层（含第 2+ 元素），需解包
            // （v0.2.10 修复：此前只收集直接 expression 子，第 2+ 元素被静默丢弃）
            for p in pair.into_inner().flat_map(|q| {
                if q.as_rule() == Rule::tuple_tail {
                    q.into_inner().collect::<Vec<_>>()
                } else {
                    vec![q]
                }
            }) {
                if p.as_rule() == Rule::expression {
                    elems.push(expr(p, sess));
                }
            }
            Expr { kind: ExprKind::Tuple(elems), span }
        }
        Rule::array_expression => {
            let span = pair_span(&pair, f);
            let mut parts: Vec<Pair<'_, Rule>> = pair.into_inner().collect();
            // array_elements = [expression, ";", expression] | [expression, ...]
            if parts.len() == 1 {
                // 只有 array_elements 一个容器
                let arr = parts.remove(0);
                let elems: Vec<Pair<'_, Rule>> = arr.clone().into_inner().collect();
                if elems.len() == 2 && arr.as_str().contains(';') {
                    let elem = expr(elems[0].clone(), sess);
                    let count = expr(elems[1].clone(), sess);
                    Expr { kind: ExprKind::ArrayRepeat { elem: Box::new(elem), count: Box::new(count) }, span }
                } else {
                    let list = elems.into_iter().map(|p| expr(p, sess)).collect();
                    Expr { kind: ExprKind::Array(list), span }
                }
            } else {
                Expr { kind: ExprKind::Array(vec![]), span }
            }
        }
        Rule::struct_expression => {
            let span = pair_span(&pair, f);
            let mut inner = pair.into_inner();
            let path = path_from_segments(inner.next().expect("struct path"), sess);
            let mut fields = Vec::new();
            let mut spread = None;
            if let Some(fields_pair) = inner.next() {
                for p in fields_pair.into_inner() {
                    let fs = pair_span(&p, f);
                    let nodes: Vec<Pair<'_, Rule>> = p.into_inner().collect();
                    match nodes.as_slice() {
                        [id, val] if id.as_rule() == Rule::identifier && val.as_rule() == Rule::expression => {
                            fields.push(StructExprField {
                                name: FieldIndex::Named(ident(id.clone(), f)),
                                value: Some(expr(val.clone(), sess)),
                                span: fs,
                            });
                        }
                        [id] if id.as_rule() == Rule::identifier => {
                            fields.push(StructExprField {
                                name: FieldIndex::Named(ident(id.clone(), f)),
                                value: None,
                                span: fs,
                            });
                        }
                        // 功能更新 `..base`：`".."` 为字面量不产生 Pair，字段只剩
                        // 一个 expression 子对 —— 此前 [dotdot, val] 双子对形态永不命中，
                        // spread 被静默丢弃（S7 误报 base 未使用）
                        [val] if val.as_rule() == Rule::expression => {
                            spread = Some(Box::new(expr(val.clone(), sess)));
                        }
                        [lit] if lit.as_rule() == Rule::integer_literal => {
                            let value = lit.as_str().parse::<u32>().unwrap_or(0);
                            fields.push(StructExprField {
                                name: FieldIndex::Index(value, pair_span(lit, f)),
                                value: None,
                                span: fs,
                            });
                        }
                        _ => {}
                    }
                }
            }
            Expr { kind: ExprKind::Struct { path, fields, spread }, span }
        }
        Rule::closure_expression => {
            let span = pair_span(&pair, f);
            let mut inner = pair.into_inner();
            let is_move = inner.peek().map(|p| p.as_rule() == Rule::move_kw).unwrap_or(false);
            if is_move {
                inner.next();
            }
            let is_async = inner.peek().map(|p| p.as_rule() == Rule::async_kw).unwrap_or(false);
            if is_async {
                inner.next();
            }
            let mut params = Vec::new();
            let mut ret = None;
            let mut body: Option<BlockExprOrExpr> = None;
            for p in inner {
                match p.as_rule() {
                    Rule::closure_params => {
                        for q in p.into_inner() {
                            // closure_param = pattern (":" type)?
                            let qs = pair_span(&q, f);
                            let mut ci = q.into_inner();
                            let pat = pattern(ci.next().expect("closure param pattern"), sess);
                            let tyv = ci.next().map(|x| ty(x, sess));
                            params.push(Param { kind: ParamKind::Pattern(pat), ty: tyv.unwrap_or(Type { kind: TypeKind::Infer, span: qs }), span: qs });
                        }
                    }
                    Rule::type_rule => ret = Some(ty(p, sess)),
                    Rule::block_expression => body = Some(BlockExprOrExpr::Block(block_expr(p, sess))),
                    Rule::or_expression => body = Some(BlockExprOrExpr::Expr(expr(p, sess))),
                    _ => {}
                }
            }
            let body_expr = match body {
                Some(BlockExprOrExpr::Block(b)) => Expr { kind: ExprKind::Block(b), span },
                Some(BlockExprOrExpr::Expr(e)) => e,
                None => unreachable!("closure must have body"),
            };
            Expr { kind: ExprKind::Closure { is_move, is_async, params, ret, body: Box::new(body_expr) }, span }
        }
        Rule::if_expression => {
            let span = pair_span(&pair, f);
            let mut inner = pair.into_inner();
            let cond = expr(inner.next().expect("if cond"), sess);
            let then = block_expr(inner.next().expect("if then"), sess);
            let else_ = inner.next().map(|p| Box::new(expr(p, sess)));
            Expr { kind: ExprKind::If { cond: Box::new(cond), then, else_ }, span }
        }
        Rule::if_let_expression => {
            let span = pair_span(&pair, f);
            let mut inner = pair.into_inner();
            let pat = pattern(inner.next().expect("if-let pattern"), sess);
            let e = expr(inner.next().expect("if-let expr"), sess);
            let then = block_expr(inner.next().expect("if-let then"), sess);
            let else_ = inner.next().map(|p| Box::new(expr(p, sess)));
            Expr { kind: ExprKind::IfLet { pattern: pat, expr: Box::new(e), then, else_ }, span }
        }
        Rule::match_expression => {
            let span = pair_span(&pair, f);
            let mut inner = pair.into_inner();
            let scrutinee = expr(inner.next().expect("match scrutinee"), sess);
            let mut arms = Vec::new();
            for p in inner {
                let as_ = pair_span(&p, f);
                let mut ai = p.into_inner();
                let attrs = attributes(&mut ai, f);
                let pat = pattern(ai.next().expect("arm pattern"), sess);
                // guard_expr 容器（"if" 后的表达式），peek 判断避免误消费
                let guard = if ai.peek().map(|q| q.as_rule() == Rule::guard_expr).unwrap_or(false) {
                    let gp = ai.next().expect("guard_expr");
                    Some(expr(sole(gp.into_inner()), sess))
                } else {
                    None
                };
                let body_pair = ai.next().expect("arm body");
                let body = expr(body_pair, sess);
                arms.push(MatchArm { attrs, pattern: pat, guard, body, span: as_ });
            }
            Expr { kind: ExprKind::Match { scrutinee: Box::new(scrutinee), arms }, span }
        }
        Rule::loop_expression => {
            let span = pair_span(&pair, f);
            let mut inner = pair.into_inner();
            let label = label_of(&mut inner, f);
            let body = block_expr(inner.next().expect("loop body"), sess);
            Expr { kind: ExprKind::Loop { label, body }, span }
        }
        Rule::while_expression => {
            let span = pair_span(&pair, f);
            let mut inner = pair.into_inner();
            let label = label_of(&mut inner, f);
            let cond = expr(inner.next().expect("while cond"), sess);
            let body = block_expr(inner.next().expect("while body"), sess);
            Expr { kind: ExprKind::While { label, cond: Box::new(cond), body }, span }
        }
        Rule::while_let_expression => {
            let span = pair_span(&pair, f);
            let mut inner = pair.into_inner();
            let label = label_of(&mut inner, f);
            let pat = pattern(inner.next().expect("while-let pattern"), sess);
            let e = expr(inner.next().expect("while-let expr"), sess);
            let body = block_expr(inner.next().expect("while-let body"), sess);
            Expr { kind: ExprKind::WhileLet { label, pattern: pat, expr: Box::new(e), body }, span }
        }
        Rule::for_expression => {
            let span = pair_span(&pair, f);
            let mut inner = pair.into_inner();
            let label = label_of(&mut inner, f);
            let pat = pattern(inner.next().expect("for pattern"), sess);
            let iter = expr(inner.next().expect("for iter"), sess);
            let body = block_expr(inner.next().expect("for body"), sess);
            Expr { kind: ExprKind::For { label, pattern: pat, iter: Box::new(iter), body }, span }
        }
        Rule::block_expression => {
            let span = pair_span(&pair, f);
            let b = block_expr(pair, sess);
            Expr { kind: ExprKind::Block(b), span }
        }
        Rule::async_block_expression => {
            let span = pair_span(&pair, f);
            let mut inner = pair.into_inner();
            let is_move = inner.peek().map(|p| p.as_rule() == Rule::move_kw).unwrap_or(false);
            if is_move {
                inner.next();
            }
            let block_pair = inner.next().expect("async block body");
            let body = block_expr(block_pair, sess);
            Expr { kind: ExprKind::AsyncBlock { is_move, body }, span }
        }
        Rule::break_expression => {
            let span = pair_span(&pair, f);
            let mut inner = pair.into_inner();
            let label = inner.peek().filter(|p| p.as_rule() == Rule::label_token).map(|p| {
                let lp = p.clone();
                let lt = lp.as_str().trim_start_matches('\'').to_string();
                Ident::new(lt, pair_span(&lp, f))
            });
            if label.is_some() {
                inner.next();
            }
            let value = inner.next().map(|p| Box::new(expr(p, sess)));
            Expr { kind: ExprKind::Break { label, value }, span }
        }
        Rule::continue_expression => {
            let span = pair_span(&pair, f);
            let label = pair.into_inner().next().filter(|p| p.as_rule() == Rule::label_token).map(|p| {
                let lt = p.as_str().trim_start_matches('\'').to_string();
                Ident::new(lt, pair_span(&p, f))
            });
            Expr { kind: ExprKind::Continue { label }, span }
        }
        Rule::return_expression => {
            let span = pair_span(&pair, f);
            let value = pair.into_inner().next().map(|p| Box::new(expr(p, sess)));
            Expr { kind: ExprKind::Return(value), span }
        }
        Rule::macro_invocation => {
            let span = pair_span(&pair, f);
            let mut inner = pair.into_inner();
            let path = simple_path(inner.next().expect("macro path"), f);
            let delim_pair = inner.next().expect("macro args");
            let args = macro_args(delim_pair, f);
            Expr { kind: ExprKind::Macro { path, args }, span }
        }
        Rule::native_block_expression => {
            let span = pair_span(&pair, f);
            let native = native_block(pair, sess);
            Expr { kind: ExprKind::Native(native), span }
        }
        r => unreachable!("unexpected expression rule: {r:?}"),
    }
}

/// 闭包 body 的双态容器（block 或表达式）
enum BlockExprOrExpr {
    Block(BlockExpr),
    Expr(Expr),
}

fn label_of(inner: &mut Pairs<'_, Rule>, f: FileId) -> Option<Ident> {
    if let Some(p) = inner.peek() {
        if p.as_rule() == Rule::loop_label {
            let lp = p.clone();
            inner.next();
            let name = lp.as_str().split(':').next().unwrap_or("").trim_start_matches('\'').to_string();
            return Some(Ident::new(name, pair_span(&lp, f)));
        }
    }
    None
}

fn apply_postfix(base: Expr, op: Pair<'_, Rule>, sess: &mut ParseSession) -> Expr {
    let f = sess.file_id;
    let span = Span::merge(base.span, pair_span(&op, f));
    let inner = sole(op.into_inner());
    let kind = match inner.as_rule() {
        Rule::try_op => ExprKind::Try(Box::new(base)),
        Rule::await_op => ExprKind::Await(Box::new(base)),
        Rule::method_call => {
            let mut mi = inner.into_inner();
            let method = ident(mi.next().expect("method name"), f);
            let mut generic_args = Vec::new();
            let mut args = Vec::new();
            for p in mi {
                match p.as_rule() {
                    Rule::generic_args => {
                        for g in p.into_inner() {
                            generic_args.push(generic_arg(g, sess));
                        }
                    }
                    Rule::call_args => {
                        for a in p.into_inner() {
                            args.push(expr(a, sess));
                        }
                    }
                    _ => {}
                }
            }
            ExprKind::MethodCall { receiver: Box::new(base), method, generic_args, args }
        }
        Rule::field_access => {
            let fp = sole(inner.into_inner());
            match fp.as_rule() {
                Rule::identifier => ExprKind::Field { base: Box::new(base), field: FieldIndex::Named(ident(fp, f)) },
                Rule::integer_literal => {
                    let idx = fp.as_str().parse::<u32>().unwrap_or(0);
                    ExprKind::Field { base: Box::new(base), field: FieldIndex::Index(idx, pair_span(&fp, f)) }
                }
                r => unreachable!("unexpected field: {r:?}"),
            }
        }
        Rule::call_op => {
            let mut args = Vec::new();
            // call_op = { "(" ~ call_args? ~ ")" }，call_args 为 named 包裹层，需解包
            // （v0.2.10 修复：此前直接 match expression 永不命中，实参被静默丢弃）
            for p in inner.into_inner().flat_map(|q| {
                if q.as_rule() == Rule::call_args {
                    q.into_inner().collect::<Vec<_>>()
                } else {
                    vec![q]
                }
            }) {
                if p.as_rule() == Rule::expression {
                    args.push(expr(p, sess));
                }
            }
            ExprKind::Call { callee: Box::new(base), args }
        }
        Rule::index_op => {
            // index_op = { "[" ~ index_or_range ~ "]" }，index_or_range 为 named 包裹层需解包
            let mut idx_pair = sole(inner.into_inner());
            if idx_pair.as_rule() == Rule::index_or_range {
                idx_pair = sole(idx_pair.into_inner());
            }
            match idx_pair.as_rule() {
                Rule::range_full => {
                    let mut ri = idx_pair.clone().into_inner();
                    let lo = ri.next().map(|p| Box::new(expr(p, sess)));
                    // 中间的 range_op 跳过
                    let mut hi = None;
                    for p in ri {
                        if p.as_rule() == Rule::expression {
                            hi = Some(Box::new(expr(p, sess)));
                        }
                    }
                    let inclusive = idx_pair.as_str().contains("..=");
                    ExprKind::Slice { base: Box::new(base), range: RangeExpr { lo, hi, inclusive } }
                }
                Rule::expression => {
                    let index = Box::new(expr(idx_pair, sess));
                    ExprKind::Index { base: Box::new(base), index }
                }
                r => unreachable!("unexpected index: {r:?}"),
            }
        }
        r => unreachable!("unexpected postfix op: {r:?}"),
    };
    Expr { kind, span }
}

fn binary_op(s: &str) -> BinaryOp {
    match s {
        "+" => BinaryOp::Add,
        "-" => BinaryOp::Sub,
        "*" => BinaryOp::Mul,
        "/" => BinaryOp::Div,
        "%" => BinaryOp::Rem,
        "&" => BinaryOp::BitAnd,
        "|" => BinaryOp::BitOr,
        "^" => BinaryOp::BitXor,
        "<<" => BinaryOp::Shl,
        ">>" => BinaryOp::Shr,
        "==" => BinaryOp::Eq,
        "!=" => BinaryOp::Ne,
        "<" => BinaryOp::Lt,
        ">" => BinaryOp::Gt,
        "<=" => BinaryOp::Le,
        ">=" => BinaryOp::Ge,
        "&&" => BinaryOp::And,
        "||" => BinaryOp::Or,
        _ => unreachable!("unknown binary operator: {s}"),
    }
}

fn compound_op(s: &str) -> BinaryOp {
    match s {
        "+=" => BinaryOp::Add,
        "-=" => BinaryOp::Sub,
        "*=" => BinaryOp::Mul,
        "/=" => BinaryOp::Div,
        "%=" => BinaryOp::Rem,
        "&=" => BinaryOp::BitAnd,
        "|=" => BinaryOp::BitOr,
        "^=" => BinaryOp::BitXor,
        "<<=" => BinaryOp::Shl,
        ">>=" => BinaryOp::Shr,
        _ => unreachable!("unknown compound op: {s}"),
    }
}

// ============================================================================
// 块与语句
// ============================================================================

fn block_expr(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> BlockExpr {
    let f = sess.file_id;
    let span = pair_span(&pair, f);
    let mut stmts = Vec::new();
    let mut tail = None;
    let stmt_pairs: Vec<Pair<'_, Rule>> = pair.into_inner().collect();
    let n = stmt_pairs.len();
    for (i, sp) in stmt_pairs.into_iter().enumerate() {
        debug_assert_eq!(sp.as_rule(), Rule::statement);
        let inner = sole(sp.clone().into_inner());
        match inner.as_rule() {
            Rule::let_statement => {
                stmts.push(Stmt::Let(let_statement(inner, sess)));
            }
            Rule::item => {
                stmts.push(Stmt::Item(item(inner, sess)));
            }
            Rule::expression_statement => {
                let es_span = pair_span(&inner, f);
                let has_semi = inner.as_str().trim_end().ends_with(';');
                let e = expr(sole(inner.into_inner()), sess);
                if i + 1 == n && !has_semi {
                    tail = Some(Box::new(e));
                } else {
                    stmts.push(Stmt::Expr { expr: e, has_semi });
                }
                let _ = es_span;
            }
            _ => {
                // 空语句 ";"
                stmts.push(Stmt::Empty(pair_span(&sp, f)));
            }
        }
    }
    BlockExpr { stmts, tail, span }
}

fn let_statement(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> LetStmt {
    let f = sess.file_id;
    let span = pair_span(&pair, f);
    let mut inner = pair.into_inner();
    let attrs = attributes(&mut inner, f);
    let mutable = inner.peek().map(|p| p.as_rule() == Rule::pat_mut).unwrap_or(false);
    if mutable {
        inner.next();
    }
    let pattern = pattern(inner.next().expect("let pattern"), sess);
    let mut ty = None;
    let mut init = None;
    let mut else_block = None;
    for p in inner {
        match p.as_rule() {
            Rule::type_rule => ty = Some(crate_parser_ty(p, sess)),
            Rule::expression => init = Some(expr(p, sess)),
            Rule::let_else_block => {
                else_block = Some(block_expr(sole(p.into_inner()), sess));
            }
            _ => {}
        }
    }
    LetStmt { attrs, mutable, pattern, ty, init, else_block, span }
}

fn crate_parser_ty(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> Type {
    ty(pair, sess)
}

fn statement(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> Stmt {
    let f = sess.file_id;
    let inner = sole(pair.clone().into_inner());
    match inner.as_rule() {
        Rule::let_statement => Stmt::Let(let_statement(inner, sess)),
        Rule::item => Stmt::Item(item(inner, sess)),
        Rule::expression_statement => {
            let has_semi = inner.as_str().trim_end().ends_with(';');
            let e = expr(sole(inner.into_inner()), sess);
            Stmt::Expr { expr: e, has_semi }
        }
        _ => Stmt::Empty(pair_span(&pair, f)),
    }
}

// ============================================================================
// HSL 专属：graph / edge
// ============================================================================

fn graph_def(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> GraphDef {
    let f = sess.file_id;
    let span = pair_span(&pair, f);
    let mut inner = pair.into_inner();
    let attrs = attributes(&mut inner, f);
    let name = ident(inner.next().expect("graph name"), f);
    let generics = take_generic_params(&mut inner, sess);
    let mut params = Vec::new();
    let mut ret = None;
    let mut where_clause = Vec::new();
    let mut body = Vec::new();
    for p in inner {
        match p.as_rule() {
            Rule::graph_params => {
                for q in p.into_inner() {
                    // graph_param = pat_mut? ~ identifier ~ ":" ~ type_rule
                    let qs = pair_span(&q, f);
                    let mut gi = q.into_inner();
                    let mutable = gi.peek().map(|x| x.as_rule() == Rule::pat_mut).unwrap_or(false);
                    if mutable {
                        gi.next();
                    }
                    let pname = ident(gi.next().expect("graph param name"), f);
                    let pty = ty(gi.next().expect("graph param type"), sess);
                    params.push(Param {
                        kind: ParamKind::Pattern(Pattern {
                            kind: PatternKind::Ident { mutable, name: pname.clone(), sub: None },
                            span: pname.span,
                        }),
                        ty: pty,
                        span: qs,
                    });
                }
            }
            Rule::type_rule => ret = Some(ty(p, sess)),
            Rule::where_clause => where_clause = where_clause_items(p, sess),
            Rule::graph_body => {
                for q in p.into_inner() {
                    graph_stmt(q, sess, &mut body);
                }
            }
            _ => {}
        }
    }
    GraphDef { attrs, name, generics, params, ret, where_clause, body, span }
}

fn graph_stmt(pair: Pair<'_, Rule>, sess: &mut ParseSession, body: &mut Vec<GraphStmt>) {
    let f = sess.file_id;
    // graph_body 的子是 graph_stmt（named 包裹层），需解包
    // （v0.2.10 修复：此前直接 match node_decl/edge_decl/... 永不命中，graph 语句被静默丢弃）
    let pair = if pair.as_rule() == Rule::graph_stmt {
        sole(pair.into_inner())
    } else {
        pair
    };
    match pair.as_rule() {
        Rule::node_decl => {
            let ns = pair_span(&pair, f);
            let mut inner = pair.into_inner();
            // node_kw 是 named 上下文关键字规则，会产生 Pair，需先跳过
            if inner.peek().map(|p| p.as_rule() == Rule::node_kw).unwrap_or(false) {
                inner.next();
            }
            let mutable = inner.peek().map(|p| p.as_rule() == Rule::pat_mut).unwrap_or(false);
            if mutable {
                inner.next();
            }
            let name = ident(inner.next().expect("node name"), f);
            let tyv = ty(inner.next().expect("node type"), sess);
            let init = inner.next().map(|p| expr(p, sess));
            body.push(GraphStmt::Node(NodeDecl { mutable, name, ty: tyv, init, span: ns }));
        }
        Rule::edge_decl => {
            let es = pair_span(&pair, f);
            let inner = pair.into_inner();
            let mut endpoints = Vec::new();
            let mut on = None;
            let mut attrs = Vec::new();
            for p in inner {
                match p.as_rule() {
                    Rule::edge_endpoint => {
                        endpoints.push(path_from_segments(sole(p.into_inner()), sess));
                    }
                    Rule::edge_guard => {
                        // edge_guard = { pattern | expression } 为 named 包裹层，需解包
                        // （修复：此前直接匹配 pattern/expression 永不命中，`on` 守卫被静默丢弃
                        //  → dsh.hsl `edge .. on Event::Observed` 的 Event 被 S7 误报为未使用）
                        let g = sole(p.into_inner());
                        match g.as_rule() {
                            Rule::pattern => on = Some(EdgeGuard::Pattern(pattern(g, sess))),
                            Rule::expression => on = Some(EdgeGuard::Expr(expr(g, sess))),
                            _ => {}
                        }
                    }
                    Rule::pattern => on = Some(EdgeGuard::Pattern(pattern(p, sess))),
                    Rule::expression => on = Some(EdgeGuard::Expr(expr(p, sess))),
                    Rule::edge_attrs => {
                        for q in p.into_inner() {
                            if q.as_rule() != Rule::edge_attr {
                                continue; // with_kw 等非 edge_attr 子对跳过
                            }
                            let as_ = pair_span(&q, f);
                            let mut ei = q.into_inner();
                            let name = ident(ei.next().expect("edge attr name"), f);
                            let value = ei.next().map(|v| literal(v, f));
                            attrs.push(EdgeAttr { name, value, span: as_ });
                        }
                    }
                    _ => {}
                }
            }
            body.push(GraphStmt::Edge(EdgeDecl { endpoints, on, attrs, span: es }));
        }
        Rule::let_statement => body.push(GraphStmt::Let(let_statement(pair, sess))),
        Rule::statement => body.push(GraphStmt::Stmt(statement(pair, sess))),
        _ => {}
    }
}

// ============================================================================
// HSL 专属：block / static 静态资源（span 原文重组，BNF §1.9 模式 A）
// ============================================================================

fn static_resource_def(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> StaticResourceDef {
    let f = sess.file_id;
    let span = pair_span(&pair, f);
    let mut inner = pair.into_inner();
    let kind_pair = inner.next().expect("resource kind");
    let kind = if kind_pair.as_str() == "block" { ResourceKind::Block } else { ResourceKind::Static };
    let name = ident(inner.next().expect("resource name"), f);
    let attrs = Vec::new(); // 顶层属性已在 item 层剥离（resource_kind 前无属性分支）
    let body_pair = inner.next().expect("block body");
    let content = block_body_content(body_pair, sess);
    StaticResourceDef { attrs, kind, name, content, span }
}

/// block 体 → Vec<RawContentPart>
/// 用 **span 原文重组**：深度遍历收集所有 interpolation 的 span，
/// 区间之间的文本直接从源文件切片，保证 100% 保真。
fn block_body_content(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> Vec<RawContentPart> {
    let f = sess.file_id;
    let src = sess.src;
    let body_span = pair.as_span();
    let base = body_span.start();
    // 收集（深度遍历）所有 interpolation
    let mut interps: Vec<(usize, usize, Pair<'_, Rule>)> = Vec::new();
    collect_interpolations(&pair, &mut interps);
    interps.sort_by_key(|(s, _, _)| *s);
    let mut parts = Vec::new();
    let mut cursor = base;
    for (start, end, ipair) in interps {
        if start > cursor {
            parts.push(RawContentPart::Text(src[cursor..start].to_string()));
        }
        let ip_span = pair_span(&ipair, f);
        let expr_pair = sole(ipair.into_inner());
        let e = expr(expr_pair, sess);
        parts.push(RawContentPart::Interpolation { expr: e, span: ip_span });
        cursor = end;
    }
    if cursor < body_span.end() {
        parts.push(RawContentPart::Text(src[cursor..body_span.end()].to_string()));
    }
    parts
}

fn collect_interpolations<'i>(
    pair: &Pair<'i, Rule>,
    out: &mut Vec<(usize, usize, Pair<'i, Rule>)>,
) {
    for p in pair.clone().into_inner() {
        match p.as_rule() {
            Rule::interpolation => {
                let s = p.as_span();
                out.push((s.start(), s.end(), p));
            }
            // block_body 的直接子是 block_element（named 包裹层），需递归穿透
            // （v0.2.10 修复：此前未穿透 block_element，{{...}} 全部沦为 Text 原文）
            Rule::block_nested_brace | Rule::block_element => collect_interpolations(&p, out),
            _ => {}
        }
    }
}

// ============================================================================
// HSL 专属：native 逃生舱（整体 span 原文，BNF §1.9 模式 B）
// ============================================================================

fn native_block(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> NativeBlock {
    let f = sess.file_id;
    let span = pair_span(&pair, f);
    let mut inner = pair.into_inner();
    let lang = ident(inner.next().expect("native lang"), f);
    let body = inner.next().expect("native body");
    // native_body 整体原文（as_str 覆盖首尾字符之间的完整区间）
    let code = body.as_str().to_string();
    NativeBlock { lang, code, span }
}

// ============================================================================
// project / scale / import / export
// ============================================================================

fn project_block(pair: Pair<'_, Rule>, sess: &mut ParseSession) -> ProjectBlock {
    let f = sess.file_id;
    let span = pair_span(&pair, f);
    let mut projections = Vec::new();
    let mut rules = Vec::new();
    for p in pair.into_inner() {
        match p.as_rule() {
            // §2.15（BNF v1.5）：rules { kind -> "path" : lang } 投射规则组
            Rule::rules_block => {
                for ri in p.into_inner() {
                    let rs = pair_span(&ri, f);
                    let mut rinner = ri.into_inner();
                    let kind = rinner.next().expect("rule kind").as_str().to_string();
                    let path = unquote_string(rinner.next().expect("rule path").as_str());
                    let lang = ident(rinner.next().expect("rule lang"), f);
                    rules.push(ProjectionRule { kind, path, lang, span: rs });
                }
            }
            _ => {
                let ps = pair_span(&p, f);
                let mut inner = p.into_inner();
                let target = path_from_segments(inner.next().expect("projection target"), sess);
                let path = unquote_string(inner.next().expect("projection path").as_str());
                let lang = ident(inner.next().expect("projection lang"), f);
                projections.push(Projection { target, path, lang, span: ps });
            }
        }
    }
    ProjectBlock { projections, rules, span }
}

fn scale_decl(pair: Pair<'_, Rule>, f: FileId) -> ScaleDecl {
    let span = pair_span(&pair, f);
    let mode_pair = sole(pair.into_inner());
    let text = mode_pair.as_str();
    let mode = match text {
        "monolith" => ScaleMode::Monolith,
        "microkernel" => ScaleMode::Microkernel,
        other => ScaleMode::Custom(other.to_string()),
    };
    ScaleDecl { mode, span }
}

fn import_decl(pair: Pair<'_, Rule>, f: FileId) -> ImportDecl {
    let span = pair_span(&pair, f);
    let mut inner = pair.into_inner();
    let spec_pair = inner.next().expect("import spec");
    // import_spec = { import_braced | import_namespace | import_single } 为 named 规则，解包一层
    let spec_pair = if spec_pair.as_rule() == Rule::import_spec {
        sole(spec_pair.into_inner())
    } else {
        spec_pair
    };
    let spec = match spec_pair.as_rule() {
        Rule::import_braced => {
            let mut items = Vec::new();
            for p in spec_pair.into_inner() {
                items.push(import_item(p, f));
            }
            ImportSpec::Named(items)
        }
        Rule::import_namespace => {
            // ["as_kw", identifier]
            let alias = spec_pair
                .into_inner()
                .filter(|p| p.as_rule() == Rule::identifier)
                .map(|p| ident(p, f))
                .next()
                .expect("namespace alias");
            ImportSpec::Namespace { alias }
        }
        Rule::import_single => {
            let mut idents: Vec<Ident> = spec_pair
                .into_inner()
                .filter(|p| p.as_rule() == Rule::identifier)
                .map(|p| ident(p, f))
                .collect();
            let name = idents.remove(0);
            let alias = idents.pop_if_exists();
            ImportSpec::Single(ImportItem { name, alias })
        }
        r => unreachable!("unexpected import spec: {r:?}"),
    };
    let from = unquote_string(inner.next().expect("import from").as_str());
    ImportDecl { spec, from, span }
}

trait PopIfExists {
    fn pop_if_exists(&mut self) -> Option<Ident>;
}
impl PopIfExists for Vec<Ident> {
    fn pop_if_exists(&mut self) -> Option<Ident> {
        if self.is_empty() {
            None
        } else {
            Some(self.remove(0))
        }
    }
}

fn import_item(pair: Pair<'_, Rule>, f: FileId) -> ImportItem {
    let mut idents: Vec<Ident> = pair
        .into_inner()
        .filter(|p| p.as_rule() == Rule::identifier)
        .map(|p| ident(p, f))
        .collect();
    let name = idents.remove(0);
    let alias = if idents.is_empty() { None } else { Some(idents.remove(0)) };
    ImportItem { name, alias }
}

fn unquote_string(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        unescape_string(&t[1..t.len() - 1])
    } else {
        t.to_string()
    }
}

// ============================================================================
// 宏
// ============================================================================

fn macro_rules_def(pair: Pair<'_, Rule>, f: FileId) -> MacroRulesDefinition {
    let span = pair_span(&pair, f);
    let mut inner = pair.into_inner();
    let name = ident(inner.next().expect("macro name"), f);
    let mut rules = Vec::new();
    for p in inner {
        // macro_rule_semi = "(" ~ macro_match* ~ ")" ~ "=>" ~ macro_transcriber ~ ";"
        let ri = p.into_inner();
        let mut matcher = Vec::new();
        let mut transcriber = Vec::new();
        for q in ri {
            match q.as_rule() {
                Rule::macro_match => matcher.push(macro_match(q, f)),
                Rule::macro_transcriber => {
                    // macro_transcriber = { transcriber_delim }；遍历 delim 的子 token
                    let dt = sole(q.into_inner());
                    for t in dt.into_inner() {
                        transcriber.push(macro_transcribe(t, f));
                    }
                }
                _ => {}
            }
        }
        rules.push(MacroRule { matcher, transcriber });
    }
    MacroRulesDefinition { name, rules, span }
}

fn macro_match(pair: Pair<'_, Rule>, f: FileId) -> MacroMatch {
    let inner = sole(pair.into_inner());
    match inner.as_rule() {
        Rule::macro_frag_binding => {
            let mut fi = inner.into_inner();
            let name = ident(fi.next().expect("frag name"), f);
            let frag = match fi.next().expect("frag spec").as_str() {
                "ident" => MacroFragSpec::Ident,
                "path" => MacroFragSpec::Path,
                "expr" => MacroFragSpec::Expr,
                "ty" => MacroFragSpec::Ty,
                "pat" => MacroFragSpec::Pat,
                "stmt" => MacroFragSpec::Stmt,
                "block" => MacroFragSpec::Block,
                "item" => MacroFragSpec::Item,
                "literal" => MacroFragSpec::Literal,
                "tt" => MacroFragSpec::Tt,
                "meta" => MacroFragSpec::Meta,
                _ => unreachable!(),
            };
            MacroMatch::Fragment { name, frag }
        }
        Rule::macro_rep => {
            let ri = inner.into_inner();
            let mut pattern = Vec::new();
            let mut separator = None;
            let mut op = RepetitionOp::ZeroOrMore;
            for q in ri {
                match q.as_rule() {
                    Rule::macro_match => pattern.push(macro_match(q, f)),
                    Rule::macro_rep_sep => separator = Some(q.as_str().to_string()),
                    Rule::macro_rep_op => {
                        op = match q.as_str() {
                            "*" => RepetitionOp::ZeroOrMore,
                            "+" => RepetitionOp::OneOrMore,
                            _ => RepetitionOp::ZeroOrOne,
                        };
                    }
                    _ => {}
                }
            }
            MacroMatch::Repetition { pattern, separator, op }
        }
        Rule::delim_token_tree => MacroMatch::Token(token_tree(inner, f)),
        Rule::macro_token => {
            let s = pair_span(&inner, f);
            MacroMatch::Token(TokenTree::Token(
                token(sole(inner.into_inner()), f),
                s,
            ))
        }
        r => unreachable!("unexpected macro match: {r:?}"),
    }
}

fn macro_transcribe(pair: Pair<'_, Rule>, f: FileId) -> MacroTranscribe {
    let inner = sole(pair.into_inner());
    match inner.as_rule() {
        Rule::macro_var => {
            let name = inner
                .into_inner()
                .filter(|p| p.as_rule() == Rule::identifier)
                .map(|p| ident(p, f))
                .next()
                .expect("macro var");
            MacroTranscribe::Var(name)
        }
        Rule::macro_trans_rep => {
            let ri = inner.into_inner();
            let mut pattern = Vec::new();
            let mut separator = None;
            let mut op = RepetitionOp::ZeroOrMore;
            for q in ri {
                match q.as_rule() {
                    Rule::transcriber_tt => pattern.push(macro_transcribe(q, f)),
                    Rule::macro_rep_sep => separator = Some(q.as_str().to_string()),
                    Rule::macro_rep_op => {
                        op = match q.as_str() {
                            "*" => RepetitionOp::ZeroOrMore,
                            _ => RepetitionOp::OneOrMore,
                        };
                    }
                    _ => {}
                }
            }
            MacroTranscribe::Repetition { pattern, separator, op }
        }
        Rule::delim_token_tree => MacroTranscribe::Token(token_tree(inner, f)),
        _ => MacroTranscribe::Token(token_tree(inner, f)),
    }
}

fn macro_args(pair: Pair<'_, Rule>, f: FileId) -> MacroArgs {
    let span = pair_span(&pair, f);
    let text = pair.as_str();
    let delim = match text.chars().next() {
        Some('(') => Delimiter::Paren,
        Some('[') => Delimiter::Bracket,
        _ => Delimiter::Brace,
    };
    let tokens = pair.into_inner().map(|p| token_tree(p, f)).collect();
    MacroArgs { delim, tokens, span }
}

fn token_tree(pair: Pair<'_, Rule>, f: FileId) -> TokenTree {
    match pair.as_rule() {
        // token_tree = { delim_token_tree | token } 为 named 规则，解包一层
        Rule::token_tree => token_tree(sole(pair.into_inner()), f),
        // 关键字在 token 树中按 ident token 处理（transcriber/matcher 语境）
        Rule::keyword => {
            let s = pair_span(&pair, f);
            TokenTree::Token(Token::Ident(pair.as_str().to_string()), s)
        }
        Rule::delim_token_tree => {
            let span = pair_span(&pair, f);
            let text = pair.as_str();
            let delim = match text.chars().next() {
                Some('(') => Delimiter::Paren,
                Some('[') => Delimiter::Bracket,
                _ => Delimiter::Brace,
            };
            let tokens = pair.into_inner().map(|p| token_tree(p, f)).collect();
            TokenTree::Delimited { delim, tokens, span }
        }
        Rule::token => {
            let s = pair_span(&pair, f);
            TokenTree::Token(token(sole(pair.into_inner()), f), s)
        }
        r => unreachable!("unexpected token tree: {r:?}"),
    }
}

fn token(pair: Pair<'_, Rule>, f: FileId) -> Token {
    match pair.as_rule() {
        Rule::identifier => Token::Ident(pair.as_str().to_string()),
        Rule::raw_identifier => Token::RawIdent(pair.as_str().trim_start_matches("r#").to_string()),
        Rule::literal => {
            let lit = literal(pair, f);
            Token::Literal(lit)
        }
        Rule::label_token => Token::Label(pair.as_str().to_string()),
        _ => Token::Punct(pair.as_str().to_string()),
    }
}
