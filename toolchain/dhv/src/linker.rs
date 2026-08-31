//! 最小模块链接器（M 系列名字解析的加载层）
//!
//! 解析 `import { A } from "./path.hsl"` / `import * as ns from "./path.hsl"`，
//! 加载依赖模块 AST（BFS + 环检测 + 按规范路径去重），供类型检查器在检查根文件前
//! 收集跨模块语义注册表：
//! - enum 变体注册表（S6 跨模块穷尽性校验）
//! - 静态资源清单（P4 跨模块 block/static 投射合法性）
//!
//! 模块体内的 S 系列检查由 TypeChecker::check_module_body 逐文件执行（对齐 dhv-ts）。

use crate::ast::{Item, SourceFile, TopLevel};
use crate::parser;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct LinkedProject {
    /// 依赖模块（BFS 序，不含根文件）：(展示路径, AST, 源码)
    pub modules: Vec<(String, SourceFile)>,
    /// 依赖模块源码（展示路径 → 源码，用于多文件诊断渲染）
    pub module_sources: Vec<(String, String)>,
    /// 加载失败列表：(导入方文件, 模块路径, 原因)
    pub errors: Vec<(String, String, String)>,
}

/// 收集一个文件的全部 import 模块路径
fn import_paths(file: &SourceFile) -> Vec<String> {
    let mut out = Vec::new();
    for top in &file.items {
        if let TopLevel::Item(Item::Import(imp)) = top {
            if !imp.from.is_empty() && !out.contains(&imp.from) {
                out.push(imp.from.clone());
            }
        }
    }
    out
}

/// 从根文件出发加载整个模块闭包（BFS）
pub fn link(root_name: &str, root: &SourceFile) -> LinkedProject {
    let mut project = LinkedProject { modules: Vec::new(), module_sources: Vec::new(), errors: Vec::new() };
    let Some(root_dir) = Path::new(root_name).parent().map(|p| p.to_path_buf()) else {
        return project;
    };

    let mut visited: HashSet<PathBuf> = HashSet::new();
    // 工作队列：(待加载模块的绝对路径, 展示路径, 所属目录, 导入方)
    let mut queue: Vec<(PathBuf, String, PathBuf, String)> = Vec::new();
    for from in import_paths(root) {
        if let Some(spec) = resolve(&root_dir, &from) {
            queue.push((spec.0, spec.1, spec.2, root_name.to_string()));
        } else {
            project.errors.push((
                root_name.to_string(),
                from.clone(),
                "模块文件不存在（相对导入路径无法解析）".to_string(),
            ));
        }
    }

    let mut guard = 0usize;
    while let Some((abs, display, dir, importer)) = queue.pop() {
        guard += 1;
        if guard > 4096 {
            // 防御性上限：模块闭包异常庞大时停止（正常工程远小于此）
            break;
        }
        if !visited.insert(abs.clone()) {
            continue;
        }
        let src = match std::fs::read_to_string(&abs) {
            Ok(s) => s,
            Err(e) => {
                project.errors.push((importer, display, format!("读取模块失败: {e}")));
                continue;
            }
        };
        let ast = match parser::parse(0, &src) {
            Ok(f) => f,
            Err(ds) => {
                let first = ds
                    .items
                    .first()
                    .map(|d| d.message.clone())
                    .unwrap_or_else(|| "解析失败".to_string());
                project.errors.push((importer, display.clone(), first));
                continue;
            }
        };
        for from in import_paths(&ast) {
            if let Some(spec) = resolve(&dir, &from) {
                queue.push((spec.0, spec.1, spec.2, display.clone()));
            } else {
                project.errors.push((
                    display.clone(),
                    from.clone(),
                    "模块文件不存在（相对导入路径无法解析）".to_string(),
                ));
            }
        }
        project.modules.push((display.clone(), ast));
        project.module_sources.push((display, src));
    }
    project
}

/// 相对导入路径解析：返回 (规范绝对路径, 展示路径, 模块自身目录)
fn resolve(dir: &Path, from: &str) -> Option<(PathBuf, String, PathBuf)> {
    let joined = dir.join(from);
    let abs = joined.canonicalize().ok()?;
    let display = abs.display().to_string();
    let parent = abs.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
    Some((abs, display, parent))
}
