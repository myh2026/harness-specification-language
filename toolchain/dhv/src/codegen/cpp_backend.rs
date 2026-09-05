//! C++ backend (Logic tier) — type mapping + full function body translation
//! C++ is a Tier 1 Harness core language. This backend generates C++17 code.
//! struct → struct/class; enum → enum class / std::variant;
//! fn → free function; trait → pure virtual base class (abstract);
//! impl → class methods; const → constexpr; graph → int main()
//! Expressions: binary/unary/call/method/field/index/slice/range/assign/
//!   compound_assign/if/match/for/while/while-let/loop/if-let/closure/
//!   return/break/continue/array/struct/tuple/block/try/await/cast/native/macro

use crate::ast::*;
use crate::codegen::{CodegenBackend, CodegenContext};

pub struct CppBackend;

impl CodegenBackend for CppBackend {
    fn lang(&self) -> &'static str { "cpp" }

    fn emit_item(&self, ctx: &CodegenContext, item: &Item) -> Result<String, String> {
        let mut out = String::new();
        out.push_str(&format!("// {}\n", crate::sourcemap::generated_header("cpp")));
        out.push_str("#include <string>\n");
        out.push_str("#include <vector>\n");
        out.push_str("#include <unordered_map>\n");
        out.push_str("#include <unordered_set>\n");
        out.push_str("#include <optional>\n");
        out.push_str("#include <variant>\n");
        out.push_str("#include <cstdint>\n");
        out.push_str("#include <stdexcept>\n");
        out.push_str("#include <algorithm>\n");
        out.push_str("#include <functional>\n");
        out.push_str("using namespace std;\n\n");
        match item {
            Item::Struct(s) => out.push_str(&cpp_struct(s)),
            Item::Enum(e) => out.push_str(&cpp_enum(e)),
            Item::Trait(t) => out.push_str(&cpp_trait(t)),
            Item::Fn(f) => out.push_str(&cpp_fn(f)),
            Item::Graph(g) => out.push_str(&cpp_graph(g, ctx)),
            Item::Impl(imp) => out.push_str(&cpp_impl(imp)),
            Item::Const(c) => out.push_str(&cpp_const(c)),
            Item::TypeAlias(a) => out.push_str(&cpp_typealias(a)),
            _ => return Err(format!("cpp 后端暂不支持 {}", crate::ast::item_kind_name(item))),
        }
        Ok(out)
    }
}

// ────────────────────────────────────────────────────────────────
// Item-level emitters
// ────────────────────────────────────────────────────────────────

/// struct → class/struct with public fields
fn cpp_struct(s: &StructDef) -> String {
    let mut out = String::new();
    out.push_str(&format!("struct {} {{\n", cpp_ident(&s.name.name)));
    out.push_str("public:\n");
    match &s.kind {
        StructKind::Named(fields) => {
            for f in fields {
                let name = f.name.as_ref().map(|n| cpp_ident(&n.name)).unwrap_or_else(|| "_".into());
                out.push_str(&format!("    {} {};\n", cpp_type(&f.ty), name));
            }
        }
        StructKind::Tuple(fields) => {
            for (i, _f) in fields.iter().enumerate() {
                out.push_str(&format!("    std::tuple_element_t<{}, decltype(_tuple_)> _{};\n", i, i));
            }
        }
        StructKind::Unit => {}
    }
    out.push_str("};\n\n");
    out
}

/// enum → unit: enum class X : int; data: struct variants + std::variant
fn cpp_enum(e: &EnumDef) -> String {
    let mut out = String::new();
    let has_data = e.variants.iter().any(|v| !matches!(&v.fields, StructKind::Unit));
    let name = cpp_ident(&e.name.name);
    if !has_data {
        out.push_str(&format!("enum class {} : int {{\n", name));
        for (i, v) in e.variants.iter().enumerate() {
            let comma = if i < e.variants.len() - 1 { "," } else { "" };
            out.push_str(&format!("    {}{}\n", cpp_ident(&v.name.name), comma));
        }
        out.push_str("};\n\n");
    } else {
        // Data-carrying enum → struct per variant + std::variant
        for v in &e.variants {
            let vname = cpp_ident(&v.name.name);
            match &v.fields {
                StructKind::Unit => {
                    out.push_str(&format!("struct {} {{}};\n", vname));
                }
                StructKind::Named(fields) => {
                    out.push_str(&format!("struct {} {{\n", vname));
                    for f in fields {
                        let fname = f.name.as_ref().map(|n| cpp_ident(&n.name)).unwrap_or_else(|| "_".into());
                        out.push_str(&format!("    {} {};\n", cpp_type(&f.ty), fname));
                    }
                    out.push_str("};\n");
                }
                StructKind::Tuple(fields) => {
                    out.push_str(&format!("struct {} {{\n", vname));
                    for (i, f) in fields.iter().enumerate() {
                        out.push_str(&format!("    {} _{};\n", cpp_type(&f.ty), i));
                    }
                    out.push_str("};\n");
                }
            }
        }
        let variant_types = e.variants.iter().map(|v| cpp_ident(&v.name.name)).collect::<Vec<_>>();
        out.push_str(&format!("using {} = std::variant<{}>;\n\n", name, variant_types.join(", ")));
    }
    out
}

/// trait → abstract base class with pure virtual methods
fn cpp_trait(t: &TraitDef) -> String {
    let mut out = String::new();
    out.push_str(&format!("class {} {{\n", cpp_ident(&t.name.name)));
    out.push_str("public:\n");
    for ti in &t.items {
        if let TraitItem::FnSig(sig) = ti {
            let params = sig.params.iter().filter_map(cpp_param).collect::<Vec<_>>().join(", ");
            let ret = sig.ret.as_ref().map(|t| cpp_type(t)).unwrap_or_else(|| "void".into());
            out.push_str(&format!(
                "    virtual {} {}({}) = 0;\n",
                ret,
                cpp_ident(&sig.name.name),
                params
            ));
        }
    }
    out.push_str(&format!("    virtual ~{}() = default;\n", cpp_ident(&t.name.name)));
    out.push_str("};\n\n");
    out
}

/// fn → free function
fn cpp_fn(f: &FnDef) -> String {
    let mut out = String::new();
    let params = f.params.iter().filter_map(cpp_param).collect::<Vec<_>>().join(", ");
    let ret = f.ret.as_ref().map(|t| cpp_type(t)).unwrap_or_else(|| "void".into());
    out.push_str(&format!("{} {}({}) {{\n", ret, cpp_ident(&f.name.name), params));
    if let Some(body) = &f.body {
        out.push_str(&emit_block_cpp(body, 1));
    } else {
        out.push_str("    // TODO\n");
    }
    out.push_str("}\n\n");
    out
}

/// graph → int main() entry
fn cpp_graph(g: &GraphDef, ctx: &CodegenContext) -> String {
    let mut out = String::new();
    out.push_str(&format!("// graph {} — scale: {:?}\n", g.name.name, ctx.scale));
    out.push_str("int main() {\n");
    if !g.body.is_empty() {
        out.push_str("    // TODO: graph body GraphStmt translation\n");
    } else {
        out.push_str("    // TODO: AgentLoop\n");
    }
    out.push_str("    return 0;\n");
    out.push_str("}\n\n");
    out
}

/// impl → class methods (non-trait: class declaration; trait impl: override methods)
fn cpp_impl(imp: &ImplDef) -> String {
    let mut out = String::new();
    let self_type = cpp_type(&imp.self_ty);
    if let Some(trait_ty) = &imp.trait_ty {
        out.push_str(&format!("// impl {} for {}\n", cpp_type(trait_ty), self_type));
    } else {
        out.push_str(&format!("// impl for {}\n", self_type));
    }
    for ii in &imp.items {
        if let ImplItem::Fn(f) = ii {
            let is_override = imp.trait_ty.is_some();
            let params = f.params.iter().filter_map(cpp_param).collect::<Vec<_>>().join(", ");
            let ret = f.ret.as_ref().map(|t| cpp_type(t)).unwrap_or_else(|| "void".into());
            out.push_str(&format!(
                "{} {} {}::{}({}) {{\n",
                if is_override { "" } else { "" }, // virtual handled by trait
                ret,
                self_type,
                cpp_ident(&f.name.name),
                params
            ));
            if let Some(body) = &f.body {
                out.push_str(&emit_block_cpp(body, 1));
            } else {
                out.push_str("    // TODO\n");
            }
            out.push_str("}\n\n");
        }
    }
    out
}

/// const → constexpr
fn cpp_const(c: &ConstDef) -> String {
    format!("constexpr {} {} = {};\n\n", cpp_type(&c.ty), cpp_ident(&c.name.name), emit_expr_cpp(&c.value))
}

/// typealias → using alias
fn cpp_typealias(a: &TypeAliasDef) -> String {
    format!("using {} = {};\n\n", cpp_ident(&a.name.name), cpp_type(&a.ty))
}

// ────────────────────────────────────────────────────────────────
// Type mapping
// ────────────────────────────────────────────────────────────────

/// C++17 type mapping (aligns with dhv-ts registry.ts TypeMap for cpp)
fn cpp_type(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(p) => {
            let name = p.path.segments.last().map(|s| s.name.clone()).unwrap_or_default();
            match name.as_str() {
                "bool" => "bool".into(),
                "String" | "str" => "std::string".into(),
                "char" => "char".into(),
                "i8" => "int8_t".into(),
                "i16" => "int16_t".into(),
                "i32" => "int32_t".into(),
                "i64" => "int64_t".into(),
                "u8" => "uint8_t".into(),
                "u16" => "uint16_t".into(),
                "u32" => "uint32_t".into(),
                "u64" => "uint64_t".into(),
                "usize" => "size_t".into(),
                "isize" => "intptr_t".into(),
                "f32" => "float".into(),
                "f64" => "double".into(),
                "Vec" => {
                    let elem = p.generic_args.iter().next().map(cpp_generic).unwrap_or_else(|| "int".into());
                    format!("std::vector<{}>", elem)
                }
                "HashMap" => {
                    let (k, v) = cpp_two_generics(&p.generic_args);
                    format!("std::unordered_map<{}, {}>", k, v)
                }
                "HashSet" => {
                    let elem = p.generic_args.iter().next().map(cpp_generic).unwrap_or_else(|| "int".into());
                    format!("std::unordered_set<{}>", elem)
                }
                "Option" => {
                    let elem = p.generic_args.iter().next().map(cpp_generic).unwrap_or_else(|| "int".into());
                    format!("std::optional<{}>", elem)
                }
                "Result" => {
                    let (ok, _err) = cpp_two_generics(&p.generic_args);
                    ok // C++ doesn't have Result; emit Ok type
                }
                "Box" => {
                    let elem = p.generic_args.iter().next().map(cpp_generic).unwrap_or_else(|| "int".into());
                    format!("std::unique_ptr<{}>", elem)
                }
                other => cpp_ident(other),
            }
        }
        TypeKind::Ref { inner, mutable: _, .. } => cpp_type(inner), // C++ uses references implicitly
        TypeKind::Tuple(elems) => {
            format!("std::tuple<{}>", elems.iter().map(cpp_type).collect::<Vec<_>>().join(", "))
        }
        TypeKind::Array { elem, len } => {
            let sz = match &len.kind {
                ConstArgKind::Literal(lit) => match &lit.kind {
                    LiteralKind::Int { value, .. } => value.to_string(),
                    _ => String::from("0"),
                },
                _ => String::from("0"),
            };
            format!("std::array<{}, {}>", cpp_type(elem), sz)
        }
        TypeKind::Slice(elem) => {
            // C++ has no slice; use vector reference
            format!("std::vector<{}>", cpp_type(elem))
        }
        TypeKind::FnPtr { params, ret } => {
            let ps: Vec<String> = params.iter().map(cpp_type).collect();
            let r = ret.as_ref().map(|t| cpp_type(t)).unwrap_or_else(|| String::from("void"));
            format!("std::function<{}({})>", r, ps.join(", "))
        }
        TypeKind::DynTrait(_) | TypeKind::ImplTrait(_) | TypeKind::Infer => "auto".into(),
        TypeKind::Paren(inner) => cpp_type(inner),
        TypeKind::Never => "/* never */".into(),
    }
}

fn cpp_generic(arg: &GenericArg) -> String {
    match arg {
        GenericArg::Type(t) => cpp_type(t),
        GenericArg::Const(c) => match &c.kind {
            ConstArgKind::Literal(lit) => lit.raw.clone(),
            ConstArgKind::Block(_) => "0".into(),
        },
    }
}

fn cpp_two_generics(args: &[GenericArg]) -> (String, String) {
    let mut it = args.iter();
    let first = it.next().map(cpp_generic).unwrap_or_else(|| "int".into());
    let second = it.next().map(cpp_generic).unwrap_or_else(|| "int".into());
    (first, second)
}

fn cpp_param(p: &Param) -> Option<String> {
    match &p.kind {
        ParamKind::Self_(_) => None,
        ParamKind::Pattern(pat) => {
            let name = match &pat.kind {
                PatternKind::Ident { name, .. } => cpp_ident(&name.name),
                _ => "arg".into(),
            };
            Some(format!("{} {}", cpp_type(&p.ty), name))
        }
    }
}

// ────────────────────────────────────────────────────────────────
// C++ keyword avoidance
// ────────────────────────────────────────────────────────────────

const CPP_KW: &[&str] = &[
    "alignas", "alignof", "and", "and_eq", "asm", "auto", "bitand", "bitor",
    "bool", "break", "case", "catch", "char", "char8_t", "char16_t", "char32_t",
    "class", "compl", "concept", "const", "consteval", "constexpr", "constinit",
    "const_cast", "continue", "co_await", "co_return", "co_yield", "decltype",
    "default", "delete", "do", "double", "dynamic_cast", "else", "enum",
    "explicit", "export", "extern", "false", "float", "for", "friend", "goto",
    "if", "inline", "int", "long", "mutable", "namespace", "new", "noexcept",
    "not", "not_eq", "nullptr", "operator", "or", "or_eq", "private",
    "protected", "public", "register", "reinterpret_cast", "requires", "return",
    "short", "signed", "sizeof", "static", "static_assert", "static_cast",
    "struct", "switch", "template", "this", "thread_local", "throw", "true",
    "try", "typedef", "typeid", "typename", "union", "unsigned", "using",
    "virtual", "void", "volatile", "wchar_t", "while", "xor", "xor_eq",
    // C++20 additions
    "char8_t", "concept", "co_await", "co_return", "co_yield", "requires",
];

/// Suffix C++ keywords with underscore to avoid conflicts
fn cpp_ident(name: &str) -> String {
    if CPP_KW.contains(&name) {
        format!("{}_", name)
    } else {
        name.to_string()
    }
}

// ────────────────────────────────────────────────────────────────
// Statement block translation
// ────────────────────────────────────────────────────────────────

/// Emit a block of statements with given indentation level
fn emit_block_cpp(block: &BlockExpr, indent: usize) -> String {
    let pad = "    ".repeat(indent);
    let mut out = String::new();
    for s in &block.stmts {
        emit_stmt_cpp(s, &pad, &mut out);
    }
    if let Some(tail) = &block.tail {
        out.push_str(&format!("{}return {};\n", pad, emit_expr_cpp(tail)));
    }
    out
}

fn emit_stmt_cpp(stmt: &Stmt, pad: &str, out: &mut String) {
    match stmt {
        Stmt::Item(_) | Stmt::Empty(_) => {},
        Stmt::Let(l) => {
            let mut_kw = if l.mutable { "auto" } else { "const auto" };
            let name = match &l.pattern.kind {
                PatternKind::Ident { name, .. } => cpp_ident(&name.name),
                _ => "_v".into(),
            };
            out.push_str(&format!("{}{} {} = {};\n", pad, mut_kw, name, l.init.as_ref().map(|e| emit_expr_cpp(e)).unwrap_or_else(|| "0".into())));
        }
        Stmt::Expr { expr, has_semi: _ } => {
            let e = emit_expr_cpp(expr);
            out.push_str(&format!("{}{};\n", pad, e));
        }
    }
}

// ────────────────────────────────────────────────────────────────
// Expression translation
// ────────────────────────────────────────────────────────────────

/// Main expression emitter
fn emit_expr_cpp(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(lit) => cpp_literal(lit),
        ExprKind::Path(p) => {
            let segs: Vec<String> = p.segments.iter().map(|s| cpp_ident(&s.name)).collect();
            let prefix = if p.leading_colon { "::" } else { "" };
            format!("{}{}", prefix, segs.join("::"))
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let l = emit_expr_cpp(lhs);
            let r = emit_expr_cpp(rhs);
            cpp_binop(op, &l, &r)
        }
        ExprKind::Unary { op, operand } => {
            let e = emit_expr_cpp(operand);
            match op {
                UnaryOp::Neg => format!("-{}", e),
                UnaryOp::Not => format!("!{}", e),
                UnaryOp::Deref => format!("(*{})", e),
                UnaryOp::Ref | UnaryOp::RefMut => e, // C++ handles references implicitly
            }
        }
        ExprKind::Call { callee: func, args } => {
            let f = emit_expr_cpp(func);
            let a: Vec<String> = args.iter().map(emit_expr_cpp).collect();
            format!("{}({})", f, a.join(", "))
        }
        ExprKind::MethodCall { receiver, method, args, .. } => {
            let recv = emit_expr_cpp(receiver);
            let name = &method.name;
            let a: Vec<String> = args.iter().map(emit_expr_cpp).collect();
            cpp_std_method(&recv, name, &a)
        }
        ExprKind::Field { base, field } => {
            let b = emit_expr_cpp(base);
            match field {
                FieldIndex::Named(id) => format!("{}.{}", b, cpp_ident(&id.name)),
                FieldIndex::Index(i, _) => format!("std::get<{}>({})", i, b),
            }
        }
        ExprKind::Index { base, index } => {
            format!("{}[{}]", emit_expr_cpp(base), emit_expr_cpp(index))
        }
        ExprKind::Slice { base, .. } => {
            // C++ slice: just reference the vector (no built-in slicing)
            format!("{}", emit_expr_cpp(base))
        }
        ExprKind::Range(_re) => {
            String::from("/* range */")
        }
        ExprKind::Assign { lhs, rhs } => {
            format!("{} = {}", emit_expr_cpp(lhs), emit_expr_cpp(rhs))
        }
        ExprKind::CompoundAssign { op, lhs, rhs } => {
            let t = emit_expr_cpp(lhs);
            let v = emit_expr_cpp(rhs);
            let op_str = match op {
                BinaryOp::Add => "+=", BinaryOp::Sub => "-=", BinaryOp::Mul => "*=",
                BinaryOp::Div => "/=", BinaryOp::Rem => "%=",
                BinaryOp::BitAnd => "&=", BinaryOp::BitOr => "|=", BinaryOp::BitXor => "^=",
                BinaryOp::Shl => "<<=", BinaryOp::Shr => ">>=",
                _ => "=", // fallback
            };
            format!("{} {} {}", t, op_str, v)
        }
        ExprKind::If { cond, then, else_ } => {
            let c = emit_expr_cpp(cond);
            let mut out = format!("({} ? ", c);
            if let Some(tail) = &then.tail {
                out.push_str(&format!("{} : ", emit_expr_cpp(tail)));
            } else {
                // Block if without tail → statement-level; wrap as void
                out.push_str("([&]() { ");
                out.push_str(&emit_block_cpp(then, 0));
                out.push_str("}() : ");
            }
            if let Some(els) = else_ {
                out.push_str(&format!("{})", emit_expr_cpp(els)));
            } else {
                out.push_str("void())"); // no else → void fallback
            }
            out
        }
        ExprKind::Match { scrutinee, arms } => {
            // C++ match → while(true) + if-else chain (aligns with dhv-ts approach)
            let scrut = emit_expr_cpp(scrutinee);
            let mut out = format!("([&]() {{ auto _scrut = {}; while(true) {{\n", scrut);
            let inner_pad = "    ";
            for (i, arm) in arms.iter().enumerate() {
                if i == 0 {
                    out.push_str(&format!("{}if ({}) {{\n", inner_pad, cpp_match_condition(&arm.pattern, "_scrut")));
                } else if arm.guard.is_some() || !matches!(&arm.pattern.kind, PatternKind::Wildcard { .. }) {
                    out.push_str(&format!("{}else if ({}) {{\n", inner_pad, cpp_match_condition(&arm.pattern, "_scrut")));
                } else {
                    out.push_str(&format!("{}else {{\n", inner_pad));
                }
                // Add guard
                if let Some(guard) = &arm.guard {
                    out.push_str(&format!("{}if ({}) {{\n", inner_pad, emit_expr_cpp(guard)));
                    out.push_str(&format!("{}    return {};\n", inner_pad, emit_expr_cpp(&arm.body)));
                    out.push_str(&format!("{}}}\n", inner_pad));
                } else {
                    out.push_str(&format!("{}    return {};\n", inner_pad, emit_expr_cpp(&arm.body)));
                }
                out.push_str(&format!("{}}}\n", inner_pad));
            }
            out.push_str("    break;\n");
            out.push_str("}}\n})();");
            out
        }
        ExprKind::For { pattern, iter, body, .. } => {
            // C++ range-for
            let iter_expr = emit_expr_cpp(iter);
            let (pat_decl, pat_name) = cpp_pattern_binding(pattern);
            let mut out = format!("([&]() {{ for ({} {} : {}) {{\n", pat_decl, pat_name, iter_expr);
            out.push_str(&emit_block_cpp(body, 2));
            out.push_str("}}\n})();");
            out
        }
        ExprKind::While { cond, body, .. } => {
            let c = emit_expr_cpp(cond);
            let mut out = format!("([&]() {{ while({}) {{\n", c);
            out.push_str(&emit_block_cpp(body, 2));
            out.push_str("}}\n})();");
            out
        }
        ExprKind::WhileLet { pattern, expr: scrut, body, .. } => {
            let scrut_e = emit_expr_cpp(scrut);
            let (cond, binds) = cpp_let_condition(pattern, &scrut_e);
            let mut out = format!("([&]() {{ while({}) {{\n", cond);
            for b in &binds {
                out.push_str(&format!("    {};\n", b));
            }
            out.push_str(&emit_block_cpp(body, 2));
            out.push_str("}}\n})();");
            out
        }
        ExprKind::Loop { body, .. } => {
            let mut out = String::from("([&]() { while(true) {\n");
            out.push_str(&emit_block_cpp(body, 2));
            out.push_str("}\n})();");
            out
        }
        ExprKind::IfLet { pattern, expr: scrut, then, else_, .. } => {
            let scrut_e = emit_expr_cpp(scrut);
            let (cond, binds) = cpp_let_condition(pattern, &scrut_e);
            let mut out = format!("({} ? ([&]() {{ ", cond);
            for b in &binds {
                out.push_str(&format!("{}; ", b));
            }
            if let Some(tail) = &then.tail {
                out.push_str(&format!("return {}; ", emit_expr_cpp(tail)));
            }
            out.push_str("}()) : ");
            if let Some(els) = else_ {
                out.push_str(&format!("{})", emit_expr_cpp(els)));
            } else {
                out.push_str("void())");
            }
            out
        }
        ExprKind::Closure { params, body, .. } => {
            let ps: Vec<String> = params.iter().map(|p| {
                match &p.kind {
                    ParamKind::Pattern(pat) => match &pat.kind {
                        PatternKind::Ident { name, .. } => format!("auto {}", cpp_ident(&name.name)),
                        _ => "auto _arg".into(),
                    },
                    ParamKind::Self_(_) => "auto _self".into(),
                }
            }).collect();
            let body_expr = emit_expr_cpp(&body);
            format!("[&]({}) {{ {} }}", ps.join(", "), body_expr)
        }
        ExprKind::Return(value) => {
            if let Some(v) = value {
                format!("return {}", emit_expr_cpp(v))
            } else {
                "return".into()
            }
        }
        ExprKind::Break { label: _, value } => {
            if let Some(v) = value {
                format!("/* break with value not supported in C++ */ return {}", emit_expr_cpp(v))
            } else {
                "break".into()
            }
        }
        ExprKind::Continue { .. } => "continue".into(),
        ExprKind::Array(elements) => {
            // CTAD: std::vector{...} (C++17)
            let items: Vec<String> = elements.iter().map(emit_expr_cpp).collect();
            format!("std::vector{{{}}}", items.join(", "))
        }
        ExprKind::ArrayRepeat { elem, count } => {
            format!("std::vector({}, {})", emit_expr_cpp(elem), emit_expr_cpp(count))
        }
        ExprKind::Struct { fields, .. } => {
            let items: Vec<String> = fields.iter().map(|f| {
                match &f.name {
                    FieldIndex::Named(id) => {
                        let val = f.value.as_ref().map(emit_expr_cpp).unwrap_or_else(|| cpp_ident(&id.name).clone());
                        format!(".{} = {}", cpp_ident(&id.name), val)
                    }
                    FieldIndex::Index(_, _) => {
                        f.value.as_ref().map(emit_expr_cpp).unwrap_or_else(|| String::from("0"))
                    }
                }
            }).collect();
            format!("{{{}}}", items.join(", "))
        }
        ExprKind::Tuple(elems) => {
            let items: Vec<String> = elems.iter().map(emit_expr_cpp).collect();
            format!("std::make_tuple({})", items.join(", "))
        }
        ExprKind::Block(b) => {
            format!("([&]() {{ {} return {}; }})()", emit_block_cpp(b, 1),
                b.tail.as_ref().map(|t| emit_expr_cpp(t)).unwrap_or_else(|| "void".into()))
        }
        ExprKind::AsyncBlock { body, .. } => {
            format!("/* async block */ ([&]() {{ {} }})()", emit_block_cpp(body, 1))
        }
        ExprKind::Try(inner) => {
            format!("try {{ {} }} catch (...) {{ throw; }}", emit_expr_cpp(inner))
        }
        ExprKind::Await(inner) => {
            format!("/* await */ ({})", emit_expr_cpp(inner))
        }
        ExprKind::Cast { expr, .. } => emit_expr_cpp(expr),
        ExprKind::Macro { path, args, .. } => {
            let a: Vec<String> = args.tokens.iter().map(|t| format!("{:?}", t)).collect();
            let macro_name = path.segments.last().map(|s| s.name.as_str()).unwrap_or("");
            match macro_name {
                "println" => format!("std::cout << {} << std::endl", a.join(" << ")),
                "format" => format!("/* format! */ \"{}\"", a.join(", ")),
                "eprintln" => format!("std::cerr << {} << std::endl", a.join(" << ")),
                "dbg" => format!("/* dbg! */ ({})", a.join(", ")),
                other => format!("/* macro {} */ {}({})", other, other, a.join(", ")),
            }
        }
                // Fallback for unhandled expressions
        _ => format!("/* unhandled expr: {:?} */", expr.kind),
    }
}

// ────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────

fn cpp_literal(lit: &Literal) -> String {
    match &lit.kind {
        LiteralKind::Str { value, .. } => format!("\"{}\"", value),
        LiteralKind::Int { value, .. } => value.to_string(),
        LiteralKind::Float { value, suffix: _ } => value.to_string(),
        LiteralKind::Bool(b) => if *b { "true".into() } else { "false".into() },
        LiteralKind::Char(c) => format!("'{}'", c),
    }
}

fn cpp_binop(op: &BinaryOp, l: &str, r: &str) -> String {
    match op {
        BinaryOp::Add => format!("{} + {}", l, r),
        BinaryOp::Sub => format!("{} - {}", l, r),
        BinaryOp::Mul => format!("{} * {}", l, r),
        BinaryOp::Div => format!("{} / {}", l, r),
        BinaryOp::Rem => format!("{} % {}", l, r),
        BinaryOp::Eq => format!("{} == {}", l, r),
        BinaryOp::Ne => format!("{} != {}", l, r),
        BinaryOp::Lt => format!("{} < {}", l, r),
        BinaryOp::Gt => format!("{} > {}", l, r),
        BinaryOp::Le => format!("{} <= {}", l, r),
        BinaryOp::Ge => format!("{} >= {}", l, r),
        BinaryOp::And => format!("{} && {}", l, r),
        BinaryOp::Or => format!("{} || {}", l, r),
        BinaryOp::BitAnd => format!("{} & {}", l, r),
        BinaryOp::BitOr => format!("{} | {}", l, r),
        BinaryOp::BitXor => format!("{} ^ {}", l, r),
        BinaryOp::Shl => format!("{} << {}", l, r),
        BinaryOp::Shr => format!("{} >> {}", l, r),
                    }
}

/// C++ std method dispatch (aligns with dhv-ts body.ts cpp branches)
fn cpp_std_method(recv: &str, name: &str, args: &[String]) -> String {
    let a0 = || args.first().map(|s| s.as_str()).unwrap_or("");
    let a1 = || args.get(1).map(|s| s.as_str()).unwrap_or("");
    match name {
        // Vec methods
        "push" | "push_back" => format!("{}.push_back({})", recv, a0()),
        "pop" => format!("_dhvPop({})", recv),
        "len" | "size" => format!("{}.size()", recv),
        "is_empty" | "empty" => format!("{}.empty()", recv),
        "clear" => format!("{}.clear()", recv),
        "contains" => format!("(std::find({}.begin(), {}.end(), {}) != {}.end())", recv, recv, a0(), recv),
        "sort" => format!("std::sort({}.begin(), {}.end())", recv, recv),
        "reverse" => format!("std::reverse({}.begin(), {}.end())", recv, recv),
        "first" => format!("_dhvFirst({})", recv),
        "last" => format!("_dhvLast({})", recv),
        "clone" => format!("{}", recv),
        "extend" => format!("_dhvExtend({}, {})", recv, a0()),
        "remove_at" => format!("_dhvRemoveAt({}, {})", recv, a0()),
        // String methods
        "to_string" => format!("std::to_string({})", recv),
        "trim" => format!("_dhvTrim({})", recv),
        "to_lowercase" => format!("_dhvToLower({})", recv),
        "to_uppercase" => format!("_dhvToUpper({})", recv),
        "starts_with" => format!("{}.starts_with({})", recv, a0()),
        "ends_with" => format!("{}.ends_with({})", recv, a0()),
        "replace" => format!("_dhvReplaceAll({}, {}, {})", recv, a0(), a1()),
        "split" => format!("_dhvSplit({}, {})", recv, a0()),
        "join" => format!("_dhvJoin({}, {})", recv, a0()),
        "repeat" => format!("_dhvRepeat({}, {})", recv, a0()),
        "chars" | "char_count" => format!("_dhvCharCount({})", recv),
        "split_whitespace" => format!("_dhvSplitWS({})", recv),
        "lines" => format!("_dhvSplit({}, \"\\n\")", recv),
        "parse" => format!("_dhvParse<T>({})", recv),
        // Map methods
        "insert" => format!("{}[{}] = {}", recv, a0(), a1()),
        "remove" => format!("_dhvMapRemove({}, {})", recv, a0()),
        "contains_key" => format!("({}.find({}) != {}.end())", recv, a0(), recv),
        "keys" => format!("_dhvKeys({})", recv),
        "values" => format!("_dhvValues({})", recv),
        // Option methods
        "is_some" => format!("{}.has_value()", recv),
        "is_none" => format!("(!{}.has_value())", recv),
        "unwrap" => format!("*{}", recv),
        "unwrap_or" => format!("{}.value_or({})", recv, a0()),
        "expect" => format!("_dhvOptExpect({}, {})", recv, a0()),
        "map" => format!("_dhvOptMap({}, {})", recv, a0()),
        "and_then" => format!("_dhvOptAndThen({}, {})", recv, a0()),
        "or" => format!("_dhvOptOr({}, {})", recv, a0()),
        "unwrap_or_else" => format!("_dhvOptUnwrapOrElse({}, {})", recv, a0()),
        "filter" => format!("_dhvOptFilter({}, {})", recv, a0()),
        "ok_or" => format!("{}.value_or({})", recv, a0()),
        // Generic / fallback
        _ => format!("{}.{}({})", recv, name, args.join(", ")),
    }
}

/// Generate match arm condition from pattern + scrutinee variable
fn cpp_match_condition(pat: &Pattern, scrut: &str) -> String {
    match &pat.kind {
        PatternKind::Wildcard { .. } => "true".into(),
        PatternKind::Literal(lit) => {
            format!("{} == {}", scrut, cpp_literal(lit))
        }
        PatternKind::Ident { .. } => {
            String::from("true") // ident pattern always matches
        }
        PatternKind::Path(p) => {
            let path_name = p.segments.last().map(|s| s.name.clone()).unwrap_or_default();
            format!("{} == {}", scrut, cpp_ident(&path_name))
        }
        PatternKind::TupleStruct { path, .. } => {
            let vname = path.segments.last().map(|s| cpp_ident(&s.name)).unwrap_or_default();
            format!("std::holds_alternative<{}>({})", vname, scrut)
        }
        PatternKind::Struct { .. } => {
            "/* struct pattern */ true".into()
        }
        PatternKind::Tuple { .. } => {
            format!("/* tuple pattern */ true")
        }
        PatternKind::Or(pats) => {
            let conds: Vec<String> = pats.iter()
                .map(|p| cpp_match_condition(p, scrut))
                .collect();
            format!("({})", conds.join(" || "))
        }
        PatternKind::Range { .. } | PatternKind::Rest => "/* range/rest pattern */ true".into(),
    }
}

/// Generate (type, name) for a pattern binding in for-loop
fn cpp_pattern_binding(pat: &Pattern) -> (String, String) {
    match &pat.kind {
        PatternKind::Ident { name, .. } => ("const auto&".into(), cpp_ident(&name.name)),
        PatternKind::Wildcard { .. } => ("const auto&".into(), "_".into()),
        PatternKind::TupleStruct { path, .. } => {
            let vname = path.segments.last().map(|s| cpp_ident(&s.name)).unwrap_or_default();
            (format!("const {}&", vname), "_v".into())
        }
        _ => ("const auto&".into(), "_v".into()),
    }
}

/// Generate if-let condition and binding assignments
fn cpp_let_condition(pat: &Pattern, scrut: &str) -> (String, Vec<String>) {
    let mut binds = Vec::new();
    let cond = match &pat.kind {
        PatternKind::Ident { name, .. } => {
            binds.push(format!("auto& {} = {}", cpp_ident(&name.name), scrut));
            "true".into()
        }
        PatternKind::Wildcard { .. } => "true".into(),
        PatternKind::TupleStruct { path, .. } => {
            let vname = path.segments.last().map(|s| cpp_ident(&s.name)).unwrap_or_default();
            format!("std::holds_alternative<{}>({})", vname, scrut)
        }
        PatternKind::Path(p) => {
            let path_name = p.segments.last().map(|s| s.name.clone()).unwrap_or_default();
            format!("({} == {})", scrut, cpp_ident(&path_name))
        }
        PatternKind::Literal(lit) => {
            format!("({} == {})", scrut, cpp_literal(lit))
        }
        PatternKind::Or(pats) => {
            let conds: Vec<String> = pats.iter()
                .map(|p| {
                    let (c, b) = cpp_let_condition(p, scrut);
                    binds.extend(b);
                    c
                })
                .collect();
            format!("({})", conds.join(" || "))
        }
        _ => "true".into(),
    };
    (cond, binds)
}

/// Translate pattern for use in variable declarations
#[allow(dead_code)]
fn cpp_pattern(pat: &Pattern) -> String {
    match &pat.kind {
        PatternKind::Ident { name, .. } => cpp_ident(&name.name),
        PatternKind::Wildcard { .. } => "_".into(),
        PatternKind::Literal(lit) => cpp_literal(lit),
        PatternKind::Path(p) => {
            p.segments.last().map(|s| cpp_ident(&s.name)).unwrap_or_default()
        }
        PatternKind::Tuple { elems, .. } => {
            let items: Vec<String> = elems.iter().map(cpp_pattern).collect();
            format!("std::tie({})", items.join(", "))
        }
        PatternKind::TupleStruct { path, elems, .. } => {
            let vname = path.segments.last().map(|s| cpp_ident(&s.name)).unwrap_or_default();
            if elems.is_empty() {
                format!("{}{{}}", vname)
            } else if elems.len() == 1 {
                format!("{}{{ {} }}", vname, cpp_pattern(&elems[0]))
            } else {
                let items: Vec<String> = elems.iter().map(cpp_pattern).collect();
                format!("{}{{ {} }}", vname, items.join(", "))
            }
        }
        PatternKind::Struct { path, fields, .. } => {
            let sname = path.segments.last().map(|s| cpp_ident(&s.name)).unwrap_or_default();
            let items: Vec<String> = fields.iter().map(|f| {
                format!(".{} = {}", cpp_ident(&f.name.name), f.pattern.as_ref().map(|p| cpp_pattern(p)).unwrap_or_else(|| "_".into()))
            }).collect();
            format!("{}{{ {} }}", sname, items.join(", "))
        }
        PatternKind::Or(pats) => {
            /* or pattern in C++ — not directly supported */
            cpp_pattern(&pats[0])
        }
        PatternKind::Range { .. } | PatternKind::Rest => "/* range/rest pattern */ _".into(),
    }
}