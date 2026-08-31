// ============================================================================
// dhv/src/langs.rs — 后端语言注册表（BNF v1.4 §5.2，与 dhv-ts/src/backends/registry.ts 对齐）
// ----------------------------------------------------------------------------
// 32 编程语言 + 6 静态格式 = 38 后端。
// 能力分级（诚实边界）：
//   full    —— 活体语句翻译（函数体真实转译）
//   logic   —— 语句子集翻译，不可翻译时回退 contract
//   contract—— 类型契约投射（类型/签名真实翻译，函数体围栏内嵌 HSL 原文）
//   raw     —— 静态资源原文 + {{}} 插值渲染
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Full,
    Logic,
    Contract,
    Raw,
}

#[derive(Debug, Clone, Copy)]
pub struct LangSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub tier: u8, // 0=静态 1-4=编程语言 tier
    pub ext: &'static str,
    pub line_comment: &'static str,
    pub comment_close: Option<&'static str>,
    pub capability: Capability,
}

pub const LANGS: &[LangSpec] = &[
    // ===== Tier 1 · Harness 核心（10）=====
    LangSpec { id: "python", name: "Python", tier: 1, ext: ".py", line_comment: "#", comment_close: None, capability: Capability::Full },
    LangSpec { id: "typescript", name: "TypeScript", tier: 1, ext: ".ts", line_comment: "//", comment_close: None, capability: Capability::Full },
    LangSpec { id: "javascript", name: "JavaScript", tier: 1, ext: ".js", line_comment: "//", comment_close: None, capability: Capability::Full },
    LangSpec { id: "rust", name: "Rust", tier: 1, ext: ".rs", line_comment: "//", comment_close: None, capability: Capability::Logic },
    LangSpec { id: "go", name: "Go", tier: 1, ext: ".go", line_comment: "//", comment_close: None, capability: Capability::Logic },
    LangSpec { id: "cpp", name: "C++", tier: 1, ext: ".cpp", line_comment: "//", comment_close: None, capability: Capability::Logic },
    LangSpec { id: "java", name: "Java", tier: 1, ext: ".java", line_comment: "//", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "csharp", name: "C#", tier: 1, ext: ".cs", line_comment: "//", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "kotlin", name: "Kotlin", tier: 1, ext: ".kt", line_comment: "//", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "swift", name: "Swift", tier: 1, ext: ".swift", line_comment: "//", comment_close: None, capability: Capability::Contract },
    // ===== Tier 2 · 脚本与动态（8）=====
    LangSpec { id: "ruby", name: "Ruby", tier: 2, ext: ".rb", line_comment: "#", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "php", name: "PHP", tier: 2, ext: ".php", line_comment: "//", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "lua", name: "Lua", tier: 2, ext: ".lua", line_comment: "--", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "perl", name: "Perl", tier: 2, ext: ".pl", line_comment: "#", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "bash", name: "Bash", tier: 2, ext: ".sh", line_comment: "#", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "powershell", name: "PowerShell", tier: 2, ext: ".ps1", line_comment: "#", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "r", name: "R", tier: 2, ext: ".R", line_comment: "#", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "julia", name: "Julia", tier: 2, ext: ".jl", line_comment: "#", comment_close: None, capability: Capability::Contract },
    // ===== Tier 3 · 函数式（6）=====
    LangSpec { id: "scala", name: "Scala", tier: 3, ext: ".scala", line_comment: "//", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "elixir", name: "Elixir", tier: 3, ext: ".ex", line_comment: "#", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "erlang", name: "Erlang", tier: 3, ext: ".erl", line_comment: "%", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "haskell", name: "Haskell", tier: 3, ext: ".hs", line_comment: "--", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "ocaml", name: "OCaml", tier: 3, ext: ".ml", line_comment: "(*", comment_close: Some("*)"), capability: Capability::Contract },
    LangSpec { id: "fsharp", name: "F#", tier: 3, ext: ".fs", line_comment: "//", comment_close: None, capability: Capability::Contract },
    // ===== Tier 4 · 系统与现代（8）=====
    LangSpec { id: "zig", name: "Zig", tier: 4, ext: ".zig", line_comment: "//", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "nim", name: "Nim", tier: 4, ext: ".nim", line_comment: "#", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "crystal", name: "Crystal", tier: 4, ext: ".cr", line_comment: "#", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "dart", name: "Dart", tier: 4, ext: ".dart", line_comment: "//", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "groovy", name: "Groovy", tier: 4, ext: ".groovy", line_comment: "//", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "objectivec", name: "Objective-C", tier: 4, ext: ".m", line_comment: "//", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "d", name: "D", tier: 4, ext: ".d", line_comment: "//", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "vb", name: "Visual Basic", tier: 4, ext: ".vb", line_comment: "'", comment_close: None, capability: Capability::Contract },
    // ===== 静态格式（6）=====
    LangSpec { id: "yaml", name: "YAML", tier: 0, ext: ".yml", line_comment: "#", comment_close: None, capability: Capability::Raw },
    LangSpec { id: "markdown", name: "Markdown", tier: 0, ext: ".md", line_comment: "<!--", comment_close: Some("-->"), capability: Capability::Raw },
    LangSpec { id: "json", name: "JSON", tier: 0, ext: ".json", line_comment: "//", comment_close: None, capability: Capability::Raw },
    LangSpec { id: "toml", name: "TOML", tier: 0, ext: ".toml", line_comment: "#", comment_close: None, capability: Capability::Raw },
    LangSpec { id: "ini", name: "INI", tier: 0, ext: ".ini", line_comment: ";", comment_close: None, capability: Capability::Raw },
    LangSpec { id: "xml", name: "XML", tier: 0, ext: ".xml", line_comment: "<!--", comment_close: Some("-->"), capability: Capability::Raw },
];

/// 查询注册表；别名归一（ts/py/md/yml/c++/sh）
pub fn resolve(lang: &str) -> Option<&'static LangSpec> {
    let lowered = lang.to_ascii_lowercase();
    let norm = match lowered.as_str() {
        "ts" => "typescript",
        "js" => "javascript",
        "py" => "python",
        "md" => "markdown",
        "yml" => "yaml",
        "c++" => "cpp",
        "sh" | "shell" => "bash",
        "objective-c" => "objectivec",
        other => other,
    };
    LANGS.iter().find(|l| l.id == norm)
}

pub fn is_static(lang: &str) -> bool {
    resolve(lang).map(|l| l.tier == 0).unwrap_or(false)
}

pub fn is_code(lang: &str) -> bool {
    resolve(lang).map(|l| l.tier != 0).unwrap_or(false)
}

pub const COUNT: usize = LANGS.len();
pub const CODE_COUNT: usize = 32;
pub const STATIC_COUNT: usize = 6;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_counts() {
        assert_eq!(COUNT, 38);
        assert_eq!(LANGS.iter().filter(|l| l.tier != 0).count(), CODE_COUNT);
        assert_eq!(LANGS.iter().filter(|l| l.tier == 0).count(), STATIC_COUNT);
    }

    #[test]
    fn alias_resolution() {
        assert_eq!(resolve("ts").unwrap().id, "typescript");
        assert_eq!(resolve("c++").unwrap().id, "cpp");
        assert!(resolve("cobol").is_none());
    }
}
