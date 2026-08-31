//! # DHV 一致性 / 回归测试套件（fixture 驱动）
//!
//! 目录布局（`tests/fixtures/`）：
//! - `parse/*.hsl`     —— 必须成功解析（文法层回归）
//! - `check/*.hsl`     —— 必须通过 check（语义层回归：S/G/P/M 全绿）
//! - `errors/*.hsl`    —— 必须 check 失败，且文件名首段为期望的诊断代码
//!   （如 `S7_xxx.hsl` 期望渲染输出含 `S7`，`E0001_xxx.hsl` 期望语法错误）
//! - `modules/pass_*`  —— 多模块工程（root.hsl + 依赖模块），check 必须通过
//! - `modules/fail_CODE_*` —— 多模块工程，check 必须失败且含 CODE
//!
//! 每个fixture 文件头注释说明它锁定的回归点。新增修复请同步补 fixture：
//! **修复一个 bug，就锁定一个用例** —— 这是工具链的宪法。

use std::path::{Path, PathBuf};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn hsl_files(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("读取目录失败 {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "hsl").unwrap_or(false))
        .collect();
    out.sort();
    out
}

fn run_check(path: &Path) -> String {
    let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("读取失败: {e}"));
    let name = path.display().to_string();
    let result = dhv::compile_check(&name, &src);
    let mut out = String::new();
    for d in &result.diags.items {
        out.push_str(&dhv::diagnostics::render(d, &src, &name));
        out.push('\n');
        out.push_str(&d.code.as_str());
        out.push('\n');
    }
    out
}

/// ---- parse/：必须解析成功 ----
#[test]
fn parse_fixtures_must_parse() {
    let dir = fixtures_dir().join("parse");
    let mut failures = Vec::new();
    for f in hsl_files(&dir) {
        let src = std::fs::read_to_string(&f).unwrap();
        if let Err(diags) = dhv::parser::parse(0, &src) {
            let rendered = diags.render_all(&src, &f.display().to_string());
            failures.push(format!("{}\n{rendered}", f.display()));
        }
    }
    assert!(failures.is_empty(), "以下 fixture 解析失败:\n{}", failures.join("\n=====\n"));
}

/// ---- check/：必须 check 全绿 ----
#[test]
fn check_fixtures_must_pass() {
    let dir = fixtures_dir().join("check");
    let mut failures = Vec::new();
    for f in hsl_files(&dir) {
        let out = run_check(&f);
        // E0001/E0002 等错误码出现即失败
        if out.contains("ERROR") {
            failures.push(format!("{}\n{out}", f.display()));
        }
    }
    assert!(failures.is_empty(), "以下 fixture check 失败:\n{}", failures.join("\n=====\n"));
}

/// ---- errors/：必须 check 失败且命中期望代码 ----
#[test]
fn error_fixtures_must_fail_with_expected_code() {
    let dir = fixtures_dir().join("errors");
    let mut failures = Vec::new();
    for f in hsl_files(&dir) {
        let stem = f.file_stem().unwrap().to_string_lossy().to_string();
        let Some((expected_code, _)) = stem.split_once('_') else {
            panic!("errors/ 文件名必须以期望代码开头（如 S7_xxx.hsl）: {}", f.display());
        };
        let out = run_check(&f);
        if !out.contains("ERROR") {
            failures.push(format!("{}: 期望 {} 但 check 意外通过", f.display(), expected_code));
        } else if !out.contains(expected_code) {
            failures.push(format!(
                "{}: 期望诊断代码 {} 未出现，实际输出:\n{out}",
                f.display(),
                expected_code
            ));
        }
    }
    assert!(failures.is_empty(), "errors/ 断言未满足:\n{}", failures.join("\n=====\n"));
}

/// ---- modules/：多模块工程（linker 集成回归） ----
#[test]
fn module_projects_link_and_check() {
    let dir = fixtures_dir().join("modules");
    let mut failures = Vec::new();
    for project in std::fs::read_dir(&dir).unwrap() {
        let project = project.unwrap().path();
        if !project.is_dir() {
            continue;
        }
        let root = project.join("root.hsl");
        if !root.is_file() {
            continue;
        }
        let name = project.file_name().unwrap().to_string_lossy().to_string();
        let out = run_check(&root);
        if name.starts_with("pass_") {
            if out.contains("ERROR") {
                failures.push(format!("{}\n{out}", root.display()));
            }
        } else if let Some(rest) = name.strip_prefix("fail_") {
            let expected_code = rest.split('_').next().unwrap_or("");
            if !out.contains("ERROR") {
                failures.push(format!("{}: 期望 {} 但 check 意外通过", root.display(), expected_code));
            } else if !out.contains(expected_code) {
                failures.push(format!("{}: 期望 {} 未出现:\n{out}", root.display(), expected_code));
            }
        }
    }
    assert!(failures.is_empty(), "modules/ 断言未满足:\n{}", failures.join("\n=====\n"));
}

/// ---- 值语境 range：双编译器一致回归 ----
/// `let r = a..b;` 作一等值 —— dhv 与 dhv-ts 均支持（BNF v1.5 §2.11.7）。
/// fixture: check/value_context_range.hsl
#[test]
fn value_context_ranges_dhv_only() {
    let src = r#"export fn f(a: i64, b: i64) -> i64 {
    let r = a..b;
    let s = a..=b;
    let mut acc: i64 = 0;
    for i in r {
        acc = acc + i;
    }
    for i in s {
        acc = acc + i * 2;
    }
    acc
}
"#;
    let result = dhv::compile_check("value_ranges.hsl", src);
    assert!(!result.diags.has_errors(), "值语境 range 应通过 check: {:?}", result.diags.items.iter().map(|d| d.message.clone()).collect::<Vec<_>>());
}
