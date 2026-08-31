//! Go backend (Logic tier) - type mapping + full function body translation
//! Go 是 Tier 1 Harness 核心语言，本后端生成合法 Go 代码。
//! struct → type X struct{}; enum → const iota / interface+struct;
//! fn → func; graph → func main() error;
//! 表达式覆盖：binary/unary/call/method/field/await/cast/if/match/for/while
//!   assign/compound_assign/index/slice/array/struct/closure/return/break/continue/block/group/try/range

use crate::ast::*;
use crate::codegen::{CodegenBackend, CodegenContext};

pub struct GoBackend;

impl CodegenBackend for GoBackend {
    fn lang(&self) -> &'static str { "go" }

    fn emit_item(&self, ctx: &CodegenContext, item: &Item) -> Result<String, String> {
        let mut out = String::new();
        out.push_str(&format!("// {}\n", crate::sourcemap::generated_header("go")));
        out.push_str("package hsl\n\n");
        match item {
            Item::Struct(s) => {
                out.push_str(&go_struct(s));
            }
            Item::Enum(e) => {
                out.push_str(&go_enum(e));
            }
            Item::Trait(t) => {
                // Go interface
                out.push_str(&format!("type {} interface {{\n", t.name.name));
                for ti in &t.items {
                    if let TraitItem::FnSig(sig) = ti {
                        let _params = sig.params.iter()
                            .filter_map(|p| go_param(p))
                            .collect::<Vec<_>>().join(", ");
                        let ret = sig.ret.as_ref()
                            .map(|t| format!(" {}", go_type(t)))
                            .unwrap_or_default();
                        out.push_str(&format!(
                            "    {}({}){}\n",
                            if sig.is_async { "// async " } else { "" },
                            export_name(&sig.name.name),
                            ret
                        ));
                    }
                }
                out.push_str("}\n");
            }
            Item::Fn(f) => {
                out.push_str(&go_fn(f));
            }
            Item::Graph(g) => {
                out.push_str(&format!(
                    "// graph {} — scale: {:?}\n",
                    g.name.name, ctx.scale
                ));
                out.push_str("func main() error {\n");
                if !g.body.is_empty() {
                    out.push_str("    // TODO: graph body GraphStmt translation\n");
                } else {
                    out.push_str("    // TODO: AgentLoop\n");
                }
                out.push_str("    return nil\n");
                out.push_str("}\n");
            }
            Item::Impl(imp) => {
                // Go 无 impl 语法 → 生成方法集（接收者参数）
                if let Some(trait_ty) = &imp.trait_ty {
                    out.push_str(&format!(
                        "// impl {} for {}\n",
                        go_type(trait_ty),
                        go_type(&imp.self_ty)
                    ));
                }
                for ii in &imp.items {
                    if let ImplItem::Fn(f) = ii {
                        let receiver = go_type(&imp.self_ty);
                        out.push_str(&format!(
                            "func (self *{}) {}({}){} {{\n",
                            &receiver,
                            export_name(&f.name.name),
                            f.params.iter().filter_map(|p| go_param(p)).collect::<Vec<_>>().join(", "),
                            f.ret.as_ref().map(|t| format!(" {}", go_type(t))).unwrap_or_default()
                        ));
                        if let Some(body) = &f.body {
                            out.push_str(&emit_block_go(body, 2));
                        } else {
                            out.push_str("    // TODO\n");
                        }
                        out.push_str("}\n");
                    }
                }
            }
            _ => {
                return Err(format!("go 后端暂不支持 {}", crate::ast::item_kind_name(item)));
            }
        }
        Ok(out)
    }
}

/// struct → type X struct { Fields... }
fn go_struct(s: &StructDef) -> String {
    let mut out = String::new();
    out.push_str(&format!("type {} struct {{\n", export_name(&s.name.name)));
    if let StructKind::Named(fields) = &s.kind {
        for f in fields {
            let name = f.name.as_ref().map(|n| export_name(&n.name)).unwrap_or_else(|| "_".to_string());
            out.push_str(&format!("    {} {}\n", name, go_type(&f.ty)));
        }
    } else if let StructKind::Tuple(fields) = &s.kind {
        for (i, f) in fields.iter().enumerate() {
            out.push_str(&format!("    Field{} {}\n", i, go_type(&f.ty)));
        }
    }
    out.push_str("}\n");
    out
}

/// enum → const iota + type X int (unit variants) 或 interface + struct variants
fn go_enum(e: &EnumDef) -> String {
    let mut out = String::new();
    let has_data = e.variants.iter().any(|v| {
        !matches!(&v.fields, StructKind::Unit)
    });
    if !has_data {
        out.push_str(&format!("type {} int\n\n", export_name(&e.name.name)));
        out.push_str("const (\n");
        for (i, v) in e.variants.iter().enumerate() {
            let name = export_name(&v.name.name);
            out.push_str(&format!("    {} {} = iota\n", name, name));
            if i == 0 {
                out.push_str("    // ...\n");
            }
        }
        out.push_str(")\n");
    } else {
        let iface_name = export_name(&e.name.name);
        out.push_str(&format!("type {} interface {{\n", iface_name));
        out.push_str(&format!("    is{}()\n", iface_name));
        out.push_str("}\n\n");
        for v in &e.variants {
            let vname = export_name(&v.name.name);
            match &v.fields {
                StructKind::Unit => {
                    out.push_str(&format!("type {} struct {{}}\n", vname));
                }
                StructKind::Named(fields) => {
                    out.push_str(&format!("type {} struct {{\n", vname));
                    for f in fields {
                        let fname = f.name.as_ref().map(|n| export_name(&n.name)).unwrap_or_else(|| "_".to_string());
                        out.push_str(&format!("    {} {}\n", fname, go_type(&f.ty)));
                    }
                    out.push_str("}\n");
                }
                StructKind::Tuple(fields) => {
                    out.push_str(&format!("type {} struct {{\n", vname));
                    for (i, f) in fields.iter().enumerate() {
                        out.push_str(&format!("    Field{} {}\n", i, go_type(&f.ty)));
                    }
                    out.push_str("}\n");
                }
            }
        }
    }
    out
}

/// fn → func 签名 + 函数体
fn go_fn(f: &FnDef) -> String {
    let mut out = String::new();
    let params = f.params.iter()
        .filter_map(|p| go_param(p))
        .collect::<Vec<_>>().join(", ");
    let ret = f.ret.as_ref()
        .map(|t| format!(" {}", go_type(t)))
        .unwrap_or_default();
    out.push_str(&format!(
        "func {}({}){} {{\n",
        export_name(&f.name.name),
        params,
        ret
    ));
    if let Some(body) = &f.body {
        out.push_str(&emit_block_go(body, 1));
    } else {
        out.push_str("    // TODO\n");
    }
    out.push_str("}\n");
    out
}

/// Go 类型映射
fn go_type(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(p) => {
            let name = p.path.segments.last().map(|s| s.name.clone()).unwrap_or_default();
            let base = match name.as_str() {
                "bool" => "bool".to_string(),
                "String" | "str" => "string".to_string(),
                "i8" | "i16" | "i32" | "i64" | "i128" | "isize"
                | "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => "int".to_string(),
                "f32" | "f64" => "float64".to_string(),
                "Vec" => {
                    let elem = p.generic_args.iter().next().map(go_generic).unwrap_or_else(|| "any".to_string());
                    return format!("[]{}", elem);
                }
                "HashMap" => "map[string]any".to_string(),
                "HashSet" => "map[any]bool".to_string(),
                "Option" => format!("*{}", p.generic_args.iter().next().map(go_generic).unwrap_or_else(|| "any".to_string())),
                "Result" => "(any, error)".to_string(),
                "Box" => p.generic_args.iter().next().map(go_generic).unwrap_or_else(|| "any".to_string()),
                other => export_name(other),
            };
            if p.generic_args.is_empty() {
                base
            } else {
                format!("{}[{}]", base, p.generic_args.iter().map(go_generic).collect::<Vec<_>>().join(", "))
            }
        }
        TypeKind::Ref { inner, .. } => go_type(inner), // Go 指针隐式可变；GC 引用语义不同
        TypeKind::Tuple(elems) => {
            format!("struct{{ {} }}", elems.iter().enumerate().map(|(i, t)| format!("F{} {}", i, go_type(t))).collect::<Vec<_>>().join("; "))
        }
        TypeKind::Array { elem, .. } | TypeKind::Slice(elem) => {
            format!("[]{}", go_type(elem))
        }
        TypeKind::FnPtr { params, ret } => {
            let params_go: Vec<String> = params.iter().map(go_type).collect();
            let ret_go = ret.as_ref().map(|t| go_type(t)).unwrap_or_default();
            format!("func({}) {}", params_go.join(", "), ret_go)
        }
        TypeKind::DynTrait(_) | TypeKind::ImplTrait(_) | TypeKind::Infer => "any".into(),
        TypeKind::Paren(inner) => go_type(inner),
        TypeKind::Never => "/* never */".into(),
    }
}

fn go_generic(arg: &GenericArg) -> String {
    match arg {
        GenericArg::Type(t) => go_type(t),
        GenericArg::Const(c) => match &c.kind {
            ConstArgKind::Literal(lit) => lit.raw.clone(),
            ConstArgKind::Block(_) => "0".into(),
        },
    }
}

fn go_param(p: &Param) -> Option<String> {
    match &p.kind {
        ParamKind::Self_(_) => None,
        ParamKind::Pattern(pat) => {
            let name = match &pat.kind {
                PatternKind::Ident { name, .. } => export_name(&name.name),
                _ => "arg".into(),
            };
            Some(format!("{} {}", name, go_type(&p.ty)))
        }
    }
}

/// Go 导出要求首字母大写
fn export_name(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        None => name.to_string(),
        Some(first) => first.to_uppercase().to_string() + chars.as_str(),
    }
}

// ────────────────────────────────────────────────────────────────
// 语句块转译
// ────────────────────────────────────────────────────────────────

/// Emit block statements + tail as return (helper for tail-position if/match arms)
fn emit_block_stmts_return(block: &BlockExpr, pad: &str, out: &mut String) {
    for s in &block.stmts {
        if let Stmt::Expr { expr, .. } = s {
            out.push_str(&format!("{}{}\n", pad, emit_expr_go(expr)));
        }
    }
    if let Some(tail) = &block.tail {
        out.push_str(&format!("{}return {}\n", pad, emit_expr_go(tail)));
    }
}

/// Handle the else branch of an if in tail position (with return), supporting else-if chains
fn emit_tail_else_return(out: &mut String, pad: &str, ip: &str, els: &Expr) {
    if let ExprKind::If { cond, then, else_ } = &els.kind {
        out.push_str(&format!("{}}} else if {} {{\n", pad, emit_expr_go(cond)));
        emit_block_stmts_return(then, ip, out);
        if let Some(inner_els) = else_ {
            emit_tail_else_return(out, pad, ip, inner_els);
        } else {
            out.push_str(&format!("{}}}\n", pad));
        }
    } else if let ExprKind::Block(b) = &els.kind {
        out.push_str(&format!("{}}} else {{\n", pad));
        emit_block_stmts_return(b, ip, out);
        out.push_str(&format!("{}}}\n", pad));
    } else {
        out.push_str(&format!("{}}} else {{\n", pad));
        out.push_str(&format!("{}return {}\n", ip, emit_expr_go(els)));
        out.push_str(&format!("{}}}\n", pad));
    }
}

fn emit_block_go(block: &BlockExpr, indent: usize) -> String {
    let pad = "    ".repeat(indent);
    let mut out = String::new();
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let(l) => {
                let name = go_pattern_name(&l.pattern);
                let type_ann = l.ty.as_ref().map(|t| format!(" {}", go_type(t)));
                match &l.init {
                    Some(init) => out.push_str(&format!("{}{} := {}\n", pad, name, emit_expr_go(init))),
                    None => {
                        if let Some(ty) = type_ann {
                            out.push_str(&format!("{}var {} {}\n", pad, name, ty));
                        } else {
                            out.push_str(&format!("{}var {} interface{{}}\n", pad, name));
                        }
                    }
                }
            }
            Stmt::Expr { expr, has_semi: _ } => {
                // 检查是否是需要特殊处理的语句级表达式
                let code = emit_stmt_expr_go(expr, &pad);
                out.push_str(&code);
            }
            Stmt::Empty(_) => {}
            Stmt::Item(_) => out.push_str(&format!("{}// 局部项\n", pad)),
        }
    }
    if let Some(tail) = &block.tail {
        // Tail expression -> return; use stmt-level form for if/match in tail position
        match &tail.kind {
            ExprKind::If { cond, then, else_ } => {
                let ip = "    ".repeat(pad.len() / 4 + 1);
                out.push_str(&format!("{}if {} {{\n", pad, emit_expr_go(cond)));
                emit_block_stmts_return(then, &ip, &mut out);
                if let Some(els) = else_ {
                    emit_tail_else_return(&mut out, &pad, &ip, els);
                } else {
                    out.push_str(&format!("{}}}\n", pad));
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                let ip = "    ".repeat(pad.len() / 4 + 1);
                out.push_str(&format!("{}switch {} {{\n", pad, emit_expr_go(scrutinee)));
                for arm in arms {
                    let pat_str = go_pattern(&arm.pattern);
                    out.push_str(&format!("{}case {}:\n", pad, pat_str));
                    if let ExprKind::Block(b) = &arm.body.kind {
                        emit_block_stmts_return(b, &ip, &mut out);
                        out.push_str(&format!("{}\n", ip));
                    } else {
                        out.push_str(&format!("{}return {}\n", ip, emit_expr_go(&arm.body)));
                    }
                }
                out.push_str(&format!("{}}}\n", pad));
            }
            _ => {
                out.push_str(&format!("{}return {}\n", pad, emit_expr_go(tail)));
            }
        }
    }
    if out.is_empty() {
        out.push_str(&format!("{}// TODO\n", pad));
    }
    out
}

/// Emit block without wrapping the tail expression in `return`
/// (used for loop bodies where tail is just the last iteration expression)
fn emit_block_go_no_return_tail(block: &BlockExpr, indent: usize) -> String {
    let pad = "    ".repeat(indent);
    let mut out = String::new();
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let(l) => {
                let name = go_pattern_name(&l.pattern);
                let type_ann = l.ty.as_ref().map(|t| format!(" {}", go_type(t)));
                match &l.init {
                    Some(init) => out.push_str(&format!("{}{} := {}\n", pad, name, emit_expr_go(init))),
                    None => {
                        if let Some(ty) = type_ann {
                            out.push_str(&format!("{}var {} {}\n", pad, name, ty));
                        } else {
                            out.push_str(&format!("{}var {} interface{{}}\n", pad, name));
                        }
                    }
                }
            }
            Stmt::Expr { expr, has_semi: _ } => {
                let code = emit_stmt_expr_go(expr, &pad);
                out.push_str(&code);
            }
            Stmt::Empty(_) => {}
            Stmt::Item(_) => out.push_str(&format!("{}// 局部项\n", pad)),
        }
    }
    if let Some(tail) = &block.tail {
        out.push_str(&format!("{}{}\n", pad, emit_expr_go(tail)));
    }
    if out.is_empty() {
        out.push_str(&format!("{}// TODO\n", pad));
    }
    out
}

/// 语句位置的表达式（if/while/for/match 在语句位置可以省略外层 return）
fn emit_stmt_expr_go(expr: &Expr, pad: &str) -> String {
    match &expr.kind {
        ExprKind::If { cond, then, else_ } => {
            let mut out = String::new();
            out.push_str(&format!("{}if {} {{\n", pad, emit_expr_go(cond)));
            out.push_str(&emit_block_go(then, pad.len() / 4 + 1));
            if let Some(els) = else_ {
                // else if 链
                if let ExprKind::If { .. } = &els.kind {
                    out.push_str(&format!("{}}} else ", pad));
                    out.push_str(&emit_stmt_expr_go(els, pad));
                } else {
                    out.push_str(&format!("{}}} else {{\n", pad));
                    // else 块作为语句处理
                    if let ExprKind::Block(b) = &els.kind {
                        out.push_str(&emit_block_go(b, pad.len() / 4 + 1));
                    } else {
                        out.push_str(&format!("{}return {}\n", "    ".repeat(pad.len() / 4 + 1), emit_expr_go(els)));
                    }
                    out.push_str(&format!("{}}}\n", pad));
                }
            } else {
                out.push_str(&format!("{}}}\n", pad));
            }
            out
        }
        ExprKind::While { cond, body, .. } => {
            let mut out = String::new();
            out.push_str(&format!("{}for {} {{\n", pad, emit_expr_go(cond)));
            out.push_str(&emit_block_go_no_return_tail(body, pad.len() / 4 + 1));
            out.push_str(&format!("{}}}\n", pad));
            out
        }
        ExprKind::For { pattern, iter, body, .. } => {
            let mut out = String::new();
            let var = go_pattern_name(pattern);
            out.push_str(&format!("{}for {}, {} := range {} {{\n", pad, var, var, emit_expr_go(iter)));
            out.push_str(&emit_block_go_no_return_tail(body, pad.len() / 4 + 1));
            out.push_str(&format!("{}}}\n", pad));
            out
        }
        ExprKind::Match { scrutinee, arms } => {
            emit_match_as_switch(scrutinee, arms, pad)
        }
        _ => {
            // 普通表达式语句
            format!("{}{}\n", pad, emit_expr_go(expr))
        }
    }
}

/// match → Go switch（穷尽性用 default 兜底）
fn emit_match_as_switch(scrutinee: &Expr, arms: &[MatchArm], pad: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}switch {} {{\n", pad, emit_expr_go(scrutinee)));
    for arm in arms {
        let pat_str = go_pattern(&arm.pattern);
        out.push_str(&format!("{}case {}:\n", pad, pat_str));
        let inner_pad = "    ".repeat(pad.len() / 4 + 1);
        if let ExprKind::Block(b) = &arm.body.kind {
            out.push_str(&emit_block_go(b, pad.len() / 4 + 1));
            out.push_str(&format!("{}\n", inner_pad));
        } else {
            out.push_str(&format!("{}return {}\n", inner_pad, emit_expr_go(&arm.body)));
        }
    }
    out.push_str(&format!("{}}}\n", pad));
    out
}

// ────────────────────────────────────────────────────────────────
// 模式转译（用于 match arm / for-in / let 解构）
// ────────────────────────────────────────────────────────────────

/// 从模式提取变量名（简化：仅 Ident / Wildcard / Path）
fn go_pattern_name(pat: &Pattern) -> String {
    match &pat.kind {
        PatternKind::Ident { name, .. } => export_name(&name.name),
        PatternKind::Wildcard => "_".into(),
        PatternKind::Path(p) => export_name(&p.last().name),
        _ => "bound".into(),
    }
}

/// 模式转译（用于 match case）
fn go_pattern(pat: &Pattern) -> String {
    match &pat.kind {
        PatternKind::Ident { name, .. } => export_name(&name.name),
        PatternKind::Wildcard => "_".into(),
        PatternKind::Literal(lit) => lit.raw.clone(),
        PatternKind::Path(p) => {
            if p.segments.len() >= 2 {
                // Enum variant: Some / None / Action::Stop
                let last = export_name(&p.last().name);
                if last == "None" { return "nil".into(); }
                // For simple enums (int type), compare value directly
                return last;
            }
            export_name(&p.last().name)
        }
        PatternKind::TupleStruct { path, elems, .. } => {
            let name = export_name(&path.last().name);
            if name == "None" { return "nil".into(); }
            if name == "Some" && elems.len() == 1 {
                // Option::Some(x) - Go has no direct equivalent
                let inner = go_pattern(&elems[0]);
                return format!("/* Some({}) */ true", inner);
            }
            format!("{}{{ /* {} fields */ }}", name, elems.len())
        }
        PatternKind::Struct { path, fields, rest } => {
            let name = export_name(&path.last().name);
            let fields_str = fields.iter().map(|f| {
                let fname = export_name(&f.name.name);
                if let Some(pat) = &f.pattern {
                    format!("{}: {}", fname, go_pattern(pat))
                } else {
                    format!("{}: {}", fname, fname)
                }
            }).collect::<Vec<_>>().join(", ");
            let rest_str = if *rest { ", /* .. */" } else { "" };
            format!("{}{{ {}{} }}", name, fields_str, rest_str)
        }
        PatternKind::Tuple { elems, .. } => {
            format!("struct{{ {} }}", elems.iter().enumerate().map(|(i, e)| format!("F{}: {}", i, go_pattern(e))).collect::<Vec<_>>().join("; "))
        }
        PatternKind::Or(pats) => {
            // Go switch case 支持逗号分隔多个值
            pats.iter().map(go_pattern).collect::<Vec<_>>().join(", ")
        }
        PatternKind::Range { .. } => "/* range pattern */".into(),
        PatternKind::Rest => "/* .. */".into(),
    }
}

// ────────────────────────────────────────────────────────────────
// 表达式转译
// ────────────────────────────────────────────────────────────────

fn emit_expr_go(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(lit) => go_literal(lit),
        ExprKind::Path(p) => export_name(&p.last().name),
        ExprKind::Binary { op, lhs, rhs } => {
            let op_go = go_binary_op(op);
            format!("({} {} {})", emit_expr_go(lhs), op_go, emit_expr_go(rhs))
        }
        ExprKind::Unary { op, operand } => {
            let op_go = match op.as_str() {
                "!" => "!",
                "-" => "-",
                "*" => "*",
                "&" => "&",
                other => other,
            };
            format!("{}{}", op_go, emit_expr_go(operand))
        }
        ExprKind::Call { callee, args } => {
            format!(
                "{}({})",
                emit_expr_go(callee),
                args.iter().map(emit_expr_go).collect::<Vec<_>>().join(", ")
            )
        }
        ExprKind::MethodCall { receiver, method, args, .. } => {
            format!(
                "{}.{}({})",
                emit_expr_go(receiver),
                export_name(&method.name),
                args.iter().map(emit_expr_go).collect::<Vec<_>>().join(", ")
            )
        }
        ExprKind::Field { base, field } => {
            let f = match field {
                FieldIndex::Named(id) => export_name(&id.name),
                FieldIndex::Index(i, _) => i.to_string(),
            };
            format!("{}.{}", emit_expr_go(base), f)
        }
        ExprKind::Index { base, index } => {
            format!("{}[{}]", emit_expr_go(base), emit_expr_go(index))
        }
        ExprKind::Slice { base, range } => {
            let base_str = emit_expr_go(base);
            let lo = range.lo.as_ref().map(|e| emit_expr_go(e)).unwrap_or_default();
            let hi = range.hi.as_ref().map(|e| emit_expr_go(e)).unwrap_or_default();
            if range.inclusive {
                format!("{}[{}:{}+1]", base_str, lo, hi)
            } else {
                format!("{}[{}:{}{}]", base_str, lo, ":", hi)
            }
        }
        ExprKind::Assign { lhs, rhs } => {
            format!("{} = {}", emit_expr_go(lhs), emit_expr_go(rhs))
        }
        ExprKind::CompoundAssign { op, lhs, rhs } => {
            let op_go = go_binary_op(op);
            format!("{} {}= {}", emit_expr_go(lhs), op_go, emit_expr_go(rhs))
        }
        ExprKind::If { cond, then, else_ } => {
            // if 作为表达式（三元替代）→ Go 无三元，用即时函数或直接输出
            // 在 Go 中 if-else 作为表达式需返回值时用匿名函数
            let mut out = String::new();
            out.push_str(&format!("(func() any {{ if {} {{ return {} }}", emit_expr_go(cond),
                go_block_tail_expr(then)));
            if let Some(els) = else_ {
                out.push_str(&format!(" }} else {{ return {} }}", emit_expr_go(els)));
            } else {
                out.push_str(" } return nil })()");
            }
            out.push_str(")");
            out
        }
        ExprKind::Match { scrutinee, arms } => {
            // match 作为表达式 → 匿名函数包裹 switch
            let mut out = String::new();
            out.push_str("(func() any {");
            out.push_str(&format!("switch {} {{\n", emit_expr_go(scrutinee)));
            for arm in arms {
                let pat_str = go_pattern(&arm.pattern);
                out.push_str(&format!("case {}: return {}\n", pat_str, emit_expr_go(&arm.body)));
            }
            out.push_str("default: return nil\n");
            out.push_str("} })()");
            out
        }
        ExprKind::Closure { params, body, .. } => {
            let params_go: Vec<String> = params.iter()
                .filter_map(|p| {
                    let name = match &p.kind {
                        ParamKind::Pattern(pat) => match &pat.kind {
                            PatternKind::Ident { name, .. } => export_name(&name.name),
                            _ => "arg".into(),
                        },
                        _ => return None,
                    };
                    Some(format!("{} {}", name, go_type(&p.ty)))
                })
                .collect();
            let body_str = match &body.kind {
                ExprKind::Block(b) => {
                    let mut b_out = String::new();
                    for s in &b.stmts {
                        if let Stmt::Expr { expr, .. } = s {
                            b_out.push_str(&format!("{}\n", emit_expr_go(expr)));
                        }
                    }
                    if let Some(tail) = &b.tail {
                        b_out.push_str(&format!("return {}", emit_expr_go(tail)));
                    }
                    b_out
                }
                _ => format!("return {}", emit_expr_go(body)),
            };
            format!("func({}) {{ {} }}", params_go.join(", "), body_str)
        }
        ExprKind::Array(elems) => {
            format!("[]any{{ {} }}", elems.iter().map(emit_expr_go).collect::<Vec<_>>().join(", "))
        }
        ExprKind::ArrayRepeat { elem, count } => {
            // Go 没有直接等价 → 用循环初始化的注释 + make
            format!("/* repeat: {} × {} */ make([]any, {})", emit_expr_go(elem), emit_expr_go(count), emit_expr_go(count))
        }
        ExprKind::Struct { path, fields, spread } => {
            let name = export_name(&path.last().name);
            let fields_str = fields.iter().map(|f| {
                let fname = match &f.name {
                    FieldIndex::Named(id) => export_name(&id.name),
                    FieldIndex::Index(i, _) => format!("Field{}", i),
                };
                let val = f.value.as_ref().map(|v| emit_expr_go(v)).unwrap_or_else(|| fname.clone());
                format!("{}: {}", fname, val)
            }).collect::<Vec<_>>().join(", ");
            let spread_str = if let Some(spread) = spread {
                // Go 没有结构体展开 → 注释提示
                format!(", /* ..{} */", emit_expr_go(spread))
            } else {
                String::new()
            };
            format!("{}{{ {}{} }}", name, fields_str, spread_str)
        }
        ExprKind::Block(b) => {
            let mut out = String::new();
            out.push_str("(func() any {\n");
            for s in &b.stmts {
                if let Stmt::Expr { expr, .. } = s {
                    out.push_str(&format!("{}\n", emit_expr_go(expr)));
                }
            }
            if let Some(tail) = &b.tail {
                out.push_str(&format!("return {}\n", emit_expr_go(tail)));
            }
            out.push_str("})()");
            out
        }
        ExprKind::Return(val) => {
            match val {
                Some(v) => format!("return {}", emit_expr_go(v)),
                None => "return".into(),
            }
        }
        ExprKind::Break { value, .. } => {
            match value {
                Some(v) => format!("break /* with value: {} */", emit_expr_go(v)),
                None => "break".into(),
            }
        }
        ExprKind::Continue { .. } => "continue".into(),
        ExprKind::Range(r) => {
            let lo = r.lo.as_ref().map(|e| emit_expr_go(e)).unwrap_or_else(|| "0".into());
            let hi = r.hi.as_ref().map(|e| emit_expr_go(e)).unwrap_or_else(|| "/* .. */".into());
            if r.inclusive {
                format!("/* {}..={} */", lo, hi)
            } else {
                format!("/* {}..{} */", lo, hi)
            }
        }
        ExprKind::Try(inner) => {
            // Go 用 if err != nil 模式 → 占位
            format!("/* ? {} */ {}", emit_expr_go(inner), emit_expr_go(inner))
        }
        ExprKind::Await(inner) => format!("<-{}", emit_expr_go(inner)),
        ExprKind::Cast { expr, ty } => format!("{}({})", go_type(ty), emit_expr_go(expr)),
        ExprKind::Native(nb) => nb.code.trim().to_string(),
        ExprKind::Tuple(elems) => {
            format!("struct{{ {} }}", elems.iter().enumerate().map(|(i, e)| format!("F{}: {}", i, emit_expr_go(e))).collect::<Vec<_>>().join("; "))
        }
        ExprKind::Loop { body, .. } => {
            format!("for {{ {} }}", emit_block_go(body, 0))
        }
        ExprKind::While { cond, body, .. } => {
            format!("for {} {{ {} }}", emit_expr_go(cond), emit_block_go(body, 0))
        }
        ExprKind::For { pattern, iter, body, .. } => {
            let var = go_pattern_name(pattern);
            format!("for {}, {} := range {} {{ {} }}", var, var, emit_expr_go(iter), emit_block_go(body, 0))
        }
        ExprKind::IfLet { pattern, expr, then, else_ } => {
            // if let → Go type assertion or value check
            let var = go_pattern_name(pattern);
            let mut out = String::new();
            out.push_str(&format!("if {} := {}; {} != nil {{ {} }}", var, emit_expr_go(expr), var, emit_block_go(then, 0)));
            if let Some(els) = else_ {
                out.push_str(&format!(" else {{ {} }}", emit_expr_go(els)));
            }
            out
        }
        ExprKind::WhileLet { pattern, expr, body, .. } => {
            let var = go_pattern_name(pattern);
            format!("for {} := {}; {} != nil {{ {} }}", var, emit_expr_go(expr), var, emit_block_go(body, 0))
        }
        ExprKind::AsyncBlock { body, .. } => {
            format!("/* async */ (func() any {{ {} }})()", emit_block_go(body, 0))
        }
        ExprKind::Macro { .. } => "/* macro */".into(),
    }
}

/// 块的尾表达式（用于 if 三元替代）
fn go_block_tail_expr(block: &BlockExpr) -> String {
    if let Some(tail) = &block.tail {
        emit_expr_go(tail)
    } else {
        "nil".into()
    }
}

/// Go 二元运算符映射
fn go_binary_op(op: &BinaryOp) -> &'static str {
    match op.as_str() {
        "&&" => "&&",
        "||" => "||",
        "==" => "==",
        "!=" => "!=",
        "<" => "<",
        ">" => ">",
        "<=" => "<=",
        ">=" => ">=",
        "+" => "+",
        "-" => "-",
        "*" => "*",
        "/" => "/",
        "%" => "%",
        _ => "/* unknown_op */",
    }
}

/// Go 字面量映射
fn go_literal(lit: &Literal) -> String {
    match &lit.kind {
        LiteralKind::Bool(b) => if *b { "true".into() } else { "false".into() },
        LiteralKind::Str { .. } => format!("\"{}\"", &lit.raw[1..lit.raw.len()-1]),
        LiteralKind::Char(c) => format!("'{}'", c),
        _ => lit.raw.clone(), // int/float 保持原样
    }
}
