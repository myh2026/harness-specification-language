// Elixir backend (Logic tier) -- type mapping + full function body translation
// Elixir (Elixir 1.16+) code generation.
// struct -> defmodule + defstruct (named) / defmodule (tuple/unit);
// enum (unit) -> @type + module attributes;
// enum (data) -> defmodule with tagged tuples;
// trait -> behaviour + @callback;
// impl -> defmodule implementing behaviour;
// fn -> def (top-level function in a module);
// const -> @constant module attribute;
// graph -> def main()

use crate::ast::*;
use crate::codegen::{CodegenBackend, CodegenContext};

pub struct ElixirBackend;

impl CodegenBackend for ElixirBackend {
    fn lang(&self) -> &'static str {
        "elixir"
    }

    fn emit_item(&self, ctx: &CodegenContext, item: &Item) -> Result<String, String> {
        let mut out = String::new();
        out.push_str(&format!(
            "// {}\n",
            crate::sourcemap::generated_header("elixir")
        ));
        out.push_str("// HSL-generated Elixir code -- do not edit manually\n\n");

        // Elixir requires all code to be inside a module; wrap in a top-level module
        out.push_str("defmodule HSLGenerated do\n");

        match item {
            Item::Struct(s) => out.push_str(&ex_struct(s)),
            Item::Enum(e) => out.push_str(&ex_enum(e)),
            Item::Trait(t) => out.push_str(&ex_trait(t)),
            Item::Fn(f) => out.push_str(&ex_fn(f)),
            Item::Graph(g) => out.push_str(&ex_graph(g, ctx)),
            Item::Impl(imp) => out.push_str(&ex_impl(imp)),
            Item::Const(c) => out.push_str(&ex_const(c)),
            Item::TypeAlias(a) => out.push_str(&ex_typealias(a)),
            Item::MacroRules(m) => out.push_str(&ex_macro_rules(m)),
            _ => {
                return Err(format!(
                    "elixir backend does not support {}",
                    item_kind_name(item)
                ))
            }
        }
        out.push_str("end\n");
        Ok(out)
    }
}

// ──────────────────────────────────────────────────────
// Elixir keyword avoidance table (60+ keywords)
// ──────────────────────────────────────────────────────

const EX_KW: &[&str] = &[
    "def", "defp", "defmodule", "do", "end", "fn", "if", "else", "case", "cond",
    "when", "and", "or", "not", "true", "false", "nil", "receive", "after",
    "try", "catch", "rescue", "raise", "throw", "for", "in", "unless", "with",
    "use", "import", "require", "alias", "quote", "unquote", "super", "self",
    "return", "__MODULE__", "__ENV__", "__DIR__", "__CALLER__",
    "@module", "@doc", "@spec", "@type", "@typep", "@opaque", "@callback",
    "@macrocallback", "@optional_callbacks", "@behaviour", "@impl", "@moduledoc",
    "@derive", "@enum", "@for", "@async",
    // Elixir special forms
    "block", "break", "send", "spawn", "spawn_link", "exit", "Process",
    // Elixir operators used as identifiers can conflict
    "assign", "loop", "match", "guard", "node", "edge", "graph",
];

fn ex_ident(s: &str) -> String {
    if EX_KW.contains(&s) {
        format!("ex_{}", s)
    } else {
        s.to_string()
    }
}

/// Convert to CamelCase for module names
fn ex_module_name(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;
    for c in s.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    // Ensure first char is uppercase (Elixir module names must start with uppercase)
    if let Some(first) = result.chars().next() {
        if !first.is_ascii_uppercase() {
            result = format!("E{}", result);
        }
    }
    result
}

// ──────────────────────────────────────────────────────
// Type mapping (aligned with langs.rs Elixir TypeMap)
// ──────────────────────────────────────────────────────

fn ex_generic_arg(arg: &GenericArg) -> String {
    match arg {
        GenericArg::Type(t) => ex_type(t),
        GenericArg::Const(c) => match &c.kind {
            ConstArgKind::Literal(lit) => lit.raw.clone(),
            ConstArgKind::Block(_) => "0".into(),
        },
    }
}

fn ex_two_generic_args(args: &[GenericArg]) -> (String, String) {
    let mut it = args.iter();
    (
        it.next()
            .map(ex_generic_arg)
            .unwrap_or_else(|| "any".into()),
        it.next()
            .map(ex_generic_arg)
            .unwrap_or_else(|| "any".into()),
    )
}

fn ex_type(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(pt) => {
            let name = pt.path.last().name.as_str();
            match name {
                "String" | "str" => "String.t()".into(),
                "char" => "String.t()".into(),
                "bool" => "boolean".into(),
                "i8" | "i16" | "i32" | "isize" => "integer".into(),
                "i64" | "i128" => "integer".into(),
                "u8" | "u16" | "u32" | "usize" => "non_neg_integer()".into(),
                "u64" | "u128" => "non_neg_integer()".into(),
                "f32" | "f64" => "float()".into(),
                "Vec" => format!(
                    "list({})",
                    pt.generic_args
                        .iter()
                        .next()
                        .map(ex_generic_arg)
                        .unwrap_or_else(|| "any()".into())
                ),
                "HashMap" | "BTreeMap" => {
                    let (k, v) = ex_two_generic_args(&pt.generic_args);
                    format!("%{{{} => {}}}", k, v)
                }
                "HashSet" | "BTreeSet" => format!(
                    "MapSet.t({})",
                    pt.generic_args
                        .iter()
                        .next()
                        .map(ex_generic_arg)
                        .unwrap_or_else(|| "any()".into())
                ),
                "Option" => {
                    if !pt.generic_args.is_empty() {
                        let inner = ex_generic_arg(&pt.generic_args[0]);
                        format!("{{:ok, {}}} | :error", inner)
                    } else {
                        "{:ok, any()} | :error".into()
                    }
                }
                "Result" => {
                    if pt.generic_args.len() >= 2 {
                        let t = ex_generic_arg(&pt.generic_args[0]);
                        let e = ex_generic_arg(&pt.generic_args[1]);
                        format!("{{:ok, {}}} | {{:error, {}}}", t, e)
                    } else if !pt.generic_args.is_empty() {
                        let t = ex_generic_arg(&pt.generic_args[0]);
                        format!("{{:ok, {}}} | {{:error, any()}}", t)
                    } else {
                        "{:ok, any()} | {:error, any()}".into()
                    }
                }
                "Box" => {
                    if !pt.generic_args.is_empty() {
                        ex_generic_arg(&pt.generic_args[0])
                    } else {
                        "any()".into()
                    }
                }
                _ => ex_ident(name),
            }
        }
        TypeKind::Ref { inner, .. } => ex_type(inner),
        TypeKind::Tuple(elems) => {
            if elems.is_empty() {
                ":ok".into() // unit -> :ok atom
            } else {
                let es: Vec<String> = elems.iter().map(ex_type).collect();
                format!("{{{}}}", es.join(", "))
            }
        }
        TypeKind::Array { elem, .. } => format!(
            "list({})",
            ex_type(elem)
        ),
        TypeKind::Slice(inner) => format!("list({})", ex_type(inner)),
        TypeKind::Paren(inner) => ex_type(inner),
        TypeKind::Never => "no_return()".into(),
        TypeKind::FnPtr { params, ret } => {
            let r = ret
                .as_ref()
                .map(|t| ex_type(t))
                .unwrap_or_else(|| ":ok".into());
            if params.is_empty() {
                format!("(-> {})", r)
            } else {
                format!(
                    "({} -> {})",
                    params.iter().map(ex_type).collect::<Vec<_>>().join(", "),
                    r
                )
            }
        }
        TypeKind::DynTrait(_) | TypeKind::ImplTrait(_) | TypeKind::Infer => "any()".into(),
    }
}

// ──────────────────────────────────────────────────────
// Standard library method mapping
// ──────────────────────────────────────────────────────

fn ex_std_method(receiver: &str, method: &str, args: &[Expr]) -> Option<String> {
    let args_str: Vec<String> = args.iter().map(|a| emit_expr_ex(a, 0)).collect();
    match method {
        // Vec/List methods
        "push" | "append" => {
            if args_str.len() == 1 {
                // Elixir lists are immutable; use [elem | list] prepend
                Some(format!("{} ++ [{}]", receiver, args_str[0]))
            } else {
                None
            }
        }
        "pop" => Some(format!("Enum.drop({}, -1)", receiver)),
        "len" | "length" => Some(format!("length({})", receiver)),
        "is_empty" => Some(format!("{} == []", receiver)),
        "sort" => Some(format!("Enum.sort({})", receiver)),
        "sorted" => Some(format!("Enum.sort({})", receiver)),
        "reverse" => Some(format!("Enum.reverse({})", receiver)),
        "map" => {
            if args_str.len() == 1 {
                Some(format!("Enum.map({}, {})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "filter" => {
            if args_str.len() == 1 {
                Some(format!("Enum.filter({}, {})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "fold" => {
            if args_str.len() == 2 {
                Some(format!("Enum.reduce({}, {}, {})", receiver, args_str[0], args_str[1]))
            } else {
                None
            }
        }
        "for_each" | "foreach" => {
            if args_str.len() == 1 {
                Some(format!("Enum.each({}, {})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "find" => {
            if args_str.len() == 1 {
                Some(format!("Enum.find({}, {})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "any" => {
            if args_str.len() == 1 {
                Some(format!("Enum.any?( {}, {})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "all" => {
            if args_str.len() == 1 {
                Some(format!("Enum.all?( {}, {})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "flat_map" | "flatMap" => {
            if args_str.len() == 1 {
                Some(format!("Enum.flat_map({}, {})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "collect" => Some(receiver.to_string()),
        "contains" => {
            if args_str.len() == 1 {
                Some(format!("{} in {}", args_str[0], receiver))
            } else {
                None
            }
        }
        "clear" => Some("[]".into()),
        // String methods
        "to_string" | "to_str" | "toString" => Some(format!("to_string({})", receiver)),
        "trim" => Some(format!("String.trim({})", receiver)),
        "to_lowercase" | "toLowerCase" => Some(format!("String.downcase({})", receiver)),
        "to_uppercase" | "toUpperCase" => Some(format!("String.upcase({})", receiver)),
        "starts_with" | "startsWith" => {
            if args_str.len() == 1 {
                Some(format!("String.starts_with?({}, {})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "ends_with" | "endsWith" => {
            if args_str.len() == 1 {
                Some(format!("String.ends_with?({}, {})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "split" => {
            if args_str.len() == 1 {
                Some(format!("String.split({}, {})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "replace" => {
            if args_str.len() == 2 {
                Some(format!("String.replace({}, {}, {})", receiver, args_str[0], args_str[1]))
            } else {
                None
            }
        }
        "chars" | "toCharArray" => Some(format!("String.graphemes({})", receiver)),
        // Option methods (Elixir uses {:ok, val} | :error)
        "is_some" => Some(format!("match {} do {{ :ok, _ -> true; :error -> false end", receiver)),
        "is_none" => Some(format!("match {} do {{ :ok, _ -> false; :error -> true end", receiver)),
        "unwrap" => Some(format!("elem({}, 1)", receiver)),
        "expect" => {
            if args_str.len() == 1 {
                Some(format!("elem({}, 1) # {}", receiver, args_str[0]))
            } else {
                Some(format!("elem({}, 1)", receiver))
            }
        }
        "and_then" => {
            if args_str.len() == 1 {
                Some(format!("with {{ :ok, v <- {} }}, {}.(v) end", receiver, args_str[0]))
            } else {
                None
            }
        }
        "unwrap_or" | "getOrElse" => {
            if args_str.len() == 1 {
                Some(format!(
                    "case {} do {{ :ok, v -> v; :error -> {} end",
                    receiver, args_str[0]
                ))
            } else {
                None
            }
        }
        "ok" | "to_option" => Some(receiver.to_string()),
        // Result methods
        "is_ok" => Some(format!("match {} do {{ :ok, _ -> true; _ -> false end", receiver)),
        "is_err" => Some(format!("match {} do {{ :error -> true; _ -> false end", receiver)),
        // Map methods
        "insert" | "put" => {
            if args_str.len() == 2 {
                Some(format!("Map.put({}, {}, {})", receiver, args_str[0], args_str[1]))
            } else {
                None
            }
        }
        "get" => {
            if args_str.len() == 1 {
                Some(format!("Map.get({}, {})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "keys" => Some(format!("Map.keys({})", receiver)),
        "values" => Some(format!("Map.values({})", receiver)),
        "remove" => {
            if args_str.len() == 1 {
                Some(format!("Map.delete({}, {})", receiver, args_str[0]))
            } else {
                None
            }
        }
        // Math
        "abs" => Some(format!("abs({})", receiver)),
        "min" => {
            if args_str.len() == 1 {
                Some(format!("min({}, {})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "max" => {
            if args_str.len() == 1 {
                Some(format!("max({}, {})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "floor" => Some(format!("floor({})", receiver)),
        "ceil" => Some(format!("ceil({})", receiver)),
        "round" => Some(format!("round({})", receiver)),
        "sqrt" => Some(format!(":math.sqrt({})", receiver)),
        "parse" => {
            // String.parse::<i32>() -> Integer.parse/Float.parse
            if args_str.is_empty() {
                Some(format!("String.to_integer({})", receiver))
            } else {
                Some(format!("String.to_integer({})", receiver))
            }
        }
        "clone" => Some(receiver.to_string()),
        "into" => Some(receiver.to_string()),
        _ => None,
    }
}

// ──────────────────────────────────────────────────────
// Parameters
// ──────────────────────────────────────────────────────

fn ex_param(p: &Param) -> Option<String> {
    match &p.kind {
        ParamKind::Self_(_) => None,
        ParamKind::Pattern(pat) => {
            let name = match &pat.kind {
                PatternKind::Ident { name, .. } => ex_ident(&name.name),
                _ => "_arg".into(),
            };
            // Elixir doesn't have type annotations on function params in the same way
            // but we can use @spec. For now, just return the name.
            Some(name)
        }
    }
}

fn ex_param_with_type(p: &Param) -> Option<String> {
    match &p.kind {
        ParamKind::Self_(_) => None,
        ParamKind::Pattern(pat) => {
            let name = match &pat.kind {
                PatternKind::Ident { name, .. } => ex_ident(&name.name),
                _ => "_arg".into(),
            };
            Some(format!("{} \\ {}", name, ex_type(&p.ty)))
        }
    }
}

// ──────────────────────────────────────────────────────
// Item translation
// ──────────────────────────────────────────────────────

fn ex_struct(s: &StructDef) -> String {
    let mod_name = ex_module_name(&s.name.name);
    match &s.kind {
        StructKind::Named(fields) => {
            let fs: Vec<String> = fields
                .iter()
                .map(|f| {
                    let fn_ = f
                        .name
                        .as_ref()
                        .map(|n| format!(":{}", ex_ident(&n.name)))
                        .unwrap_or_else(|| ":_".into());
                    fn_
                })
                .collect();
            let mut o = format!(
                "  defmodule {} do\n    defstruct [{}]\n  end\n\n",
                mod_name,
                fs.join(", ")
            );
            // Also generate a @type for external use
            let type_fields: Vec<String> = fields
                .iter()
                .map(|f| {
                    let fn_ = f
                        .name
                        .as_ref()
                        .map(|n| ex_ident(&n.name))
                        .unwrap_or_else(|| "_".into());
                    format!("{}: {}", fn_, ex_type(&f.ty))
                })
                .collect();
            if !type_fields.is_empty() {
                let fields_str = if type_fields.is_empty() {
                    String::new()
                } else {
                    format!(", {}", type_fields.join(", "))
                };
                o.push_str(&format!(
                    "  @type t :: %{}{}\n\n",
                    mod_name,
                    fields_str
                ));
            }
            o
        }
        StructKind::Tuple(fields) => {
            let fs: Vec<String> = fields
                .iter()
                .enumerate()
                .map(|(i, f)| format!("field_{}: {}", i, ex_type(&f.ty)))
                .collect();
            let mut o = format!(
                "  defmodule {} do\n    defstruct [{}]\n  end\n\n",
                mod_name,
                fs.join(", ")
            );
            o.push_str(&format!(
                "  @type t :: {{{}}}\n\n",
                fields.iter().map(|f| ex_type(&f.ty)).collect::<Vec<_>>().join(", ")
            ));
            o
        }
        StructKind::Unit => {
            format!(
                "  defmodule {} do\n    defstruct []\n  end\n\n",
                mod_name
            )
        }
    }
}

fn ex_enum(e: &EnumDef) -> String {
    let mod_name = ex_module_name(&e.name.name);
    let has_data = e
        .variants
        .iter()
        .any(|v| !matches!(&v.fields, StructKind::Unit));

    if !has_data {
        // Simple enum -> module with atom constants
        let mut o = format!("  defmodule {} do\n", mod_name);
        for v in &e.variants {
            let vn = ex_ident(&v.name.name);
            o.push_str(&format!("    @{} :{}\n", vn, vn));
        }
        // Generate @type union
        let variants: Vec<String> = e
            .variants
            .iter()
            .map(|v| format!(":{}", ex_ident(&v.name.name)))
            .collect();
        o.push_str(&format!(
            "    @type t :: {}\n",
            variants.join(" | ")
        ));
        o.push_str("  end\n\n");
        o
    } else {
        // Data enum -> module with tagged tuples
        let mut o = format!("  defmodule {} do\n", mod_name);
        let mut type_variants: Vec<String> = Vec::new();
        for v in &e.variants {
            let vn = ex_ident(&v.name.name);
            match &v.fields {
                StructKind::Unit => {
                    o.push_str(&format!("    @{} :{}\n", vn, vn));
                    type_variants.push(format!(":{}", vn));
                }
                StructKind::Named(fields) => {
                    let field_names: Vec<String> = fields
                        .iter()
                        .map(|f| {
                            f.name
                                .as_ref()
                                .map(|n| ex_ident(&n.name))
                                .unwrap_or_else(|| "_".into())
                        })
                        .collect();
                    let field_types: Vec<String> = fields.iter().map(|f| ex_type(&f.ty)).collect();
                    o.push_str(&format!(
                        "    defstruct [{}]\n",
                        field_names
                            .iter()
                            .zip(field_types.iter())
                            .map(|(n, _)| format!("{}: nil", n))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                    type_variants.push(format!(
                        "{{:{}, {{{}}}}}",
                        vn,
                        field_names
                            .iter()
                            .zip(field_types.iter())
                            .map(|(n, t)| format!("{}: {}", n, t))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                StructKind::Tuple(fields) => {
                    let field_types: Vec<String> = fields.iter().map(|f| ex_type(&f.ty)).collect();
                    type_variants.push(format!(
                        "{{:{}, {{{}}}}}",
                        vn,
                        field_types.join(", ")
                    ));
                }
            }
        }
        o.push_str(&format!(
            "    @type t :: {}\n",
            type_variants.join(" | ")
        ));
        o.push_str("  end\n\n");
        o
    }
}

fn ex_trait(t: &TraitDef) -> String {
    let mod_name = ex_module_name(&t.name.name);
    let mut o = format!("  defmodule {} do\n", mod_name);
    for ti in &t.items {
        match ti {
            TraitItem::FnSig(sig) => {
                let ps: Vec<String> = sig.params.iter().filter_map(ex_param).collect();
                let r = sig
                    .ret
                    .as_ref()
                    .map(|t| format!(" :: {}", ex_type(t)))
                    .unwrap_or_default();
                o.push_str(&format!(
                    "    @callback {}({}){}\n",
                    ex_ident(&sig.name.name),
                    ps.join(", "),
                    r
                ));
            }
            TraitItem::Fn(f) => {
                let ps: Vec<String> = f.params.iter().filter_map(ex_param).collect();
                let r = f
                    .ret
                    .as_ref()
                    .map(|t| format!(" :: {}", ex_type(t)))
                    .unwrap_or_default();
                o.push_str(&format!(
                    "    @callback {}({}){}\n",
                    ex_ident(&f.name.name),
                    ps.join(", "),
                    r
                ));
                // Default implementation as a regular function
                o.push_str(&format!(
                    "    def {}({}) do\n",
                    ex_ident(&f.name.name),
                    ps.join(", ")
                ));
                if let Some(body) = &f.body {
                    o.push_str(&emit_block_ex(body, 3));
                }
                o.push_str("    end\n");
            }
            TraitItem::Const(c) => {
                o.push_str(&format!(
                    "    @{} {}\n",
                    ex_ident(&c.name.name),
                    emit_expr_ex(&c.value, 0)
                ));
            }
            TraitItem::TypeAlias(_) => {}
        }
    }
    o.push_str("  end\n\n");
    o
}

fn ex_fn(f: &FnDef) -> String {
    let name = ex_ident(&f.name.name);
    let ps: Vec<String> = f.params.iter().filter_map(ex_param).collect();
    let mut o = String::new();
    if f.is_async {
        o.push_str("  # async \n");
    }
    // @spec type annotation
    let ret_type = f
        .ret
        .as_ref()
        .map(|t| ex_type(t))
        .unwrap_or_else(|| ":ok".into());
    o.push_str(&format!(
        "  @spec {}({}) :: {}\n",
        name,
        f.params.iter().filter_map(ex_param_with_type).collect::<Vec<_>>().join(", "),
        ret_type
    ));
    o.push_str(&format!(
        "  def {}({}) do\n",
        name,
        ps.join(", ")
    ));
    if let Some(body) = &f.body {
        o.push_str(&emit_block_ex(body, 3));
    }
    o.push_str("  end\n\n");
    o
}

fn ex_graph(g: &GraphDef, ctx: &CodegenContext) -> String {
    let _gn = ex_ident(&g.name.name);
    let mut o = format!(
        "  # graph {} -- scale: {:?}\n",
        g.name.name, ctx.scale
    );
    o.push_str("  def main() do\n");
    for gs in &g.body {
        match gs {
            GraphStmt::Node(n) => o.push_str(&format!(
                "    # node {}: {}\n",
                ex_ident(&n.name.name),
                ex_type(&n.ty)
            )),
            GraphStmt::Edge(e) => {
                let ep: Vec<String> = e
                    .endpoints
                    .iter()
                    .map(|p| p.last().name.clone())
                    .collect();
                o.push_str(&format!(
                    "    # edge: {}\n",
                    ep.join(" -> ")
                ));
            }
            GraphStmt::Let(l) => o.push_str(&format!(
                "    {}",
                emit_let_ex(l, 0)
            )),
            GraphStmt::Stmt(s) => o.push_str(&emit_stmt_ex(s, 2)),
            GraphStmt::Item(_) => {}
        }
    }
    o.push_str("  end\n\n");
    o
}

fn type_to_name(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(pt) => pt.path.last().name.clone(),
        _ => "Unknown".into(),
    }
}

fn ex_impl(imp: &ImplDef) -> String {
    let tn = imp
        .trait_ty
        .as_ref()
        .map(|t| ex_module_name(&type_to_name(t)));
    let sn = ex_module_name(&type_to_name(&imp.self_ty));
    let mut o = format!("  defmodule {} do\n", sn);
    if let Some(trait_name) = &tn {
        o.push_str(&format!("    @behaviour {}\n", trait_name));
    }
    for item in &imp.items {
        match item {
            ImplItem::Fn(f) => {
                let fn_ = ex_ident(&f.name.name);
                let ps: Vec<String> = f.params.iter().filter_map(ex_param).collect();
                o.push_str(&format!(
                    "    @impl true\n"
                ));
                o.push_str(&format!(
                    "    def {}({}) do\n",
                    fn_,
                    ps.join(", ")
                ));
                if let Some(body) = &f.body {
                    o.push_str(&emit_block_ex(body, 3));
                }
                o.push_str("    end\n");
            }
            ImplItem::Const(c) => {
                o.push_str(&format!(
                    "    @{} {}\n",
                    ex_ident(&c.name.name),
                    emit_expr_ex(&c.value, 0)
                ));
            }
            ImplItem::TypeAlias(_) => {}
        }
    }
    o.push_str("  end\n\n");
    o
}

fn ex_const(c: &ConstDef) -> String {
    format!(
        "  @{} {}\n\n",
        ex_ident(&c.name.name),
        emit_expr_ex(&c.value, 0)
    )
}

fn ex_typealias(a: &TypeAliasDef) -> String {
    format!(
        "  @type {} :: {}\n\n",
        ex_ident(&a.name.name),
        ex_type(&a.ty)
    )
}

fn ex_macro_rules(m: &MacroRulesDefinition) -> String {
    format!(
        "  # macro_rules {} -- not directly translatable to Elixir\n\n",
        ex_ident(&m.name.name)
    )
}

// ──────────────────────────────────────────────────────
// Expression translation
// ──────────────────────────────────────────────────────

pub fn emit_expr_ex(expr: &Expr, indent: usize) -> String {
    let ind = "  ".repeat(indent);
    match &expr.kind {
        ExprKind::Literal(lit) => ex_literal(lit),
        ExprKind::Path(p) => {
            let name = p.last().name.as_str();
            // Check if it's a module-like path (Enum::Variant -> :Variant)
            if p.segments.len() > 1 {
                format!(":{}", ex_ident(name))
            } else {
                ex_ident(name)
            }
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let l = emit_expr_ex(lhs, 0);
            let r = emit_expr_ex(rhs, 0);
            match op {
                BinaryOp::And => format!("{} and {}", l, r),
                BinaryOp::Or => format!("{} or {}", l, r),
                _ => format!("{} {} {}", l, ex_binop(op), r),
            }
        }
        ExprKind::Unary { op, operand } => format!(
            "{}{}",
            ex_unop(op),
            emit_expr_ex(operand, 0)
        ),
        ExprKind::Call { callee, args } => {
            let callee_str = if let ExprKind::Path(p) = &callee.kind {
                Some(p.last().name.as_str())
            } else {
                None
            };
            match callee_str {
                Some("println") => {
                    if args.is_empty() {
                        r#"IO.puts("")"#.into()
                    } else if args.len() == 1 {
                        if let ExprKind::Literal(lit) = &args[0].kind {
                            if let LiteralKind::Str { value, .. } = &lit.kind {
                                return format!("IO.puts(\"{}\")", value.replace('\\', "\\\\").replace('"', "\\\"").replace("#", "\\#"));
                            }
                        }
                        format!("IO.puts({})", emit_expr_ex(&args[0], 0))
                    } else {
                        format!("IO.puts(#{})", args.iter().map(|a| emit_expr_ex(a, 0)).collect::<Vec<_>>().join(", "))
                    }
                }
                Some("eprintln") => {
                    if args.is_empty() {
                        "IO.puts(:stderr, \"\")".into()
                    } else {
                        format!("IO.puts(:stderr, #{})", args.iter().map(|a| emit_expr_ex(a, 0)).collect::<Vec<_>>().join(", "))
                    }
                }
                Some("format") => {
                    if !args.is_empty() {
                        if let ExprKind::Literal(lit) = &args[0].kind {
                            if let LiteralKind::Str { value, .. } = &lit.kind {
                                return ex_format_string(value, &args[1..]);
                            }
                        }
                    }
                    let as_ = args.iter().map(|a| emit_expr_ex(a, 0)).collect::<Vec<_>>();
                    format!("\"{}\"", as_.join(" <> "))
                }
                Some("vec") => {
                    let as_ = args.iter().map(|a| emit_expr_ex(a, 0)).collect::<Vec<_>>();
                    format!("[{}]", as_.join(", "))
                }
                Some("panic") => {
                    if !args.is_empty() {
                        format!("raise \"{}\"", emit_expr_ex(&args[0], 0).trim_matches('"'))
                    } else {
                        "raise \"panic\"".into()
                    }
                }
                Some("todo") | Some("unimplemented") => {
                    "raise \"not implemented\"".into()
                }
                Some("some") => {
                    if !args.is_empty() {
                        format!("{{:ok, {}}}", emit_expr_ex(&args[0], 0))
                    } else {
                        ":ok".into()
                    }
                }
                Some("none") | Some("err") => {
                    if !args.is_empty() {
                        format!("{{:error, {}}}", emit_expr_ex(&args[0], 0))
                    } else {
                        ":error".into()
                    }
                }
                Some("ok") => {
                    if !args.is_empty() {
                        format!("{{:ok, {}}}", emit_expr_ex(&args[0], 0))
                    } else {
                        ":ok".into()
                    }
                }
                _ => {
                    let as_ = args.iter().map(|a| emit_expr_ex(a, 0)).collect::<Vec<_>>();
                    format!("{}({})", emit_expr_ex(callee, 0), as_.join(", "))
                }
            }
        }
        ExprKind::MethodCall { receiver, method, generic_args: _, args } => {
            let mn = &method.name;
            let recv_str = emit_expr_ex(receiver, 0);
            if let Some(mapped) = ex_std_method(&recv_str, mn, args) {
                return mapped;
            }
            let mn_esc = ex_ident(mn);
            let as_ = args.iter().map(|a| emit_expr_ex(a, 0)).collect::<Vec<_>>();
            // In Elixir, method calls use module.function(receiver, args)
            format!("{}.{}({}, {})", "Kernel", mn_esc, recv_str, as_.join(", "))
        }
        ExprKind::Field { base, field } => {
            let fn_ = match field {
                FieldIndex::Named(id) => ex_ident(&id.name),
                FieldIndex::Index(i, _) => format!("field_{}", i),
            };
            let recv = emit_expr_ex(base, 0);
            // Elixir struct field access: struct.field_name or Map.get
            if fn_.starts_with("field_") {
                format!("elem({}, {})", recv, &fn_[6..])
            } else {
                format!("{}.{}", recv, fn_)
            }
        }
        ExprKind::Index { base, index } => {
            format!("Enum.at({}, {})", emit_expr_ex(base, 0), emit_expr_ex(index, 0))
        }
        ExprKind::Slice { base, range } => {
            let bs = emit_expr_ex(base, 0);
            let s = range
                .lo
                .as_ref()
                .map(|e| emit_expr_ex(e, 0))
                .unwrap_or_else(|| "0".into());
            let e = range
                .hi
                .as_ref()
                .map(|e| emit_expr_ex(e, 0))
                .unwrap_or_else(|| format!("length({})", bs));
            if range.inclusive {
                format!("Enum.slice({}, {}..{})", bs, s, e)
            } else {
                format!("Enum.slice({}, {}..{}", bs, s, e)
            }
        }
        ExprKind::Range(r) => {
            let l = r
                .lo
                .as_ref()
                .map(|e| emit_expr_ex(e, 0))
                .unwrap_or_else(|| "0".into());
            let hi = r
                .hi
                .as_ref()
                .map(|e| emit_expr_ex(e, 0))
                .unwrap_or_else(|| "// range without upper bound".into());
            if r.inclusive {
                format!("{}..{}//", l, hi)
            } else {
                format!("{}..{}", l, hi)
            }
        }
        ExprKind::Assign { lhs, rhs } => {
            // Elixir: variables are immutable, assignment is rebinding
            format!("{} = {}", emit_expr_ex(lhs, 0), emit_expr_ex(rhs, 0))
        }
        ExprKind::CompoundAssign { op, lhs, rhs } => {
            format!(
                "{} = {} {} {}",
                emit_expr_ex(lhs, 0),
                emit_expr_ex(lhs, 0),
                ex_binop(op),
                emit_expr_ex(rhs, 0)
            )
        }
        ExprKind::If { cond, then, else_ } => {
            let mut o = format!("{}if {} do\n", ind, emit_expr_ex(cond, 0));
            o.push_str(&emit_block_ex(then, indent + 1));
            if let Some(els) = else_ {
                o.push_str(&format!("{}end", ind));
                match &els.kind {
                    ExprKind::If { .. } => {
                        o.push_str(&format!(" else "));
                        o.push_str(&emit_expr_ex(els, indent));
                    }
                    ExprKind::Block(b) => {
                        o.push_str(" else\n");
                        o.push_str(&emit_block_ex(b, indent + 1));
                        o.push_str(&format!("{}end\n", ind));
                    }
                    _ => {
                        o.push_str(" else\n");
                        o.push_str(&format!("{}  {}\n", ind, emit_expr_ex(els, 0)));
                        o.push_str(&format!("{}end\n", ind));
                    }
                }
            } else {
                o.push_str(&format!("{}end\n", ind));
            }
            o
        }
        ExprKind::IfLet { pattern, expr: scrut, then, else_ } => {
            let mut o = format!(
                "{}case {} do\n  {} ->\n",
                ind,
                emit_expr_ex(scrut, 0),
                ex_pattern(pattern)
            );
            o.push_str(&emit_block_ex(then, indent + 2));
            if let Some(els) = else_ {
                o.push_str(&format!("{}  _ ->\n", ind));
                match &els.kind {
                    ExprKind::Block(b) => {
                        o.push_str(&emit_block_ex(b, indent + 2));
                    }
                    _ => {
                        o.push_str(&format!(
                            "{}    {}\n",
                            ind,
                            emit_expr_ex(els, 0)
                        ));
                    }
                }
            }
            o.push_str(&format!("{}end\n", ind));
            o
        }
        ExprKind::Match { scrutinee, arms } => {
            let mut o = format!("{}case {} do\n", ind, emit_expr_ex(scrutinee, 0));
            for arm in arms {
                let p = ex_pattern(&arm.pattern);
                let g = if let Some(g) = &arm.guard {
                    format!(" when {}", emit_expr_ex(g, 0))
                } else {
                    String::new()
                };
                match &arm.body.kind {
                    ExprKind::Block(b) => {
                        o.push_str(&format!(
                            "  {}{} ->\n",
                            p, g
                        ));
                        o.push_str(&emit_block_ex(b, indent + 2));
                    }
                    _ => {
                        o.push_str(&format!(
                            "  {}{} -> {}\n",
                            p,
                            g,
                            emit_expr_ex(&arm.body, 0)
                        ));
                    }
                }
            }
            o.push_str(&format!("{}end\n", ind));
            o
        }
        ExprKind::For { label: _, pattern, iter, body } => {
            let mut o = format!(
                "{}Enum.each({})\n",
                ind,
                emit_expr_ex(iter, 0)
            );
            o.push_str(&format!("{}  fn {} ->\n", ind, ex_pattern(pattern)));
            o.push_str(&emit_block_ex(body, indent + 2));
            o.push_str(&format!("{}  end\n", ind));
            o
        }
        ExprKind::While { label: _, cond, body } => {
            let mut o = format!("{}# while loop\n", ind);
            o.push_str(&format!("{}defp while_loop() do\n", ind));
            o.push_str(&format!(
                "{}  if {} do\n",
                ind,
                emit_expr_ex(cond, 0)
            ));
            o.push_str(&emit_block_ex(body, indent + 2));
            o.push_str(&format!("{}    while_loop()\n", ind));
            o.push_str(&format!("{}  end\n", ind));
            o.push_str(&format!("{}end\n", ind));
            o.push_str(&format!("{}while_loop()\n", ind));
            o
        }
        ExprKind::WhileLet { label: _, pattern, expr: scrut, body } => {
            let mut o = format!("{}# while let loop\n", ind);
            o.push_str(&format!("{}defp while_let_loop() do\n", ind));
            o.push_str(&format!(
                "{}  case {} do\n",
                ind,
                emit_expr_ex(scrut, 0)
            ));
            o.push_str(&format!(
                "{}    {} ->\n",
                ind,
                ex_pattern(pattern)
            ));
            o.push_str(&emit_block_ex(body, indent + 3));
            o.push_str(&format!("{}    while_let_loop()\n", ind));
            o.push_str(&format!("{}    _ -> :ok\n", ind));
            o.push_str(&format!("{}  end\n", ind));
            o.push_str(&format!("{}end\n", ind));
            o.push_str(&format!("{}while_let_loop()\n", ind));
            o
        }
        ExprKind::Loop { label, body } => {
            let mut o = String::new();
            if let Some(l) = label {
                o.push_str(&format!("{}# label: {}\n", ind, ex_ident(&l.name)));
            }
            o.push_str(&format!("{}# infinite loop\n", ind));
            o.push_str(&format!("{}defp loop_body() do\n", ind));
            o.push_str(&emit_block_ex(body, indent + 1));
            o.push_str(&format!("{}  loop_body()\n", ind));
            o.push_str(&format!("{}end\n", ind));
            o.push_str(&format!("{}loop_body()\n", ind));
            o
        }
        ExprKind::Closure { params, body, ret: _, .. } => {
            let ps: Vec<String> = params
                .iter()
                .filter_map(|p| match &p.kind {
                    ParamKind::Pattern(pat) => match &pat.kind {
                        PatternKind::Ident { name, .. } => Some(ex_ident(&name.name)),
                        _ => Some("_x".into()),
                    },
                    _ => None,
                })
                .collect();
            if let ExprKind::Block(be) = &body.kind {
                if be.stmts.is_empty() {
                    if let Some(tail) = &be.tail {
                        return format!(
                            "fn {} -> {} end",
                            ps.join(", "),
                            emit_expr_ex(tail, 0)
                        );
                    }
                }
                let mut o = format!("fn {} ->\n", ps.join(", "));
                o.push_str(&emit_block_ex(be, indent + 1));
                o.push_str(&format!("{}end", ind));
                o
            } else {
                format!(
                    "fn {} -> {} end",
                    ps.join(", "),
                    emit_expr_ex(body, 0)
                )
            }
        }
        ExprKind::Return(value) => {
            if let Some(v) = value {
                format!("# return {}", emit_expr_ex(v, 0))
            } else {
                "# return".into()
            }
        }
        ExprKind::Break { label, value } => {
            let mut o = "# break".to_string();
            if let Some(l) = label {
                o = format!("# break from {}", ex_ident(&l.name));
            }
            if let Some(v) = value {
                o.push_str(&format!(" # value: {}", emit_expr_ex(v, 0)));
            }
            o
        }
        ExprKind::Continue { label } => {
            if let Some(l) = label {
                format!("# continue {}", ex_ident(&l.name))
            } else {
                "# continue".into()
            }
        }
        ExprKind::Array(elems) => {
            let es: Vec<String> = elems.iter().map(|e| emit_expr_ex(e, 0)).collect();
            format!("[{}]", es.join(", "))
        }
        ExprKind::ArrayRepeat { elem, count } => {
            format!(
                "List.duplicate({}, {})",
                emit_expr_ex(elem, 0),
                emit_expr_ex(count, 0)
            )
        }
        ExprKind::Struct { path, fields, spread } => {
            let mod_name = ex_module_name(&path.last().name);
            let fs: Vec<String> = fields
                .iter()
                .map(|f| {
                    let fn_ = match &f.name {
                        FieldIndex::Named(id) => format!(":{}", ex_ident(&id.name)),
                        FieldIndex::Index(i, _) => format!(":field_{}", i),
                    };
                    let v = f
                        .value
                        .as_ref()
                        .map(|v| emit_expr_ex(v, 0))
                        .unwrap_or_else(|| fn_.clone());
                    format!("{}: {}", fn_, v)
                })
                .collect();
            let spread_str = if let Some(spread) = spread {
                format!(" | {}", emit_expr_ex(spread, 0))
            } else {
                String::new()
            };
            format!("%{}{{{}{}{}}}", mod_name, fs.join(", "), if fs.is_empty() { "" } else { ", " }, spread_str)
        }
        ExprKind::Tuple(elems) => {
            let es: Vec<String> = elems.iter().map(|e| emit_expr_ex(e, 0)).collect();
            match es.len() {
                0 => ":ok".into(),
                _ => format!("{{{}}}", es.join(", ")),
            }
        }
        ExprKind::Block(be) => {
            let mut o = "(\n".to_string();
            o.push_str(&emit_block_ex(be, indent + 1));
            if let Some(tail) = &be.tail {
                o.push_str(&format!(
                    "{}{}\n",
                    ind,
                    emit_expr_ex(tail, 0)
                ));
            }
            o.push_str(&format!("{} )", ind));
            o
        }
        ExprKind::AsyncBlock { body, .. } => {
            let mut o = "# async ".to_string();
            o.push_str("(\n");
            o.push_str(&emit_block_ex(body, indent + 1));
            if let Some(tail) = &body.tail {
                o.push_str(&format!(
                    "{}{}\n",
                    ind,
                    emit_expr_ex(tail, 0)
                ));
            }
            o.push_str(&format!("{} )", ind));
            o
        }
        ExprKind::Try(inner) => {
            let inner_str = emit_expr_ex(inner, 0);
            format!("case {} do {{ :ok, v }} -> v; {{ :error, e }} -> raise e end", inner_str)
        }
        ExprKind::Await(inner) => {
            format!("Task.await({})", emit_expr_ex(inner, 0))
        }
        ExprKind::Cast { expr: inner, ty } => {
            // Elixir doesn't have explicit casts; comment and pass through
            format!("# cast to {}\n{}", ex_type(ty), emit_expr_ex(inner, 0))
        }
        ExprKind::Macro { path, args: _ } => ex_macro(path.last().name.as_str()),
        ExprKind::Native(nb) => {
            if nb.lang.name == "elixir" {
                nb.code.clone()
            } else {
                "# native block".into()
            }
        }
    }
}

fn ex_literal(lit: &Literal) -> String {
    match &lit.kind {
        LiteralKind::Str { value, .. } => {
            format!(
                "\"{}\"",
                value
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace("#", "\\#")
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
        LiteralKind::Char(c) => format!("?{}", c),
    }
}

/// Convert Rust format! string to Elixir interpolation "...#{expr}..."
fn ex_format_string(fmt: &str, args: &[Expr]) -> String {
    let mut result = String::from("\"");
    let mut arg_idx = 0;
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' && chars.peek() == Some(&'{') {
            chars.next();
            result.push_str("#{");
            if arg_idx < args.len() {
                result.push_str(&emit_expr_ex(&args[arg_idx], 0));
                arg_idx += 1;
            }
            while let Some(nc) = chars.next() {
                if nc == '}' && chars.peek() == Some(&'}') {
                    chars.next();
                    break;
                }
            }
            result.push('}');
        } else if c == '}' && chars.peek() == Some(&'}') {
            chars.next();
        } else if c == '}' {
            result.push('}');
        } else {
            result.push(c);
        }
    }
    result.push('"');
    result
}

fn ex_binop(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Rem => "rem",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Gt => ">",
        BinaryOp::Le => "<=",
        BinaryOp::Ge => ">=",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        BinaryOp::BitAnd => "band",
        BinaryOp::BitOr => "bor",
        BinaryOp::BitXor => "bxor",
        BinaryOp::Shl => "bsl",
        BinaryOp::Shr => "bsr",
    }
}

fn ex_unop(op: &UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Not => "not ",
        UnaryOp::Deref | UnaryOp::Ref | UnaryOp::RefMut => "",
    }
}

fn ex_pattern(pat: &Pattern) -> String {
    match &pat.kind {
        PatternKind::Wildcard => "_".into(),
        PatternKind::Ident { name, .. } => ex_ident(&name.name),
        PatternKind::Literal(lit) => ex_literal(lit),
        PatternKind::Path(p) => {
            let name = p.last().name.as_str();
            if p.segments.len() > 1 {
                format!(":{}", ex_ident(name))
            } else {
                ex_ident(name)
            }
        }
        PatternKind::TupleStruct { path, elems, .. } => {
            let n = path.last().name.as_str();
            let es: Vec<String> = elems.iter().map(ex_pattern).collect();
            if es.is_empty() {
                format!(":{}", ex_ident(n))
            } else {
                format!(":{}, {{{}}}", ex_ident(n), es.join(", "))
            }
        }
        PatternKind::Struct { path, fields, .. } => {
            let n = path.last().name.as_str();
            let fs: Vec<String> = fields
                .iter()
                .map(|f| {
                    let fn_ = ex_ident(&f.name.name);
                    let p = f
                        .pattern
                        .as_ref()
                        .map(|p| ex_pattern(p))
                        .unwrap_or_else(|| ex_ident(&f.name.name));
                    format!("{}: {}", fn_, p)
                })
                .collect();
            if fs.is_empty() {
                format!("%{}{{}}", ex_module_name(n))
            } else {
                format!("%{}{{{}}}", ex_module_name(n), fs.join(", "))
            }
        }
        PatternKind::Tuple { elems, .. } => {
            let es: Vec<String> = elems.iter().map(ex_pattern).collect();
            format!("{{{}}}", es.join(", "))
        }
        PatternKind::Or(elems) => elems
            .iter()
            .map(|e| ex_pattern(e))
            .collect::<Vec<_>>()
            .join(" | "),
        PatternKind::Range { lo, hi, inclusive } => {
            let l = ex_pattern(lo);
            let r = ex_pattern(hi);
            if *inclusive {
                format!("{} in {}..{}//", l, l, r)
            } else {
                format!("{} in {}..{}", l, l, r)
            }
        }
        PatternKind::Rest => "_".into(),
    }
}

fn ex_macro(name: &str) -> String {
    match name {
        "println" => "IO.puts".into(),
        "eprintln" => "IO.puts(:stderr, ...)".into(),
        "format" => "\"\"".into(),
        "todo" | "unimplemented" => "raise \"not implemented\"".into(),
        "panic" => "raise".into(),
        "vec" => "[]".into(),
        _ => format!("# macro: {}", name),
    }
}

// ──────────────────────────────────────────────────────
// Statement block translation
// ──────────────────────────────────────────────────────

pub fn emit_block_ex(be: &BlockExpr, indent: usize) -> String {
    let ind = "  ".repeat(indent);
    let mut o = String::new();
    for stmt in &be.stmts {
        match stmt {
            Stmt::Let(l) => o.push_str(&format!("{}{}", ind, emit_let_ex(l, 0))),
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
                        o.push_str(&format!("{}\n", emit_expr_ex(expr, indent)));
                    }
                    ExprKind::Return(_) => {
                        o.push_str(&format!(
                            "{}{}\n",
                            ind,
                            emit_expr_ex(expr, 0)
                        ));
                    }
                    _ => {
                        o.push_str(&format!(
                            "{}{}\n",
                            ind,
                            emit_expr_ex(expr, 0)
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
                o.push_str(&format!("{}", emit_expr_ex(tail, indent)));
            }
            _ => {
                o.push_str(&format!(
                    "{}{}\n",
                    ind,
                    emit_expr_ex(tail, 0)
                ));
            }
        }
    }
    o
}

fn emit_let_ex(l: &LetStmt, _indent: usize) -> String {
    let pat = ex_pattern(&l.pattern);
    if l.mutable {
        // Elixir: use process dictionary or Agent for mutable state
        // For generated code, just note the mutability
        if let Some(ty) = &l.ty {
            format!(
                "# mutable {} :: {} = {}\n",
                pat,
                ex_type(ty),
                l.init
                    .as_ref()
                    .map(|e| emit_expr_ex(e, 0))
                    .unwrap_or_else(|| "nil".into())
            )
        } else {
            format!(
                "# mutable {} = {}\n",
                pat,
                l.init
                    .as_ref()
                    .map(|e| emit_expr_ex(e, 0))
                    .unwrap_or_else(|| "nil".into())
            )
        }
    } else {
        let _ty = &l.ty;
        format!(
            "{} = {}\n",
            pat,
            l.init
                .as_ref()
                .map(|e| emit_expr_ex(e, 0))
                .unwrap_or_else(|| "nil".into())
        )
    }
}

pub fn emit_stmt_ex(stmt: &Stmt, indent: usize) -> String {
    match stmt {
        Stmt::Let(l) => {
            let ind = "  ".repeat(indent);
            format!("{}{}", ind, emit_let_ex(l, 0))
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
                | ExprKind::IfLet { .. } => emit_expr_ex(expr, indent) + "\n",
                _ => {
                    let ind = "  ".repeat(indent);
                    format!("{}{}\n", ind, emit_expr_ex(expr, 0))
                }
            }
        }
    }
}