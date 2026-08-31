//! # SourceMap — 双向工程的基石
//!
//! 生成的每个物理文件注入围栏标记，标明 HSL 源码映射位置（BNF §2 双向工程机制）：
//!
//! ```text
//! # @dhv:source-map: main.hsl:15, block: graph_MyAgent
//! <可自由修改的逻辑层代码 —— 实时反编译回写 HSL>
//! # @dhv:end-source-map
//! ```
//!
//! 不可手改的内核/骨架代码用 `@dhv:generated` 标记。
//! 回写校验：回写后立即重新编译 + Lint，违反严格规则的修改被拒绝。

/// 围栏起止标记（注释形式按目标语言适配）
pub const FENCE_BEGIN: &str = "@dhv:source-map:";
pub const FENCE_END: &str = "@dhv:end-source-map";
pub const GENERATED_MARK: &str = "@dhv:generated";

/// HSL 源位置（文件 + 行号 + 逻辑块 id）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRef {
    pub file: String,
    pub line: usize,
    pub block: String,
}

impl SourceRef {
    pub fn new(file: impl Into<String>, line: usize, block: impl Into<String>) -> Self {
        SourceRef { file: file.into(), line, block: block.into() }
    }
    pub fn encode(&self) -> String {
        format!("{}{}:{}, block: {}", FENCE_BEGIN, self.file, self.line, self.block)
    }
}

/// 目标语言的注释定界符
#[derive(Debug, Clone, Copy)]
pub struct CommentStyle {
    pub line: &'static str,
    /// 块注释（可选，如 rust/md 的 <!-- -->）
    pub block: Option<(&'static str, &'static str)>,
}

pub fn comment_style(lang: &str) -> CommentStyle {
    match lang {
        "python" | "yaml" | "toml" => CommentStyle { line: "#", block: None },
        "rust" | "typescript" | "json" => CommentStyle { line: "//", block: None },
        "markdown" => CommentStyle { line: "", block: Some(("<!--", "-->") ) },
        _ => CommentStyle { line: "#", block: None },
    }
}

/// 用围栏包裹可编辑区（逻辑层代码）
pub fn wrap_editable(ref_: &SourceRef, body: &str, lang: &str) -> String {
    let style = comment_style(lang);
    match style.block {
        Some((open, close)) => format!("{open} {} {close}\n{body}\n{open} {} {close}", ref_.encode(), FENCE_END),
        None => format!("{} {}\n{}\n{} {}", style.line, ref_.encode(), body, style.line, FENCE_END),
    }
}

/// 生成不可手改标记头
pub fn generated_header(lang: &str) -> String {
    let style = comment_style(lang);
    match style.block {
        Some((open, close)) => format!("{open} {} — 不可手改，改了被下次编译覆盖 {close}", GENERATED_MARK),
        None => format!("{} {} — 不可手改，改了被下次编译覆盖", style.line, GENERATED_MARK),
    }
}

/// 从物理文件中提取一个围栏区间的原文（实时反编译流程的第 2 步）
pub fn extract_fence(content: &str) -> Option<(SourceRef, String)> {
    let begin_idx = content.find(FENCE_BEGIN)?;
    let line_start = content[..begin_idx].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = content[begin_idx..].find('\n').map(|i| begin_idx + i).unwrap_or(content.len());
    let header = &content[line_start..line_end];
    // 解析 "…@dhv:source-map: main.hsl:15, block: graph_MyAgent…"
    let rest = header.split(FENCE_BEGIN).nth(1)?.trim();
    let rest = rest.trim_end_matches(|c: char| c == '-' || c == '>' || c == ' ').trim();
    // main.hsl:15, block: graph_MyAgent
    let (file_line, block_part) = rest.split_once(", block:")?;
    let block = block_part.trim().trim_end().to_string();
    let (file, line) = file_line.trim().rsplit_once(':')?;
    let source_ref = SourceRef {
        file: file.trim().to_string(),
        line: line.trim().parse().ok()?,
        block,
    };
    // 围栏体：begin 行结束到 FENCE_END 行开始
    let body_start = line_end + 1;
    let end_marker = content[body_start..].find(FENCE_END)?;
    let mut body_end = body_start + end_marker;
    // 回退到 FENCE_END 所在行的行首
    if let Some(nl) = content[..body_end].rfind('\n') {
        body_end = nl;
    }
    let body = content[body_start..body_end].trim_end_matches('\n').to_string();
    Some((source_ref, body))
}

/// 回写：替换 HSL 源码中对应位置（实时反编译流程的第 3-4 步）
/// v0.1 骨架：按 block id 定位（完整版按精确行列定位 + 增量替换）
pub fn write_back(hsl_source: &str, ref_: &SourceRef, _new_body: &str) -> Result<String, String> {
    // P6 实现：File Watcher → 逆向解析器 → AST 节点替换。
    // 骨架阶段：验证 block 存在性并返回原源码（回写闭环在 P6 落地）。
    if !hsl_source.contains(&ref_.block) {
        return Err(format!("回写目标 block `{}` 在 {} 中不存在", ref_.block, ref_.file));
    }
    Ok(hsl_source.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fence_roundtrip() {
        let r = SourceRef::new("main.hsl", 15, "graph_MyAgent");
        let wrapped = wrap_editable(&r, "def main():\n    pass", "python");
        assert!(wrapped.contains(FENCE_BEGIN));
        let (parsed, body) = extract_fence(&wrapped).expect("fence extracted");
        assert_eq!(parsed.file, "main.hsl");
        assert_eq!(parsed.line, 15);
        assert_eq!(parsed.block, "graph_MyAgent");
        assert_eq!(body, "def main():\n    pass");
    }
}
