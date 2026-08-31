//! TypeScript 后端（P7 骨架）—— 类型映射 + 项转译基础

use crate::ast::*;
use crate::codegen::{CodegenBackend, CodegenContext};

pub struct TypeScriptBackend;

impl CodegenBackend for TypeScriptBackend {
    fn lang(&self) -> &'static str { "typescript" }

    fn emit_item(&self, _ctx: &CodegenContext, item: &Item) -> Result<String, String> {
        let mut out = String::new();
        out.push_str(&format!("// {}\n", crate::sourcemap::generated_header("typescript")));
        match item {
            Item::Struct(s) => {
                out.push_str(&format!("export interface {} {{\n", s.name.name));
                if let StructKind::Named(fields) = &s.kind {
                    for f in fields {
                        let name = f.name.as_ref().map(|n| n.name.as_str()).unwrap_or("_");
                        let optional = matches!(&f.ty.kind, TypeKind::Path(p) if p.path.is_ident("Option"));
                        out.push_str(&format!(
                            "  {}{}: {};\n",
                            camel_case(name),
                            if optional { "?" } else { "" },
                            ts_type(&f.ty)
                        ));
                    }
                }
                out.push_str("}\n");
            }
            Item::Enum(e) => {
                out.push_str(&format!("export type {} =\n", e.name.name));
                for (i, v) in e.variants.iter().enumerate() {
                    let prefix = if i == 0 { "  | " } else { "  | " };
                    match &v.fields {
                        StructKind::Named(fields) => {
                            let body = fields
                                .iter()
                                .map(|f| {
                                    let name = f.name.as_ref().map(|n| n.name.as_str()).unwrap_or("_");
                                    format!("{}: {}", camel_case(name), ts_type(&f.ty))
                                })
                                .collect::<Vec<_>>().join("; ");
                            out.push_str(&format!("{prefix}{{ kind: '{}', {} }}\n", v.name.name, body));
                        }
                        StructKind::Tuple(fields) => {
                            let body = fields.iter().map(|f| ts_type(&f.ty)).collect::<Vec<_>>().join(", ");
                            out.push_str(&format!("{prefix}{{ kind: '{}', value: [{}] }}\n", v.name.name, body));
                        }
                        StructKind::Unit => out.push_str(&format!("{prefix}'{}'\n", v.name.name)),
                    }
                }
                out.push_str(";\n");
            }
            Item::Fn(f) => {
                let ret = f.ret.as_ref().map(|t| format!(": {}", ts_type(t))).unwrap_or_else(|| ": void".into());
                out.push_str(&format!(
                    "export {}function {}({}){} {{\n",
                    if f.is_async { "async " } else { "" },
                    camel_case(&f.name.name),
                    f.params.iter().map(ts_param).collect::<Vec<_>>().join(", "),
                    ret
                ));
                match &f.body {
                    Some(b) => {
                        for stmt in &b.stmts {
                            if let Stmt::Let(l) = stmt {
                                let name = match &l.pattern.kind {
                                    PatternKind::Ident { name, .. } => camel_case(&name.name),
                                    _ => "bound".to_string(),
                                };
                                match &l.init {
                                    Some(init) => out.push_str(&format!("  const {name} = {};\n", emit_expr_ts(init))),
                                    None => out.push_str(&format!("  let {name}: any;\n")),
                                }
                            }
                        }
                        if let Some(tail) = &b.tail {
                            out.push_str(&format!("  return {};\n", emit_expr_ts(tail)));
                        }
                    }
                    None => out.push_str("  throw new Error('not implemented');\n"),
                }
                out.push_str("}\n");
            }
            Item::Graph(g) => {
                out.push_str(&format!(
                    "// graph {} — AgentLoop 转译（P7 路线图）\n",
                    g.name.name
                ));
                out.push_str("export async function main(): Promise<void> {\n  // loop + match → while(true) + switch\n}\n");
            }
            _ => {
                return Err(format!("typescript 后端暂不支持 {}", crate::ast::item_kind_name(item)));
            }
        }
        Ok(out)
    }
}

pub fn ts_type(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(p) => {
            let name = p.path.segments.last().map(|s| s.name.clone()).unwrap_or_default();
            let inner = p.generic_args.iter().map(map_generic).next();
            match name.as_str() {
                "bool" => "boolean".into(),
                "String" | "str" => "string".into(),
                "i8" | "i16" | "i32" | "i64" | "isize" | "u8" | "u16" | "u32" | "u64" | "usize" | "f32" | "f64" => "number".into(),
                "Vec" => format!("{}[]", inner.unwrap_or_else(|| "any".into())),
                "HashMap" => "Record<string, any>".into(),
                "HashSet" => "Set<any>".into(),
                "Option" => format!("{} | undefined", inner.unwrap_or_else(|| "any".into())),
                "Result" => inner.unwrap_or_else(|| "any".into()),
                "Box" => inner.unwrap_or_else(|| "any".into()),
                other => other.to_string(),
            }
        }
        TypeKind::Ref { inner, .. } => ts_type(inner),
        TypeKind::Tuple(elems) => format!("[{}]", elems.iter().map(ts_type).collect::<Vec<_>>().join(", ")),
        TypeKind::Array { elem, .. } | TypeKind::Slice(elem) => format!("readonly {}[]", ts_type(elem)),
        TypeKind::DynTrait(_) => "any".into(),
        TypeKind::ImplTrait(_) => "any".into(),
        TypeKind::Infer => "any".into(),
        _ => "any".into(),
    }
}

fn map_generic(arg: &GenericArg) -> String {
    match arg {
        GenericArg::Type(t) => ts_type(t),
        GenericArg::Const(_) => "number".into(),
    }
}

fn ts_param(p: &Param) -> String {
    if let ParamKind::Pattern(pat) = &p.kind {
        if let PatternKind::Ident { name, .. } = &pat.kind {
            return format!("{}: {}", camel_case(&name.name), ts_type(&p.ty));
        }
    }
    "arg: any".to_string()
}

pub fn emit_expr_ts(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(lit) => match &lit.kind {
            LiteralKind::Str { value, .. } => format!("{value:?}"),
            LiteralKind::Bool(b) => b.to_string(),
            other => format!("{other:?}"),
        },
        ExprKind::Path(p) => camel_case(&p.last().name),
        ExprKind::Binary { op, lhs, rhs } => {
            format!("{} {} {}", emit_expr_ts(lhs), op.as_str(), emit_expr_ts(rhs))
        }
        ExprKind::Unary { op, operand } => format!("{}{}", op.as_str(), emit_expr_ts(operand)),
        ExprKind::Call { callee, args } => {
            format!(
                "{}({})",
                emit_expr_ts(callee),
                args.iter().map(emit_expr_ts).collect::<Vec<_>>().join(", ")
            )
        }
        ExprKind::MethodCall { receiver, method, args, .. } => {
            format!(
                "{}.{}({})",
                emit_expr_ts(receiver),
                camel_case(&method.name),
                args.iter().map(emit_expr_ts).collect::<Vec<_>>().join(", ")
            )
        }
        ExprKind::Field { base, field } => {
            let f = match field {
                FieldIndex::Named(id) => camel_case(&id.name),
                FieldIndex::Index(i, _) => i.to_string(),
            };
            format!("{}.{}", emit_expr_ts(base), f)
        }
        ExprKind::Await(inner) => format!("await {}", emit_expr_ts(inner)),
        _ => "undefined /* 表达式待 P7 完整实现 */".to_string(),
    }
}

pub fn camel_case(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
