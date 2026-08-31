//! # DHV — HSL (Harness Specification Language) 编译器
//!
//! 多后端源到源转译器：HSL → Rust / Python / TypeScript / YAML / Markdown / JSON
//!
//! 管线（对应路线图 P0-P11）：
//! ```text
//! HSL 源码 → Parser (pest PEG, P0/P2) → AST (P1)
//!         → Type Check (严格性 + 拓扑 + 投射校验)
//!         → Multi-Target Codegen (P3-P7) → Physical Writer + SourceMap
//! ```
//!
//! 权威文法：hsl-spec/BNF.md (v1.0)
//! 本骨架完成度：P0 ✅ P1 ✅ P2 ✅(核心) P3-P7 骨架 ✅ P6 双向工程 骨架

pub mod ast;
pub mod codegen;
pub mod diagnostics;
pub mod linker;
pub mod parser;
pub mod sourcemap;
pub mod langs;
pub mod typecheck;

pub use diagnostics::Diagnostics;

/// 一次完整的编译：parse → typecheck → codegen
pub struct CompileResult {
    pub ast: Option<ast::SourceFile>,
    pub diags: Diagnostics,
    pub files: Vec<codegen::GeneratedFile>,
    /// 依赖模块源码（file_hint → source，用于多文件诊断渲染）
    pub module_sources: Vec<(String, String)>,
}

pub fn compile(file_name: &str, src: &str) -> CompileResult {
    compile_ext(file_name, src, true)
}

/// 仅校验（check 命令）：check ≠ emit —— dhv-ts 参考实现的 check 同样不驱动代码生成，
/// codegen 能力缺口（未注册后端等）不应阻塞校验。
pub fn compile_check(file_name: &str, src: &str) -> CompileResult {
    compile_ext(file_name, src, false)
}

fn compile_ext(file_name: &str, src: &str, do_codegen: bool) -> CompileResult {
    let empty = || CompileResult { ast: None, diags: Diagnostics::new(), files: vec![], module_sources: vec![] };
    let mut all_diags = Diagnostics::new();

    // 1. Parse
    let parsed = parser::parse(0, src);
    let file = match parsed {
        Ok(f) => Some(f),
        Err(d) => {
            all_diags.extend(d.items);
            let mut r = empty(); r.diags = all_diags; return r;
        }
    };
    let file = file.unwrap();

    // 2. Link（最小模块链接器）：解析 import 相对路径，加载依赖模块闭包。
    //    模块导出的 enum / 静态资源进入跨模块注册表（S6 穷尽性 / P4 投射合法性需要）。
    let linked = linker::link(file_name, &file);
    for (importer, module, reason) in &linked.errors {
        all_diags.push(diagnostics::Diagnostic::error(
            diagnostics::DiagCode::NameResolution("M2"),
            format!("模块加载失败：`{module}`（被 `{importer}` 导入）：{reason}"),
            ast::Span::default(),
        ));
    }

    // 3. TypeCheck（严格性 + 拓扑 + 投射）
    let mut tc = typecheck::TypeChecker::new();
    for (mpath, mfile) in &linked.modules {
        tc.harvest_module(mpath, mfile);
    }
    // 依赖模块体级 S 系列检查（对齐 dhv-ts：先链接后逐文件检查）
    for (mpath, mfile) in &linked.modules {
        tc.check_m3_imports(mpath, mfile);
        tc.check_module_body(mpath, mfile);
    }
    tc.check_m3_imports(file_name, &file);
    tc.check_file(&file);
    all_diags.extend(tc.diags.items.clone());
    let module_sources: Vec<(String, String)> = linked.module_sources;
    if all_diags.has_errors() || !do_codegen {
        let mut r = empty(); r.ast = Some(file); r.diags = all_diags; r.module_sources = module_sources; return r;
    }

    // 4. Codegen：跨模块投射需要依赖模块的项（agent.hsl 投射 model.hsl 的 Prompt 等），
    //    构造「模块项在前 + 根文件项在后」的合并视图驱动 emit（project 块只取根文件的）。
    let mut merged_items: Vec<ast::TopLevel> = Vec::new();
    for (_mpath, mfile) in &linked.modules {
        merged_items.extend(mfile.items.iter().cloned());
    }
    merged_items.extend(file.items.iter().cloned());
    let merged = ast::SourceFile { items: merged_items, span: file.span.clone() };
    let scale = file
        .items
        .iter()
        .find_map(|t| match t {
            ast::TopLevel::Scale(s) => Some(codegen::Scale::from_mode(&s.mode)),
            _ => None,
        })
        .unwrap_or_default();
    let ctx = codegen::CodegenContext::new(file_name, scale, &merged);
    match ctx.emit(&merged) {
        Ok(files) => CompileResult { ast: Some(file), diags: all_diags, files, module_sources },
        Err(errors) => {
            for e in errors {
                all_diags.push(diagnostics::Diagnostic::error(
                    diagnostics::DiagCode::Codegen,
                    e,
                    ast::Span::default(),
                ));
            }
            let mut r = empty(); r.ast = Some(file); r.diags = all_diags; r.module_sources = module_sources; r
        }
    }
}
