//! Java backend (Logic tier) — type mapping + full function body translation
//! Java is a Tier 1 Harness core language. This backend generates Java 17+ code.
//! struct → record (named) / class (tuple/unit);
//! enum → sealed interface + record (data) / enum (unit);
//! trait → interface; impl → class implementing interface;
//! fn → static method in class; const → static final;
//! graph → public static void main(String[] args)
//! Expressions: binary/unary/call/method/field/index/slice/range/assign/
//!   compound_assign/if/match(switch)/for/while/while-let/loop/if-let/closure/
//!   return/break/continue/array/struct/tuple/block/try/await/cast/native/macro

use crate::ast::*;
use crate::codegen::{CodegenBackend, CodegenContext};

pub struct JavaBackend;

impl CodegenBackend for JavaBackend {
    fn lang(&self) -> &'static str {
        "java"
    }

    fn emit_item(&self, ctx: &CodegenContext, item: &Item) -> Result<String, String> {
        let mut out = String::new();
        out.push_str(&format!(
            "// {}\n",
            crate::sourcemap::generated_header("java")
        ));
        out.push_str("import java.util.*;\n");
        out.push_str("import java.util.stream.*;\n");
        out.push_str("import java.util.function.*;\n");
        out.push_str("import java.util.Optional;\n\n");

        match item {
            Item::Struct(s) => out.push_str(&java_struct(s)),
            Item::Enum(e) => out.push_str(&java_enum(e)),
            Item::Trait(t) => out.push_str(&java_trait(t)),
            Item::Fn(f) => out.push_str(&java_fn(f, None)),
            Item::Graph(g) => out.push_str(&java_graph(g, ctx)),
            Item::Impl(imp) => out.push_str(&java_impl(imp)),
            Item::Const(c) => out.push_str(&java_const(c)),
            Item::TypeAlias(a) => out.push_str(&java_typealias(a)),
            Item::MacroRules(m) => out.push_str(&java_macro_rules(m)),
            _ => {
                return Err(format!(
                    "java 后端暂不支持 {}",
                    crate::ast::item_kind_name(item)
                ))
            }
        }
        Ok(out)
    }
}

// ────────────────────────────────────────────────────────────────
// Java keyword avoidance (70+)
// ────────────────────────────────────────────────────────────────

const JAVA_KW: &[&str] = &[
    "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char",
    "class", "const", "continue", "default", "do", "double", "else", "enum",
    "extends", "final", "finally", "float", "for", "goto", "if", "implements",
    "import", "instanceof", "int", "interface", "long", "native", "new", "package",
    "private", "protected", "public", "return", "short", "static", "strictfp",
    "super", "switch", "synchronized", "this", "throw", "throws", "transient",
    "try", "void", "volatile", "while", "true", "false", "null", "record",
    "sealed", "permits", "non-sealed", "var", "yield", "module", "open",
    "opens", "provides", "requires", "to", "transitive", "uses", "with",
    "_",
];

fn java_ident(s: &str) -> String {
    if JAVA_KW.contains(&s) {
        format!("{}__", s)
    } else {
        s.to_string()
    }
}

// ────────────────────────────────────────────────────────────────
// Generic arg helper
// ────────────────────────────────────────────────────────────────

fn java_generic_arg(arg: &GenericArg) -> String {
    match arg {
        GenericArg::Type(t) => java_type(t),
        GenericArg::Const(c) => match &c.kind {
            ConstArgKind::Literal(lit) => lit.raw.clone(),
            ConstArgKind::Block(_) => "0".into(),
        },
    }
}

fn java_two_generic_args(args: &[GenericArg]) -> (String, String) {
    let mut it = args.iter();
    let first = it.next().map(java_generic_arg).unwrap_or_else(|| "Object".into());
    let second = it.next().map(java_generic_arg).unwrap_or_else(|| "Object".into());
    (first, second)
}

// ────────────────────────────────────────────────────────────────
// Type mapping (matches registry.ts TypeMap for Java)
// ────────────────────────────────────────────────────────────────

fn java_type(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(pt) => {
            let name = pt.path.last().name.as_str();
            match name {
                "String" | "str" => "String".into(),
                "char" => "char".into(),
                "bool" => "boolean".into(),
                "i8" => "byte".into(),
                "i16" => "short".into(),
                "i32" => "int".into(),
                "i64" => "long".into(),
                "u8" => "int".into(),
                "u16" => "int".into(),
                "u32" => "long".into(),
                "u64" => "long".into(),
                "usize" | "isize" => "long".into(),
                "f32" => "float".into(),
                "f64" => "double".into(),
                "Vec" => {
                    let elem = pt.generic_args.iter().next().map(java_generic_arg).unwrap_or_else(|| "Object".into());
                    format!("List<{}>", elem)
                }
                "HashMap" => {
                    let (k, v) = java_two_generic_args(&pt.generic_args);
                    format!("Map<{}, {}>", k, v)
                }
                "HashSet" => {
                    let elem = pt.generic_args.iter().next().map(java_generic_arg).unwrap_or_else(|| "Object".into());
                    format!("Set<{}>", elem)
                }
                "Option" => {
                    let elem = pt.generic_args.iter().next().map(java_generic_arg).unwrap_or_else(|| "Object".into());
                    format!("Optional<{}>", elem)
                }
                "Result" => {
                    // Java has no Result; use Ok type
                    if !pt.generic_args.is_empty() {
                        java_generic_arg(&pt.generic_args[0])
                    } else {
                        "Object".into()
                    }
                }
                "Box" => {
                    if !pt.generic_args.is_empty() {
                        java_generic_arg(&pt.generic_args[0])
                    } else {
                        "Object".into()
                    }
                }
                _ => java_ident(name),
            }
        }
        TypeKind::Ref { inner, .. } => java_type(inner),
        TypeKind::Tuple(elems) => {
            if elems.is_empty() {
                "Void".into()
            } else {
                // Java has no tuples; use a generic record placeholder
                let ts: Vec<String> = elems.iter().map(java_type).collect();
                format!("Tuple{}<{}>", elems.len(), ts.join(", "))
            }
        }
        TypeKind::Array { elem, .. } => format!("{}[]", java_type(elem)),
        TypeKind::Slice(inner) => format!("{}[]", java_type(inner)),
        TypeKind::Paren(inner) => java_type(inner),
        TypeKind::Never => "void".into(),
        TypeKind::FnPtr { params, ret } => {
            let r = ret.as_ref().map(|t| java_type(t)).unwrap_or_else(|| "void".into());
            if params.is_empty() {
                format!("Supplier<{}>", r)
            } else if params.len() == 1 {
                format!("Function<{}, {}>", java_type(&params[0]), r)
            } else {
                // BiFunction for 2+ params
                format!("BiFunction<{}, {}, {}>", java_type(&params[0]), java_type(&params[1]), r)
            }
        }
        TypeKind::DynTrait(_) | TypeKind::ImplTrait(_) | TypeKind::Infer => "Object".into(),
    }
}

// ────────────────────────────────────────────────────────────────
// Param helper
// ────────────────────────────────────────────────────────────────

fn java_param(p: &Param) -> Option<String> {
    match &p.kind {
        ParamKind::Self_(_) => None,
        ParamKind::Pattern(pat) => {
            let name = match &pat.kind {
                PatternKind::Ident { name, .. } => java_ident(&name.name),
                _ => "_arg".into(),
            };
            Some(format!("{} {}", java_type(&p.ty), name))
        }
    }
}

// ────────────────────────────────────────────────────────────────
// Item-level emitters
// ────────────────────────────────────────────────────────────────

/// struct → public record (named) / public class (tuple/unit)
fn java_struct(s: &StructDef) -> String {
    let name = java_ident(&s.name.name);
    match &s.kind {
        StructKind::Named(fields) => {
            let field_strs: Vec<String> = fields
                .iter()
                .map(|f| {
                    let fname = f.name.as_ref().map(|n| java_ident(&n.name)).unwrap_or_else(|| "_".into());
                    format!("{} {}", java_type(&f.ty), fname)
                })
                .collect();
            format!("public record {}({}) {{}}\n\n", name, field_strs.join(", "))
        }
        StructKind::Tuple(fields) => {
            let mut out = format!("public class {} {{\n", name);
            for (i, f) in fields.iter().enumerate() {
                out.push_str(&format!("    public {} _{};\n", java_type(&f.ty), i));
            }
            out.push_str("\n");
            let param_strs: Vec<String> = fields
                .iter()
                .enumerate()
                .map(|(i, f)| format!("{} _{}", java_type(&f.ty), i))
                .collect();
            out.push_str(&format!("    public {}({}) {{\n", name, param_strs.join(", ")));
            for i in 0..fields.len() {
                out.push_str(&format!("        this._{} = _{};\n", i, i));
            }
            out.push_str("    }\n");
            out.push_str("}\n\n");
            out
        }
        StructKind::Unit => {
            format!("public class {} {{}}\n\n", name)
        }
    }
}

/// enum → sealed interface + record variants (data) / enum (unit)
fn java_enum(e: &EnumDef) -> String {
    let name = java_ident(&e.name.name);
    let has_data = e.variants.iter().any(|v| !matches!(&v.fields, StructKind::Unit));

    if !has_data {
        let mut out = format!("public enum {} {{\n", name);
        for (i, v) in e.variants.iter().enumerate() {
            let comma = if i < e.variants.len() - 1 { "," } else { ";" };
            out.push_str(&format!("    {}{}\n", java_ident(&v.name.name), comma));
        }
        out.push_str("}\n\n");
        out
    } else {
        let variant_names: Vec<String> = e.variants.iter().map(|v| java_ident(&v.name.name)).collect();
        let mut out = format!(
            "public sealed interface {} permits {} {{\n}}\n\n",
            name,
            variant_names.join(", ")
        );

        for v in &e.variants {
            let vname = java_ident(&v.name.name);
            match &v.fields {
                StructKind::Unit => {
                    out.push_str(&format!("public record {}() implements {} {{}}\n\n", vname, name));
                }
                StructKind::Named(fields) => {
                    let field_strs: Vec<String> = fields
                        .iter()
                        .map(|f| {
                            let fname = f.name.as_ref().map(|n| java_ident(&n.name)).unwrap_or_else(|| "_".into());
                            format!("{} {}", java_type(&f.ty), fname)
                        })
                        .collect();
                    out.push_str(&format!(
                        "public record {}({}) implements {} {{}}\n\n",
                        vname, field_strs.join(", "), name
                    ));
                }
                StructKind::Tuple(fields) => {
                    out.push_str(&format!("public final class {} implements {} {{\n", vname, name));
                    for (i, f) in fields.iter().enumerate() {
                        out.push_str(&format!("    public final {} _{};\n", java_type(&f.ty), i));
                    }
                    out.push_str("\n");
                    let param_strs: Vec<String> = fields
                        .iter()
                        .enumerate()
                        .map(|(i, f)| format!("{} _{}", java_type(&f.ty), i))
                        .collect();
                    out.push_str(&format!("    public {}({}) {{\n", vname, param_strs.join(", ")));
                    for i in 0..fields.len() {
                        out.push_str(&format!("        this._{} = _{};\n", i, i));
                    }
                    out.push_str("    }\n");
                    out.push_str("}\n\n");
                }
            }
        }
        out
    }
}

/// trait → Java interface
fn java_trait(t: &TraitDef) -> String {
    let name = java_ident(&t.name.name);
    let mut out = format!("public interface {} {{\n", name);
    for ti in &t.items {
        if let TraitItem::FnSig(sig) = ti {
            let params: Vec<String> = sig.params.iter().filter_map(java_param).collect();
            let ret = sig.ret.as_ref().map(|t| format!(" {}", java_type(t))).unwrap_or_else(|| " void".into());
            out.push_str(&format!(
                "    {} {}({});\n",
                ret, java_ident(&sig.name.name), params.join(", ")
            ));
        } else if let TraitItem::Fn(f) = ti {
            // Default method
            let params: Vec<String> = f.params.iter().filter_map(java_param).collect();
            let ret = f.ret.as_ref().map(|t| format!(" {}", java_type(t))).unwrap_or_else(|| " void".into());
            out.push_str(&format!(
                "    default {} {}({}) {{\n",
                ret, java_ident(&f.name.name), params.join(", ")
            ));
            if let Some(body) = &f.body {
                out.push_str(&emit_block_java(body, 2));
            }
            out.push_str("    }\n");
        }
    }
    out.push_str("}\n\n");
    out
}

/// fn → static method (wrapped in a class if top-level)
fn java_fn(f: &FnDef, class_name: Option<&str>) -> String {
    let name = java_ident(&f.name.name);
    let ret = f.ret.as_ref().map(|t| java_type(t)).unwrap_or_else(|| "void".into());
    let params: Vec<String> = f.params.iter().filter_map(java_param).collect();

    let mut out = String::new();
    if let Some(cls) = class_name {
        out.push_str(&format!("public class {} {{\n", cls));
    }
    out.push_str(&format!(
        "    public static {} {}({}) {{\n",
        ret, name, params.join(", ")
    ));
    if let Some(body) = &f.body {
        out.push_str(&emit_block_java(body, 2));
    }
    out.push_str("    }\n");
    if class_name.is_some() {
        out.push_str("}\n");
    }
    out.push_str("\n");
    out
}

/// graph → public static void main(String[] args)
fn java_graph(g: &GraphDef, ctx: &CodegenContext) -> String {
    let gname = java_ident(&g.name.name);
    let mut out = format!("// graph {} — scale: {:?}\n", g.name.name, ctx.scale);
    out.push_str(&format!("public class {} {{\n", gname));
    out.push_str("    public static void main(String[] args) {\n");
    // GraphDef body is Vec<GraphStmt>, not Option<Block>
    if !g.body.is_empty() {
        out.push_str("        // TODO: graph body GraphStmt translation\n");
        for gs in &g.body {
            match gs {
                GraphStmt::Node(n) => {
                    out.push_str(&format!(
                        "        // node {}: {}\n",
                        java_ident(&n.name.name),
                        java_type(&n.ty)
                    ));
                }
                GraphStmt::Edge(e) => {
                    let endpoints: Vec<String> = e.endpoints.iter().map(|p| p.last().name.clone()).collect();
                    out.push_str(&format!(
                        "        // edge: {}\n",
                        endpoints.join(" -> ")
                    ));
                }
                GraphStmt::Let(l) => {
                    out.push_str(&format!("        {}", emit_let_java(l, "        ")));
                }
                GraphStmt::Stmt(s) => {
                    out.push_str(&emit_stmt_java(s, "        "));
                }
                GraphStmt::Item(_) => {}
            }
        }
    } else {
        out.push_str("        // TODO: AgentLoop\n");
    }
    out.push_str("    }\n");
    out.push_str("}\n\n");
    out
}

/// impl → class implementing interface
fn java_impl(imp: &ImplDef) -> String {
    let self_name = match &imp.self_ty.kind {
        TypeKind::Path(pt) => java_ident(&pt.path.last().name),
        _ => "HslImpl".into(),
    };
    let mut out = String::new();
    if let Some(trait_ty) = &imp.trait_ty {
        let trait_name = java_type(trait_ty);
        out.push_str(&format!(
            "public class {} implements {} {{\n",
            self_name, trait_name
        ));
    } else {
        out.push_str(&format!("public class {} {{\n", self_name));
    }
    for item in &imp.items {
        if let ImplItem::Fn(f) = item {
            let name = java_ident(&f.name.name);
            let ret = f.ret.as_ref().map(|t| java_type(t)).unwrap_or_else(|| "void".into());
            let params: Vec<String> = f.params.iter().filter_map(java_param).collect();
            out.push_str(&format!(
                "    @Override\n    public {} {}({})",
                ret, name, params.join(", ")
            ));
            if let Some(body) = &f.body {
                out.push_str(" {\n");
                out.push_str(&emit_block_java(body, 3));
                out.push_str("    }\n");
            } else {
                out.push_str(" {\n        throw new UnsupportedOperationException();\n    }\n");
            }
        }
    }
    out.push_str("}\n\n");
    out
}

/// const → public static final
fn java_const(c: &ConstDef) -> String {
    let name = java_ident(&c.name.name);
    let ty = java_type(&c.ty);
    let val = emit_expr_java(&c.value, 0);
    format!("public static final {} {} = {};\n\n", ty, name, val)
}

/// typealias → comment (Java has no type alias)
fn java_typealias(a: &TypeAliasDef) -> String {
    format!(
        "// Type alias: {} = {}\n\n",
        java_ident(&a.name.name),
        java_type(&a.ty)
    )
}

/// macro_rules → comment (Java has no macro system)
fn java_macro_rules(m: &MacroRulesDefinition) -> String {
    format!(
        "// Macro: {} ({} rules) — Java has no macro system\n\n",
        java_ident(&m.name.name),
        m.rules.len()
    )
}

// ────────────────────────────────────────────────────────────────
// Block / statement emitter
// ────────────────────────────────────────────────────────────────

/// Emit a BlockExpr with indentation
fn emit_block_java(block: &BlockExpr, indent: usize) -> String {
    let pad = " ".repeat(indent);
    let mut out = String::new();
    for stmt in &block.stmts {
        out.push_str(&emit_stmt_java(stmt, &pad));
    }
    if let Some(tail) = &block.tail {
        let expr_str = emit_expr_java(tail, indent);
        match &tail.kind {
            ExprKind::If { .. }
            | ExprKind::Match { .. }
            | ExprKind::For { .. }
            | ExprKind::While { .. }
            | ExprKind::WhileLet { .. }
            | ExprKind::Loop { .. }
            | ExprKind::Block(_) => {
                out.push_str(&expr_str);
            }
            ExprKind::Return(_) => {
                out.push_str(&format!("{}{};\n", pad, expr_str));
            }
            _ => {
                // Other expression as tail: comment it
                out.push_str(&format!("{}// expression: {}\n", pad, expr_str));
            }
        }
    }
    out
}

fn emit_stmt_java(stmt: &Stmt, pad: &str) -> String {
    match stmt {
        Stmt::Let(l) => emit_let_java(l, pad),
        Stmt::Item(_) | Stmt::Empty(_) => String::new(),
        Stmt::Expr { expr, has_semi: _ } => {
            let expr_str = emit_expr_java(expr, pad.len());
            match &expr.kind {
                ExprKind::If { .. }
                | ExprKind::Match { .. }
                | ExprKind::For { .. }
                | ExprKind::While { .. }
                | ExprKind::WhileLet { .. }
                | ExprKind::Loop { .. }
                | ExprKind::Block(_) => {
                    format!("{}\n", expr_str)
                }
                _ => format!("{}{};\n", pad, expr_str),
            }
        }
    }
}

fn emit_let_java(l: &LetStmt, pad: &str) -> String {
    let name = match &l.pattern.kind {
        PatternKind::Ident { name, .. } => java_ident(&name.name),
        _ => "_v".into(),
    };
    let init = l.init.as_ref().map(|e| emit_expr_java(e, pad.len()));
    let ty_str = l.ty.as_ref().map(|t| java_type(t));

    match (ty_str, init) {
        (Some(ty), Some(expr_str)) => {
            format!("{}final {} {} = {};\n", pad, ty, name, expr_str)
        }
        (None, Some(expr_str)) => {
            format!("{}var {} = {};\n", pad, name, expr_str)
        }
        (Some(ty), None) => {
            format!("{}{} {};\n", pad, ty, name)
        }
        (None, None) => {
            format!("{}var {};\n", pad, name)
        }
    }
}

// ────────────────────────────────────────────────────────────────
// Expression emitter
// ────────────────────────────────────────────────────────────────

fn emit_expr_java(expr: &Expr, indent: usize) -> String {
    let pad = " ".repeat(indent);
    match &expr.kind {
        ExprKind::Literal(lit) => java_literal(lit),
        ExprKind::Path(p) => {
            let segs: Vec<String> = p.segments.iter().map(|s| java_ident(&s.name)).collect();
            segs.join(".")
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let l = emit_expr_java(lhs, indent);
            let r = emit_expr_java(rhs, indent);
            let op_str = java_binop(op);
            format!("({} {} {})", l, op_str, r)
        }
        ExprKind::Unary { op, operand } => {
            let e = emit_expr_java(operand, indent);
            match op {
                UnaryOp::Neg => format!("(-{})", e),
                UnaryOp::Not => format!("(!{})", e),
                UnaryOp::Deref | UnaryOp::Ref | UnaryOp::RefMut => e,
            }
        }
        ExprKind::Call { callee, args } => {
            let f = emit_expr_java(callee, indent);
            let a: Vec<String> = args.iter().map(|a| emit_expr_java(a, indent)).collect();
            format!("{}({})", f, a.join(", "))
        }
        ExprKind::MethodCall { receiver, method, generic_args: _, args } => {
            let recv = emit_expr_java(receiver, indent);
            let mname = java_ident(&method.name);
            let a: Vec<String> = args.iter().map(|a| emit_expr_java(a, indent)).collect();
            java_std_method_call(&recv, &mname, &a)
        }
        ExprKind::Field { base, field } => {
            let b = emit_expr_java(base, indent);
            let fname = match field {
                FieldIndex::Named(id) => java_ident(&id.name),
                FieldIndex::Index(i, _) => format!("_{}", i),
            };
            format!("{}.{}", b, fname)
        }
        ExprKind::Index { base, index } => {
            let b = emit_expr_java(base, indent);
            let i = emit_expr_java(index, indent);
            format!("{}.get({})", b, i)
        }
        ExprKind::Slice { base, range } => {
            let b = emit_expr_java(base, indent);
            let lo_s = range.lo.as_ref().map(|e| emit_expr_java(e, indent)).unwrap_or_else(|| "0".into());
            let hi_s = range.hi.as_ref().map(|e| emit_expr_java(e, indent)).unwrap_or_else(|| format!("{}.size()", b));
            format!("{}.subList({}, {})", b, lo_s, hi_s)
        }
        ExprKind::Range(re) => {
            let lo_s = re.lo.as_ref().map(|e| emit_expr_java(e, indent)).unwrap_or_else(|| "0".into());
            let hi_s = re.hi.as_ref().map(|e| emit_expr_java(e, indent)).unwrap_or_else(|| "".into());
            if re.inclusive {
                format!("IntStream.rangeClosed({}, {})", lo_s, hi_s)
            } else {
                format!("IntStream.range({}, {})", lo_s, hi_s)
            }
        }
        ExprKind::Assign { lhs, rhs } => {
            let t = emit_expr_java(lhs, indent);
            let v = emit_expr_java(rhs, indent);
            format!("{} = {}", t, v)
        }
        ExprKind::CompoundAssign { op, lhs, rhs } => {
            let t = emit_expr_java(lhs, indent);
            let v = emit_expr_java(rhs, indent);
            let op_str = match op {
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Mul => "*",
                BinaryOp::Div => "/",
                BinaryOp::Rem => "%",
                BinaryOp::BitAnd => "&",
                BinaryOp::BitOr => "|",
                BinaryOp::BitXor => "^",
                BinaryOp::Shl => "<<",
                BinaryOp::Shr => ">>",
                _ => "+",
            };
            format!("{} {}= {}", t, op_str, v)
        }
        ExprKind::If { cond, then, else_ } => {
            let c = emit_expr_java(cond, indent);
            let mut out = format!("{}if ({}) {{\n", pad, c);
            out.push_str(&emit_block_java(then, indent + 2));
            out.push_str(&format!("{}}}", pad));
            if let Some(els) = else_ {
                match &els.kind {
                    ExprKind::If { .. } => {
                        let else_code = emit_expr_java(els, indent);
                        let stripped = else_code.trim_start();
                        out.push_str(&format!(" else {}\n", stripped));
                    }
                    ExprKind::Block(b) => {
                        out.push_str(" else {\n");
                        out.push_str(&emit_block_java(b, indent + 2));
                        out.push_str(&format!("{}}}\n", pad));
                    }
                    _ => {
                        out.push_str(" else {\n");
                        out.push_str(&format!("{}    // else: {}\n", pad, emit_expr_java(els, indent + 2)));
                        out.push_str(&format!("{}}}\n", pad));
                    }
                }
            } else {
                out.push_str("\n");
            }
            out
        }
        ExprKind::Match { scrutinee, arms } => {
            let s = emit_expr_java(scrutinee, indent);
            let mut out = format!("{}switch ({}) {{\n", pad, s);
            for (i, arm) in arms.iter().enumerate() {
                if let Some(guard) = &arm.guard {
                    out.push_str(&format!(
                        "{}    case {} when {} -> {{\n",
                        pad, java_match_pattern(&arm.pattern), emit_expr_java(guard, indent + 4)
                    ));
                } else if matches!(&arm.pattern.kind, PatternKind::Wildcard) {
                    out.push_str(&format!("{}    default -> {{\n", pad));
                } else {
                    out.push_str(&format!(
                        "{}    case {} -> {{\n",
                        pad, java_match_pattern(&arm.pattern)
                    ));
                }
                // Emit body as block
                let body_str = emit_expr_java(&arm.body, indent + 3);
                match &arm.body.kind {
                    ExprKind::Block(b) => {
                        out.push_str(&emit_block_java(b, indent + 3));
                    }
                    _ => {
                        out.push_str(&format!("{}        {}\n", pad, body_str));
                    }
                }
                out.push_str(&format!("{}    }}\n", pad));
                if matches!(&arm.pattern.kind, PatternKind::Wildcard) && i == arms.len() - 1 {
                    // default must be last; no break needed
                }
            }
            out.push_str(&format!("{}}}\n", pad));
            out
        }
        ExprKind::For { label: _, pattern, iter, body } => {
            let vname = match &pattern.kind {
                PatternKind::Ident { name, .. } => java_ident(&name.name),
                PatternKind::Wildcard => "_".into(),
                _ => "_v".into(),
            };
            let iter_expr = emit_expr_java(iter, indent);
            let mut out = String::new();
            match &iter.kind {
                ExprKind::Range(re) => {
                    let lo_s = re.lo.as_ref().map(|e| emit_expr_java(e, indent)).unwrap_or_else(|| "0".into());
                    let hi_s = re.hi.as_ref().map(|e| emit_expr_java(e, indent)).unwrap_or_else(|| "".into());
                    if re.inclusive {
                        out.push_str(&format!(
                            "{}for (int {} = {}; {} <= {}; {}++) {{\n",
                            pad, vname, lo_s, vname, hi_s, vname
                        ));
                    } else {
                        out.push_str(&format!(
                            "{}for (int {} = {}; {} < {}; {}++) {{\n",
                            pad, vname, lo_s, vname, hi_s, vname
                        ));
                    }
                }
                _ => {
                    out.push_str(&format!("{}for (var {} : {}) {{\n", pad, vname, iter_expr));
                }
            }
            out.push_str(&emit_block_java(body, indent + 2));
            out.push_str(&format!("{}}}\n", pad));
            out
        }
        ExprKind::While { label: _, cond, body } => {
            let c = emit_expr_java(cond, indent);
            let mut out = format!("{}while ({}) {{\n", pad, c);
            out.push_str(&emit_block_java(body, indent + 2));
            out.push_str(&format!("{}}}\n", pad));
            out
        }
        ExprKind::WhileLet { label: _, pattern, expr: scrut, body } => {
            let e = emit_expr_java(scrut, indent);
            let mut out = String::new();
            match &pattern.kind {
                PatternKind::Ident { name, .. } => {
                    out.push_str(&format!("{}while ({} != null) {{\n", pad, e));
                    out.push_str(&format!("{}    var {} = {};\n", pad, java_ident(&name.name), e));
                }
                _ => {
                    out.push_str(&format!("{}while (true) {{\n", pad));
                    out.push_str(&format!("{}    // while-let pattern: {}\n", pad, java_match_pattern(pattern)));
                    out.push_str(&format!("{}    if (!(false)) break;\n", pad));
                }
            }
            out.push_str(&emit_block_java(body, indent + 2));
            out.push_str(&format!("{}}}\n", pad));
            out
        }
        ExprKind::Loop { label, body } => {
            let mut out = String::new();
            if let Some(lb) = label {
                out.push_str(&format!("{}{}: while (true) {{\n", pad, java_ident(&lb.name)));
            } else {
                out.push_str(&format!("{}while (true) {{\n", pad));
            }
            out.push_str(&emit_block_java(body, indent + 2));
            out.push_str(&format!("{}}}\n", pad));
            out
        }
        ExprKind::IfLet { pattern, expr: scrut, then, else_ } => {
            let e = emit_expr_java(scrut, indent);
            let mut out = String::new();
            match &pattern.kind {
                PatternKind::Ident { name, .. } => {
                    out.push_str(&format!("{}if ({} != null) {{\n", pad, e));
                    out.push_str(&format!("{}    var {} = {};\n", pad, java_ident(&name.name), e));
                }
                PatternKind::Path(p) => {
                    out.push_str(&format!(
                        "{}if ({} instanceof {}) {{\n",
                        pad, e, java_ident(&p.last().name)
                    ));
                }
                PatternKind::TupleStruct { path, .. } => {
                    out.push_str(&format!(
                        "{}if ({} instanceof {}) {{\n",
                        pad, e, java_ident(&path.last().name)
                    ));
                }
                PatternKind::Struct { path, .. } => {
                    out.push_str(&format!(
                        "{}if ({} instanceof {}) {{\n",
                        pad, e, java_ident(&path.last().name)
                    ));
                }
                PatternKind::Literal(lit) => {
                    let lv = java_pattern_literal(lit);
                    out.push_str(&format!("{}if ({}.equals({})) {{\n", pad, e, lv));
                }
                _ => {
                    out.push_str(&format!(
                        "{}if (true /* if-let: {} */) {{\n",
                        pad, java_match_pattern(pattern)
                    ));
                }
            }
            out.push_str(&emit_block_java(then, indent + 2));
            out.push_str(&format!("{}}}", pad));
            if let Some(els) = else_ {
                match &els.kind {
                    ExprKind::Block(b) => {
                        out.push_str(" else {\n");
                        out.push_str(&emit_block_java(b, indent + 2));
                        out.push_str(&format!("{}}}\n", pad));
                    }
                    ExprKind::If { .. } => {
                        let else_code = emit_expr_java(els, indent);
                        let stripped = else_code.trim_start();
                        out.push_str(&format!(" else {}\n", stripped));
                    }
                    _ => {
                        out.push_str(" else {\n");
                        out.push_str(&format!("{}    // else\n", pad));
                        out.push_str(&format!("{}}}\n", pad));
                    }
                }
            } else {
                out.push_str("\n");
            }
            out
        }
        ExprKind::Closure { params, body, ret: _, .. } => {
            let param_strs: Vec<String> = params
                .iter()
                .map(|p| match &p.kind {
                    ParamKind::Pattern(pat) => match &pat.kind {
                        PatternKind::Ident { name, .. } => java_ident(&name.name),
                        _ => "_arg".into(),
                    },
                    ParamKind::Self_(_) => "_self".into(),
                })
                .collect();
            let body_expr = emit_expr_java(body, indent);
            format!("({}) -> {}", param_strs.join(", "), body_expr)
        }
        ExprKind::Return(value) => {
            if let Some(v) = value {
                format!("return {}", emit_expr_java(v, indent))
            } else {
                "return".into()
            }
        }
        ExprKind::Break { label, value } => {
            let mut out = "break".to_string();
            if let Some(lb) = label {
                out.push_str(&format!(" {}", java_ident(&lb.name)));
            }
            if let Some(v) = value {
                out.push_str(&format!(" /* value: {} */", emit_expr_java(v, indent)));
            }
            out
        }
        ExprKind::Continue { label } => {
            let mut out = "continue".to_string();
            if let Some(lb) = label {
                out.push_str(&format!(" {}", java_ident(&lb.name)));
            }
            out
        }
        ExprKind::Array(elems) => {
            let items: Vec<String> = elems.iter().map(|e| emit_expr_java(e, indent)).collect();
            if items.is_empty() {
                "List.of()".into()
            } else {
                format!("List.of({})", items.join(", "))
            }
        }
        ExprKind::ArrayRepeat { elem, count } => {
            let e = emit_expr_java(elem, indent);
            let c = emit_expr_java(count, indent);
            format!(
                "Collections.nCopies({}, {}).stream().collect(Collectors.toList())",
                c, e
            )
        }
        ExprKind::Struct { path, fields, spread } => {
            let name = java_ident(&path.last().name);
            let field_strs: Vec<String> = fields
                .iter()
                .map(|f| match &f.name {
                    FieldIndex::Named(id) => {
                        let val = f.value.as_ref().map(|v| emit_expr_java(v, indent)).unwrap_or_else(|| java_ident(&id.name).clone());
                        format!("{}", val)
                    }
                    FieldIndex::Index(_, _) => {
                        f.value.as_ref().map(|v| emit_expr_java(v, indent)).unwrap_or_else(|| "0".into())
                    }
                })
                .collect();
            let mut args = field_strs.join(", ");
            if let Some(spread_expr) = spread {
                if !args.is_empty() {
                    args.push_str(", ");
                }
                args.push_str(&format!("/* spread: {} */", emit_expr_java(spread_expr, indent)));
            }
            format!("new {}({})", name, args)
        }
        ExprKind::Tuple(elems) => {
            let items: Vec<String> = elems.iter().map(|e| emit_expr_java(e, indent)).collect();
            format!("new Tuple{}({})", elems.len(), items.join(", "))
        }
        ExprKind::Block(b) => {
            let mut out = format!("{}{{\n", pad);
            out.push_str(&emit_block_java(b, indent + 2));
            out.push_str(&format!("{}}}", pad));
            out
        }
        ExprKind::AsyncBlock { body, .. } => {
            let mut out = format!("{}/* async block */ {{\n", pad);
            out.push_str(&emit_block_java(body, indent + 2));
            out.push_str(&format!("{}}}", pad));
            out
        }
        ExprKind::Try(inner) => {
            let mut out = format!("{}try {{\n", pad);
            out.push_str(&format!("{}    {};\n", pad, emit_expr_java(inner, indent)));
            out.push_str(&format!("{}}} catch (Exception _e) {{\n", pad));
            out.push_str(&format!("{}    throw _e;\n", pad));
            out.push_str(&format!("{}}}", pad));
            out
        }
        ExprKind::Await(inner) => {
            let e = emit_expr_java(inner, indent);
            format!("{}.join()", e)
        }
        ExprKind::Cast { expr, ty } => {
            let e = emit_expr_java(expr, indent);
            let t = java_type(ty);
            format!("(({}) {})", t, e)
        }
        ExprKind::Macro { path, args } => {
            let macro_name = path.segments.last().map(|s| s.name.as_str()).unwrap_or("");
            let a: Vec<String> = args.tokens.iter().map(|t| format!("{:?}", t)).collect();
            match macro_name {
                "println" => {
                    if a.is_empty() {
                        "System.out.println()".into()
                    } else {
                        format!("System.out.println({})", a.join(" + "))
                    }
                }
                "eprintln" => {
                    if a.is_empty() {
                        "System.err.println()".into()
                    } else {
                        format!("System.err.println({})", a.join(" + "))
                    }
                }
                "format" => format!("/* format! */ \"{}\"", a.join(", ")),
                "dbg" => format!("/* dbg */ ({})", a.join(", ")),
                other => format!("/* macro {} */ {}({})", other, other, a.join(", ")),
            }
        }
        ExprKind::Native(nb) => {
            format!("/* native {} */ {}", nb.lang.name, nb.code)
        }
    }
}

// ────────────────────────────────────────────────────────────────
// Pattern emitter (for match/switch)
// ────────────────────────────────────────────────────────────────

fn java_match_pattern(pat: &Pattern) -> String {
    match &pat.kind {
        PatternKind::Wildcard => "default".into(),
        PatternKind::Ident { name, .. } => java_ident(&name.name),
        PatternKind::Literal(lit) => java_pattern_literal(lit),
        PatternKind::Path(p) => java_ident(&p.last().name),
        PatternKind::TupleStruct { path, elems, .. } => {
            let name = java_ident(&path.last().name);
            if elems.is_empty() {
                name
            } else {
                format!(
                    "{} {}",
                    name,
                    elems.iter().map(|e| java_match_pattern(e)).collect::<Vec<_>>().join(" ")
                )
            }
        }
        PatternKind::Struct { path, fields, .. } => {
            let name = java_ident(&path.last().name);
            if fields.is_empty() {
                name
            } else {
                let field_pats: Vec<String> = fields
                    .iter()
                    .map(|f| {
                        let fname = java_ident(&f.name.name);
                        let pat = f.pattern.as_ref().map(|p| java_match_pattern(p)).unwrap_or_else(|| fname.clone());
                        if fname == pat {
                            fname
                        } else {
                            format!("{} {}", fname, pat)
                        }
                    })
                    .collect();
                format!("{} {{ {} }}", name, field_pats.join(", "))
            }
        }
        PatternKind::Tuple { elems, .. } => {
            elems.iter().map(|e| java_match_pattern(e)).collect::<Vec<_>>().join(", ")
        }
        PatternKind::Or(pats) => pats
            .iter()
            .map(|e| java_match_pattern(e))
            .collect::<Vec<_>>()
            .join(", "),
        PatternKind::Range { lo, hi, inclusive } => {
            // Pattern Range uses Box<Pattern>, not Box<Expr>
            let l = match &lo.kind {
                PatternKind::Literal(lit) => java_pattern_literal(lit),
                _ => "?".into(),
            };
            let h = match &hi.kind {
                PatternKind::Literal(lit) => java_pattern_literal(lit),
                _ => "?".into(),
            };
            if *inclusive {
                format!("{} .. {}", l, h)
            } else {
                format!("{} ... {}", l, h)
            }
        }
        PatternKind::Rest => "...".into(),
    }
}

fn java_pattern_literal(lit: &Literal) -> String {
    match &lit.kind {
        LiteralKind::Bool(b) => b.to_string(),
        LiteralKind::Int { value, .. } => value.to_string(),
        LiteralKind::Float { value, .. } => {
            let mut s = value.to_string();
            if !s.contains('.') {
                s.push_str(".0");
            }
            s
        }
        LiteralKind::Str { value, .. } => format!("\"{}\"", value),
        LiteralKind::Char(c) => format!("'{}'", c),
    }
}

// ────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────

fn java_literal(lit: &Literal) -> String {
    match &lit.kind {
        LiteralKind::Bool(b) => if *b { "true".into() } else { "false".into() },
        LiteralKind::Int { value, suffix } => {
            let mut s = value.to_string();
            match suffix {
                Some(IntSuffix::I8) | Some(IntSuffix::U8) => s.push_str("/* byte */"),
                Some(IntSuffix::I16) | Some(IntSuffix::U16) => s.push_str("/* short */"),
                Some(IntSuffix::I32) => {},
                Some(IntSuffix::U32) | Some(IntSuffix::I64) | Some(IntSuffix::U64) => s.push_str("L"),
                Some(IntSuffix::Usize) | Some(IntSuffix::Isize) => s.push_str("L"),
                Some(IntSuffix::I128) | Some(IntSuffix::U128) => s.push_str("/* big */"),
                None => {},
            }
            s
        }
        LiteralKind::Float { value, suffix } => {
            let mut s = value.to_string();
            if !s.contains('.') {
                s.push_str(".0");
            }
            match suffix {
                Some(FloatSuffix::F32) => s.push_str("f"),
                Some(FloatSuffix::F64) | None => {},
            }
            s
        }
        LiteralKind::Str { value, .. } => format!("\"{}\"", value),
        LiteralKind::Char(c) => format!("'{}'", c),
    }
}

fn java_binop(op: &BinaryOp) -> &'static str {
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

/// Java std method dispatch (50+ methods, aligned with registry.ts Java branches)
fn java_std_method_call(recv: &str, name: &str, args: &[String]) -> String {
    let a0 = || args.first().map(|s| s.as_str()).unwrap_or("");
    let a1 = || args.get(1).map(|s| s.as_str()).unwrap_or("");
    match name {
        // Vec/List methods
        "push" | "push_back" => format!("{}.add({})", recv, a0()),
        "pop" => format!("{}.remove({}.size() - 1)", recv, recv),
        "len" | "size" | "length" => format!("{}.size()", recv),
        "is_empty" | "empty" => format!("{}.isEmpty()", recv),
        "clear" => format!("{}.clear()", recv),
        "contains" => format!("{}.contains({})", recv, a0()),
        "sort" => format!("{}.sort(null)", recv),
        "reverse" => format!("{}.reverse()", recv),
        "first" => format!("{}.get(0)", recv),
        "last" => format!("{}.get({}.size() - 1)", recv, recv),
        "get" => format!("{}.get({})", recv, a0()),
        "remove_at" => format!("{}.remove((int){})", recv, a0()),
        "insert_at" => format!("{}.add((int){}, {})", recv, a0(), a1()),
        "extend" => format!("{}.addAll({})", recv, a0()),
        "clone" => format!("new ArrayList<>({})", recv),
        "capacity" => format!("{}/* capacity */", recv),
        "with_capacity" => format!("new ArrayList<>({})", a0()),
        "retain" => format!("{}.removeIf({})", recv, a0()),
        "dedup" => format!("{}/* dedup */", recv),
        "find" => format!("{}.stream().filter({}).findFirst()", recv, a0()),
        "position" => format!("{}.indexOf({})", recv, a0()),
        "binary_search" => format!("{}.indexOf({})", recv, a0()),
        "chunk" => format!("{}/* chunk */", recv),
        "windows" => format!("{}/* windows */", recv),
        "chunks" => format!("{}/* chunks */", recv),
        "zip" => format!("{}/* zip */", recv),
        "unzip" => format!("{}/* unzip */", recv),
        "flatten" => format!("{}.stream().flatMap({}).collect(Collectors.toList())", recv, a0()),
        "fold" => format!("{}.stream().reduce({})", recv, a0()),
        "all" => format!("{}.stream().allMatch({})", recv, a0()),
        "any" => format!("{}.stream().anyMatch({})", recv, a0()),
        "sum" => format!("{}.stream().mapToInt(Integer::parseInt).sum()", recv),
        "product" => format!("{}/* product */", recv),
        "count" => format!("{}.size()", recv),
        "min_by" => format!("{}.stream().min({}).orElseThrow()", recv, a0()),
        "max_by" => format!("{}.stream().max({}).orElseThrow()", recv, a0()),
        "filter" => format!("{}.stream().filter({}).collect(Collectors.toList())", recv, a0()),
        "map" => format!("{}.stream().map({}).collect(Collectors.toList())", recv, a0()),
        "collect" => format!("{}.stream().collect(Collectors.toList())", recv),
        "iter" => format!("{}.iterator()", recv),
        "next" => format!("{}.next()", recv),
        // String methods
        "to_string" | "to_str" => format!("{}.toString()", recv),
        "trim" => format!("{}.trim()", recv),
        "to_lowercase" | "to_lower" => format!("{}.toLowerCase()", recv),
        "to_uppercase" | "to_upper" => format!("{}.toUpperCase()", recv),
        "starts_with" => format!("{}.startsWith({})", recv, a0()),
        "ends_with" => format!("{}.endsWith({})", recv, a0()),
        "replace" => format!("{}.replace({}.replace({}, {}))", recv, recv, a0(), a1()),
        "split" => format!("{}.split({})", recv, a0()),
        "join" => format!("String.join({}.toArray(new String[0]))", a0()),
        "repeat" => format!("{}.repeat((int){})", recv, a0()),
        "chars" | "char_count" => format!("{}.length()", recv),
        "split_whitespace" => format!("{}.trim().split(\"\\\\s+\")", recv),
        "lines" => format!("{}.split(\"\\n\")", recv),
        "parse" => format!("{}.parse{}()", recv, ""),
        "append" => format!("{}.append({})", recv, a0()),
        // Map methods
        "insert" => format!("{}.put({}, {})", recv, a0(), a1()),
        "remove" => format!("{}.remove({})", recv, a0()),
        "contains_key" => format!("{}.containsKey({})", recv, a0()),
        "keys" => format!("{}.keySet()", recv),
        "values" => format!("{}.values()", recv),
        "entries" => format!("{}.entrySet()", recv),
        // Option methods
        "is_some" => format!("{}.isPresent()", recv),
        "is_none" => format!("{}.isEmpty()", recv),
        "unwrap" => format!("{}.get()", recv),
        "expect" => format!("{}.orElseThrow()", recv),
        "unwrap_or" => format!("{}.orElse({})", recv, a0()),
        "ok_or" => format!("{}.orElseThrow()", recv),
        "or" => format!("{}.or(() -> {})", recv, a0()),
        "and_then" => format!("{}.flatMap({})", recv, a0()),
        "unwrap_or_else" => format!("{}.orElseGet({})", recv, a0()),
        // Math
        "abs" => format!("Math.abs({})", a0()),
        "min" => format!("Math.min({}, {})", a0(), a1()),
        "max" => format!("Math.max({}, {})", a0(), a1()),
        "floor" => format!("(long)Math.floor({})", a0()),
        "ceil" => format!("(long)Math.ceil({})", a0()),
        "round" => format!("Math.round({})", a0()),
        "sqrt" => format!("Math.sqrt({})", a0()),
        // Generic / fallback
        _ => {
            if args.is_empty() {
                format!("{}.{}()", recv, java_ident(name))
            } else {
                format!("{}.{}({})", recv, java_ident(name), args.join(", "))
            }
        }
    }
}
