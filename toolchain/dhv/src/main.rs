//! DHV CLI — HSL 编译器命令行入口
//!
//! ```text
//! dhv parse <file.hsl>          解析并 dump AST 摘要
//! dhv check <file.hsl>          解析 + 类型/拓扑/投射校验
//! dhv emit <file.hsl> -o out/   生成工程仓库（Physical Writer）
//! dhv watch <dir>               File Watcher + 实时反编译（P6 骨架）
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use clap::{Parser as ClapParser, Subcommand};

#[derive(ClapParser)]
#[command(name = "dhv", version, about = "DHV — HSL (Harness Specification Language) 编译器")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 解析 .hsl 文件并输出 AST 摘要（--dump-values 值级对拍模式）
    Parse {
        file: PathBuf,
        /// v0.2.54 值级对拍：dump 全部整数字面量（raw/值/后缀）—— 供双编译器逐值比对
        #[arg(long)]
        dump_values: bool,
    },
    /// 解析 + 严格性/拓扑/投射校验
    Check { file: PathBuf },
    /// 生成工程仓库（按 project {} 投射写物理文件）
    Emit {
        file: PathBuf,
        /// 输出目录（默认 ./generated）
        #[arg(short, long, default_value = "generated")]
        out: PathBuf,
    },
    /// 监视工程目录（双向工程：P6 骨架）
    Watch { dir: PathBuf },
}

fn main() {
    let cli = Cli::parse();
    // pest 递归下降在 debug 构建下栈帧较大，深嵌套表达式会溢出默认主线程栈；
    // 在大栈工作线程中执行（release 同样受益于更深的程序）
    let worker = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || match &cli.command {
            Commands::Parse { file, dump_values } => cmd_parse(file, *dump_values),
            Commands::Check { file } => cmd_check(file),
            Commands::Emit { file, out } => cmd_emit(file, out),
            Commands::Watch { dir } => cmd_watch(dir),
        });
    let result = match worker {
        Ok(handle) => handle.join().unwrap_or_else(|p| {
            let msg = p.downcast_ref::<String>().cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "未知 panic".into());
            Err(format!("内部错误：{msg}"))
        }),
        Err(e) => Err(format!("无法启动工作线程：{e}")),
    };
    if let Err(e) = result {
        eprintln!("dhv: {e}");
        std::process::exit(1);
    }
}

fn read_source(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("无法读取 {}: {e}", path.display()))
}

fn cmd_parse(path: &Path, dump_values: bool) -> Result<(), String> {
    let src = read_source(path)?;
    let name = path.display().to_string();
    match dhv::parser::parse(0, &src) {
        Ok(ast) => {
            println!("✓ 解析成功: {name}");
            println!("{}", summarize(&ast));
            // v0.2.54 值级对拍（L-11 教训）：按文件内出现顺序 dump 所有整数字面量
            // —— 双编译器逐字面量比对值与域，parse 层静默损坏（归零/舍入）
            // 无处遁形。格式：`int\t<raw>\t<value>[u 后缀]`
            //
            // v0.2.56 三族扩展（L-12/L-14 教训：值损坏不只在整数）：
            // - float：IEEE754 位模式（{:016x}，双端 16 hex），NaN payload / 符号位全保真
            // - string：统一转义 repr（长度 + 可见化），unescape 层吞字/错值无处遁形
            if dump_values {
                for v in collect_literals(&ast) {
                    match v {
                        LitDump::Int { raw, value, suffix } => {
                            println!("int\t{}\t{}{}", escape_for_dump(&raw), value, suffix)
                        }
                        LitDump::Float { raw, bits, suffix } => {
                            println!("float\t{}\t{}\t{}", escape_for_dump(&raw), bits, suffix)
                        }
                        LitDump::Str { raw, value } => {
                            println!("string\t{}\t{}", escape_for_dump(&raw), value)
                        }
                    }
                }
            }
            Ok(())
        }
        Err(diags) => {
            eprint!("{}", diags.render_all(&src, &name));
            Err("解析失败".into())
        }
    }
}

fn cmd_check(path: &Path) -> Result<(), String> {
    let src = read_source(path)?;
    let name = path.display().to_string();
    let result = dhv::compile_check(&name, &src);
    if result.diags.has_errors() {
        // 构建文件名→源码映射（含依赖模块），用于多文件诊断渲染
        let mut sources: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
        sources.insert(name.clone(), src.as_str());
        for (mpath, msrc) in &result.module_sources {
            sources.insert(mpath.clone(), msrc.as_str());
        }
        let mut out = String::new();
        for d in &result.diags.items {
            let hint = if d.file_hint.is_empty() { &name } else { &d.file_hint };
            let render_src = sources.get(hint).map(|s| *s).unwrap_or("");
            out.push_str(&dhv::diagnostics::render(d, render_src, hint));
        }
        out.push_str(&format!(
            "error: aborting due to {} previous error(s)\n",
            result.diags.error_count()
        ));
        eprint!("{out}");
        Err("校验失败".into())
    } else {
        println!("✓ 校验通过: {name}");
        for d in &result.diags.items {
            println!("{}", dhv::diagnostics::render(d, &src, &name));
        }
        Ok(())
    }
}

fn cmd_emit(path: &Path, out: &Path) -> Result<(), String> {
    let src = read_source(path)?;
    let name = path.display().to_string();
    let result = dhv::compile(&name, &src);
    if result.diags.has_errors() {
        eprint!("{}", result.diags.render_all(&src, &name));
        return Err("编译失败，未生成任何文件".into());
    }
    if result.files.is_empty() {
        println!("（无 project 投射声明，未生成文件）");
        return Ok(());
    }
    fs::create_dir_all(out).map_err(|e| format!("创建输出目录失败: {e}"))?;
    for f in &result.files {
        let target = out.join(&f.path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
        }
        fs::write(&target, &f.content).map_err(|e| format!("写入 {} 失败: {e}", target.display()))?;
        println!("✓ {} -> {}", f.lang, target.display());
    }
    println!("\n完成: {} 个文件（已注入 @dhv:source-map 围栏，逻辑层可自由修改并实时回写）", result.files.len());
    Ok(())
}

fn cmd_watch(dir: &Path) -> Result<(), String> {
    // P6 骨架：notify 文件监听 + SourceMap 反编译回写
    println!("dhv watch: 监视 {}（P6 双向工程骨架 — File Watcher 将在 P6 完整版落地）", dir.display());
    println!("流程: 物理文件变化 → 提取 @dhv:source-map 围栏 → 逆向解析 → 回写 HSL → 重新编译+Lint");
    Ok(())
}

/// v0.2.54 值级对拍：按文件顺序收集所有字面量（含模式位/枚举判别式）。
/// source map 保序依赖 walk 顺序与 pest 解析顺序一致（语句序列）。
/// v0.2.56 三族扩展：int / float（IEEE754 位模式）/ string（统一转义 repr）。
enum LitDump {
    Int { raw: String, value: String, suffix: String },
    Float { raw: String, bits: String, suffix: String },
    Str { raw: String, value: String },
}

fn collect_literals(file: &dhv::ast::SourceFile) -> Vec<LitDump> {
    use dhv::ast::*;
    let mut out = Vec::new();
    let push = |l: &Literal, out: &mut Vec<LitDump>| {
        match &l.kind {
            LiteralKind::Int { value, suffix, overflow } => {
                let suffix_str = suffix.map(|s| match s {
                    IntSuffix::I8 => "u i8", IntSuffix::I16 => "u i16", IntSuffix::I32 => "u i32",
                    IntSuffix::I64 => "u i64", IntSuffix::I128 => "u i128", IntSuffix::Isize => "u isize",
                    IntSuffix::U8 => "u u8", IntSuffix::U16 => "u u16", IntSuffix::U32 => "u u32",
                    IntSuffix::U64 => "u u64", IntSuffix::U128 => "u u128", IntSuffix::Usize => "u usize",
                }).unwrap_or("");
                let value_str = if *overflow {
                    format!("OVERFLOW({})", l.raw)
                } else {
                    value.to_string()
                };
                out.push(LitDump::Int { raw: l.raw.clone(), value: value_str, suffix: suffix_str.to_string() });
            }
            LiteralKind::Float { value, suffix } => {
                // IEEE754 位模式：双端唯一可靠的浮点值等价判据（字符串格式化
                // 双端有差异：Rust "inf" vs JS "Infinity"、大数指数形态不同）
                let bits = format!("{:016x}", value.to_bits());
                let suffix_str = match suffix {
                    Some(FloatSuffix::F32) => "f32",
                    Some(FloatSuffix::F64) => "f64",
                    None => "",
                };
                out.push(LitDump::Float { raw: l.raw.clone(), bits, suffix: suffix_str.to_string() });
            }
            LiteralKind::Str { value, raw_string: _ } => {
                out.push(LitDump::Str { raw: l.raw.clone(), value: escape_for_dump(value) });
            }
            _ => {}
        }
    };
    for top in &file.items {
        if let TopLevel::Item(item) = top {
            walk_item_lits(item, &push, &mut out);
        }
    }
    out
}

/// v0.2.56：字符串值级 dump 的统一转义 repr（双端同规则）。
/// \\ \" \n \r \t \0 控制字符 \xNN（两位小写 hex）；其余字符原样（含 unicode）。
fn escape_for_dump(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn walk_item_lits(
    item: &dhv::ast::Item,
    push: &dyn Fn(&dhv::ast::Literal, &mut Vec<LitDump>),
    out: &mut Vec<LitDump>,
) {
    use dhv::ast::*;
    match item {
        Item::Fn(f) => {
            if let Some(body) = &f.body {
                walk_block_lits(body, push, out);
            }
        }
        Item::Const(c) => walk_expr_lits(&c.value, push, out),
        Item::Enum(e) => {
            for v in &e.variants {
                if let Some(d) = &v.discriminant {
                    push(d, out);
                }
            }
        }
        Item::Graph(g) => {
            for gs in &g.body {
                match gs {
                    GraphStmt::Node(n) => {
                        if let Some(init) = &n.init {
                            walk_expr_lits(init, push, out);
                        }
                    }
                    GraphStmt::Edge(e) => {
                        if let Some(guard) = &e.on {
                            match guard {
                                EdgeGuard::Pattern(p) => walk_pattern_lits(p, push, out),
                                EdgeGuard::Expr(ex) => walk_expr_lits(ex, push, out),
                            }
                        }
                        for attr in &e.attrs {
                            if let Some(v) = &attr.value {
                                push(v, out);
                            }
                        }
                    }
                    GraphStmt::Let(l) => walk_stmt_lits(&dhv::ast::Stmt::Let(l.clone()), push, out),
                    GraphStmt::Stmt(st) => walk_stmt_lits(st, push, out),
                    GraphStmt::Item(i) => walk_item_lits(i, push, out),
                }
            }
        }
        Item::Export(inner) => walk_item_lits(&inner.item, push, out),
        _ => {}
    }
}

fn walk_block_lits(
    b: &dhv::ast::BlockExpr,
    push: &dyn Fn(&dhv::ast::Literal, &mut Vec<LitDump>),
    out: &mut Vec<LitDump>,
) {
    for s in &b.stmts {
        walk_stmt_lits(s, push, out);
    }
    if let Some(t) = &b.tail {
        walk_expr_lits(t, push, out);
    }
}

fn walk_stmt_lits(
    s: &dhv::ast::Stmt,
    push: &dyn Fn(&dhv::ast::Literal, &mut Vec<LitDump>),
    out: &mut Vec<LitDump>,
) {
    match s {
        dhv::ast::Stmt::Let(l) => {
            walk_pattern_lits(&l.pattern, push, out);
            if let Some(init) = &l.init {
                walk_expr_lits(init, push, out);
            }
            if let Some(els) = &l.else_block {
                walk_block_lits(els, push, out);
            }
        }
        dhv::ast::Stmt::Item(i) => walk_item_lits(i, push, out),
        dhv::ast::Stmt::Expr { expr, .. } => walk_expr_lits(expr, push, out),
        dhv::ast::Stmt::Empty(_) => {}
    }
}

fn walk_pattern_lits(
    p: &dhv::ast::Pattern,
    push: &dyn Fn(&dhv::ast::Literal, &mut Vec<LitDump>),
    out: &mut Vec<LitDump>,
) {
    use dhv::ast::PatternKind::*;
    match &p.kind {
        Literal(l) => push(l, out),
        Tuple { elems, .. } | TupleStruct { elems, .. } => {
            for ip in elems {
                walk_pattern_lits(ip, push, out);
            }
        }
        Struct { fields, .. } => {
            for fp in fields {
                if let Some(pat) = &fp.pattern {
                    walk_pattern_lits(pat, push, out);
                }
            }
        }
        Or(alts) => {
            for ap in alts {
                walk_pattern_lits(ap, push, out);
            }
        }
        Range { lo, hi, .. } => {
            walk_pattern_lits(lo, push, out);
            walk_pattern_lits(hi, push, out);
        }
        Ident { sub: Some(sub), .. } => walk_pattern_lits(sub, push, out),
        _ => {}
    }
}

fn walk_expr_lits(
    e: &dhv::ast::Expr,
    push: &dyn Fn(&dhv::ast::Literal, &mut Vec<LitDump>),
    out: &mut Vec<LitDump>,
) {
    use dhv::ast::ExprKind::*;
    match &e.kind {
        Literal(l) => push(l, out),
        Binary { lhs, rhs, .. } => {
            walk_expr_lits(lhs, push, out);
            walk_expr_lits(rhs, push, out);
        }
        Unary { operand, .. } | Try(operand) | Await(operand) => walk_expr_lits(operand, push, out),
        Call { callee, args } => {
            walk_expr_lits(callee, push, out);
            for a in args {
                walk_expr_lits(a, push, out);
            }
        }
        MethodCall { receiver, args, .. } => {
            walk_expr_lits(receiver, push, out);
            for a in args {
                walk_expr_lits(a, push, out);
            }
        }
        Field { base, .. } => walk_expr_lits(base, push, out),
        Index { base, index, .. } => {
            walk_expr_lits(base, push, out);
            walk_expr_lits(index, push, out);
        }
        Slice { base, range, .. } => {
            walk_expr_lits(base, push, out);
            if let Some(lo) = &range.lo { walk_expr_lits(lo, push, out); }
            if let Some(hi) = &range.hi { walk_expr_lits(hi, push, out); }
        }
        Range(r) => {
            if let Some(lo) = &r.lo { walk_expr_lits(lo, push, out); }
            if let Some(hi) = &r.hi { walk_expr_lits(hi, push, out); }
        }
        Cast { expr, .. } => walk_expr_lits(expr, push, out),
        Assign { lhs, rhs } | CompoundAssign { lhs, rhs, .. } => {
            walk_expr_lits(lhs, push, out);
            walk_expr_lits(rhs, push, out);
        }
        Closure { body, .. } => walk_expr_lits(body, push, out),
        If { cond, then, else_, .. } => {
            walk_expr_lits(cond, push, out);
            walk_block_lits(then, push, out);
            if let Some(el) = else_ { walk_expr_lits(el, push, out); }
        }
        IfLet { pattern, expr, then, else_, .. } => {
            walk_pattern_lits(pattern, push, out);
            walk_expr_lits(expr, push, out);
            walk_block_lits(then, push, out);
            if let Some(el) = else_ { walk_expr_lits(el, push, out); }
        }
        Match { scrutinee, arms, .. } => {
            walk_expr_lits(scrutinee, push, out);
            for arm in arms {
                walk_pattern_lits(&arm.pattern, push, out);
                if let Some(g) = &arm.guard { walk_expr_lits(g, push, out); }
                walk_expr_lits(&arm.body, push, out);
            }
        }
        Block(b) | AsyncBlock { body: b, .. } => walk_block_lits(b, push, out),
        Loop { body, .. } | While { body, .. } | For { body, .. } => {
            walk_block_lits(body, push, out);
        }
        WhileLet { pattern, expr, body, .. } => {
            walk_pattern_lits(pattern, push, out);
            walk_expr_lits(expr, push, out);
            walk_block_lits(body, push, out);
        }
        For { pattern, iter, body, .. } => {
            walk_pattern_lits(pattern, push, out);
            walk_expr_lits(iter, push, out);
            walk_block_lits(body, push, out);
        }
        Break { value: Some(v), .. } | Return(Some(v)) => walk_expr_lits(v, push, out),
        Array(items) | Tuple(items) => {
            for i in items {
                walk_expr_lits(i, push, out);
            }
        }
        Struct { fields, .. } => {
            for f in fields {
                if let Some(v) = &f.value {
                    walk_expr_lits(v, push, out);
                } else if let dhv::ast::FieldIndex::Named(n) = &f.name {
                    let _ = n; // 简写字段无字面量
                }
            }
        }
        Macro { args, .. } => {
            walk_token_tree(&args.tokens, push, out);
        }
        _ => {}
    }
}

fn walk_token_tree(
    tts: &[dhv::ast::TokenTree],
    push: &dyn Fn(&dhv::ast::Literal, &mut Vec<LitDump>),
    out: &mut Vec<LitDump>,
) {
    for tt in tts {
        match tt {
            dhv::ast::TokenTree::Delimited { tokens, .. } => walk_token_tree(tokens, push, out),
            // Token 枚举里的 Literal 变体直接收集
            dhv::ast::TokenTree::Token(tok, _) => {
                if let dhv::ast::Token::Literal(l) = tok {
                    push(l, out);
                }
            }
        }
    }
}

fn summarize(file: &dhv::ast::SourceFile) -> String {
    use dhv::ast::*;
    let mut items = 0;
    let mut graphs = 0;
    let mut blocks = 0;
    let mut projections = 0;
    let mut scale = String::from("(默认 monolith)");
    for top in &file.items {
        match top {
            TopLevel::Item(Item::Graph(_)) => graphs += 1,
            TopLevel::Item(Item::StaticResource(_)) => blocks += 1,
            TopLevel::Item(_) => items += 1,
            TopLevel::Project(p) => projections += p.projections.len(),
            TopLevel::Scale(s) => scale = s.mode.to_string(),
        }
    }
    format!(
        "AST 摘要: {items} 项, {graphs} graph, {blocks} 静态资源, {projections} 投射, scale = {scale}"
    )
}
