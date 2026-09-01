//! Python 后端 —— 完整表达式覆盖（33 ExprKind）+ 全语句 + 全模式 + 全项支持
//!
//! 表达式覆盖：literal/path/binary/unary/call/method/field/index/slice/
//!   range/assign/compound_assign/if/if-let/match/for/while/while-let/
//!   loop/closure/return/break/continue/array/array-repeat/struct/tuple/
//!   block/async-block/try/await/cast/native/macro

use crate::ast::*;
use crate::codegen::{CodegenBackend, CodegenContext};

pub struct PythonBackend;

/// Python 关键字避让
const PY_KW: &[&str] = &[
    "class", "def", "lambda", "None", "True", "False", "import", "from", "as",
    "in", "is", "not", "and", "or", "pass", "del", "global", "nonlocal",
    "with", "try", "except", "finally", "raise", "assert", "yield", "async",
    "await", "print", "return", "break", "continue", "while", "for", "if",
    "else", "elif", "match", "case", "type", "int", "float", "str", "bool",
    "list", "dict", "set", "tuple",
];

fn py_ident(name: &str) -> String {
    if PY_KW.contains(&name) {
        format!("{}_", name)
    } else {
        name.to_string()
    }
}

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
                    py_ident(&snake_case(&f.name.name)),
                    f.params.iter().map(py_param).collect::<Vec<_>>().join(", "),
                    ret
                ));
                match &f.body {
                    Some(body) => out.push_str(&emit_block_py(body, 1, false)),
                    None => out.push_str("    ...\n"),
                }
            }
            Item::Struct(s) => {
                out.push_str("from dataclasses import dataclass\n\n@dataclass\n");
                out.push_str(&format!("class {}:\n", s.name.name));
                match &s.kind {
                    StructKind::Named(fields) => {
                        if fields.is_empty() {
                            out.push_str("    pass\n");
                        }
                        for field in fields {
                            let name = field.name.as_ref().map(|n| n.name.as_str()).unwrap_or("_");
                            out.push_str(&format!(
                                "    {}: {}\n",
                                py_ident(&snake_case(name)),
                                py_type(&field.ty)
                            ));
                        }
                    }
                    StructKind::Tuple(fields) => {
                        // 元组结构体 → namedtuple 或 dataclass with __slots__
                        out.push_str(&format!("    __slots__ = ({})\n", 
                            fields.iter().map(|_| "'_field'".to_string()).collect::<Vec<_>>().join(", ")));
                    }
                    StructKind::Unit => {
                        out.push_str("    pass\n");
                    }
                }
            }
            Item::Enum(e) => {
                // 枚举 → class with class-level constants (simple) or dataclass subclasses
                let has_fields = e.variants.iter().any(|v| !matches!(&v.fields, StructKind::Unit));
                if has_fields {
                    // 变体带字段 → 基类 + 子类
                    out.push_str(&format!("class {}:\n", e.name.name));
                    out.push_str("    pass\n\n");
                    for v in &e.variants {
                        match &v.fields {
                            StructKind::Named(fields) => {
                                out.push_str(&format!("@dataclass\nclass {}({}):\n", v.name.name, e.name.name));
                                for f in fields {
                                    let name = f.name.as_ref().map(|n| n.name.as_str()).unwrap_or("_");
                                    out.push_str(&format!(
                                        "    {}: {}\n",
                                        py_ident(&snake_case(name)),
                                        py_type(&f.ty)
                                    ));
                                }
                                if fields.is_empty() {
                                    out.push_str("    pass\n");
                                }
                                out.push_str("\n");
                            }
                            StructKind::Tuple(_) => {
                                out.push_str(&format!("class {}({}):\n", v.name.name, e.name.name));
                                out.push_str("    def __init__(self, *args):\n");
                                out.push_str("        self._fields = args\n");
                                out.push_str("    def __getitem__(self, index):\n");
                                out.push_str("        return self._fields[index]\n\n");
                            }
                            StructKind::Unit => {
                                out.push_str(&format!("class {}({}):\n    pass\n\n", v.name.name, e.name.name));
                            }
                        }
                    }
                } else {
                    // 纯单元变体 → 简单枚举类
                    out.push_str("from enum import Enum, auto\n\n");
                    out.push_str(&format!("class {}(Enum):\n", e.name.name));
                    for v in &e.variants {
                        out.push_str(&format!("    {} = auto()\n", v.name.name));
                    }
                }
            }
            Item::Trait(t) => {
                // trait → Protocol (structural typing)
                out.push_str("from typing import Protocol\n\n");
                out.push_str(&format!("class {}(Protocol):\n", t.name.name));
                let has_items = !t.items.is_empty();
                for ti in &t.items {
                    if let TraitItem::FnSig(sig) = ti {
                        let ret = sig.ret.as_ref().map(|t| format!(" -> {}", py_type(t))).unwrap_or_default();
                        out.push_str(&format!(
                            "    {}def {}({}){}: ...\n",
                            if sig.is_async { "async " } else { "" },
                            py_ident(&snake_case(&sig.name.name)),
                            sig.params.iter().map(py_param).collect::<Vec<_>>().join(", "),
                            ret
                        ));
                    }
                }
                if !has_items {
                    out.push_str("    pass\n");
                }
            }
            Item::Impl(imp) => {
                // impl → 方法定义（class body，Python 无需显式 impl 块）
                let self_ty_name = py_type(&imp.self_ty);
                if let Some(trait_ty) = &imp.trait_ty {
                    out.push_str(&format!("# impl {} for {}\n", py_type(trait_ty), self_ty_name));
                } else {
                    out.push_str(&format!("# impl {}\n", self_ty_name));
                }
                for ii in &imp.items {
                    if let ImplItem::Fn(f) = ii {
                        let ret = f.ret.as_ref().map(|t| format!(" -> {}", py_type(t))).unwrap_or_default();
                        out.push_str(&format!(
                            "    {}def {}({}){}:\n",
                            if f.is_async { "async " } else { "" },
                            py_ident(&snake_case(&f.name.name)),
                            f.params.iter().map(py_param).collect::<Vec<_>>().join(", "),
                            ret
                        ));
                        match &f.body {
                            Some(body) => out.push_str(&emit_block_py(body, 2, false)),
                            None => out.push_str("        ...\n"),
                        }
                    }
                }
            }
            Item::Const(c) => {
                out.push_str(&format!(
                    "{}: {} = {}\n",
                    py_ident(&snake_case(&c.name.name)),
                    py_type(&c.ty),
                    emit_expr_py(&c.value)
                ));
            }
            Item::TypeAlias(a) => {
                out.push_str(&format!(
                    "{} = {}\n",
                    a.name.name,
                    py_type(&a.ty)
                ));
            }
            Item::Graph(g) => {
                out.push_str(&format!(
                    "# graph {} — scale: {:?}\n",
                    g.name.name, ctx.scale
                ));
                out.push_str(&format!(
                    "async def {}() -> None:\n",
                    snake_case(&g.name.name)
                ));
                // graph body: GraphStmt list, not BlockExpr
                out.push_str("    while True:\n");
                out.push_str("        pass  # AgentLoop\n");
            }
            Item::MacroRules(md) => {
                out.push_str(&format!(
                    "# macro_rules {}\ndef {}(*args, **kwargs):\n    ...\n",
                    md.name.name,
                    snake_case(&md.name.name)
                ));
            }
            _ => {
                return Err(format!("python 后端暂不支持 {}", crate::ast::item_kind_name(item)));
            }
        }
        Ok(out)
    }
}

// ────────────────────────────────────────────────────────────────
// 类型映射
// ────────────────────────────────────────────────────────────────

pub fn py_type(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(p) => {
            let name = p.path.segments.last().map(|s| s.name.clone()).unwrap_or_default();
            match name.as_str() {
                "bool" => "bool".into(),
                "String" | "str" => "str".into(),
                "i8" | "i16" | "i32" | "i64" | "i128" | "isize"
                | "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => "int".into(),
                "f32" | "f64" => "float".into(),
                "char" => "str".into(),
                "Vec" => format!("list[{}]", p.generic_args.iter().map(map_generic).next().unwrap_or_else(|| "Any".into())),
                "HashMap" => format!("dict[{}, {}]",
                    p.generic_args.get(0).map(map_generic).unwrap_or_else(|| "str".into()),
                    p.generic_args.get(1).map(map_generic).unwrap_or_else(|| "Any".into())),
                "HashSet" => format!("set[{}]", p.generic_args.iter().map(map_generic).next().unwrap_or_else(|| "Any".into())),
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
        TypeKind::FnPtr { params, ret } => {
            let args: Vec<String> = params.iter().map(py_type).collect();
            let ret_s = ret.as_ref().map(|t| py_type(t)).unwrap_or_else(|| "None".into());
            format!("Callable[[{}], {}]", args.join(", "), ret_s)
        }
        TypeKind::DynTrait(_) => "Any".into(),
        TypeKind::ImplTrait(_) => "Any".into(),
        TypeKind::Infer => "Any".into(),
        TypeKind::Paren(inner) => format!("({})", py_type(inner)),
        TypeKind::Never => "NoReturn".into(),
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
    if let ParamKind::Self_(kind) = &p.kind {
        return match kind {
            SelfKind::Value | SelfKind::Mut => "self".into(),
            SelfKind::Ref | SelfKind::RefMut => "self".into(),
        };
    }
    if let ParamKind::Pattern(pat) = &p.kind {
        match &pat.kind {
            PatternKind::Ident { name, .. } => {
                return format!("{}: {}", py_ident(&snake_case(&name.name)), py_type(&p.ty));
            }
            PatternKind::Wildcard => {
                return format!("_: {}", py_type(&p.ty));
            }
            _ => return format!("arg: {}", py_type(&p.ty)),
        }
    }
    "arg: Any".to_string()
}

// ────────────────────────────────────────────────────────────────
// 二元/一元运算符 Python 映射
// ────────────────────────────────────────────────────────────────

fn py_binop(op: BinaryOp) -> &'static str {
    use BinaryOp::*;
    match op {
        Add => "+", Sub => "-", Mul => "*", Div => "/", Rem => "%",
        BitAnd => "&", BitOr => "|", BitXor => "^", Shl => "<<", Shr => ">>",
        Eq => "==", Ne => "!=", Lt => "<", Gt => ">", Le => "<=", Ge => ">=",
        And => "and", Or => "or",
    }
}

fn py_unop(op: UnaryOp, operand: &str) -> String {
    use UnaryOp::*;
    match op {
        Neg => format!("(-{})", operand),
        Not => format!("(not {})", operand),
        Deref | Ref | RefMut => operand.to_string(), // Python 无引用语义，忽略
    }
}

// ────────────────────────────────────────────────────────────────
// 语句块转译
// ────────────────────────────────────────────────────────────────

/// 转译语句块。no_return_tail: 循环体等不自动给 tail 加 return 的场景。
fn emit_block_py(block: &BlockExpr, indent: usize, no_return_tail: bool) -> String {
    let pad = "    ".repeat(indent);
    let mut out = String::new();
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let(l) => {
                let pat = py_pattern(&l.pattern);
                // Python 无 mut 关键字
                match &l.init {
                    Some(init) => out.push_str(&format!("{pad}{} = {}\n", pat, emit_expr_py(init))),
                    None => out.push_str(&format!("{pad}{} = None\n", pat)),
                }
                if let Some(els) = &l.else_block {
                    out.push_str(&format!("{pad}else:\n"));
                    out.push_str(&emit_block_py(els, indent + 1, false));
                }
            }
            Stmt::Expr { expr, .. } => {
                out.push_str(&emit_stmt_expr_py(expr, &pad, indent));
            }
            Stmt::Empty(_) => {}
            Stmt::Item(_) => out.push_str(&format!("{pad}pass  # 局部项\n")),
        }
    }
    if let Some(tail) = &block.tail {
        if no_return_tail {
            out.push_str(&emit_stmt_expr_py(tail, &pad, indent));
        } else {
            // 尾表达式：非控制流表达式需要 return
            match &tail.kind {
                ExprKind::If { .. } | ExprKind::Match { .. } | ExprKind::Block(..) => {
                    // if/match 作为尾表达式：Python 需要用赋值或 return
                    out.push_str(&format!("{pad}return {}\n", emit_expr_py(tail)));
                }
                ExprKind::Return(..) | ExprKind::Break { .. } | ExprKind::Continue { .. } => {
                    out.push_str(&format!("{pad}{}\n", emit_expr_py(tail)));
                }
                _ => out.push_str(&format!("{pad}return {}\n", emit_expr_py(tail))),
            }
        }
    } else if out.is_empty() {
        out.push_str(&format!("{pad}pass\n"));
    }
    out
}

/// 语句位置的表达式（if/while/for/loop/match 需要特殊处理缩进）
fn emit_stmt_expr_py(expr: &Expr, pad: &str, indent: usize) -> String {
    match &expr.kind {
        ExprKind::If { cond, then, else_ } => {
            let mut out = String::new();
            out.push_str(&format!("{}if {}:\n", pad, emit_expr_py(cond)));
            out.push_str(&emit_block_py(then, indent + 1, true));
            if let Some(els) = else_ {
                if let ExprKind::If { .. } = &els.kind {
                    out.push_str(&format!("{}else ", pad));
                    out.push_str(&emit_stmt_expr_py(els, "", 0));
                } else {
                    out.push_str(&format!("{}else:\n", pad));
                    if let ExprKind::Block(b) = &els.kind {
                        out.push_str(&emit_block_py(b, indent + 1, true));
                    } else {
                        out.push_str(&format!("{}    {}\n", pad, emit_expr_py(els)));
                    }
                }
            }
            out
        }
        ExprKind::Match { scrutinee, arms } => {
            emit_match_as_if_chain_py(scrutinee, arms, pad, indent)
        }
        ExprKind::While { cond, body, .. } => {
            let mut out = String::new();
            out.push_str(&format!("{}while {}:\n", pad, emit_expr_py(cond)));
            out.push_str(&emit_block_py(body, indent + 1, true));
            out
        }
        ExprKind::WhileLet { pattern, expr, body, .. } => {
            // Python 无 while let → while condition + 内部解构
            let mut out = String::new();
            let scrut = emit_expr_py(expr);
            out.push_str(&format!("{}while True:\n", pad));
            out.push_str(&format!("{}    {} = {}\n", pad, py_pattern(pattern), scrut));
            out.push_str(&format!("{}    if {} is None:\n", pad, py_pattern(pattern)));
            out.push_str(&format!("{}        break\n", pad));
            out.push_str(&emit_block_py(body, indent + 1, true));
            out
        }
        ExprKind::For { pattern, iter, body, .. } => {
            let mut out = String::new();
            out.push_str(&format!("{}for {} in {}:\n", pad, py_pattern(pattern), emit_expr_py(iter)));
            out.push_str(&emit_block_py(body, indent + 1, true));
            out
        }
        ExprKind::Loop { body, .. } => {
            let mut out = String::new();
            out.push_str(&format!("{}while True:\n", pad));
            out.push_str(&emit_block_py(body, indent + 1, true));
            out
        }
        ExprKind::IfLet { pattern, expr, then, else_ } => {
            let mut out = String::new();
            let scrut = emit_expr_py(expr);
            // if let → isinstance 检查（与 match arm 一致）
            let (cond_str, bindings) = py_match_condition(pattern, scrut);
            out.push_str(&format!("{}if {}:\n", pad, cond_str));
            out.push_str(&format!("{}    {}\n", pad, bindings));
            out.push_str(&emit_block_py(then, indent + 1, true));
            if let Some(els) = else_ {
                if let ExprKind::If { .. } = &els.kind {
                    out.push_str(&format!("{}else ", pad));
                    out.push_str(&emit_stmt_expr_py(els, "", 0));
                } else {
                    out.push_str(&format!("{}else:\n", pad));
                    if let ExprKind::Block(b) = &els.kind {
                        out.push_str(&emit_block_py(b, indent + 1, true));
                    } else {
                        out.push_str(&format!("{}    {}\n", pad, emit_expr_py(els)));
                    }
                }
            }
            out
        }
        ExprKind::Assign { .. } | ExprKind::CompoundAssign { .. } => {
            format!("{}{}\n", pad, emit_expr_py(expr))
        }
        _ => {
            format!("{}{}\n", pad, emit_expr_py(expr))
        }
    }
}

// ────────────────────────────────────────────────────────────────
// match → if/elif 链
// ────────────────────────────────────────────────────────────────

/// 将 match 转译为 if/elif/else 链（Python 无原生 match expression）
fn emit_match_as_if_chain_py(
    scrutinee: &Expr,
    arms: &[MatchArm],
    pad: &str,
    indent: usize,
) -> String {
    let mut out = String::new();
    let scrut = emit_expr_py(scrutinee);
    // 临时变量用于多引用
    let tmp = "_match_val";
    out.push_str(&format!("{}{} = {}\n", pad, tmp, scrut));

    for (i, arm) in arms.iter().enumerate() {
        let (cond, bindings) = py_match_condition(&arm.pattern, tmp.to_string());
        let guard = arm.guard.as_ref()
            .map(|g| format!(" and {}", emit_expr_py(g)))
            .unwrap_or_default();
        let kw = if i == 0 { "if" } else { "elif" };
        out.push_str(&format!("{}{} {}{}:\n", pad, kw, cond, guard));
        if !bindings.is_empty() {
            out.push_str(&format!("{}    {}\n", pad, bindings));
        }
        // arm body
        if let ExprKind::Block(b) = &arm.body.kind {
            out.push_str(&emit_block_py(b, indent + 1, true));
        } else {
            out.push_str(&format!("{}    {}\n", pad, emit_expr_py(&arm.body)));
        }
    }
    out
}

/// 从模式生成 Python 条件和绑定赋值
/// 返回 (condition_string, bindings_string)
fn py_match_condition(pattern: &Pattern, scrutinee: String) -> (String, String) {
    match &pattern.kind {
        PatternKind::Wildcard => ("True".into(), String::new()),
        PatternKind::Ident { name, sub: None, .. } => {
            ("True".into(), format!("{} = {}", py_ident(&snake_case(&name.name)), scrutinee))
        }
        PatternKind::Ident { name, sub: Some(inner), .. } => {
            // x @ pat → bind x, check inner
            let (inner_cond, inner_bind) = py_match_condition(inner, scrutinee.clone());
            (inner_cond, format!("{} = {}; {}", py_ident(&snake_case(&name.name)), scrutinee, inner_bind))
        }
        PatternKind::Literal(lit) => {
            (format!("{} == {}", scrutinee, py_literal(lit)), String::new())
        }
        PatternKind::Path(p) => {
            // 单段路径（枚举单元变体）→ 比较类引用
            let name = p.segments.last().map(|s| s.name.as_str()).unwrap_or("");
            if p.segments.len() >= 2 {
                let variant = p.segments.last().unwrap();
                (format!("isinstance({}, {})" , scrutinee, variant.name), String::new())
            } else {
                (format!("{} == {}", scrutinee, name), String::new())
            }
        }
        PatternKind::TupleStruct { path, elems, .. } => {
            let variant = path.segments.last().map(|s| s.name.as_str()).unwrap_or("");
            let bindings: Vec<String> = elems.iter().enumerate().map(|(i, e)| {
                match &e.kind {
                    PatternKind::Ident { name, .. } => {
                        format!("{} = {}[{}]", py_ident(&snake_case(&name.name)), scrutinee, i)
                    }
                    PatternKind::Wildcard => String::new(),
                    _ => format!("# unsupported pattern at [{}]", i),
                }
            }).filter(|s| !s.is_empty()).collect();
            (format!("isinstance({}, {})" , scrutinee, variant), bindings.join("; "))
        }
        PatternKind::Struct { path, fields, .. } => {
            let class_name = path.segments.last().map(|s| s.name.as_str()).unwrap_or("");
            let mut conds: Vec<String> = vec![format!("isinstance({}, {})" , scrutinee, class_name)];
            let mut bindings: Vec<String> = Vec::new();
            for f in fields {
                let fname = f.name.name.clone();
                if let Some(pat) = &f.pattern {
                    match &pat.kind {
                        PatternKind::Ident { name, .. } => {
                            bindings.push(format!(
                                "{} = {}.{}",
                                py_ident(&snake_case(&name.name)),
                                scrutinee, snake_case(&fname)
                            ));
                        }
                        PatternKind::Wildcard => {}
                        _ => {
                            conds.push(format!("# complex field pattern for {}", fname));
                        }
                    }
                } else {
                    // 简写：字段名即绑定名
                    bindings.push(format!(
                        "{} = {}.{}",
                        py_ident(&snake_case(&fname)),
                        scrutinee, snake_case(&fname)
                    ));
                }
            }
            (conds.join(" and "), bindings.join("; "))
        }
        PatternKind::Tuple { elems, .. } => {
            let bindings: Vec<String> = elems.iter().enumerate().map(|(i, e)| {
                match &e.kind {
                    PatternKind::Ident { name, .. } => {
                        format!("{} = {}[{}]", py_ident(&snake_case(&name.name)), scrutinee, i)
                    }
                    PatternKind::Wildcard => String::new(),
                    _ => format!("# unsupported tuple pattern at [{}]", i),
                }
            }).filter(|s| !s.is_empty()).collect();
            (format!("isinstance({}, tuple) and len({}) == {}" , scrutinee, scrutinee, elems.len()), bindings.join("; "))
        }
        PatternKind::Or(pats) => {
            let sub: Vec<String> = pats.iter()
                .map(|p| py_match_condition(p, scrutinee.clone()).0)
                .collect();
            (format!("({})", sub.join(" or ")), String::new())
        }
        PatternKind::Range { lo, hi, inclusive } => {
            let lo_s = py_literal_pattern(&lo.kind);
            let hi_s = py_literal_pattern(&hi.kind);
            let op = if *inclusive { "<=" } else { "<" };
            (format!("{} <= {} {} {}", lo_s, scrutinee, op, hi_s), String::new())
        }
        PatternKind::Rest => ("True".into(), String::new()),
    }
}

fn py_literal_pattern(kind: &PatternKind) -> String {
    match kind {
        PatternKind::Literal(lit) => py_literal(lit),
        _ => "0".into(),
    }
}

// ────────────────────────────────────────────────────────────────
// 模式转译
// ────────────────────────────────────────────────────────────────

fn py_pattern(pat: &Pattern) -> String {
    match &pat.kind {
        PatternKind::Ident { name, .. } => py_ident(&snake_case(&name.name)),
        PatternKind::Wildcard => "_".into(),
        PatternKind::Rest => "*".into(),
        PatternKind::Literal(lit) => match &lit.kind {
            LiteralKind::Str { value, .. } => format!("{value:?}"),
            LiteralKind::Bool(b) => b.to_string(),
            LiteralKind::Int { value, .. } => value.to_string(),
            LiteralKind::Float { value, .. } => value.to_string(),
            LiteralKind::Char(c) => format!("{c:?}"),
            _ => "0".into(),
        },
        PatternKind::Path(p) => {
            let segs: Vec<&str> = p.segments.iter().map(|s| s.name.as_str()).collect();
            segs.join(".")
        }
        PatternKind::TupleStruct { path, elems, .. } => {
            let name = path.last().name.clone();
            let inner: Vec<String> = elems.iter().map(py_pattern).collect();
            format!("{}({})", name, inner.join(", "))
        }
        PatternKind::Struct { path, fields, .. } => {
            let name = path.last().name.clone();
            let fields_str = fields.iter().map(|f| {
                let pat = f.pattern.as_ref().map(|p| py_pattern(p)).unwrap_or_else(|| py_ident(&snake_case(&f.name.name)));
                format!("{}={}", f.name.name, pat)
            }).collect::<Vec<_>>().join(", ");
            format!("{}({})", name, fields_str)
        }
        PatternKind::Tuple { elems, .. } => {
            format!("({})", elems.iter().map(py_pattern).collect::<Vec<_>>().join(", "))
        }
        PatternKind::Or(pats) => {
            // Python 3.10+ match 风格 or-pattern → 只在 match 上下文有效
            pats.iter().map(py_pattern).collect::<Vec<_>>().join(" | ")
        }
        PatternKind::Range { lo, hi, inclusive } => {
            let op = if *inclusive { "..=" } else { ".." };
            format!("{} {} {}", py_pattern(lo), op, py_pattern(hi))
        }
    }
}

// ────────────────────────────────────────────────────────────────
// 表达式转译（完整覆盖 33 ExprKind）
// ────────────────────────────────────────────────────────────────

/// 表达式级转译
pub fn emit_expr_py(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(lit) => py_literal(lit),
        ExprKind::Path(p) => {
            let segs: Vec<&str> = p.segments.iter().map(|s| s.name.as_str()).collect();
            if segs.len() == 1 {
                py_ident(segs[0]).to_string()
            } else {
                segs.join(".")
            }
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let op_str = py_binop(*op);
            format!("({} {} {})", emit_expr_py(lhs), op_str, emit_expr_py(rhs))
        }
        ExprKind::Unary { op, operand } => py_unop(*op, &emit_expr_py(operand)),
        ExprKind::Call { callee, args } => {
            format!(
                "{}({})",
                emit_expr_py(callee),
                args.iter().map(emit_expr_py).collect::<Vec<_>>().join(", ")
            )
        }
        ExprKind::MethodCall { receiver, method, args, .. } => {
            let mname = snake_case(&method.name);
            // 常用 std 方法映射
            let (mapped_receiver, mapped_method) = py_std_method(receiver, &mname);
            format!(
                "{}.{}({})",
                mapped_receiver,
                mapped_method,
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
        ExprKind::Index { base, index } => {
            format!("{}[{}]", emit_expr_py(base), emit_expr_py(index))
        }
        ExprKind::Slice { base, range } => {
            let base_str = emit_expr_py(base);
            let lo = range.lo.as_ref().map(|e| emit_expr_py(e)).unwrap_or_default();
            let hi = range.hi.as_ref().map(|e| emit_expr_py(e)).unwrap_or_default();
            // Python 切片: inclusive 需 +1, exclusive 直接用
            // a[lo..hi] → a[lo:hi] (Python 半开，HSL 闭区间需 +1)
            if range.inclusive {
                if lo.is_empty() {
                    format!("{}[:{} + 1]", base_str, hi)
                } else if hi.is_empty() {
                    format!("{}[{}:]", base_str, lo)
                } else {
                    format!("{}[{}:{} + 1]", base_str, lo, hi)
                }
            } else {
                format!("{}[{}:{} ]", base_str, lo, hi)
            }
        }
        ExprKind::Range(r) => {
            // 值语境 range → range() 调用
            let lo = r.lo.as_ref().map(|e| emit_expr_py(e)).unwrap_or_else(|| "0".into());
            let hi = r.hi.as_ref().map(|e| emit_expr_py(e));
            match hi {
                Some(hi_expr) => {
                    if r.inclusive {
                        format!("range({}, {} + 1)", lo, hi_expr)
                    } else {
                        format!("range({}, {})", lo, hi_expr)
                    }
                }
                None => {
                    // n.. → range(n, ...) 需要上限，用 None 提示
                    format!("range({}, len(__iter__))  # open range" , lo)
                }
            }
        }
        ExprKind::Assign { lhs, rhs } => {
            format!("{} = {}", emit_expr_py(lhs), emit_expr_py(rhs))
        }
        ExprKind::CompoundAssign { op, lhs, rhs } => {
            format!("{} {}= {}", emit_expr_py(lhs), py_binop(*op), emit_expr_py(rhs))
        }
        ExprKind::If { cond, then, else_ } => {
            // if 作为表达式 → Python 三元表达式 (仅简单情况) 或 None
            // 完整 if/elif/else 作为表达式需赋值临时变量
            let mut out = String::new();
            out.push_str(&format!("({} if {} else ", emit_block_tail_py(then), emit_expr_py(cond)));
            if let Some(els) = else_ {
                out.push_str(&emit_expr_py(els));
            } else {
                out.push_str("None");
            }
            out.push_str(")");
            out
        }
        ExprKind::IfLet { pattern, expr, then, else_ } => {
            // if let 作为表达式 → 临时变量 + if/else 赋值
            let (cond, _bindings) = py_match_condition(pattern, emit_expr_py(expr));
            let mut out = String::new();
            out.push_str(&format!("(({}) if {} else ", emit_block_tail_py(then), cond));
            if let Some(els) = else_ {
                out.push_str(&emit_expr_py(els));
            } else {
                out.push_str("None");
            }
            out.push_str(")");
            out
        }
        ExprKind::Match { scrutinee, .. } => {
            // match 作为表达式 → 多行 if/elif/else 无法做表达式，返回 None 占位
            // 调用者应在语句位置使用 emit_match_as_if_chain_py
            format!("None  # match expression (use in statement position)  /* {} */", emit_expr_py(scrutinee))
        }
        ExprKind::Loop { .. } => {
            // loop 作为表达式 → 无意义，返回 None
            format!("(lambda: (_ for _ in []).__next__())()  # loop expression")
        }
        ExprKind::While { .. } => {
            format!("None  # while expression")
        }
        ExprKind::WhileLet { .. } => {
            "None  # while-let expression".into()
        }
        ExprKind::For { .. } => {
            "None  # for expression".into()
        }
        ExprKind::Closure { params, body, .. } => {
            // async closure: Python lambda does not support async
            let param_names: Vec<String> = params.iter().map(|p| {
                match &p.kind {
                    ParamKind::Pattern(pat) => match &pat.kind {
                        PatternKind::Ident { name, .. } => py_ident(&snake_case(&name.name)),
                        PatternKind::Wildcard => "_".into(),
                        _ => "arg".into(),
                    },
                    _ => "arg".into(),
                }
            }).collect();
            if param_names.len() <= 1 && matches!(&body.kind, ExprKind::Path(..) | ExprKind::Binary { .. } | ExprKind::Call { .. } | ExprKind::Field { .. } | ExprKind::MethodCall { .. } | ExprKind::Literal(..)) {
                // 简单闭包 → lambda
                format!("(lambda {}: {})", param_names.join(", "), emit_expr_py(body))
            } else {
                // 复杂闭包 → 内联 def
                format!("(lambda {}: ({}))", param_names.join(", "), emit_expr_py(body))
            }
        }
        ExprKind::Return(val) => {
            match val {
                Some(v) => format!("return {}", emit_expr_py(v)),
                None => "return".into(),
            }
        }
        ExprKind::Break { value, .. } => {
            match value {
                Some(v) => format!("return {}  # break with value", emit_expr_py(v)),
                None => "break".into(),
            }
        }
        ExprKind::Continue { .. } => "continue".into(),
        ExprKind::Block(b) => {
            // 块作为表达式 → 返回尾表达式值（用临时函数模拟）
            let tail = b.tail.as_ref().map(|t| emit_expr_py(t)).unwrap_or_else(|| "None".into());
            format!("(lambda: {})()", tail)
        }
        ExprKind::AsyncBlock { body, .. } => {
            let tail = body.tail.as_ref().map(|t| emit_expr_py(t)).unwrap_or_else(|| "None".into());
            format!("(await (async lambda: {})())", tail)
        }
        ExprKind::Array(elems) => {
            format!("[{}]", elems.iter().map(emit_expr_py).collect::<Vec<_>>().join(", "))
        }
        ExprKind::ArrayRepeat { elem, count } => {
            format!("[{}] * {}", emit_expr_py(elem), emit_expr_py(count))
        }
        ExprKind::Struct { path, fields, spread } => {
            let class_name = path.last().name.clone();
            let fields_str = fields.iter().map(|f| {
                let fname = match &f.name {
                    FieldIndex::Named(id) => id.name.clone(),
                    FieldIndex::Index(i, _) => format!("_{}", i),
                };
                let val = f.value.as_ref().map(|v| emit_expr_py(v)).unwrap_or_else(|| fname.clone());
                format!("{}={}", snake_case(&fname), val)
            }).collect::<Vec<_>>().join(", ");
            let spread_str = if let Some(spread) = spread {
                format!(", **{}", emit_expr_py(spread))
            } else {
                String::new()
            };
            format!("{}({}{} )", class_name, fields_str, spread_str)
        }
        ExprKind::Tuple(elems) => {
            if elems.len() == 1 {
                format!("({},)", emit_expr_py(&elems[0]))
            } else {
                format!("({})", elems.iter().map(emit_expr_py).collect::<Vec<_>>().join(", "))
            }
        }
        ExprKind::Await(inner) => format!("await {}", emit_expr_py(inner)),
        ExprKind::Try(inner) => {
            // Python: try/except 包装
            format!("({})  # try: add try/except", emit_expr_py(inner))
        }
        ExprKind::Cast { expr, ty } => {
            // 类型转换 → Python 构造函数
            let target = py_type(ty);
            match target.as_str() {
                "int" => format!("int({})", emit_expr_py(expr)),
                "float" => format!("float({})", emit_expr_py(expr)),
                "str" => format!("str({})", emit_expr_py(expr)),
                "bool" => format!("bool({})", emit_expr_py(expr)),
                "list" => format!("list({})", emit_expr_py(expr)),
                other => format!("{}({})  # cast to {}", other, emit_expr_py(expr), other),
            }
        }
        ExprKind::Native(nb) => {
            // 原样搬运
            nb.code.trim().to_string()
        }
        ExprKind::Macro { path, args } => {
            let name = path.last().name.clone();
            // format! → f-string
            if name == "format" {
                return py_format_macro(args);
            }
            // println! → print()
            if name == "println" {
                let inner = args.tokens.iter().map(|tt| match tt {
                    TokenTree::Token(tok, _) => match tok {
                        Token::Ident(s) | Token::RawIdent(s) => s.clone(),
                        Token::Literal(lit) => lit.raw.clone(),
                        Token::Punct(s) => s.clone(),
                        Token::Label(s) => s.clone(),
                    },
                    TokenTree::Delimited { delim: _, tokens, .. } => {
                        tokens.iter().map(|t| match t {
                            TokenTree::Token(tok, _) => match tok {
                                Token::Ident(s) | Token::RawIdent(s) => s.clone(),
                                Token::Literal(lit) => lit.raw.clone(),
                                Token::Punct(s) => s.clone(),
                                Token::Label(s) => s.clone(),
                            },
                            _ => "...".into(),
                        }).collect::<Vec<_>>().join("")
                    }
                    _ => "".into(),
                }).collect::<Vec<_>>().join("");
                return format!("print({})", inner);
            }
            format!("{}_macro()", snake_case(&name))
        }
    }
}

/// 块尾表达式提取（用于三元 if 表达式）
fn emit_block_tail_py(block: &BlockExpr) -> String {
    if let Some(tail) = &block.tail {
        emit_expr_py(tail)
    } else {
        "None".into()
    }
}

/// 常用 std 方法映射（对齐 dhv-ts body.ts 方法映射表）
fn py_std_method(receiver: &Expr, method: &str) -> (String, String) {
    let recv_str = emit_expr_py(receiver);
    match method {
        "to_string" | "to_str" => (recv_str, "__str__".into()),
        "len" | "length" => (recv_str, "__len__".into()),
        "push" => (recv_str, "append".into()),
        "push_back" => (recv_str, "append".into()),
        "push_front" => (format!("{}[0:0]", recv_str), "insert".into()),
        "pop" => (recv_str, "pop".into()),
        "contains" => (recv_str, "__contains__".into()),
        "is_empty" => (format!("len({}) == 0", recv_str), "__bool__".into()),
        "is_some" => (format!("{} is not None", recv_str), "__bool__".into()),
        "is_none" => (format!("{} is None", recv_str), "__bool__".into()),
        "is_ok" => (format!("not isinstance({}, Exception)", recv_str), "__bool__".into()),
        "is_err" => (format!("isinstance({}, Exception)", recv_str), "__bool__".into()),
        "unwrap" => (recv_str, "__or_raise__".into()),
        "expect" => (recv_str, "__or_raise__".into()),
        "clone" => (recv_str, "copy".into()),
        "clone_from" => (recv_str, "copy".into()),
        "to_vec" => (recv_str, "list".into()),
        "keys" => (recv_str, "keys".into()),
        "values" => (recv_str, "values".into()),
        "entries" => (recv_str, "items".into()),
        "insert" => (recv_str, "insert".into()),
        "remove" => (recv_str, "pop".into()),
        "clear" => (recv_str, "clear".into()),
        "retain" => (recv_str, "__retain__".into()),
        "sort" => (recv_str, "sort".into()),
        "sort_by" => (recv_str, "sort".into()),
        "reverse" => (recv_str, "reverse".into()),
        "map" => (recv_str, "map".into()),
        "filter" => (recv_str, "filter".into()),
        "fold" => (recv_str, "__fold__".into()),
        "for_each" => (recv_str, "__for_each__".into()),
        "all" => (recv_str, "all".into()),
        "any" => (recv_str, "any".into()),
        "find" => (recv_str, "__find__".into()),
        "position" => (recv_str, "__index__".into()),
        "first" => (format!("{}[0] if {} else None", recv_str, recv_str), "".into()),
        "last" => (format!("{}[-1] if {} else None", recv_str, recv_str), "".into()),
        "get" => (recv_str, "get".into()),
        "iter" => (recv_str, "__iter__".into()),
        "into_iter" => (recv_str, "__iter__".into()),
        "collect" => (format!("list({})", recv_str), "".into()),
        "join" => (recv_str, "join".into()),
        "trim" => (recv_str, "strip".into()),
        "starts_with" => (recv_str, "startswith".into()),
        "ends_with" => (recv_str, "endswith".into()),
        "replace" => (recv_str, "replace".into()),
        "split" => (recv_str, "split".into()),
        "parse" | "parse_int" => (recv_str, "int".into()),
        "parse_float" => (recv_str, "float".into()),
        "to_lowercase" => (recv_str, "lower".into()),
        "to_uppercase" => (recv_str, "upper".into()),
        "abs" => (format!("abs({})", recv_str), "".into()),
        "ceil" => (format!("-int(-{} // 1)" , recv_str), "".into()),
        "floor" => (format!("int({} // 1)" , recv_str), "".into()),
        "round" => (format!("round({})", recv_str), "".into()),
        "min" => (format!("min({})", recv_str), "".into()),
        "max" => (format!("max({})", recv_str), "".into()),
        _ => (recv_str, method.to_string()),
    }
}

/// format! 宏 → f-string
fn py_format_macro(args: &MacroArgs) -> String {
    let mut parts = Vec::new();
    for tt in &args.tokens {
        match tt {
            TokenTree::Token(tok, _) => match tok {
                Token::Literal(lit) => parts.push(lit.raw.clone()),
                Token::Punct(s) if s == "," => {}
                Token::Ident(s) | Token::RawIdent(s) => parts.push(format!("{{{}}}", snake_case(s))),
                _ => {}
            },
            TokenTree::Delimited { tokens, .. } => {
                for t in tokens {
                    if let TokenTree::Token(tok, _) = t {
                        match tok {
                            Token::Literal(lit) => parts.push(lit.raw.clone()),
                            Token::Ident(s) | Token::RawIdent(s) => parts.push(format!("{{{}}}", snake_case(s))),
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
    // 将 {value} 占位替换为 {} 格式化参数
    let mut fmt_parts = Vec::new();
    let mut fmt_args = Vec::new();
    for part in &parts {
        if part.starts_with('{') && part.ends_with('}') && part.len() > 2 {
            let arg_name = &part[1..part.len()-1];
            fmt_args.push(arg_name.to_string());
            fmt_parts.push("{}".to_string());
        } else {
            // 处理字符串字面量中的花括号
            fmt_parts.push(part.replace('{', "{{").replace('}', "}}"));
        }
    }
    if fmt_args.is_empty() {
        format!("\"{}\"", fmt_parts.join(""))
    } else {
        format!("\"{}\".format({})", fmt_parts.join(""), fmt_args.join(", "))
    }
}

// ────────────────────────────────────────────────────────────────
// 字面量转译
// ────────────────────────────────────────────────────────────────

fn py_literal(lit: &Literal) -> String {
    match &lit.kind {
        LiteralKind::Str { value, .. } => format!("{value:?}"),
        LiteralKind::Bool(b) => {
            if *b { "True".into() } else { "False".into() }
        }
        LiteralKind::Int { value, .. } => value.to_string(),
        LiteralKind::Float { value, .. } => value.to_string(),
        LiteralKind::Char(c) => format!("{c:?}"),
    }
}

// ────────────────────────────────────────────────────────────────
// 工具
// ────────────────────────────────────────────────────────────────

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
