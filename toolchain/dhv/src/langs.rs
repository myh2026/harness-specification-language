// ============================================================================
// dhv/src/langs.rs — 后端语言注册表（BNF v1.5 §5.2，与 dhv-ts/src/backends/registry.ts 对齐）
// ----------------------------------------------------------------------------
// 32 编程语言 + 6 静态格式 = 38 后端。
// 能力分级（诚实边界）：
//   full    (3)  —— 活体语句翻译（函数体真实转译）：python / typescript / javascript
//   logic   (10) —— 语句子集翻译（专属后端）：rust / go / cpp / java / csharp / kotlin / swift / scala / dart / elixir
//   contract(19) —— 类型契约投射（通用后端，函数体围栏内嵌 HSL 原文）
//   raw     (6)  —— 静态资源原文 + {{}} 插值渲染：yaml / markdown / json / toml / ini / xml
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
    LangSpec { id: "java", name: "Java", tier: 1, ext: ".java", line_comment: "//", comment_close: None, capability: Capability::Logic },
    LangSpec { id: "csharp", name: "C#", tier: 1, ext: ".cs", line_comment: "//", comment_close: None, capability: Capability::Logic },
    LangSpec { id: "kotlin", name: "Kotlin", tier: 1, ext: ".kt", line_comment: "//", comment_close: None, capability: Capability::Logic },
    LangSpec { id: "swift", name: "Swift", tier: 1, ext: ".swift", line_comment: "//", comment_close: None, capability: Capability::Logic },
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
    LangSpec { id: "scala", name: "Scala", tier: 3, ext: ".scala", line_comment: "//", comment_close: None, capability: Capability::Logic },
    LangSpec { id: "elixir", name: "Elixir", tier: 3, ext: ".ex", line_comment: "#", comment_close: None, capability: Capability::Logic },
    LangSpec { id: "erlang", name: "Erlang", tier: 3, ext: ".erl", line_comment: "%", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "haskell", name: "Haskell", tier: 3, ext: ".hs", line_comment: "--", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "ocaml", name: "OCaml", tier: 3, ext: ".ml", line_comment: "(*", comment_close: Some("*)"), capability: Capability::Contract },
    LangSpec { id: "fsharp", name: "F#", tier: 3, ext: ".fs", line_comment: "//", comment_close: None, capability: Capability::Contract },
    // ===== Tier 4 · 系统与现代（8）=====
    LangSpec { id: "zig", name: "Zig", tier: 4, ext: ".zig", line_comment: "//", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "nim", name: "Nim", tier: 4, ext: ".nim", line_comment: "#", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "crystal", name: "Crystal", tier: 4, ext: ".cr", line_comment: "#", comment_close: None, capability: Capability::Contract },
    LangSpec { id: "dart", name: "Dart", tier: 4, ext: ".dart", line_comment: "//", comment_close: None, capability: Capability::Logic },
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

/// 语言语法族（决定 contract 后端的声明输出风格）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LangFamily {
    /// Java, C#, Kotlin, Swift, Scala, Dart, Groovy, F#, VB, Crystal — class-based OOP
    OOClass,
    /// C++, D, Zig, Nim, Objective-C — C-family systems
    CFamily,
    /// Ruby, PHP, Lua, Perl, Bash, PowerShell, R, Julia — dynamic/scripting
    Script,
    /// Elixir, Erlang, Haskell, OCaml — functional
    Functional,
}

/// 返回语言的语法族
pub fn family_for(lang_id: &str) -> LangFamily {
    match lang_id {
        "java" | "csharp" | "kotlin" | "swift" | "scala" | "dart" | "groovy"
        | "fsharp" | "vb" | "crystal" => LangFamily::OOClass,
        "cpp" | "d" | "zig" | "nim" | "objectivec" => LangFamily::CFamily,
        "ruby" | "php" | "lua" | "perl" | "bash" | "powershell"
        | "r" | "julia" => LangFamily::Script,
        "elixir" | "erlang" | "haskell" | "ocaml" => LangFamily::Functional,
        _ => LangFamily::OOClass,
    }
}

// ---------------------------------------------------------------------------
// 每语言 HSL→目标 类型映射表（对齐 dhv-ts backends/registry.ts types: TypeMap）
// 占位符：%T = 唯一类型参数，%K/%V = 键/值，%E = Result 错误类型
// ---------------------------------------------------------------------------

/// 返回语言的 HSL→目标类型映射（基本类型 + 泛型容器）
pub fn type_map_for(lang_id: &str) -> &'static [(&'static str, &'static str)] {
    match lang_id {
        "python" => &[
            ("String", "str"), ("char", "str"), ("bool", "bool"),
            ("i32", "int"), ("i64", "int"), ("u32", "int"), ("u64", "int"),
            ("usize", "int"), ("isize", "int"), ("f32", "float"), ("f64", "float"),
            ("Vec", "list[%T]"), ("HashMap", "dict[%K, %V]"), ("HashSet", "set[%T]"),
            ("Option", "%T | None"), ("Result", "%T"), ("Box", "%T"), ("unit", "None"),
        ],
        "typescript" => &[
            ("String", "string"), ("char", "string"), ("bool", "boolean"),
            ("i32", "number"), ("i64", "number"), ("u32", "number"), ("u64", "number"),
            ("usize", "number"), ("isize", "number"), ("f32", "number"), ("f64", "number"),
            ("Vec", "%T[]"), ("HashMap", "Map<%K, %V>"), ("HashSet", "Set<%T>"),
            ("Option", "%T | null"), ("Result", "%T"), ("Box", "%T"), ("unit", "void"),
        ],
        "javascript" => &[
            ("String", "string"), ("char", "string"), ("bool", "boolean"),
            ("i32", "number"), ("i64", "number"), ("u32", "number"), ("u64", "number"),
            ("usize", "number"), ("isize", "number"), ("f32", "number"), ("f64", "number"),
            ("Vec", "Array"), ("HashMap", "Map"), ("HashSet", "Set"),
            ("Option", "?"), ("Result", "?"), ("Box", "?"), ("unit", "undefined"),
        ],
        "rust" => &[
            ("String", "String"), ("char", "char"), ("bool", "bool"),
            ("i32", "i32"), ("i64", "i64"), ("u32", "u32"), ("u64", "u64"),
            ("usize", "usize"), ("isize", "isize"), ("f32", "f32"), ("f64", "f64"),
            ("Vec", "Vec<%T>"), ("HashMap", "HashMap<%K, %V>"), ("HashSet", "HashSet<%T>"),
            ("Option", "Option<%T>"), ("Result", "Result<%T, %E>"), ("Box", "Box<%T>"), ("unit", "()"),
        ],
        "go" => &[
            ("String", "string"), ("char", "rune"), ("bool", "bool"),
            ("i32", "int32"), ("i64", "int64"), ("u32", "uint32"), ("u64", "uint64"),
            ("usize", "uint"), ("isize", "int"), ("f32", "float32"), ("f64", "float64"),
            ("Vec", "[]%T"), ("HashMap", "map[%K]%V"), ("HashSet", "map[%T]struct{}"),
            ("Option", "*%T"), ("Result", "(%T, error)"), ("Box", "*%T"), ("unit", "struct{}"),
        ],
        "cpp" => &[
            ("String", "std::string"), ("char", "char"), ("bool", "bool"),
            ("i32", "int32_t"), ("i64", "int64_t"), ("u32", "uint32_t"), ("u64", "uint64_t"),
            ("usize", "size_t"), ("isize", "intptr_t"), ("f32", "float"), ("f64", "double"),
            ("Vec", "std::vector<%T>"), ("HashMap", "std::unordered_map<%K, %V>"), ("HashSet", "std::unordered_set<%T>"),
            ("Option", "std::optional<%T>"), ("Result", "%T"), ("Box", "std::unique_ptr<%T>"), ("unit", "void"),
        ],
        "java" => &[
            ("String", "String"), ("char", "char"), ("bool", "boolean"),
            ("i32", "int"), ("i64", "long"), ("u32", "int"), ("u64", "long"),
            ("usize", "long"), ("isize", "long"), ("f32", "float"), ("f64", "double"),
            ("Vec", "List<%T>"), ("HashMap", "Map<%K, %V>"), ("HashSet", "Set<%T>"),
            ("Option", "Optional<%T>"), ("Result", "%T"), ("Box", "%T"), ("unit", "void"),
        ],
        "csharp" => &[
            ("String", "string"), ("char", "char"), ("bool", "bool"),
            ("i32", "int"), ("i64", "long"), ("u32", "uint"), ("u64", "ulong"),
            ("usize", "nuint"), ("isize", "nint"), ("f32", "float"), ("f64", "double"),
            ("Vec", "List<%T>"), ("HashMap", "Dictionary<%K, %V>"), ("HashSet", "HashSet<%T>"),
            ("Option", "%T?"), ("Result", "%T"), ("Box", "%T"), ("unit", "void"),
        ],
        "kotlin" => &[
            ("String", "String"), ("char", "Char"), ("bool", "Boolean"),
            ("i32", "Int"), ("i64", "Long"), ("u32", "UInt"), ("u64", "ULong"),
            ("usize", "UInt"), ("isize", "Int"), ("f32", "Float"), ("f64", "Double"),
            ("Vec", "List<%T>"), ("HashMap", "Map<%K, %V>"), ("HashSet", "Set<%T>"),
            ("Option", "%T?"), ("Result", "%T"), ("Box", "%T"), ("unit", "Unit"),
        ],
        "swift" => &[
            ("String", "String"), ("char", "Character"), ("bool", "Bool"),
            ("i32", "Int32"), ("i64", "Int64"), ("u32", "UInt32"), ("u64", "UInt64"),
            ("usize", "Int"), ("isize", "Int"), ("f32", "Float"), ("f64", "Double"),
            ("Vec", "[%T]"), ("HashMap", "[%K: %V]"), ("HashSet", "Set<%T>"),
            ("Option", "%T?"), ("Result", "Result<%T, %E>"), ("Box", "%T"), ("unit", "Void"),
        ],
        "scala" => &[
            ("String", "String"), ("char", "Char"), ("bool", "Boolean"),
            ("i32", "Int"), ("i64", "Long"), ("u32", "Int"), ("u64", "Long"),
            ("usize", "Long"), ("isize", "Long"), ("f32", "Float"), ("f64", "Double"),
            ("Vec", "Vector[%T]"), ("HashMap", "Map[%K, %V]"), ("HashSet", "Set[%T]"),
            ("Option", "Option[%T]"), ("Result", "Either[%E, %T]"), ("Box", "%T"), ("unit", "Unit"),
        ],
        "dart" => &[
            ("String", "String"), ("char", "String"), ("bool", "bool"),
            ("i32", "int"), ("i64", "int"), ("u32", "int"), ("u64", "int"),
            ("usize", "int"), ("isize", "int"), ("f32", "double"), ("f64", "double"),
            ("Vec", "List<%T>"), ("HashMap", "Map<%K, %V>"), ("HashSet", "Set<%T>"),
            ("Option", "%T?"), ("Result", "%T"), ("Box", "%T"), ("unit", "void"),
        ],
        "groovy" => &[
            ("String", "String"), ("char", "char"), ("bool", "boolean"),
            ("i32", "int"), ("i64", "long"), ("u32", "int"), ("u64", "long"),
            ("usize", "long"), ("isize", "long"), ("f32", "float"), ("f64", "double"),
            ("Vec", "List<%T>"), ("HashMap", "Map<%K, %V>"), ("HashSet", "Set<%T>"),
            ("Option", "Optional<%T>"), ("Result", "%T"), ("Box", "%T"), ("unit", "void"),
        ],
        "fsharp" => &[
            ("String", "string"), ("char", "char"), ("bool", "bool"),
            ("i32", "int32"), ("i64", "int64"), ("u32", "uint32"), ("u64", "uint64"),
            ("usize", "int"), ("isize", "int"), ("f32", "float32"), ("f64", "float"),
            ("Vec", "%T list"), ("HashMap", "Map<%K, %V>"), ("HashSet", "Set<%T>"),
            ("Option", "%T option"), ("Result", "Result<%T, %E>"), ("Box", "%T"), ("unit", "unit"),
        ],
        "crystal" => &[
            ("String", "String"), ("char", "Char"), ("bool", "Bool"),
            ("i32", "Int32"), ("i64", "Int64"), ("u32", "UInt32"), ("u64", "UInt64"),
            ("usize", "Int64"), ("isize", "Int64"), ("f32", "Float32"), ("f64", "Float64"),
            ("Vec", "Array(%T)"), ("HashMap", "Hash(%K, %V)"), ("HashSet", "Set(%T)"),
            ("Option", "%T | Nil"), ("Result", "%T"), ("Box", "%T"), ("unit", "Nil"),
        ],
        "vb" => &[
            ("String", "String"), ("char", "Char"), ("bool", "Boolean"),
            ("i32", "Integer"), ("i64", "Long"), ("u32", "UInteger"), ("u64", "ULong"),
            ("usize", "ULong"), ("isize", "Long"), ("f32", "Single"), ("f64", "Double"),
            ("Vec", "List(Of %T)"), ("HashMap", "Dictionary(Of %K, %V)"), ("HashSet", "HashSet(Of %T)"),
            ("Option", "%T"), ("Result", "%T"), ("Box", "%T"), ("unit", "Sub"),
        ],
        "objectivec" => &[
            ("String", "NSString *"), ("char", "unichar"), ("bool", "BOOL"),
            ("i32", "int32_t"), ("i64", "int64_t"), ("u32", "uint32_t"), ("u64", "uint64_t"),
            ("usize", "NSUInteger"), ("isize", "NSInteger"), ("f32", "float"), ("f64", "double"),
            ("Vec", "NSArray<%T> *"), ("HashMap", "NSDictionary<%K, %V> *"), ("HashSet", "NSSet<%T> *"),
            ("Option", "%T"), ("Result", "%T"), ("Box", "%T"), ("unit", "void"),
        ],
        "d" => &[
            ("String", "string"), ("char", "char"), ("bool", "bool"),
            ("i32", "int"), ("i64", "long"), ("u32", "uint"), ("u64", "ulong"),
            ("usize", "size_t"), ("isize", "ptrdiff_t"), ("f32", "float"), ("f64", "double"),
            ("Vec", "%T[]"), ("HashMap", "%V[%K]"), ("HashSet", "%T[int]"),
            ("Option", "%T"), ("Result", "%T"), ("Box", "%T*"), ("unit", "void"),
        ],
        "zig" => &[
            ("String", "[]const u8"), ("char", "u8"), ("bool", "bool"),
            ("i32", "i32"), ("i64", "i64"), ("u32", "u32"), ("u64", "u64"),
            ("usize", "usize"), ("isize", "isize"), ("f32", "f32"), ("f64", "f64"),
            ("Vec", "[]%T"), ("HashMap", "std.AutoHashMap(%K, %V)"), ("HashSet", "std.AutoHashMap(%T, void)"),
            ("Option", "?%T"), ("Result", "%T"), ("Box", "*%T"), ("unit", "void"),
        ],
        "nim" => &[
            ("String", "string"), ("char", "char"), ("bool", "bool"),
            ("i32", "int32"), ("i64", "int64"), ("u32", "uint32"), ("u64", "uint64"),
            ("usize", "int"), ("isize", "int"), ("f32", "float32"), ("f64", "float64"),
            ("Vec", "seq[%T]"), ("HashMap", "Table[%K, %V]"), ("HashSet", "HashSet[%T]"),
            ("Option", "Option[%T]"), ("Result", "Result[%T, %E]"), ("Box", "ref %T"), ("unit", "void"),
        ],
        "ruby" => &[
            ("String", "String"), ("char", "String"), ("bool", "Boolean"),
            ("i32", "Integer"), ("i64", "Integer"), ("u32", "Integer"), ("u64", "Integer"),
            ("usize", "Integer"), ("isize", "Integer"), ("f32", "Float"), ("f64", "Float"),
            ("Vec", "Array"), ("HashMap", "Hash"), ("HashSet", "Set"),
            ("Option", "nilable"), ("Result", "nilable"), ("Box", "Object"), ("unit", "nil"),
        ],
        "php" => &[
            ("String", "string"), ("char", "string"), ("bool", "bool"),
            ("i32", "int"), ("i64", "int"), ("u32", "int"), ("u64", "int"),
            ("usize", "int"), ("isize", "int"), ("f32", "float"), ("f64", "float"),
            ("Vec", "array"), ("HashMap", "array"), ("HashSet", "array"),
            ("Option", "?%T"), ("Result", "%T"), ("Box", "%T"), ("unit", "void"),
        ],
        "lua" => &[
            ("String", "string"), ("char", "string"), ("bool", "boolean"),
            ("i32", "number"), ("i64", "number"), ("u32", "number"), ("u64", "number"),
            ("usize", "number"), ("isize", "number"), ("f32", "number"), ("f64", "number"),
            ("Vec", "table"), ("HashMap", "table"), ("HashSet", "table"),
            ("Option", "nilable"), ("Result", "nilable"), ("Box", "any"), ("unit", "nil"),
        ],
        "perl" => &[
            ("String", "Str"), ("char", "Str"), ("bool", "Bool"),
            ("i32", "Int"), ("i64", "Int"), ("u32", "Int"), ("u64", "Int"),
            ("usize", "Int"), ("isize", "Int"), ("f32", "Num"), ("f64", "Num"),
            ("Vec", "ArrayRef[%T]"), ("HashMap", "HashRef[%K, %V]"), ("HashSet", "HashRef[%T, Int]"),
            ("Option", "Maybe[%T]"), ("Result", "%T"), ("Box", "%T"), ("unit", "Undef"),
        ],
        "bash" => &[
            ("String", "string"), ("char", "string"), ("bool", "bool"),
            ("i32", "int"), ("i64", "int"), ("u32", "int"), ("u64", "int"),
            ("usize", "int"), ("isize", "int"), ("f32", "float"), ("f64", "float"),
            ("Vec", "array"), ("HashMap", "assoc"), ("HashSet", "assoc"),
            ("Option", "nullable"), ("Result", "retcode"), ("Box", "value"), ("unit", "void"),
        ],
        "powershell" => &[
            ("String", "string"), ("char", "char"), ("bool", "bool"),
            ("i32", "int"), ("i64", "long"), ("u32", "uint"), ("u64", "ulong"),
            ("usize", "long"), ("isize", "long"), ("f32", "float"), ("f64", "double"),
            ("Vec", "System.Collections.Generic.List[%T]"), ("HashMap", "System.Collections.Generic.Dictionary[%K, %V]"),
            ("HashSet", "System.Collections.Generic.HashSet[%T]"),
            ("Option", "%T"), ("Result", "%T"), ("Box", "%T"), ("unit", "void"),
        ],
        "r" => &[
            ("String", "character"), ("char", "character"), ("bool", "logical"),
            ("i32", "integer"), ("i64", "integer"), ("u32", "integer"), ("u64", "double"),
            ("usize", "double"), ("isize", "double"), ("f32", "double"), ("f64", "double"),
            ("Vec", "vector"), ("HashMap", "list"), ("HashSet", "vector"),
            ("Option", "nullable"), ("Result", "nullable"), ("Box", "any"), ("unit", "NULL"),
        ],
        "julia" => &[
            ("String", "String"), ("char", "Char"), ("bool", "Bool"),
            ("i32", "Int32"), ("i64", "Int64"), ("u32", "UInt32"), ("u64", "UInt64"),
            ("usize", "Int"), ("isize", "Int"), ("f32", "Float32"), ("f64", "Float64"),
            ("Vec", "Vector{%T}"), ("HashMap", "Dict{%K, %V}"), ("HashSet", "Set{%T}"),
            ("Option", "Union{%T, Nothing}"), ("Result", "Union{%T, %E}"), ("Box", "%T"), ("unit", "Nothing"),
        ],
        "elixir" => &[
            ("String", "String.t()"), ("char", "String.t()"), ("bool", "boolean"),
            ("i32", "integer"), ("i64", "integer"), ("u32", "non_neg_integer"), ("u64", "non_neg_integer"),
            ("usize", "non_neg_integer"), ("isize", "integer"), ("f32", "float"), ("f64", "float"),
            ("Vec", "list(%T)"), ("HashMap", "map()"), ("HashSet", "MapSet.t()"),
            ("Option", "{:ok, %T} | :error"), ("Result", "{:ok, %T} | {:error, %E}"), ("Box", "any"), ("unit", ":ok"),
        ],
        "erlang" => &[
            ("String", "binary()"), ("char", "char()"), ("bool", "boolean()"),
            ("i32", "integer()"), ("i64", "integer()"), ("u32", "non_neg_integer()"), ("u64", "non_neg_integer()"),
            ("usize", "non_neg_integer()"), ("isize", "integer()"), ("f32", "float()"), ("f64", "float()"),
            ("Vec", "list(%T)"), ("HashMap", "map()"), ("HashSet", "sets:set()"),
            ("Option", "{some, %T} | none"), ("Result", "{ok, %T} | {error, %E}"), ("Box", "any()"), ("unit", "ok"),
        ],
        "haskell" => &[
            ("String", "String"), ("char", "Char"), ("bool", "Bool"),
            ("i32", "Int"), ("i64", "Int64"), ("u32", "Word32"), ("u64", "Word64"),
            ("usize", "Int"), ("isize", "Int"), ("f32", "Float"), ("f64", "Double"),
            ("Vec", "[%T]"), ("HashMap", "Map %K %V"), ("HashSet", "Set %T"),
            ("Option", "Maybe %T"), ("Result", "Either %E %T"), ("Box", "%T"), ("unit", "()"),
        ],
        "ocaml" => &[
            ("String", "string"), ("char", "char"), ("bool", "bool"),
            ("i32", "int32"), ("i64", "int64"), ("u32", "uint32"), ("u64", "uint64"),
            ("usize", "int"), ("isize", "int"), ("f32", "float32"), ("f64", "float"),
            ("Vec", "%T list"), ("HashMap", "(%K, %V) Hashtbl.t"), ("HashSet", "%T list"),
            ("Option", "%T option"), ("Result", "(%T, %E) result"), ("Box", "%T"), ("unit", "unit"),
        ],
        _ => &[],
    }
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

    #[test]
    fn type_map_coverage() {
        // 每个编程语言都必须有类型映射（静态格式除外）
        for lang in LANGS.iter().filter(|l| l.tier != 0) {
            let map = type_map_for(lang.id);
            assert!(!map.is_empty(), "{} 缺少类型映射", lang.id);
            // 必须包含基础类型
            assert!(map.iter().any(|(k, _)| *k == "String"), "{} 缺少 String 映射", lang.id);
            assert!(map.iter().any(|(k, _)| *k == "i64"), "{} 缺少 i64 映射", lang.id);
            assert!(map.iter().any(|(k, _)| *k == "bool"), "{} 缺少 bool 映射", lang.id);
        }
    }

    #[test]
    fn family_coverage() {
        for lang in LANGS.iter().filter(|l| l.tier != 0) {
            let f = family_for(lang.id);
            assert!(matches!(f, LangFamily::OOClass | LangFamily::CFamily | LangFamily::Script | LangFamily::Functional),
                "{} 缺少语法族", lang.id);
        }
    }
}
