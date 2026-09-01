// Dart 3 backend (Logic tier) -- type mapping + full function body translation
// Dart 3 (Dart 3.x) code generation.
// struct -> class (named, with constructor) / class (tuple/unit);
// enum (unit) -> sealed class + static const fields;
// enum (data) -> sealed class + subclass with factory constructor;
// trait -> abstract class (with default method implementations);
// impl -> class extends abstract class;
// fn -> top-level function;
// const -> const / final;
// graph -> void main() { }

use crate::ast::*;
use crate::codegen::{CodegenBackend, CodegenContext};

pub struct DartBackend;

impl CodegenBackend for DartBackend {
    fn lang(&self) -> &'static str {
        "dart"
    }

    fn emit_item(&self, ctx: &CodegenContext, item: &Item) -> Result<String, String> {
        let mut out = String::new();
        out.push_str(&format!(
            "// {}\n",
            crate::sourcemap::generated_header("dart")
        ));
        out.push_str("// HSL-generated Dart 3 code -- do not edit manually\n\n");

        match item {
            Item::Struct(s) => out.push_str(&dart_struct(s)),
            Item::Enum(e) => out.push_str(&dart_enum(e)),
            Item::Trait(t) => out.push_str(&dart_trait(t)),
            Item::Fn(f) => out.push_str(&dart_fn(f)),
            Item::Graph(g) => out.push_str(&dart_graph(g, ctx)),
            Item::Impl(imp) => out.push_str(&dart_impl(imp)),
            Item::Const(c) => out.push_str(&dart_const(c)),
            Item::TypeAlias(a) => out.push_str(&dart_typealias(a)),
            Item::MacroRules(m) => out.push_str(&dart_macro_rules(m)),
            _ => {
                return Err(format!(
                    "dart backend does not support {}",
                    item_kind_name(item)
                ))
            }
        }
        Ok(out)
    }
}

// ──────────────────────────────────────────────────────
// Dart 3 keyword avoidance table (50+ keywords)
// ──────────────────────────────────────────────────────

const DART_KW: &[&str] = &[
    "abstract", "as", "assert", "async", "await", "break", "case", "catch", "class",
    "const", "continue", "covariant", "default", "deferred", "do", "dynamic", "else",
    "enum", "extends", "export", "extension", "external", "factory", "false", "final",
    "finally", "for", "function", "get", "hide", "if", "implements", "import", "in",
    "interface", "is", "late", "mixin", "new", "null", "on", "operator", "part",
    "required", "rethrow", "return", "set", "show", "static", "super", "switch", "sync",
    "this", "throw", "true", "try", "type", "typedef", "var", "void", "while", "with",
    "yield",
];

fn dart_ident(s: &str) -> String {
    if DART_KW.contains(&s) {
        format!("{}$", s)
    } else {
        s.to_string()
    }
}

// ──────────────────────────────────────────────────────
// Type mapping (aligned with registry.ts Dart TypeMap)
// ──────────────────────────────────────────────────────

fn dart_generic_arg(arg: &GenericArg) -> String {
    match arg {
        GenericArg::Type(t) => dart_type(t),
        GenericArg::Const(c) => match &c.kind {
            ConstArgKind::Literal(lit) => lit.raw.clone(),
            ConstArgKind::Block(_) => "0".into(),
        },
    }
}

fn dart_two_generic_args(args: &[GenericArg]) -> (String, String) {
    let mut it = args.iter();
    (
        it.next()
            .map(dart_generic_arg)
            .unwrap_or_else(|| "dynamic".into()),
        it.next()
            .map(dart_generic_arg)
            .unwrap_or_else(|| "dynamic".into()),
    )
}

fn dart_type(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(pt) => {
            let name = pt.path.last().name.as_str();
            match name {
                "String" | "str" => "String".into(),
                "char" => "String".into(),
                "bool" => "bool".into(),
                "i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "i64" | "u64"
                | "usize" | "isize" => "int".into(),
                "i128" | "u128" => "int".into(),
                "f32" | "f64" => "double".into(),
                "Vec" => format!(
                    "List<{}>",
                    pt.generic_args
                        .iter()
                        .next()
                        .map(dart_generic_arg)
                        .unwrap_or_else(|| "dynamic".into())
                ),
                "HashMap" | "BTreeMap" => {
                    let (k, v) = dart_two_generic_args(&pt.generic_args);
                    format!("Map<{}, {}>", k, v)
                }
                "HashSet" | "BTreeSet" => format!(
                    "Set<{}>",
                    pt.generic_args
                        .iter()
                        .next()
                        .map(dart_generic_arg)
                        .unwrap_or_else(|| "dynamic".into())
                ),
                "Option" => format!(
                    "{}?",
                    pt.generic_args
                        .iter()
                        .next()
                        .map(dart_generic_arg)
                        .unwrap_or_else(|| "dynamic".into())
                ),
                "Result" => {
                    // Dart uses exceptions, not Result. Map to just T.
                    if !pt.generic_args.is_empty() {
                        dart_generic_arg(&pt.generic_args[0])
                    } else {
                        "dynamic".into()
                    }
                }
                "Box" => {
                    if !pt.generic_args.is_empty() {
                        dart_generic_arg(&pt.generic_args[0])
                    } else {
                        "dynamic".into()
                    }
                }
                "unit" => "void".into(),
                _ => dart_ident(name),
            }
        }
        TypeKind::Ref { inner, .. } => dart_type(inner),
        TypeKind::Tuple(elems) => {
            if elems.is_empty() {
                "void".into()
            } else if elems.len() == 2 {
                // Dart 3 Record type
                let es: Vec<String> = elems.iter().map(dart_type).collect();
                format!("({}, {})", es[0], es[1])
            } else if elems.len() == 3 {
                let es: Vec<String> = elems.iter().map(dart_type).collect();
                format!("({}, {}, {})", es[0], es[1], es[2])
            } else {
                // Fallback: use List<dynamic>
                "List<dynamic>".into()
            }
        }
        TypeKind::Array { elem, .. } => format!("List<{}>", dart_type(elem)),
        TypeKind::Slice(inner) => format!("List<{}>", dart_type(inner)),
        TypeKind::Paren(inner) => dart_type(inner),
        TypeKind::Never => "Never".into(),
        TypeKind::FnPtr { params, ret } => {
            let r = ret
                .as_ref()
                .map(|t| dart_type(t))
                .unwrap_or_else(|| "void".into());
            if params.is_empty() {
                format!("{} Function()", r)
            } else {
                format!(
                    "{} Function({})",
                    r,
                    params.iter().map(dart_type).collect::<Vec<_>>().join(", ")
                )
            }
        }
        TypeKind::DynTrait(_) | TypeKind::ImplTrait(_) | TypeKind::Infer => "dynamic".into(),
    }
}

// ──────────────────────────────────────────────────────
// Standard library method mapping (Vec→List, String, Option)
// ──────────────────────────────────────────────────────

/// Map Rust/Vec std method names to Dart equivalents
fn dart_std_method(receiver: &str, method: &str, args: &[Expr]) -> Option<String> {
    let args_str: Vec<String> = args.iter().map(|a| emit_expr_dart(a, 0)).collect();
    match method {
        // Vec/List methods
        "push" | "append" => {
            if args_str.len() == 1 {
                Some(format!("{}.add({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "pop" => Some(format!("{}.removeLast()", receiver)),
        "len" | "length" => Some(format!("{}.length", receiver)),
        "is_empty" => Some(format!("{}.isEmpty", receiver)),
        "sort" => Some(format!("{}.sort()", receiver)),
        "sorted" => Some(format!("(..{}).toList()..sort()", receiver)),
        "reverse" => Some(format!("{} = {}.reversed.toList()", receiver, receiver)),
        "map" => {
            if args_str.len() == 1 {
                Some(format!("{}.map({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "where" | "filter" => {
            if args_str.len() == 1 {
                Some(format!("{}.where({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "first" => Some(format!("{}.first", receiver)),
        "last" => Some(format!("{}.last", receiver)),
        "contains" => {
            if args_str.len() == 1 {
                Some(format!("{}.contains({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "fold" => {
            if args_str.len() == 2 {
                Some(format!(
                    "{}.fold<{}, {}>({}, {})",
                    receiver,
                    "dynamic",
                    "dynamic",
                    args_str[0],
                    args_str[1]
                ))
            } else {
                None
            }
        }
        "reduce" => {
            if args_str.len() == 1 {
                Some(format!("{}.reduce({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "any" | "exists" => {
            if args_str.len() == 1 {
                Some(format!("{}.any({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "all" | "forall" => {
            if args_str.len() == 1 {
                Some(format!("{}.every({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "flat_map" | "flatMap" => {
            if args_str.len() == 1 {
                Some(format!("{}.expand({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "collect" => Some(format!("{}.toList()", receiver)),
        "for_each" | "foreach" => {
            if args_str.len() == 1 {
                Some(format!("{}.forEach({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "find" => {
            if args_str.len() == 1 {
                Some(format!(
                    "{}.cast<dynamic?>().firstWhere({}, orElse: () => null)",
                    receiver, args_str[0]
                ))
            } else {
                None
            }
        }
        // String methods
        "to_string" | "toString" => Some(format!("{}.toString()", receiver)),
        "trim" => Some(format!("{}.trim()", receiver)),
        "to_lowercase" | "toLowerCase" => Some(format!("{}.toLowerCase()", receiver)),
        "to_uppercase" | "toUpperCase" => Some(format!("{}.toUpperCase()", receiver)),
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
                Some(format!("{}.split({})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "replace" => {
            if args_str.len() == 2 {
                Some(format!(
                    "{}.replaceAll({}, {})",
                    receiver, args_str[0], args_str[1]
                ))
            } else {
                None
            }
        }
        "substring" | "chars" | "toCharArray" => Some(format!("{}.split('')", receiver)),
        // Option (?) methods
        "is_some" => Some(format!("{} != null", receiver)),
        "is_none" => Some(format!("{} == null", receiver)),
        "unwrap" | "get" => Some(format!("{}!", receiver)),
        "expect" => {
            if args_str.len() == 1 {
                Some(format!("{} /* expect: {} */ !", receiver, args_str[0]))
            } else {
                Some(format!("{}!", receiver))
            }
        }
        "and_then" => {
            if args_str.len() == 1 {
                Some(format!("{}.andThen((x) {})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "unwrap_or" | "getOrElse" => {
            if args_str.len() == 1 {
                Some(format!("{} ?? {}", receiver, args_str[0]))
            } else {
                None
            }
        }
        "then" => {
            if args_str.len() == 1 {
                Some(format!(
                    "{} != null ? {}({}!) : null",
                    receiver, args_str[0], receiver
                ))
            } else {
                None
            }
        }
        "or_else" => {
            if args_str.len() == 1 {
                Some(format!("{} ?? {}", receiver, args_str[0]))
            } else {
                None
            }
        }
        // Result methods (Dart uses exceptions)
        "is_ok" => Some("true /* is_ok not applicable in Dart */".into()),
        "is_err" => Some("false /* is_err not applicable in Dart */".into()),
        "ok" | "toOption" => Some("/* Result.ok not applicable */".into()),
        // Map methods
        "insert" => {
            if args_str.len() == 2 {
                Some(format!("{}[{}] = {}", receiver, args_str[0], args_str[1]))
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
// Parameters
// ──────────────────────────────────────────────────────

fn dart_param(p: &Param) -> Option<String> {
    match &p.kind {
        ParamKind::Self_(_) => None,
        ParamKind::Pattern(pat) => {
            let name = match &pat.kind {
                PatternKind::Ident { name, .. } => dart_ident(&name.name),
                _ => "arg".into(),
            };
            Some(format!("{} {}", dart_type(&p.ty), name))
        }
    }
}

// ──────────────────────────────────────────────────────
// Item translation
// ──────────────────────────────────────────────────────

fn dart_struct(s: &StructDef) -> String {
    let name = dart_ident(&s.name.name);
    match &s.kind {
        StructKind::Named(fields) => {
            let mut o = format!("class {} {{\n", name);
            // Fields
            for f in fields {
                let fn_ = f
                    .name
                    .as_ref()
                    .map(|n| dart_ident(&n.name))
                    .unwrap_or_else(|| "_".into());
                o.push_str(&format!("  {} {};
", dart_type(&f.ty), fn_));
            }
            // Constructor
            let params: Vec<String> = fields
                .iter()
                .map(|f| {
                    let fn_ = f
                        .name
                        .as_ref()
                        .map(|n| dart_ident(&n.name))
                        .unwrap_or_else(|| "_".into());
                    format!("this.{}", fn_)
                })
                .collect();
            if !params.is_empty() {
                o.push_str(&format!("  {}({});\n", name, params.join(", ")));
            }
            o.push_str("}\n\n");
            o
        }
        StructKind::Tuple(fields) => {
            // Dart 3 Records for tuple structs
            let fs: Vec<String> = fields.iter().map(|f| dart_type(&f.ty)).collect();
            match fs.len() {
                2 => format!("typedef {} = ({});\n\n", name, fs.join(", ")),
                3 => format!(
                    "typedef {} = ({}, {}, {});\n\n",
                    name, fs[0], fs[1], fs[2]
                ),
                _ => {
                    let mut o = format!("class {} {{\n", name);
                    for (i, f) in fields.iter().enumerate() {
                        o.push_str(&format!(
                            "  {} ${};\n",
                            dart_type(&f.ty), i
                        ));
                    }
                    o.push_str("}\n\n");
                    o
                }
            }
        }
        StructKind::Unit => format!("class {} {{}}\n\n", name),
    }
}

fn dart_enum(e: &EnumDef) -> String {
    let name = dart_ident(&e.name.name);
    let has_data = e
        .variants
        .iter()
        .any(|v| !matches!(&v.fields, StructKind::Unit));

    if !has_data {
        // Unit enum -> sealed class + static const instances
        let mut o = format!("sealed class {} {{\n", name);
        o.push_str(&format!("  const {}();\n", name));
        o.push_str("\n");
        for v in &e.variants {
            let vn = dart_ident(&v.name.name);
            o.push_str(&format!(
                "  static const {} {} = {}._();\n",
                name, vn, name
            ));
        }
        o.push_str(&format!("  const {}._();\n", name));
        o.push_str("}\n\n");
        o
    } else {
        // Data enum -> sealed class + subclass with factory constructor (Dart 3 pattern)
        let mut o = format!("sealed class {} {{\n  const {}();\n}}\n\n", name, name);
        for v in &e.variants {
            let vn = dart_ident(&v.name.name);
            match &v.fields {
                StructKind::Unit => {
                    o.push_str(&format!(
                        "class {} extends {} {{\n  const {}();\n}}\n\n",
                        vn, name, vn
                    ));
                }
                StructKind::Named(fields) => {
                    let field_decls: Vec<String> = fields
                        .iter()
                        .map(|f| {
                            let fn_ = f
                                .name
                                .as_ref()
                                .map(|n| dart_ident(&n.name))
                                .unwrap_or_else(|| "_".into());
                            format!("  final {} {};", dart_type(&f.ty), fn_)
                        })
                        .collect();
                    let ctor_params: Vec<String> = fields
                        .iter()
                        .map(|f| {
                            let fn_ = f
                                .name
                                .as_ref()
                                .map(|n| dart_ident(&n.name))
                                .unwrap_or_else(|| "_".into());
                            format!("required this.{}", fn_)
                        })
                        .collect();
                    o.push_str(&format!(
                        "class {} extends {} {{\n  const {}({{{}}});\n{}\n}}\n\n",
                        vn,
                        name,
                        vn,
                        ctor_params.join(", "),
                        field_decls.join("\n")
                    ));
                }
                StructKind::Tuple(fields) => {
                    let field_decls: Vec<String> = fields
                        .iter()
                        .enumerate()
                        .map(|(i, f)| format!("  final {} ${};", dart_type(&f.ty), i))
                        .collect();
                    let ctor_params: Vec<String> = fields
                        .iter()
                        .enumerate()
                        .map(|(i, f)| {
                            format!(
                                "required {} ${}{}",
                                dart_type(&f.ty),
                                i,
                                if fields.len() == 2 { "" } else { "," }
                            )
                        })
                        .collect();
                    let ctor_params_str = ctor_params.join(" ");
                    o.push_str(&format!(
                        "class {} extends {} {{\n  const {}({{{}}});\n{}\n}}\n\n",
                        vn, name, vn, ctor_params_str, field_decls.join("\n")
                    ));
                }
            }
        }
        o
    }
}

fn dart_trait(t: &TraitDef) -> String {
    let name = dart_ident(&t.name.name);
    let mut o = format!("abstract class {} {{\n", name);
    for ti in &t.items {
        match ti {
            TraitItem::FnSig(sig) => {
                let ps: Vec<String> = sig.params.iter().filter_map(dart_param).collect();
                let r = sig
                    .ret
                    .as_ref()
                    .map(|t| format!(" {}", dart_type(t)))
                    .unwrap_or_else(|| "void".into());
                o.push_str(&format!(
                    "  {} {}({}) {}\n",
                    r, dart_ident(&sig.name.name), ps.join(", "), "{ }"
                ));
            }
            TraitItem::Fn(f) => {
                let ps: Vec<String> = f.params.iter().filter_map(dart_param).collect();
                let r = f
                    .ret
                    .as_ref()
                    .map(|t| format!(" {}", dart_type(t)))
                    .unwrap_or_else(|| "void".into());
                o.push_str(&format!(
                    "  {} {}({}) {}\n",
                    r, dart_ident(&f.name.name), ps.join(", "), "{ }"
                ));
                if let Some(body) = &f.body {
                    o.push_str(&emit_block_dart(body, 2));
                }
                o.push_str("  }\n");
            }
            TraitItem::Const(_) | TraitItem::TypeAlias(_) => {}
        }
    }
    o.push_str("}\n\n");
    o
}

fn dart_fn(f: &FnDef) -> String {
    let name = dart_ident(&f.name.name);
    let ret = f
        .ret
        .as_ref()
        .map(|t| format!(" {}", dart_type(t)))
        .unwrap_or_else(|| "void".into());
    let ps: Vec<String> = f.params.iter().filter_map(dart_param).collect();
    let async_kw = if f.is_async { "async " } else { "" };
    let mut o = format!(
        "{}{} {}({}) {{\n",
        async_kw, ret, name, ps.join(", ")
    );
    if let Some(body) = &f.body {
        o.push_str(&emit_block_dart(body, 1));
    }
    o.push_str("}\n\n");
    o
}

fn dart_graph(g: &GraphDef, ctx: &CodegenContext) -> String {
    let _gn = dart_ident(&g.name.name);
    let mut o = format!(
        "// graph {} -- scale: {:?}\n",
        g.name.name, ctx.scale
    );
    o.push_str("void main() {\n");
    for gs in &g.body {
        match gs {
            GraphStmt::Node(n) => o.push_str(&format!(
                "  // node {}: {}\n",
                dart_ident(&n.name.name),
                dart_type(&n.ty)
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
                emit_let_dart(l, 0)
            )),
            GraphStmt::Stmt(s) => o.push_str(&emit_stmt_dart(s, 1)),
            GraphStmt::Item(_) => {}
        }
    }
    o.push_str("}\n\n");
    o
}

fn dart_impl(imp: &ImplDef) -> String {
    let tn = imp
        .trait_ty
        .as_ref()
        .map(|t| dart_type(t))
        .unwrap_or_default();
    let sn = dart_type(&imp.self_ty);
    let mut o = if !tn.is_empty() {
        format!("class {} extends {} {{\n", sn, tn)
    } else {
        format!("class {} {{\n", sn)
    };
    for item in &imp.items {
        match item {
            ImplItem::Fn(f) => {
                let fn_ = dart_ident(&f.name.name);
                let r = f
                    .ret
                    .as_ref()
                    .map(|t| format!(" {}", dart_type(t)))
                    .unwrap_or_else(|| "void".into());
                let ps: Vec<String> = f.params.iter().filter_map(dart_param).collect();
                let async_kw = if f.is_async { "async " } else { "" };
                o.push_str(&format!(
                    "  {}{} {}({}) {{\n",
                    async_kw, r, fn_, ps.join(", ")
                ));
                if let Some(body) = &f.body {
                    o.push_str(&emit_block_dart(body, 2));
                }
                o.push_str("  }\n");
            }
            ImplItem::Const(c) => {
                o.push_str(&format!(
                    "  static final {} = {};\n",
                    dart_ident(&c.name.name),
                    emit_expr_dart(&c.value, 0)
                ));
            }
            ImplItem::TypeAlias(_) => {}
        }
    }
    o.push_str("}\n\n");
    o
}

fn dart_const(c: &ConstDef) -> String {
    format!(
        "const {} {} = {};\n\n",
        dart_type(&c.ty),
        dart_ident(&c.name.name),
        emit_expr_dart(&c.value, 0)
    )
}

fn dart_typealias(a: &TypeAliasDef) -> String {
    format!(
        "// Type alias: {} = {}\n",
        dart_ident(&a.name.name),
        dart_type(&a.ty)
    )
}

fn dart_macro_rules(m: &MacroRulesDefinition) -> String {
    format!(
        "// macro_rules {} -- not directly translatable to Dart\n\n",
        dart_ident(&m.name.name)
    )
}

// ──────────────────────────────────────────────────────
// Expression translation
// ──────────────────────────────────────────────────────

pub fn emit_expr_dart(expr: &Expr, indent: usize) -> String {
    let ind = "  ".repeat(indent);
    match &expr.kind {
        ExprKind::Literal(lit) => dart_literal(lit),
        ExprKind::Path(p) => dart_ident(&p.last().name),
        ExprKind::Binary { op, lhs, rhs } => format!(
            "({} {} {})",
            emit_expr_dart(lhs, 0),
            dart_binop(op),
            emit_expr_dart(rhs, 0)
        ),
        ExprKind::Unary { op, operand } => format!(
            "{}{}",
            dart_unop(op),
            emit_expr_dart(operand, 0)
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
                        "print('')".into()
                    } else if args.len() == 1 {
                        if let ExprKind::Literal(lit) = &args[0].kind {
                            if let LiteralKind::Str { value: _, .. } = &lit.kind {
                                // println!("text") -> print("text")
                                return format!("print({})", dart_literal(lit));
                            }
                        }
                        format!("print({})", emit_expr_dart(&args[0], 0))
                    } else {
                        // Multiple args: use string interpolation
                        format!("print({})", args.iter().map(|a| emit_expr_dart(a, 0)).collect::<Vec<_>>().join(" + "))
                    }
                }
                Some("eprintln") => {
                    if args.is_empty() {
                        "stderr.writeln('')".into()
                    } else {
                        format!("stderr.writeln({})", args.iter().map(|a| emit_expr_dart(a, 0)).collect::<Vec<_>>().join(" + "))
                    }
                }
                Some("format") => {
                    // format!("hello {}", x) -> "hello ${x}"
                    if !args.is_empty() {
                        if let ExprKind::Literal(lit) = &args[0].kind {
                            if let LiteralKind::Str { value, .. } = &lit.kind {
                                return dart_format_string(value, &args[1..]);
                            }
                        }
                    }
                    let as_ = args.iter().map(|a| emit_expr_dart(a, 0)).collect::<Vec<_>>();
                    as_.join(" + ")
                }
                Some("vec") => {
                    let as_ = args.iter().map(|a| emit_expr_dart(a, 0)).collect::<Vec<_>>();
                    format!("[{}]", as_.join(", "))
                }
                Some("panic") => {
                    if !args.is_empty() {
                        format!("throw Exception({})", emit_expr_dart(&args[0], 0))
                    } else {
                        "throw Exception('panic')".into()
                    }
                }
                Some("todo") | Some("unimplemented") => {
                    "throw UnimplementedError()".into()
                }
                _ => {
                    let as_ = args.iter().map(|a| emit_expr_dart(a, 0)).collect::<Vec<_>>();
                    format!("{}({})", emit_expr_dart(callee, 0), as_.join(", "))
                }
            }
        }
        ExprKind::MethodCall { receiver, method, generic_args: _, args } => {
            let mn = &method.name;
            let recv_str = emit_expr_dart(receiver, 0);
            // Check std method mapping
            if let Some(mapped) = dart_std_method(&recv_str, mn, args) {
                return mapped;
            }
            let mn_esc = dart_ident(mn);
            let as_ = args.iter().map(|a| emit_expr_dart(a, 0)).collect::<Vec<_>>();
            format!("{}.{}({})", recv_str, mn_esc, as_.join(", "))
        }
        ExprKind::Field { base, field } => {
            let fn_ = match field {
                FieldIndex::Named(id) => dart_ident(&id.name),
                FieldIndex::Index(i, _) => format!("${}", i),
            };
            let recv = emit_expr_dart(base, 0);
            format!("{}.{}", recv, fn_)
        }
        ExprKind::Index { base, index } => {
            format!("{}[{}]", emit_expr_dart(base, 0), emit_expr_dart(index, 0))
        }
        ExprKind::Slice { base, range } => {
            let bs = emit_expr_dart(base, 0);
            let s = range
                .lo
                .as_ref()
                .map(|e| emit_expr_dart(e, 0))
                .unwrap_or_else(|| "0".into());
            let e = range
                .hi
                .as_ref()
                .map(|e| emit_expr_dart(e, 0))
                .unwrap_or_else(|| format!("{}.length", bs));
            if range.inclusive {
                format!("{}.sublist({}, {} + 1)", bs, s, e)
            } else {
                format!("{}.sublist({}, {})", bs, s, e)
            }
        }
        ExprKind::Range(r) => {
            let l = r
                .lo
                .as_ref()
                .map(|e| emit_expr_dart(e, 0))
                .unwrap_or_else(|| "0".into());
            let hi = r
                .hi
                .as_ref()
                .map(|e| emit_expr_dart(e, 0))
                .unwrap_or_else(|| "/* no hi */".into());
            if r.inclusive {
                format!("/* inclusive range: {}..={} */", l, hi)
            } else {
                format!("/* range: {}..{} */", l, hi)
            }
        }
        ExprKind::Assign { lhs, rhs } => {
            format!("{} = {}", emit_expr_dart(lhs, 0), emit_expr_dart(rhs, 0))
        }
        ExprKind::CompoundAssign { op, lhs, rhs } => {
            format!(
                "{} {}= {}",
                emit_expr_dart(lhs, 0),
                dart_binop(op).trim(),
                emit_expr_dart(rhs, 0)
            )
        }
        ExprKind::If { cond, then, else_ } => {
            let mut o = format!("if ({}) {{\n", emit_expr_dart(cond, 0));
            o.push_str(&emit_block_dart(then, indent + 1));
            if let Some(els) = else_ {
                o.push_str(&format!("{}}} else ", ind));
                match &els.kind {
                    ExprKind::If { .. } => {
                        // else if chain
                        o.push_str(&emit_expr_dart(els, indent));
                    }
                    ExprKind::Block(b) => {
                        o.push_str("{\n");
                        o.push_str(&emit_block_dart(b, indent + 1));
                        o.push_str(&format!("{}}}\n", ind));
                    }
                    _ => {
                        o.push_str(&emit_expr_dart(els, 0));
                        o.push('\n');
                    }
                }
            } else {
                o.push_str(&format!("{}}}\n", ind));
            }
            o
        }
        ExprKind::Match { scrutinee, arms } => {
            // Dart 3: switch expression
            let mut o = format!("switch ({}) {{\n", emit_expr_dart(scrutinee, 0));
            for arm in arms {
                let p = dart_pattern(&arm.pattern);
                let g = if let Some(g) = &arm.guard {
                    format!(" when ({})", emit_expr_dart(g, 0))
                } else {
                    String::new()
                };
                match &arm.body.kind {
                    ExprKind::Block(b) => {
                        o.push_str(&format!(
                            "{}  case {}{} =>\n",
                            ind, p, g
                        ));
                        o.push_str(&emit_block_dart(b, indent + 2));
                    }
                    _ => {
                        o.push_str(&format!(
                            "{}  case {}{} => {}\n",
                            ind,
                            p,
                            g,
                            emit_expr_dart(&arm.body, 0)
                        ));
                    }
                }
            }
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::For { label: _, pattern, iter, body } => {
            // Check if iter is a Range expression -> translate to for (int i = lo; i < hi; i++)
            let mut o = String::new();
            if let ExprKind::Range(r) = &iter.kind {
                let lo = r
                    .lo
                    .as_ref()
                    .map(|e| emit_expr_dart(e, 0))
                    .unwrap_or_else(|| "0".into());
                let hi = r
                    .hi
                    .as_ref()
                    .map(|e| emit_expr_dart(e, 0))
                    .unwrap_or_else(|| "0".into());
                // Extract variable name from pattern if possible
                let var_name = match &pattern.kind {
                    PatternKind::Ident { name, .. } => dart_ident(&name.name),
                    _ => "i".into(),
                };
                if r.inclusive {
                    o.push_str(&format!(
                        "{}for (int {} = {}; {} <= {}; {}++) {{\n",
                        ind, var_name, lo, var_name, hi, var_name
                    ));
                } else {
                    o.push_str(&format!(
                        "{}for (int {} = {}; {} < {}; {}++) {{\n",
                        ind, var_name, lo, var_name, hi, var_name
                    ));
                }
                o.push_str(&emit_block_dart(body, indent + 1));
                o.push_str(&format!("{}}}\n", ind));
            } else {
                o.push_str(&format!(
                    "{}for (final {} in {}) {{\n",
                    ind,
                    dart_pattern(pattern),
                    emit_expr_dart(iter, 0)
                ));
                o.push_str(&emit_block_dart(body, indent + 1));
                o.push_str(&format!("{}}}\n", ind));
            }
            o
        }
        ExprKind::While { label: _, cond, body } => {
            let mut o = format!("{}while ({}) {{\n", ind, emit_expr_dart(cond, 0));
            o.push_str(&emit_block_dart(body, indent + 1));
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::WhileLet { label: _, pattern, expr: scrut, body } => {
            // while let pat = expr → while (true) { switch (expr) { case pat => body; default: break; } }
            let mut o = format!("{}while (true) {{\n", ind);
            o.push_str(&format!(
                "{}  switch ({}) {{\n",
                ind,
                emit_expr_dart(scrut, 0)
            ));
            o.push_str(&format!(
                "{}    case {}:\n",
                ind,
                dart_pattern(pattern)
            ));
            o.push_str(&emit_block_dart(body, indent + 3));
            o.push_str(&format!("{}    default:\n", ind));
            o.push_str(&format!("{}      break;\n", ind));
            o.push_str(&format!("{}  }}\n", ind));
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::Loop { label, body } => {
            let mut o = String::new();
            if let Some(l) = label {
                o.push_str(&format!("{}// label: {}\n", ind, dart_ident(&l.name)));
            }
            o.push_str(&format!("{}while (true) {{\n", ind));
            o.push_str(&emit_block_dart(body, indent + 1));
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::Closure { is_move: _, is_async, params, ret: _, body } => {
            let ps: Vec<String> = params
                .iter()
                .filter_map(|p| match &p.kind {
                    ParamKind::Pattern(pat) => match &pat.kind {
                        PatternKind::Ident { name, .. } => Some(dart_ident(&name.name)),
                        _ => Some("x".into()),
                    },
                    _ => None,
                })
                .collect();
            let async_kw = if *is_async { "async " } else { "" };
            if let ExprKind::Block(be) = &body.kind {
                if be.stmts.is_empty() {
                    if let Some(tail) = &be.tail {
                        return format!(
                            "{}({}) => {}",
                            async_kw,
                            ps.join(", "),
                            emit_expr_dart(tail, 0)
                        );
                    }
                }
                let mut o = format!("{}({}) => {{\n", async_kw, ps.join(", "));
                o.push_str(&emit_block_dart(be, indent + 1));
                o.push_str(&format!("{}}}", ind));
                o
            } else {
                format!(
                    "{}({}) => {}",
                    async_kw,
                    ps.join(", "),
                    emit_expr_dart(body, 0)
                )
            }
        }
        ExprKind::Return(value) => {
            if let Some(v) = value {
                format!("return {}", emit_expr_dart(v, 0))
            } else {
                "return".into()
            }
        }
        ExprKind::Break { label, value } => {
            let mut o = "break".to_string();
            if let Some(l) = label {
                o = format!("// break from {}", dart_ident(&l.name));
            }
            if let Some(v) = value {
                o.push_str(&format!(" /* with value: {} */", emit_expr_dart(v, 0)));
            }
            o
        }
        ExprKind::Continue { label } => {
            if let Some(l) = label {
                format!("// continue {}", dart_ident(&l.name))
            } else {
                "continue".into()
            }
        }
        ExprKind::Array(elems) => {
            let es: Vec<String> = elems.iter().map(|e| emit_expr_dart(e, 0)).collect();
            if es.is_empty() {
                "<dynamic>[]".into()
            } else {
                format!("[{}]", es.join(", "))
            }
        }
        ExprKind::ArrayRepeat { elem, count } => {
            format!(
                "List.filled({}, {})",
                emit_expr_dart(count, 0),
                emit_expr_dart(elem, 0)
            )
        }
        ExprKind::Struct { path, fields, spread } => {
            let name = dart_ident(&path.last().name);
            let fs: Vec<String> = fields
                .iter()
                .map(|f| {
                    let fn_ = match &f.name {
                        FieldIndex::Named(id) => dart_ident(&id.name),
                        FieldIndex::Index(i, _) => format!("${}", i),
                    };
                    let v = f
                        .value
                        .as_ref()
                        .map(|v| emit_expr_dart(v, 0))
                        .unwrap_or_else(|| fn_.clone());
                    format!("{}: {}", fn_, v)
                })
                .collect();
            let spread_str = if let Some(spread) = spread {
                format!(", /* ..{} */", emit_expr_dart(spread, 0))
            } else {
                String::new()
            };
            format!("{}({}{}{})", name, fs.join(", "), if fs.is_empty() { "" } else { "" }, spread_str)
        }
        ExprKind::Tuple(elems) => {
            let es: Vec<String> = elems.iter().map(|e| emit_expr_dart(e, 0)).collect();
            match es.len() {
                0 => "/* unit */".into(),
                _ => format!("({})", es.join(", ")),
            }
        }
        ExprKind::Block(be) => {
            let mut o = "{\n".to_string();
            o.push_str(&emit_block_dart(be, indent + 1));
            if let Some(tail) = &be.tail {
                o.push_str(&format!(
                    "{}{}\n",
                    ind,
                    emit_expr_dart(tail, 0)
                ));
            }
            o.push_str(&format!("{}}}", ind));
            o
        }
        ExprKind::AsyncBlock { body, .. } => {
            let mut o = "// async ".to_string();
            o.push_str("{\n");
            o.push_str(&emit_block_dart(body, indent + 1));
            if let Some(tail) = &body.tail {
                o.push_str(&format!(
                    "{}{}\n",
                    ind,
                    emit_expr_dart(tail, 0)
                ));
            }
            o.push_str(&format!("{}}}", ind));
            o
        }
        ExprKind::Try(inner) => {
            // Dart uses try/catch for exceptions
            let inner_str = emit_expr_dart(inner, 0);
            format!(
                "/* try({}) -- Dart uses try/catch */",
                inner_str
            )
        }
        ExprKind::Await(inner) => {
            format!("await {}", emit_expr_dart(inner, 0))
        }
        ExprKind::Cast { expr: inner, ty } => {
            format!("{} as {}", emit_expr_dart(inner, 0), dart_type(ty))
        }
        ExprKind::IfLet { pattern, expr: scrut, then, else_ } => {
            // if let pat = expr → switch (expr) { case pat => ... default: ... }
            let mut o = format!(
                "{}switch ({}) {{\n  case {}:\n",
                ind,
                emit_expr_dart(scrut, 0),
                dart_pattern(pattern)
            );
            o.push_str(&emit_block_dart(then, indent + 2));
            if let Some(els) = else_ {
                o.push_str(&format!(
                    "{}  default:\n",
                    ind
                ));
                match &els.kind {
                    ExprKind::Block(b) => {
                        o.push_str(&emit_block_dart(b, indent + 2));
                    }
                    _ => {
                        o.push_str(&format!(
                            "{}    {}\n",
                            ind,
                            emit_expr_dart(els, 0)
                        ));
                    }
                }
            }
            o.push_str(&format!("{}}}\n", ind));
            o
        }
        ExprKind::Macro { path, args: _ } => dart_macro(path.last().name.as_str()),
        ExprKind::Native(nb) => {
            // native dart block: emit as-is
            if nb.lang.name == "dart" {
                nb.code.clone()
            } else {
                "// native block".into()
            }
        }
    }
}

fn dart_literal(lit: &Literal) -> String {
    match &lit.kind {
        LiteralKind::Str { value, .. } => {
            format!(
                "'{}'",
                value.replace('\\', "\\\\").replace('\'', "\\'")
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

/// Convert Rust format! string to Dart string interpolation "...${expr}..."
fn dart_format_string(fmt: &str, args: &[Expr]) -> String {
    let mut result = String::from('"');
    let mut arg_idx = 0;
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' && chars.peek() == Some(&'{') {
            chars.next(); // consume second '{'
            result.push_str("${");
            if arg_idx < args.len() {
                result.push_str(&emit_expr_dart(&args[arg_idx], 0));
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
            result.push('}');
        } else if c == '}' {
            result.push('}');
        } else {
            result.push(c);
        }
    }
    result.push('"');
    result
}

fn dart_binop(op: &BinaryOp) -> &'static str {
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

fn dart_unop(op: &UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Not => "!",
        UnaryOp::Deref | UnaryOp::Ref | UnaryOp::RefMut => "",
    }
}

fn dart_pattern(pat: &Pattern) -> String {
    match &pat.kind {
        PatternKind::Wildcard => "_".into(),
        PatternKind::Ident { name, .. } => dart_ident(&name.name),
        PatternKind::Literal(lit) => dart_literal(lit),
        PatternKind::Path(p) => dart_ident(&p.last().name),
        PatternKind::TupleStruct { path, elems, .. } => {
            let n = dart_ident(&path.last().name);
            let es: Vec<String> = elems.iter().map(dart_pattern).collect();
            if es.is_empty() {
                n
            } else {
                format!("{}({})", n, es.join(", "))
            }
        }
        PatternKind::Struct { path, fields, .. } => {
            let n = dart_ident(&path.last().name);
            let fs: Vec<String> = fields
                .iter()
                .map(|f| {
                    let fn_ = dart_ident(&f.name.name);
                    let p = f
                        .pattern
                        .as_ref()
                        .map(|p| dart_pattern(p))
                        .unwrap_or_else(|| dart_ident(&f.name.name));
                    format!("{}: {}", fn_, p)
                })
                .collect();
            if fs.is_empty() {
                format!("{}()", n)
            } else {
                format!("{}({})", n, fs.join(", "))
            }
        }
        PatternKind::Tuple { elems, .. } => {
            let es: Vec<String> = elems.iter().map(dart_pattern).collect();
            format!("({})", es.join(", "))
        }
        PatternKind::Or(elems) => elems
            .iter()
            .map(|e| dart_pattern(e))
            .collect::<Vec<_>>()
            .join(" | "),
        PatternKind::Range { lo, hi, inclusive } => {
            let l = dart_pattern(lo);
            let r = dart_pattern(hi);
            if *inclusive {
                format!("{} <= .. <= {}", l, r)
            } else {
                format!("{} <= .. < {}", l, r)
            }
        }
        PatternKind::Rest => "...".into(),
    }
}

fn dart_macro(name: &str) -> String {
    match name {
        "println" => "print".into(),
        "eprintln" => "stderr.writeln".into(),
        "format" => "/* format macro */".into(),
        "todo" | "unimplemented" => "throw UnimplementedError()".into(),
        "panic" => "throw Exception('panic')".into(),
        "vec" => "<dynamic>[]".into(),
        _ => format!("/* macro: {} */", name),
    }
}

// ──────────────────────────────────────────────────────
// Block translation
// ──────────────────────────────────────────────────────

pub fn emit_block_dart(be: &BlockExpr, indent: usize) -> String {
    let ind = "  ".repeat(indent);
    let mut o = String::new();
    for stmt in &be.stmts {
        match stmt {
            Stmt::Let(l) => o.push_str(&format!("{}{}", ind, emit_let_dart(l, 0))),
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
                        o.push_str(&format!("{}\n", emit_expr_dart(expr, indent)));
                    }
                    ExprKind::Return(_) => {
                        o.push_str(&format!(
                            "{}{}\n",
                            ind,
                            emit_expr_dart(expr, 0)
                        ));
                    }
                    _ => {
                        o.push_str(&format!(
                            "{}{}\n",
                            ind,
                            emit_expr_dart(expr, 0)
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
                o.push_str(&format!("{}", emit_expr_dart(tail, indent)));
            }
            _ => {
                o.push_str(&format!(
                    "{}{}\n",
                    ind,
                    emit_expr_dart(tail, 0)
                ));
            }
        }
    }
    o
}

fn emit_let_dart(l: &LetStmt, _indent: usize) -> String {
    let pat = dart_pattern(&l.pattern);
    let kw = if l.mutable { "var" } else { "final" };
    if let Some(ty) = &l.ty {
        format!(
            "{} {} {} = {};\n",
            kw,
            dart_type(ty),
            pat,
            l.init
                .as_ref()
                .map(|e| emit_expr_dart(e, 0))
                .unwrap_or_else(|| "null".into())
        )
    } else {
        format!(
            "{} {} = {};\n",
            kw,
            pat,
            l.init
                .as_ref()
                .map(|e| emit_expr_dart(e, 0))
                .unwrap_or_else(|| "null".into())
        )
    }
}

pub fn emit_stmt_dart(stmt: &Stmt, indent: usize) -> String {
    match stmt {
        Stmt::Let(l) => {
            let ind = "  ".repeat(indent);
            format!("{}{}", ind, emit_let_dart(l, 0))
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
                | ExprKind::IfLet { .. } => emit_expr_dart(expr, indent) + "\n",
                _ => {
                    let ind = "  ".repeat(indent);
                    format!("{}{}\n", ind, emit_expr_dart(expr, 0))
                }
            }
        }
    }
}
