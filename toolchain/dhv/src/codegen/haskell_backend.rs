//! Haskell backend (Logic tier) -- type mapping + full function body translation
//! Haskell (GHC 9.x) code generation.
//! struct -> data Name { field :: Type } deriving (Show, Eq) (named)
//! struct -> type Name = (T1, T2) (tuple)
//! struct -> data Name = Name deriving (Show, Eq) (unit)
//! enum (unit) -> data Name = V1 | V2 | V3 deriving (Show, Eq, Bounded, Enum)
//! enum (data) -> data Name = V1 T1 | V2 { field :: T } deriving (Show, Eq)
//! trait -> class TraitName a where (type class)
//! impl -> instance TraitName ConcreteType where
//! fn -> top-level functionName :: Type1 -> Type2 -> ReturnType
//! const -> constName :: Type  +  constName = value
//! graph -> main :: IO ()  +  main = do ...

use crate::ast::*;
use crate::codegen::{CodegenBackend, CodegenContext};

pub struct HaskellBackend;

impl CodegenBackend for HaskellBackend {
    fn lang(&self) -> &'static str {
        "haskell"
    }

    fn emit_item(&self, ctx: &CodegenContext, item: &Item) -> Result<String, String> {
        let mut out = String::new();
        out.push_str(&format!("-- {}\n", crate::sourcemap::generated_header("haskell")));
        out.push_str("-- HSL-generated Haskell code -- do not edit manually\n\n");

        // Module header with common imports
        out.push_str(MODULE_HEADER);

        match item {
            Item::Struct(s) => out.push_str(&hs_struct(s)),
            Item::Enum(e) => out.push_str(&hs_enum(e)),
            Item::Trait(t) => out.push_str(&hs_trait(t)),
            Item::Fn(f) => out.push_str(&hs_fn(f)),
            Item::Graph(g) => out.push_str(&hs_graph(g, ctx)),
            Item::Impl(imp) => out.push_str(&hs_impl(imp)),
            Item::Const(c) => out.push_str(&hs_const(c)),
            Item::TypeAlias(a) => out.push_str(&hs_typealias(a)),
            Item::MacroRules(m) => out.push_str(&hs_macro_rules(m)),
            _ => {
                return Err(format!(
                    "haskell backend does not support {}",
                    item_kind_name(item)
                ))
            }
        }
        Ok(out)
    }
}

// ──────────────────────────────────────────────────────
// Module header with standard imports
// ──────────────────────────────────────────────────────

const MODULE_HEADER: &str = "{-# LANGUAGE LambdaCase #-}\n\
{-# LANGUAGE RecordWildCards #-}\n\
module HSLGenerated where\n\
\n\
import Data.Map.Strict (Map)\n\
import qualified Data.Map.Strict as Map\n\
import Data.Set (Set)\n\
import qualified Data.Set as Set\n\
import Data.List (sort, reverse, find, isPrefixOf, isSuffixOf, intercalate, splitOn)\n\
import Data.Char (toLower, toUpper)\n\
import Data.Maybe (isJust, isNothing, fromJust, fromMaybe, mapMaybe, catMaybes, maybeToList)\n\
import Control.Monad (forM_, mapM_, forever, void, when, unless, guard)\n\
import System.IO (hPutStrLn, stderr)\n\
import Text.Read (readMaybe)\n\
\n";

// ──────────────────────────────────────────────────────
// Haskell keywords to avoid
// ──────────────────────────────────────────────────────

const HS_KW: &[&str] = &[
    "case", "class", "data", "default", "deriving", "do", "else", "if",
    "import", "in", "infixl", "infixr", "infix", "instance", "let",
    "module", "newtype", "of", "then", "type", "where", "qualified",
    "as", "hiding", "forall", "family", "pattern",
];

fn hs_ident(s: &str) -> String {
    if HS_KW.contains(&s) {
        format!("{}'", s)
    } else {
        s.to_string()
    }
}

// ──────────────────────────────────────────────────────
// Type mapping
// ──────────────────────────────────────────────────────

fn hs_generic_arg(arg: &GenericArg) -> String {
    match arg {
        GenericArg::Type(t) => hs_type(t),
        GenericArg::Const(c) => match &c.kind {
            ConstArgKind::Literal(lit) => lit.raw.clone(),
            ConstArgKind::Block(_) => "0".into(),
        },
    }
}

fn hs_two_generic_args(args: &[GenericArg]) -> (String, String) {
    let mut it = args.iter();
    (
        it.next()
            .map(hs_generic_arg)
            .unwrap_or_else(|| "k".into()),
        it.next()
            .map(hs_generic_arg)
            .unwrap_or_else(|| "v".into()),
    )
}

fn hs_type(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(pt) => {
            let name = pt.path.last().name.as_str();
            match name {
                "String" | "str" => "String".into(),
                "char" => "Char".into(),
                "bool" => "Bool".into(),
                "i8" | "u8" => "Int".into(),
                "i16" | "u16" => "Int".into(),
                "i32" | "u32" | "usize" | "isize" => "Int".into(),
                "i64" => "Int64".into(),
                "u64" => "Word64".into(),
                "i128" | "u128" => "Integer".into(),
                "f32" => "Float".into(),
                "f64" => "Double".into(),
                "Vec" => format!(
                    "[{}]",
                    pt.generic_args
                        .iter()
                        .next()
                        .map(hs_generic_arg)
                        .unwrap_or_else(|| "a".into())
                ),
                "HashMap" | "BTreeMap" => {
                    let (k, v) = hs_two_generic_args(&pt.generic_args);
                    format!("Map {} {}", k, v)
                }
                "HashSet" | "BTreeSet" => format!(
                    "Set {}",
                    pt.generic_args
                        .iter()
                        .next()
                        .map(hs_generic_arg)
                        .unwrap_or_else(|| "a".into())
                ),
                "Option" => format!(
                    "Maybe {}",
                    pt.generic_args
                        .iter()
                        .next()
                        .map(hs_generic_arg)
                        .unwrap_or_else(|| "a".into())
                ),
                "Result" => {
                    // Result<T, E> -> Either E T (Haskell Either is right-biased for success)
                    if pt.generic_args.len() >= 2 {
                        let t = hs_generic_arg(&pt.generic_args[0]);
                        let e = hs_generic_arg(&pt.generic_args[1]);
                        format!("Either {} {}", e, t)
                    } else if !pt.generic_args.is_empty() {
                        hs_generic_arg(&pt.generic_args[0])
                    } else {
                        "(Either String a)".into()
                    }
                }
                "Box" => {
                    if !pt.generic_args.is_empty() {
                        hs_generic_arg(&pt.generic_args[0])
                    } else {
                        "a".into()
                    }
                }
                "unit" => "()".into(),
                _ => hs_ident(name),
            }
        }
        TypeKind::Ref { inner, .. } => hs_type(inner),
        TypeKind::Tuple(elems) => {
            if elems.is_empty() {
                "()".into()
            } else {
                let es: Vec<String> = elems.iter().map(hs_type).collect();
                format!("({})", es.join(", "))
            }
        }
        TypeKind::Array { elem, .. } => format!("[{}]", hs_type(elem)),
        TypeKind::Slice(inner) => format!("[{}]", hs_type(inner)),
        TypeKind::Paren(inner) => hs_type(inner),
        TypeKind::Never => "forall a. a".into(),
        TypeKind::FnPtr { params, ret } => {
            let r = ret
                .as_ref()
                .map(|t| hs_type(t))
                .unwrap_or_else(|| "()".into());
            if params.is_empty() {
                format!("() -> {}", r)
            } else {
                format!(
                    "{} -> {}",
                    params.iter().map(hs_type).collect::<Vec<_>>().join(" -> "),
                    r
                )
            }
        }
        TypeKind::DynTrait(_) | TypeKind::ImplTrait(_) | TypeKind::Infer => {
            "a".into()
        }
    }
}

// ──────────────────────────────────────────────────────
// Standard library method mapping
// ──────────────────────────────────────────────────────

/// Map Rust/Vec std method names to Haskell equivalents
fn hs_std_method(receiver: &str, method: &str, args: &[Expr]) -> Option<String> {
    let args_str: Vec<String> = args.iter().map(|a| hs_emit_expr(a, 0)).collect();
    match method {
        // Vec/List methods
        "push" | "append" => {
            if args_str.len() == 1 {
                Some(format!("({} ++ [{}])", receiver, args_str[0]))
            } else {
                None
            }
        }
        "pop" => Some(format!("init {}", receiver)),
        "len" | "length" => Some(format!("length {}", receiver)),
        "is_empty" => Some(format!("null {}", receiver)),
        "sort" => Some(format!("sort {}", receiver)),
        "sorted" => Some(format!("sort {}", receiver)),
        "reverse" => Some(format!("reverse {}", receiver)),
        "map" => {
            if args_str.len() == 1 {
                Some(format!("map ({}) {}", args_str[0], receiver))
            } else {
                None
            }
        }
        "filter" => {
            if args_str.len() == 1 {
                Some(format!("filter ({}) {}", args_str[0], receiver))
            } else {
                None
            }
        }
        "fold" => {
            if args_str.len() == 2 {
                Some(format!("foldl ({}) {} {}", args_str[1], args_str[0], receiver))
            } else {
                None
            }
        }
        "for_each" | "foreach" => {
            if args_str.len() == 1 {
                Some(format!("mapM_ ({}) {}", args_str[0], receiver))
            } else {
                None
            }
        }
        "find" => {
            if args_str.len() == 1 {
                Some(format!("find ({}) {}", args_str[0], receiver))
            } else {
                None
            }
        }
        "any" | "exists" => {
            if args_str.len() == 1 {
                Some(format!("any ({}) {}", args_str[0], receiver))
            } else {
                None
            }
        }
        "all" | "forall" => {
            if args_str.len() == 1 {
                Some(format!("all ({}) {}", args_str[0], receiver))
            } else {
                None
            }
        }
        "flat_map" | "flatMap" => {
            if args_str.len() == 1 {
                Some(format!("concatMap ({}) {}", args_str[0], receiver))
            } else {
                None
            }
        }
        "collect" => Some(format!("concat {}", receiver)),
        "contains" => {
            if args_str.len() == 1 {
                Some(format!("elem {} {}", args_str[0], receiver))
            } else {
                None
            }
        }
        "iter" => Some(receiver.to_string()),
        // String methods
        "to_string" | "toString" => Some(format!("show {}", receiver)),
        "trim" => Some(format!("strip {}", receiver)),
        "to_lowercase" | "toLowerCase" => Some(format!("map toLower {}", receiver)),
        "to_uppercase" | "toUpperCase" => Some(format!("map toUpper {}", receiver)),
        "starts_with" | "startsWith" => {
            if args_str.len() == 1 {
                Some(format!("isPrefixOf {} {}", args_str[0], receiver))
            } else {
                None
            }
        }
        "ends_with" | "endsWith" => {
            if args_str.len() == 1 {
                Some(format!("isSuffixOf {} {}", args_str[0], receiver))
            } else {
                None
            }
        }
        "split" => {
            if args_str.len() == 1 {
                Some(format!("splitOn {} {}", args_str[0], receiver))
            } else {
                None
            }
        }
        "replace" => {
            if args_str.len() == 2 {
                Some(format!(
                    "Data.List.intercalate [{}] $ splitOn [{}] {}",
                    args_str[1], args_str[0], receiver
                ))
            } else {
                None
            }
        }
        "chars" => Some(format!("{} :: String", receiver)),
        // Option/Maybe methods
        "is_some" => Some(format!("isJust {}", receiver)),
        "is_none" => Some(format!("isNothing {}", receiver)),
        "unwrap" | "get" => Some(format!("fromJust {}", receiver)),
        "expect" => {
            if args_str.len() == 1 {
                Some(format!(
                    "fromMaybe (error {}) {}",
                    args_str[0], receiver
                ))
            } else {
                Some(format!("fromJust {}", receiver))
            }
        }
        "and_then" => {
            if args_str.len() == 1 {
                Some(format!("({} >>= {})", receiver, args_str[0]))
            } else {
                None
            }
        }
        "unwrap_or" | "getOrElse" | "or_else" => {
            if args_str.len() == 1 {
                Some(format!("fromMaybe {} {}", args_str[0], receiver))
            } else {
                None
            }
        }
        "ok" | "toOption" => Some(format!("either (const Nothing) Just {}", receiver)),
        // Result/Either methods
        "is_ok" => Some(format!("isRight {}", receiver)),
        "is_err" => Some(format!("isLeft {}", receiver)),
        "err" => Some(format!("fromLeft (error \"fromLeft on Right\") {}", receiver)),
        // Map methods
        "insert" | "updated" => {
            if args_str.len() == 2 {
                Some(format!("Map.insert {} {} {}", args_str[0], args_str[1], receiver))
            } else {
                None
            }
        }
        "keys" => Some(format!("Map.keys {}", receiver)),
        "values" => Some(format!("Map.elems {}", receiver)),
        "remove" | "delete" => {
            if args_str.len() == 1 {
                Some(format!("Map.delete {} {}", args_str[0], receiver))
            } else {
                None
            }
        }
        "join" => Some(format!("concat {}", receiver)),
        _ => None,
    }
}

// ──────────────────────────────────────────────────────
// Parameters
// ──────────────────────────────────────────────────────

fn hs_param(p: &Param) -> Option<String> {
    match &p.kind {
        ParamKind::Self_(_) => None,
        ParamKind::Pattern(pat) => {
            let name = match &pat.kind {
                PatternKind::Ident { name, .. } => hs_ident(&name.name),
                _ => "arg".into(),
            };
            Some(format!("{} :: {}", name, hs_type(&p.ty)))
        }
    }
}

// ──────────────────────────────────────────────────────
// Item emitters
// ──────────────────────────────────────────────────────

fn hs_struct(s: &StructDef) -> String {
    let name = hs_ident(&s.name.name);
    match &s.kind {
        StructKind::Named(fields) => {
            let fs: Vec<String> = fields
                .iter()
                .map(|f| {
                    let fn_ = f
                        .name
                        .as_ref()
                        .map(|n| hs_ident(&n.name))
                        .unwrap_or_else(|| "field".into());
                    format!("    {} :: {}", fn_, hs_type(&f.ty))
                })
                .collect();
            format!(
                "data {} = {}\n    {{ {}\n    }}\n    deriving (Show, Eq)\n\n",
                name,
                name,
                fs.join(",\n")
            )
        }
        StructKind::Tuple(fields) => {
            let fs: Vec<String> = fields.iter().map(|f| hs_type(&f.ty)).collect();
            format!(
                "type {} = ({})\n\n",
                name,
                fs.join(", ")
            )
        }
        StructKind::Unit => {
            format!(
                "data {} = {}\n    deriving (Show, Eq)\n\n",
                name, name
            )
        }
    }
}

fn hs_enum(e: &EnumDef) -> String {
    let name = hs_ident(&e.name.name);
    let has_data = e
        .variants
        .iter()
        .any(|v| !matches!(&v.fields, StructKind::Unit));
    if !has_data {
        // Simple enum -> data with Bounded, Enum
        let vs: Vec<String> = e
            .variants
            .iter()
            .map(|v| hs_ident(&v.name.name))
            .collect();
        format!(
            "data {} = {}\n    deriving (Show, Eq, Bounded, Enum)\n\n",
            name,
            vs.join(" | ")
        )
    } else {
        // Enum with data variants
        let vs: Vec<String> = e
            .variants
            .iter()
            .map(|v| {
                let vn = hs_ident(&v.name.name);
                match &v.fields {
                    StructKind::Unit => vn,
                    StructKind::Named(fields) => {
                        let fs: Vec<String> = fields
                            .iter()
                            .map(|f| {
                                let fn_ = f
                                    .name
                                    .as_ref()
                                    .map(|n| hs_ident(&n.name))
                                    .unwrap_or_else(|| "field".into());
                                format!("{} :: {}", fn_, hs_type(&f.ty))
                            })
                            .collect();
                        format!("{} {{ {} }}", vn, fs.join(", "))
                    }
                    StructKind::Tuple(fields) => {
                        let fs: Vec<String> = fields.iter().map(|f| hs_type(&f.ty)).collect();
                        if fs.is_empty() {
                            vn
                        } else {
                            format!("{} {}", vn, fs.join(" "))
                        }
                    }
                }
            })
            .collect();
        format!(
            "data {}\n    = {}\n    deriving (Show, Eq)\n\n",
            name,
            vs.join("\n    | ")
        )
    }
}

fn hs_trait(t: &TraitDef) -> String {
    let name = hs_ident(&t.name.name);
    let mut o = format!("class {} a where\n", name);
    for ti in &t.items {
        match ti {
            TraitItem::FnSig(sig) => {
                // Type class method signature with type variable for self
                let ps: Vec<String> = sig
                    .params
                    .iter()
                    .filter_map(hs_param)
                    .collect();
                let all_params = if ps.is_empty() {
                    "a".into()
                } else {
                    format!("a -> {}", ps.join(" -> "))
                };
                let r = sig
                    .ret
                    .as_ref()
                    .map(|t| format!(" -> {}", hs_type(t)))
                    .unwrap_or_default();
                o.push_str(&format!(
                    "    {} :: {}{}\n",
                    hs_ident(&sig.name.name),
                    all_params,
                    r
                ));
            }
            TraitItem::Fn(f) => {
                // Default implementation in type class
                let ps: Vec<String> = f.params.iter().filter_map(hs_param).collect();
                let all_params = if ps.is_empty() {
                    "a".into()
                } else {
                    format!("a -> {}", ps.join(" -> "))
                };
                let r = f
                    .ret
                    .as_ref()
                    .map(|t| format!(" -> {}", hs_type(t)))
                    .unwrap_or_default();
                o.push_str(&format!(
                    "    {} :: {}{}\n",
                    hs_ident(&f.name.name),
                    all_params,
                    r
                ));
                o.push_str(&format!(
                    "    {} val{}\n",
                    hs_ident(&f.name.name),
                    if ps.is_empty() { "" } else { " params" }
                ));
                if let Some(body) = &f.body {
                    o.push_str(&hs_emit_block(body, 2));
                }
                o.push_str("\n");
            }
            TraitItem::Const(_) | TraitItem::TypeAlias(_) => {}
        }
    }
    o.push_str("\n");
    o
}

fn hs_fn(f: &FnDef) -> String {
    let name = hs_ident(&f.name.name);
    let ps: Vec<String> = f.params.iter().filter_map(hs_param).collect();
    let param_names: Vec<String> = f
        .params
        .iter()
        .filter_map(|p| match &p.kind {
            ParamKind::Pattern(pat) => match &pat.kind {
                PatternKind::Ident { name, .. } => Some(hs_ident(&name.name)),
                _ => Some("arg".into()),
            },
            _ => None,
        })
        .collect();
    let ret = f
        .ret
        .as_ref()
        .map(|t| format!(" -> {}", hs_type(t)))
        .unwrap_or_default();
    let mut o = String::new();
    if f.is_async {
        o.push_str("-- async ");
    }
    // Type signature
    o.push_str(&format!(
        "{} :: {}{}\n",
        name,
        if ps.is_empty() { "()".into() } else { ps.join(" -> ") },
        ret
    ));
    // Function definition
    o.push_str(&format!(
        "{} {} = \n",
        name,
        if param_names.is_empty() {
            "()".into()
        } else {
            param_names.join(" ")
        }
    ));
    if let Some(body) = &f.body {
        o.push_str(&hs_emit_block(body, 1));
    } else {
        o.push_str("  undefined\n");
    }
    o.push_str("\n");
    o
}

fn hs_graph(g: &GraphDef, ctx: &CodegenContext) -> String {
    let _gn = hs_ident(&g.name.name);
    let mut o = format!(
        "-- graph {} -- scale: {:?}\n",
        g.name.name, ctx.scale
    );
    o.push_str("main :: IO ()\n");
    o.push_str("main = do\n");
    for gs in &g.body {
        match gs {
            GraphStmt::Node(n) => o.push_str(&format!(
                "  -- node {}: {}\n",
                hs_ident(&n.name.name),
                hs_type(&n.ty)
            )),
            GraphStmt::Edge(e) => {
                let ep: Vec<String> = e
                    .endpoints
                    .iter()
                    .map(|p| p.last().name.clone())
                    .collect();
                o.push_str(&format!(
                    "  -- edge: {}\n",
                    ep.join(" -> ")
                ));
            }
            GraphStmt::Let(l) => o.push_str(&format!(
                "  {}",
                hs_emit_let(l, 0)
            )),
            GraphStmt::Stmt(s) => o.push_str(&hs_emit_stmt(s, 2)),
            GraphStmt::Item(_) => {}
        }
    }
    o.push_str("\n");
    o
}

fn hs_impl(imp: &ImplDef) -> String {
    let tn = imp
        .trait_ty
        .as_ref()
        .map(|t| hs_type(t))
        .unwrap_or_default();
    let sn = hs_type(&imp.self_ty);
    let mut o = if !tn.is_empty() {
        format!("instance {} {} where\n", tn, sn)
    } else {
        // inherent impl -> just emit standalone functions
        format!("-- inherent impl for {}\n", sn)
    };
    for item in &imp.items {
        match item {
            ImplItem::Fn(f) => {
                let fn_ = hs_ident(&f.name.name);
                let r = f
                    .ret
                    .as_ref()
                    .map(|t| format!(" -> {}", hs_type(t)))
                    .unwrap_or_default();
                // Filter out self param for instance method
                let ps: Vec<String> = f
                    .params
                    .iter()
                    .filter_map(|p| match &p.kind {
                        ParamKind::Self_(_) => None,
                        ParamKind::Pattern(pat) => match &pat.kind {
                            PatternKind::Ident { name, .. } => {
                                Some(format!("{} :: {}", hs_ident(&name.name), hs_type(&p.ty)))
                            }
                            _ => Some(format!("arg :: {}", hs_type(&p.ty))),
                        },
                    })
                    .collect();
                let param_names: Vec<String> = f
                    .params
                    .iter()
                    .filter_map(|p| match &p.kind {
                        ParamKind::Self_(_) => None,
                        ParamKind::Pattern(pat) => match &pat.kind {
                            PatternKind::Ident { name, .. } => Some(hs_ident(&name.name)),
                            _ => Some("arg".into()),
                        },
                    })
                    .collect();
                o.push_str(&format!(
                    "    {} :: {}{}\n",
                    fn_,
                    if ps.is_empty() { sn.clone() } else { format!("{} -> {}", sn, ps.join(" -> ")) },
                    r
                ));
                o.push_str(&format!(
                    "    {} {}\n",
                    fn_,
                    if param_names.is_empty() {
                        "val".into()
                    } else {
                        format!("val {}", param_names.join(" "))
                    }
                ));
                if let Some(body) = &f.body {
                    o.push_str(&hs_emit_block(body, 2));
                } else {
                    o.push_str("      undefined\n");
                }
                o.push_str("\n");
            }
            ImplItem::Const(c) => {
                o.push_str(&format!(
                    "    {} :: {}\n",
                    hs_ident(&c.name.name),
                    hs_type(&c.ty)
                ));
                o.push_str(&format!(
                    "    {} = {}\n",
                    hs_ident(&c.name.name),
                    hs_emit_expr(&c.value, 0)
                ));
            }
            ImplItem::TypeAlias(_) => {}
        }
    }
    o.push_str("\n");
    o
}

fn hs_const(c: &ConstDef) -> String {
    format!(
        "{} :: {}\n{} = {}\n\n",
        hs_ident(&c.name.name),
        hs_type(&c.ty),
        hs_ident(&c.name.name),
        hs_emit_expr(&c.value, 0)
    )
}

fn hs_typealias(a: &TypeAliasDef) -> String {
    format!(
        "type {} = {}\n\n",
        hs_ident(&a.name.name),
        hs_type(&a.ty)
    )
}

fn hs_macro_rules(m: &MacroRulesDefinition) -> String {
    format!(
        "-- macro_rules {} -- not directly translatable to Haskell (use Template Haskell)\n\n",
        hs_ident(&m.name.name)
    )
}

// ──────────────────────────────────────────────────────
// Expression emitter
// ──────────────────────────────────────────────────────

pub fn hs_emit_expr(expr: &Expr, indent: usize) -> String {
    let ind = "  ".repeat(indent);
    match &expr.kind {
        ExprKind::Literal(lit) => hs_literal(lit),
        ExprKind::Path(p) => hs_ident(&p.last().name),
        ExprKind::Binary { op, lhs, rhs } => {
            let lhs_s = hs_emit_expr(lhs, 0);
            let rhs_s = hs_emit_expr(rhs, 0);
            let op_s = hs_binop(op);
            // Haskell function call via ($) or just juxtaposition for simple cases
            match op {
                BinaryOp::And => format!("({} && {})", lhs_s, rhs_s),
                BinaryOp::Or => format!("({} || {})", lhs_s, rhs_s),
                BinaryOp::Ne => format!("({} /= {})", lhs_s, rhs_s),
                _ => format!("({} {} {})", lhs_s, op_s, rhs_s),
            }
        }
        ExprKind::Unary { op, operand } => {
            let inner = hs_emit_expr(operand, 0);
            match op {
                UnaryOp::Neg => format!("(negate {})", inner),
                UnaryOp::Not => format!("(not {})", inner),
                UnaryOp::Deref | UnaryOp::Ref | UnaryOp::RefMut => inner,
            }
        }
        ExprKind::Call { callee, args } => {
            // Check for special function names (println, format, etc.)
            let callee_str = if let ExprKind::Path(p) = &callee.kind {
                Some(p.last().name.as_str())
            } else {
                None
            };
            match callee_str {
                Some("println") => {
                    if args.is_empty() {
                        "putStrLn \"\"".into()
                    } else if args.len() == 1 {
                        if let ExprKind::Literal(lit) = &args[0].kind {
                            if let LiteralKind::Str { value: _, .. } = &lit.kind {
                                return format!("putStrLn {}", hs_literal(lit));
                            }
                        }
                        format!("print (show ({}))", hs_emit_expr(&args[0], 0))
                    } else {
                        // Multiple args: concat with show
                        let args_s: Vec<String> = args
                            .iter()
                            .map(|a| {
                                if let ExprKind::Literal(lit) = &a.kind {
                                    hs_literal(lit)
                                } else {
                                    format!("show ({})", hs_emit_expr(a, 0))
                                }
                            })
                            .collect();
                        format!("putStrLn ({})", args_s.join(" ++ "))
                    }
                }
                Some("eprintln") => {
                    if args.is_empty() {
                        "hPutStrLn stderr \"\"".into()
                    } else if args.len() == 1 {
                        if let ExprKind::Literal(lit) = &args[0].kind {
                            if let LiteralKind::Str { value: _, .. } = &lit.kind {
                                return format!("hPutStrLn stderr {}", hs_literal(lit));
                            }
                        }
                        format!("hPutStrLn stderr (show ({}))", hs_emit_expr(&args[0], 0))
                    } else {
                        let args_s: Vec<String> = args
                            .iter()
                            .map(|a| {
                                if let ExprKind::Literal(lit) = &a.kind {
                                    hs_literal(lit)
                                } else {
                                    format!("show ({})", hs_emit_expr(a, 0))
                                }
                            })
                            .collect();
                        format!("hPutStrLn stderr ({})", args_s.join(" ++ "))
                    }
                }
                Some("format") => {
                    // format!("hello {}", x) -> string concatenation with show
                    if !args.is_empty() {
                        if let ExprKind::Literal(lit) = &args[0].kind {
                            if let LiteralKind::Str { value, .. } = &lit.kind {
                                return hs_format_string(value, &args[1..]);
                            }
                        }
                    }
                    let as_ = args
                        .iter()
                        .map(|a| format!("show ({})", hs_emit_expr(a, 0)))
                        .collect::<Vec<_>>();
                    format!("concat [{}]", as_.join(", "))
                }
                Some("vec") => {
                    let as_ = args.iter().map(|a| hs_emit_expr(a, 0)).collect::<Vec<_>>();
                    format!("[{}]", as_.join(", "))
                }
                Some("panic") => {
                    if !args.is_empty() {
                        format!("error {}", hs_emit_expr(&args[0], 0))
                    } else {
                        "error \"panic\"".into()
                    }
                }
                Some("todo") | Some("unimplemented") => "undefined".into(),
                Some("ok") => "Just".into(),
                Some("err") => "Left".into(),
                Some("some") => {
                    if args.len() == 1 {
                        format!("Just ({})", hs_emit_expr(&args[0], 0))
                    } else {
                        "Just".into()
                    }
                }
                Some("none") => "Nothing".into(),
                Some("read") | Some("parse") => {
                    if args.len() == 1 {
                        format!("read \"{}\"", hs_emit_expr(&args[0], 0))
                    } else {
                        "read".into()
                    }
                }
                Some("to_string") => {
                    if args.len() == 1 {
                        format!("show ({})", hs_emit_expr(&args[0], 0))
                    } else {
                        "show".into()
                    }
                }
                _ => {
                    // Haskell uses juxtaposition for function application
                    let callee_s = hs_emit_expr(callee, 0);
                    let args_s: Vec<String> = args.iter().map(|a| hs_emit_expr(a, 0)).collect();
                    if args_s.is_empty() {
                        callee_s
                    } else {
                        format!("({} {})", callee_s, args_s.join(" "))
                    }
                }
            }
        }
        ExprKind::MethodCall { receiver, method, generic_args: _, args } => {
            let mn = &method.name;
            let recv_str = hs_emit_expr(receiver, 0);
            // Check std method mapping first
            if let Some(mapped) = hs_std_method(&recv_str, mn, args) {
                return mapped;
            }
            // Generic method call -> record field or function application
            let mn_esc = hs_ident(mn);
            let as_ = args.iter().map(|a| hs_emit_expr(a, 0)).collect::<Vec<_>>();
            if as_.is_empty() {
                // Could be a field access function: fieldName record
                format!("({} {})", mn_esc, recv_str)
            } else {
                format!("({} {} {})", mn_esc, recv_str, as_.join(" "))
            }
        }
        ExprKind::Field { base, field } => {
            let fn_ = match field {
                FieldIndex::Named(id) => hs_ident(&id.name),
                FieldIndex::Index(i, _) => {
                    // Tuple index -> use fst/snd for 2-tuples, or generic
                    return match i {
                        0 => format!("fst ({})", hs_emit_expr(base, 0)),
                        1 => format!("snd ({})", hs_emit_expr(base, 0)),
                        _ => {
                            // For larger tuples, use a helper comment
                            let base_s = hs_emit_expr(base, 0);
                            format!("({} !! {})", base_s, i)
                        }
                    };
                }
            };
            // Haskell record field access: fieldName record
            let recv = hs_emit_expr(base, 0);
            format!("({} {})", fn_, recv)
        }
        ExprKind::Index { base, index } => {
            // List index: list !! idx
            format!("({} !! {})", hs_emit_expr(base, 0), hs_emit_expr(index, 0))
        }
        ExprKind::Slice { base, range } => {
            let bs = hs_emit_expr(base, 0);
            let s = range
                .lo
                .as_ref()
                .map(|e| hs_emit_expr(e, 0))
                .unwrap_or_else(|| "0".into());
            let e = range
                .hi
                .as_ref()
                .map(|e| hs_emit_expr(e, 0))
                .unwrap_or_else(|| format!("length {}", bs));
            let count = format!("({} - {})", e, s);
            if s == "0" {
                format!("take {} {}", count, bs)
            } else {
                format!("take {} (drop {} {})", count, s, bs)
            }
        }
        ExprKind::Range(r) => {
            let l = r
                .lo
                .as_ref()
                .map(|e| hs_emit_expr(e, 0))
                .unwrap_or_else(|| "0".into());
            let hi = r
                .hi
                .as_ref()
                .map(|e| hs_emit_expr(e, 0))
                .unwrap_or_else(|| "undefined".into());
            if r.inclusive {
                format!("[{}..{}]", l, hi)
            } else {
                // Exclusive range: [lo..hi-1]
                format!("[{}..{} - 1]", l, hi)
            }
        }
        ExprKind::Assign { lhs, rhs } => {
            // Haskell doesn't have mutable assignment in pure code
            // In do-notation, this becomes a let binding or IORef write
            let rhs_s = hs_emit_expr(rhs, 0);
            match &lhs.kind {
                ExprKind::Path(p) => {
                    let name = hs_ident(&p.last().name);
                    format!("let {} = {}", name, rhs_s)
                }
                ExprKind::Field { base, field } => {
                    let fn_ = match field {
                        FieldIndex::Named(id) => hs_ident(&id.name),
                        FieldIndex::Index(i, _) => format!("_{}", i),
                    };
                    let base_s = hs_emit_expr(base, 0);
                    format!("let {} = {{}} {{ {} = {} }}", base_s, fn_, rhs_s)
                }
                ExprKind::Index { base, index } => {
                    let base_s = hs_emit_expr(base, 0);
                    let idx_s = hs_emit_expr(index, 0);
                    format!(
                        "-- assign {}[{}] = {}  (requires mutable data structure)\n  let _ = ({}, {})",
                        base_s, idx_s, rhs_s, base_s, idx_s
                    )
                }
                _ => format!("-- assignment: {} = {}  (Haskell uses let bindings)", hs_emit_expr(lhs, 0), rhs_s),
            }
        }
        ExprKind::CompoundAssign { op, lhs, rhs } => {
            let rhs_s = hs_emit_expr(rhs, 0);
            let op_s = hs_binop(op);
            let lhs_s = hs_emit_expr(lhs, 0);
            format!(
                "-- compound assign: {} {}= {}  (Haskell uses let bindings)\n  let {} = {} {} {}",
                lhs_s, op_s, rhs_s, lhs_s, lhs_s, op_s, rhs_s
            )
        }
        ExprKind::If { cond, then, else_ } => {
            let mut o = format!("{}if {} then \n", ind, hs_emit_expr(cond, 0));
            o.push_str(&hs_emit_block(then, indent + 1));
            if let Some(els) = else_ {
                o.push_str(&format!("{}else \n", ind));
                match &els.kind {
                    ExprKind::If { .. } => {
                        // else-if chain
                        o.push_str(&hs_emit_expr(els, indent));
                    }
                    ExprKind::Block(b) => {
                        o.push_str(&hs_emit_block(b, indent + 1));
                    }
                    _ => {
                        let els_ind = "  ".repeat(indent + 1);
                        o.push_str(&format!("{}{}\n", els_ind, hs_emit_expr(els, 0)));
                    }
                }
            }
            o
        }
        ExprKind::Match { scrutinee, arms } => {
            let mut o = format!("{}case {} of\n", ind, hs_emit_expr(scrutinee, 0));
            for (i, arm) in arms.iter().enumerate() {
                let p = hs_pattern(&arm.pattern);
                let g = if let Some(g) = &arm.guard {
                    format!(" | {}", hs_emit_expr(g, 0))
                } else {
                    String::new()
                };
                let arm_ind = "  ".repeat(indent + 1);
                match &arm.body.kind {
                    ExprKind::Block(b) => {
                        o.push_str(&format!(
                            "{}  {}{} ->\n",
                            ind, p, g
                        ));
                        o.push_str(&hs_emit_block(b, indent + 2));
                    }
                    ExprKind::If { .. } | ExprKind::Match { .. } => {
                        o.push_str(&format!(
                            "{}  {}{} ->\n",
                            ind, p, g
                        ));
                        o.push_str(&format!(
                            "{}{}\n",
                            arm_ind,
                            hs_emit_expr(&arm.body, 0)
                        ));
                    }
                    _ => {
                        let is_last = i == arms.len() - 1;
                        if is_last {
                            o.push_str(&format!(
                                "{}  {}{} -> {}\n",
                                ind, p, g,
                                hs_emit_expr(&arm.body, 0)
                            ));
                        } else {
                            o.push_str(&format!(
                                "{}  {}{} -> {}\n",
                                ind, p, g,
                                hs_emit_expr(&arm.body, 0)
                            ));
                        }
                    }
                }
            }
            o
        }
        ExprKind::For { label: _, pattern, iter, body } => {
            let iter_s = hs_emit_expr(iter, 0);
            // for pat in iter { body } -> mapM_ (\pat -> body) iter
            let mut o = String::new();
            // Generate lambda params from pattern
            let (lambda_params, lambda_body_prefix) = hs_pattern_to_lambda(pattern);
            let lambda_prefix_str = if lambda_body_prefix.is_empty() {
                String::new()
            } else {
                format!(" -> {}", lambda_body_prefix)
            };
            o.push_str(&format!(
                "{}mapM_ (\\{}{} -> \n",
                ind,
                lambda_params,
                lambda_prefix_str
            ));
            o.push_str(&hs_emit_block(body, indent + 2));
            o.push_str(&format!("{}) {}\n", ind, iter_s));
            o
        }
        ExprKind::While { label: _, cond, body } => {
            // while -> recursive helper function
            let mut o = format!("{}let loop = do\n", ind);
            let cond_s = hs_emit_expr(cond, 0);
            o.push_str(&format!("{}  {} <- {}\n", ind, hs_ident("cond"), cond_s));
            o.push_str(&format!("{}  when {} $ do\n", ind, hs_ident("cond")));
            o.push_str(&hs_emit_block(body, indent + 2));
            o.push_str(&format!("{}  loop\n", ind));
            o.push_str(&format!("{}loop\n", ind));
            o
        }
        ExprKind::WhileLet { label: _, pattern, expr: scrut, body } => {
            // while let pat = expr -> recursive helper
            let scrut_s = hs_emit_expr(scrut, 0);
            let pat_s = hs_pattern(pattern);
            let mut o = format!("{}let loop = do\n", ind);
            o.push_str(&format!(
                "{}  case {} of\n",
                ind, scrut_s
            ));
            o.push_str(&format!(
                "{}    {} -> do\n",
                ind, pat_s
            ));
            o.push_str(&hs_emit_block(body, indent + 3));
            o.push_str(&format!("{}    loop\n", ind));
            o.push_str(&format!("{}    _ -> return ()\n", ind));
            o.push_str(&format!("{}loop\n", ind));
            o
        }
        ExprKind::Loop { label, body } => {
            let mut o = String::new();
            if let Some(l) = label {
                o.push_str(&format!("{}-- label: {}\n", ind, hs_ident(&l.name)));
            }
            // loop -> forever $ do { ... }
            o.push_str(&format!("{}forever $ do\n", ind));
            o.push_str(&hs_emit_block(body, indent + 1));
            o
        }
        ExprKind::Closure { params, body, ret: _, .. } => {
            let ps: Vec<String> = params
                .iter()
                .filter_map(|p| match &p.kind {
                    ParamKind::Pattern(pat) => match &pat.kind {
                        PatternKind::Ident { name, .. } => Some(hs_ident(&name.name)),
                        PatternKind::Wildcard => Some("_".into()),
                        _ => Some("x".into()),
                    },
                    _ => None,
                })
                .collect();
            if ps.is_empty() {
                // Nullary closure: \_ -> body or just body
                if let ExprKind::Block(be) = &body.kind {
                    if be.stmts.is_empty() {
                        if let Some(tail) = &be.tail {
                            return format!("(\\_ -> {})", hs_emit_expr(tail, 0));
                        }
                    }
                    let mut o = format!("(\\_ ->\n", );
                    o.push_str(&hs_emit_block(be, indent + 1));
                    o.push_str(&format!("{} )", ind));
                    return o;
                }
                return format!("(\\_ -> {})", hs_emit_expr(body, 0));
            }
            if let ExprKind::Block(be) = &body.kind {
                if be.stmts.is_empty() {
                    if let Some(tail) = &be.tail {
                        return format!(
                            "(\\{} -> {})",
                            ps.join(" "),
                            hs_emit_expr(tail, 0)
                        );
                    }
                }
                let mut o = format!("(\\{} ->\n", ps.join(" "));
                o.push_str(&hs_emit_block(be, indent + 1));
                o.push_str(&format!("{} )", ind));
                o
            } else {
                format!(
                    "(\\{} -> {})",
                    ps.join(" "),
                    hs_emit_expr(body, 0)
                )
            }
        }
        ExprKind::Return(value) => {
            if let Some(v) = value {
                format!("return ({})", hs_emit_expr(v, 0))
            } else {
                "return ()".into()
            }
        }
        ExprKind::Break { label, value } => {
            let mut o = String::new();
            if let Some(l) = label {
                o.push_str(&format!("-- break from {}", hs_ident(&l.name)));
            } else {
                o.push_str("-- break");
            }
            if let Some(v) = value {
                o.push_str(&format!("  /* with value: {} */", hs_emit_expr(v, 0)));
            }
            o
        }
        ExprKind::Continue { label } => {
            if let Some(l) = label {
                format!("-- continue {}", hs_ident(&l.name))
            } else {
                "-- continue".into()
            }
        }
        ExprKind::Array(elems) => {
            let es: Vec<String> = elems.iter().map(|e| hs_emit_expr(e, 0)).collect();
            format!("[{}]", es.join(", "))
        }
        ExprKind::ArrayRepeat { elem, count } => {
            format!(
                "replicate ({}) ({})",
                hs_emit_expr(count, 0),
                hs_emit_expr(elem, 0)
            )
        }
        ExprKind::Struct { path, fields, spread } => {
            let name = hs_ident(&path.last().name);
            let fs: Vec<String> = fields
                .iter()
                .map(|f| {
                    let fn_ = match &f.name {
                        FieldIndex::Named(id) => hs_ident(&id.name),
                        FieldIndex::Index(i, _) => format!("_{}", i),
                    };
                    let v = f
                        .value
                        .as_ref()
                        .map(|v| hs_emit_expr(v, 0))
                        .unwrap_or_else(|| fn_.clone());
                    format!("{} = {}", fn_, v)
                })
                .collect();
            let spread_str = if let Some(spread) = spread {
                format!("  -- ..{}", hs_emit_expr(spread, 0))
            } else {
                String::new()
            };
            if fs.is_empty() {
                format!("{}{}", name, spread_str)
            } else {
                format!("{} {{ {} }}{}", name, fs.join(", "), spread_str)
            }
        }
        ExprKind::Tuple(elems) => {
            let es: Vec<String> = elems.iter().map(|e| hs_emit_expr(e, 0)).collect();
            match es.len() {
                0 => "()".into(),
                _ => format!("({})", es.join(", ")),
            }
        }
        ExprKind::Block(be) => {
            let mut o = format!("{}do\n", ind);
            o.push_str(&hs_emit_block(be, indent + 1));
            o
        }
        ExprKind::AsyncBlock { body, .. } => {
            let mut o = format!("{}-- async do\n", ind);
            o.push_str(&format!("{}do\n", ind));
            o.push_str(&hs_emit_block(body, indent + 1));
            o
        }
        ExprKind::Try(inner) => {
            // Try in Haskell -> try from Control.Exception or Either
            let inner_s = hs_emit_expr(inner, 0);
            format!("(try {} :: IO (Either SomeException a))", inner_s)
        }
        ExprKind::Await(inner) => {
            // await -> monadic bind (<-)
            format!("<- {}", hs_emit_expr(inner, 0))
        }
        ExprKind::Cast { expr: inner, ty } => {
            let inner_s = hs_emit_expr(inner, 0);
            let type_s = hs_type(ty);
            // Check if it's a numeric cast
            match &ty.kind {
                TypeKind::Path(pt) => {
                    let name = pt.path.last().name.as_str();
                    if matches!(
                        name,
                        "Int" | "Int64" | "Word32" | "Word64" | "Float" | "Double"
                        | "Integer" | "i32" | "i64" | "u32" | "u64" | "f32" | "f64"
                    ) {
                        return format!("(fromIntegral {} :: {})", inner_s, type_s);
                    }
                }
                _ => {}
            }
            format!("({} :: {})", inner_s, type_s)
        }
        ExprKind::IfLet { pattern, expr: scrut, then, else_ } => {
            // if let pat = expr -> case expr of { pat -> ... ; _ -> ... }
            let scrut_s = hs_emit_expr(scrut, 0);
            let pat_s = hs_pattern(pattern);
            let mut o = format!("{}case {} of\n", ind, scrut_s);
            o.push_str(&format!(
                "{}  {} ->\n",
                ind, pat_s
            ));
            o.push_str(&hs_emit_block(then, indent + 2));
            if let Some(els) = else_ {
                o.push_str(&format!(
                    "{}  _ ->\n",
                    ind
                ));
                match &els.kind {
                    ExprKind::Block(b) => {
                        o.push_str(&hs_emit_block(b, indent + 2));
                    }
                    _ => {
                        let els_ind = "  ".repeat(indent + 2);
                        o.push_str(&format!(
                            "{}{}\n",
                            els_ind,
                            hs_emit_expr(els, 0)
                        ));
                    }
                }
            }
            o
        }
        ExprKind::Macro { path, args: _ } => hs_macro(path.last().name.as_str()),
        ExprKind::Native(nb) => {
            // native haskell block: emit as-is
            if nb.lang.name == "haskell" {
                nb.code.clone()
            } else {
                "-- native block".into()
            }
        }
    }
}

// ──────────────────────────────────────────────────────
// Literals
// ──────────────────────────────────────────────────────

fn hs_literal(lit: &Literal) -> String {
    match &lit.kind {
        LiteralKind::Str { value, .. } => {
            format!(
                "\"{}\"",
                value
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t")
            )
        }
        LiteralKind::Bool(b) => {
            if *b { "True".into() } else { "False".into() }
        }
        LiteralKind::Int { value, .. } => {
            let s = value.to_string();
            if *value < 0 {
                format!("({})", s)
            } else if *value > i64::MAX as i128 {
                format!("{} :: Integer", s)
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
        LiteralKind::Char(c) => {
            format!("'{}'", c.escape_unicode())
        }
    }
}

// ──────────────────────────────────────────────────────
// Format string conversion
// ──────────────────────────────────────────────────────

/// Convert Rust format! string to Haskell string concatenation with show
fn hs_format_string(fmt: &str, args: &[Expr]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut arg_idx = 0;
    let mut chars = fmt.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' && chars.peek() == Some(&'{') {
            // Escaped brace {{
            chars.next();
            current.push('{');
        } else if c == '}' && chars.peek() == Some(&'}') {
            // Escaped brace }}
            chars.next();
            current.push('}');
        } else if c == '{' {
            // Start of placeholder - consume until }
            let mut spec = String::new();
            while let Some(nc) = chars.next() {
                if nc == '}' {
                    break;
                }
                spec.push(nc);
            }
            // Flush current text
            if !current.is_empty() {
                parts.push(format!("\"{}\"", current.replace('\\', "\\\\").replace('"', "\\\"")));
                current.clear();
            }
            // Add argument
            if arg_idx < args.len() {
                let arg_s = hs_emit_expr(&args[arg_idx], 0);
                // Check for :? (debug) or other format specs
                if spec.contains('?') {
                    parts.push(format!("show ({})", arg_s));
                } else {
                    parts.push(format!("show ({})", arg_s));
                }
                arg_idx += 1;
            }
        } else if c == '}' {
            current.push('}');
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        parts.push(format!(
            "\"{}\"",
            current
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r")
                .replace('\t', "\\t")
        ));
    }
    if parts.is_empty() {
        "\"\"".into()
    } else if parts.len() == 1 {
        parts.into_iter().next().unwrap()
    } else {
        format!("({})", parts.join(" ++ "))
    }
}

// ──────────────────────────────────────────────────────
// Operators
// ──────────────────────────────────────────────────────

fn hs_binop(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "div",
        BinaryOp::Rem => "mod",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "/=",
        BinaryOp::Lt => "<",
        BinaryOp::Gt => ">",
        BinaryOp::Le => "<=",
        BinaryOp::Ge => ">=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
        BinaryOp::BitAnd => ".&.",
        BinaryOp::BitOr => ".|.",
        BinaryOp::BitXor => "xor",
        BinaryOp::Shl => "shiftL",
        BinaryOp::Shr => "shiftR",
    }
}

// ──────────────────────────────────────────────────────
// Patterns
// ──────────────────────────────────────────────────────

fn hs_pattern(pat: &Pattern) -> String {
    match &pat.kind {
        PatternKind::Wildcard => "_".into(),
        PatternKind::Ident { name, sub: None, .. } => hs_ident(&name.name),
        PatternKind::Ident { name, sub: Some(sub), .. } => {
            format!("{}@{}", hs_ident(&name.name), hs_pattern(sub))
        }
        PatternKind::Literal(lit) => hs_literal(lit),
        PatternKind::Path(p) => hs_ident(&p.last().name),
        PatternKind::TupleStruct { path, elems, .. } => {
            let n = hs_ident(&path.last().name);
            let es: Vec<String> = elems.iter().map(hs_pattern).collect();
            if es.is_empty() {
                n
            } else {
                format!("({} {})", n, es.join(" "))
            }
        }
        PatternKind::Struct { path, fields, .. } => {
            let n = hs_ident(&path.last().name);
            let fs: Vec<String> = fields
                .iter()
                .map(|f| {
                    let fn_ = hs_ident(&f.name.name);
                    let p = f
                        .pattern
                        .as_ref()
                        .map(|p| hs_pattern(p))
                        .unwrap_or_else(|| hs_ident(&f.name.name));
                    format!("{} = {}", fn_, p)
                })
                .collect();
            if fs.is_empty() {
                format!("{}", n)
            } else {
                format!("({} {{ {} }})", n, fs.join(", "))
            }
        }
        PatternKind::Tuple { elems, .. } => {
            let es: Vec<String> = elems.iter().map(hs_pattern).collect();
            format!("({})", es.join(", "))
        }
        PatternKind::Or(elems) => {
            // GHC supports or-patterns with LambdaCase or MultiWayIf
            // For standard Haskell, we can't use or-patterns in case easily
            // Use nested case or comment
            if elems.len() == 2 {
                // In GHC 9.x+, or-patterns are supported
                format!("({} | {})", hs_pattern(&elems[0]), hs_pattern(&elems[1]))
            } else {
                elems
                    .iter()
                    .map(|e| hs_pattern(e))
                    .collect::<Vec<_>>()
                    .join(" | ")
            }
        }
        PatternKind::Range { lo, hi, inclusive } => {
            let l = hs_pattern(lo);
            let r = hs_pattern(hi);
            if *inclusive {
                format!("({} <= {} && {} <= {})", l, hs_ident("x"), hs_ident("x"), r)
            } else {
                format!("({} <= {} && {} < {})", l, hs_ident("x"), hs_ident("x"), r)
            }
        }
        PatternKind::Rest => "_".into(),
    }
}

/// Extract lambda parameter names from a pattern for use in mapM_/forM_
fn hs_pattern_to_lambda(pat: &Pattern) -> (String, String) {
    match &pat.kind {
        PatternKind::Ident { name, .. } => (hs_ident(&name.name), String::new()),
        PatternKind::Wildcard => ("_".into(), String::new()),
        PatternKind::Tuple { elems, .. } => {
            let names: Vec<String> = elems
                .iter()
                .enumerate()
                .map(|(i, e)| match &e.kind {
                    PatternKind::Ident { name, .. } => hs_ident(&name.name),
                    PatternKind::Wildcard => format!("_{}", i),
                    _ => format!("p{}", i),
                })
                .collect();
            let tuple_pat = format!("({})", names.join(", "));
            (tuple_pat, String::new())
        }
        PatternKind::TupleStruct { path, elems, .. } => {
            let n = hs_ident(&path.last().name);
            let names: Vec<String> = elems
                .iter()
                .enumerate()
                .map(|(i, e)| match &e.kind {
                    PatternKind::Ident { name, .. } => hs_ident(&name.name),
                    PatternKind::Wildcard => format!("_{}", i),
                    _ => format!("p{}", i),
                })
                .collect();
            let pat_str = if names.is_empty() {
                n
            } else {
                format!("({} {})", n, names.join(" "))
            };
            (pat_str, String::new())
        }
        _ => ("x".into(), String::new()),
    }
}

// ──────────────────────────────────────────────────────
// Macro handling
// ──────────────────────────────────────────────────────

fn hs_macro(name: &str) -> String {
    match name {
        "println" => "putStrLn".into(),
        "eprintln" => "hPutStrLn stderr".into(),
        "format" => "-- format macro (use string concatenation)".into(),
        "todo" | "unimplemented" => "undefined".into(),
        "panic" => "error".into(),
        "vec" => "-- use list literal [a, b, c]".into(),
        "dbg" => "-- dbg macro (use trace from Debug.Trace)".into(),
        _ => format!("-- macro: {}", name),
    }
}

// ──────────────────────────────────────────────────────
// Block / statement emitters
// ──────────────────────────────────────────────────────

pub fn hs_emit_block(be: &BlockExpr, indent: usize) -> String {
    let ind = "  ".repeat(indent);
    let mut o = String::new();
    for stmt in &be.stmts {
        match stmt {
            Stmt::Let(l) => o.push_str(&format!("{}{}", ind, hs_emit_let(l, 0))),
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
                        o.push_str(&format!("{}\n", hs_emit_expr(expr, indent)));
                    }
                    ExprKind::Return(_) => {
                        o.push_str(&format!(
                            "{}{}\n",
                            ind,
                            hs_emit_expr(expr, 0)
                        ));
                    }
                    _ => {
                        o.push_str(&format!(
                            "{}{}\n",
                            ind,
                            hs_emit_expr(expr, 0)
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
                o.push_str(&format!("{}{}\n", ind, hs_emit_expr(tail, indent)));
            }
            _ => {
                o.push_str(&format!(
                    "{}{}\n",
                    ind,
                    hs_emit_expr(tail, 0)
                ));
            }
        }
    }
    o
}

fn hs_emit_let(l: &LetStmt, _indent: usize) -> String {
    let pat = hs_pattern(&l.pattern);
    let init_s = l
        .init
        .as_ref()
        .map(|e| hs_emit_expr(e, 0))
        .unwrap_or_else(|| "undefined".into());
    if let Some(ty) = &l.ty {
        format!(
            "let {} :: {}\n  {} = {}\n",
            pat,
            hs_type(ty),
            pat,
            init_s
        )
    } else {
        format!(
            "let {} = {}\n",
            pat,
            init_s
        )
    }
}

pub fn hs_emit_stmt(stmt: &Stmt, indent: usize) -> String {
    match stmt {
        Stmt::Let(l) => {
            let ind = "  ".repeat(indent);
            format!("{}{}", ind, hs_emit_let(l, 0))
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
                | ExprKind::IfLet { .. } => hs_emit_expr(expr, indent) + "\n",
                _ => {
                    let ind = "  ".repeat(indent);
                    format!("{}{}\n", ind, hs_emit_expr(expr, 0))
                }
            }
        }
    }
}

// ──────────────────────────────────────────────────────
// Utility: disambiguate constructor-like vs variable-like names
// ──────────────────────────────────────────────────────

/// In Haskell, type names and constructor names must start with an uppercase letter.
/// Identifier names (variables, functions) must start with a lowercase letter.
/// This function capitalizes the first letter for use in type/constructor contexts.
#[allow(dead_code)]
fn hs_constructor_name(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => "X".into(),
        Some(first) => {
            if first.is_ascii_uppercase() {
                s.to_string()
            } else {
                format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
            }
        }
    }
}

/// Ensure a name starts with lowercase (for function/variable names in Haskell)
#[allow(dead_code)]
fn hs_var_name(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => "x".into(),
        Some(first) => {
            if first.is_ascii_lowercase() || first == '_' {
                hs_ident(s)
            } else {
                format!("{}{}", first.to_ascii_lowercase(), chars.as_str())
            }
        }
    }
}
