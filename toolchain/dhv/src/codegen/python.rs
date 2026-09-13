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
                // v0.2.65：导入区后两空行（isort 规范 I001 —— 此前一空行实测被 ruff 拒）
                out.push_str("from dataclasses import dataclass\n\n\n@dataclass\n");
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
                    // v0.2.65：Named 变体投射 @dataclass —— 需要导入（此前混合枚举
                    // 只在纯 struct 路径导入，混合枚举文件用 @dataclass 而无导入
                    // → F821 实测：action.py/verdict.py/work_status.py/shape.py）
                    if e.variants.iter().any(|v| matches!(&v.fields, StructKind::Named(_))) {
                        out.push_str("from dataclasses import dataclass\n\n\n");
                    }
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
                    out.push_str("from enum import Enum, auto\n\n\n");
                    out.push_str(&format!("class {}(Enum):\n", e.name.name));
                    for v in &e.variants {
                        out.push_str(&format!("    {} = auto()\n", v.name.name));
                    }
                }
            }
            Item::Trait(t) => {
                // trait → Protocol (structural typing)
                out.push_str("from typing import Protocol\n\n\n");
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
                // v0.2.65：顶层 def（此前 4 空格缩进发射 = IndentationError E999 ——
                // impl 是独立投射项，不在 class 体内；self 作显式首参）
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
                }
            }
            Item::Const(c) => {
                // v0.2.65：常量名保持原样（全大写惯例；此前 snake_case 把
                // DEFAULT_PRIORITY 打碎成 d_e_f_a_u_l_t… —— 引用侧不匹配 → F821）
                out.push_str(&format!(
                    "{}: {} = {}\n",
                    py_ident(&c.name.name),
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
                // v0.2.65：Some(x) 模式直译为绑定（python 的 Option 语义 = 裸值/None；
                // 此前把模式文本当赋值目标 → `Some(x) = e` 非法赋值 E999）
                if let Some(init) = &l.init {
                    if let Some(name) = py_some_bind(&l.pattern) {
                        out.push_str(&format!("{pad}{} = {}\n", name, py_strip_outer(&emit_expr_py(init))));
                        if let Some(els) = &l.else_block {
                            out.push_str(&format!("{pad}else:\n"));
                            out.push_str(&emit_block_py(els, indent + 1, false));
                        }
                        continue;
                    }
                }
                let pat = py_pattern(&l.pattern);
                // Python 无 mut 关键字
                match &l.init {
                    Some(init) => out.push_str(&format!("{pad}{} = {}\n", pat, py_strip_outer(&emit_expr_py(init)))),
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
            Stmt::Item(item) => {
                // v0.2.65：语句级宏（println! 等）真实投射 —— 此前一律
                // `pass  # 局部项` 吞掉语句：变量使用点消失（F841）+ 块内
                // 多余 pass（PIE790）双违规；println! → print(f-string)
                if let Item::MacroCall { path, args } = item {
                    let mname = path.last().name.clone();
                    if mname == "println" || mname == "print" {
                        out.push_str(&format!("{pad}print({})\n", py_format_macro(args)));
                        continue;
                    }
                    // 其它语句级宏：诚实注释降级（不静默伪造语义）
                    out.push_str(&format!("{pad}# macro {}!(…) — 未投射\n", mname));
                    continue;
                }
                // 局部项（fn 内 struct/fn 声明）：注释占位
                out.push_str(&format!("{pad}# 局部项（未投射）\n"));
            }
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
                _ => out.push_str(&format!("{pad}return {}\n", py_strip_outer(&emit_expr_py(tail)))),
            }
        }
    } else {
        // v0.2.65：块为空（或仅注释）时补 pass —— 注释不构成语句体，
        // 纯注释体在 python 是 IndentationError（诚实空体探测）
        let has_code = out
            .lines()
            .any(|l| { let t = l.trim(); !t.is_empty() && !t.starts_with('#') });
        if !has_code {
            out.push_str(&format!("{pad}pass\n"));
        }
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
            // v0.2.65：python 无 while let → 合成 while True + 循环体内单次求值 + break。
            // 此前两缺陷同源（把模式文本当赋值目标）：
            //   ① `Some(head) = q.pop()` 非法赋值目标（E999 实测 ×4 文件）；
            //   ② 条件与绑定各自重求值 scrutinee —— 副作用双 popping。
            // 模式语义统一走 py_match_condition（与 match/if-let 同源）。
            let mut out = String::new();
            let scrut_expr = emit_expr_py(expr);
            out.push_str(&format!("{}while True:\n", pad));
            let inner = format!("{pad}    ");
            let scrut = if py_is_simple_name(&scrut_expr) {
                scrut_expr
            } else {
                out.push_str(&format!("{}_wl_scrut = {}\n", inner, scrut_expr));
                "_wl_scrut".to_string()
            };
            let (cond, bindings) = py_match_condition(pattern, scrut);
            out.push_str(&format!("{}if {}:\n", inner, py_negate(&cond)));
            out.push_str(&format!("{}    break\n", inner));
            for b in bindings.lines().filter(|l| !l.trim().is_empty()) {
                out.push_str(&format!("{}{}\n", inner, b));
            }
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
            for b in bindings.lines().filter(|l| !l.trim().is_empty()) {
                out.push_str(&format!("{}    {}\n", pad, b));
            }
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
            for b in bindings.lines().filter(|l| !l.trim().is_empty()) {
                out.push_str(&format!("{}    {}\n", pad, b));
            }
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
            (inner_cond, format!("{} = {};\n{}", py_ident(&snake_case(&name.name)), scrutinee, inner_bind))
        }
        // v0.2.65：Option::None / None 路径模式 → is None（此前 len>=2 落到
        // isinstance(scrut, None) —— 无意义且运行期必假）
        PatternKind::Path(p) if py_is_none_path(p) => {
            (format!("{} is None", scrutinee), String::new())
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
        PatternKind::TupleStruct { path, elems, .. } if py_is_some_path(path) && elems.len() == 1 => {
            // v0.2.65：Some(x) → python 语义 = 非 None 即有值（值即绑定）。
            // 此前落入通用 TupleStruct 分支：isinstance(scrut, Some) 必假
            // + 绑定 `x = scrut[0]` 对裸值越界。
            match &elems[0].kind {
                PatternKind::Ident { name, .. } => (
                    format!("{} is not None", scrutinee),
                    format!("{} = {}", py_ident(&snake_case(&name.name)), scrutinee),
                ),
                _ => (format!("{} is not None", scrutinee), String::new()),
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
            (format!("isinstance({}, {})" , scrutinee, variant), bindings.join("\n"))
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
            (conds.join(" and "), bindings.join("\n"))
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
            (format!("isinstance({}, tuple) and len({}) == {}" , scrutinee, scrutinee, elems.len()), bindings.join("\n"))
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
                // v0.2.65：None 是值不是标识符（此前 py_ident 关键字转义 →
                // `None_` 值损坏；Some 作构造名由 Call 分支消解）
                if segs[0] == "None" { return "None".into(); }
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
            // v0.2.65：Option/原生构造直译（与 dhv-ts 同源约定）：
            //   Some(x) → x · None → None（python 的 Option = 裸值/None）
            //   String::from(x) → str(x)（字面量直出 —— `from` 是 python 关键字，
            //   此前 `String.from(x)` 非法语法 E999）
            if let ExprKind::Path(p) = &callee.kind {
                let segs: Vec<&str> = p.segments.iter().map(|s| s.name.as_str()).collect();
                if segs.len() == 1 && segs[0] == "Some" && args.len() == 1 {
                    return emit_expr_py(&args[0]);
                }
                if segs.len() == 1 && segs[0] == "None" && args.is_empty() {
                    return "None".into();
                }
                if segs.len() == 2 && segs[0] == "Option" {
                    if segs[1] == "Some" && args.len() == 1 { return emit_expr_py(&args[0]); }
                    if segs[1] == "None" && args.is_empty() { return "None".into(); }
                }
                // v0.2.65：内置容器构造直译（tally 实测：HashMap::new() 裸引用
                // → F821；语义即空容器字面量）
                if segs.len() >= 2 && args.is_empty() {
                    let (head, tail) = (segs[segs.len() - 2], segs[segs.len() - 1]);
                    if (head == "HashMap" || head == "BTreeMap") && tail == "new" {
                        return "{}".into();
                    }
                    if (head == "Vec" || head == "VecDeque") && tail == "new" {
                        return "[]".into();
                    }
                    if head == "String" && tail == "new" {
                        return "\"\"".into();
                    }
                    if (head == "HashSet" || head == "BTreeSet") && tail == "new" {
                        return "set()".into();
                    }
                }
                if segs.len() >= 2 && segs[segs.len() - 2] == "String" && segs[segs.len() - 1] == "from" && args.len() == 1 {
                    if let ExprKind::Literal(l) = &args[0].kind {
                        if matches!(l.kind, LiteralKind::Str { .. }) { return py_literal(l); }
                    }
                    return format!("str({})", emit_expr_py(&args[0]));
                }
            }
            // v0.2.65：实参外层括号剥离（二元全括号化发射的副作用 ——
            // `f((a - b))` 触发 UP034；元组由 py_strip_outer 顶层逗号保护）
            let args_str = args.iter().map(|a| py_strip_outer(&emit_expr_py(a))).collect::<Vec<_>>().join(", ");
            format!("{}({})", emit_expr_py(callee), args_str)
        }
        ExprKind::MethodCall { receiver, method, args, .. } => {
            let mname = snake_case(&method.name);
            // 常用 std 方法映射
            let (mapped_receiver, mapped_method) = py_std_method(receiver, &mname);
            if mapped_method.is_empty() {
                // v0.2.65：映射已把整式塞进 receiver（first/last/abs/collect…）——
                // 空方法名此前发射 `recv.()`（E999）；实参诚实丢弃（min/max 形态）
                return mapped_receiver;
            }
            let args_str = args.iter().map(|a| py_strip_outer(&emit_expr_py(a))).collect::<Vec<_>>().join(", ");
            format!(
                "{}.{}({})",
                mapped_receiver,
                mapped_method,
                args_str
            )
        }
        ExprKind::Field { base, field } => {
            match field {
                FieldIndex::Named(id) => format!("{}.{}", emit_expr_py(base), snake_case(&id.name)),
                // v0.2.65：元组下标访问 → 下标（`kv.1` 在 python 是非法属性名 E999 实测）
                FieldIndex::Index(i, _) => format!("{}[{}]", emit_expr_py(base), i),
            }
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
            format!("{} = {}", emit_expr_py(lhs), py_strip_outer(&emit_expr_py(rhs)))
        }
        ExprKind::CompoundAssign { op, lhs, rhs } => {
            format!("{} {}= {}", emit_expr_py(lhs), py_binop(*op), py_strip_outer(&emit_expr_py(rhs)))
        }
        ExprKind::If { cond, then, else_ } => {
            // if 作为表达式 → 三元表达式
            // v0.2.65 FURB136：min/max 惯用法直译 —— `b if a > b else a` →
            // min(a, b)（ruff 对条件表达式夹逼形态会要求 min/max 重写，clamp 实测）
            if let Some(els) = else_ {
                if let ExprKind::Binary { op, lhs, rhs } = &cond.kind {
                    if !matches!(op, BinaryOp::Gt | BinaryOp::Lt) {
                        // 仅夹逼形态（Gt/Lt）走 min/max 直译，其它比较算符走通用三元
                    } else {
                    let a_s = emit_expr_py(lhs);
                    let b_s = emit_expr_py(rhs);
                    let then_s = emit_block_tail_py(then);
                    let else_s = if let ExprKind::Block(b) = &els.kind {
                        emit_block_tail_py(b)
                    } else {
                        emit_expr_py(els)
                    };
                    if then_s.trim() == b_s && else_s.trim() == a_s {
                        let fname = match op { BinaryOp::Gt => "min", _ => "max" };
                        return format!("{}({}, {})", fname, a_s, b_s);
                    }
                    }
                }
            }
            // v0.2.65：else 分支为块时取尾表达式（此前 emit_expr_py(Block)
            // → `(lambda: x)()` IIFE —— PLC3002 + 徒增调用层，fact/clamp 实测）
            let then_s = py_strip_outer(&emit_block_tail_py(then));
            let cond_s = {
                let c = emit_expr_py(cond);
                // 括号剥离仅限无条件表达式内嵌形态（含 if/else 的内层不剥 —— 优先级风险）
                if c.contains(" if ") || c.contains(" else ") { c } else { py_strip_outer(&c) }
            };
            let else_s = match else_ {
                Some(els) => {
                    let raw = if let ExprKind::Block(b) = &els.kind {
                        emit_block_tail_py(b)
                    } else {
                        emit_expr_py(els)
                    };
                    py_strip_outer(&raw)
                }
                None => "None".into(),
            };
            format!("({} if {} else {})", then_s, cond_s, else_s)
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
                Some(v) => format!("return {}", py_strip_outer(&emit_expr_py(v))),
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
            // v0.2.65：纯尾块直接内联（(lambda: x)() ≡ x，此前徒增 IIFE
            // 调用层且触发 PLC3002）；带语句块在表达式位无法安全内联 → 诚实降级
            if b.stmts.is_empty() {
                b.tail.as_ref().map(|t| emit_expr_py(t)).unwrap_or_else(|| "None".into())
            } else {
                "None  # block expression（多语句块无法内联表达式位）".into()
            }
        }
        ExprKind::AsyncBlock { .. } => {
            // v0.2.65：python 无 async lambda（此前 `(await (async lambda: …)())`
            // 非法语法 E999）—— 异步块表达式诚实降级（语句位用 async def）
            "None  # async block expression（Python 无 async lambda）".into()
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
            format!("{}({}{})", class_name, fields_str, spread_str)
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
            // vec! → 列表字面量（v0.2.65：此前 `vec_macro()` 引用未定义函数 F821）
            if name == "vec" {
                return py_vec_macro(args);
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

/// format! 宏 → f-string（v0.2.65 重写）
/// ----------------------------------------------------------------------------
/// 此前实现的拼接损坏（三处叠加）：字面量带引号原样入 parts + 整体再包一层
/// 引号 + `{}` 占位与实参错位 → `""tpl""{}{}"`.format(...) 非法语法 E999
/// （classify/describe/summarize/tally 实测复现）。
/// 重写语义：
///   · 模板 = 首个字符串字面量（`{{`/`}}` 转义 → 字面括号，`{}` → 消耗实参）
///   · 实参 = 顶层逗号分组的 token 序列 → py_token_expr 翻译
///     （`x.len()` → `len(x)` · `String::from(x)` → `str(x)` · 字段链直连）
///   · 有占位 → f-string（ruff UP032 首选形态）；无占位 → 普通字面量
///     （F541 规避 —— f-string 无占位被拒）
fn py_format_macro(args: &MacroArgs) -> String {
    let mut template: Option<String> = None;
    let mut template_quote = '"';
    let mut arg_groups: Vec<Vec<TokenTree>> = Vec::new();
    let mut cur: Vec<TokenTree> = Vec::new();
    let mut seen_template = false;
    for tt in &args.tokens {
        match tt {
            // 顶层逗号 = 实参分组边界
            TokenTree::Token(Token::Punct(p), _) if p == "," => {
                // v0.2.65 修正：模板字面量后的首逗号不产生空组（此前空组进入
                // 实参表 → f-string 占位 `{}` 空表达式 = E999 实测 ×5 文件）
                if !cur.is_empty() {
                    arg_groups.push(std::mem::take(&mut cur));
                }
            }
            // 首个字符串字面量 = 模板（剥离引号，记录引号风格）
            TokenTree::Token(Token::Literal(l), _)
                if !seen_template && matches!(l.kind, LiteralKind::Str { .. }) =>
            {
                let raw = l.raw.as_str();
                if raw.len() >= 2 {
                    template_quote = raw.chars().next().unwrap();
                    template = Some(raw[1..raw.len() - 1].to_string());
                }
                seen_template = true;
            }
            other => cur.push(other.clone()),
        }
    }
    if !cur.is_empty() {
        arg_groups.push(cur);
    }
    let Some(tpl) = template else {
        return "\"\"".into();
    };
    let arg_texts: Vec<String> = arg_groups.iter().map(|g| py_token_expr(g)).collect();

    // 模板走两遍语义：f-string 形态（花括号转义）与 plain 形态（花括号字面）
    let mut f_body = String::new();
    let mut plain_body = String::new();
    let mut placeholders = 0usize;
    let chars: Vec<char> = tpl.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            '{' if next == Some('{') => {
                f_body.push('{');
                plain_body.push('{');
                i += 2;
            }
            '}' if next == Some('}') => {
                f_body.push('}');
                plain_body.push('}');
                i += 2;
            }
            '{' if next == Some('}') => {
                // {} 占位 —— 消耗下一个实参
                if placeholders < arg_texts.len() {
                    f_body.push('{');
                    f_body.push_str(&arg_texts[placeholders]);
                    f_body.push('}');
                    placeholders += 1;
                } else {
                    // 实参不足：占位原样保留（运行期可见的诚实降级）
                    f_body.push_str("{}");
                }
                plain_body.push_str("{}");
                i += 2;
            }
            '{' => {
                f_body.push_str("{{");
                plain_body.push('{');
                i += 1;
            }
            '}' => {
                f_body.push_str("}}");
                plain_body.push('}');
                i += 1;
            }
            _ => {
                f_body.push(c);
                plain_body.push(c);
                i += 1;
            }
        }
    }
    if placeholders == 0 {
        // 无占位 → 普通字符串字面量（花括号无需转义）
        format!("{template_quote}{plain_body}{template_quote}")
    } else {
        // 引号冲突回避：f-string 体内含同款引号时换用另一种（3.12 前不允许嵌套）
        let quote = if f_body.contains(template_quote) {
            if template_quote == '"' { '\'' } else { '"' }
        } else {
            template_quote
        };
        format!("f{quote}{f_body}{quote}")
    }
}

/// vec! 宏 → 列表字面量（v0.2.65：此前兜底 `vec_macro()` 引用未定义函数 F821）
fn py_vec_macro(args: &MacroArgs) -> String {
    // token 形状：单个 Bracket delimited（剥皮）或扁平 token 列表（直接收编）
    let toks: Vec<TokenTree> = if args.tokens.len() == 1 {
        match &args.tokens[0] {
            TokenTree::Delimited { delim: Delimiter::Bracket, tokens, .. } => tokens.clone(),
            _ => args.tokens.clone(),
        }
    } else {
        args.tokens.clone()
    };
    let mut groups: Vec<Vec<TokenTree>> = Vec::new();
    let mut cur: Vec<TokenTree> = Vec::new();
    for tt in toks {
        match &tt {
            TokenTree::Token(Token::Punct(p), _) if p == "," => {
                if !cur.is_empty() {
                    groups.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(tt),
        }
    }
    if !cur.is_empty() {
        groups.push(cur);
    }
    let items: Vec<String> = groups.iter().map(|g| py_token_expr(g)).collect();
    format!("[{}]", items.join(", "))
}

/// 宏实参 token → python 表达式文本（token 级翻译）
/// ----------------------------------------------------------------------------
/// 识别常用形状（整式重写），其余按连接规则拼装（`::` → `.`、字面量原样）：
///   · `x.len()` → `len(x)` · `x.to_string()` → `str(x)`
///   · `String::from(x)` → `str(x)`（字面量直出）
///   · `self.title` / `m.calls` 等字段链 → 直连
fn py_token_expr(tokens: &[TokenTree]) -> String {
    // 形状 1：recv.<len|to_string>()（4 token 精确匹配，空实参括号）
    if tokens.len() == 4 {
        if let (
            TokenTree::Token(Token::Ident(recv), _),
            TokenTree::Token(Token::Punct(p), _),
            TokenTree::Token(Token::Ident(m), _),
            TokenTree::Delimited { delim: Delimiter::Paren, tokens: inner, .. },
        ) = (&tokens[0], &tokens[1], &tokens[2], &tokens[3])
        {
            if inner.is_empty() && (p == "." || p == "::") {
                match m.as_str() {
                    "len" => return format!("len({})", py_ident(recv)),
                    "to_string" | "to_str" => return format!("str({})", py_ident(recv)),
                    _ => {}
                }
            }
        }
    }
    // 形状 2：String::from(x)（字面量直出避免 str("lit") 冗余）
    if tokens.len() == 4 {
        if let (
            TokenTree::Token(Token::Ident(h), _),
            TokenTree::Token(Token::Punct(p), _),
            TokenTree::Token(Token::Ident(m), _),
            TokenTree::Delimited { delim: Delimiter::Paren, tokens: inner, .. },
        ) = (&tokens[0], &tokens[1], &tokens[2], &tokens[3])
        {
            if p == "::" && h == "String" && m == "from" {
                if inner.len() == 1 {
                    if let TokenTree::Token(Token::Literal(l), _) = &inner[0] {
                        if matches!(l.kind, LiteralKind::Str { .. }) {
                            return l.raw.clone();
                        }
                    }
                }
                return format!("str({})", py_token_expr(inner));
            }
        }
    }
    // 通用连接：Ident 直出（py_ident 关键字避让）· `::` → `.` · 字面量 raw · 分组递归
    let mut out = String::new();
    for tt in tokens {
        match tt {
            TokenTree::Token(tok, _) => match tok {
                Token::Ident(s) | Token::RawIdent(s) => out.push_str(&py_ident(s)),
                Token::Literal(l) => out.push_str(&l.raw),
                Token::Label(s) => out.push_str(s),
                Token::Punct(s) => out.push_str(if s == "::" { "." } else { s }),
            },
            TokenTree::Delimited { delim, tokens: inner, .. } => {
                let (open, close) = match delim {
                    Delimiter::Paren => ("(", ")"),
                    Delimiter::Bracket => ("[", "]"),
                    Delimiter::Brace => ("{", "}"),
                };
                out.push_str(open);
                out.push_str(&py_token_expr(inner));
                out.push_str(close);
            }
        }
    }
    out
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

// ────────────────────────────────────────────────────────────────
// v0.2.65 工具：括号剥离 / 模式辅助 / 条件取反
// ────────────────────────────────────────────────────────────────

/// 语句位外层括号剥离（ruff UP034「多余括号」治理，与 dhv-ts pyStripOuter 同源算法）。
///
/// 二元/一元表达式全括号化发射（`Binary → (a op b)`）在语句位（return /
/// 调用实参 / 赋值 RHS）产生冗余包裹。仅剥「完整覆盖整个串的一层括号」：
///   · 深度探测：内层任何位置深度归零后再度上升 = 不是单层包裹，不剥；
///   · 元组保护：顶层逗号（`(a, b)`）的括号是语义，绝不剥。
fn py_strip_outer(s: &str) -> String {
    let t = s.trim();
    if t.len() < 2 || !t.starts_with('(') || !t.ends_with(')') {
        return t.to_string();
    }
    let inner = &t[1..t.len() - 1];
    let mut depth = 0i32;
    for ch in inner.chars() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => return t.to_string(), // 元组字面量 —— 括号是语义
            _ => {}
        }
    }
    inner.trim().to_string()
}

/// 简单名探测（while-let scrutinee 缓存判定：简单名每次迭代重读即单次求值语义）
fn py_is_simple_name(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty()
        && t.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false)
        && t.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// while-let 循环退出条件的取反（常见形态直译，避免双重否定）
fn py_negate(cond: &str) -> String {
    let t = cond.trim();
    if let Some(name) = t.strip_suffix(" is not None") {
        return format!("{} is None", name);
    }
    if let Some(name) = t.strip_suffix(" is None") {
        return format!("{} is not None", name);
    }
    if t == "True" {
        return "False".into();
    }
    if t.starts_with("isinstance(") && t.ends_with(')') {
        return format!("not {}", t);
    }
    format!("not ({})", t)
}

/// 路径是否为 Option::Some 构造形态（[Some] / [Option, Some]）
fn py_is_some_path(path: &crate::ast::Path) -> bool {
    let segs: Vec<&str> = path.segments.iter().map(|s| s.name.as_str()).collect();
    (segs.len() == 1 && segs[0] == "Some") || (segs.len() == 2 && segs[0] == "Option" && segs[1] == "Some")
}

/// 路径是否为 Option::None 形态（[None] / [Option, None]）
fn py_is_none_path(path: &crate::ast::Path) -> bool {
    let segs: Vec<&str> = path.segments.iter().map(|s| s.name.as_str()).collect();
    (segs.len() == 1 && segs[0] == "None") || (segs.len() == 2 && segs[0] == "Option" && segs[1] == "None")
}

/// let 语句的 Some(x) 模式 → 绑定名（python 的 Option = 裸值/None，直接绑定）
fn py_some_bind(pat: &Pattern) -> Option<String> {
    if let PatternKind::TupleStruct { path, elems, .. } = &pat.kind {
        if py_is_some_path(path) && elems.len() == 1 {
            if let PatternKind::Ident { name, .. } = &elems[0].kind {
                return Some(py_ident(&snake_case(&name.name)));
            }
        }
    }
    None
}

// ────────────────────────────────────────────────────────────────
// v0.2.65：python 产物跨文件引用收尾（emit 后置处理）
// ────────────────────────────────────────────────────────────────
//
// dhv(Rust) 的 emit 是「一项一文件」：跨文件类型引用（main.py 引用
// meter.py 的 Meter）此前裸引用 → F821 未定义名（status_line/provider/
// first_ok/describe 实测）。本收尾与 dhv-ts 的 finalizePython 同思路：
//   1. 全 python 产物建注册表（顶层名 → 模块 stem）；
//   2. 逐文件扫描代码（剥离注释/字符串）中的跨文件引用；
//   3. 注入 `from <module> import <names>`（按模块分组、isort 排序、
//      与文件既有导入合并重排 —— I001 友好）；
//   4. `isinstance(x, Ok/Err)` 的 Result 变体 → 注入桩类
//      （`class Ok:` + `_fields/__getitem__`，与枚举 tuple 变体投射同构）。
//
// 局限（诚实边界）：运行期 parity（prelude 助手/_dhv_str 显示语义）不在
// 本批范围 —— dhv(Rust) python 产物是静态投射门禁目标（ruff 全绿），
// 活体运行请用 dhv-ts（nativeRuntime）。

/// 收尾入口：由 codegen::CodegenContext::emit 在产出全部文件后调用（仅处理 python）
pub fn finalize_crossrefs(files: &mut [crate::codegen::GeneratedFile]) {
    use std::collections::{BTreeMap, BTreeSet};

    // ---- 1. 注册表：顶层名 → 模块 stem ----
    let mut registry: BTreeMap<String, String> = BTreeMap::new();
    for f in files.iter().filter(|f| f.lang == "python") {
        let stem = py_module_stem(&f.path);
        for name in py_top_level_defs(&f.content) {
            registry.entry(name).or_insert_with(|| stem.clone());
        }
    }

    // ---- 2. 逐文件：扫描引用 → 注入导入/桩类 ----
    for f in files.iter_mut().filter(|f| f.lang == "python") {
        let own_defs: BTreeSet<String> = py_top_level_defs(&f.content).into_iter().collect();
        let code = py_code_only(&f.content); // 剥离注释与字符串字面量

        // 需要导入的跨文件名（按模块分组）
        let mut needed: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (name, module) in &registry {
            if own_defs.contains(name) {
                continue;
            }
            if py_word_used(&code, name) {
                needed.entry(module.clone()).or_default().insert(name.clone());
            }
        }
        // Result 变体桩（Ok/Err 被引用且未在本文件定义）
        let mut stubs: Vec<&str> = Vec::new();
        for v in ["Ok", "Err"] {
            if !own_defs.contains(v) && py_word_used(&code, v) {
                stubs.push(v);
            }
        }
        if needed.is_empty() && stubs.is_empty() {
            continue; // 自包含文件零改动
        }
        f.content = py_rebuild_head(&f.content, &needed, &stubs);
    }
}

/// 投射路径 → 模块 stem（flat import 语义：同目录互引，与 dhv-ts 约定一致）
fn py_module_stem(path: &str) -> String {
    path.rsplit(['/']).next()
        .map(|s| s.strip_suffix(".py").unwrap_or(s).to_string())
        .unwrap_or_default()
}

/// 顶层定义名收集：`^class X` / `^(async )?def X` / `^X: T = ` / `^X = `
fn py_top_level_defs(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in content.lines() {
        let t = line.trim_end();
        if t.starts_with("class ") {
            if let Some(name) = t["class ".len()..].split(['(', ':', ' ']).next() {
                if !name.is_empty() {
                    out.push(name.to_string());
                }
            }
        } else if let Some(rest) = t.strip_prefix("async def ").or_else(|| t.strip_prefix("def ")) {
            if let Some(name) = rest.split(['(', ':', ' ']).next() {
                if !name.is_empty() {
                    out.push(name.to_string());
                }
            }
        } else {
            // 常量/别名：^NAME: T = v 或 ^NAME = v（仅列首、标识符形态）
            let valid_start = t
                .chars()
                .next()
                .map(|c| c.is_alphabetic() || c == '_')
                .unwrap_or(false);
            if valid_start {
                let name_end = t
                    .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .unwrap_or(t.len());
                let name = &t[..name_end];
                let rest = &t[name_end..];
                let rest_trim = rest.trim_start();
                if rest_trim.starts_with("= ")
                    || rest_trim.starts_with(": ")
                    || rest_trim == "="
                    || rest_trim.starts_with(": ")
                {
                    if !name.is_empty() && name != "if" && name != "while" && name != "for" {
                        out.push(name.to_string());
                    }
                }
            }
        }
    }
    out
}

/// 代码视图：整行注释剔除 + 行内注释截断 + 字符串字面量内容抹除
///（@dhv:hsl-mirror 镜像注释里的名字不构成引用 —— 与 dhv-ts F401 修复同源）。
/// f-string 的占位表达式（`f"{expr}"` 的 `{expr}`）是代码 —— 保留；
/// 普通串与 f-string 字面段抹除（main.py 实测：跨引用藏在 f-string 占位里，
/// 一并抹除 → status_line/WorkStatus/Verdict 漏注入 F821）。
#[derive(PartialEq)]
enum PyScan {
    Code,
    /// (引号, 是否 f-string)
    Str(char, bool),
    /// f-string 占位表达式（深度计数 + 宿主串引号，出占位回 Str）
    Placeholder(i32, char),
}

fn py_code_only(content: &str) -> String {
    let mut out = String::new();
    'line: for line in content.lines() {
        let t = line.trim_start();
        if t.starts_with('#') {
            continue; // 整行注释
        }
        let mut cleaned = String::new();
        let mut state = PyScan::Code;
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            match state {
                PyScan::Code => {
                    if c == '#' {
                        continue 'line; // 行内注释截断
                    }
                    if c == '"' || c == '\'' {
                        state = PyScan::Str(c, false);
                        cleaned.push(' ');
                        continue;
                    }
                    if matches!(c, 'f' | 'F') && matches!(chars.peek(), Some('"' | '\'')) {
                        chars.next(); // 吞引号，进 f-string
                        state = PyScan::Str('"', true);
                        cleaned.push(' ');
                        cleaned.push(' ');
                        continue;
                    }
                    if (c == 'r' || c == 'b' || c == 'R' || c == 'B')
                        && matches!(chars.peek(), Some('"' | '\'' | 'f' | 'F' | 'r' | 'b'))
                    {
                        // 前缀组合（rb/rf…）首字符：留给下一轮处理
                        continue;
                    }
                    cleaned.push(c);
                }
                PyScan::Str(q, is_f) => {
                    if c == '\\' {
                        chars.next(); // 转义对抹除
                        cleaned.push(' ');
                        cleaned.push(' ');
                        continue;
                    }
                    if c == q {
                        state = PyScan::Code;
                        cleaned.push(' ');
                        continue;
                    }
                    if is_f {
                        match c {
                            '{' if chars.peek() == Some(&'{') => {
                                chars.next(); // {{ 字面转义
                                cleaned.push(' ');
                                cleaned.push(' ');
                            }
                            '{' => {
                                state = PyScan::Placeholder(1, q);
                                cleaned.push('{');
                            }
                            _ => cleaned.push(' '),
                        }
                    } else {
                        cleaned.push(' '); // 普通串内容抹除
                    }
                }
                PyScan::Placeholder(depth, q) => {
                    match c {
                        '{' => {
                            state = PyScan::Placeholder(depth + 1, q);
                            cleaned.push('{');
                        }
                        '}' => {
                            if depth <= 1 {
                                state = PyScan::Str(q, true); // 回 f-string 字面段
                                cleaned.push('}');
                            } else {
                                state = PyScan::Placeholder(depth - 1, q);
                                cleaned.push('}');
                            }
                        }
                        _ => cleaned.push(c), // 占位表达式 = 代码，保留
                    }
                }
            }
        }
        out.push_str(&cleaned);
        out.push('\n');
    }
    out
}

/// 词边界引用探测（`X.attr` 形态的 attr 不算裸名使用 —— 属性访问经由
/// 宿主对象解析，import 该名反而 F401「导入未用」，main.py 实测）
fn py_word_used(code: &str, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let bytes = code.as_bytes();
    let w = word.as_bytes();
    let mut i = 0;
    while i + w.len() <= bytes.len() {
        if &bytes[i..i + w.len()] == w {
            // `.` 前缀 = 属性访问位（宿主对象已解析，裸名导入反而未用）
            let before_ok = i == 0
                || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_' || bytes[i - 1] == b'.');
            let after = i + w.len();
            let after_ok = after >= bytes.len() || !(bytes[after].is_ascii_alphanumeric() || bytes[after] == b'_');
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// 头部重排：源映射注释头之后 = 导入块（既有 + 注入合并 isort 排序）→ 桩类 → 原文
fn py_rebuild_head(
    content: &str,
    needed: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    stubs: &[&str],
) -> String {
    let lines: Vec<&str> = content.lines().collect();
    // 1. 头部：前导注释行（源映射围栏头 + generated 标记）
    let mut head_end = 0;
    while head_end < lines.len() && lines[head_end].trim_start().starts_with('#') {
        head_end += 1;
    }
    // 2. 既有导入块（连续 from/import 行；其后空行归入分隔，不计入 rest）
    let mut idx = head_end;
    let mut existing_imports: Vec<String> = Vec::new();
    while idx < lines.len() {
        let t = lines[idx].trim_end();
        if t.starts_with("from ") || t.starts_with("import ") {
            existing_imports.push(t.to_string());
            idx += 1;
        } else {
            break;
        }
    }
    // 导入块后的空行（既有两空行约定）跳过
    while idx < lines.len() && lines[idx].trim().is_empty() {
        idx += 1;
    }
    // 3. 注入导入（按模块分组、名排序）
    let mut injected: Vec<String> = Vec::new();
    for (module, names) in needed {
        let names_sorted: Vec<&String> = names.iter().collect();
        injected.push(format!("from {} import {}", module, names_sorted.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
    }
    // 4. 合并 + isort 排序（按模块名）
    let mut all_imports: Vec<String> = existing_imports;
    all_imports.extend(injected);
    all_imports.sort_by_key(|l| py_import_key(l));

    // 5. 分区（isort 语义：stdlib 在前、本地/三方在后，区间一空行 ——
    //    provider.py 实测：typing + prompt 混排单区被 I001 拒）
    let (stdlib, local): (Vec<&String>, Vec<&String>) = all_imports
        .iter()
        .partition(|l| py_is_stdlib_import(l));
    // 6. 重组
    let mut out = String::new();
    for l in &lines[..head_end] {
        out.push_str(l);
        out.push('\n');
    }
    for l in &stdlib {
        out.push_str(l);
        out.push('\n');
    }
    if !stdlib.is_empty() && !local.is_empty() {
        out.push('\n'); // 区间一空行（isort 分区规范）
    }
    for l in &local {
        out.push_str(l);
        out.push('\n');
    }
    out.push('\n'); // 导入块后两空行（isort 规范）
    out.push('\n');
    for v in stubs {
        out.push_str(&format!(
            "class {v}:\n    def __init__(self, *args):\n        self._fields = args\n    def __getitem__(self, index):\n        return self._fields[index]\n\n\n"
        ));
    }
    for l in &lines[idx..] {
        out.push_str(l);
        out.push('\n');
    }
    out
}

/// isort 排序键：模块名（`from X import …` 取 X；`import X` 取 X）
fn py_import_key(line: &str) -> String {
    if let Some(rest) = line.strip_prefix("from ") {
        rest.split(" import").next().unwrap_or(rest).trim().to_lowercase()
    } else if let Some(rest) = line.strip_prefix("import ") {
        rest.trim().to_lowercase()
    } else {
        line.to_lowercase()
    }
}

/// isort 分区判定：本后端自产导入（dataclasses/typing/enum）与常见 stdlib
fn py_is_stdlib_import(line: &str) -> bool {
    const STDLIB: &[&str] = &[
        "dataclasses", "typing", "enum", "math", "json", "collections", "itertools",
        "functools", "re", "os", "sys", "time", "random", "pathlib", "__future__",
    ];
    let module = if let Some(rest) = line.strip_prefix("from ") {
        rest.split(" import").next().unwrap_or(rest).trim().to_string()
    } else if let Some(rest) = line.strip_prefix("import ") {
        rest.trim().split(['.', ' ']).next().unwrap_or(rest).trim().to_string()
    } else {
        return false;
    };
    STDLIB.contains(&module.as_str())
}
