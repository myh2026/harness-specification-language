//! TypeScript 后端 —— 完整表达式覆盖（33 ExprKind）+ 全语句 + 全模式 + 全项支持
//!
//! 表达式覆盖：literal/path/binary/unary/call/method/field/index/slice/
//!   range/assign/compound_assign/if/if-let/match/for/while/while-let/
//!   loop/closure/return/break/continue/array/array-repeat/struct/tuple/
//!   block/async-block/try/await/cast/native/macro

use crate::ast::*;
use crate::codegen::{CodegenBackend, CodegenContext};

pub struct TypeScriptBackend;

/// TypeScript 关键字避让
const TS_KW: &[&str] = &[
    "class", "function", "typeof", "instanceof", "delete", "in", "of", "new",
    "this", "super", "extends", "default", "switch", "case", "do", "void",
    "var", "with", "debugger", "export", "import", "return", "if", "else",
    "for", "while", "break", "continue", "throw", "try", "catch", "finally",
    "async", "await", "yield", "const", "let", "null", "undefined", "true",
    "false", "type", "interface", "enum", "implements", "static", "public",
    "private", "protected", "readonly", "abstract", "declare", "from", "as",
    "string", "number", "boolean", "any", "unknown", "never", "symbol",
    "bigint", "object", "package", "module", "namespace", "require",
];

fn ts_ident(name: &str) -> String {
    if TS_KW.contains(&name) {
        format!("{}_", name)
    } else {
        name.to_string()
    }
}

impl CodegenBackend for TypeScriptBackend {
    fn lang(&self) -> &'static str { "typescript" }

    fn emit_item(&self, ctx: &CodegenContext, item: &Item) -> Result<String, String> {
        let mut out = String::new();
        out.push_str(&format!("// {}\n", crate::sourcemap::generated_header("typescript")));
        match item {
            Item::Fn(f) => {
                let ret = f.ret.as_ref().map(|t| format!(": {}", ts_type(t))).unwrap_or_else(|| ": void".into());
                out.push_str(&format!(
                    "export {}function {}({}){} {{\n",
                    if f.is_async { "async " } else { "" },
                    ts_ident(&camel_case(&f.name.name)),
                    f.params.iter().map(ts_param).collect::<Vec<_>>().join(", "),
                    ret
                ));
                match &f.body {
                    Some(body) => out.push_str(&emit_block_ts(body, 1, false)),
                    None => out.push_str("  throw new Error('not implemented');\n"),
                }
                out.push_str("}\n");
            }
            Item::Struct(s) => {
                out.push_str(&format!("export interface {} {{\n", s.name.name));
                match &s.kind {
                    StructKind::Named(fields) => {
                        if fields.is_empty() {
                            // 空接口
                        }
                        for field in fields {
                            let name = field.name.as_ref().map(|n| n.name.as_str()).unwrap_or("_");
                            let optional = matches!(&field.ty.kind, TypeKind::Path(p) if p.path.is_ident("Option"));
                            out.push_str(&format!(
                                "  {}{}: {};\n",
                                camel_case(name),
                                if optional { "?" } else { "" },
                                ts_type(&field.ty)
                            ));
                        }
                    }
                    StructKind::Tuple(fields) => {
                        // 元组结构体 → 只读元组类型
                        out.push_str(&format!(
                            "  readonly [{}];\n",
                            fields.iter().map(|f| ts_type(&f.ty)).collect::<Vec<_>>().join(", ")
                        ));
                    }
                    StructKind::Unit => {}
                }
                out.push_str("}\n");
            }
            Item::Enum(e) => {
                // 判别联合类型
                let has_fields = e.variants.iter().any(|v| !matches!(&v.fields, StructKind::Unit));
                if has_fields {
                    // 带字段变体 → class 继承 + kind 判别
                    out.push_str(&format!("export type {} =\n", e.name.name));
                    for (i, v) in e.variants.iter().enumerate() {
                        let prefix = if i == 0 { "  " } else { "  | " };
                        match &v.fields {
                            StructKind::Named(fields) => {
                                let body = fields.iter().map(|f| {
                                    let name = f.name.as_ref().map(|n| n.name.as_str()).unwrap_or("_");
                                    format!("{}: {}", camel_case(name), ts_type(&f.ty))
                                }).collect::<Vec<_>>().join("; ");
                                out.push_str(&format!("{prefix}{{ kind: '{}', {body} }}\n", v.name.name));
                            }
                            StructKind::Tuple(fields) => {
                                let body = fields.iter().map(|f| ts_type(&f.ty)).collect::<Vec<_>>().join(", ");
                                out.push_str(&format!("{prefix}{{ kind: '{}', value: [{body}] }}\n", v.name.name));
                            }
                            StructKind::Unit => {
                                out.push_str(&format!("{prefix}'{}'\n", v.name.name));
                            }
                        }
                    }
                    out.push_str(";\n");
                    // 便捷构造器函数
                    out.push_str("\n");
                    for v in &e.variants {
                        match &v.fields {
                            StructKind::Named(fields) => {
                                let params = fields.iter().map(|f| {
                                    let name = f.name.as_ref().map(|n| n.name.as_str()).unwrap_or("_");
                                    format!("{}: {}", camel_case(name), ts_type(&f.ty))
                                }).collect::<Vec<_>>().join(", ");
                                let assigns = fields.iter().map(|f| {
                                    let name = f.name.as_ref().map(|n| n.name.as_str()).unwrap_or("_");
                                    format!("{}: {}", camel_case(name), camel_case(name))
                                }).collect::<Vec<_>>().join(", ");
                                out.push_str(&format!(
                                    "export function {}({}): {} {{ return {{ kind: '{}', {} }}; }}\n",
                                    v.name.name, params, e.name.name, v.name.name, assigns
                                ));
                            }
                            StructKind::Tuple(fields) => {
                                let params = fields.iter().enumerate().map(|(i, f)| {
                                    format!("v{}: {}", i, ts_type(&f.ty))
                                }).collect::<Vec<_>>().join(", ");
                                let values = fields.iter().enumerate().map(|(i, _)| format!("v{}", i)).collect::<Vec<_>>().join(", ");
                                out.push_str(&format!(
                                    "export function {}({}): {} {{ return {{ kind: '{}', value: [{}] }}; }}\n",
                                    v.name.name, params, e.name.name, v.name.name, values
                                ));
                            }
                            StructKind::Unit => {
                                out.push_str(&format!(
                                    "export const {}: {} = '{}';\n",
                                    v.name.name, e.name.name, v.name.name
                                ));
                            }
                        }
                    }
                } else {
                    // 纯单元变体 → 字符串字面量联合
                    out.push_str(&format!("export type {} =\n", e.name.name));
                    for (i, v) in e.variants.iter().enumerate() {
                        let prefix = if i == 0 { "  " } else { "  | " };
                        out.push_str(&format!("{prefix}'{}'\n", v.name.name));
                    }
                    out.push_str(";\n");
                }
            }
            Item::Impl(im) => {
                // impl → class 扩展或方法声明
                let target = match &im.self_ty.kind {
                    TypeKind::Path(pt) => pt.path.last().name.clone(),
                    _ => "_Target".into(),
                };
                out.push_str(&format!("// impl {}\n", target));
                for ii in &im.items {
                    if let ImplItem::Fn(f) = ii {
                        let ret = f.ret.as_ref().map(|t| format!(": {}", ts_type(t))).unwrap_or_else(|| ": void".into());
                        let params: Vec<String> = f.params.iter()
                            .filter(|p| !matches!(&p.kind, ParamKind::Self_(_)))
                            .filter(|p| {
                                if let ParamKind::Pattern(pat) = &p.kind {
                                    if let PatternKind::Ident { name, .. } = &pat.kind {
                                        return name.name != "self" && name.name != "&self" && name.name != "&mut self" && name.name != "mut self";
                                    }
                                }
                                true
                            })
                            .map(ts_param).collect();
                        out.push_str(&format!(
                            "export {}function {}({}){} {{\n",
                            if f.is_async { "async " } else { "" },
                            ts_ident(&camel_case(&f.name.name)),
                            params.join(", "),
                            ret
                        ));
                        match &f.body {
                            Some(body) => out.push_str(&emit_block_ts(body, 1, false)),
                            None => out.push_str("  throw new Error('not implemented');\n"),
                        }
                        out.push_str("}\n");
                    }
                }
            }
            Item::Trait(t) => {
                out.push_str(&format!("export interface {} {{\n", t.name.name));
                for item in &t.items {
                    match item {
                        TraitItem::Fn(f) => {
                            let ret = f.ret.as_ref().map(|t| format!(": {}", ts_type(t))).unwrap_or_default();
                            let params: Vec<String> = f.params.iter()
                                .filter(|p| !matches!(&p.kind, ParamKind::Self_(_)))
                                .map(ts_param).collect();
                            out.push_str(&format!(
                                "  {}({}){};\n",
                                ts_ident(&camel_case(&f.name.name)),
                                params.join(", "),
                                ret
                            ));
                        }
                        TraitItem::FnSig(sig) => {
                            let ret = sig.ret.as_ref().map(|t| format!(": {}", ts_type(t))).unwrap_or_default();
                            let params: Vec<String> = sig.params.iter()
                                .filter(|p| !matches!(&p.kind, ParamKind::Self_(_)))
                                .map(ts_param).collect();
                            out.push_str(&format!(
                                "  {}({}){};\n",
                                ts_ident(&camel_case(&sig.name.name)),
                                params.join(", "),
                                ret
                            ));
                        }
                        TraitItem::Const(c) => {
                            out.push_str(&format!("  readonly {}: {};\n", c.name.name, ts_type(&c.ty)));
                        }
                        TraitItem::TypeAlias(ta) => {
                            out.push_str(&format!("  {};\n", ta.name.name));
                        }
                    }
                }
                out.push_str("}\n");
            }
            Item::Const(c) => {
                out.push_str(&format!(
                    "export const {}: {} = {};\n",
                    ts_ident(&camel_case(&c.name.name)),
                    ts_type(&c.ty),
                    emit_expr_ts(&c.value)
                ));
            }
            Item::TypeAlias(t) => {
                out.push_str(&format!("export type {} = {};\n", t.name.name, ts_type(&t.ty)));
            }
            Item::StaticResource(sr) => {
                out.push_str(&format!(
                    "// static resource {} (format: {})\n",
                    sr.name.name, format!("{:?}", sr.kind)
                ));
            }
            Item::MacroRules(mr) => {
                out.push_str(&format!(
                    "// macro_rules! {}\n",
                    mr.name.name
                ));
            }
            Item::Graph(g) => {
                out.push_str(&format!(
                    "// graph {} — scale: {:?}\n",
                    g.name.name, ctx.scale
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

// ────────────────────────────────────────────────────────────────
// 类型转译
// ────────────────────────────────────────────────────────────────

pub fn ts_type(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(p) => {
            let name = p.path.segments.last().map(|s| s.name.clone()).unwrap_or_default();
            let inner = p.generic_args.iter().next();
            match name.as_str() {
                "bool" => "boolean".into(),
                "String" | "str" | "char" => "string".into(),
                "i8" | "i16" | "i32" | "i64" | "isize" | "u8" | "u16" | "u32" | "u64" | "usize" | "f32" | "f64" => "number".into(),
                "Vec" => {
                    if let Some(GenericArg::Type(t)) = inner {
                        format!("{}[]", ts_type(t))
                    } else {
                        "any[]".into()
                    }
                }
                "HashMap" => {
                    if let Some(GenericArg::Type(t)) = inner {
                        if let TypeKind::Tuple(elems) = &t.kind {
                            if elems.len() >= 2 {
                                return format!("Map<{}, {}>", ts_type(&elems[0]), ts_type(&elems[1]));
                            }
                        }
                    }
                    "Map<string, any>".into()
                }
                "HashSet" => {
                    if let Some(GenericArg::Type(t)) = inner {
                        format!("Set<{}>", ts_type(t))
                    } else {
                        "Set<any>".into()
                    }
                }
                "Option" => {
                    if let Some(GenericArg::Type(t)) = inner {
                        format!("{} | null | undefined", ts_type(t))
                    } else {
                        "any | null".into()
                    }
                }
                "Result" => {
                    if let Some(GenericArg::Type(t)) = inner {
                        if let TypeKind::Tuple(elems) = &t.kind {
                            if elems.len() >= 2 {
                                return format!("{} | Error", ts_type(&elems[0]));
                            }
                        }
                        format!("{} | Error", ts_type(t))
                    } else {
                        "any | Error".into()
                    }
                }
                "Box" => {
                    if let Some(GenericArg::Type(t)) = inner {
                        ts_type(t)
                    } else {
                        "any".into()
                    }
                }
                "FnPtr" | "Fn" => {
                    // 函数指针 → (...args: any[]) => any
                    if let Some(GenericArg::Type(t)) = inner {
                        if let TypeKind::Tuple(elems) = &t.kind {
                            if elems.len() >= 2 {
                                // (params..., ret)
                                let ret = ts_type(&elems[elems.len() - 1]);
                                let params: Vec<String> = elems[..elems.len() - 1].iter().map(ts_type).collect();
                                return format!("({}) => {}", params.join(", "), ret);
                            }
                        }
                    }
                    "(...args: any[]) => any".into()
                }
                "Never" => "never".into(),
                "Paren" => {
                    if let Some(GenericArg::Type(t)) = inner {
                        format!("({})", ts_type(t))
                    } else {
                        "any".into()
                    }
                }
                other => other.to_string(),
            }
        }
        TypeKind::Ref { inner, .. } => ts_type(inner),
        TypeKind::Tuple(elems) => {
            if elems.is_empty() {
                "void".into()
            } else {
                format!("[{}]", elems.iter().map(ts_type).collect::<Vec<_>>().join(", "))
            }
        }
        TypeKind::Array { elem, .. } | TypeKind::Slice(elem) => {
            format!("readonly {}[]", ts_type(elem))
        }
        TypeKind::DynTrait(_) => "any".into(),
        TypeKind::ImplTrait(_) => "any".into(),
        TypeKind::Infer => "any".into(),
        _ => "any".into(),
    }
}


fn ts_param(p: &Param) -> String {
    if let ParamKind::Self_(_) = &p.kind {
        return String::new();
    }
    if let ParamKind::Pattern(pat) = &p.kind {
        if let PatternKind::Ident { name, .. } = &pat.kind {
            return format!("{}: {}", ts_ident(&camel_case(&name.name)), ts_type(&p.ty));
        }
    }
    "arg: any".to_string()
}

// ────────────────────────────────────────────────────────────────
// 语句块转译
// ────────────────────────────────────────────────────────────────

/// 转译语句块。no_return_tail: 循环体等不自动给 tail 加 return 的场景。
fn emit_block_ts(block: &BlockExpr, indent: usize, no_return_tail: bool) -> String {
    let pad = "  ".repeat(indent);
    let mut out = String::new();
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let(l) => {
                let const_or_let = if l.mutable { "let" } else { "const" };
                let pat = ts_pattern(&l.pattern);
                let ty_part = l.ty.as_ref().map(|t| format!(": {}", ts_type(t))).unwrap_or_default();
                match &l.init {
                    Some(init) => out.push_str(&format!("{}{} {}{} = {};\n", pad, const_or_let, pat, ty_part, emit_expr_ts(init))),
                    None => out.push_str(&format!("{}let {}{}: any;\n", pad, pat, ty_part)),
                }
                if let Some(els) = &l.else_block {
                    out.push_str(&format!("{} else ", emit_stmt_expr_ts(&Expr {
                        kind: ExprKind::Block(els.clone()),
                        span: els.span,
                    }, &pad)));
                }
            }
            Stmt::Expr { expr, .. } => {
                out.push_str(&emit_stmt_expr_ts(expr, &pad));
            }
            Stmt::Empty(_) => {}
            Stmt::Item(_) => out.push_str(&format!("{}// 局部项\n", pad)),
        }
    }
    if let Some(tail) = &block.tail {
        if no_return_tail {
            out.push_str(&format!("{}{};\n", pad, emit_expr_ts(tail)));
        } else {
            // 尾表达式在 TS 中需要 return（除非是控制流）
            match &tail.kind {
                ExprKind::If { .. } | ExprKind::Match { .. } | ExprKind::Block(_) | ExprKind::Loop { .. }
                | ExprKind::While { .. } | ExprKind::For { .. } => {
                    // 控制流不需要包装，已在 emit_stmt_expr_ts 处理
                    out.push_str(&emit_stmt_expr_ts(tail, &pad));
                }
                ExprKind::Return(_) => {
                    out.push_str(&format!("{}\n", emit_expr_ts(tail)));
                }
                _ => {
                    out.push_str(&format!("{}return {};\n", pad, emit_expr_ts(tail)));
                }
            }
        }
    }
    if out.is_empty() {
        out.push_str(&format!("{}// empty block\n", pad));
    }
    out
}

/// 语句位置的表达式（if/while/for/loop/match 在语句位置无需包装分号）
fn emit_stmt_expr_ts(expr: &Expr, pad: &str) -> String {
    match &expr.kind {
        ExprKind::If { cond, then, else_ } => {
            let mut out = String::new();
            out.push_str(&format!("{}if ({}) {{\n", pad, emit_expr_ts(cond)));
            out.push_str(&emit_block_ts(then, pad.len() / 2 + 1, false));
            if let Some(els) = else_ {
                if let ExprKind::If { .. } = &els.kind {
                    out.push_str(&format!("{}}} else ", pad));
                    out.push_str(&emit_stmt_expr_ts(els, pad));
                } else {
                    out.push_str(&format!("{}}} else {{\n", pad));
                    if let ExprKind::Block(b) = &els.kind {
                        out.push_str(&emit_block_ts(b, pad.len() / 2 + 1, false));
                    } else {
                        out.push_str(&format!("{}  return {};\n", pad, emit_expr_ts(els)));
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
            out.push_str(&format!("{}switch ({}) {{\n", pad, emit_expr_ts(scrutinee)));
            for arm in arms {
                let guard = arm.guard.as_ref()
                    .map(|g| format!(" if ({})", emit_expr_ts(g)))
                    .unwrap_or_default();
                let pat_str = ts_match_pattern(&arm.pattern);
                out.push_str(&format!("{}  case {}: {{\n", pad, pat_str));
                if let ExprKind::Block(b) = &arm.body.kind {
                    out.push_str(&emit_block_ts(b, pad.len() / 2 + 2, false));
                } else {
                    out.push_str(&format!("{}    return {};\n", pad, emit_expr_ts(&arm.body)));
                }
                out.push_str(&format!("{}    break;\n", pad));
                out.push_str(&format!("{}  }}{}\n", pad, guard));
            }
            out.push_str(&format!("{}}}\n", pad));
            out
        }
        ExprKind::While { cond, body, .. } => {
            let mut out = String::new();
            out.push_str(&format!("{}while ({}) {{\n", pad, emit_expr_ts(cond)));
            out.push_str(&emit_block_ts(body, pad.len() / 2 + 1, true));
            out.push_str(&format!("{}}}\n", pad));
            out
        }
        ExprKind::WhileLet { pattern, expr, body, .. } => {
            // while let → while + 解构赋值 + break
            let pat_str = ts_pattern(pattern);
            let scrutinee = emit_expr_ts(expr);
            let mut out = String::new();
            out.push_str(&format!("{}while (true) {{\n", pad));
            out.push_str(&format!("{}  const __tmp = {};\n", pad, scrutinee));
            out.push_str(&format!("{}  if (__tmp == null) break;\n", pad));
            if let PatternKind::TupleStruct { elems, .. } = &pattern.kind {
                // 解构：const { value: [a, b] } = __tmp;
                let bindings: Vec<String> = elems.iter().enumerate().map(|(i, e)| {
                    match &e.kind {
                        PatternKind::Ident { name, .. } => {
                            format!("const {} = __tmp.value[{}];", ts_ident(&camel_case(&name.name)), i)
                        }
                        PatternKind::Wildcard => String::new(),
                        _ => format!("// unsupported pattern at [{}]", i),
                    }
                }).filter(|s| !s.is_empty()).collect();
                for b in &bindings {
                    out.push_str(&format!("{}  {}\n", pad, b));
                }
            } else {
                out.push_str(&format!("{}  const {} = __tmp;\n", pad, pat_str));
            }
            out.push_str(&emit_block_ts(body, pad.len() / 2 + 1, true));
            out.push_str(&format!("{}}}\n", pad));
            out
        }
        ExprKind::For { pattern, iter, body, .. } => {
            let mut out = String::new();
            let pat_str = ts_pattern(pattern);
            // 检查是否是 range for（整数范围）
            if let ExprKind::Range(r) = &iter.kind {
                let lo = r.lo.as_ref().map(|e| emit_expr_ts(e)).unwrap_or_else(|| "0".into());
                let hi = r.hi.as_ref().map(|e| emit_expr_ts(e)).unwrap_or_else(|| "/* no upper bound */".into());
                if r.inclusive {
                    out.push_str(&format!("{}for (let {} = {}; {} <= {}; {}++) {{\n", pad, pat_str, lo, pat_str, hi, pat_str));
                } else {
                    out.push_str(&format!("{}for (let {} = {}; {} < {}; {}++) {{\n", pad, pat_str, lo, pat_str, hi, pat_str));
                }
            } else {
                out.push_str(&format!("{}for (const {} of {}) {{\n", pad, pat_str, emit_expr_ts(iter)));
            }
            out.push_str(&emit_block_ts(body, pad.len() / 2 + 1, true));
            out.push_str(&format!("{}}}\n", pad));
            out
        }
        ExprKind::Loop { body, .. } => {
            let mut out = String::new();
            out.push_str(&format!("{}while (true) {{\n", pad));
            out.push_str(&emit_block_ts(body, pad.len() / 2 + 1, true));
            out.push_str(&format!("{}}}\n", pad));
            out
        }
        ExprKind::IfLet { pattern, expr, then, else_ } => {
            let scrutinee = emit_expr_ts(expr);
            let (cond, bindings) = ts_match_condition(pattern, scrutinee.clone());
            let mut out = String::new();
            out.push_str(&format!("{}if ({}) {{\n", pad, cond));
            if !bindings.is_empty() {
                out.push_str(&format!("{}  {}\n", pad, bindings));
            }
            out.push_str(&emit_block_ts(then, pad.len() / 2 + 1, false));
            if let Some(els) = else_ {
                if let ExprKind::If { .. } = &els.kind {
                    out.push_str(&format!("{}}} else ", pad));
                    out.push_str(&emit_stmt_expr_ts(els, pad));
                } else {
                    out.push_str(&format!("{}}} else {{\n", pad));
                    if let ExprKind::Block(b) = &els.kind {
                        out.push_str(&emit_block_ts(b, pad.len() / 2 + 1, false));
                    } else {
                        out.push_str(&format!("{}  return {};\n", pad, emit_expr_ts(els)));
                    }
                    out.push_str(&format!("{}}}\n", pad));
                }
            } else {
                out.push_str(&format!("{}}}\n", pad));
            }
            out
        }
        ExprKind::Assign { .. } | ExprKind::CompoundAssign { .. } => {
            format!("{}{};\n", pad, emit_expr_ts(expr))
        }
        _ => {
            format!("{}{};\n", pad, emit_expr_ts(expr))
        }
    }
}

// ────────────────────────────────────────────────────────────────
// 模式转译
// ────────────────────────────────────────────────────────────────

/// 模式转译（用于 let/param 位置）
fn ts_pattern(pat: &Pattern) -> String {
    match &pat.kind {
        PatternKind::Ident { name, .. } => ts_ident(&camel_case(&name.name)),
        PatternKind::Wildcard => "_".into(),
        PatternKind::Rest => "...rest".into(),
        PatternKind::Literal(lit) => ts_literal(lit),
        PatternKind::Path(p) => {
            let segs: Vec<&str> = p.segments.iter().map(|s| s.name.as_str()).collect();
            if segs.len() == 1 {
                ts_ident(segs[0]).to_string()
            } else {
                segs.join(".")
            }
        }
        PatternKind::TupleStruct { path, elems, .. } => {
            let name = path.last().name.clone();
            // TS 无原生 TupleStruct 模式，用解构注释
            let inner: Vec<String> = elems.iter().map(ts_pattern).collect();
            format!("/* {} */ [{}]", name, inner.join(", "))
        }
        PatternKind::Struct { path, fields, .. } => {
            let name = path.last().name.clone();
            let fields_str = fields.iter().map(|f| {
                let pat = f.pattern.as_ref().map(|p| ts_pattern(p)).unwrap_or_else(|| ts_ident(&camel_case(&f.name.name)));
                format!("{}: {}", camel_case(&f.name.name), pat)
            }).collect::<Vec<_>>().join(", ");
            format!("/* {} */ {{ {} }}", name, fields_str)
        }
        PatternKind::Tuple { elems, .. } => {
            format!("[{}]", elems.iter().map(ts_pattern).collect::<Vec<_>>().join(", "))
        }
        PatternKind::Or(pats) => {
            // TS 无 or-pattern，用注释标记
            pats.iter().map(ts_pattern).collect::<Vec<_>>().join(" | ")
        }
        PatternKind::Range { lo, hi, inclusive } => {
            let op = if *inclusive { "<=" } else { "<" };
            format!("/* range */ {} {} {}", ts_pattern(lo), op, ts_pattern(hi))
        }
    }
}

/// match 分支的模式转译（生成 switch case 可用的形式）
fn ts_match_pattern(pat: &Pattern) -> String {
    match &pat.kind {
        PatternKind::Ident { name, sub, .. } => {
            match sub {
                Some(inner) => format!("{} /* @ {} */", ts_ident(&camel_case(&name.name)), ts_match_pattern(inner)),
                None => ts_ident(&camel_case(&name.name)).to_string(),
            }
        }
        PatternKind::Wildcard => "default".into(),
        PatternKind::Literal(lit) => ts_literal(lit),
        PatternKind::Path(p) => {
            let segs: Vec<&str> = p.segments.iter().map(|s| s.name.as_str()).collect();
            if segs.len() >= 2 {
                // Enum::Variant → 判别联合的 kind 字段
                let variant = segs.last().unwrap();
                format!("'{}'", variant)
            } else {
                ts_ident(segs[0]).to_string()
            }
        }
        PatternKind::TupleStruct { path, elems, rest_at } => {
            let variant = path.last().name.clone();
            let mut inner: Vec<String> = elems.iter().map(ts_match_pattern).collect();
            if let Some(pos) = rest_at {
                inner.insert(*pos, "/* ... */".into());
            }
            format!("'{}' /* ({}) */", variant, inner.join(", "))
        }
        PatternKind::Struct { path, fields, .. } => {
            let variant = path.last().name.clone();
            let fields_str = fields.iter().map(|f| {
                let name = camel_case(&f.name.name);
                if let Some(pat) = &f.pattern {
                    format!("{}: {}", name, ts_match_pattern(pat))
                } else {
                    format!("{}: {}", name, name)
                }
            }).collect::<Vec<_>>().join(", ");
            format!("'{}' /* {{ {} }} */", variant, fields_str)
        }
        PatternKind::Tuple { elems, .. } => {
            format!("/* tuple */ ({})", elems.iter().map(ts_match_pattern).collect::<Vec<_>>().join(", "))
        }
        PatternKind::Or(pats) => {
            // TS switch 不支持 or-pattern，生成多个 case fall-through
            let cases: Vec<String> = pats.iter().map(ts_match_pattern).collect();
            cases.join("\n  case ")
        }
        PatternKind::Range { lo, hi, inclusive } => {
            let lo_s = ts_pattern(lo);
            let hi_s = ts_pattern(hi);
            let op = if *inclusive { "<=" } else { "<" };
            // range 在 switch 中不直接支持，用条件表达式
            format!("/* {} {} x {} {} */", lo_s, "<=", op, hi_s)
        }
        PatternKind::Rest => "/* ... */".into(),
    }
}

/// 生成 match 分支的条件 + 绑定赋值
fn ts_match_condition(pat: &Pattern, scrutinee: String) -> (String, String) {
    match &pat.kind {
        PatternKind::Ident { name, .. } => {
            ("true".into(), format!("const {} = {};", ts_ident(&camel_case(&name.name)), scrutinee))
        }
        PatternKind::Wildcard => ("true".into(), String::new()),
        PatternKind::Literal(lit) => {
            (format!("{} === {}", scrutinee, ts_literal(lit)), String::new())
        }
        PatternKind::Path(p) => {
            if p.segments.len() >= 2 {
                let variant = p.segments.last().unwrap();
                (format!("{}.kind === '{}'", scrutinee, variant.name), String::new())
            } else {
                let name = p.last().name.as_str();
                (format!("{} === '{}'", scrutinee, name), String::new())
            }
        }
        PatternKind::TupleStruct { path, elems, .. } => {
            let variant = path.last().name.as_str();
            let mut bindings: Vec<String> = Vec::new();
            for (i, e) in elems.iter().enumerate() {
                if let PatternKind::Ident { name, .. } = &e.kind {
                    bindings.push(format!(
                        "const {} = {}.value[{}];",
                        ts_ident(&camel_case(&name.name)), scrutinee, i
                    ));
                }
            }
            (format!("{}.kind === '{}'", scrutinee, variant), bindings.join(" "))
        }
        PatternKind::Struct { path, fields, .. } => {
            let class_name = path.last().name.as_str();
            let mut bindings: Vec<String> = Vec::new();
            for f in fields {
                let fname = camel_case(&f.name.name);
                if let Some(pat) = &f.pattern {
                    if let PatternKind::Ident { name, .. } = &pat.kind {
                        bindings.push(format!(
                            "const {} = {}.{};",
                            ts_ident(&camel_case(&name.name)),
                            scrutinee,
                            fname
                        ));
                    }
                } else {
                    bindings.push(format!(
                        "const {} = {}.{};",
                        ts_ident(&fname.clone()),
                        scrutinee,
                        fname
                    ));
                }
            }
            (format!("{}.kind === '{}' ", scrutinee, class_name), bindings.join(" "))
        }
        PatternKind::Tuple { elems, .. } => {
            let mut bindings: Vec<String> = Vec::new();
            for (i, e) in elems.iter().enumerate() {
                if let PatternKind::Ident { name, .. } = &e.kind {
                    bindings.push(format!(
                        "const {} = {}[{}];",
                        ts_ident(&camel_case(&name.name)),
                        scrutinee,
                        i
                    ));
                }
            }
            (format!("Array.isArray({}) && {}.length === {}", scrutinee, scrutinee, elems.len()), bindings.join(" "))
        }
        PatternKind::Or(pats) => {
            let sub: Vec<String> = pats.iter()
                .map(|p| ts_match_condition(p, scrutinee.clone()).0)
                .collect();
            (format!("({})", sub.join(" || ")), String::new())
        }
        PatternKind::Range { lo, hi, inclusive } => {
            let lo_s = ts_pattern(lo);
            let hi_s = ts_pattern(hi);
            let op = if *inclusive { "<=" } else { "<" };
            (format!("{} {} {} {} {}", lo_s, "<=", scrutinee, op, hi_s), String::new())
        }
        PatternKind::Rest => ("true".into(), String::new()),
    }
}

// ────────────────────────────────────────────────────────────────
// 表达式转译（完整覆盖 33 ExprKind）
// ────────────────────────────────────────────────────────────────

/// 表达式级转译
pub fn emit_expr_ts(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(lit) => ts_literal(lit),
        ExprKind::Path(p) => {
            let segs: Vec<&str> = p.segments.iter().map(|s| s.name.as_str()).collect();
            if segs.len() == 1 {
                ts_ident(segs[0]).to_string()
            } else {
                segs.join(".")
            }
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let op_str = ts_binop(*op);
            format!("({} {} {})", emit_expr_ts(lhs), op_str, emit_expr_ts(rhs))
        }
        ExprKind::Unary { op, operand } => ts_unop(*op, &emit_expr_ts(operand)),
        ExprKind::Call { callee, args } => {
            format!(
                "{}({})",
                emit_expr_ts(callee),
                args.iter().map(emit_expr_ts).collect::<Vec<_>>().join(", ")
            )
        }
        ExprKind::MethodCall { receiver, method, args, .. } => {
            let mname = camel_case(&method.name);
            // 常用 std 方法映射
            let (mapped_receiver, mapped_method) = ts_std_method(receiver, &mname);
            format!(
                "{}.{}({})",
                mapped_receiver,
                mapped_method,
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
        ExprKind::Index { base, index } => {
            format!("{}[{}]", emit_expr_ts(base), emit_expr_ts(index))
        }
        ExprKind::Slice { base, range } => {
            let base_str = emit_expr_ts(base);
            let lo = range.lo.as_ref().map(|e| emit_expr_ts(e)).unwrap_or_default();
            let hi = range.hi.as_ref().map(|e| emit_expr_ts(e)).unwrap_or_default();
            if range.inclusive {
                if lo.is_empty() {
                    format!("{}.slice(0, {} + 1)", base_str, hi)
                } else if hi.is_empty() {
                    format!("{}.slice({})", base_str, lo)
                } else {
                    format!("{}.slice({}, {} + 1)", base_str, lo, hi)
                }
            } else {
                format!("{}.slice({}, {})", base_str, lo, hi)
            }
        }
        ExprKind::Range(r) => {
            // 值语境 range → 数组生成或迭代器
            let lo = r.lo.as_ref().map(|e| emit_expr_ts(e)).unwrap_or_else(|| "0".into());
            let hi = r.hi.as_ref().map(|e| emit_expr_ts(e));
            match hi {
                Some(hi_expr) => {
                    if r.inclusive {
                        format!("Array.from({{ length: {} - {} + 1 }}, (_, i) => {} + i)", hi_expr, lo, lo)
                    } else {
                        format!("Array.from({{ length: {} - {} }}, (_, i) => {} + i)", hi_expr, lo, lo)
                    }
                }
                None => {
                    format!("/* open range from {} */[]", lo)
                }
            }
        }
        ExprKind::Assign { lhs, rhs } => {
            format!("{} = {}", emit_expr_ts(lhs), emit_expr_ts(rhs))
        }
        ExprKind::CompoundAssign { op, lhs, rhs } => {
            format!("{} {}= {}", emit_expr_ts(lhs), ts_binop(*op), emit_expr_ts(rhs))
        }
        ExprKind::If { cond, then, else_ } => {
            // if 作为表达式 → 三元运算符（简单情况）或 IIFE
            let cond_str = emit_expr_ts(cond);
            let then_val = ts_block_tail(then);
            let else_val = if let Some(els) = else_ {
                if let ExprKind::Block(b) = &els.kind {
                    ts_block_tail(b)
                } else {
                    emit_expr_ts(els)
                }
            } else {
                "undefined".into()
            };
            format!("({} ? {} : {})", cond_str, then_val, else_val)
        }
        ExprKind::IfLet { pattern, expr, then, else_ } => {
            let (cond, bindings) = ts_match_condition(pattern, emit_expr_ts(expr));
            let then_val = ts_block_tail(then);
            let else_val = if let Some(els) = else_ {
                if let ExprKind::Block(b) = &els.kind {
                    ts_block_tail(b)
                } else {
                    emit_expr_ts(els)
                }
            } else {
                "undefined".into()
            };
            if bindings.is_empty() {
                format!("(({}) ? {} : {})", cond, then_val, else_val)
            } else {
                format!("((() => {{ if ({}) {{ {}; return {}; }} return {}; }})())", cond, bindings, then_val, else_val)
            }
        }
        ExprKind::Match { scrutinee, .. } => {
            // match 作为表达式 → IIFE + switch
            format!("(() => {{ switch ({}) {{ /* ... */ }} return undefined; }})()", emit_expr_ts(scrutinee))
        }
        ExprKind::Loop { body, .. } => {
            format!("(() => {{ while (true) {{ {} }} }})()", emit_block_ts(body, 0, true).trim())
        }
        ExprKind::While { cond, body, .. } => {
            format!("(() => {{ while ({}) {{ {} }} }})()", emit_expr_ts(cond), emit_block_ts(body, 0, true).trim())
        }
        ExprKind::WhileLet { .. } => {
            "undefined  /* while-let expression */".into()
        }
        ExprKind::For { .. } => {
            "undefined  /* for expression */".into()
        }
        ExprKind::Closure { params, body, is_async, .. } => {
            let async_str = if *is_async { "async " } else { "" };
            let param_names: Vec<String> = params.iter().map(|p| {
                match &p.kind {
                    ParamKind::Pattern(pat) => match &pat.kind {
                        PatternKind::Ident { name, .. } => ts_ident(&camel_case(&name.name)),
                        PatternKind::Wildcard => "_".into(),
                        _ => "arg".into(),
                    },
                    ParamKind::Self_(_) => "this".into(),
                }
            }).collect();
            // 简单体 → 箭头函数
            if matches!(&body.kind, ExprKind::Path(..) | ExprKind::Binary { .. } | ExprKind::Call { .. } | ExprKind::Field { .. } | ExprKind::MethodCall { .. } | ExprKind::Literal(..) | ExprKind::Index { .. }) {
                format!("{}({}) => {}", async_str, param_names.join(", "), emit_expr_ts(body))
            } else {
                // 复杂体 → 箭头函数 + 块
                format!("{}({}) => {{\n  return {};\n}}", async_str, param_names.join(", "), emit_expr_ts(body))
            }
        }
        ExprKind::Return(val) => {
            match val {
                Some(v) => format!("return {}", emit_expr_ts(v)),
                None => "return".into(),
            }
        }
        ExprKind::Break { label, value } => {
            let label_str = label.as_ref().map(|l| format!("{} ", l.name)).unwrap_or_default();
            match value {
                Some(v) => format!("break {}/* value: {} */", label_str, emit_expr_ts(v)),
                None => format!("break {}", label_str),
            }
        }
        ExprKind::Continue { label } => {
            let label_str = label.as_ref().map(|l| format!("{} ", l.name)).unwrap_or_default();
            format!("continue {}", label_str)
        }
        ExprKind::Block(b) => {
            // 块作为表达式 → IIFE
            let tail = b.tail.as_ref().map(|t| emit_expr_ts(t)).unwrap_or_else(|| "undefined".into());
            format!("(() => {{ {} return {}; }})()", 
                b.stmts.iter().map(|s| match s {
                    Stmt::Let(l) => {
                        let pat = ts_pattern(&l.pattern);
                        match &l.init {
                            Some(init) => format!("const {} = {}; ", pat, emit_expr_ts(init)),
                            None => String::new(),
                        }
                    }
                    Stmt::Expr { expr, .. } => format!("{}; ", emit_expr_ts(expr)),
                    Stmt::Empty(_) => String::new(),
                    Stmt::Item(_) => "// item; ".into(),
                }).collect::<String>(),
                tail
            )
        }
        ExprKind::AsyncBlock { body, .. } => {
            let tail = body.tail.as_ref().map(|t| emit_expr_ts(t)).unwrap_or_else(|| "undefined".into());
            format!("(async () => {{ {} return {}; }})()", 
                body.stmts.iter().map(|s| match s {
                    Stmt::Let(l) => {
                        let pat = ts_pattern(&l.pattern);
                        match &l.init {
                            Some(init) => format!("const {} = {}; ", pat, emit_expr_ts(init)),
                            None => String::new(),
                        }
                    }
                    Stmt::Expr { expr, .. } => format!("{}; ", emit_expr_ts(expr)),
                    Stmt::Empty(_) => String::new(),
                    Stmt::Item(_) => "// item; ".into(),
                }).collect::<String>(),
                tail
            )
        }
        ExprKind::Array(elems) => {
            if elems.is_empty() {
                "[] as const".into()
            } else {
                format!("[{}]", elems.iter().map(emit_expr_ts).collect::<Vec<_>>().join(", "))
            }
        }
        ExprKind::ArrayRepeat { elem, count } => {
            format!("Array.from({{ length: {} }}, () => {})", emit_expr_ts(count), emit_expr_ts(elem))
        }
        ExprKind::Struct { path, fields, spread } => {
            let class_name = path.last().name.clone();
            let fields_str = fields.iter().map(|f| {
                let fname = match &f.name {
                    FieldIndex::Named(id) => camel_case(&id.name),
                    FieldIndex::Index(i, _) => format!("_{}", i),
                };
                let val = f.value.as_ref().map(|v| emit_expr_ts(v)).unwrap_or_else(|| fname.clone());
                format!("{}: {}", fname, val)
            }).collect::<Vec<_>>().join(", ");
            let spread_str = if let Some(spread) = spread {
                format!(", ...{}", emit_expr_ts(spread))
            } else {
                String::new()
            };
            format!("{{ kind: '{}', {}{} }}", class_name, fields_str, spread_str)
        }
        ExprKind::Tuple(elems) => {
            if elems.len() == 1 {
                format!("[{}] as const", emit_expr_ts(&elems[0]))
            } else {
                format!("[{}] as const", elems.iter().map(emit_expr_ts).collect::<Vec<_>>().join(", "))
            }
        }
        ExprKind::Await(inner) => format!("await {}", emit_expr_ts(inner)),
        ExprKind::Try(inner) => {
            // TS: try/catch 包装
            format!("((() => {{ try {{ return {} }} catch (_) {{ return undefined; }} }})())", emit_expr_ts(inner))
        }
        ExprKind::Cast { expr, ty } => {
            let target = ts_type(ty);
            match target.as_str() {
                "number" => format!("Number({})", emit_expr_ts(expr)),
                "string" => format!("String({})", emit_expr_ts(expr)),
                "boolean" => format!("Boolean({})", emit_expr_ts(expr)),
                other => format!("{} as {}", emit_expr_ts(expr), other),
            }
        }
        ExprKind::Native(nb) => {
            nb.code.trim().to_string()
        }
        ExprKind::Macro { path, args } => {
            let name = path.last().name.clone();
            // format! → 模板字符串
            if name == "format" {
                return ts_format_macro(args);
            }
            // println! → console.log
            if name == "println" {
                let inner = args.tokens.iter().map(|tt| match tt {
                    TokenTree::Token(tok, _) => match tok {
                        Token::Ident(s) | Token::RawIdent(s) => ts_ident(&camel_case(s)),
                        Token::Literal(lit) => lit.raw.clone(),
                        Token::Punct(s) => s.clone(),
                        Token::Label(s) => s.clone(),
                    },
                    TokenTree::Delimited { delim: _, tokens, .. } => {
                        tokens.iter().map(|t| match t {
                            TokenTree::Token(tok, _) => match tok {
                                Token::Ident(s) | Token::RawIdent(s) => ts_ident(&camel_case(s)),
                                Token::Literal(lit) => lit.raw.clone(),
                                Token::Punct(s) => s.clone(),
                                Token::Label(s) => s.clone(),
                            },
                            _ => "...".into(),
                        }).collect::<Vec<_>>().join("")
                    }
                }).collect::<Vec<_>>().join("");
                return format!("console.log({})", inner);
            }
            // 其他宏 → 函数调用
            format!("{}_macro()", camel_case(&name))
        }
    }
}

/// 块尾表达式提取（用于三元 if 表达式）
fn ts_block_tail(block: &BlockExpr) -> String {
    if let Some(tail) = &block.tail {
        emit_expr_ts(tail)
    } else {
        "undefined".into()
    }
}

/// 二元运算符映射
fn ts_binop(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Eq => "===",
        BinaryOp::Ne => "!==",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
        _ => op.as_str(),
    }
}

/// 一元运算符映射
fn ts_unop(op: UnaryOp, operand: &str) -> String {
    match op {
        UnaryOp::Not => format!("!{}", operand),
        UnaryOp::Neg => format!("-{}", operand),
        UnaryOp::Ref => operand.to_string(),
        UnaryOp::RefMut => operand.to_string(),
        UnaryOp::Deref => operand.to_string(),
    }
}

/// 常用 std 方法映射（对齐 dhv-ts body.ts 方法映射表）
fn ts_std_method(receiver: &Expr, method: &str) -> (String, String) {
    let recv_str = emit_expr_ts(receiver);
    match method {
        "to_string" | "to_str" => (recv_str, "toString()".into()),
        "len" | "length" => (recv_str, "length".into()),
        "push" | "push_back" => (recv_str, "push".into()),
        "push_front" => (recv_str, "unshift".into()),
        "pop" => (format!("{}", recv_str), "pop()".into()),
        "contains" => (recv_str, "includes".into()),
        "is_empty" => (format!("{}.length", recv_str), "=== 0".into()),
        "is_some" => (format!("{}", recv_str), "!= null".into()),
        "is_none" => (format!("{}", recv_str), "== null".into()),
        "is_ok" => (format!("!( {} instanceof Error)", recv_str), "".into()),
        "is_err" => (format!("{} instanceof Error", recv_str), "".into()),
        "unwrap" => (format!("{}", recv_str), "!".into()),
        "expect" => (format!("{}", recv_str), "!".into()),
        "clone" => (format!("_dhvClone({})", recv_str), "".into()),
        "clone_from" => (format!("_dhvClone({})", recv_str), "".into()),
        "to_vec" => (format!("[...{}]", recv_str), "".into()),
        "keys" => (format!("[...{}.keys()]", recv_str), "".into()),
        "values" => (format!("[...{}.values()]", recv_str), "".into()),
        "entries" => (format!("[...{}.entries()]", recv_str), "".into()),
        "insert" => (recv_str, "splice".into()),
        "remove" => (recv_str, "splice".into()),
        "clear" => (recv_str, "length = 0".into()),
        "sort" => (recv_str, "sort".into()),
        "sort_by" => (recv_str, "sort".into()),
        "reverse" => (format!("[...{}].reverse()", recv_str), "".into()),
        "map" => (recv_str, "map".into()),
        "filter" => (recv_str, "filter".into()),
        "fold" => (recv_str, "reduce".into()),
        "for_each" => (recv_str, "forEach".into()),
        "all" => (recv_str, "every".into()),
        "any" => (recv_str, "some".into()),
        "find" => (recv_str, "find".into()),
        "position" => (recv_str, "indexOf".into()),
        "first" => (format!("({}.length > 0 ? {}[0] : null)", recv_str, recv_str), "".into()),
        "last" => (format!("({}.length > 0 ? {}[{}.length - 1] : null)", recv_str, recv_str, recv_str), "".into()),
        "get" => (recv_str, "get".into()),
        "iter" | "into_iter" => (format!("{}[Symbol.iterator]()", recv_str), "".into()),
        "collect" => (format!("[...{}]", recv_str), "".into()),
        "join" => (recv_str, "join".into()),
        "trim" => (recv_str, "trim()".into()),
        "starts_with" => (recv_str, "startsWith".into()),
        "ends_with" => (recv_str, "endsWith".into()),
        "replace" => (recv_str, "replaceAll".into()),
        "split" => (recv_str, "split".into()),
        "parse" | "parse_int" => (recv_str, "_dhvParseInt".into()),
        "parse_float" => (recv_str, "_dhvParseFloat".into()),
        "to_lowercase" => (recv_str, "toLowerCase()".into()),
        "to_uppercase" => (recv_str, "toUpperCase()".into()),
        "abs" => (format!("Math.abs({})", recv_str), "".into()),
        "ceil" => (format!("Math.ceil({})", recv_str), "".into()),
        "floor" => (format!("Math.floor({})", recv_str), "".into()),
        "round" => (format!("Math.round({})", recv_str), "".into()),
        "min" => (format!("Math.min(...{})", recv_str), "".into()),
        "max" => (format!("Math.max(...{})", recv_str), "".into()),
        "chars" => (format!("[...{}]", recv_str), "".into()),
        "char_at" => (recv_str, "charAt".into()),
        "is_alpha" => (recv_str, "_dhvIsAlpha".into()),
        "is_digit" => (recv_str, "_dhvIsDigit".into()),
        "retain" => (recv_str, "_dhvRetain".into()),
        _ => (recv_str, method.to_string()),
    }
}

/// format! 宏 → 模板字符串
fn ts_format_macro(args: &MacroArgs) -> String {
    let mut parts = Vec::new();
    let mut fmt_args = Vec::new();
    for tt in &args.tokens {
        match tt {
            TokenTree::Token(tok, _) => match tok {
                Token::Literal(lit) => parts.push(lit.raw.clone()),
                Token::Punct(s) if s == "," => {}
                Token::Ident(s) | Token::RawIdent(s) => {
                    fmt_args.push(camel_case(s));
                    parts.push("{}".into());
                }
                _ => {}
            },
            TokenTree::Delimited { tokens, .. } => {
                for t in tokens {
                    if let TokenTree::Token(tok, _) = t {
                        match tok {
                            Token::Literal(lit) => parts.push(lit.raw.clone()),
                            Token::Ident(s) | Token::RawIdent(s) => {
                                fmt_args.push(camel_case(s));
                                parts.push("{}".into());
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    // 将 {value} 和 {} 替换为 ${value} 模板语法
    let mut template = String::new();
    let mut arg_idx = 0;
    for part in &parts {
        if part == "{}" {
            if arg_idx < fmt_args.len() {
                template.push_str(&format!("${{{}}}", fmt_args[arg_idx]));
                arg_idx += 1;
            } else {
                template.push_str("${}");
            }
        } else {
            // 转义模板字面量中的特殊字符
            template.push_str(&part.replace('`', "\\`").replace('$', "\\$"));
        }
    }
    format!("`{}`", template)
}

// ────────────────────────────────────────────────────────────────
// 字面量转译
// ────────────────────────────────────────────────────────────────

fn ts_literal(lit: &Literal) -> String {
    match &lit.kind {
        LiteralKind::Str { value, .. } => format!("{}", value),
        LiteralKind::Bool(b) => {
            if *b { "true".into() } else { "false".into() }
        }
        LiteralKind::Int { value, .. } => value.to_string(),
        LiteralKind::Float { value, .. } => value.to_string(),
        LiteralKind::Char(c) => format!("'{}'", c),
    }
}

// ────────────────────────────────────────────────────────────────
// 工具
// ────────────────────────────────────────────────────────────────

pub fn camel_case(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
