//! Swift backend (Logic tier) -- type mapping + full function body translation
//! Swift is a modern systems language from Apple. This backend generates Swift 5.9+ code.
//! struct -> struct (named) / final class (tuple/unit);
//! enum -> enum with associated values (indirect for recursive);
//! trait -> protocol;
//! impl -> extension;
//! fn -> top-level func;
//! const -> static let;
//! graph -> @main struct with main()

use crate::ast::*;
use crate::codegen::{CodegenBackend, CodegenContext};

pub struct SwiftBackend;

impl CodegenBackend for SwiftBackend {
    fn lang(&self) -> &'static str {
        "swift"
    }

    fn emit_item(&self, ctx: &CodegenContext, item: &Item) -> Result<String, String> {
        let mut out = String::new();
        out.push_str(&format!(
            "// {}\n",
            crate::sourcemap::generated_header("swift")
        ));
        out.push_str("// HSL-generated Swift code -- do not edit manually\n\n");
        // Swift standard library prelude
        out.push_str("import Foundation\n\n");

        match item {
            Item::Struct(s) => out.push_str(&swift_struct(s)),
            Item::Enum(e) => out.push_str(&swift_enum(e)),
            Item::Trait(t) => out.push_str(&swift_protocol(t)),
            Item::Fn(f) => out.push_str(&swift_fn(f)),
            Item::Graph(g) => out.push_str(&swift_graph(g, ctx)),
            Item::Impl(imp) => out.push_str(&swift_extension(imp)),
            Item::Const(c) => out.push_str(&swift_const(c)),
            Item::TypeAlias(a) => out.push_str(&swift_typealias(a)),
            Item::MacroRules(m) => out.push_str(&swift_macro_rules(m)),
            _ => {
                return Err(format!(
                    "swift backend does not support {}",
                    crate::ast::item_kind_name(item)
                ))
            }
        }
        Ok(out)
    }
}

// ────────────────────────────────────────────────────────────────
// Swift 关键字列表
// ────────────────────────────────────────────────────────────────

const SWIFT_KW: &[&str] = &[
    // Primary keywords
    "associatedtype", "class", "deinit", "enum", "extension", "fileprivate",
    "func", "import", "init", "inout", "internal", "let", "open", "operator",
    "private", "protocol", "public", "static", "struct", "subscript", "typealias",
    "var", "break", "case", "continue", "default", "defer", "do", "else",
    "fallthrough", "for", "guard", "if", "in", "repeat", "return", "switch",
    "where", "while", "as", "catch", "false", "is", "nil", "rethrows",
    "super", "self", "Self", "throw", "throws", "true", "try", "async",
    "await",
    // Contextual / reserved keywords
    "Any", "Protocol", "Type", "Optional", "Result", "Array", "Dictionary",
    "Set", "String", "Int", "Int8", "Int16", "Int32", "Int64", "UInt",
    "UInt8", "UInt16", "UInt32", "UInt64", "Float", "Double", "Bool",
    "Character", "Void", "Never", "AnyObject",
    "@main", "@escaping", "@autoclosure", "@available", "@discardableResult",
    "@inlinable", "@inline", "@usableFromInline", "@frozen", "@unknown",
    "mutating", "nonmutating", "lazy", "weak", "unowned", "convenience",
    "required", "override", "indirect", "final", "get", "set", "willSet",
    "didSet", "some", "any", "actor", "precedencegroup", "_",
];

fn sw_ident(s: &str) -> String {
    if SWIFT_KW.contains(&s) {
        format!("`{}`", s)
    } else {
        s.to_string()
    }
}

fn sw_capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

// ────────────────────────────────────────────────────────────────
// 类型映射
// ────────────────────────────────────────────────────────────────

fn sw_generic_arg(arg: &GenericArg) -> String {
    match arg {
        GenericArg::Type(t) => sw_type(t),
        GenericArg::Const(c) => match &c.kind {
            ConstArgKind::Literal(lit) => lit.raw.clone(),
            ConstArgKind::Block(_) => "0".into(),
        },
    }
}

fn sw_two_generic_args(args: &[GenericArg]) -> (String, String) {
    let mut it = args.iter();
    (
        it.next()
            .map(sw_generic_arg)
            .unwrap_or_else(|| "Any".into()),
        it.next()
            .map(sw_generic_arg)
            .unwrap_or_else(|| "Any".into()),
    )
}

fn sw_type(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(pt) => {
            let name = pt.path.last().name.as_str();
            match name {
                "String" | "str" => "String".into(),
                "char" => "Character".into(),
                "bool" => "Bool".into(),
                "i8" => "Int8".into(),
                "i16" => "Int16".into(),
                "i32" => "Int32".into(),
                "i64" => "Int64".into(),
                "u8" => "UInt8".into(),
                "u16" => "UInt16".into(),
                "u32" => "UInt32".into(),
                "u64" => "UInt64".into(),
                "usize" | "isize" => "Int".into(),
                "f32" => "Float".into(),
                "f64" => "Double".into(),
                "Vec" => format!(
                    "[{}]",
                    pt.generic_args
                        .iter()
                        .next()
                        .map(sw_generic_arg)
                        .unwrap_or_else(|| "Any".into())
                ),
                "HashMap" => {
                    let (k, v) = sw_two_generic_args(&pt.generic_args);
                    format!("[{}: {}]", k, v)
                }
                "HashSet" => format!(
                    "Set<{}>",
                    pt.generic_args
                        .iter()
                        .next()
                        .map(sw_generic_arg)
                        .unwrap_or_else(|| "Any".into())
                ),
                "Option" => format!(
                    "{}?",
                    pt.generic_args
                        .iter()
                        .next()
                        .map(sw_generic_arg)
                        .unwrap_or_else(|| "Any".into())
                ),
                "Result" => {
                    if pt.generic_args.len() >= 2 {
                        let (ok, err) = sw_two_generic_args(&pt.generic_args);
                        format!("Result<{}, {}>", ok, err)
                    } else if !pt.generic_args.is_empty() {
                        sw_generic_arg(&pt.generic_args[0])
                    } else {
                        "Result<Any, Error>".into()
                    }
                }
                "Box" => {
                    if !pt.generic_args.is_empty() {
                        sw_generic_arg(&pt.generic_args[0])
                    } else {
                        "Any".into()
                    }
                }
                _ => sw_ident(name),
            }
        }
        TypeKind::Ref { inner, .. } => sw_type(inner),
        TypeKind::Tuple(elems) => {
            if elems.is_empty() {
                "Void".into()
            } else {
                // Swift doesn't have built-in tuples > N; use (A, B) syntax
                let es: Vec<String> = elems.iter().map(sw_type).collect();
                format!("({})", es.join(", "))
            }
        }
        TypeKind::Array { elem, .. } => format!("[{}]", sw_type(elem)),
        TypeKind::Slice(inner) => format!("[{}]", sw_type(inner)),
        TypeKind::Paren(inner) => sw_type(inner),
        TypeKind::Never => "Never".into(),
        TypeKind::FnPtr { params, ret } => {
            let r = ret
                .as_ref()
                .map(|t| sw_type(t))
                .unwrap_or_else(|| "Void".into());
            if params.is_empty() {
                format!("() -> {}", r)
            } else {
                format!(
                    "({}) -> {}",
                    params.iter().map(sw_type).collect::<Vec<_>>().join(", "),
                    r
                )
            }
        }
        TypeKind::DynTrait(_) | TypeKind::ImplTrait(_) | TypeKind::Infer => "Any".into(),
    }
}

// ────────────────────────────────────────────────────────────────
// 参数
// ────────────────────────────────────────────────────────────────

fn sw_param(p: &Param) -> Option<String> {
    match &p.kind {
        ParamKind::Self_(_) => None,
        ParamKind::Pattern(pat) => {
            let name = match &pat.kind {
                PatternKind::Ident { name, .. } => sw_ident(&name.name),
                _ => "_arg".into(),
            };
            // Swift external param name: use _ for unlabeled
            Some(format!("_ {}: {}", name, sw_type(&p.ty)))
        }
    }
}

// ────────────────────────────────────────────────────────────────
// 项转译
// ────────────────────────────────────────────────────────────────

fn swift_struct(s: &StructDef) -> String {
    let name = sw_ident(&s.name.name);
    match &s.kind {
        StructKind::Named(fields) => {
            let fs: Vec<String> = fields
                .iter()
                .map(|f| {
                    let fn_ = f
                        .name
                        .as_ref()
                        .map(|n| sw_ident(&n.name))
                        .unwrap_or_else(|| "_".into());
                    let kw = "let";
                    format!("    {} {}: {}", kw, fn_, sw_type(&f.ty))
                })
                .collect();
            format!(
                "struct {} {{\n{}\n}}\n\n",
                name,
                fs.join("\n")
            )
        }
        StructKind::Tuple(fields) => {
            // Swift tuple-like: use a class with stored properties
            let mut o = format!("final class {} {{\n", name);
            for (i, f) in fields.iter().enumerate() {
                o.push_str(&format!(
                    "    let _{}: {}\n",
                    i,
                    sw_type(&f.ty)
                ));
            }
            o.push_str("}\n\n");
            o
        }
        StructKind::Unit => format!("struct {} {{}}\n\n", name),
    }
}

fn swift_enum(e: &EnumDef) -> String {
    let name = sw_ident(&e.name.name);
    let has_data = e
        .variants
        .iter()
        .any(|v| !matches!(&v.fields, StructKind::Unit));
    let indirect = if has_data { "indirect " } else { "" };
    let mut o = format!("{}enum {} {{\n", indirect, name);
    for v in &e.variants {
        let vn = sw_ident(&v.name.name);
        match &v.fields {
            StructKind::Unit => {
                o.push_str(&format!("    case {}\n", vn));
            }
            StructKind::Named(fields) => {
                let fs: Vec<String> = fields
                    .iter()
                    .map(|f| {
                        let fn_ = f
                            .name
                            .as_ref()
                            .map(|n| sw_ident(&n.name))
                            .unwrap_or_else(|| "_".into());
                        format!("{}: {}", fn_, sw_type(&f.ty))
                    })
                    .collect();
                o.push_str(&format!(
                    "    case {}({})\n",
                    vn,
                    fs.join(", ")
                ));
            }
            StructKind::Tuple(fields) => {
                let fs: Vec<String> = fields.iter().map(|f| sw_type(&f.ty)).collect();
                o.push_str(&format!(
                    "    case {}({})\n",
                    vn,
                    fs.join(", ")
                ));
            }
        }
    }
    o.push_str("}\n\n");
    o
}

fn swift_protocol(t: &TraitDef) -> String {
    let name = sw_ident(&t.name.name);
    let mut o = format!("protocol {} {{\n", name);
    for ti in &t.items {
        match ti {
            TraitItem::FnSig(sig) => {
                let ps: Vec<String> = sig.params.iter().filter_map(sw_param).collect();
                let r = sig
                    .ret
                    .as_ref()
                    .map(|t| format!(" -> {}", sw_type(t)))
                    .unwrap_or_default();
                let async_kw = if sig.is_async { "async " } else { "" };
                o.push_str(&format!(
                    "    {}func {}({}){}\n",
                    async_kw,
                    sw_ident(&sig.name.name),
                    ps.join(", "),
                    r
                ));
            }
            TraitItem::Fn(f) => {
                let ps: Vec<String> = f.params.iter().filter_map(sw_param).collect();
                let r = f
                    .ret
                    .as_ref()
                    .map(|t| format!(" -> {}", sw_type(t)))
                    .unwrap_or_default();
                let async_kw = if f.is_async { "async " } else { "" };
                o.push_str(&format!(
                    "    {}func {}({}){} {{\n",
                    async_kw,
                    sw_ident(&f.name.name),
                    ps.join(", "),
                    r
                ));
                if let Some(body) = &f.body {
                    o.push_str(&emit_block_sw(body, 2));
                }
                o.push_str("    }\n");
            }
            TraitItem::Const(_) | TraitItem::TypeAlias(_) => {}
        }
    }
    o.push_str("}\n\n");
    o
}

fn swift_fn(f: &FnDef) -> String {
    let name = sw_ident(&f.name.name);
    let ret = f
        .ret
        .as_ref()
        .map(|t| format!(" -> {}", sw_type(t)))
        .unwrap_or_default();
    let ps: Vec<String> = f.params.iter().filter_map(sw_param).collect();
    let mut o = String::new();
    if f.is_async {
        o.push_str("async ");
    }
    o.push_str(&format!(
        "func {}({}){} {{\n",
        name,
        ps.join(", "),
        ret
    ));
    if let Some(body) = &f.body {
        o.push_str(&emit_block_sw(body, 1));
    }
    o.push_str("}\n\n");
    o
}

fn swift_graph(g: &GraphDef, ctx: &CodegenContext) -> String {
    let gn = sw_ident(&g.name.name);
    let mut o = format!(
        "// graph {} -- scale: {:?}\n",
        g.name.name, ctx.scale
    );
    o.push_str("@main\n");
    o.push_str(&format!("struct {} {{\n", gn));
    o.push_str("    static func main() {\n");
    for gs in &g.body {
        match gs {
            GraphStmt::Node(n) => o.push_str(&format!(
                "        // node {}: {}\n",
                sw_ident(&n.name.name),
                sw_type(&n.ty)
            )),
            GraphStmt::Edge(e) => {
                let ep: Vec<String> = e
                    .endpoints
                    .iter()
                    .map(|p| p.last().name.clone())
                    .collect();
                o.push_str(&format!(
                    "        // edge: {}\n",
                    ep.join(" -> ")
                ));
            }
            GraphStmt::Let(l) => o.push_str(&format!(
                "        {}",
                emit_let_sw(l, 2)
            )),
            GraphStmt::Stmt(s) => o.push_str(&emit_stmt_sw(s, 2)),
            GraphStmt::Item(_) => {}
        }
    }
    o.push_str("    }\n");
    o.push_str("}\n\n");
    o
}

fn swift_extension(imp: &ImplDef) -> String {
    let tn = imp
        .trait_ty
        .as_ref()
        .map(|t| sw_type(t))
        .unwrap_or_default();
    let sn = sw_type(&imp.self_ty);
    let mut o = if !tn.is_empty() {
        format!("extension {}: {} {{\n", sn, tn)
    } else {
        format!("extension {} {{\n", sn)
    };
    for item in &imp.items {
        match item {
            ImplItem::Fn(f) => {
                let fn_ = sw_ident(&f.name.name);
                let r = f
                    .ret
                    .as_ref()
                    .map(|t| format!(" -> {}", sw_type(t)))
                    .unwrap_or_default();
                let ps: Vec<String> = f.params.iter().filter_map(sw_param).collect();
                let async_kw = if f.is_async { "async " } else { "" };
                o.push_str(&format!(
                    "    {}func {}({}){} {{\n",
                    async_kw,
                    fn_,
                    ps.join(", "),
                    r
                ));
                if let Some(body) = &f.body {
                    o.push_str(&emit_block_sw(body, 2));
                }
                o.push_str("    }\n");
            }
            ImplItem::Const(c) => {
                o.push_str(&format!(
                    "    static let {} = {}\n",
                    sw_ident(&c.name.name),
                    emit_expr_sw(&c.value, 0)
                ));
            }
            ImplItem::TypeAlias(_) => {}
        }
    }
    o.push_str("}\n\n");
    o
}

fn swift_const(c: &ConstDef) -> String {
    format!(
        "static let {}: {} = {}\n\n",
        sw_ident(&c.name.name),
        sw_type(&c.ty),
        emit_expr_sw(&c.value, 0)
    )
}

fn swift_typealias(a: &TypeAliasDef) -> String {
    format!(
        "// Type alias: {} = {}\n",
        sw_ident(&a.name.name),
        sw_type(&a.ty)
    )
}

fn swift_macro_rules(m: &MacroRulesDefinition) -> String {
    format!(
        "// macro_rules {} -- not directly translatable to Swift\n\n",
        sw_ident(&m.name.name)
    )
}

// ────────────────────────────────────────────────────────────────
// 表达式转译
// ────────────────────────────────────────────────────────────────

pub fn emit_expr_sw(expr: &Expr, indent: usize) -> String {
    let ind = "    ".repeat(indent);
    match &expr.kind {
        ExprKind::Literal(lit) => sw_literal(lit),
        ExprKind::Path(p) => sw_ident(&p.last().name),
        ExprKind::Binary { op, lhs, rhs } => format!(
            "({} {} {})",
            emit_expr_sw(lhs, 0),
            sw_binop(op),
            emit_expr_sw(rhs, 0)
        ),
        ExprKind::Unary { op, operand } => {
            let uop = sw_unop(op);
            if uop.is_empty() {
                emit_expr_sw(operand, 0)
            } else {
                format!("{}{}", uop, emit_expr_sw(operand, 0))
            }
        }
        ExprKind::Call { callee, args } => {
            let as_ = args.iter().map(|a| emit_expr_sw(a, 0)).collect::<Vec<_>>();
            format!("{}({})", emit_expr_sw(callee, 0), as_.join(", "))
        }
        ExprKind::MethodCall { receiver, method, generic_args: _, args } => {
            let mn = sw_ident(&method.name);
            let as_ = args.iter().map(|a| emit_expr_sw(a, 0)).collect::<Vec<_>>();
            format!("{}.{}({})", emit_expr_sw(receiver, 0), mn, as_.join(", "))
        }
        ExprKind::Field { base, field } => {
            let fn_ = match field {
                FieldIndex::Named(id) => sw_ident(&id.name),
                FieldIndex::Index(i, _) => format!("_{}", i),
            };
            format!("{}.{}", emit_expr_sw(base, 0), fn_)
        }
        ExprKind::Index { base, index } => {
            format!("{}[{}]", emit_expr_sw(base, 0), emit_expr_sw(index, 0))
        }
        ExprKind::Slice { base, range } => {
            let bs = emit_expr_sw(base, 0);
            let s = range
                .lo
                .as_ref()
                .map(|e| emit_expr_sw(e, 0))
                .unwrap_or_else(|| "0".into());
            let e = range
                .hi
                .as_ref()
                .map(|e| emit_expr_sw(e, 0))
                .unwrap_or_else(|| format!("{}.count", bs));
            // Swift: Array(bs[s..<e]) or Array(bs[s...e])
            if range.inclusive {
                format!("Array({}[{}...{}])", bs, s, e)
            } else {
                format!("Array({}[{}..<{}])", bs, s, e)
            }
        }
        ExprKind::Range(r) => {
            let l = r
                .lo
                .as_ref()
                .map(|e| emit_expr_sw(e, 0))
                .unwrap_or_else(|| "0".into());
            let hi = r
                .hi
                .as_ref()
                .map(|e| emit_expr_sw(e, 0))
                .unwrap_or_else(|| "???".into());
            if r.inclusive {
                format!("{}...{}", l, hi)
            } else {
                format!("{}..<{}", l, hi)
            }
        }
        ExprKind::Assign { lhs, rhs } => {
            format!("{} = {}", emit_expr_sw(lhs, 0), emit_expr_sw(rhs, 0))
        }
        ExprKind::CompoundAssign { op, lhs, rhs } => {
            format!(
                "{} {}= {}",
                emit_expr_sw(lhs, 0),
                sw_binop(op).trim(),
                emit_expr_sw(rhs, 0)
            )
        }
        ExprKind::If { cond, then, else_ } => {
            let mut o = format!("if {} {{\n", emit_expr_sw(cond, 0));
            o.push_str(&emit_block_sw(then, indent + 1));
            if let Some(els) = else_ {
                o.push_str(&format!("{}}} else ", ind));
                match &els.kind {
                    ExprKind::If { .. } => {
                        // else if chain
                        o.push_str(&emit_expr_sw(els, indent));
                    }
                    ExprKind::Block(b) => {
                        o.push_str("{\n");
                        o.push_str(&emit_block_sw(b, indent + 1));
                        o.push_str(&format!("{}}}\n", ind));
                    }
                    _ => {
                        o.push_str(&emit_expr_sw(els, 0));
                        o.push('\n');
                    }
                }
            } else {
                o.push_str(&format!("{}}}\n", ind));
            }
            o
        }
        ExprKind::Match { scrutinee, arms } => {
            let mut o = format!("switch {} {{\n", emit_expr_sw(scrutinee, 0));
            for arm in arms {
                let p = sw_pattern(&arm.pattern);
                let g = if let Some(g) = &arm.guard {
                    format!(" where {}", emit_expr_sw(g, 0))
                } else {
                    String::new()
                };
                match &arm.body.kind {
                    ExprKind::Block(b) => {
                        o.push_str(&format!(
                            "{}    case {}: \n",
                            ind, p
                        ));
                        o.push_str(&format!(
                            "{}{}{}:\n",
                            ind, ind, g
                        ));
                        o.push_str(&emit_block_sw(b, indent + 2));
                    }
                    _ => {
                        o.push_str(&format!(
                            "{}    case {}{}: {}\n",
                            ind,
                            p,
                            g,
                            emit_expr_sw(&arm.body, 0)
                        ));
                    }
                }
            }
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::For { label: _, pattern, iter, body } => {
            let mut o = format!(
                "for {} in {} {{\n",
                sw_pattern(pattern),
                emit_expr_sw(iter, 0)
            );
            o.push_str(&emit_block_sw(body, indent + 1));
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::While { label: _, cond, body } => {
            let mut o = format!("while {} {{\n", emit_expr_sw(cond, 0));
            o.push_str(&emit_block_sw(body, indent + 1));
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::WhileLet { label: _, pattern, expr: scrut, body } => {
            // while let pat = expr → while case expr = pat
            let mut o = format!(
                "while case let {} = {} {{\n",
                sw_pattern(pattern),
                emit_expr_sw(scrut, 0)
            );
            o.push_str(&emit_block_sw(body, indent + 1));
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::Loop { label, body } => {
            let mut o = String::new();
            if let Some(l) = label {
                o.push_str(&format!("{}: ", sw_ident(&l.name)));
            }
            o.push_str("while true {\n");
            o.push_str(&emit_block_sw(body, indent + 1));
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::Closure { params, body, ret: _, .. } => {
            let ps: Vec<String> = params
                .iter()
                .filter_map(|p| match &p.kind {
                    ParamKind::Pattern(pat) => match &pat.kind {
                        PatternKind::Ident { name, .. } => Some(sw_ident(&name.name)),
                        _ => Some("x".into()),
                    },
                    _ => None,
                })
                .collect();
            if let ExprKind::Block(be) = &body.kind {
                if be.stmts.is_empty() {
                    if let Some(tail) = &be.tail {
                        return format!(
                            "{{ {} in {} }}",
                            ps.join(", "),
                            emit_expr_sw(tail, 0)
                        );
                    }
                }
                let mut o = format!("{{ {} in\n", ps.join(", "));
                o.push_str(&emit_block_sw(be, indent + 1));
                o.push_str(&format!("{}}}\n", ind));
                o
            } else {
                format!(
                    "{{ {} in {} }}",
                    ps.join(", "),
                    emit_expr_sw(body, 0)
                )
            }
        }
        ExprKind::Return(value) => {
            if let Some(v) = value {
                format!("return {}", emit_expr_sw(v, 0))
            } else {
                "return".into()
            }
        }
        ExprKind::Break { label, value } => {
            let mut o = "break".to_string();
            if let Some(l) = label {
                o = format!("break {}", sw_ident(&l.name));
            }
            if let Some(v) = value {
                o.push_str(&format!(" /* with value: {} */", emit_expr_sw(v, 0)));
            }
            o
        }
        ExprKind::Continue { label } => {
            if let Some(l) = label {
                format!("continue {}", sw_ident(&l.name))
            } else {
                "continue".into()
            }
        }
        ExprKind::Array(elems) => {
            let es: Vec<String> = elems.iter().map(|e| emit_expr_sw(e, 0)).collect();
            if es.is_empty() {
                "[Any]()".into()
            } else {
                format!("[{}]", es.join(", "))
            }
        }
        ExprKind::ArrayRepeat { elem, count } => {
            format!(
                "Array(repeating: {}, count: {})",
                emit_expr_sw(elem, 0),
                emit_expr_sw(count, 0)
            )
        }
        ExprKind::Struct { path, fields, spread } => {
            let name = sw_ident(&path.last().name);
            let fs: Vec<String> = fields
                .iter()
                .map(|f| {
                    let fn_ = match &f.name {
                        FieldIndex::Named(id) => sw_ident(&id.name),
                        FieldIndex::Index(i, _) => format!("_{}", i),
                    };
                    let v = f
                        .value
                        .as_ref()
                        .map(|v| emit_expr_sw(v, 0))
                        .unwrap_or_else(|| fn_.clone());
                    format!("{}: {}", fn_, v)
                })
                .collect();
            let spread_str = if let Some(spread) = spread {
                format!("/* ..{} */", emit_expr_sw(spread, 0))
            } else {
                String::new()
            };
            format!(
                "{}({}{}{})",
                name,
                fs.join(", "),
                if fs.is_empty() { "" } else { ", " },
                spread_str
            )
        }
        ExprKind::Tuple(elems) => {
            let es: Vec<String> = elems.iter().map(|e| emit_expr_sw(e, 0)).collect();
            if es.is_empty() {
                "()".into()
            } else {
                format!("({})", es.join(", "))
            }
        }
        ExprKind::Block(be) => {
            let mut o = "do {\n".to_string();
            o.push_str(&emit_block_sw(be, indent + 1));
            if let Some(tail) = &be.tail {
                o.push_str(&format!(
                    "{}return {}\n",
                    ind,
                    emit_expr_sw(tail, 0)
                ));
            }
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::AsyncBlock { body, .. } => {
            let mut o = "// async ".to_string();
            o.push_str(&emit_block_sw(body, indent));
            o
        }
        ExprKind::Try(inner) => {
            let inner_str = emit_expr_sw(inner, 0);
            format!(
                "try {}",
                inner_str
            )
        }
        ExprKind::Await(inner) => emit_expr_sw(inner, 0),
        ExprKind::Cast { expr: inner, ty } => {
            format!("{} as! {}", emit_expr_sw(inner, 0), sw_type(ty))
        }
        ExprKind::IfLet { pattern, expr: scrut, then, else_ } => {
            let mut o = format!(
                "if case let {} = {} {{\n",
                sw_pattern(pattern),
                emit_expr_sw(scrut, 0)
            );
            o.push_str(&emit_block_sw(then, indent + 1));
            if let Some(els) = else_ {
                o.push_str(&format!("{}}} else ", ind));
                o.push_str(&emit_expr_sw(els, indent));
            } else {
                o.push_str(&format!("{}}}\n", ind));
            }
            o
        }
        ExprKind::Macro { path, args: _ } => sw_macro(path.last().name.as_str()),
        ExprKind::Native(_) => "// native block".into(),
    }
}

fn sw_literal(lit: &Literal) -> String {
    match &lit.kind {
        LiteralKind::Str { value, .. } => {
            format!(
                "\"{}\"",
                value.replace('\\', "\\\\").replace('"', "\\\"")
            )
        }
        LiteralKind::Bool(b) => {
            if *b {
                "true".into()
            } else {
                "false".into()
            }
        }
        LiteralKind::Int { value, .. } => {
            let s = value.to_string();
            if *value < 0 {
                format!("({})", s)
            } else {
                s
            }
        }
        LiteralKind::Float { value, .. } => {
            let s = value.to_string();
            if !s.contains('.') {
                format!("{}.0", s)
            } else {
                s
            }
        }
        LiteralKind::Char(c) => format!("'{}'", c),
    }
}

fn sw_binop(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Rem => "%",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Gt => ">",
        BinaryOp::Le => "<=",
        BinaryOp::Ge => ">=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
        BinaryOp::Shl => "<<",
        BinaryOp::Shr => ">>",
    }
}

fn sw_unop(op: &UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Not => "!",
        UnaryOp::Deref | UnaryOp::Ref | UnaryOp::RefMut => "",
    }
}

fn sw_pattern(pat: &Pattern) -> String {
    match &pat.kind {
        PatternKind::Wildcard => "_".into(),
        PatternKind::Ident { name, .. } => sw_ident(&name.name),
        PatternKind::Literal(lit) => sw_literal(lit),
        PatternKind::Path(p) => sw_capitalize(&sw_ident(&p.last().name)),
        PatternKind::TupleStruct { path, elems, .. } => {
            let n = sw_capitalize(&sw_ident(&path.last().name));
            let es: Vec<String> = elems.iter().map(sw_pattern).collect();
            if es.is_empty() {
                format!(".{}", n)
            } else {
                format!(".{}({})", n, es.join(", "))
            }
        }
        PatternKind::Struct { path, fields, .. } => {
            let n = sw_capitalize(&sw_ident(&path.last().name));
            let fs: Vec<String> = fields
                .iter()
                .map(|f| {
                    let fn_ = sw_ident(&f.name.name);
                    let p = f
                        .pattern
                        .as_ref()
                        .map(|p| sw_pattern(p))
                        .unwrap_or_else(|| sw_ident(&f.name.name));
                    format!("{}: {}", fn_, p)
                })
                .collect();
            format!(
                ".{}({}{}{})",
                n,
                fs.join(", "),
                if fs.is_empty() { "" } else { ", " },
                "/* .. */"
            )
        }
        PatternKind::Tuple { elems, .. } => {
            let es: Vec<String> = elems.iter().map(sw_pattern).collect();
            format!("({})", es.join(", "))
        }
        PatternKind::Or(elems) => elems
            .iter()
            .map(|e| sw_pattern(e))
            .collect::<Vec<_>>()
            .join(", "),
        PatternKind::Range { lo, hi, inclusive } => {
            let l = sw_pattern(lo);
            let r = sw_pattern(hi);
            if *inclusive {
                format!("{}...{}", l, r)
            } else {
                format!("{}..<{}", l, r)
            }
        }
        PatternKind::Rest => "/* .. */".into(),
    }
}

fn sw_macro(name: &str) -> String {
    match name {
        "println" => "print".into(),
        "eprintln" => "flockPrint(stderr:)".into(),
        "format" => "String(format:)".into(),
        "todo" | "unimplemented" => "fatalError()".into(),
        "panic" => "fatalError()".into(),
        "vec" => "[Any]".into(),
        _ => format!("/* macro: {} */", name),
    }
}

// ────────────────────────────────────────────────────────────────
// 语句块转译
// ────────────────────────────────────────────────────────────────

pub fn emit_block_sw(be: &BlockExpr, indent: usize) -> String {
    let ind = "    ".repeat(indent);
    let mut o = String::new();
    for stmt in &be.stmts {
        match stmt {
            Stmt::Let(l) => o.push_str(&format!("{}{}", ind, emit_let_sw(l, 0))),
            Stmt::Item(_) | Stmt::Empty(_) => {}
            Stmt::Expr { expr, has_semi: _ } => {
                match &expr.kind {
                    ExprKind::If { .. }
                    | ExprKind::Match { .. }
                    | ExprKind::For { .. }
                    | ExprKind::While { .. }
                    | ExprKind::WhileLet { .. }
                    | ExprKind::Loop { .. }
                    | ExprKind::Block(_)
                    | ExprKind::Try(_)
                    | ExprKind::AsyncBlock { .. }
                    | ExprKind::IfLet { .. } => {
                        o.push_str(&format!("{}\n", emit_expr_sw(expr, indent)));
                    }
                    ExprKind::Return(_) => {
                        o.push_str(&format!(
                            "{}{}\n",
                            ind,
                            emit_expr_sw(expr, 0)
                        ));
                    }
                    _ => {
                        o.push_str(&format!(
                            "{}{}\n",
                            ind,
                            emit_expr_sw(expr, 0)
                        ));
                    }
                }
            }
        }
    }
    if let Some(tail) = &be.tail {
        match &tail.kind {
            ExprKind::If { .. }
            | ExprKind::Match { .. }
            | ExprKind::Block(_) => {
                o.push_str(&format!("{}", emit_expr_sw(tail, indent)));
            }
            _ => {
                o.push_str(&format!(
                    "{}{}\n",
                    ind,
                    emit_expr_sw(tail, 0)
                ));
            }
        }
    }
    o
}

fn emit_let_sw(l: &LetStmt, _indent: usize) -> String {
    let pat = sw_pattern(&l.pattern);
    let kw = if l.mutable { "var" } else { "let" };
    if let Some(ty) = &l.ty {
        format!(
            "{} {}: {} = {}\n",
            kw,
            pat,
            sw_type(ty),
            l.init
                .as_ref()
                .map(|e| emit_expr_sw(e, 0))
                .unwrap_or_else(|| "/* TODO */".into())
        )
    } else {
        format!(
            "{} {} = {}\n",
            kw,
            pat,
            l.init
                .as_ref()
                .map(|e| emit_expr_sw(e, 0))
                .unwrap_or_else(|| "/* TODO */".into())
        )
    }
}

pub fn emit_stmt_sw(stmt: &Stmt, indent: usize) -> String {
    match stmt {
        Stmt::Let(l) => {
            let ind = "    ".repeat(indent);
            format!("{}{}", ind, emit_let_sw(l, 0))
        }
        Stmt::Item(_) | Stmt::Empty(_) => String::new(),
        Stmt::Expr { expr, has_semi: _ } => {
            match &expr.kind {
                ExprKind::If { .. }
                | ExprKind::Match { .. }
                | ExprKind::For { .. }
                | ExprKind::While { .. }
                | ExprKind::WhileLet { .. }
                | ExprKind::Loop { .. }
                | ExprKind::Block(_)
                | ExprKind::Try(_)
                | ExprKind::AsyncBlock { .. }
                | ExprKind::IfLet { .. } => emit_expr_sw(expr, indent) + "\n",
                _ => {
                    let ind = "    ".repeat(indent);
                    format!("{}{}\n", ind, emit_expr_sw(expr, 0))
                }
            }
        }
    }
}