//! 诊断系统 — 错误 / 警告 / 提示的收集与渲染
//!
//! 所有编译阶段（Parse / TypeCheck / Topology / Project / Lint / Codegen）
//! 统一使用 `Diagnostic` 结构，渲染为带源码摘录（line:col + 波浪线）的
//! 人可读文本。

use crate::ast::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagCode {
    /// P2 解析错误
    Parse,
    /// 词法 / 文法歧义（如 BNF §1.8 消解规则违例）
    Lexical,
    /// S1-S8 严格性铁律（BNF §5.1）
    Strictness(&'static str),
    /// G1-G6 拓扑校验（BNF §5.3）
    Topology(&'static str),
    /// P1-P7 投射一致性（BNF §5.4）
    Projection(&'static str),
    /// N1-N5 native / 插值安全（BNF §5.5）
    NativeSafety(&'static str),
    /// M1-M5 名字解析（BNF §5.8）
    NameResolution(&'static str),
    /// 类型检查（通用）
    Type,
    /// Lint（三层 Lint 的第一层：HSL 原生）
    Lint(&'static str),
    /// Codegen / 后端
    Codegen,
}

impl DiagCode {
    pub fn as_str(self) -> String {
        match self {
            DiagCode::Parse => "E0001".into(),
            DiagCode::Lexical => "E0002".into(),
            DiagCode::Strictness(id) => format!("S-{id}"),
            DiagCode::Topology(id) => format!("G-{id}"),
            DiagCode::Projection(id) => format!("P-{id}"),
            DiagCode::NativeSafety(id) => format!("N-{id}"),
            DiagCode::NameResolution(id) => format!("M-{id}"),
            DiagCode::Type => "E0100".into(),
            DiagCode::Lint(id) => format!("L-{id}"),
            DiagCode::Codegen => "E0900".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: DiagCode,
    pub message: String,
    pub span: Span,
    /// 附注（帮助信息 / 关联位置）
    pub notes: Vec<String>,
}

impl Diagnostic {
    pub fn error(code: DiagCode, message: impl Into<String>, span: Span) -> Self {
        Diagnostic { severity: Severity::Error, code, message: message.into(), span, notes: vec![] }
    }
    pub fn warning(code: DiagCode, message: impl Into<String>, span: Span) -> Self {
        Diagnostic { severity: Severity::Warning, code, message: message.into(), span, notes: vec![] }
    }
    pub fn note(mut self, msg: impl Into<String>) -> Self {
        self.notes.push(msg.into());
        self
    }
}

/// 计算字节偏移对应的 (line, col)（1-based）
pub fn line_col_of(src: &str, offset: usize) -> (usize, usize) {
    let bytes = src.as_bytes();
    let offset = offset.min(bytes.len());
    let mut line = 1usize;
    let mut col = 1usize;
    for &b in &bytes[..offset] {
        if b == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// 把诊断渲染为人可读文本（带源码摘录与波泚线）
pub fn render(diag: &Diagnostic, src: &str, file_name: &str) -> String {
    let (line, col) = line_col_of(src, diag.span.start);
    let mut out = String::new();
    out.push_str(&format!(
        "{}[{}] {}:{}:{}: {}\n",
        diag.severity.label().to_uppercase(),
        diag.code.as_str(),
        file_name,
        line,
        col,
        diag.message
    ));
    // 源码摘录：定位行首/行尾
    if let Some(line_start) = src[..diag.span.start.min(src.len())].rfind('\n').map(|i| i + 1) {
        let line_end = src[diag.span.start.min(src.len())..]
            .find('\n')
            .map(|i| diag.span.start + i)
            .unwrap_or(src.len());
        let snippet = &src[line_start..line_end];
        out.push_str(&format!("  {line} | {snippet}\n"));
        let caret_col = diag.span.start - line_start;
        out.push_str(&format!(
            "  {} | {}{}\n",
            " ".repeat(line.to_string().len()),
            " ".repeat(caret_col),
            "^".repeat((diag.span.end - diag.span.start).max(1))
        ));
    }
    for note in &diag.notes {
        out.push_str(&format!("  = note: {note}\n"));
    }
    out
}

/// 诊断汇聚器
#[derive(Debug, Default)]
pub struct Diagnostics {
    pub items: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Diagnostics { items: Vec::new() }
    }
    pub fn push(&mut self, d: Diagnostic) {
        self.items.push(d);
    }
    pub fn extend(&mut self, ds: impl IntoIterator<Item = Diagnostic>) {
        self.items.extend(ds);
    }
    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.severity == Severity::Error)
    }
    pub fn error_count(&self) -> usize {
        self.items.iter().filter(|d| d.severity == Severity::Error).count()
    }
    pub fn render_all(&self, src: &str, file_name: &str) -> String {
        let mut out = String::new();
        for d in &self.items {
            out.push_str(&render(d, src, file_name));
        }
        if self.has_errors() {
            out.push_str(&format!(
                "error: aborting due to {} previous error(s)\n",
                self.error_count()
            ));
        }
        out
    }
}
