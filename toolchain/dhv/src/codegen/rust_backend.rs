//! Rust 后端（P3 骨架）—— 类型映射 + 项转译基础
//! Rust 是 HSL 类型系统的哲学来源，转译保真度最高：struct/enum/trait/fn 近似直译。

use crate::ast::*;
use crate::codegen::{CodegenBackend, CodegenContext};

pub struct RustBackend;

impl CodegenBackend for RustBackend {
    fn lang(&self) -> &'static str { "rust" }

    fn emit_item(&self, ctx: &CodegenContext, item: &Item) -> Result<String, String> {
        let mut out = String::new();
        out.push_str(&format!("// {}\n", crate::sourcemap::generated_header("rust")));
        match item {
            Item::Struct(s) => {
                out.push_str("#[derive(Debug, Clone)]\n");
                out.push_str(&format!("pub struct {} ", s.name.name));
                match &s.kind {
                    StructKind::Named(fields) => {
                        out.push_str("{\n");
                        for f in fields {
                            let name = f.name.as_ref().map(|n| n.name.as_str()).unwrap_or("_");
                            out.push_str(&format!("    pub {}: {},\n", name, rs_type(&f.ty)));
                        }
                        out.push_str("}\n");
                    }
                    StructKind::Tuple(fields) => {
                        out.push('(');
                        out.push_str(&fields.iter().map(|f| rs_type(&f.ty)).collect::<Vec<_>>().join(", "));
                        out.push_str(");\n");
                    }
                    StructKind::Unit => out.push_str(";\n"),
                }
            }
            Item::Enum(e) => {
                out.push_str("#[derive(Debug, Clone)]\n");
                out.push_str(&format!("pub enum {} {{\n", e.name.name));
                for v in &e.variants {
                    match &v.fields {
                        StructKind::Named(fields) => {
                            out.push_str(&format!("    {} {{\n", v.name.name));
                            for f in fields {
                                let name = f.name.as_ref().map(|n| n.name.as_str()).unwrap_or("_");
                                out.push_str(&format!("        {}: {},\n", name, rs_type(&f.ty)));
                            }
                            out.push_str("    },\n");
                        }
                        StructKind::Tuple(fields) => {
                            let inner = fields.iter().map(|f| rs_type(&f.ty)).collect::<Vec<_>>().join(", ");
                            out.push_str(&format!("    {}({}),\n", v.name.name, inner));
                        }
                        StructKind::Unit => out.push_str(&format!("    {},\n", v.name.name)),
                    }
                }
                out.push_str("}\n");
            }
            Item::Trait(t) => {
                out.push_str(&format!("pub trait {} {{\n", t.name.name));
                for ti in &t.items {
                    if let TraitItem::FnSig(sig) = ti {
                        let ret = sig.ret.as_ref().map(|t| format!(" -> {}", rs_type(t))).unwrap_or_default();
                        out.push_str(&format!(
                            "    {}fn {}({}){};\n",
                            if sig.is_async { "async " } else { "" },
                            sig.name.name,
                            sig.params.iter().map(rs_param).collect::<Vec<_>>().join(", "),
                            ret
                        ));
                    }
                }
                out.push_str("}\n");
            }
            Item::Fn(f) => {
                let ret = f.ret.as_ref().map(|t| format!(" -> {}", rs_type(t))).unwrap_or_default();
                out.push_str(&format!(
                    "pub {}fn {}({}){} {{\n",
                    if f.is_async { "async " } else { "" },
                    f.name.name,
                    f.params.iter().map(rs_param).collect::<Vec<_>>().join(", "),
                    ret
                ));
                match &f.body {
                    Some(b) => out.push_str(&emit_block_rs(b, 1)),
                    None => out.push_str("    unimplemented!()\n"),
                }
                out.push_str("}\n");
            }
            Item::Graph(g) => {
                // graph → 入口函数骨架；microkernel 尺度下由 CodegenContext 决定插件化形态
                out.push_str(&format!(
                    "// graph {} — scale: {:?}（节点=函数/Plugin，边=调用/事件订阅）\n",
                    g.name.name, ctx.scale
                ));
                out.push_str("pub async fn main() -> Result<(), Box<dyn std::error::Error>> {\n");
                out.push_str("    // P3 完整版：AgentLoop 转译 + edge 胶水生成\n");
                out.push_str("    unimplemented!()\n}\n");
            }
            Item::Impl(imp) => {
                if let Some(trait_ty) = &imp.trait_ty {
                    out.push_str(&format!("impl {} for {} {{\n", rs_type(trait_ty), rs_type(&imp.self_ty)));
                } else {
                    out.push_str(&format!("impl {} {{\n", rs_type(&imp.self_ty)));
                }
                for ii in &imp.items {
                    if let ImplItem::Fn(f) = ii {
                        let ret = f.ret.as_ref().map(|t| format!(" -> {}", rs_type(t))).unwrap_or_default();
                        out.push_str(&format!(
                            "    {}fn {}({}){} {{\n",
                            if f.is_async { "async " } else { "" },
                            f.name.name,
                            f.params.iter().map(rs_param).collect::<Vec<_>>().join(", "),
                            ret
                        ));
                        match &f.body {
                            Some(b) => out.push_str(&emit_block_rs(b, 2)),
                            None => out.push_str("        unimplemented!()\n"),
                        }
                        out.push_str("    }\n");
                    }
                }
                out.push_str("}\n");
            }
            _ => {
                return Err(format!("rust 后端暂不支持 {}", crate::ast::item_kind_name(item)));
            }
        }
        Ok(out)
    }
}

pub fn rs_type(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(p) => {
            let name = p.path.segments.last().map(|s| s.name.clone()).unwrap_or_default();
            let generic_part = if p.generic_args.is_empty() {
                String::new()
            } else {
                format!(
                    "<{}>",
                    p.generic_args.iter().map(map_generic).collect::<Vec<_>>().join(", ")
                )
            };
            format!("{name}{generic_part}")
        }
        TypeKind::Ref { mutable, inner } => {
            format!("&{}{}", if *mutable { "mut " } else { "" }, rs_type(inner))
        }
        TypeKind::Tuple(elems) => format!("({})", elems.iter().map(rs_type).collect::<Vec<_>>().join(", ")),
        TypeKind::Array { elem, len } => match &len.kind {
            ConstArgKind::Literal(lit) => format!("[{}; {}]", rs_type(elem), lit.raw),
            ConstArgKind::Block(_) => format!("[{}; N]", rs_type(elem)),
        },
        TypeKind::Slice(elem) => format!("[{}]", rs_type(elem)),
        TypeKind::Never => "!".into(),
        TypeKind::FnPtr { params, ret } => {
            let ret_s = ret.as_ref().map(|t| format!(" -> {}", rs_type(t))).unwrap_or_default();
            format!("fn({}){}", params.iter().map(rs_type).collect::<Vec<_>>().join(", "), ret_s)
        }
        TypeKind::DynTrait(bounds) => {
            let traits = bounds.iter().map(|b| rs_type(&b.ty)).collect::<Vec<_>>().join(" + ");
            format!("dyn {traits}")
        }
        TypeKind::ImplTrait(bounds) => {
            let traits = bounds.iter().map(|b| rs_type(&b.ty)).collect::<Vec<_>>().join(" + ");
            format!("impl {traits}")
        }
        TypeKind::Infer => "_".into(),
        TypeKind::Paren(inner) => format!("({})", rs_type(inner)),
    }
}

fn map_generic(arg: &GenericArg) -> String {
    match arg {
        GenericArg::Type(t) => rs_type(t),
        GenericArg::Const(c) => match &c.kind {
            ConstArgKind::Literal(lit) => lit.raw.clone(),
            ConstArgKind::Block(_) => "N".to_string(),
        },
    }
}

fn rs_param(p: &Param) -> String {
    if let ParamKind::Self_(kind) = &p.kind {
        return match kind {
            SelfKind::Value => "self".into(),
            SelfKind::Mut => "mut self".into(),
            SelfKind::Ref => "&self".into(),
            SelfKind::RefMut => "&mut self".into(),
        };
    }
    let pat = match &p.kind {
        ParamKind::Pattern(pat) => match &pat.kind {
            PatternKind::Ident { mutable, name, .. } => {
                if *mutable { format!("mut {}", name.name) } else { name.name.clone() }
            }
            _ => "arg".to_string(),
        },
        ParamKind::Self_(_) => "self".to_string(),
    };
    format!("{pat}: {}", rs_type(&p.ty))
}

fn emit_block_rs(block: &BlockExpr, indent: usize) -> String {
    let pad = "    ".repeat(indent);
    let mut out = String::new();
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let(l) => {
                let pat = match &l.pattern.kind {
                    PatternKind::Ident { mutable, name, .. } => {
                        if *mutable { format!("mut {}", name.name) } else { name.name.clone() }
                    }
                    PatternKind::Wildcard => "_".into(),
                    _ => "bound".into(),
                };
                let ty_part = l.ty.as_ref().map(|t| format!(": {}", rs_type(t))).unwrap_or_default();
                match &l.init {
                    Some(init) => out.push_str(&format!("{pad}let {pat}{ty_part} = {};\n", emit_expr_rs(init))),
                    None => out.push_str(&format!("{pad}let {pat}{ty_part};\n")),
                }
            }
            Stmt::Expr { expr, .. } => out.push_str(&format!("{pad}{};\n", emit_expr_rs(expr))),
            Stmt::Empty(_) => {}
            Stmt::Item(_) => out.push_str(&format!("{pad}// 局部项: P3 完整版\n")),
        }
    }
    if let Some(tail) = &block.tail {
        out.push_str(&format!("{pad}{}\n", emit_expr_rs(tail)));
    }
    if out.is_empty() {
        out.push_str(&format!("{pad}unimplemented!()\n"));
    }
    out
}

/// 表达式级转译（骨架：HSL 与 Rust 高度同构，多数节点直译）
pub fn emit_expr_rs(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(lit) => lit.raw.clone(),
        ExprKind::Path(p) => {
            let segs: Vec<&str> = p.segments.iter().map(|s| s.name.as_str()).collect();
            segs.join("::")
        }
        ExprKind::Binary { op, lhs, rhs } => {
            format!("{} {} {}", emit_expr_rs(lhs), op.as_str(), emit_expr_rs(rhs))
        }
        ExprKind::Unary { op, operand } => format!("{}{}", op.as_str(), emit_expr_rs(operand)),
        ExprKind::Call { callee, args } => {
            format!(
                "{}({})",
                emit_expr_rs(callee),
                args.iter().map(emit_expr_rs).collect::<Vec<_>>().join(", ")
            )
        }
        ExprKind::MethodCall { receiver, method, args, .. } => {
            format!(
                "{}.{}({})",
                emit_expr_rs(receiver),
                method.name,
                args.iter().map(emit_expr_rs).collect::<Vec<_>>().join(", ")
            )
        }
        ExprKind::Field { base, field } => {
            let f = match field {
                FieldIndex::Named(id) => id.name.clone(),
                FieldIndex::Index(i, _) => i.to_string(),
            };
            format!("{}.{}", emit_expr_rs(base), f)
        }
        ExprKind::Await(inner) => format!("{}.await", emit_expr_rs(inner)),
        ExprKind::Try(inner) => format!("{}?", emit_expr_rs(inner)),
        ExprKind::Cast { expr, ty } => format!("{} as {}", emit_expr_rs(expr), rs_type(ty)),
        ExprKind::Native(nb) => {
            // native rust 块：原样内联（同为 rust 时零成本）；其他语言 → FFI 胶水（P8）
            nb.code.trim().to_string()
        }
        _ => "/* 表达式待 P3 完整实现 */".to_string(),
    }
}
