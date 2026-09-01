//! Kotlin backend (Logic tier) -- type mapping + full function body translation
//! Kotlin is a modern JVM language. This backend generates Kotlin 1.7+ code.
//! struct -> data class (named) / class (tuple/unit);
//! enum -> sealed class/interface (data) / enum class (unit);
//! trait -> interface (with default implementations);
//! impl -> class implementing interface;
//! fn -> top-level function / companion object method;
//! const -> const val / companion object val;
//! graph -> fun main(args: Array<String>)

use crate::ast::*;
use crate::codegen::{CodegenBackend, CodegenContext};

pub struct KotlinBackend;

impl CodegenBackend for KotlinBackend {
    fn lang(&self) -> &'static str {
        "kotlin"
    }

    fn emit_item(&self, ctx: &CodegenContext, item: &Item) -> Result<String, String> {
        let mut out = String::new();
        out.push_str(&format!("// {}\n", crate::sourcemap::generated_header("kotlin")));
        out.push_str("// HSL-generated Kotlin code — do not edit manually\n\n");

        match item {
            Item::Struct(s) => out.push_str(&kt_struct(s)),
            Item::Enum(e) => out.push_str(&kt_enum(e)),
            Item::Trait(t) => out.push_str(&kt_trait(t)),
            Item::Fn(f) => out.push_str(&kt_fn(f)),
            Item::Graph(g) => out.push_str(&kt_graph(g, ctx)),
            Item::Impl(imp) => out.push_str(&kt_impl(imp)),
            Item::Const(c) => out.push_str(&kt_const(c)),
            Item::TypeAlias(a) => out.push_str(&kt_typealias(a)),
            Item::MacroRules(m) => out.push_str(&kt_macro_rules(m)),
            _ => {
                return Err(format!(
                    "kotlin backend does not support {}",
                    crate::ast::item_kind_name(item)
                ))
            }
        }
        Ok(out)
    }
}

// ────────────────────────────────────────────────────────────────
// Kotlin 关键字列表
// ────────────────────────────────────────────────────────────────

const KT_KW: &[&str] = &[
    "as", "as?", "break", "class", "continue", "do", "else", "false", "for",
    "fun", "if", "in", "!in", "interface", "is", "!is", "null", "object",
    "package", "return", "super", "this", "throw", "true", "try", "typealias",
    "val", "var", "when", "while",
    "abstract", "actual", "annotation", "companion", "const", "constructor",
    "crossinline", "data", "enum", "external", "final", "infix", "inline",
    "inner", "internal", "lateinit", "noinline", "open", "operator", "out",
    "override", "private", "protected", "public", "reified", "sealed", "suspend",
    "tailrec", "vararg", "where", "by", "catch", "finally", "import",
    "init", "dynamic", "field", "property", "get", "set", "it", "_",
];

fn kt_ident(s: &str) -> String {
    if KT_KW.contains(&s) {
        format!("{}_", s)
    } else {
        s.to_string()
    }
}

fn kt_capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

// ────────────────────────────────────────────────────────────────
// 类型映射
// ────────────────────────────────────────────────────────────────

fn kt_generic_arg(arg: &GenericArg) -> String {
    match arg {
        GenericArg::Type(t) => kt_type(t),
        GenericArg::Const(c) => match &c.kind {
            ConstArgKind::Literal(lit) => lit.raw.clone(),
            ConstArgKind::Block(_) => "0".into(),
        },
    }
}

fn kt_two_generic_args(args: &[GenericArg]) -> (String, String) {
    let mut it = args.iter();
    (
        it.next()
            .map(kt_generic_arg)
            .unwrap_or_else(|| "Any".into()),
        it.next()
            .map(kt_generic_arg)
            .unwrap_or_else(|| "Any".into()),
    )
}

fn kt_type(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(pt) => {
            let name = pt.path.last().name.as_str();
            match name {
                "String" | "str" => "String".into(),
                "char" => "Char".into(),
                "bool" => "Boolean".into(),
                "i8" => "Byte".into(),
                "i16" => "Short".into(),
                "i32" => "Int".into(),
                "i64" => "Long".into(),
                "u8" | "u16" | "u32" => "Int".into(),
                "u64" => "Long".into(),
                "usize" | "isize" => "Int".into(),
                "f32" => "Float".into(),
                "f64" => "Double".into(),
                "Vec" => format!(
                    "List<{}>",
                    pt.generic_args
                        .iter()
                        .next()
                        .map(kt_generic_arg)
                        .unwrap_or_else(|| "Any".into())
                ),
                "HashMap" => {
                    let (k, v) = kt_two_generic_args(&pt.generic_args);
                    format!("HashMap<{}, {}>", k, v)
                }
                "HashSet" => format!(
                    "HashSet<{}>",
                    pt.generic_args
                        .iter()
                        .next()
                        .map(kt_generic_arg)
                        .unwrap_or_else(|| "Any".into())
                ),
                "Option" => format!(
                    "{}?",
                    pt.generic_args
                        .iter()
                        .next()
                        .map(kt_generic_arg)
                        .unwrap_or_else(|| "Any".into())
                ),
                "Result" => {
                    if !pt.generic_args.is_empty() {
                        kt_generic_arg(&pt.generic_args[0])
                    } else {
                        "Any".into()
                    }
                }
                "Box" => {
                    if !pt.generic_args.is_empty() {
                        kt_generic_arg(&pt.generic_args[0])
                    } else {
                        "Any".into()
                    }
                }
                _ => kt_ident(name),
            }
        }
        TypeKind::Ref { inner, .. } => kt_type(inner),
        TypeKind::Tuple(elems) => {
            if elems.is_empty() {
                "Unit".into()
            } else {
                format!(
                    "Pair<{}, {}>",
                    kt_type(&elems[0]),
                    if elems.len() > 1 {
                        kt_type(&elems[1])
                    } else {
                        "Unit".into()
                    }
                )
            }
        }
        TypeKind::Array { elem, .. } => format!("Array<{}>", kt_type(elem)),
        TypeKind::Slice(inner) => format!("List<{}>", kt_type(inner)),
        TypeKind::Paren(inner) => kt_type(inner),
        TypeKind::Never => "Nothing".into(),
        TypeKind::FnPtr { params, ret } => {
            let r = ret
                .as_ref()
                .map(|t| kt_type(t))
                .unwrap_or_else(|| "Unit".into());
            if params.is_empty() {
                format!("() -> {}", r)
            } else {
                format!(
                    "({}) -> {}",
                    params.iter().map(kt_type).collect::<Vec<_>>().join(", "),
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

fn kt_param(p: &Param) -> Option<String> {
    match &p.kind {
        ParamKind::Self_(_) => None,
        ParamKind::Pattern(pat) => {
            let name = match &pat.kind {
                PatternKind::Ident { name, .. } => kt_ident(&name.name),
                _ => "_arg".into(),
            };
            Some(format!("{}: {}", name, kt_type(&p.ty)))
        }
    }
}

// ────────────────────────────────────────────────────────────────
// 项转译
// ────────────────────────────────────────────────────────────────

fn kt_struct(s: &StructDef) -> String {
    let name = kt_ident(&s.name.name);
    match &s.kind {
        StructKind::Named(fields) => {
            let fs: Vec<String> = fields
                .iter()
                .map(|f| {
                    let fn_ = f
                        .name
                        .as_ref()
                        .map(|n| kt_ident(&n.name))
                        .unwrap_or_else(|| "_".into());
                    format!("    val {}: {}", fn_, kt_type(&f.ty))
                })
                .collect();
            format!("data class {}(\n{}\n)\n\n", name, fs.join(",\n"))
        }
        StructKind::Tuple(fields) => {
            let mut o = format!("class {}(\n", name);
            for (i, f) in fields.iter().enumerate() {
                o.push_str(&format!(
                    "    val component{}: {}\n",
                    i + 1,
                    kt_type(&f.ty)
                ));
            }
            o.push_str(")\n\n");
            o
        }
        StructKind::Unit => format!("class {}\n\n", name),
    }
}

fn kt_enum(e: &EnumDef) -> String {
    let name = kt_ident(&e.name.name);
    let has_data = e
        .variants
        .iter()
        .any(|v| !matches!(&v.fields, StructKind::Unit));
    if !has_data {
        // 简单枚举 → enum class
        let mut o = format!("enum class {} {{\n", name);
        for (i, v) in e.variants.iter().enumerate() {
            let comma = if i < e.variants.len() - 1 {
                ","
            } else {
                ""
            };
            o.push_str(&format!(
                "    {}{}\n",
                kt_capitalize(&kt_ident(&v.name.name)),
                comma
            ));
        }
        o.push_str("}\n\n");
        o
    } else {
        // 带数据的枚举 → sealed class/interface + data class
        let mut o = format!("sealed class {}\n\n", name);
        for v in &e.variants {
            let vn = kt_capitalize(&kt_ident(&v.name.name));
            match &v.fields {
                StructKind::Unit => {
                    o.push_str(&format!(
                        "object {} : {}()\n\n",
                        vn, name
                    ));
                }
                StructKind::Named(fields) => {
                    let fs: Vec<String> = fields
                        .iter()
                        .map(|f| {
                            let fn_ = f
                                .name
                                .as_ref()
                                .map(|n| kt_ident(&n.name))
                                .unwrap_or_else(|| "_".into());
                            format!("    val {}: {}", fn_, kt_type(&f.ty))
                        })
                        .collect();
                    o.push_str(&format!(
                        "data class {}(\n{}\n) : {}()\n\n",
                        vn,
                        fs.join(",\n"),
                        name
                    ));
                }
                StructKind::Tuple(fields) => {
                    let fs: Vec<String> = fields.iter().map(|f| kt_type(&f.ty)).collect();
                    o.push_str(&format!(
                        "data class {}({}) : {}()\n\n",
                        vn,
                        fs.join(", "),
                        name
                    ));
                }
            }
        }
        o
    }
}

fn kt_trait(t: &TraitDef) -> String {
    let name = kt_ident(&t.name.name);
    let mut o = format!("interface {} {{\n", name);
    for ti in &t.items {
        match ti {
            TraitItem::FnSig(sig) => {
                let ps: Vec<String> = sig.params.iter().filter_map(kt_param).collect();
                let r = sig
                    .ret
                    .as_ref()
                    .map(|t| format!(": {}", kt_type(t)))
                    .unwrap_or_default();
                o.push_str(&format!(
                    "    fun {}({}){}\n",
                    kt_ident(&sig.name.name),
                    ps.join(", "),
                    r
                ));
            }
            TraitItem::Fn(f) => {
                let ps: Vec<String> = f.params.iter().filter_map(kt_param).collect();
                let r = f
                    .ret
                    .as_ref()
                    .map(|t| format!(": {}", kt_type(t)))
                    .unwrap_or_default();
                o.push_str(&format!(
                    "    fun {}({}){} {{\n",
                    kt_ident(&f.name.name),
                    ps.join(", "),
                    r
                ));
                if let Some(body) = &f.body {
                    o.push_str(&emit_block_kt(body, 2));
                }
                o.push_str("    }\n");
            }
            TraitItem::Const(_) | TraitItem::TypeAlias(_) => {}
        }
    }
    o.push_str("}\n\n");
    o
}

fn kt_fn(f: &FnDef) -> String {
    let name = kt_ident(&f.name.name);
    let ret = f
        .ret
        .as_ref()
        .map(|t| format!(": {}", kt_type(t)))
        .unwrap_or_default();
    let ps: Vec<String> = f.params.iter().filter_map(kt_param).collect();
    let mut o = String::new();
    if f.is_async {
        o.push_str("suspend ");
    }
    o.push_str(&format!(
        "fun {}({}){} {{\n",
        name,
        ps.join(", "),
        ret
    ));
    if let Some(body) = &f.body {
        o.push_str(&emit_block_kt(body, 1));
    }
    o.push_str("}\n\n");
    o
}

fn kt_graph(g: &GraphDef, ctx: &CodegenContext) -> String {
    let gn = kt_ident(&g.name.name);
    let mut o = format!(
        "// graph {} -- scale: {:?}\n",
        g.name.name, ctx.scale
    );
    o.push_str(&format!(
        "class {} {{\n    companion object {{\n",
        gn
    ));
    o.push_str("        @JvmStatic\n");
    o.push_str("        fun main(args: Array<String>) {\n");
    for gs in &g.body {
        match gs {
            GraphStmt::Node(n) => o.push_str(&format!(
                "            // node {}: {}\n",
                kt_ident(&n.name.name),
                kt_type(&n.ty)
            )),
            GraphStmt::Edge(e) => {
                let ep: Vec<String> = e
                    .endpoints
                    .iter()
                    .map(|p| p.last().name.clone())
                    .collect();
                o.push_str(&format!(
                    "            // edge: {}\n",
                    ep.join(" -> ")
                ));
            }
            GraphStmt::Let(l) => o.push_str(&format!(
                "            {}",
                emit_let_kt(l, 3)
            )),
            GraphStmt::Stmt(s) => o.push_str(&emit_stmt_kt(s, 3)),
            GraphStmt::Item(_) => {}
        }
    }
    o.push_str("        }\n");
    o.push_str("    }\n");
    o.push_str("}\n\n");
    o
}

fn kt_impl(imp: &ImplDef) -> String {
    let tn = imp
        .trait_ty
        .as_ref()
        .map(|t| kt_type(t))
        .unwrap_or_default();
    let sn = kt_type(&imp.self_ty);
    let mut o = if !tn.is_empty() {
        format!("class {} : {} {{\n", sn, tn)
    } else {
        format!("class {} {{\n", sn)
    };
    for item in &imp.items {
        match item {
            ImplItem::Fn(f) => {
                let fn_ = kt_ident(&f.name.name);
                let r = f
                    .ret
                    .as_ref()
                    .map(|t| format!(": {}", kt_type(t)))
                    .unwrap_or_default();
                let ps: Vec<String> = f.params.iter().filter_map(kt_param).collect();
                o.push_str(&format!(
                    "    fun {}({}){} {{\n",
                    fn_,
                    ps.join(", "),
                    r
                ));
                if let Some(body) = &f.body {
                    o.push_str(&emit_block_kt(body, 2));
                }
                o.push_str("    }\n");
            }
            ImplItem::Const(c) => {
                o.push_str(&format!(
                    "    companion object {{ const val {} = {} }}\n",
                    kt_ident(&c.name.name),
                    emit_expr_kt(&c.value, 0)
                ));
            }
            ImplItem::TypeAlias(_) => {}
        }
    }
    o.push_str("}\n\n");
    o
}

fn kt_const(c: &ConstDef) -> String {
    format!(
        "const val {}: {} = {}\n\n",
        kt_ident(&c.name.name),
        kt_type(&c.ty),
        emit_expr_kt(&c.value, 0)
    )
}

fn kt_typealias(a: &TypeAliasDef) -> String {
    format!(
        "// Type alias: {} = {}\n",
        kt_ident(&a.name.name),
        kt_type(&a.ty)
    )
}

fn kt_macro_rules(m: &MacroRulesDefinition) -> String {
    format!(
        "// macro_rules {} -- not directly translatable to Kotlin\n\n",
        kt_ident(&m.name.name)
    )
}

// ────────────────────────────────────────────────────────────────
// 表达式转译
// ────────────────────────────────────────────────────────────────

pub fn emit_expr_kt(expr: &Expr, indent: usize) -> String {
    let ind = "    ".repeat(indent);
    match &expr.kind {
        ExprKind::Literal(lit) => kt_literal(lit),
        ExprKind::Path(p) => kt_ident(&p.last().name),
        ExprKind::Binary { op, lhs, rhs } => format!(
            "({} {} {})",
            emit_expr_kt(lhs, 0),
            kt_binop(op),
            emit_expr_kt(rhs, 0)
        ),
        ExprKind::Unary { op, operand } => format!(
            "{}{}",
            kt_unop(op),
            emit_expr_kt(operand, 0)
        ),
        ExprKind::Call { callee, args } => {
            let as_ = args.iter().map(|a| emit_expr_kt(a, 0)).collect::<Vec<_>>();
            format!("{}({})", emit_expr_kt(callee, 0), as_.join(", "))
        }
        ExprKind::MethodCall { receiver, method, generic_args: _, args } => {
            let mn = kt_ident(&method.name);
            let as_ = args.iter().map(|a| emit_expr_kt(a, 0)).collect::<Vec<_>>();
            format!("{}.{}({})", emit_expr_kt(receiver, 0), mn, as_.join(", "))
        }
        ExprKind::Field { base, field } => {
            let fn_ = match field {
                FieldIndex::Named(id) => kt_ident(&id.name),
                FieldIndex::Index(i, _) => format!("component{}", i + 1),
            };
            format!("{}.{}", emit_expr_kt(base, 0), fn_)
        }
        ExprKind::Index { base, index } => {
            format!("{}[{}]", emit_expr_kt(base, 0), emit_expr_kt(index, 0))
        }
        ExprKind::Slice { base, range } => {
            let bs = emit_expr_kt(base, 0);
            let s = range
                .lo
                .as_ref()
                .map(|e| emit_expr_kt(e, 0))
                .unwrap_or_else(|| "0".into());
            let e = range
                .hi
                .as_ref()
                .map(|e| emit_expr_kt(e, 0))
                .unwrap_or_else(|| format!("{}.size", bs));
            if range.inclusive {
                format!("{}.subList({}, {} + 1)", bs, s, e)
            } else {
                format!("{}.subList({}, {})\n", bs, s, e)
            }
        }
        ExprKind::Range(r) => {
            let l = r
                .lo
                .as_ref()
                .map(|e| emit_expr_kt(e, 0))
                .unwrap_or_else(|| "0".into());
            let hi = r
                .hi
                .as_ref()
                .map(|e| emit_expr_kt(e, 0))
                .unwrap_or_else(|| "???".into());
            if r.inclusive {
                format!("{}..{}", l, hi)
            } else {
                format!("{} until {}", l, hi)
            }
        }
        ExprKind::Assign { lhs, rhs } => {
            format!("{} = {}", emit_expr_kt(lhs, 0), emit_expr_kt(rhs, 0))
        }
        ExprKind::CompoundAssign { op, lhs, rhs } => {
            format!(
                "{} {}= {}",
                emit_expr_kt(lhs, 0),
                kt_binop(op).trim(),
                emit_expr_kt(rhs, 0)
            )
        }
        ExprKind::If { cond, then, else_ } => {
            // Kotlin uses if as expression natively
            let mut o = format!("if ({}) {{\n", emit_expr_kt(cond, 0));
            o.push_str(&emit_block_kt(then, indent + 1));
            if let Some(els) = else_ {
                o.push_str(&format!("{}}} else ", ind));
                match &els.kind {
                    ExprKind::If { .. } => {
                        // else if chain
                        o.push_str(&emit_expr_kt(els, indent));
                    }
                    ExprKind::Block(b) => {
                        o.push_str("{\n");
                        o.push_str(&emit_block_kt(b, indent + 1));
                        o.push_str(&format!("{}}}\n", ind));
                    }
                    _ => {
                        o.push_str(&emit_expr_kt(els, 0));
                        o.push('\n');
                    }
                }
            } else {
                o.push_str(&format!("{}}}\n", ind));
            }
            o
        }
        ExprKind::Match { scrutinee, arms } => {
            let mut o = format!("when ({}) {{\n", emit_expr_kt(scrutinee, 0));
            for arm in arms {
                let p = kt_pattern(&arm.pattern);
                let g = if let Some(g) = &arm.guard {
                    format!(" if ({})", emit_expr_kt(g, 0))
                } else {
                    String::new()
                };
                match &arm.body.kind {
                    ExprKind::Block(b) => {
                        o.push_str(&format!(
                            "{}    {}{} -> {{\n",
                            ind, p, g
                        ));
                        o.push_str(&emit_block_kt(b, indent + 2));
                        o.push_str(&format!("{}    }}\n", ind));
                    }
                    _ => {
                        o.push_str(&format!(
                            "{}    {}{} -> {}\n",
                            ind,
                            p,
                            g,
                            emit_expr_kt(&arm.body, 0)
                        ));
                    }
                }
            }
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::For { label: _, pattern, iter, body } => {
            let mut o = format!(
                "for ({} in {}) {{\n",
                kt_pattern(pattern),
                emit_expr_kt(iter, 0)
            );
            o.push_str(&emit_block_kt(body, indent + 1));
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::While { label: _, cond, body } => {
            let mut o = format!("while ({}) {{\n", emit_expr_kt(cond, 0));
            o.push_str(&emit_block_kt(body, indent + 1));
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::WhileLet { label: _, pattern, expr: scrut, body } => {
            // while let pat = expr → while (expr matches pat)
            let mut o = format!(
                "while ({} is {}) {{\n",
                emit_expr_kt(scrut, 0),
                kt_pattern(pattern)
            );
            o.push_str(&emit_block_kt(body, indent + 1));
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::Loop { label, body } => {
            let mut o = String::new();
            if let Some(l) = label {
                o.push_str(&format!("{}@ ", kt_ident(&l.name)));
            }
            o.push_str("while (true) {\n");
            o.push_str(&emit_block_kt(body, indent + 1));
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::Closure { params, body, ret: _, .. } => {
            let ps: Vec<String> = params
                .iter()
                .filter_map(|p| match &p.kind {
                    ParamKind::Pattern(pat) => match &pat.kind {
                        PatternKind::Ident { name, .. } => Some(kt_ident(&name.name)),
                        _ => Some("x".into()),
                    },
                    _ => None,
                })
                .collect();
            if let ExprKind::Block(be) = &body.kind {
                if be.stmts.is_empty() {
                    if let Some(tail) = &be.tail {
                        return format!(
                            "{{ {} -> {} }}",
                            ps.join(", "),
                            emit_expr_kt(tail, 0)
                        );
                    }
                }
                let mut o = format!("{{ {} ->\n", ps.join(", "));
                o.push_str(&emit_block_kt(be, indent + 1));
                o.push_str(&format!("{}}}\n", ind));
                o
            } else {
                format!(
                    "{{ {} -> {} }}",
                    ps.join(", "),
                    emit_expr_kt(body, 0)
                )
            }
        }
        ExprKind::Return(value) => {
            if let Some(v) = value {
                format!("return {}", emit_expr_kt(v, 0))
            } else {
                "return".into()
            }
        }
        ExprKind::Break { label, value } => {
            let mut o = "break".to_string();
            if let Some(l) = label {
                o = format!("break@{}", kt_ident(&l.name));
            }
            if let Some(v) = value {
                o.push_str(&format!(" /* with value: {} */", emit_expr_kt(v, 0)));
            }
            o
        }
        ExprKind::Continue { label } => {
            if let Some(l) = label {
                format!("continue@{}", kt_ident(&l.name))
            } else {
                "continue".into()
            }
        }
        ExprKind::Array(elems) => {
            let es: Vec<String> = elems.iter().map(|e| emit_expr_kt(e, 0)).collect();
            if es.is_empty() {
                "emptyList<Any>()".into()
            } else {
                format!("listOf({})", es.join(", "))
            }
        }
        ExprKind::ArrayRepeat { elem, count } => {
            format!(
                "List({}) {{ repeat({{ {} }}) }}",
                emit_expr_kt(count, 0),
                emit_expr_kt(elem, 0)
            )
        }
        ExprKind::Struct { path, fields, spread } => {
            let name = kt_ident(&path.last().name);
            let fs: Vec<String> = fields
                .iter()
                .map(|f| {
                    let fn_ = match &f.name {
                        FieldIndex::Named(id) => kt_ident(&id.name),
                        FieldIndex::Index(i, _) => format!("component{}", i + 1),
                    };
                    let v = f
                        .value
                        .as_ref()
                        .map(|v| emit_expr_kt(v, 0))
                        .unwrap_or_else(|| fn_.clone());
                    format!("{} = {}", fn_, v)
                })
                .collect();
            let spread_str = if let Some(spread) = spread {
                format!(", /* ..{} */", emit_expr_kt(spread, 0))
            } else {
                String::new()
            };
            format!("{}({}{}{})", name, fs.join(", "), fs.is_empty().then(|| "" ).unwrap_or(""), spread_str)
        }
        ExprKind::Tuple(elems) => {
            let es: Vec<String> = elems.iter().map(|e| emit_expr_kt(e, 0)).collect();
            match es.len() {
                0 => "Unit".into(),
                2 => format!("Pair({}, {})", es[0], es[1]),
                3 => format!("Triple({}, {}, {})", es[0], es[1], es[2]),
                _ => format!("/* tuple of {} */", es.len()),
            }
        }
        ExprKind::Block(be) => {
            let mut o = "run {\n".to_string();
            o.push_str(&emit_block_kt(be, indent + 1));
            if let Some(tail) = &be.tail {
                o.push_str(&format!(
                    "{}{}\n",
                    ind,
                    emit_expr_kt(tail, 0)
                ));
            }
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::AsyncBlock { body, .. } => {
            let mut o = "// async ".to_string();
            o.push_str(&emit_block_kt(body, indent));
            o
        }
        ExprKind::Try(inner) => {
            // Kotlin: try expression
            let inner_str = emit_expr_kt(inner, 0);
            format!(
                "try {{ {} }} catch (e: Exception) {{ throw e }}",
                inner_str
            )
        }
        ExprKind::Await(inner) => emit_expr_kt(inner, 0),
        ExprKind::Cast { expr: inner, ty } => {
            format!("{} as {}", emit_expr_kt(inner, 0), kt_type(ty))
        }
        ExprKind::IfLet { pattern, expr: scrut, then, else_ } => {
            let mut o = format!(
                "if ({} is {}) {{\n",
                emit_expr_kt(scrut, 0),
                kt_pattern(pattern)
            );
            o.push_str(&emit_block_kt(then, indent + 1));
            if let Some(els) = else_ {
                o.push_str(&format!("{}}} else ", ind));
                o.push_str(&emit_expr_kt(els, indent));
            } else {
                o.push_str(&format!("{}}}\n", ind));
            }
            o
        }
        ExprKind::Macro { path, args: _ } => kt_macro(path.last().name.as_str()),
        ExprKind::Native(_) => "// native block".into(),
    }
}

fn kt_literal(lit: &Literal) -> String {
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
                // Add L suffix for Long literals > Int range
                if *value > i32::MAX as i128 || *value < i32::MIN as i128 {
                    format!("{}L", s)
                } else {
                    s
                }
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

fn kt_binop(op: &BinaryOp) -> &'static str {
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
        BinaryOp::BitAnd => "and",
        BinaryOp::BitOr => "or",
        BinaryOp::BitXor => "xor",
        BinaryOp::Shl => "shl",
        BinaryOp::Shr => "shr",
    }
}

fn kt_unop(op: &UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Not => "!",
        UnaryOp::Deref | UnaryOp::Ref | UnaryOp::RefMut => "",
    }
}

fn kt_pattern(pat: &Pattern) -> String {
    match &pat.kind {
        PatternKind::Wildcard => "_".into(),
        PatternKind::Ident { name, .. } => kt_ident(&name.name),
        PatternKind::Literal(lit) => kt_literal(lit),
        PatternKind::Path(p) => kt_capitalize(&kt_ident(&p.last().name)),
        PatternKind::TupleStruct { path, elems, .. } => {
            let n = kt_capitalize(&kt_ident(&path.last().name));
            let es: Vec<String> = elems.iter().map(kt_pattern).collect();
            if es.is_empty() {
                n
            } else {
                format!("{}({})", n, es.join(", "))
            }
        }
        PatternKind::Struct { path, fields, .. } => {
            let n = kt_capitalize(&kt_ident(&path.last().name));
            let fs: Vec<String> = fields
                .iter()
                .map(|f| {
                    let fn_ = kt_ident(&f.name.name);
                    let p = f
                        .pattern
                        .as_ref()
                        .map(|p| kt_pattern(p))
                        .unwrap_or_else(|| kt_ident(&f.name.name));
                    format!("{} = {}", fn_, p)
                })
                .collect();
            format!("{}({}{}{})", n, fs.join(", "), if fs.is_empty() { "" } else { ", " }, "/* .. */")
        }
        PatternKind::Tuple { elems, .. } => {
            let es: Vec<String> = elems.iter().map(kt_pattern).collect();
            format!("({})", es.join(", "))
        }
        PatternKind::Or(elems) => elems
            .iter()
            .map(|e| kt_pattern(e))
            .collect::<Vec<_>>()
            .join(", "),
        PatternKind::Range { lo, hi, inclusive } => {
            let l = kt_pattern(lo);
            let r = kt_pattern(hi);
            if *inclusive {
                format!("{}..{}", l, r)
            } else {
                format!("{} until {}", l, r)
            }
        }
        PatternKind::Rest => "/* .. */".into(),
    }
}

fn kt_macro(name: &str) -> String {
    match name {
        "println" => "println".into(),
        "eprintln" => "System.err.println".into(),
        "format" => "String.format".into(),
        "todo" | "unimplemented" => "TODO()".into(),
        "panic" => "throw RuntimeException()".into(),
        "vec" => "mutableListOf".into(),
        _ => format!("/* macro: {} */", name),
    }
}

// ────────────────────────────────────────────────────────────────
// 语句块转译
// ────────────────────────────────────────────────────────────────

pub fn emit_block_kt(be: &BlockExpr, indent: usize) -> String {
    let ind = "    ".repeat(indent);
    let mut o = String::new();
    for stmt in &be.stmts {
        match stmt {
            Stmt::Let(l) => o.push_str(&format!("{}{}", ind, emit_let_kt(l, 0))),
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
                        o.push_str(&format!("{}\n", emit_expr_kt(expr, indent)));
                    }
                    ExprKind::Return(_) => {
                        o.push_str(&format!(
                            "{}{}\n",
                            ind,
                            emit_expr_kt(expr, 0)
                        ));
                    }
                    _ => {
                        o.push_str(&format!(
                            "{}{}\n",
                            ind,
                            emit_expr_kt(expr, 0)
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
                // These are expression forms that can be the last expression in a block
                o.push_str(&format!("{}", emit_expr_kt(tail, indent)));

            }
            _ => {
                o.push_str(&format!(
                    "{}{}\n",
                    ind,
                    emit_expr_kt(tail, 0)
                ));
            }
        }
    }
    o
}

fn emit_let_kt(l: &LetStmt, _indent: usize) -> String {
    let pat = kt_pattern(&l.pattern);
    let kw = if l.mutable { "var" } else { "val" };
    if let Some(ty) = &l.ty {
        format!(
            "{} {}: {} = {}\n",
            kw,
            pat,
            kt_type(ty),
            l.init
                .as_ref()
                .map(|e| emit_expr_kt(e, 0))
                .unwrap_or_else(|| "TODO()".into())
        )
    } else {
        format!(
            "{} {} = {}\n",
            kw,
            pat,
            l.init
                .as_ref()
                .map(|e| emit_expr_kt(e, 0))
                .unwrap_or_else(|| "TODO()".into())
        )
    }
}

pub fn emit_stmt_kt(stmt: &Stmt, indent: usize) -> String {
    match stmt {
        Stmt::Let(l) => {
            let ind = "    ".repeat(indent);
            format!("{}{}", ind, emit_let_kt(l, 0))
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
                | ExprKind::IfLet { .. } => emit_expr_kt(expr, indent) + "\n",
                _ => {
                    let ind = "    ".repeat(indent);
                    format!("{}{}\n", ind, emit_expr_kt(expr, 0))
                }
            }
        }
    }
}