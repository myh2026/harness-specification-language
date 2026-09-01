//! Rust 后端 —— HSL 与 Rust 高度同构，转译接近直译。
//!
//! 表达式覆盖：literal/path/binary/unary/call/method/field/index/slice/
//!   range/assign/compound_assign/if/if-let/match/for/while/while-let/
//!   loop/closure/return/break/continue/array/array-repeat/struct/tuple/
//!   block/async-block/try/await/cast/native/macro

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
                    Some(b) => out.push_str(&emit_block_rs(b, 1, false)),
                    None => out.push_str("    unimplemented!()\n"),
                }
                out.push_str("}\n");
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
                            Some(b) => out.push_str(&emit_block_rs(b, 2, false)),
                            None => out.push_str("        unimplemented!()\n"),
                        }
                        out.push_str("    }\n");
                    }
                }
                out.push_str("}\n");
            }
            Item::Const(c) => {
                out.push_str(&format!("pub const {}: {} = {};\n", c.name.name, rs_type(&c.ty), emit_expr_rs(&c.value)));
            }
            Item::TypeAlias(t) => {
                out.push_str(&format!("pub type {} = {};\n", t.name.name, rs_type(&t.ty)));
            }
            Item::Graph(g) => {
                out.push_str(&format!(
                    "// graph {} — scale: {:?}\n",
                    g.name.name, ctx.scale
                ));
                out.push_str("pub async fn main() -> Result<(), Box<dyn std::error::Error>> {\n");
                out.push_str("    // TODO: graph 转译（AgentLoop → loop + edge 胶水）\n");
                out.push_str("    unimplemented!()\n}\n");
            }
            _ => {
                return Err(format!("rust 后端暂不支持 {}", crate::ast::item_kind_name(item)));
            }
        }
        Ok(out)
    }
}

// ────────────────────────────────────────────────────────────────
// 类型转译
// ────────────────────────────────────────────────────────────────

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

// ────────────────────────────────────────────────────────────────
// 语句块转译
// ────────────────────────────────────────────────────────────────

/// 转译语句块。no_return_tail: 循环体等不自动给 tail 加 return 的场景。
fn emit_block_rs(block: &BlockExpr, indent: usize, no_return_tail: bool) -> String {
    let pad = "    ".repeat(indent);
    let mut out = String::new();
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let(l) => {
                let mut_prefix = if l.mutable { "mut " } else { "" };
                let pat = rs_pattern(&l.pattern);
                let ty_part = l.ty.as_ref().map(|t| format!(": {}", rs_type(t))).unwrap_or_default();
                match &l.init {
                    Some(init) => out.push_str(&format!("{pad}let {}{pat}{ty_part} = {};\n", mut_prefix, emit_expr_rs(init))),
                    None => out.push_str(&format!("{pad}let {}{pat}{ty_part};\n", mut_prefix)),
                }
                if let Some(els) = &l.else_block {
                    out.push_str(&format!("{} else ", emit_expr_rs(&Expr {
                        kind: ExprKind::Block(els.clone()),
                        span: els.span,
                    })));
                }
            }
            Stmt::Expr { expr, .. } => {
                // 语句位置的表达式：if/match/for/while/loop 不需要尾分号
                out.push_str(&emit_stmt_expr_rs(expr, &pad));
            }
            Stmt::Empty(_) => {}
            Stmt::Item(_) => out.push_str(&format!("{pad}// 局部项\n")),
        }
    }
    if let Some(tail) = &block.tail {
        if no_return_tail {
            out.push_str(&format!("{pad}{}\n", emit_expr_rs(tail)));
        } else {
            // 尾位置 if/match: 作为表达式直接输出（Rust 块表达式）
            match &tail.kind {
                ExprKind::If { .. } | ExprKind::Match { .. } => {
                    out.push_str(&emit_expr_rs(tail));
                }
                _ => out.push_str(&format!("{pad}{}\n", emit_expr_rs(tail))),
            }
        }
    }
    if out.is_empty() {
        out.push_str(&format!("{pad}()\n"));
    }
    out
}

/// 语句位置的表达式（if/while/for/loop/match 在语句位置无需包装）
fn emit_stmt_expr_rs(expr: &Expr, pad: &str) -> String {
    match &expr.kind {
        ExprKind::If { cond, then, else_ } => {
            let mut out = String::new();
            out.push_str(&format!("{}if {} {{\n", pad, emit_expr_rs(cond)));
            out.push_str(&emit_block_rs(then, pad.len() / 4 + 1, false));
            if let Some(els) = else_ {
                // else if 链
                if let ExprKind::If { .. } = &els.kind {
                    out.push_str(&format!("{}}} else ", pad));
                    out.push_str(&emit_stmt_expr_rs(els, pad));
                } else {
                    out.push_str(&format!("{}}} else {{\n", pad));
                    if let ExprKind::Block(b) = &els.kind {
                        out.push_str(&emit_block_rs(b, pad.len() / 4 + 1, false));
                    } else {
                        out.push_str(&format!("{}{}\n", "    ".repeat(pad.len() / 4 + 1), emit_expr_rs(els)));
                    }
                    out.push_str(&format!("{}}}\n", pad));
                }
            } else {
                out.push_str(&format!("{}}}\n", pad));
            }
            out
        }
        ExprKind::Match { scrutinee, arms } => {
            let mut out = String::new();
            out.push_str(&format!("{}match {} {{\n", pad, emit_expr_rs(scrutinee)));
            for arm in arms {
                let guard = arm.guard.as_ref()
                    .map(|g| format!(" if {}", emit_expr_rs(g)))
                    .unwrap_or_default();
                out.push_str(&format!("{}    {}{} => ", pad, rs_pattern(&arm.pattern), guard));
                if let ExprKind::Block(b) = &arm.body.kind {
                    out.push_str("{\n");
                    out.push_str(&emit_block_rs(b, pad.len() / 4 + 2, false));
                    out.push_str(&format!("{}    }}\n", pad));
                } else {
                    out.push_str(&format!("{},\n", emit_expr_rs(&arm.body)));
                }
            }
            out.push_str(&format!("{}}}\n", pad));
            out
        }
        ExprKind::While { cond, body, .. } => {
            let mut out = String::new();
            out.push_str(&format!("{}while {} {{\n", pad, emit_expr_rs(cond)));
            out.push_str(&emit_block_rs(body, pad.len() / 4 + 1, true));
            out.push_str(&format!("{}}}\n", pad));
            out
        }
        ExprKind::WhileLet { pattern, expr, body, .. } => {
            let mut out = String::new();
            out.push_str(&format!("{}while let {} = {} {{\n", pad, rs_pattern(pattern), emit_expr_rs(expr)));
            out.push_str(&emit_block_rs(body, pad.len() / 4 + 1, true));
            out.push_str(&format!("{}}}\n", pad));
            out
        }
        ExprKind::For { pattern, iter, body, .. } => {
            let mut out = String::new();
            out.push_str(&format!("{}for {} in {} {{\n", pad, rs_pattern(pattern), emit_expr_rs(iter)));
            out.push_str(&emit_block_rs(body, pad.len() / 4 + 1, true));
            out.push_str(&format!("{}}}\n", pad));
            out
        }
        ExprKind::Loop { body, .. } => {
            let mut out = String::new();
            out.push_str(&format!("{}loop {{\n", pad));
            out.push_str(&emit_block_rs(body, pad.len() / 4 + 1, true));
            out.push_str(&format!("{}}}\n", pad));
            out
        }
        ExprKind::IfLet { pattern, expr, then, else_ } => {
            let mut out = String::new();
            out.push_str(&format!("{}if let {} = {} {{\n", pad, rs_pattern(pattern), emit_expr_rs(expr)));
            out.push_str(&emit_block_rs(then, pad.len() / 4 + 1, false));
            if let Some(els) = else_ {
                if let ExprKind::If { .. } = &els.kind {
                    out.push_str(&format!("{}}} else ", pad));
                    out.push_str(&emit_stmt_expr_rs(els, pad));
                } else {
                    out.push_str(&format!("{}}} else {{\n", pad));
                    if let ExprKind::Block(b) = &els.kind {
                        out.push_str(&emit_block_rs(b, pad.len() / 4 + 1, false));
                    } else {
                        out.push_str(&format!("{}{}\n", "    ".repeat(pad.len() / 4 + 1), emit_expr_rs(els)));
                    }
                    out.push_str(&format!("{}}}\n", pad));
                }
            } else {
                out.push_str(&format!("{}}}\n", pad));
            }
            out
        }
        ExprKind::Assign { .. } | ExprKind::CompoundAssign { .. } => {
            // 赋值语句必须加分号
            format!("{}{};\n", pad, emit_expr_rs(expr))
        }
        _ => {
            // 普通表达式语句
            format!("{}{}\n", pad, emit_expr_rs(expr))
        }
    }
}

// ────────────────────────────────────────────────────────────────
// 模式转译
// ────────────────────────────────────────────────────────────────

/// 模式转译（Rust 与 HSL 模式高度同构，接近直译）
fn rs_pattern(pat: &Pattern) -> String {
    match &pat.kind {
        PatternKind::Ident { mutable, name, sub } => {
            let prefix = if *mutable { "mut " } else { "" };
            match sub {
                Some(inner) => format!("{}{} @ {}", prefix, name.name, rs_pattern(inner)),
                None => format!("{}{}", prefix, name.name),
            }
        }
        PatternKind::Wildcard => "_".into(),
        PatternKind::Rest => "..".into(),
        PatternKind::Literal(lit) => lit.raw.clone(),
        PatternKind::Path(p) => {
            let segs: Vec<&str> = p.segments.iter().map(|s| s.name.as_str()).collect();
            segs.join("::")
        }
        PatternKind::TupleStruct { path, elems, rest_at } => {
            let name: Vec<&str> = path.segments.iter().map(|s| s.name.as_str()).collect();
            let inner: Vec<String> = elems.iter().map(rs_pattern).collect();
            let rest = if let Some(pos) = rest_at {
                // 在 pos 位置插入 ..
                let mut v = inner;
                v.insert(*pos, "..".into());
                v
            } else {
                inner
            };
            format!("{}({})", name.join("::"), rest.join(", "))
        }
        PatternKind::Struct { path, fields, rest } => {
            let name: Vec<&str> = path.segments.iter().map(|s| s.name.as_str()).collect();
            let fields_str = fields.iter().map(|f| {
                let pat = f.pattern.as_ref().map(|p| rs_pattern(p)).unwrap_or_else(|| f.name.name.clone());
                format!("{}: {}", f.name.name, pat)
            }).collect::<Vec<_>>().join(", ");
            let rest_str = if *rest { ", .." } else { "" };
            format!("{} {{ {}{} }}", name.join("::"), fields_str, rest_str)
        }
        PatternKind::Tuple { elems, rest_at } => {
            let inner: Vec<String> = elems.iter().map(rs_pattern).collect();
            let rest = if let Some(pos) = rest_at {
                let mut v = inner;
                v.insert(*pos, "..".into());
                v
            } else {
                inner
            };
            format!("({})", rest.join(", "))
        }
        PatternKind::Or(pats) => {
            pats.iter().map(rs_pattern).collect::<Vec<_>>().join(" | ")
        }
        PatternKind::Range { lo, hi, inclusive } => {
            let op = if *inclusive { "..=" } else { ".." };
            format!("{} {} {}", rs_pattern(lo), op, rs_pattern(hi))
        }
    }
}

// ────────────────────────────────────────────────────────────────
// 表达式转译
// ────────────────────────────────────────────────────────────────

/// 表达式级转译（HSL 与 Rust 高度同构，多数节点直译）
pub fn emit_expr_rs(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(lit) => lit.raw.clone(),
        ExprKind::Path(p) => {
            let segs: Vec<&str> = p.segments.iter().map(|s| s.name.as_str()).collect();
            segs.join("::")
        }
        ExprKind::Binary { op, lhs, rhs } => {
            format!("({} {} {})", emit_expr_rs(lhs), op.as_str(), emit_expr_rs(rhs))
        }
        ExprKind::Unary { op, operand } => format!("{}{}", op.as_str(), emit_expr_rs(operand)),
        ExprKind::Call { callee, args } => {
            format!(
                "{}({})",
                emit_expr_rs(callee),
                args.iter().map(emit_expr_rs).collect::<Vec<_>>().join(", ")
            )
        }
        ExprKind::MethodCall { receiver, method, generic_args, args } => {
            let turbo = if generic_args.is_empty() {
                String::new()
            } else {
                format!("::<{}>", generic_args.iter().map(map_generic).collect::<Vec<_>>().join(", "))
            };
            format!(
                "{}.{}{}({})",
                emit_expr_rs(receiver),
                method.name,
                turbo,
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
        ExprKind::Index { base, index } => {
            format!("{}[{}]", emit_expr_rs(base), emit_expr_rs(index))
        }
        ExprKind::Slice { base, range } => {
            let base_str = emit_expr_rs(base);
            let lo = range.lo.as_ref().map(|e| emit_expr_rs(e)).unwrap_or_default();
            let hi = range.hi.as_ref().map(|e| emit_expr_rs(e)).unwrap_or_default();
            if range.inclusive {
                format!("{}[{}..={} ]", base_str, lo, hi)
            } else {
                format!("{}[{}..{} ]", base_str, lo, hi)
            }
        }
        ExprKind::Range(r) => {
            let lo = r.lo.as_ref().map(|e| emit_expr_rs(e)).unwrap_or_default();
            let hi = r.hi.as_ref().map(|e| emit_expr_rs(e)).unwrap_or_default();
            let op = if r.inclusive { "..=" } else { ".." };
            format!("{}{}{}", lo, op, hi)
        }
        ExprKind::Assign { lhs, rhs } => {
            format!("{} = {}", emit_expr_rs(lhs), emit_expr_rs(rhs))
        }
        ExprKind::CompoundAssign { op, lhs, rhs } => {
            format!("{} {}= {}", emit_expr_rs(lhs), op.as_str(), emit_expr_rs(rhs))
        }
        ExprKind::If { cond, then, else_ } => {
            let mut out = String::new();
            out.push_str(&format!("if {} {{ ", emit_expr_rs(cond)));
            out.push_str(&emit_block_rs(then, 0, false));
            if let Some(els) = else_ {
                if let ExprKind::If { .. } = &els.kind {
                    out.push_str("} else ");
                    out.push_str(&emit_expr_rs(els));
                } else {
                    out.push_str("} else {");
                    if let ExprKind::Block(b) = &els.kind {
                        out.push_str(&emit_block_rs(b, 0, false));
                    } else {
                        out.push_str(&emit_expr_rs(els));
                    }
                    out.push_str("}");
                }
            } else {
                out.push_str("}");
            }
            out
        }
        ExprKind::IfLet { pattern, expr, then, else_ } => {
            let mut out = String::new();
            out.push_str(&format!("if let {} = {} {{ ", rs_pattern(pattern), emit_expr_rs(expr)));
            out.push_str(&emit_block_rs(then, 0, false));
            if let Some(els) = else_ {
                if let ExprKind::If { .. } = &els.kind {
                    out.push_str("} else ");
                    out.push_str(&emit_expr_rs(els));
                } else {
                    out.push_str("} else {");
                    if let ExprKind::Block(b) = &els.kind {
                        out.push_str(&emit_block_rs(b, 0, false));
                    } else {
                        out.push_str(&emit_expr_rs(els));
                    }
                    out.push_str("}");
                }
            } else {
                out.push_str("}");
            }
            out
        }
        ExprKind::Match { scrutinee, arms } => {
            let mut out = String::new();
            out.push_str(&format!("match {} {{ ", emit_expr_rs(scrutinee)));
            for arm in arms {
                let guard = arm.guard.as_ref()
                    .map(|g| format!(" if {}", emit_expr_rs(g)))
                    .unwrap_or_default();
                out.push_str(&format!("{}{} => ", rs_pattern(&arm.pattern), guard));
                if let ExprKind::Block(b) = &arm.body.kind {
                    out.push_str("{");
                    out.push_str(&emit_block_rs(b, 0, false));
                    out.push_str("} ");
                } else {
                    out.push_str(&format!("{}, ", emit_expr_rs(&arm.body)));
                }
            }
            out.push_str("}");
            out
        }
        ExprKind::Loop { body, label } => {
            let label_str = label.as_ref().map(|l| format!("'{}: ", l.name)).unwrap_or_default();
            format!("{}loop {{ {} }}", label_str, emit_block_rs(body, 0, true))
        }
        ExprKind::While { cond, body, label } => {
            let label_str = label.as_ref().map(|l| format!("'{}: ", l.name)).unwrap_or_default();
            format!("{}while {} {{ {} }}", label_str, emit_expr_rs(cond), emit_block_rs(body, 0, true))
        }
        ExprKind::WhileLet { pattern, expr, body, label } => {
            let label_str = label.as_ref().map(|l| format!("'{}: ", l.name)).unwrap_or_default();
            format!("{}while let {} = {} {{ {} }}", label_str, rs_pattern(pattern), emit_expr_rs(expr), emit_block_rs(body, 0, true))
        }
        ExprKind::For { pattern, iter, body, label } => {
            let label_str = label.as_ref().map(|l| format!("'{}: ", l.name)).unwrap_or_default();
            format!("{}for {} in {} {{ {} }}", label_str, rs_pattern(pattern), emit_expr_rs(iter), emit_block_rs(body, 0, true))
        }
        ExprKind::Closure { is_move, is_async, params, ret, body } => {
            let move_str = if *is_move { "move " } else { "" };
            let async_str = if *is_async { "async " } else { "" };
            let ret_str = ret.as_ref().map(|t| format!(" -> {}", rs_type(t))).unwrap_or_default();
            let params_str: Vec<String> = params.iter().map(|p| {
                match &p.kind {
                    ParamKind::Pattern(pat) => rs_pattern(pat),
                    ParamKind::Self_(_) => "self".to_string(),
                }
            }).collect();
            format!("{}{}|{}|{} {}", move_str, async_str, params_str.join(", "), ret_str, emit_expr_rs(body))
        }
        ExprKind::Return(val) => {
            match val {
                Some(v) => format!("return {}", emit_expr_rs(v)),
                None => "return".into(),
            }
        }
        ExprKind::Break { label, value } => {
            let label_str = label.as_ref().map(|l| format!("'{} ", l.name)).unwrap_or_default();
            match value {
                Some(v) => format!("break {}{}", label_str, emit_expr_rs(v)),
                None => format!("break {}", label_str),
            }
        }
        ExprKind::Continue { label } => {
            let label_str = label.as_ref().map(|l| format!("'{}", l.name)).unwrap_or_default();
            format!("continue {}", label_str)
        }
        ExprKind::Block(b) => {
            let mut out = String::new();
            out.push_str("{");
            out.push_str(&emit_block_rs(b, 0, false));
            out.push_str("}");
            out
        }
        ExprKind::AsyncBlock { is_move, body } => {
            let move_str = if *is_move { "move " } else { "" };
            format!("{}async {{ {} }}", move_str, emit_block_rs(body, 0, false))
        }
        ExprKind::Array(elems) => {
            format!("[{}]", elems.iter().map(emit_expr_rs).collect::<Vec<_>>().join(", "))
        }
        ExprKind::ArrayRepeat { elem, count } => {
            format!("[{}; {}]", emit_expr_rs(elem), emit_expr_rs(count))
        }
        ExprKind::Struct { path, fields, spread } => {
            let name: Vec<&str> = path.segments.iter().map(|s| s.name.as_str()).collect();
            let fields_str = fields.iter().map(|f| {
                let fname = match &f.name {
                    FieldIndex::Named(id) => id.name.clone(),
                    FieldIndex::Index(i, _) => format!("{}", i),
                };
                let val = f.value.as_ref().map(|v| emit_expr_rs(v)).unwrap_or_else(|| fname.clone());
                format!("{}: {}", fname, val)
            }).collect::<Vec<_>>().join(", ");
            let spread_str = if let Some(spread) = spread {
                format!(", ..{}", emit_expr_rs(spread))
            } else {
                String::new()
            };
            format!("{} {{ {}{} }}", name.join("::"), fields_str, spread_str)
        }
        ExprKind::Tuple(elems) => {
            format!("({})", elems.iter().map(emit_expr_rs).collect::<Vec<_>>().join(", "))
        }
        ExprKind::Await(inner) => format!("{}.await", emit_expr_rs(inner)),
        ExprKind::Try(inner) => format!("{}?", emit_expr_rs(inner)),
        ExprKind::Cast { expr, ty } => format!("{} as {}", emit_expr_rs(expr), rs_type(ty)),
        ExprKind::Native(nb) => {
            nb.code.trim().to_string()
        }
        ExprKind::Macro { path, args } => {
            let delim = match args.delim {
                Delimiter::Paren => "(",
                Delimiter::Bracket => "[",
                Delimiter::Brace => "{",
            };
            let inner = args.tokens.iter().map(|tt| match tt {
                TokenTree::Token(tok, _) => match tok {
                    Token::Ident(s) | Token::RawIdent(s) => s.clone(),
                    Token::Literal(lit) => lit.raw.clone(),
                    Token::Punct(s) => s.clone(),
                    Token::Label(s) => s.clone(),
                },
                TokenTree::Delimited { delim: d, tokens, .. } => {
                    let d_ch = match d { Delimiter::Paren => "(", Delimiter::Bracket => "[", Delimiter::Brace => "{" };
                    let inner = tokens.iter().map(|t| match t {
                        TokenTree::Token(tok, _) => match tok {
                            Token::Ident(s) | Token::RawIdent(s) => s.clone(),
                            Token::Literal(lit) => lit.raw.clone(),
                            Token::Punct(s) => s.clone(),
                            Token::Label(s) => s.clone(),
                        },
                        _ => "...".into(),
                    }).collect::<Vec<_>>().join(" ");
                    let close = match d { Delimiter::Paren => ")", Delimiter::Bracket => "]", Delimiter::Brace => "}" };
                    format!("{}{}{}", d_ch, inner, close)
                }
            }).collect::<Vec<_>>().join(" ");
            let name: Vec<&str> = path.segments.iter().map(|s| s.name.as_str()).collect();
            let close = match args.delim { Delimiter::Paren => ")", Delimiter::Bracket => "]", Delimiter::Brace => "}" };
            format!("{}!{}{}{}", name.join("::"), delim, inner, close)
        }
    }
}
