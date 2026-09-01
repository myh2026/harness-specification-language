// Ruby backend (Logic tier) -- type mapping + full function body translation
// Ruby 3.x code generation.
// struct -> class with attr_accessor (named) / Struct.new() (tuple/unit);
// enum (unit) -> module with constants;
// enum (data) -> class hierarchy with named fields;
// trait -> module with instance methods (duck typing);
// impl -> class/include module;
// fn -> top-level method (def ... end);
// const -> constant (UpperCamelCase);
// graph -> top-level execution block

use crate::ast::*;
use crate::codegen::{CodegenBackend, CodegenContext};

pub struct RubyBackend;

impl CodegenBackend for RubyBackend {
    fn lang(&self) -> &'static str {
        "ruby"
    }

    fn emit_item(&self, ctx: &CodegenContext, item: &Item) -> Result<String, String> {
        let mut out = String::new();
        out.push_str(&format!(
            "# {}\n",
            crate::sourcemap::generated_header("ruby")
        ));
        out.push_str("# HSL-generated Ruby code -- do not edit manually\n\n");
        out.push_str("require 'set'\n\n");

        match item {
            Item::Struct(s) => out.push_str(&rb_struct(s)),
            Item::Enum(e) => out.push_str(&rb_enum(e)),
            Item::Trait(t) => out.push_str(&rb_trait(t)),
            Item::Fn(f) => out.push_str(&rb_fn(f)),
            Item::Graph(g) => out.push_str(&rb_graph(g, ctx)),
            Item::Impl(imp) => out.push_str(&rb_impl(imp)),
            Item::Const(c) => out.push_str(&rb_const(c)),
            Item::TypeAlias(a) => out.push_str(&rb_typealias(a)),
            Item::MacroRules(m) => out.push_str(&rb_macro_rules(m)),
            _ => {
                return Err(format!(
                    "ruby backend does not support {}",
                    item_kind_name(item)
                ))
            }
        }
        Ok(out)
    }
}

// ──────────────────────────────────────────────────────
// Ruby keyword avoidance table
// ──────────────────────────────────────────────────────

const RB_KW: &[&str] = &[
    "__ENCODING__", "__FILE__", "__LINE__", "__END__", "alias", "and",
    "begin", "break", "case", "class", "def", "defined?", "do", "else",
    "elsif", "end", "ensure", "false", "for", "if", "in", "module",
    "next", "nil", "not", "or", "redo", "rescue", "retry", "return",
    "self", "super", "then", "true", "undef", "unless", "until", "when",
    "while", "yield",
    "require", "require_relative", "include", "extend", "attr_reader",
    "attr_writer", "attr_accessor", "attr", "private", "protected", "public",
    "new", "initialize", "freeze", "nil?", "is_a?", "kind_of?",
    "respond_to?", "to_s", "to_i", "to_f", "to_a", "to_h", "inspect",
    "puts", "print", "p", "raise", "fail", "throw", "catch",
    "lambda", "proc", "loop", "tap", "send", "class_eval",
    "instance_eval", "instance_variable_get", "instance_variable_set",
    "Array", "Hash", "Set", "String", "Integer", "Float", "Boolean",
    "Numeric", "Object", "Module", "Class", "Method", "Proc",
    "Range", "Regexp", "Symbol", "Struct", "Enumerable", "Comparable",
    "Exception", "StandardError", "RuntimeError", "ArgumentError",
    "TypeError", "NameError", "NoMethodError", "KeyError", "IndexError",
    "StopIteration", "IO", "File", "Dir", "Pathname",
    "match", "guard", "node", "edge", "graph", "loop", "block",
    "static", "import", "export", "project",
];

fn rb_ident(s: &str) -> String {
    if RB_KW.contains(&s) {
        format!("rb_{}", s)
    } else {
        s.to_string()
    }
}

/// Convert snake_case to CamelCase for class/module/constant names
fn rb_class_name(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;
    for c in s.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    if let Some(first) = result.chars().next() {
        if !first.is_ascii_uppercase() {
            result = format!("R{}", result);
        }
    }
    result
}

/// Extract class name string from a Type (for impl blocks etc.)
fn rb_type_name(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(pt) => rb_class_name(&pt.path.last().name),
        _ => "Object".into(),
    }
}

// ──────────────────────────────────────────────────────
// Type mapping
// ──────────────────────────────────────────────────────

fn rb_type(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(pt) => {
            let name = pt.path.last().name.clone();
            match name.as_str() {
                "String" | "str" | "char" => "String".into(),
                "bool" => "Boolean".into(),
                "i8" | "i16" | "i32" | "i64" | "isize"
                | "u8" | "u16" | "u32" | "u64" | "usize" => "Integer".into(),
                "f32" | "f64" => "Float".into(),
                "Vec" => "Array".into(),
                "HashMap" | "BTreeMap" => "Hash".into(),
                "HashSet" | "BTreeSet" => "Set".into(),
                "Option" | "Result" => "Object".into(),
                "Box" => "Object".into(),
                other => rb_ident(other),
            }
        }
        TypeKind::Tuple(_) => "Array".into(),
        TypeKind::Ref { inner, .. } => rb_type(inner),
        TypeKind::Array { elem, .. } => format!("Array<{}>", rb_type(elem)),
        TypeKind::Slice(inner) => format!("Array<{}>", rb_type(inner)),
        TypeKind::Never => "nil".into(),
        TypeKind::Paren(inner) => rb_type(inner),
        TypeKind::FnPtr { .. } | TypeKind::DynTrait(_) | TypeKind::ImplTrait(_) | TypeKind::Infer => "Object".into(),
    }
}

// ──────────────────────────────────────────────────────
// Param helper
// ──────────────────────────────────────────────────────

fn rb_param_name(p: &Param) -> Option<String> {
    match &p.kind {
        ParamKind::Self_(_) => None,
        ParamKind::Pattern(pat) => match &pat.kind {
            PatternKind::Ident { name, .. } => Some(rb_ident(&name.name)),
            _ => Some("_arg".into()),
        },
    }
}

// ──────────────────────────────────────────────────────
// Item emitters
// ──────────────────────────────────────────────────────

fn rb_struct(s: &StructDef) -> String {
    let name = rb_class_name(&s.name.name);
    let mut out = String::new();

    match &s.kind {
        StructKind::Named(fields) => {
            out.push_str(&format!("class {}\n", name));
            out.push_str("  # HSL-generated struct\n");
            for f in fields {
                let fname = f.name.as_ref().map(|n| rb_ident(&n.name)).unwrap_or_else(|| "_".into());
                out.push_str(&format!("  attr_accessor :{}\n", fname));
            }
            let params: Vec<String> = fields.iter()
                .map(|f| f.name.as_ref().map(|n| rb_ident(&n.name)).unwrap_or_else(|| "_".into()))
                .collect();
            out.push_str(&format!("  def initialize({})\n", params.join(", ")));
            for f in fields {
                let fname = f.name.as_ref().map(|n| rb_ident(&n.name)).unwrap_or_else(|| "_".into());
                out.push_str(&format!("    @{} = {}\n", fname, fname));
            }
            out.push_str("  end\n");
            out.push_str("end\n");
        }
        StructKind::Tuple(fields) => {
            let field_names: Vec<String> = fields.iter()
                .enumerate()
                .map(|(i, _)| format!("field_{}", i))
                .collect();
            out.push_str(&format!(
                "{} = Struct.new({})\n",
                name,
                field_names.iter().map(|f| format!(":{}", f)).collect::<Vec<_>>().join(", ")
            ));
        }
        StructKind::Unit => {
            out.push_str(&format!("{} = Struct.new(:__hsl_unit__)\n", name));
        }
    }
    out
}

fn rb_enum(e: &EnumDef) -> String {
    let name = rb_class_name(&e.name.name);
    let mut out = String::new();

    let has_data = e.variants.iter().any(|v| !matches!(&v.fields, StructKind::Unit));

    if has_data {
        out.push_str(&format!("class {}\n", name));
        out.push_str("  # HSL-generated enum base\n");
        out.push_str("end\n\n");
        for v in &e.variants {
            let vname = rb_class_name(&v.name.name);
            out.push_str(&format!("class {} < {}\n", vname, name));
            match &v.fields {
                StructKind::Named(fields) => {
                    let fnames: Vec<String> = fields.iter()
                        .map(|f| f.name.as_ref().map(|n| rb_ident(&n.name)).unwrap_or_else(|| "_".into()))
                        .collect();
                    for fname in &fnames {
                        out.push_str(&format!("  attr_reader :{}\n", fname));
                    }
                    out.push_str(&format!("  def initialize({})\n", fnames.join(", ")));
                    for fname in &fnames {
                        out.push_str(&format!("    @{} = {}\n", fname, fname));
                    }
                    out.push_str("  end\n");
                }
                StructKind::Tuple(fields) => {
                    let fnames: Vec<String> = fields.iter()
                        .enumerate().map(|(i, _)| format!("field_{}", i)).collect();
                    for fname in &fnames {
                        out.push_str(&format!("  attr_reader :{}\n", fname));
                    }
                    out.push_str(&format!("  def initialize({})\n", fnames.join(", ")));
                    for fname in &fnames {
                        out.push_str(&format!("    @{} = {}\n", fname, fname));
                    }
                    out.push_str("  end\n");
                }
                StructKind::Unit => {
                    out.push_str("  # Unit variant\n");
                }
            }
            out.push_str("end\n\n");
        }
    } else {
        out.push_str(&format!("module {}\n", name));
        for v in &e.variants {
            out.push_str(&format!("  {} = :{}\n", rb_class_name(&v.name.name), v.name.name));
        }
        out.push_str("end\n");
    }
    out
}

fn rb_trait(t: &TraitDef) -> String {
    let name = rb_class_name(&t.name.name);
    let mut out = String::new();
    out.push_str(&format!("module {}\n", name));
    out.push_str("  # HSL-generated trait (Ruby mixin module)\n");
    for item in &t.items {
        match item {
            TraitItem::Fn(f) => {
                let fname = rb_ident(&f.name.name);
                let params: Vec<String> = f.params.iter().filter_map(rb_param_name).collect();
                out.push_str(&format!("  def {}({})\n", fname, params.join(", ")));
                out.push_str("    raise NotImplementedError, \"\\#{self.class}\\#{__method__} must be implemented\"\n");
                out.push_str("  end\n\n");
            }
            TraitItem::FnSig(sig) => {
                let fname = rb_ident(&sig.name.name);
                let params: Vec<String> = sig.params.iter().filter_map(rb_param_name).collect();
                out.push_str(&format!("  def {}({})\n", fname, params.join(", ")));
                out.push_str("    raise NotImplementedError, \"\\#{self.class}\\#{__method__} must be implemented\"\n");
                out.push_str("  end\n\n");
            }
            TraitItem::Const(_) | TraitItem::TypeAlias(_) => {}
        }
    }
    out.push_str("end\n");
    out
}

fn rb_fn(f: &FnDef) -> String {
    let name = rb_ident(&f.name.name);
    let mut out = String::new();

    if f.is_async {
        out.push_str("# async def (Ruby: use threads or async gem)\n");
    }

    let params: Vec<String> = f.params.iter()
        .filter_map(|p| rb_param_name(p))
        .collect();

    out.push_str(&format!("def {}({})\n", name, params.join(", ")));
    if let Some(body) = &f.body {
        out.push_str(&emit_block(body, 1));
    }
    out.push_str("end\n");
    out
}

fn rb_graph(g: &GraphDef, _ctx: &CodegenContext) -> String {
    let mut out = String::new();
    out.push_str("# HSL graph entry point\n");
    out.push_str("def main_graph\n");
    for gs in &g.body {
        match gs {
            GraphStmt::Node(n) => {
                out.push_str(&format!("  # node {}: {}\n", rb_ident(&n.name.name), rb_type(&n.ty)));
            }
            GraphStmt::Edge(e) => {
                let ep: Vec<String> = e.endpoints.iter().map(|p| p.last().name.clone()).collect();
                out.push_str(&format!("  # edge: {}\n", ep.join(" -> ")));
            }
            GraphStmt::Let(l) => {
                let pat = rb_pattern(&l.pattern);
                match &l.init {
                    Some(init) => out.push_str(&format!("  {} = {}\n", pat, rb_expr(init))),
                    None => out.push_str(&format!("  {} = nil\n", pat)),
                }
            }
            GraphStmt::Stmt(s) => out.push_str(&emit_graph_stmt(s, "  ")),
            GraphStmt::Item(_) => {}
        }
    }
    out.push_str("end\n");
    out
}

fn rb_impl(imp: &ImplDef) -> String {
    let mut out = String::new();
    let target_name = rb_class_name(&rb_type_name(&imp.self_ty));
    let trait_name = imp.trait_ty.as_ref().map(|t| rb_class_name(&rb_type_name(t)));

    out.push_str(&format!("class {}\n", target_name));
    if let Some(tname) = &trait_name {
        out.push_str(&format!("  include {}\n", tname));
    }
    for item in &imp.items {
        match item {
            ImplItem::Fn(f) => {
                let fname = rb_ident(&f.name.name);
                let params: Vec<String> = f.params.iter().filter_map(rb_param_name).collect();
                out.push_str(&format!("\n  def {}({})\n", fname, params.join(", ")));
                if let Some(body) = &f.body {
                    out.push_str(&emit_block(body, 2));
                }
                out.push_str("  end\n");
            }
            ImplItem::Const(c) => {
                out.push_str(&format!(
                    "  {} = {}\n",
                    rb_class_name(&c.name.name),
                    rb_expr(&c.value)
                ));
            }
            ImplItem::TypeAlias(_) => {}
        }
    }
    out.push_str("end\n");
    out
}

fn rb_const(c: &ConstDef) -> String {
    let name = rb_class_name(&c.name.name);
    format!("{} = {}\n", name, rb_expr(&c.value))
}

fn rb_typealias(a: &TypeAliasDef) -> String {
    format!("# type alias {} = {}\n", rb_class_name(&a.name.name), rb_type(&a.ty))
}

fn rb_macro_rules(m: &MacroRulesDefinition) -> String {
    format!("# macro {} (Ruby does not support macros)\n", rb_ident(&m.name.name))
}

// ──────────────────────────────────────────────────────
// Block / statement emitters
// ──────────────────────────────────────────────────────

fn emit_block(block: &BlockExpr, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let mut out = String::new();
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let(l) => {
                let pat = rb_pattern(&l.pattern);
                match &l.init {
                    Some(init) => out.push_str(&format!("{}{} = {}\n", pad, pat, rb_expr(init))),
                    None => out.push_str(&format!("{}{} = nil\n", pad, pat)),
                }
                if let Some(els) = &l.else_block {
                    out.push_str(&format!("{}else\n", pad));
                    out.push_str(&emit_block(els, indent + 1));
                }
            }
            Stmt::Expr { expr, .. } => {
                out.push_str(&emit_stmt_expr(expr, &pad));
            }
            Stmt::Empty(_) => {}
            Stmt::Item(_) => out.push_str(&format!("{}# local item\n", pad)),
        }
    }
    if let Some(tail) = &block.tail {
        out.push_str(&format!("{}{}\n", pad, rb_expr(tail)));
    }
    out
}

/// Emit a Stmt (for graph body)
fn emit_graph_stmt(stmt: &Stmt, pad: &str) -> String {
    match stmt {
        Stmt::Let(l) => {
            let pat = rb_pattern(&l.pattern);
            match &l.init {
                Some(init) => format!("{}{} = {}\n", pad, pat, rb_expr(init)),
                None => format!("{}{} = nil\n", pad, pat),
            }
        }
        Stmt::Item(_) | Stmt::Empty(_) => String::new(),
        Stmt::Expr { expr, .. } => emit_stmt_expr(expr, pad),
    }
}

/// Emit an expression as a statement (with newline)
fn emit_stmt_expr(expr: &Expr, pad: &str) -> String {
    match &expr.kind {
        ExprKind::If { .. } | ExprKind::Match { .. } | ExprKind::For { .. }
        | ExprKind::While { .. } | ExprKind::Loop { .. } | ExprKind::WhileLet { .. }
        | ExprKind::Try(..) | ExprKind::Block(..) => {
            // Multi-line expressions: emit inline without extra wrapping
            format!("{}\n", rb_expr(expr))
        }
        _ => {
            format!("{}{}\n", pad, rb_expr(expr))
        }
    }
}

/// Convert an Expr into an indented block string (for match arms, else branches)
fn rb_expr_as_block(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Block(b) => emit_block(b, 3),
        _ => {
            let block = BlockExpr { stmts: vec![], tail: Some(Box::new(expr.clone())), span: expr.span };
            emit_block(&block, 3)
        }
    }
}

// ──────────────────────────────────────────────────────
// Expression translation
// ──────────────────────────────────────────────────────

fn rb_expr(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Literal(lit) => rb_literal(lit),

        ExprKind::Path(p) => rb_ident(&p.last().name),

        ExprKind::Binary { op, lhs, rhs } => {
            let l = rb_expr(lhs);
            let r = rb_expr(rhs);
            match op.as_str() {
                "&&" => format!("(({}) && ({}))", l, r),
                "||" => format!("(({}) || ({}))", l, r),
                "==" => format!("({}) == ({})", l, r),
                "!=" => format!("({}) != ({})", l, r),
                "+" => format!("({}) + ({})", l, r),
                "-" => format!("({}) - ({})", l, r),
                "*" => format!("({}) * ({})", l, r),
                "/" => format!("({}).to_f / ({})", l, r),
                "%" => format!("({}) % ({})", l, r),
                "<<" => format!("({}) << ({})", l, r),
                ">>" => format!("({}) >> ({})", l, r),
                "&" => format!("({}) & ({})", l, r),
                "|" => format!("({}) | ({})", l, r),
                "^" => format!("({}) ^ ({})", l, r),
                other => format!("({}) {} ({})", l, other, r),
            }
        }

        ExprKind::Unary { op, operand } => {
            let inner = rb_expr(operand);
            match op.as_str() {
                "!" => format!("!({})", inner),
                "-" => format!("-({})", inner),
                "*" | "&" => inner,
                other => format!("{}({})", other, inner),
            }
        }

        ExprKind::Call { callee, args } => {
            let fname = rb_expr(callee);
            let args_s: Vec<String> = args.iter().map(rb_expr).collect();
            if args_s.is_empty() {
                format!("{}()", fname)
            } else {
                format!("{}({})", fname, args_s.join(", "))
            }
        }

        ExprKind::MethodCall { receiver, method, generic_args: _, args } => {
            let obj = rb_expr(receiver);
            let mname = rb_ident(&method.name);
            let args_s: Vec<String> = args.iter().map(rb_expr).collect();
            // Check for standard library method mappings
            if let Some(mapped) = rb_std_method(&obj, &method.name, &args_s) {
                return mapped;
            }
            if args_s.is_empty() {
                format!("{}.{}", obj, mname)
            } else {
                format!("{}.{}({})", obj, mname, args_s.join(", "))
            }
        }

        ExprKind::Field { base, field } => {
            let obj = rb_expr(base);
            match field {
                FieldIndex::Named(id) => format!("{}.{}", obj, rb_ident(&id.name)),
                FieldIndex::Index(idx, _) => format!("{}[{}]", obj, idx),
            }
        }

        ExprKind::Index { base, index } => {
            format!("{}[{}]", rb_expr(base), rb_expr(index))
        }

        ExprKind::Slice { base, range } => {
            let obj = rb_expr(base);
            match (&range.lo, &range.hi) {
                (Some(l), Some(h)) => format!("{}[{}..{}]", obj, rb_expr(l), rb_expr(h)),
                (Some(l), None) => format!("{}[{}..]", obj, rb_expr(l)),
                (None, Some(h)) => format!("{}[0..{}]", obj, rb_expr(h)),
                (None, None) => format!("{}[]", obj),
            }
        }

        ExprKind::Range(range) => {
            let l_s = range.lo.as_ref().map(|e| rb_expr(e)).unwrap_or_default();
            let h_s = range.hi.as_ref().map(|e| rb_expr(e)).unwrap_or_default();
            if range.inclusive {
                format!("{}..{}", l_s, h_s)
            } else {
                format!("{}...{}", l_s, h_s)
            }
        }

        ExprKind::Assign { lhs, rhs } => {
            format!("{} = {}", rb_expr(lhs), rb_expr(rhs))
        }

        ExprKind::CompoundAssign { op, lhs, rhs } => {
            let tgt = rb_expr(lhs);
            let val = rb_expr(rhs);
            match op.as_str() {
                "+=" => format!("{} += {}", tgt, val),
                "-=" => format!("{} -= {}", tgt, val),
                "*=" => format!("{} *= {}", tgt, val),
                "/=" => format!("{} /= {}", tgt, val),
                "%=" => format!("{} %= {}", tgt, val),
                "&=" => format!("{} &= {}", tgt, val),
                "|=" => format!("{} |= {}", tgt, val),
                "^=" => format!("{} ^= {}", tgt, val),
                "<<=" => format!("{} <<= {}", tgt, val),
                ">>=" => format!("{} >>= {}", tgt, val),
                other => format!("{} = ({} {} {})", tgt, tgt, other, val),
            }
        }

        ExprKind::If { cond, then, else_ } => {
            let mut out = format!("if {}\n", rb_expr(cond));
            out.push_str(&emit_block(then, 1));
            if let Some(els) = else_ {
                out.push_str("else\n");
                out.push_str(&rb_expr_as_block(els));
            }
            out.push_str("end");
            out
        }

        ExprKind::Match { scrutinee, arms } => {
            let scrut = rb_expr(scrutinee);
            let mut out = format!("case {}\n", scrut);
            for arm in arms {
                let pat = rb_pattern(&arm.pattern);
                let guard = if let Some(g) = &arm.guard {
                    format!(" if {}", rb_expr(g))
                } else {
                    String::new()
                };
                out.push_str(&format!("  when {}{}\n", pat, guard));
                out.push_str(&rb_expr_as_block(&arm.body));
            }
            out.push_str("end");
            out
        }

        ExprKind::For { label: _, pattern, iter, body } => {
            let pat = rb_pattern(pattern);
            let iter_s = rb_expr(iter);
            let mut out = format!("{}.each do |{}|\n", iter_s, pat);
            out.push_str(&emit_block(body, 1));
            out.push_str("end");
            out
        }

        ExprKind::While { cond, body, .. } => {
            let mut out = format!("while {}\n", rb_expr(cond));
            out.push_str(&emit_block(body, 1));
            out.push_str("end");
            out
        }

        ExprKind::WhileLet { label: _, pattern, expr, body } => {
            let pat = rb_pattern(pattern);
            let ex = rb_expr(expr);
            let mut out = format!("while (val = {})\n  case val\n  when {}\n", ex, pat);
            out.push_str(&emit_block(body, 3));
            out.push_str("  end\nend");
            out
        }

        ExprKind::Loop { body, .. } => {
            let mut out = String::from("loop do\n");
            out.push_str(&emit_block(body, 1));
            out.push_str("end");
            out
        }

        ExprKind::Closure { params, body, .. } => {
            let params_s: Vec<String> = params.iter().filter_map(rb_param_name).collect();
            format!("-> ({}) {{ {} }}", params_s.join(", "), rb_expr(body))
        }

        ExprKind::Return(v) => {
            match v {
                Some(val) => format!("return {}", rb_expr(val)),
                None => "return".into(),
            }
        }

        ExprKind::Break { value, .. } => {
            match value {
                Some(v) => format!("break {}", rb_expr(v)),
                None => "break".into(),
            }
        }

        ExprKind::Continue { .. } => "next".into(),

        ExprKind::Array(elems) => {
            let els: Vec<String> = elems.iter().map(rb_expr).collect();
            format!("[{}]", els.join(", "))
        }

        ExprKind::ArrayRepeat { elem, count } => {
            format!("Array.new({}, {})", rb_expr(count), rb_expr(elem))
        }

        ExprKind::Struct { path: _, fields, spread, .. } => {
            let mut pairs: Vec<String> = Vec::new();
            for f in fields {
                let fname = match &f.name {
                    FieldIndex::Named(id) => rb_ident(&id.name),
                    FieldIndex::Index(i, _) => format!("field_{}", i),
                };
                let val = f.value.as_ref().map(rb_expr).unwrap_or_else(|| fname.clone());
                pairs.push(format!("{}: {}", fname, val));
            }
            if let Some(spr) = spread {
                pairs.push(format!("**{}", rb_expr(spr)));
            }
            format!("{{ {} }}", pairs.join(", "))
        }

        ExprKind::Tuple(elems) => {
            let els: Vec<String> = elems.iter().map(rb_expr).collect();
            format!("[{}]", els.join(", "))
        }

        ExprKind::Block(b) => {
            let mut out = String::from("begin\n");
            out.push_str(&emit_block(b, 1));
            out.push_str("end");
            out
        }

        ExprKind::Try(inner) => {
            // Ruby: try/rescue wrapper
            format!("begin\n  {}\nrescue => e\n  # handle error\nend", rb_expr(inner))
        }

        ExprKind::AsyncBlock { body, .. } => {
            emit_block(body, 0)
        }

        ExprKind::Await(expr) => {
            format!("{} # await", rb_expr(expr))
        }

        ExprKind::Cast { expr, ty } => {
            let target = rb_type(ty);
            match target.as_str() {
                "Integer" => format!("Integer({})", rb_expr(expr)),
                "Float" => format!("Float({})", rb_expr(expr)),
                "String" => format!("String({})", rb_expr(expr)),
                "Boolean" => format!("(!{}).nil?", rb_expr(expr)),
                _ => format!("{}  # cast to {}", rb_expr(expr), target),
            }
        }

        ExprKind::Native(nb) => {
            nb.code.clone()
        }

        ExprKind::Macro { path, args } => {
            let mname = &path.last().name;
            // Extract string-like arguments for known macros
            let args_str: Vec<String> = args.tokens.iter().map(|tt| {
                match tt {
                    TokenTree::Token(Token::Literal(lit), _) => rb_literal(lit),
                    TokenTree::Token(Token::Ident(s), _) => rb_ident(s),
                    _ => String::new(),
                }
            }).collect();
            match mname.as_str() {
                "println" => {
                    if args_str.len() == 1 {
                        format!("puts {}", args_str[0])
                    } else {
                        format!("puts \"{}\"", args_str.join(" "))
                    }
                }
                "format" => {
                    if !args_str.is_empty() { args_str[0].clone() } else { String::new() }
                }
                "panic" | "todo" => {
                    let msg = if !args_str.is_empty() { args_str[0].clone() } else { String::from("\"panic\"") };
                    format!("raise RuntimeError, {}", msg)
                }
                "dbg" => {
                    if !args_str.is_empty() {
                        format!("p {} # dbg", args_str[0])
                    } else {
                        "p caller # dbg".into()
                    }
                }
                "eprintln" => {
                    if args_str.len() == 1 {
                        format!("$stderr.puts {}", args_str[0])
                    } else {
                        format!("$stderr.puts \"{}\"", args_str.join(" "))
                    }
                }
                _ => {
                    format!("{}({})", rb_ident(mname), args_str.join(", "))
                }
            }
        }

        ExprKind::IfLet { pattern, expr, then, else_ } => {
            let pat = rb_pattern(pattern);
            let ex = rb_expr(expr);
            let mut out = format!("case {}\nwhen {}\n", ex, pat);
            out.push_str(&emit_block(then, 1));
            if let Some(els) = else_ {
                out.push_str("else\n");
                out.push_str(&rb_expr_as_block(els));
            }
            out.push_str("end");
            out
        }
    }
}

// ──────────────────────────────────────────────────────
// Pattern translation
// ──────────────────────────────────────────────────────

fn rb_pattern(pat: &Pattern) -> String {
    match &pat.kind {
        PatternKind::Wildcard => "_".into(),
        PatternKind::Ident { name, .. } => rb_ident(&name.name),
        PatternKind::Literal(lit) => rb_literal(lit),
        PatternKind::Path(p) => rb_ident(&p.last().name),
        PatternKind::TupleStruct { path, elems, .. } => {
            let name = rb_class_name(&path.last().name);
            if elems.is_empty() {
                name
            } else {
                let args_s: Vec<String> = elems.iter().map(rb_pattern).collect();
                format!("{}", args_s.join(", "))
            }
        }
        PatternKind::Struct { path, fields, .. } => {
            let _name = rb_class_name(&path.last().name);
            let field_pats: Vec<String> = fields.iter().map(|f| {
                let fname = rb_ident(&f.name.name);
                match &f.pattern {
                    Some(pat) => format!("{}: {}", fname, rb_pattern(pat)),
                    None => fname,
                }
            }).collect();
            field_pats.join(", ")
        }
        PatternKind::Tuple { elems, .. } => {
            let els: Vec<String> = elems.iter().map(rb_pattern).collect();
            format!("[{}]", els.join(", "))
        }
        PatternKind::Or(alternatives) => {
            alternatives.iter().map(rb_pattern).collect::<Vec<_>>().join(", ")
        }
        PatternKind::Range { lo, hi, inclusive } => {
            let l = rb_pattern(lo);
            let h = rb_pattern(hi);
            if *inclusive { format!("{}..{}", l, h) } else { format!("{}...{}", l, h) }
        }
        PatternKind::Rest => "*".into(),
    }
}

// ──────────────────────────────────────────────────────
// Literal translation
// ──────────────────────────────────────────────────────

fn rb_literal(lit: &Literal) -> String {
    match &lit.kind {
        LiteralKind::Int { value, suffix: _ } => {
            value.to_string()
        }
        LiteralKind::Float { value, suffix: _ } => {
            value.to_string()
        }
        LiteralKind::Str { value, .. } => {
            format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
        }
        LiteralKind::Bool(b) => if *b { "true".into() } else { "false".into() },
        LiteralKind::Char(c) => format!("'{}'", c),
    }
}

// ──────────────────────────────────────────────────────
// Ruby std method mappings
// ──────────────────────────────────────────────────────

fn rb_std_method(obj: &str, method: &str, args: &[String]) -> Option<String> {
    let args_s = args.join(", ");
    match method {
        // Vec/Array
        "push" => Some(format!("{}.push({})", obj, args_s)),
        "pop" => Some(format!("{}.pop", obj)),
        "len" | "length" => Some(format!("{}.length", obj)),
        "is_empty" => Some(format!("{}.empty?", obj)),
        "sort" => Some(format!("{}.sort", obj)),
        "reverse" => Some(format!("{}.reverse", obj)),
        "map" => Some(format!("{}.map {{ |x| {} }}", obj, if args_s.is_empty() { "x".into() } else { args_s })),
        "filter" => Some(format!("{}.select {{ |x| {} }}", obj, if args_s.is_empty() { "x".into() } else { args_s })),
        "fold" | "fold_left" => Some(format!("{}.reduce({}) {{ |acc, x| {} }}", obj, if args.is_empty() { "0".into() } else { args[0].clone() }, if args.len() > 1 { args[1].clone() } else { "acc + x".into() })),
        "for_each" => Some(format!("{}.each {{ |x| {} }}", obj, if args_s.is_empty() { "x".into() } else { args_s })),
        "find" => Some(format!("{}.find {{ |x| {} }}", obj, if args_s.is_empty() { "x".into() } else { args_s })),
        "any" => Some(format!("{}.any? {{ |x| {} }}", obj, if args_s.is_empty() { "x".into() } else { args_s })),
        "all" => Some(format!("{}.all? {{ |x| {} }}", obj, if args_s.is_empty() { "x".into() } else { args_s })),
        "contains" => Some(format!("{}.include?({})", obj, args_s)),
        "flat_map" | "flatmap" => Some(format!("{}.flat_map {{ |x| {} }}", obj, if args_s.is_empty() { "x".into() } else { args_s })),
        "first" => Some(format!("{}.first", obj)),
        "last" => Some(format!("{}.last", obj)),
        "at" => Some(format!("{}[{}]", obj, args_s)),
        "remove" => Some(format!("{}.delete_at({})", obj, args_s)),
        "clear" => Some(format!("{}.clear", obj)),
        "join" => Some(format!("{}.join(\"{}\")", obj, if args.is_empty() { ", ".into() } else { args[0].replace('"', "") })),
        // String
        "trim" => Some(format!("{}.strip", obj)),
        "to_lowercase" => Some(format!("{}.downcase", obj)),
        "to_uppercase" => Some(format!("{}.upcase", obj)),
        "starts_with" => Some(format!("{}.start_with?(\"{}\")", obj, if args.is_empty() { String::new() } else { args[0].trim_matches('"').to_string() })),
        "ends_with" => Some(format!("{}.end_with?(\"{}\")", obj, if args.is_empty() { String::new() } else { args[0].trim_matches('"').to_string() })),
        "split" => Some(format!("{}.split(\"{}\")", obj, if args.is_empty() { " ".into() } else { args[0].trim_matches('"').to_string() })),
        "replace" => Some(format!("{}.gsub({}, {})", obj,
            if args.len() >= 2 { args[0].trim_matches('"').to_string() } else { String::new() },
            if args.len() >= 2 { args[1].trim_matches('"').to_string() } else { String::new() })),
        "to_string" | "to_str" => Some(format!("{}.to_s", obj)),
        "chars" => Some(format!("{}.chars", obj)),
        // Option
        "is_some" | "is_some?" => Some(format!("!{}.nil?", obj)),
        "is_none" | "is_none?" => Some(format!("{}.nil?", obj)),
        "unwrap" => Some(format!("{} # unwrap", obj)),
        "unwrap_or" => Some(format!("{} || {}", obj, args_s)),
        // HashMap
        "insert" => Some(format!("{}[{}] = {}", obj, if args.len() >= 1 { args[0].clone() } else { "key".into() }, if args.len() >= 2 { args[1].clone() } else { "value".into() })),
        "get" => Some(format!("{}[{}]", obj, args_s)),
        "keys" => Some(format!("{}.keys", obj)),
        "values" => Some(format!("{}.values", obj)),
        "delete" => Some(format!("{}.delete({})", obj, args_s)),
        "contains_key" => Some(format!("{}.key?({})", obj, args_s)),
        // General
        "clone" => Some(format!("{}.clone", obj)),
        "abs" => Some(format!("{}.abs", obj)),
        _ => None,
    }
}
