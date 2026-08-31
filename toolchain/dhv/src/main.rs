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
    /// 解析 .hsl 文件并输出 AST 摘要
    Parse { file: PathBuf },
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
            Commands::Parse { file } => cmd_parse(file),
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

fn cmd_parse(path: &Path) -> Result<(), String> {
    let src = read_source(path)?;
    let name = path.display().to_string();
    match dhv::parser::parse(0, &src) {
        Ok(ast) => {
            println!("✓ 解析成功: {name}");
            println!("{}", summarize(&ast));
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
        eprint!("{}", result.diags.render_all(&src, &name));
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
