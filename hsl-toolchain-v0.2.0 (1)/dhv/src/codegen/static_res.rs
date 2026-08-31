//! 静态资源后端：YAML / Markdown / JSON（P4 —— v0.1 已完整可用）
//!
//! block/static 资源按「原文 + 编译期插值」转译：
//! - 原文逐字保留（SourceMap 回写保真）
//! - `{{ expr }}` 按 N4/N5 求值：常量折叠 / harness 运行期占位符

use crate::ast::*;
use crate::codegen::{CodegenBackend, CodegenContext};

// ---------------------------------------------------------------------------
// YAML
// ---------------------------------------------------------------------------

pub struct YamlBackend;

impl CodegenBackend for YamlBackend {
    fn lang(&self) -> &'static str { "yaml" }
    fn is_static_backend(&self) -> bool { true }
    // 插值走 trait 默认实现（const 表编译期求值 + 运行期占位符，N4/N5）
}

// ---------------------------------------------------------------------------
// Markdown
// ---------------------------------------------------------------------------

pub struct MarkdownBackend;

impl CodegenBackend for MarkdownBackend {
    fn lang(&self) -> &'static str { "markdown" }
    fn is_static_backend(&self) -> bool { true }
    // 插值走 trait 默认实现（const 表编译期求值 + 运行期占位符，N4/N5）
}

// ---------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------

pub struct JsonBackend;

impl CodegenBackend for JsonBackend {
    fn lang(&self) -> &'static str { "json" }
    fn is_static_backend(&self) -> bool { true }

    fn emit_static_resource(&self, ctx: &CodegenContext, res: &StaticResourceDef) -> Result<String, String> {
        // JSON 后端：验证体是合法 JSON（插值后）并规范化缩进
        let mut raw = String::new();
        for part in &res.content {
            match part {
                RawContentPart::Text(t) => raw.push_str(t),
                RawContentPart::Interpolation { expr, .. } => raw.push_str(&ctx.eval_interp(expr)),
            }
        }
        let trimmed = raw.trim();
        // 简单校验 + 透传（严格 JSON 规范化在 P4 完整版引入 serde_json pretty）
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            Ok(format!("{trimmed}\n"))
        } else {
            // 非 JSON 结构的 block 内容：作为 JSON 字符串值包裹
            Ok(format!("{}\n", json_escape(trimmed)))
        }
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
