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
}

pub fn compile(file_name: &str, src: &str) -> CompileResult {
    let mut all_diags = Diagnostics::new();

    // 1. Parse
    let parsed = parser::parse(0, src);
    let file = match parsed {
        Ok(f) => Some(f),
        Err(d) => {
            all_diags.extend(d.items);
            return CompileResult { ast: None, diags: all_diags, files: vec![] };
        }
    };
    let file = file.unwrap();

    // 2. TypeCheck（严格性 + 拓扑 + 投射）
    let mut tc = typecheck::TypeChecker::new();
    tc.check_file(&file);
    all_diags.extend(tc.diags.items.clone());
    if all_diags.has_errors() {
        return CompileResult { ast: Some(file), diags: all_diags, files: vec![] };
    }

    // 3. Codegen
    let scale = file
        .items
        .iter()
        .find_map(|t| match t {
            ast::TopLevel::Scale(s) => Some(codegen::Scale::from_mode(&s.mode)),
            _ => None,
        })
        .unwrap_or_default();
    let ctx = codegen::CodegenContext::new(file_name, scale, &file);
    match ctx.emit(&file) {
        Ok(files) => CompileResult { ast: Some(file), diags: all_diags, files },
        Err(errors) => {
            for e in errors {
                all_diags.push(diagnostics::Diagnostic::error(
                    diagnostics::DiagCode::Codegen,
                    e,
                    ast::Span::default(),
                ));
            }
            CompileResult { ast: Some(file), diags: all_diags, files: vec![] }
        }
    }
}
