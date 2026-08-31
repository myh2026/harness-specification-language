//! Go 后端（Logic 能力级）—— 类型映射 + 项转译骨架
//! Go 是 Tier 1 Harness 核心语言，本后端生成合法 Go 类型签名与函数框架。
//! struct → type X struct{}; enum → const iota + type X int / type XVariant struct{};
//! fn → func 签名 + 函数体骨架; graph → 入口 main 函数。

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
                    "// graph {} — scale: {:?}（节点=函数/Plugin，边=调用/事件订阅）\n",
                    g.name.name, ctx.scale
                ));
                out.push_str("func main() error {\n");
                out.push_str("    // TODO: AgentLoop 转译\n");
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
                        // 需要接收者参数 —— 从 impl.self_ty 提取
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
    // Unit struct: empty body
    out.push_str("}\n");
    out
}

/// enum → const iota + type X int (unit variants) 或 struct variants
fn go_enum(e: &EnumDef) -> String {
    let mut out = String::new();
    // 检查是否有带字段的变体
    let has_data = e.variants.iter().any(|v| {
        !matches!(&v.fields, StructKind::Unit)
    });
    if !has_data {
        // 纯 unit enum → const iota
        out.push_str(&format!("type {} int\n\n", export_name(&e.name.name)));
        out.push_str(&format!("const (\n"));
        for (i, v) in e.variants.iter().enumerate() {
            let name = export_name(&v.name.name);
            out.push_str(&format!("    {} {} = iota\n", name, name));
            if i == 0 {
                out.push_str("    // ...\n");
            }
        }
        out.push_str(")\n");
    } else {
        // 有数据变体 → interface + 各变体 struct
        let iface_name = export_name(&e.name.name);
        out.push_str(&format!("type {} interface {{\n", iface_name));
        out.push_str(&format!("    is{}()\n", iface_name));
        out.push_str("}\n\n");
        for v in &e.variants {
            let vname = export_name(&v.name.name);
            match &v.fields {
                StructKind::Unit => {
                    out.push_str(&format!(
                        "type {} struct {{}}\n", vname
                    ));
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
                "Vec" => format!("[]{}", p.generic_args.iter().next().map(go_generic).unwrap_or_else(|| "any".to_string())),
                "HashMap" => "map[string]any".to_string(),
                "HashSet" => "map[any]bool".to_string(),
                "Option" => format!("*{}", p.generic_args.iter().next().map(go_generic).unwrap_or_else(|| "any".to_string())),
                "Result" => "(any, error)".to_string(),
                "Box" => p.generic_args.iter().next().map(go_generic).unwrap_or_else(|| "any".to_string()),
                other => other.to_string(),
            };
            if p.generic_args.is_empty() {
                base
            } else {
                format!("{}[{}]", base, p.generic_args.iter().map(go_generic).collect::<Vec<_>>().join(", "))
            }
        }
        TypeKind::Ref { inner, mutable } => {
            if *mutable {
                go_type(inner) // Go 指针隐式可变
            } else {
                go_type(inner) // Go GC，引用语义与 Rust 不同
            }
        }
        TypeKind::Tuple(elems) => {
            // Go 没有元组 → struct N
            format!("struct{{ {} }}", elems.iter().enumerate().map(|(i, t)| format!("F{} {}", i, go_type(t))).collect::<Vec<_>>().join("; "))
        }
        TypeKind::Array { elem, .. } | TypeKind::Slice(elem) => {
            format!("[]{}", go_type(elem))
        }
        TypeKind::FnPtr { params, ret } => {
            let params_go: Vec<String> = params.iter().map(go_type).collect();
            let ret_go = ret.as_ref().map(|t| go_type(t)).unwrap_or_else(|| "".into());
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
        ParamKind::Self_(_) => None, // Go 方法接收者由 impl 处理
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

fn emit_block_go(block: &BlockExpr, indent: usize) -> String {
    let pad = "    ".repeat(indent);
    let mut out = String::new();
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let(l) => {
                let name = match &l.pattern.kind {
                    PatternKind::Ident { name, .. } => export_name(&name.name),
                    _ => "bound".into(),
                };
                let type_ann = l.ty.as_ref().map(|t| format!(" {}", go_type(t)));
                match &l.init {
                    Some(init) => out.push_str(&format!("{pad}{} := {}\n", name, emit_expr_go(init))),
                    None => {
                        if let Some(ty) = type_ann {
                            out.push_str(&format!("{pad}var {} {}\n", name, ty));
                        } else {
                            out.push_str(&format!("{pad}var {} interface{{}}\n", name));
                        }
                    }
                }
            }
            Stmt::Expr { expr, .. } => out.push_str(&format!("{pad}{}\n", emit_expr_go(expr))),
            Stmt::Empty(_) => {}
            Stmt::Item(_) => out.push_str(&format!("{pad}// 局部项\n")),
        }
    }
    if let Some(tail) = &block.tail {
        out.push_str(&format!("{pad}return {}\n", emit_expr_go(tail)));
    }
    if out.is_empty() {
        // Go 不允许空函数体
        out.push_str(&format!("{pad}// TODO\n"));
    }
    out
}

fn emit_expr_go(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(lit) => lit.raw.clone(),
        ExprKind::Path(p) => export_name(&p.last().name),
        ExprKind::Binary { op, lhs, rhs } => {
            let op_go = match op.as_str() {
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
                other => other,
            };
            format!("{} {} {}", emit_expr_go(lhs), op_go, emit_expr_go(rhs))
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
        ExprKind::Await(inner) => format!("<-{}", emit_expr_go(inner)),
        ExprKind::Cast { expr: _, ty } => format!("{}({})", go_type(ty), emit_expr_go(expr)),
        ExprKind::Native(nb) => nb.code.trim().to_string(),
        _ => "/* TODO */".to_string(),
    }
}
