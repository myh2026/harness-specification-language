//! Python 后端（P5 骨架）—— 类型映射 + 函数签名转译基础

use crate::ast::*;
use crate::codegen::{CodegenBackend, CodegenContext};

pub struct PythonBackend;

impl CodegenBackend for PythonBackend {
    fn lang(&self) -> &'static str { "python" }

    fn emit_item(&self, ctx: &CodegenContext, item: &Item) -> Result<String, String> {
        let mut out = String::new();
        out.push_str(&format!(
            "# {}\n",
            crate::sourcemap::generated_header("python")
        ));
        match item {
            Item::Fn(f) => {
                let ret = f.ret.as_ref().map(|t| format!(" -> {}", py_type(t))).unwrap_or_default();
                out.push_str(&format!(
                    "{}def {}({}){}:\n",
                    if f.is_async { "async " } else { "" },
                    snake_case(&f.name.name),
                    f.params.iter().map(|p| py_param(p)).collect::<Vec<_>>().join(", "),
                    ret
                ));
                match &f.body {
                    Some(body) => out.push_str(&emit_block_py(body, 1)),
                    None => out.push_str("    ...\n"),
                }
            }
            Item::Struct(s) => {
                // dataclass 骨架
                out.push_str("from dataclasses import dataclass\n\n@dataclass\n");
                out.push_str(&format!("class {}:\n", s.name.name));
                if let StructKind::Named(fields) = &s.kind {
                    if fields.is_empty() {
                        out.push_str("    pass\n");
                    }
                    for field in fields {
                        out.push_str(&format!(
                            "    {}: {}\n",
                            snake_case(field.name.as_ref().map(|n| n.name.as_str()).unwrap_or("_")),
                            py_type(&field.ty)
                        ));
                    }
                } else {
                    out.push_str("    pass  # tuple struct: P5 完整版转译\n");
                }
            }
            Item::Graph(g) => {
                // graph → 入口函数（P5 完整版：loop→while True, match→穷尽 if/elif）
                out.push_str(&format!(
                    "# graph {} — scale: {:?}\n",
                    g.name.name, ctx.scale
                ));
                out.push_str("async def main() -> None:\n");
                out.push_str("    # P5: AgentLoop → while True + match Action（骨架）\n");
                out.push_str("    ...\n");
            }
            _ => {
                return Err(format!("python 后端暂不支持 {}", crate::ast::item_kind_name(item)));
            }
        }
        Ok(out)
    }
}

pub fn py_type(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(p) => {
            let name = p.path.segments.last().map(|s| s.name.clone()).unwrap_or_default();
            match name.as_str() {
                "bool" => "bool".into(),
                "String" | "str" => "str".into(),
                "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => "int".into(),
                "f32" | "f64" => "float".into(),
                "Vec" => format!("list[{}]", p.generic_args.iter().map(map_generic).next().unwrap_or_else(|| "Any".into())),
                "HashMap" => "dict".into(),
                "HashSet" => "set".into(),
                "Option" => format!("{} | None", p.generic_args.iter().map(map_generic).next().unwrap_or_else(|| "Any".into())),
                "Result" => format!("{} | Exception", p.generic_args.iter().map(map_generic).next().unwrap_or_else(|| "Any".into())),
                "Box" => p.generic_args.iter().map(map_generic).next().unwrap_or_else(|| "Any".into()),
                other => other.to_string(),
            }
        }
        TypeKind::Ref { inner, .. } => py_type(inner),
        TypeKind::Tuple(elems) => {
            format!("tuple[{}]", elems.iter().map(py_type).collect::<Vec<_>>().join(", "))
        }
        TypeKind::Array { elem, .. } => format!("list[{}]", py_type(elem)),
        TypeKind::Slice(elem) => format!("list[{}]", py_type(elem)),
        TypeKind::DynTrait(_) => "Any".into(),
        TypeKind::ImplTrait(_) => "Any".into(),
        TypeKind::Infer => "Any".into(),
        _ => "Any".into(),
    }
}

fn map_generic(arg: &GenericArg) -> String {
    match arg {
        GenericArg::Type(t) => py_type(t),
        GenericArg::Const(_) => "int".into(),
    }
}

fn py_param(p: &Param) -> String {
    if let ParamKind::Pattern(pat) = &p.kind {
        if let PatternKind::Ident { name, .. } = &pat.kind {
            return format!("{}: {}", snake_case(&name.name), py_type(&p.ty));
        }
    }
    "arg: Any".to_string()
}

fn emit_block_py(block: &BlockExpr, indent: usize) -> String {
    let pad = "    ".repeat(indent);
    let mut out = String::new();
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let(l) => {
                let name = match &l.pattern.kind {
                    PatternKind::Ident { name, .. } => snake_case(&name.name),
                    _ => "bound".to_string(),
                };
                match &l.init {
                    Some(init) => out.push_str(&format!("{pad}{name} = {}\n", emit_expr_py(init))),
                    None => out.push_str(&format!("{pad}{name}: Any = None\n")),
                }
            }
            Stmt::Expr { expr, .. } => {
                out.push_str(&format!("{pad}{}\n", emit_expr_py(expr)));
            }
            Stmt::Empty(_) => {}
            Stmt::Item(_) => out.push_str(&format!("{pad}pass  # 局部项: P5 完整版\n")),
        }
    }
    if let Some(tail) = &block.tail {
        out.push_str(&format!("{pad}return {}\n", emit_expr_py(tail)));
    } else if out.is_empty() {
        out.push_str(&format!("{pad}pass\n"));
    }
    out
}

/// 表达式级转译（骨架：常见节点）
pub fn emit_expr_py(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(lit) => match &lit.kind {
            LiteralKind::Str { value, .. } => format!("{value:?}"),
            LiteralKind::Bool(b) => b.to_string(),
            LiteralKind::Char(c) => format!("{c:?}"),
            other => format!("{other:?}"),
        },
        ExprKind::Path(p) => p.last().name.clone(),
        ExprKind::Binary { op, lhs, rhs } => {
            format!("{} {} {}", emit_expr_py(lhs), op.as_str(), emit_expr_py(rhs))
        }
        ExprKind::Unary { op, operand } => format!("{}{}", op.as_str(), emit_expr_py(operand)),
        ExprKind::Call { callee, args } => {
            format!(
                "{}({})",
                emit_expr_py(callee),
                args.iter().map(emit_expr_py).collect::<Vec<_>>().join(", ")
            )
        }
        ExprKind::MethodCall { receiver, method, args, .. } => {
            format!(
                "{}.{}({})",
                emit_expr_py(receiver),
                snake_case(&method.name),
                args.iter().map(emit_expr_py).collect::<Vec<_>>().join(", ")
            )
        }
        ExprKind::Field { base, field } => {
            let f = match field {
                FieldIndex::Named(id) => snake_case(&id.name),
                FieldIndex::Index(i, _) => i.to_string(),
            };
            format!("{}.{}", emit_expr_py(base), f)
        }
        ExprKind::Await(inner) => format!("await {}", emit_expr_py(inner)),
        ExprKind::Try(inner) => emit_expr_py(inner), // Result → 异常封装（P5：胶水层处理）
        ExprKind::Macro { path, .. } => format!("hsl_macro_{}()", path.last().name),
        ExprKind::Native(nb) => {
            // 原样搬运（逃生舱语义）
            format!("native_{}_inline()", nb.lang.name)
        }
        _ => "None  # 表达式待 P5 完整实现".to_string(),
    }
}

pub fn snake_case(name: &str) -> String {
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}
