//! C# backend (Logic tier) -- type mapping + full function body translation
//! C# is a Tier 1 Harness core language. This backend generates C# 9+ code.
//! struct -> record (named) / class (tuple/unit);
//! enum -> sealed abstract record + derived record (data) / enum (unit);
//! trait -> interface (C# 8+ default implementations);
//! impl -> class implementing interface;
//! fn -> static method in class; const -> const/readonly;
//! graph -> static void Main()

use crate::ast::*;
use crate::codegen::{CodegenBackend, CodegenContext};

pub struct CSharpBackend;

impl CodegenBackend for CSharpBackend {
    fn lang(&self) -> &'static str {
        "csharp"
    }

    fn emit_item(&self, ctx: &CodegenContext, item: &Item) -> Result<String, String> {
        let mut out = String::new();
        out.push_str(&format!("// {}\n", crate::sourcemap::generated_header("csharp")));
        out.push_str("using System;\n");
        out.push_str("using System.Collections.Generic;\n");
        out.push_str("using System.Linq;\n");
        out.push_str("using System.Threading.Tasks;\n\n");

        match item {
            Item::Struct(s) => out.push_str(&cs_struct(s)),
            Item::Enum(e) => out.push_str(&cs_enum(e)),
            Item::Trait(t) => out.push_str(&cs_trait(t)),
            Item::Fn(f) => out.push_str(&cs_fn(f, None)),
            Item::Graph(g) => out.push_str(&cs_graph(g, ctx)),
            Item::Impl(imp) => out.push_str(&cs_impl(imp)),
            Item::Const(c) => out.push_str(&cs_const(c)),
            Item::TypeAlias(a) => out.push_str(&cs_typealias(a)),
            Item::MacroRules(m) => out.push_str(&cs_macro_rules(m)),
            _ => {
                return Err(format!("csharp backend does not support {}", crate::ast::item_kind_name(item)))
            }
        }
        Ok(out)
    }
}

const CS_KW: &[&str] = &[
    "abstract", "as", "base", "bool", "break", "byte", "case", "catch",
    "char", "checked", "class", "const", "continue", "decimal", "default",
    "delegate", "do", "double", "else", "enum", "event", "explicit",
    "extern", "false", "file", "finally", "fixed", "float", "for",
    "foreach", "goto", "if", "implicit", "in", "int", "interface",
    "internal", "is", "lock", "long", "namespace", "new", "null",
    "object", "operator", "out", "override", "params", "private",
    "protected", "public", "readonly", "ref", "record", "return", "sbyte",
    "sealed", "short", "sizeof", "stackalloc", "static", "string",
    "struct", "switch", "this", "throw", "true", "try", "typeof",
    "uint", "ulong", "unchecked", "unsafe", "ushort", "using",
    "var", "virtual", "void", "volatile", "while", "with", "yield",
    "nint", "nuint", "init", "and", "not", "or", "required",
    "global", "scoped", "_",
];

fn cs_ident(s: &str) -> String {
    if CS_KW.contains(&s) { format!("{}_", s) } else { s.to_string() }
}

fn cs_capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

fn cs_generic_arg(arg: &GenericArg) -> String {
    match arg {
        GenericArg::Type(t) => cs_type(t),
        GenericArg::Const(c) => match &c.kind {
            ConstArgKind::Literal(lit) => lit.raw.clone(),
            ConstArgKind::Block(_) => "0".into(),
        },
    }
}

fn cs_two_generic_args(args: &[GenericArg]) -> (String, String) {
    let mut it = args.iter();
    (it.next().map(cs_generic_arg).unwrap_or_else(|| "object".into()),
     it.next().map(cs_generic_arg).unwrap_or_else(|| "object".into()))
}

fn cs_type(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(pt) => {
            let name = pt.path.last().name.as_str();
            match name {
                "String" | "str" => "string".into(),
                "char" => "char".into(),
                "bool" => "bool".into(),
                "i8" => "sbyte".into(), "i16" => "short".into(),
                "i32" => "int".into(), "i64" => "long".into(),
                "u8" => "byte".into(), "u16" => "ushort".into(),
                "u32" => "uint".into(), "u64" => "ulong".into(),
                "usize" => "nuint".into(), "isize" => "nint".into(),
                "f32" => "float".into(), "f64" => "double".into(),
                "Vec" => format!("List<{}>", pt.generic_args.iter().next().map(cs_generic_arg).unwrap_or_else(|| "object".into())),
                "HashMap" => { let (k, v) = cs_two_generic_args(&pt.generic_args); format!("Dictionary<{}, {}>", k, v) }
                "HashSet" => format!("HashSet<{}>", pt.generic_args.iter().next().map(cs_generic_arg).unwrap_or_else(|| "object".into())),
                "Option" => format!("{}?", pt.generic_args.iter().next().map(cs_generic_arg).unwrap_or_else(|| "object".into())),
                "Result" => if !pt.generic_args.is_empty() { cs_generic_arg(&pt.generic_args[0]) } else { "object".into() },
                "Box" => if !pt.generic_args.is_empty() { cs_generic_arg(&pt.generic_args[0]) } else { "object".into() },
                _ => cs_ident(name),
            }
        }
        TypeKind::Ref { inner, .. } => cs_type(inner),
        TypeKind::Tuple(elems) => if elems.is_empty() { "void".into() } else { format!("({})", elems.iter().map(cs_type).collect::<Vec<_>>().join(", ")) },
        TypeKind::Array { elem, .. } => format!("{}[]", cs_type(elem)),
        TypeKind::Slice(inner) => format!("{}[]", cs_type(inner)),
        TypeKind::Paren(inner) => cs_type(inner),
        TypeKind::Never => "void".into(),
        TypeKind::FnPtr { params, ret } => {
            let r = ret.as_ref().map(|t| cs_type(t)).unwrap_or_else(|| "void".into());
            if params.is_empty() { format!("Func<{}>", r) } else { format!("Func<{}, {}>", params.iter().map(cs_type).collect::<Vec<_>>().join(", "), r) }
        }
        TypeKind::DynTrait(_) | TypeKind::ImplTrait(_) | TypeKind::Infer => "object".into(),
    }
}

fn cs_param(p: &Param) -> Option<String> {
    match &p.kind {
        ParamKind::Self_(_) => None,
        ParamKind::Pattern(pat) => {
            let name = match &pat.kind {
                PatternKind::Ident { name, .. } => cs_ident(&name.name),
                _ => "_arg".into(),
            };
            Some(format!("{} {}", cs_type(&p.ty), name))
        }
    }
}

fn cs_struct(s: &StructDef) -> String {
    let name = cs_ident(&s.name.name);
    match &s.kind {
        StructKind::Named(fields) => {
            let fs: Vec<String> = fields.iter().map(|f| {
                let fn_ = f.name.as_ref().map(|n| cs_capitalize(&cs_ident(&n.name))).unwrap_or_else(|| "_".into());
                format!("{} {fn_}", cs_type(&f.ty))
            }).collect();
            format!("public record {}({}) {{}}\n\n", name, fs.join(", "))
        }
        StructKind::Tuple(fields) => {
            let mut o = format!("public class {} {{\n", name);
            for (i, f) in fields.iter().enumerate() { o.push_str(&format!("    public {} Item{};\n", cs_type(&f.ty), i + 1)); }
            o.push_str("\n");
            let ps: Vec<String> = fields.iter().enumerate().map(|(i, f)| format!("{} item{}", cs_type(&f.ty), i + 1)).collect();
            o.push_str(&format!("    public {}({}) {{\n", name, ps.join(", ")));
            for i in 0..fields.len() { o.push_str(&format!("        Item{} = item{};\n", i + 1, i + 1)); }
            o.push_str("    }\n}\n\n"); o
        }
        StructKind::Unit => format!("public class {} {{}}\n\n", name),
    }
}

fn cs_enum(e: &EnumDef) -> String {
    let name = cs_ident(&e.name.name);
    let has_data = e.variants.iter().any(|v| !matches!(&v.fields, StructKind::Unit));
    if !has_data {
        let mut o = format!("public enum {} {{\n", name);
        for (i, v) in e.variants.iter().enumerate() {
            let c = if i < e.variants.len() - 1 { "," } else { "" };
            o.push_str(&format!("    {}{}\n", cs_ident(&v.name.name), c));
        }
        o.push_str("}\n\n"); o
    } else {
        let mut o = format!("public abstract record {} {{}}\n\n", name);
        for v in &e.variants {
            let vn = cs_ident(&v.name.name);
            match &v.fields {
                StructKind::Unit => { o.push_str(&format!("public record {}() : {} {{}}\n\n", vn, name)); }
                StructKind::Named(fields) => {
                    let fs: Vec<String> = fields.iter().map(|f| {
                        let fn_ = f.name.as_ref().map(|n| cs_capitalize(&cs_ident(&n.name))).unwrap_or_else(|| "_".into());
                        format!("{} {fn_}", cs_type(&f.ty))
                    }).collect();
                    o.push_str(&format!("public record {}({}) : {} {{}}\n\n", vn, fs.join(", "), name));
                }
                StructKind::Tuple(fields) => {
                    let fs: Vec<String> = fields.iter().map(|f| cs_type(&f.ty)).collect();
                    o.push_str(&format!("public record {}({}) : {} {{}}\n\n", vn, fs.join(", "), name));
                }
            }
        }
        o
    }
}

fn cs_trait(t: &TraitDef) -> String {
    let name = cs_ident(&t.name.name);
    let mut o = format!("public interface {} {{\n", name);
    for ti in &t.items {
        match ti {
            TraitItem::FnSig(sig) => {
                let ps: Vec<String> = sig.params.iter().filter_map(cs_param).collect();
                let r = sig.ret.as_ref().map(|t| format!(" {}", cs_type(t))).unwrap_or_else(|| " void".into());
                o.push_str(&format!("    {} {}({});\n", r, cs_ident(&sig.name.name), ps.join(", ")));
            }
            TraitItem::Fn(f) => {
                let ps: Vec<String> = f.params.iter().filter_map(cs_param).collect();
                let r = f.ret.as_ref().map(|t| format!(" {}", cs_type(t))).unwrap_or_else(|| " void".into());
                o.push_str(&format!("    {} {}({}) {{\n", r, cs_ident(&f.name.name), ps.join(", ")));
                if let Some(body) = &f.body { o.push_str(&emit_block_cs(body, 2)); }
                o.push_str("    }\n");
            }
            TraitItem::Const(_) | TraitItem::TypeAlias(_) => {}
        }
    }
    o.push_str("}\n\n"); o
}

fn cs_fn(f: &FnDef, class_name: Option<&str>) -> String {
    let name = cs_ident(&f.name.name);
    let ret = f.ret.as_ref().map(|t| cs_type(t)).unwrap_or_else(|| "void".into());
    let ps: Vec<String> = f.params.iter().filter_map(cs_param).collect();
    let mut o = String::new();
    if let Some(cls) = class_name { o.push_str(&format!("public class {} {{\n", cls)); }
    o.push_str(&format!("    public static {} {}({}) {{\n", ret, name, ps.join(", ")));
    if let Some(body) = &f.body { o.push_str(&emit_block_cs(body, 2)); }
    o.push_str("    }\n");
    if class_name.is_some() { o.push_str("}\n"); }
    o.push_str("\n"); o
}

fn cs_graph(g: &GraphDef, ctx: &CodegenContext) -> String {
    let gn = cs_ident(&g.name.name);
    let mut o = format!("// graph {} -- scale: {:?}\n", g.name.name, ctx.scale);
    o.push_str(&format!("public class {} {{\n", gn));
    o.push_str("    public static void Main(string[] args) {\n");
    for gs in &g.body {
        match gs {
            GraphStmt::Node(n) => o.push_str(&format!("        // node {}: {}\n", cs_ident(&n.name.name), cs_type(&n.ty))),
            GraphStmt::Edge(e) => { let ep: Vec<String> = e.endpoints.iter().map(|p| p.last().name.clone()).collect(); o.push_str(&format!("        // edge: {}\n", ep.join(" -> "))); }
            GraphStmt::Let(l) => o.push_str(&format!("        {}", emit_let_cs(l))),
            GraphStmt::Stmt(s) => o.push_str(&emit_stmt_cs(s, "        ")),
            GraphStmt::Item(_) => {}
        }
    }
    o.push_str("    }\n}\n\n"); o
}

fn cs_impl(imp: &ImplDef) -> String {
    let tn = imp.trait_ty.as_ref().map(|t| cs_type(t)).unwrap_or_default();
    let sn = cs_type(&imp.self_ty);
    let mut o = if !tn.is_empty() { format!("public class {}Impl : {} {{\n", sn, tn) } else { format!("public class {} {{\n", sn) };
    for item in &imp.items {
        match item {
            ImplItem::Fn(f) => {
                let fn_ = cs_ident(&f.name.name);
                let r = f.ret.as_ref().map(|t| format!("{} ", cs_type(t))).unwrap_or_default();
                let ps: Vec<String> = f.params.iter().filter_map(cs_param).collect();
                o.push_str(&format!("    public static {}{}({}) {{\n", r, fn_, ps.join(", ")));
                if let Some(body) = &f.body { o.push_str(&emit_block_cs(body, 2)); }
                o.push_str("    }\n");
            }
            ImplItem::Const(c) => { o.push_str(&format!("    public const {} = {}\n", cs_ident(&c.name.name), emit_expr_cs(&c.value, 0))); }
            ImplItem::TypeAlias(_) => {}
        }
    }
    o.push_str("}\n\n"); o
}

fn cs_const(c: &ConstDef) -> String {
    format!("public const {} {} = {};\n\n", cs_type(&c.ty), cs_ident(&c.name.name), emit_expr_cs(&c.value, 0))
}

fn cs_typealias(a: &TypeAliasDef) -> String {
    format!("// Type alias: {} = {}\n", cs_ident(&a.name.name), cs_type(&a.ty))
}

fn cs_macro_rules(m: &MacroRulesDefinition) -> String {
    format!("// macro_rules {} -- not directly translatable to C#\n\n", cs_ident(&m.name.name))
}

// Expression emitter

pub fn emit_expr_cs(expr: &Expr, indent: usize) -> String {
    let ind = " ".repeat(indent);
    match &expr.kind {
        ExprKind::Literal(lit) => cs_literal(lit),
        ExprKind::Path(p) => cs_ident(&p.last().name),
        ExprKind::Binary { op, lhs, rhs } => format!("{} {} {}", emit_expr_cs(lhs, 0), cs_binop(op), emit_expr_cs(rhs, 0)),
        ExprKind::Unary { op, operand } => format!("{}{}", cs_unop(op), emit_expr_cs(operand, 0)),
        ExprKind::Call { callee, args } => {
            let as_ = args.iter().map(|a| emit_expr_cs(a, 0)).collect::<Vec<_>>();
            format!("{}({})", emit_expr_cs(callee, 0), as_.join(", "))
        }
        ExprKind::MethodCall { receiver, method, generic_args: _, args } => {
            let mn = cs_ident(&method.name);
            let as_ = args.iter().map(|a| emit_expr_cs(a, 0)).collect::<Vec<_>>();
            format!("{}.{}({})", emit_expr_cs(receiver, 0), mn, as_.join(", "))
        }
        ExprKind::Field { base, field } => {
            let fn_ = match field { FieldIndex::Named(id) => cs_capitalize(&cs_ident(&id.name)), FieldIndex::Index(i, _) => format!("Item{}", i + 1) };
            format!("{}.{}", emit_expr_cs(base, 0), fn_)
        }
        ExprKind::Index { base, index } => format!("{}[{}]", emit_expr_cs(base, 0), emit_expr_cs(index, 0)),
        ExprKind::Slice { base, range } => {
            let bs = emit_expr_cs(base, 0);
            let s = range.lo.as_ref().map(|e| emit_expr_cs(e, 0)).unwrap_or_else(|| "0".into());
            let e = range.hi.as_ref().map(|e| emit_expr_cs(e, 0)).unwrap_or_else(|| format!("{}.Length", bs));
            format!("{}.Skip({}).Take({} - {}).ToArray()", bs, s, e, s)
        }
        ExprKind::Range(r) => {
            let l = r.lo.as_ref().map(|e| emit_expr_cs(e, 0)).unwrap_or_else(|| "0".into());
            let hi = r.hi.as_ref().map(|e| emit_expr_cs(e, 0)).unwrap_or_else(|| "???".into());
            if r.inclusive { format!("new Range({}, {} + 1)", l, hi) } else { format!("{}..{}", l, hi) }
        }
        ExprKind::Assign { lhs, rhs } => format!("{} = {}", emit_expr_cs(lhs, 0), emit_expr_cs(rhs, 0)),
        ExprKind::CompoundAssign { op, lhs, rhs } => format!("{} {}= {}", emit_expr_cs(lhs, 0), cs_binop(op).trim(), emit_expr_cs(rhs, 0)),
        ExprKind::If { cond, then, else_ } => {
            let mut o = format!("({}) ? {}", emit_expr_cs(cond, 0), emit_block_tail(&then, indent));
            if let Some(els) = else_ { o.push_str(&format!(" : {}", emit_expr_cs(els, 0))); }
            o
        }
        ExprKind::Match { scrutinee, arms } => {
            let mut o = format!("{} switch {{", emit_expr_cs(scrutinee, 0));
            for arm in arms {
                let p = cs_pattern(&arm.pattern);
                let g = if let Some(g) = &arm.guard { format!(" when ({})", emit_expr_cs(g, 0)) } else { String::new() };
                o.push_str(&format!("\n{}{}{} => {},", ind, p, g, emit_expr_cs(&arm.body, 0)));
            }
            o.push_str(&format!("\n{}}}", ind)); o
        }
        ExprKind::For { label: _, pattern, iter, body } => {
            let mut o = format!("foreach ({} in {}) {{\n", cs_pattern(pattern), emit_expr_cs(iter, 0));
            o.push_str(&emit_block_cs(body, indent + 2));
            o.push_str(&format!("{}}}", ind)); o
        }
        ExprKind::While { label: _, cond, body } => {
            let mut o = format!("while ({}) {{\n", emit_expr_cs(cond, 0));
            o.push_str(&emit_block_cs(body, indent + 2));
            o.push_str(&format!("{}}}", ind)); o
        }
        ExprKind::WhileLet { label: _, pattern, expr: scrut, body } => {
            let mut o = format!("while ({} is {}) {{\n", emit_expr_cs(scrut, 0), cs_pattern(pattern));
            o.push_str(&emit_block_cs(body, indent + 2));
            o.push_str(&format!("{}}}", ind)); o
        }
        ExprKind::Loop { label, body } => {
            let mut o = String::new();
            if let Some(l) = label { o.push_str(&format!("{}: ", cs_ident(&l.name))); }
            o.push_str("while (true) {\n");
            o.push_str(&emit_block_cs(body, indent + 2));
            o.push_str(&format!("{}}}", ind)); o
        }
        ExprKind::Closure { params, body, ret: _, .. } => {
            let ps: Vec<String> = params.iter().filter_map(|p| match &p.kind {
                ParamKind::Pattern(pat) => match &pat.kind { PatternKind::Ident { name, .. } => Some(cs_ident(&name.name)), _ => Some("x".into()) },
                _ => None,
            }).collect();
            // body is Box<Expr>, usually a Block
            if let ExprKind::Block(be) = &body.kind {
                if be.stmts.is_empty() {
                    if let Some(tail) = &be.tail { return format!("({}) => {}", ps.join(", "), emit_expr_cs(tail, 0)); }
                }
                let mut o = format!("({}) => {{\n", ps.join(", "));
                o.push_str(&emit_block_cs(be, indent + 2));
                o.push_str(&format!("{}}}", ind)); o
            } else {
                format!("({}) => {}", ps.join(", "), emit_expr_cs(body, 0))
            }
        }
        ExprKind::Return(value) => if let Some(v) = value { format!("return {};", emit_expr_cs(v, 0)) } else { "return;".into() },
        ExprKind::Break { label, value } => {
            let mut o = "break".to_string();
            if let Some(l) = label { o = format!("break {}", cs_ident(&l.name)); }
            if let Some(v) = value { o.push_str(&format!(" {}", emit_expr_cs(v, 0))); }
            o
        }
        ExprKind::Continue { label } => {
            let mut o = "continue".to_string();
            if let Some(l) = label { o = format!("continue {}", cs_ident(&l.name)); }
            o
        }
        ExprKind::Array(elems) => {
            let es: Vec<String> = elems.iter().map(|e| emit_expr_cs(e, 0)).collect();
            if es.is_empty() { "Array.Empty<object>()".into() } else { format!("new object[] {{ {} }}", es.join(", ")) }
        }
        ExprKind::ArrayRepeat { elem, count } => format!("Enumerable.Repeat({}, {}).ToArray()", emit_expr_cs(elem, 0), emit_expr_cs(count, 0)),
        ExprKind::Struct { path, fields, spread } => {
            let name = cs_ident(&path.last().name);
            if spread.is_some() {
                let base = spread.as_ref().map(|s| emit_expr_cs(s, 0)).unwrap_or_else(|| format!("new {}()", name));
                let fs: Vec<String> = fields.iter().map(|f| {
                    let fn_ = match &f.name { FieldIndex::Named(id) => cs_capitalize(&cs_ident(&id.name)), FieldIndex::Index(i, _) => format!("Item{}", i + 1) };
                    let v = f.value.as_ref().map(|v| emit_expr_cs(v, 0)).unwrap_or_else(|| fn_.clone());
                    format!("{} = {}", fn_, v)
                }).collect();
                format!("{} with {{ {} }}", base, fs.join(", "))
            } else {
                let fs: Vec<String> = fields.iter().map(|f| {
                    let fn_ = match &f.name { FieldIndex::Named(id) => cs_capitalize(&cs_ident(&id.name)), FieldIndex::Index(i, _) => format!("Item{}", i + 1) };
                    let v = f.value.as_ref().map(|v| emit_expr_cs(v, 0)).unwrap_or_else(|| fn_.clone());
                    format!("{} = {}", fn_, v)
                }).collect();
                format!("new {} {{ {} }}", name, fs.join(", "))
            }
        }
        ExprKind::Tuple(elems) => { let es: Vec<String> = elems.iter().map(|e| emit_expr_cs(e, 0)).collect(); format!("({})", es.join(", ")) }
        ExprKind::Block(be) => { let mut o = "{\n".to_string(); o.push_str(&emit_block_cs(be, indent + 2)); if let Some(tail) = &be.tail { o.push_str(&format!("{}{}\n", ind, emit_expr_cs(tail, 0))); } o.push_str(&format!("{}}}", ind)); o }
        ExprKind::AsyncBlock { body, .. } => { let mut o = "async ".to_string(); o.push_str(&emit_block_cs(body, indent)); o }
        ExprKind::Try(inner) => { let mut o = "try {\n".to_string(); o.push_str(&emit_expr_cs(inner, indent + 2)); o.push_str("} catch (Exception _ex) { /* handle */ }"); o }
        ExprKind::Await(inner) => format!("await {}", emit_expr_cs(inner, 0)),
        ExprKind::Cast { expr: inner, ty } => format!("({}){}", cs_type(ty), emit_expr_cs(inner, 0)),
        ExprKind::IfLet { pattern, expr: scrut, then, else_ } => {
            let mut o = format!("if ({} is {}) {{\n", emit_expr_cs(scrut, 0), cs_pattern(pattern));
            o.push_str(&emit_block_cs(then, indent + 2));
            if let Some(els) = else_ { o.push_str("} else "); o.push_str(&emit_expr_cs(els, indent)); } else { o.push_str(&format!("{}}}", ind)); }
            o
        }
        ExprKind::Macro { path, args: _ } => cs_macro(path.last().name.as_str()),
        ExprKind::Native(_) => "// native block".into(),
    }
}

fn emit_block_tail(be: &BlockExpr, indent: usize) -> String {
    let mut o = emit_block_cs(be, indent + 2);
    if let Some(tail) = &be.tail { o.push_str(&format!("{}", emit_expr_cs(tail, 0))); }
    o
}

fn cs_literal(lit: &Literal) -> String {
    match &lit.kind {
        LiteralKind::Str { value, .. } => format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")),
        LiteralKind::Bool(b) => if *b { "true".into() } else { "false".into() },
        LiteralKind::Int { value, .. } => value.to_string(),
        LiteralKind::Float { value, .. } => { let s = value.to_string(); if !s.contains('.') { format!("{}f", s) } else { format!("{}f", s.trim_end_matches('0')) } },
        LiteralKind::Char(c) => format!("'{}'", c),
    }
}

fn cs_binop(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+", BinaryOp::Sub => "-", BinaryOp::Mul => "*", BinaryOp::Div => "/", BinaryOp::Rem => "%",
        BinaryOp::Eq => "==", BinaryOp::Ne => "!=", BinaryOp::Lt => "<", BinaryOp::Gt => ">", BinaryOp::Le => "<=", BinaryOp::Ge => ">=",
        BinaryOp::And => "&&", BinaryOp::Or => "||",
        BinaryOp::BitAnd => "&", BinaryOp::BitOr => "|", BinaryOp::BitXor => "^", BinaryOp::Shl => "<<", BinaryOp::Shr => ">>",
    }
}

fn cs_unop(op: &UnaryOp) -> &'static str {
    match op { UnaryOp::Neg => "-", UnaryOp::Not => "!", UnaryOp::Deref | UnaryOp::Ref | UnaryOp::RefMut => "" }
}

fn cs_pattern(pat: &Pattern) -> String {
    match &pat.kind {
        PatternKind::Wildcard => "_".into(),
        PatternKind::Ident { name, .. } => cs_ident(&name.name),
        PatternKind::Literal(lit) => cs_literal(lit),
        PatternKind::Path(p) => cs_ident(&p.last().name),
        PatternKind::TupleStruct { path, elems, .. } => {
            let n = cs_ident(&path.last().name);
            let es: Vec<String> = elems.iter().map(cs_pattern).collect();
            if es.is_empty() { n } else { format!("{}({})", n, es.join(", ")) }
        }
        PatternKind::Struct { path, fields, .. } => {
            let n = cs_ident(&path.last().name);
            let fs: Vec<String> = fields.iter().map(|f| {
                let fn_ = cs_capitalize(&cs_ident(&f.name.name));
                let p = f.pattern.as_ref().map(|p| cs_pattern(p)).unwrap_or_else(|| cs_ident(&f.name.name));
                format!("{} = {}", fn_, p)
            }).collect();
            format!("{} {{ {} }}", n, fs.join(", "))
        }
        PatternKind::Tuple { elems, .. } => { let es: Vec<String> = elems.iter().map(|e| cs_pattern(e)).collect(); format!("({})", es.join(", ")) }
        PatternKind::Or(elems) => elems.iter().map(|e| cs_pattern(e)).collect::<Vec<_>>().join(" | "),
        PatternKind::Range { lo, hi, inclusive } => {
            let l = cs_pattern(lo);
            let r = cs_pattern(hi);
            if *inclusive { format!(">= {} && <= {}", l, r) } else { format!(">= {} && < {}", l, r) }
        }
        PatternKind::Rest => "..".into(),
    }
}

fn cs_macro(name: &str) -> String {
    match name {
        "println" => "Console.WriteLine".into(),
        "eprintln" => "Console.Error.WriteLine".into(),
        "format" => "string.Format".into(),
        "todo" | "unimplemented" => "throw new NotImplementedException()".into(),
        "panic" => "throw new Exception()".into(),
        _ => format!("/* macro: {} */", name),
    }
}

pub fn emit_block_cs(be: &BlockExpr, indent: usize) -> String {
    let ind = " ".repeat(indent);
    let mut o = String::new();
    for stmt in &be.stmts {
        match stmt {
            Stmt::Let(l) => o.push_str(&format!("{}{}\n", ind, emit_let_cs(l))),
            Stmt::Item(_) | Stmt::Empty(_) => {}
            Stmt::Expr { expr, has_semi: _ } => {
                match &expr.kind {
                    ExprKind::If { .. } | ExprKind::Match { .. } | ExprKind::For { .. }
                    | ExprKind::While { .. } | ExprKind::WhileLet { .. } | ExprKind::Loop { .. }
                    | ExprKind::Block(_) | ExprKind::Try(_) | ExprKind::AsyncBlock { .. } => {
                        o.push_str(&format!("{}{}\n", ind, emit_expr_cs(expr, indent)));
                    }
                    _ => { o.push_str(&format!("{}{};\n", ind, emit_expr_cs(expr, 0))); }
                }
            }
        }
    }
    o
}

fn emit_let_cs(l: &LetStmt) -> String {
    let pat = cs_pattern(&l.pattern);
    if let Some(ty) = &l.ty {
        format!("{}{}: {} = {};", if l.mutable { "" } else { "readonly " }, pat, cs_type(ty), l.init.as_ref().map(|e| emit_expr_cs(e, 0)).unwrap_or_else(|| "default".into()))
    } else {
        format!("var {} = {};", pat, l.init.as_ref().map(|e| emit_expr_cs(e, 0)).unwrap_or_else(|| "default".into()))
    }
}

pub fn emit_stmt_cs(stmt: &Stmt, ind: &str) -> String {
    match stmt {
        Stmt::Let(l) => format!("{}{}\n", ind, emit_let_cs(l)),
        Stmt::Item(_) | Stmt::Empty(_) => String::new(),
        Stmt::Expr { expr, has_semi: _ } => {
            match &expr.kind {
                ExprKind::If { .. } | ExprKind::Match { .. } | ExprKind::For { .. }
                | ExprKind::While { .. } | ExprKind::WhileLet { .. } | ExprKind::Loop { .. }
                | ExprKind::Block(_) | ExprKind::Try(_) | ExprKind::AsyncBlock { .. } => {
                    format!("{}{}\n", ind, emit_expr_cs(expr, ind.len()))
                }
                _ => format!("{}{};\n", ind, emit_expr_cs(expr, 0)),
            }
        }
    }
}