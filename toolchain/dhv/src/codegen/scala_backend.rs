//! Scala 3 backend (Logic tier) -- type mapping + full function body translation
//! Scala 3 (Scala 3.3+) code generation.
//! struct -> case class (named) / case class (tuple/unit);
//! enum (unit) -> sealed abstract class + case object;
//! enum (data) -> sealed abstract class + case class;
//! trait -> trait (with default implementations);
//! impl -> class extends trait;
//! fn -> top-level def (Scala 3);
//! const -> val / lazy val;
//! graph -> @main def main()

use crate::ast::*;
use crate::codegen::{CodegenBackend, CodegenContext};

pub struct ScalaBackend;

impl CodegenBackend for ScalaBackend {
    fn lang(&self) -> &'static str {
        "scala"
    }

    fn emit_item(&self, ctx: &CodegenContext, item: &Item) -> Result<String, String> {
        let mut out = String::new();
        out.push_str(&format!("// {}\n", crate::sourcemap::generated_header("scala")));
        out.push_str("// HSL-generated Scala 3 code — do not edit manually\n\n");

        match item {
            Item::Struct(s) => out.push_str(&sc_struct(s)),
            Item::Enum(e) => out.push_str(&sc_enum(e)),
            Item::Trait(t) => out.push_str(&sc_trait(t)),
            Item::Fn(f) => out.push_str(&sc_fn(f)),
            Item::Graph(g) => out.push_str(&sc_graph(g, ctx)),
            Item::Impl(imp) => out.push_str(&sc_impl(imp)),
            Item::Const(c) => out.push_str(&sc_const(c)),
            Item::TypeAlias(a) => out.push_str(&sc_typealias(a)),
            Item::MacroRules(m) => out.push_str(&sc_macro_rules(m)),
            _ => {
                return Err(format!(
                    "scala backend does not support {}",
                    item_kind_name(item)
                ))
            }
        }
        Ok(out)
    }
}

// ──────────────────────────────────────────────────────
// Scala 3 关键字列表
// ───────0──────────────────────────────────────────────

const SC_KW: &[&str] = &[
    "abstract", "case", "class", "def", "do", "else", "extends", "false",
    "final", "for", "if", "implicit", "import", "lazy", "match", "new",
    "null", "object", "override", "package", "private", "protected", "return",
    "sealed", "super", "this", "throw", "trait", "true", "try", "type",
    "val", "var", "while", "with", "yield",
    // Scala 3 keywords
    "given", "using", "enum", "then", "end", "extension", "inline",
    "opaque", "open", "transparent", "export", "as", "derives",
];

fn sc_ident(s: &str) -> String {
    if SC_KW.contains(&s) {
        format!("{}$", s)
    } else {
        s.to_string()
    }
}

// ──────────────────────────────────────────────────────
// 类型映射
// ──────────────────────────────────────────────────────

fn sc_generic_arg(arg: &GenericArg) -> String {
    match arg {
        GenericArg::Type(t) => sc_type(t),
        GenericArg::Const(c) => match &c.kind {
            ConstArgKind::Literal(lit) => lit.raw.clone(),
            ConstArgKind::Block(_) => "0".into(),
        },
    }
}

fn sc_two_generic_args(args: &[GenericArg]) -> (String, String) {
    let mut it = args.iter();
    (
        it.next()
            .map(sc_generic_arg)
            .unwrap_or_else(|| "Any".into()),
        it.next()
            .map(sc_generic_arg)
            .unwrap_or_else(|| "Any".into()),
    )
}

fn sc_type(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(pt) => {
            let name = pt.path.last().name.as_str();
            match name {
                "String" | "str" => "String".into(),
                "char" => "Char".into(),
                "bool" => "Boolean".into(),
                "i8" | "u8" => "Byte".into(),
                "i16" | "u16" => "Short".into(),
                "i32" | "u32" | "usize" | "isize" => "Int".into(),
                "i64" | "u64" => "Long".into(),
                "i128" | "u128" => "BigInt".into(),
                "f32" => "Float".into(),
                "f64" => "Double".into(),
                "Vec" => format!(
                    "List[{}]",
                    pt.generic_args
                        .iter()
                        .next()
                        .map(sc_generic_arg)
                        .unwrap_or_else(|| "Any".into())
                ),
                "HashMap" | "BTreeMap" => {
                    let (k, v) = sc_two_generic_args(&pt.generic_args);
                    format!("Map[{}, {}]", k, v)
                }
                "HashSet" | "BTreeSet" => format!(
                    "Set[{}]",
                    pt.generic_args
                        .iter()
                        .next()
                        .map(sc_generic_arg)
                        .unwrap_or_else(|| "Any".into())
                ),
                "Option" => format!(
                    "Option[{}]",
                    pt.generic_args
                        .iter()
                        .next()
                        .map(sc_generic_arg)
                        .unwrap_or_else(|| "Any".into())
                ),
                "Result" => {
                    // Result<T, E> -> Either[E, T] (Scala Either is right-biased)
                    if pt.generic_args.len() >= 2 {
                        let t = sc_generic_arg(&pt.generic_args[0]);
                        let e = sc_generic_arg(&pt.generic_args[1]);
                        format!("Either[{}, {}]", e, t)
                    } else if !pt.generic_args.is_empty() {
                        sc_generic_arg(&pt.generic_args[0])
                    } else {
                        "Any".into()
                    }
                }
                "Box" => {
                    if !pt.generic_args.is_empty() {
                        sc_generic_arg(&pt.generic_args[0])
                    } else {
                        "Any".into()
                    }
                }
                _ => sc_ident(name),
            }
        }
        TypeKind::Ref { inner, .. } => sc_type(inner),
        TypeKind::Tuple(elems) => {
            if elems.is_empty() {
                "Unit".into()
            } else {
                let es: Vec<String> = elems.iter().map(sc_type).collect();
                format!("({})", es.join(", "))
            }
        }
        TypeKind::Array { elem, .. } => format!("Array[{}]", sc_type(elem)),
        TypeKind::Slice(inner) => format!("List[{}]", sc_type(inner)),
        TypeKind::Paren(inner) => sc_type(inner),
        TypeKind::Never => "Nothing".into(),
        TypeKind::FnPtr { params, ret } => {
            let r = ret
                .as_ref()
                .map(|t| sc_type(t))
                .unwrap_or_else(|| "Unit".into());
            if params.is_empty() {
                format!("() => {}", r)
            } else {
                format!(
                    "({}) => {}",
                    params.iter().map(sc_type).collect::<Vec<_>>().join(", "),
                    r
                )
            }
        }
        TypeKind::DynTrait(_) | TypeKind::ImplTrait(_) | TypeKind::Infer => "Any".into(),
    }
}

// ──────────────────────────────────────────────────────
// 标准库方法映射
// ──────────────────────────────────────────────────────

/// Map Rust/Vec std method names to Scala equivalents
fn sc_std_method(receiver: &str, method: &str, args: &[Expr]) -> Option<String> {
    let args_str: Vec<String> = args.iter().map(|a| emit_expr_sc(a, 0)).collect();
    match method {
        // Vec/List methods
        "push" | "append" => {
            if args_str.len() == 1 {
                Some(format!("{} :+= {}", receiver, args_str[0]))
            } else {
                None
            }
        }
        "pop" => Some(format!("{} = {}.init", receiver, receiver)),
        "len" | "length" => Some(format!("{}.length", receiver)),
        "is_empty" => Some(format!("{}.isEmpty", receiver)),
        "sort" => Some(format!("{} = {}.sorted", receiver, receiver)),
        "sorted" => Some(format!("{}.sorted", receiver)),
        "reverse" => Some(format!("{}.reverse", receiver)),
        "map" => {
            if args_str.len() == 1 {
                Some(format!("{}.map({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "filter" => {
            if args_str.len() == 1 {
                Some(format!("{}.filter({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "fold" => {
            if args_str.len() == 2 {
                Some(format!("{}.foldLeft({})({})", receiver, args_str[0], args_str[1]))
            } else {
                None
            }
        }
        "for_each" | "foreach" => {
            if args_str.len() == 1 {
                Some(format!("{}.foreach({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "find" => {
            if args_str.len() == 1 {
                Some(format!("{}.find({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "any" | "exists" => {
            if args_str.len() == 1 {
                Some(format!("{}.exists({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "all" | "forall" => {
            if args_str.len() == 1 {
                Some(format!("{}.forall({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "flat_map" | "flatMap" => {
            if args_str.len() == 1 {
                Some(format!("{}.flatMap({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "collect" => Some(format!("{}.flatten", receiver)),
        "contains" => {
            if args_str.len() == 1 {
                Some(format!("{}.contains({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        // String methods
        "to_string" | "toString" => Some(format!("{}.toString", receiver)),
        "trim" => Some(format!("{}.trim", receiver)),
        "to_lowercase" | "toLowerCase" => Some(format!("{}.toLowerCase", receiver)),
        "to_uppercase" | "toUpperCase" => Some(format!("{}.toUpperCase", receiver)),
        "starts_with" | "startsWith" => {
            if args_str.len() == 1 {
                Some(format!("{}.startsWith({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "ends_with" | "endsWith" => {
            if args_str.len() == 1 {
                Some(format!("{}.endsWith({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "split" => {
            if args_str.len() == 1 {
                Some(format!("{}.split({}).toArray", receiver, args_str[0]))
            } else {
                None
            }
        }
        "replace" => {
            if args_str.len() == 2 {
                Some(format!("{}.replace({}, {})", receiver, args_str[0], args_str[1]))
            } else {
                None
            }
        }
        "chars" | "toCharArray" => Some(format!("{}.toCharArray", receiver)),
        // Option methods
        "is_some" => Some(format!("{}.isDefined", receiver)),
        "is_none" => Some(format!("{}.isEmpty", receiver)),
        "unwrap" | "get" => Some(format!("{}.get", receiver)),
        "expect" => {
            if args_str.len() == 1 {
                Some(format!("{}.get.orElse(throw new RuntimeException({}))", receiver, args_str[0]))
            } else {
                Some(format!("{}.get", receiver))
            }
        }
        "and_then" => {
            if args_str.len() == 1 {
                Some(format!("{}.flatMap({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "unwrap_or" | "getOrElse" => {
            if args_str.len() == 1 {
                Some(format!("{}.getOrElse({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        // Result/Either methods
        "is_ok" | "isRight" => Some(format!("{}.isRight", receiver)),
        "is_err" | "isLeft" => Some(format!("{}.isLeft", receiver)),
        "ok" | "toOption" => Some(format!("{}.toOption", receiver)),
        // Map methods
        "insert" | "updated" => {
            if args_str.len() == 2 {
                Some(format!("{}.updated({}, {})", receiver, args_str[0], args_str[1]))
            } else {
                None
            }
        }
        "keys" => Some(format!("{}.keys", receiver)),
        "values" => Some(format!("{}.values", receiver)),
        _ => None,
    }
}

// ──────────────────────────────────────────────────────
// 参数
// ──────────────────────────────────────────────────────

fn sc_param(p: &Param) -> Option<String> {
    match &p.kind {
        ParamKind::Self_(_) => None,
        ParamKind::Pattern(pat) => {
            let name = match &pat.kind {
                PatternKind::Ident { name, .. } => sc_ident(&name.name),
                _ => "_arg".into(),
            };
            Some(format!("{}: {}", name, sc_type(&p.ty)))
        }
    }
}

// ──────────────────────────────────────────────────────
// 项转译
// ──────────────────────────────────────────────────────

fn sc_struct(s: &StructDef) -> String {
    let name = sc_ident(&s.name.name);
    match &s.kind {
        StructKind::Named(fields) => {
            let fs: Vec<String> = fields
                .iter()
                .map(|f| {
                    let fn_ = f
                        .name
                        .as_ref()
                        .map(|n| sc_ident(&n.name))
                        .unwrap_or_else(|| "_".into());
                    format!("  val {}: {}", fn_, sc_type(&f.ty))
                })
                .collect();
            format!("case class {}(\n{}\n)\n\n", name, fs.join(",\n"))
        }
        StructKind::Tuple(fields) => {
            let fs: Vec<String> = fields
                .iter()
                .enumerate()
                .map(|(i, f)| format!("  _{}: {}", i + 1, sc_type(&f.ty)))
                .collect();
            format!("case class {}(\n{}\n)\n\n", name, fs.join(",\n"))
        }
        StructKind::Unit => format!("case class {}()\n\n", name),
    }
}

fn sc_enum(e: &EnumDef) -> String {
    let name = sc_ident(&e.name.name);
    let has_data = e
        .variants
        .iter()
        .any(|v| !matches!(&v.fields, StructKind::Unit));
    if !has_data {
        // 简单枚举 → sealed abstract class + case object
        let mut o = format!("sealed abstract class {}

", name);
        for v in &e.variants {
            let vn = sc_ident(&v.name.name);
            o.push_str(&format!("case object {} extends {}\n\n", vn, name));
        }
        o
    } else {
        // 带数据的枚举 → sealed abstract class + case class
        let mut o = format!("sealed abstract class {}

", name);
        for v in &e.variants {
            let vn = sc_ident(&v.name.name);
            match &v.fields {
                StructKind::Unit => {
                    o.push_str(&format!(
                        "case object {} extends {}\n\n",
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
                                .map(|n| sc_ident(&n.name))
                                .unwrap_or_else(|| "_".into());
                            format!("  val {}: {}", fn_, sc_type(&f.ty))
                        })
                        .collect();
                    o.push_str(&format!(
                        "case class {}(\n{}\n) extends {}\n\n",
                        vn,
                        fs.join(",\n"),
                        name
                    ));
                }
                StructKind::Tuple(fields) => {
                    let fs: Vec<String> = fields
                        .iter()
                        .enumerate()
                        .map(|(i, f)| format!("  _{}: {}", i + 1, sc_type(&f.ty)))
                        .collect();
                    o.push_str(&format!(
                        "case class {}(\n{}\n) extends {}\n\n",
                        vn,
                        fs.join(",\n"),
                        name
                    ));
                }
            }
        }
        o
    }
}

fn sc_trait(t: &TraitDef) -> String {
    let name = sc_ident(&t.name.name);
    let mut o = format!("trait {} {{\n", name);
    for ti in &t.items {
        match ti {
            TraitItem::FnSig(sig) => {
                let ps: Vec<String> = sig.params.iter().filter_map(sc_param).collect();
                let r = sig
                    .ret
                    .as_ref()
                    .map(|t| format!(": {}", sc_type(t)))
                    .unwrap_or_default();
                o.push_str(&format!(
                    "  def {}({}){}\n",
                    sc_ident(&sig.name.name),
                    ps.join(", "),
                    r
                ));
            }
            TraitItem::Fn(f) => {
                let ps: Vec<String> = f.params.iter().filter_map(sc_param).collect();
                let r = f
                    .ret
                    .as_ref()
                    .map(|t| format!(": {}", sc_type(t)))
                    .unwrap_or_default();
                o.push_str(&format!(
                    "  def {}({}){} = {{\n",
                    sc_ident(&f.name.name),
                    ps.join(", "),
                    r
                ));
                if let Some(body) = &f.body {
                    o.push_str(&emit_block_sc(body, 2));
                }
                o.push_str("  }\n");
            }
            TraitItem::Const(_) | TraitItem::TypeAlias(_) => {}
        }
    }
    o.push_str("}\n\n");
    o
}

fn sc_fn(f: &FnDef) -> String {
    let name = sc_ident(&f.name.name);
    let ret = f
        .ret
        .as_ref()
        .map(|t| format!(": {}", sc_type(t)))
        .unwrap_or_default();
    let ps: Vec<String> = f.params.iter().filter_map(sc_param).collect();
    let mut o = String::new();
    if f.is_async {
        o.push_str("// async ");
    }
    o.push_str(&format!(
        "def {}({}){} = {{\n",
        name,
        ps.join(", "),
        ret
    ));
    if let Some(body) = &f.body {
        o.push_str(&emit_block_sc(body, 1));
    }
    o.push_str("}\n\n");
    o
}

fn sc_graph(g: &GraphDef, ctx: &CodegenContext) -> String {
    let _gn = sc_ident(&g.name.name);
    let mut o = format!(
        "// graph {} -- scale: {:?}\n",
        g.name.name, ctx.scale
    );
    o.push_str("@main def main(): Unit = {\n");
    for gs in &g.body {
        match gs {
            GraphStmt::Node(n) => o.push_str(&format!(
                "  // node {}: {}\n",
                sc_ident(&n.name.name),
                sc_type(&n.ty)
            )),
            GraphStmt::Edge(e) => {
                let ep: Vec<String> = e
                    .endpoints
                    .iter()
                    .map(|p| p.last().name.clone())
                    .collect();
                o.push_str(&format!(
                    "  // edge: {}\n",
                    ep.join(" -> ")
                ));
            }
            GraphStmt::Let(l) => o.push_str(&format!(
                "  {}",
                emit_let_sc(l, 0)
            )),
            GraphStmt::Stmt(s) => o.push_str(&emit_stmt_sc(s, 1)),
            GraphStmt::Item(_) => {}
        }
    }
    o.push_str("}\n\n");
    o
}

fn sc_impl(imp: &ImplDef) -> String {
    let tn = imp
        .trait_ty
        .as_ref()
        .map(|t| sc_type(t))
        .unwrap_or_default();
    let sn = sc_type(&imp.self_ty);
    let mut o = if !tn.is_empty() {
        format!("class {} extends {} {{\n", sn, tn)
    } else {
        format!("class {} {{\n", sn)
    };
    for item in &imp.items {
        match item {
            ImplItem::Fn(f) => {
                let fn_ = sc_ident(&f.name.name);
                let r = f
                    .ret
                    .as_ref()
                    .map(|t| format!(": {}", sc_type(t)))
                    .unwrap_or_default();
                let ps: Vec<String> = f.params.iter().filter_map(sc_param).collect();
                o.push_str(&format!(
                    "  def {}({}){} = {{\n",
                    fn_,
                    ps.join(", "),
                    r
                ));
                if let Some(body) = &f.body {
                    o.push_str(&emit_block_sc(body, 2));
                }
                o.push_str("  }\n");
            }
            ImplItem::Const(c) => {
                o.push_str(&format!(
                    "  val {} = {}\n",
                    sc_ident(&c.name.name),
                    emit_expr_sc(&c.value, 0)
                ));
            }
            ImplItem::TypeAlias(_) => {}
        }
    }
    o.push_str("}\n\n");
    o
}

fn sc_const(c: &ConstDef) -> String {
    format!(
        "val {}: {} = {}\n\n",
        sc_ident(&c.name.name),
        sc_type(&c.ty),
        emit_expr_sc(&c.value, 0)
    )
}

fn sc_typealias(a: &TypeAliasDef) -> String {
    format!(
        "// Type alias: {} = {}\n",
        sc_ident(&a.name.name),
        sc_type(&a.ty)
    )
}

fn sc_macro_rules(m: &MacroRulesDefinition) -> String {
    format!(
        "// macro_rules {} -- not directly translatable to Scala\n\n",
        sc_ident(&m.name.name)
    )
}

// ──────────────────────────────────────────────────────
// 表达式转译
// ──────────────────────────────────────────────────────

pub fn emit_expr_sc(expr: &Expr, indent: usize) -> String {
    let ind = "  ".repeat(indent);
    match &expr.kind {
        ExprKind::Literal(lit) => sc_literal(lit),
        ExprKind::Path(p) => sc_ident(&p.last().name),
        ExprKind::Binary { op, lhs, rhs } => format!(
            "({} {} {})",
            emit_expr_sc(lhs, 0),
            sc_binop(op),
            emit_expr_sc(rhs, 0)
        ),
        ExprKind::Unary { op, operand } => format!(
            "{}{}",
            sc_unop(op),
            emit_expr_sc(operand, 0)
        ),
        ExprKind::Call { callee, args } => {
            // Check for println! / format! macros (represented as Path calls)
            let callee_str = if let ExprKind::Path(p) = &callee.kind {
                Some(p.last().name.as_str())
            } else {
                None
            };
            match callee_str {
                Some("println") => {
                    if args.is_empty() {
                        "println()".into()
                    } else if args.len() == 1 {
                        if let ExprKind::Literal(lit) = &args[0].kind {
                            if let LiteralKind::Str { value: _, .. } = &lit.kind {
                                // println!("text") -> println("text")
                                return format!("println({})", sc_literal(lit));
                            }
                        }
                        format!("println({})", emit_expr_sc(&args[0], 0))
                    } else {
                        // Multiple args: use string interpolation
                        format!("println({})", args.iter().map(|a| emit_expr_sc(a, 0)).collect::<Vec<_>>().join(" + "))
                    }
                }
                Some("eprintln") => {
                    if args.is_empty() {
                        "System.err.println()".into()
                    } else {
                        format!("System.err.println({})", args.iter().map(|a| emit_expr_sc(a, 0)).collect::<Vec<_>>().join(" + "))
                    }
                }
                Some("format") => {
                    // format!("hello {}", x) -> s"hello $x"
                    if !args.is_empty() {
                        if let ExprKind::Literal(lit) = &args[0].kind {
                            if let LiteralKind::Str { value, .. } = &lit.kind {
                                return sc_format_string(value, &args[1..]);
                            }
                        }
                    }
                    let as_ = args.iter().map(|a| emit_expr_sc(a, 0)).collect::<Vec<_>>();
                    format!("{}", as_.join(" + "))
                }
                Some("vec") => {
                    let as_ = args.iter().map(|a| emit_expr_sc(a, 0)).collect::<Vec<_>>();
                    format!("List({})", as_.join(", "))
                }
                Some("panic") => {
                    if !args.is_empty() {
                        format!("throw new RuntimeException({})", emit_expr_sc(&args[0], 0))
                    } else {
                        "throw new RuntimeException()".into()
                    }
                }
                Some("todo") | Some("unimplemented") => {
                    "???".into()
                }
                _ => {
                    let as_ = args.iter().map(|a| emit_expr_sc(a, 0)).collect::<Vec<_>>();
                    format!("{}({})", emit_expr_sc(callee, 0), as_.join(", "))
                }
            }
        }
        ExprKind::MethodCall { receiver, method, generic_args: _, args } => {
            let mn = &method.name;
            let recv_str = emit_expr_sc(receiver, 0);
            // Check std method mapping
            if let Some(mapped) = sc_std_method(&recv_str, mn, args) {
                return mapped;
            }
            let mn_esc = sc_ident(mn);
            let as_ = args.iter().map(|a| emit_expr_sc(a, 0)).collect::<Vec<_>>();
            format!("{}.{}({})", recv_str, mn_esc, as_.join(", "))
        }
        ExprKind::Field { base, field } => {
            let fn_ = match field {
                FieldIndex::Named(id) => sc_ident(&id.name),
                FieldIndex::Index(i, _) => format!("_{}", i + 1),
            };
            // Scala case class field access with potential keyword conflict
            let recv = emit_expr_sc(base, 0);
            // If field name starts with underscore (tuple), use productElement
            if fn_.starts_with('_') && fn_.len() > 1 {
                let idx: u32 = fn_[1..].parse().unwrap_or(1);
                format!("{}.productElement({})", recv, idx - 1)
            } else {
                format!("{}.{}", recv, fn_)
            }
        }
        ExprKind::Index { base, index } => {
            format!("{}({})", emit_expr_sc(base, 0), emit_expr_sc(index, 0))
        }
        ExprKind::Slice { base, range } => {
            let bs = emit_expr_sc(base, 0);
            let s = range
                .lo
                .as_ref()
                .map(|e| emit_expr_sc(e, 0))
                .unwrap_or_else(|| "0".into());
            let e = range
                .hi
                .as_ref()
                .map(|e| emit_expr_sc(e, 0))
                .unwrap_or_else(|| format!("{}.length", bs));
            if range.inclusive {
                format!("{}.slice({}, {} + 1)", bs, s, e)
            } else {
                format!("{}.slice({}, {})", bs, s, e)
            }
        }
        ExprKind::Range(r) => {
            let l = r
                .lo
                .as_ref()
                .map(|e| emit_expr_sc(e, 0))
                .unwrap_or_else(|| "0".into());
            let hi = r
                .hi
                .as_ref()
                .map(|e| emit_expr_sc(e, 0))
                .unwrap_or_else(|| "???".into());
            if r.inclusive {
                format!("{} to {}", l, hi)
            } else {
                format!("{} until {}", l, hi)
            }
        }
        ExprKind::Assign { lhs, rhs } => {
            format!("{} = {}", emit_expr_sc(lhs, 0), emit_expr_sc(rhs, 0))
        }
        ExprKind::CompoundAssign { op, lhs, rhs } => {
            format!(
                "{} {}= {}",
                emit_expr_sc(lhs, 0),
                sc_binop(op).trim(),
                emit_expr_sc(rhs, 0)
            )
        }
        ExprKind::If { cond, then, else_ } => {
            let mut o = format!("if ({}) {{\n", emit_expr_sc(cond, 0));
            o.push_str(&emit_block_sc(then, indent + 1));
            if let Some(els) = else_ {
                o.push_str(&format!("{}}} else ", ind));
                match &els.kind {
                    ExprKind::If { .. } => {
                        // else if chain
                        o.push_str(&emit_expr_sc(els, indent));
                    }
                    ExprKind::Block(b) => {
                        o.push_str("{\n");
                        o.push_str(&emit_block_sc(b, indent + 1));
                        o.push_str(&format!("{}}}\n", ind));
                    }
                    _ => {
                        o.push_str(&emit_expr_sc(els, 0));
                        o.push('\n');
                    }
                }
            } else {
                o.push_str(&format!("{}}}\n", ind));
            }
            o
        }
        ExprKind::Match { scrutinee, arms } => {
            let mut o = format!("{} match {{\n", emit_expr_sc(scrutinee, 0));
            for arm in arms {
                let p = sc_pattern(&arm.pattern);
                let g = if let Some(g) = &arm.guard {
                    format!(" if ({})", emit_expr_sc(g, 0))
                } else {
                    String::new()
                };
                match &arm.body.kind {
                    ExprKind::Block(b) => {
                        o.push_str(&format!(
                            "{}  case {}{} =>\n",
                            ind, p, g
                        ));
                        o.push_str(&emit_block_sc(b, indent + 2));
                    }
                    _ => {
                        o.push_str(&format!(
                            "{}  case {}{} => {}\n",
                            ind,
                            p,
                            g,
                            emit_expr_sc(&arm.body, 0)
                        ));
                    }
                }
            }
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::For { label: _, pattern, iter, body } => {
            let mut o = format!(
                "for ({} <- {}) {{\n",
                sc_pattern(pattern),
                emit_expr_sc(iter, 0)
            );
            o.push_str(&emit_block_sc(body, indent + 1));
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::While { label: _, cond, body } => {
            let mut o = format!("while ({}) {{\n", emit_expr_sc(cond, 0));
            o.push_str(&emit_block_sc(body, indent + 1));
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::WhileLet { label: _, pattern, expr: scrut, body } => {
            // while let pat = expr → while (expr match { case pat => true; case _ => false })
            let mut o = format!(
                "while ({} match {{ case {} => true; case _ => false }}) {{\n",
                emit_expr_sc(scrut, 0),
                sc_pattern(pattern)
            );
            o.push_str(&emit_block_sc(body, indent + 1));
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::Loop { label, body } => {
            let mut o = String::new();
            if let Some(l) = label {
                o.push_str(&format!("// label: {}\n", sc_ident(&l.name)));
            }
            o.push_str("while (true) {\n");
            o.push_str(&emit_block_sc(body, indent + 1));
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::Closure { params, body, ret: _, .. } => {
            let ps: Vec<String> = params
                .iter()
                .filter_map(|p| match &p.kind {
                    ParamKind::Pattern(pat) => match &pat.kind {
                        PatternKind::Ident { name, .. } => Some(sc_ident(&name.name)),
                        _ => Some("x".into()),
                    },
                    _ => None,
                })
                .collect();
            if let ExprKind::Block(be) = &body.kind {
                if be.stmts.is_empty() {
                    if let Some(tail) = &be.tail {
                        return format!(
                            "({} => {})",
                            ps.join(", "),
                            emit_expr_sc(tail, 0)
                        );
                    }
                }
                let mut o = format!("({} =>\n", ps.join(", "));
                o.push_str(&emit_block_sc(be, indent + 1));
                o.push_str(&format!("{} )\n", ind));
                o
            } else {
                format!(
                    "({} => {})",
                    ps.join(", "),
                    emit_expr_sc(body, 0)
                )
            }
        }
        ExprKind::Return(value) => {
            if let Some(v) = value {
                // Scala 3: return is non-local, prefer expression context
                format!("return {}", emit_expr_sc(v, 0))
            } else {
                "return".into()
            }
        }
        ExprKind::Break { label, value } => {
            let mut o = "break".to_string();
            if let Some(l) = label {
                o = format!("// break from {}", sc_ident(&l.name));
            }
            if let Some(v) = value {
                o.push_str(&format!(" /* with value: {} */", emit_expr_sc(v, 0)));
            }
            o
        }
        ExprKind::Continue { label } => {
            if let Some(l) = label {
                format!("// continue {}", sc_ident(&l.name))
            } else {
                "// continue".into()
            }
        }
        ExprKind::Array(elems) => {
            let es: Vec<String> = elems.iter().map(|e| emit_expr_sc(e, 0)).collect();
            if es.is_empty() {
                "List.empty".into()
            } else {
                format!("List({})", es.join(", "))
            }
        }
        ExprKind::ArrayRepeat { elem, count } => {
            format!(
                "List.fill({})({})",
                emit_expr_sc(count, 0),
                emit_expr_sc(elem, 0)
            )
        }
        ExprKind::Struct { path, fields, spread } => {
            let name = sc_ident(&path.last().name);
            let fs: Vec<String> = fields
                .iter()
                .map(|f| {
                    let fn_ = match &f.name {
                        FieldIndex::Named(id) => sc_ident(&id.name),
                        FieldIndex::Index(i, _) => format!("_{}", i + 1),
                    };
                    let v = f
                        .value
                        .as_ref()
                        .map(|v| emit_expr_sc(v, 0))
                        .unwrap_or_else(|| fn_.clone());
                    format!("{} = {}", fn_, v)
                })
                .collect();
            let spread_str = if let Some(spread) = spread {
                format!(", /* ..{} */", emit_expr_sc(spread, 0))
            } else {
                String::new()
            };
            format!("{}({}{}{})", name, fs.join(", "), if fs.is_empty() { "" } else { "" }, spread_str)
        }
        ExprKind::Tuple(elems) => {
            let es: Vec<String> = elems.iter().map(|e| emit_expr_sc(e, 0)).collect();
            match es.len() {
                0 => "()".into(),
                _ => format!("({})", es.join(", ")),
            }
        }
        ExprKind::Block(be) => {
            let mut o = "{\n".to_string();
            o.push_str(&emit_block_sc(be, indent + 1));
            if let Some(tail) = &be.tail {
                o.push_str(&format!(
                    "{}{}\n",
                    ind,
                    emit_expr_sc(tail, 0)
                ));
            }
            o.push_str(&format!("{}}}", ind));
            o
        }
        ExprKind::AsyncBlock { body, .. } => {
            let mut o = "// async ".to_string();
            o.push_str("{\n");
            o.push_str(&emit_block_sc(body, indent + 1));
            if let Some(tail) = &body.tail {
                o.push_str(&format!(
                    "{}{}\n",
                    ind,
                    emit_expr_sc(tail, 0)
                ));
            }
            o.push_str(&format!("{}}}", ind));
            o
        }
        ExprKind::Try(inner) => {
            // Scala: try/catch - wrap in Try for error propagation
            let inner_str = emit_expr_sc(inner, 0);
            format!(
                "scala.util.Try({}).toOption",
                inner_str
            )
        }
        ExprKind::Await(inner) => {
            // Scala: Await.result for Future
            format!("scala.concurrent.Await.result({}, scala.concurrent.duration.Duration.Inf)", emit_expr_sc(inner, 0))
        }
        ExprKind::Cast { expr: inner, ty } => {
            format!("{}.asInstanceOf[{}]", emit_expr_sc(inner, 0), sc_type(ty))
        }
        ExprKind::IfLet { pattern, expr: scrut, then, else_ } => {
            // if let pat = expr → expr match { case pat => ... } or scala.util.Using
            let mut o = format!(
                "{} match {{\n  case {} =>\n",
                emit_expr_sc(scrut, 0),
                sc_pattern(pattern)
            );
            o.push_str(&emit_block_sc(then, indent + 2));
            if let Some(els) = else_ {
                o.push_str(&format!(
                    "{}  case _ =>\n",
                    ind
                ));
                match &els.kind {
                    ExprKind::Block(b) => {
                        o.push_str(&emit_block_sc(b, indent + 2));
                    }
                    _ => {
                        o.push_str(&format!(
                            "{}    {}\n",
                            ind,
                            emit_expr_sc(els, 0)
                        ));
                    }
                }
            }
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::Macro { path, args: _ } => sc_macro(path.last().name.as_str()),
        ExprKind::Native(nb) => {
            // native scala block: emit as-is
            if nb.lang.name == "scala" {
                nb.code.clone()
            } else {
                "// native block".into()
            }
        }
    }
}

fn sc_literal(lit: &Literal) -> String {
    match &lit.kind {
        LiteralKind::Str { value, .. } => {
            format!(
                "\"{}\"",
                value.replace('\\', "\\\\").replace('"', "\\\"").replace('$', "\\$")
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
            } else if *value > i32::MAX as i128 || *value < i32::MIN as i128 {
                format!("{}L", s)
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

/// Convert Rust format! string to Scala s"..." interpolation
fn sc_format_string(fmt: &str, args: &[Expr]) -> String {
    let mut result = String::from("s\"");
    let mut arg_idx = 0;
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' && chars.peek() == Some(&'{') {
            chars.next(); // consume second '{'
            result.push_str("${");
            if arg_idx < args.len() {
                result.push_str(&emit_expr_sc(&args[arg_idx], 0));
                arg_idx += 1;
            }
            // consume until '}}'
            while let Some(nc) = chars.next() {
                if nc == '}' && chars.peek() == Some(&'}') {
                    chars.next(); // consume second '}'
                    break;
                }
            }
            result.push('}');
        } else if c == '}' && chars.peek() == Some(&'}') {
            chars.next();
            result.push_str("}");
        } else if c == '}' {
            result.push_str("}");
        } else {
            result.push(c);
        }
    }
    result.push('"');
    result
}

fn sc_binop(op: &BinaryOp) -> &'static str {
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

fn sc_unop(op: &UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Not => "!",
        UnaryOp::Deref | UnaryOp::Ref | UnaryOp::RefMut => "",
    }
}

fn sc_pattern(pat: &Pattern) -> String {
    match &pat.kind {
        PatternKind::Wildcard => "_".into(),
        PatternKind::Ident { name, .. } => sc_ident(&name.name),
        PatternKind::Literal(lit) => sc_literal(lit),
        PatternKind::Path(p) => sc_ident(&p.last().name),
        PatternKind::TupleStruct { path, elems, .. } => {
            let n = sc_ident(&path.last().name);
            let es: Vec<String> = elems.iter().map(sc_pattern).collect();
            if es.is_empty() {
                n
            } else {
                format!("{}({})", n, es.join(", "))
            }
        }
        PatternKind::Struct { path, fields, .. } => {
            let n = sc_ident(&path.last().name);
            let fs: Vec<String> = fields
                .iter()
                .map(|f| {
                    let fn_ = sc_ident(&f.name.name);
                    let p = f
                        .pattern
                        .as_ref()
                        .map(|p| sc_pattern(p))
                        .unwrap_or_else(|| sc_ident(&f.name.name));
                    format!("{} = {}", fn_, p)
                })
                .collect();
            if fs.is_empty() {
                format!("{}(_*)", n)
            } else {
                format!("{}({}, _*)", n, fs.join(", "))
            }
        }
        PatternKind::Tuple { elems, .. } => {
            let es: Vec<String> = elems.iter().map(sc_pattern).collect();
            format!("({})", es.join(", "))
        }
        PatternKind::Or(elems) => elems
            .iter()
            .map(|e| sc_pattern(e))
            .collect::<Vec<_>>()
            .join(" | "),
        PatternKind::Range { lo, hi, inclusive } => {
            let l = sc_pattern(lo);
            let r = sc_pattern(hi);
            if *inclusive {
                format!("{} to {}", l, r)
            } else {
                format!("{} until {}", l, r)
            }
        }
        PatternKind::Rest => "_*".into(),
    }
}

fn sc_macro(name: &str) -> String {
    match name {
        "println" => "println".into(),
        "eprintln" => "System.err.println".into(),
        "format" => "String.format".into(),
        "todo" | "unimplemented" => "???".into(),
        "panic" => "throw new RuntimeException()".into(),
        "vec" => "List".into(),
        _ => format!("/* macro: {} */", name),
    }
}

// ──────────────────────────────────────────────────────
// 语句块转译
// ──────────────────────────────────────────────────────

pub fn emit_block_sc(be: &BlockExpr, indent: usize) -> String {
    let ind = "  ".repeat(indent);
    let mut o = String::new();
    for stmt in &be.stmts {
        match stmt {
            Stmt::Let(l) => o.push_str(&format!("{}{}", ind, emit_let_sc(l, 0))),
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
                        o.push_str(&format!("{}\n", emit_expr_sc(expr, indent)));
                    }
                    ExprKind::Return(_) => {
                        o.push_str(&format!(
                            "{}{}\n",
                            ind,
                            emit_expr_sc(expr, 0)
                        ));
                    }
                    _ => {
                        o.push_str(&format!(
                            "{}{}\n",
                            ind,
                            emit_expr_sc(expr, 0)
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
                o.push_str(&format!("{}", emit_expr_sc(tail, indent)));
            }
            _ => {
                o.push_str(&format!(
                    "{}{}\n",
                    ind,
                    emit_expr_sc(tail, 0)
                ));
            }
        }
    }
    o
}

fn emit_let_sc(l: &LetStmt, _indent: usize) -> String {
    let pat = sc_pattern(&l.pattern);
    let kw = if l.mutable { "var" } else { "val" };
    if let Some(ty) = &l.ty {
        format!(
            "{} {}: {} = {}\n",
            kw,
            pat,
            sc_type(ty),
            l.init
                .as_ref()
                .map(|e| emit_expr_sc(e, 0))
                .unwrap_or_else(|| "???".into())
        )
    } else {
        format!(
            "{} {} = {}\n",
            kw,
            pat,
            l.init
                .as_ref()
                .map(|e| emit_expr_sc(e, 0))
                .unwrap_or_else(|| "???".into())
        )
    }
}

pub fn emit_stmt_sc(stmt: &Stmt, indent: usize) -> String {
    match stmt {
        Stmt::Let(l) => {
            let ind = "  ".repeat(indent);
            format!("{}{}", ind, emit_let_sc(l, 0))
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
                | ExprKind::IfLet { .. } => emit_expr_sc(expr, indent) + "\n",
                _ => {
                    let ind = "  ".repeat(indent);
                    format!("{}{}\n", ind, emit_expr_sc(expr, 0))
                }
            }
        }
    }
}
