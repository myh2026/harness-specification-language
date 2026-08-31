// ============================================================================
// dhv-ts/src/backends/registry.ts — 后端语言注册表（BNF v1.4 §5.2 / 总纲 §5）
// ----------------------------------------------------------------------------
// 32 个编程语言 + 6 个静态格式 = 38 个投射后端。
// 代码生成能力分级（诚实边界，写入 manifest）：
//   full    —— 活体语句翻译（函数体真实转译为目标语言）
//   logic   —— 语句子集翻译（let/if/while/for/match/调用链；不可翻译时回退 contract）
//   contract—— 类型契约投射（类型/签名真实翻译，函数体以 @dhv:source-map 围栏
//              内嵌 HSL 原文 + 目标语言显式未实现标记）
// 静态格式 —— block/static 原文 + {{}} 插值渲染。
// ============================================================================

export type BodyCapability = 'full' | 'logic' | 'contract' | 'none';
export type StaticCapability = 'raw';

export interface TypeMap {
  String?: string; char?: string; bool?: string;
  i32?: string; i64?: string; u32?: string; u64?: string; usize?: string; isize?: string;
  f32?: string; f64?: string;
  Vec?: string;      // %T 占位（单泛型）
  HashMap?: string;  // %K %V 占位
  HashSet?: string;  // %T
  Option?: string;   // %T
  Result?: string;   // %T %E
  Box?: string;      // %T
  unit?: string;
}

export interface LangSpec {
  id: string;
  name: string;
  tier: 1 | 2 | 3 | 4 | 0; // 0 = 静态格式
  tierName: string;
  ext: string;
  lineComment: string;
  /** 行注释需要尾缀闭合（如 OCaml 的 `(* ... *)`） */
  lineCommentClose?: string;
  blockComment?: [string, string];
  body: BodyCapability | StaticCapability;
  types: TypeMap;
  /** 目标语言可用宿主工具做语法校验（交叉 Lint 第 2 层） */
  validateWith?: 'python3' | 'bun-ts' | 'bun-js' | 'bash-n';
  /** native 块可否在 dhv-ts 运行期直接执行 */
  nativeRuntime?: boolean;
  note?: string;
}

const T = (o: TypeMap): TypeMap => o;

// ---- 编程语言（32）----

export const LANGS: LangSpec[] = [
  // ===== Tier 1 · Harness 核心（10）=====
  {
    id: 'python', name: 'Python', tier: 1, tierName: 'Harness 核心', ext: '.py',
    lineComment: '#', blockComment: ['"""', '"""'], body: 'full', nativeRuntime: true,
    validateWith: 'python3',
    types: T({ String: 'str', char: 'str', bool: 'bool', i32: 'int', i64: 'int', u32: 'int', u64: 'int', usize: 'int', isize: 'int', f32: 'float', f64: 'float', Vec: 'list[%T]', HashMap: 'dict[%K, %V]', HashSet: 'set[%T]', Option: '%T | None', Result: '%T', Box: '%T', unit: 'None' }),
  },
  {
    id: 'typescript', name: 'TypeScript', tier: 1, tierName: 'Harness 核心', ext: '.ts',
    lineComment: '//', blockComment: ['/*', '*/'], body: 'full', nativeRuntime: true,
    validateWith: 'bun-ts',
    types: T({ String: 'string', char: 'string', bool: 'boolean', i32: 'number', i64: 'number', u32: 'number', u64: 'number', usize: 'number', isize: 'number', f32: 'number', f64: 'number', Vec: '%T[]', HashMap: 'Map<%K, %V>', HashSet: 'Set<%T>', Option: '%T | null', Result: '%T', Box: '%T', unit: 'void' }),
  },
  {
    id: 'javascript', name: 'JavaScript', tier: 1, tierName: 'Harness 核心', ext: '.js',
    lineComment: '//', blockComment: ['/*', '*/'], body: 'full', nativeRuntime: true,
    validateWith: 'bun-js',
    types: T({ String: 'string', char: 'string', bool: 'boolean', i32: 'number', i64: 'number', u32: 'number', u64: 'number', usize: 'number', isize: 'number', f32: 'number', f64: 'number', Vec: 'Array', HashMap: 'Map', HashSet: 'Set', Option: '?', Result: '?', Box: '?', unit: 'undefined' }),
  },
  {
    id: 'rust', name: 'Rust', tier: 1, tierName: 'Harness 核心', ext: '.rs',
    lineComment: '//', blockComment: ['/*', '*/'], body: 'logic',
    types: T({ String: 'String', char: 'char', bool: 'bool', i32: 'i32', i64: 'i64', u32: 'u32', u64: 'u64', usize: 'usize', isize: 'isize', f32: 'f32', f64: 'f64', Vec: 'Vec<%T>', HashMap: 'HashMap<%K, %V>', HashSet: 'HashSet<%T>', Option: 'Option<%T>', Result: 'Result<%T, %E>', Box: 'Box<%T>', unit: '()' }),
  },
  {
    id: 'go', name: 'Go', tier: 1, tierName: 'Harness 核心', ext: '.go',
    lineComment: '//', blockComment: ['/*', '*/'], body: 'logic',
    types: T({ String: 'string', char: 'rune', bool: 'bool', i32: 'int32', i64: 'int64', u32: 'uint32', u64: 'uint64', usize: 'uint', isize: 'int', f32: 'float32', f64: 'float64', Vec: '[]%T', HashMap: 'map[%K]%V', HashSet: 'map[%T]struct{}', Option: '*%T', Result: '(%T, error)', Box: '*%T', unit: 'struct{}' }),
  },
  {
    id: 'cpp', name: 'C++', tier: 1, tierName: 'Harness 核心', ext: '.cpp',
    lineComment: '//', blockComment: ['/*', '*/'], body: 'logic',
    types: T({ String: 'std::string', char: 'char', bool: 'bool', i32: 'int32_t', i64: 'int64_t', u32: 'uint32_t', u64: 'uint64_t', usize: 'size_t', isize: 'intptr_t', f32: 'float', f64: 'double', Vec: 'std::vector<%T>', HashMap: 'std::unordered_map<%K, %V>', HashSet: 'std::unordered_set<%T>', Option: 'std::optional<%T>', Result: '%T', Box: 'std::unique_ptr<%T>', unit: 'void' }),
  },
  {
    id: 'java', name: 'Java', tier: 1, tierName: 'Harness 核心', ext: '.java',
    lineComment: '//', blockComment: ['/*', '*/'], body: 'contract',
    types: T({ String: 'String', char: 'char', bool: 'boolean', i32: 'int', i64: 'long', u32: 'int', u64: 'long', usize: 'long', isize: 'long', f32: 'float', f64: 'double', Vec: 'List<%T>', HashMap: 'Map<%K, %V>', HashSet: 'Set<%T>', Option: 'Optional<%T>', Result: '%T', Box: '%T', unit: 'void' }),
    note: 'sealed interface + record 契约（Java 17+）',
  },
  {
    id: 'csharp', name: 'C#', tier: 1, tierName: 'Harness 核心', ext: '.cs',
    lineComment: '//', blockComment: ['/*', '*/'], body: 'contract',
    types: T({ String: 'string', char: 'char', bool: 'bool', i32: 'int', i64: 'long', u32: 'uint', u64: 'ulong', usize: 'nuint', isize: 'nint', f32: 'float', f64: 'double', Vec: 'List<%T>', HashMap: 'Dictionary<%K, %V>', HashSet: 'HashSet<%T>', Option: '%T?', Result: '%T', Box: '%T', unit: 'void' }),
    note: 'abstract record + 派生 record 契约（C# 9+）',
  },
  {
    id: 'kotlin', name: 'Kotlin', tier: 1, tierName: 'Harness 核心', ext: '.kt',
    lineComment: '//', blockComment: ['/*', '*/'], body: 'contract',
    types: T({ String: 'String', char: 'Char', bool: 'Boolean', i32: 'Int', i64: 'Long', u32: 'UInt', u64: 'ULong', usize: 'UInt', isize: 'Int', f32: 'Float', f64: 'Double', Vec: 'List<%T>', HashMap: 'Map<%K, %V>', HashSet: 'Set<%T>', Option: '%T?', Result: '%T', Box: '%T', unit: 'Unit' }),
    note: 'sealed class + data class 契约',
  },
  {
    id: 'swift', name: 'Swift', tier: 1, tierName: 'Harness 核心', ext: '.swift',
    lineComment: '//', blockComment: ['/*', '*/'], body: 'contract',
    types: T({ String: 'String', char: 'Character', bool: 'Bool', i32: 'Int32', i64: 'Int64', u32: 'UInt32', u64: 'UInt64', usize: 'Int', isize: 'Int', f32: 'Float', f64: 'Double', Vec: '[%T]', HashMap: '[%K: %V]', HashSet: 'Set<%T>', Option: '%T?', Result: 'Result<%T, %E>', Box: '%T', unit: 'Void' }),
    note: 'enum 关联值原生支持（契约级）',
  },

  // ===== Tier 2 · 脚本与动态（8）=====
  {
    id: 'ruby', name: 'Ruby', tier: 2, tierName: '脚本与动态', ext: '.rb',
    lineComment: '#', blockComment: ['=begin', '=end'], body: 'contract',
    types: T({ String: 'String', char: 'String', bool: 'Boolean', i32: 'Integer', i64: 'Integer', u32: 'Integer', u64: 'Integer', usize: 'Integer', isize: 'Integer', f32: 'Float', f64: 'Float', Vec: 'Array', HashMap: 'Hash', HashSet: 'Set', Option: 'nilable', Result: 'nilable', Box: 'Object', unit: 'nil' }),
    note: 'Struct + case/in 模式匹配（Ruby 2.7+）',
  },
  {
    id: 'php', name: 'PHP', tier: 2, tierName: '脚本与动态', ext: '.php',
    lineComment: '//', blockComment: ['/*', '*/'], body: 'contract',
    types: T({ String: 'string', char: 'string', bool: 'bool', i32: 'int', i64: 'int', u32: 'int', u64: 'int', usize: 'int', isize: 'int', f32: 'float', f64: 'float', Vec: 'array', HashMap: 'array', HashSet: 'array', Option: '?%T', Result: '%T', Box: '%T', unit: 'void' }),
    note: 'enum（PHP 8.1）/ 抽象类 + match 契约',
  },
  {
    id: 'lua', name: 'Lua', tier: 2, tierName: '脚本与动态', ext: '.lua',
    lineComment: '--', blockComment: ['--[[', ']]'], body: 'contract',
    types: T({ String: 'string', char: 'string', bool: 'boolean', i32: 'number', i64: 'number', u32: 'number', u64: 'number', usize: 'number', isize: 'number', f32: 'number', f64: 'number', Vec: 'table', HashMap: 'table', HashSet: 'table', Option: 'nilable', Result: 'nilable', Box: 'any', unit: 'nil' }),
    note: 'table 标签联合',
  },
  {
    id: 'perl', name: 'Perl', tier: 2, tierName: '脚本与动态', ext: '.pl',
    lineComment: '#', body: 'contract',
    types: T({ String: 'Str', char: 'Str', bool: 'Bool', i32: 'Int', i64: 'Int', u32: 'Int', u64: 'Int', usize: 'Int', isize: 'Int', f32: 'Num', f64: 'Num', Vec: 'ArrayRef[%T]', HashMap: 'HashRef[%K, %V]', HashSet: 'HashRef[%T, Int]', Option: 'Maybe[%T]', Result: '%T', Box: '%T', unit: 'Undef' }),
    note: 'Moose 风格类型约束（契约级）',
  },
  {
    id: 'bash', name: 'Bash', tier: 2, tierName: '脚本与动态', ext: '.sh',
    lineComment: '#', body: 'contract', validateWith: 'bash-n',
    types: T({ String: 'string', char: 'string', bool: 'bool', i32: 'int', i64: 'int', u32: 'int', u64: 'int', usize: 'int', isize: 'int', f32: 'float', f64: 'float', Vec: 'array', HashMap: 'assoc', HashSet: 'assoc', Option: 'nullable', Result: 'retcode', Box: 'value', unit: 'void' }),
    note: 'case/函数壳 + 关联数组',
  },
  {
    id: 'powershell', name: 'PowerShell', tier: 2, tierName: '脚本与动态', ext: '.ps1',
    lineComment: '#', blockComment: ['<#', '#>'], body: 'contract',
    types: T({ String: '[string]', char: '[char]', bool: '[bool]', i32: '[int]', i64: '[long]', u32: '[uint32]', u64: '[uint64]', usize: '[uint64]', isize: '[int64]', f32: '[float]', f64: '[double]', Vec: '[array]', HashMap: '[hashtable]', HashSet: '[hashtable]', Option: '[object]', Result: '[object]', Box: '[object]', unit: '[void]' }),
    note: 'class + switch 契约',
  },
  {
    id: 'r', name: 'R', tier: 2, tierName: '脚本与动态', ext: '.R',
    lineComment: '#', body: 'contract',
    types: T({ String: 'character', char: 'character', bool: 'logical', i32: 'integer', i64: 'integer', u32: 'integer', u64: 'double', usize: 'double', isize: 'double', f32: 'double', f64: 'double', Vec: 'vector', HashMap: 'list', HashSet: 'vector', Option: 'nullable', Result: 'nullable', Box: 'any', unit: 'NULL' }),
    note: 'list + switch 契约',
  },
  {
    id: 'julia', name: 'Julia', tier: 2, tierName: '脚本与动态', ext: '.jl',
    lineComment: '#', blockComment: ['#=', '=#'], body: 'contract',
    types: T({ String: 'String', char: 'Char', bool: 'Bool', i32: 'Int32', i64: 'Int64', u32: 'UInt32', u64: 'UInt64', usize: 'Int', isize: 'Int', f32: 'Float32', f64: 'Float64', Vec: 'Vector{%T}', HashMap: 'Dict{%K, %V}', HashSet: 'Set{%T}', Option: 'Union{%T, Nothing}', Result: 'Union{%T, %E}', Box: '%T', unit: 'Nothing' }),
    note: 'struct + Union 契约',
  },

  // ===== Tier 3 · 函数式（6）=====
  {
    id: 'scala', name: 'Scala', tier: 3, tierName: '函数式', ext: '.scala',
    lineComment: '//', blockComment: ['/*', '*/'], body: 'contract',
    types: T({ String: 'String', char: 'Char', bool: 'Boolean', i32: 'Int', i64: 'Long', u32: 'Int', u64: 'Long', usize: 'Long', isize: 'Long', f32: 'Float', f64: 'Double', Vec: 'List[%T]', HashMap: 'Map[%K, %V]', HashSet: 'Set[%T]', Option: 'Option[%T]', Result: 'Either[%E, %T]', Box: '%T', unit: 'Unit' }),
    note: 'sealed trait + case class（原生和类型）',
  },
  {
    id: 'elixir', name: 'Elixir', tier: 3, tierName: '函数式', ext: '.ex',
    lineComment: '#', body: 'contract',
    types: T({ String: 'String.t()', char: 'String.t()', bool: 'boolean', i32: 'integer', i64: 'integer', u32: 'non_neg_integer', u64: 'non_neg_integer', usize: 'non_neg_integer', isize: 'integer', f32: 'float', f64: 'float', Vec: 'list(%T)', HashMap: 'map()', HashSet: 'MapSet.t()', Option: '{:ok, %T} | :error', Result: '{:ok, %T} | {:error, %E}', Box: 'any', unit: ':ok' }),
    note: 'defmodule + defstruct + 模式匹配（原生和类型）',
  },
  {
    id: 'erlang', name: 'Erlang', tier: 3, tierName: '函数式', ext: '.erl',
    lineComment: '%', body: 'contract',
    types: T({ String: 'binary()', char: 'char()', bool: 'boolean()', i32: 'integer()', i64: 'integer()', u32: 'non_neg_integer()', u64: 'non_neg_integer()', usize: 'non_neg_integer()', isize: 'integer()', f32: 'float()', f64: 'float()', Vec: 'list(%T)', HashMap: 'map()', HashSet: 'sets:set()', Option: '{some, %T} | none', Result: '{ok, %T} | {error, %E}', Box: 'any()', unit: 'ok' }),
    note: 'tagged tuple（原生和类型）',
  },
  {
    id: 'haskell', name: 'Haskell', tier: 3, tierName: '函数式', ext: '.hs',
    lineComment: '--', blockComment: ['{-', '-}'], body: 'contract',
    types: T({ String: 'String', char: 'Char', bool: 'Bool', i32: 'Int', i64: 'Int64', u32: 'Word32', u64: 'Word64', usize: 'Int', isize: 'Int', f32: 'Float', f64: 'Double', Vec: '[%T]', HashMap: 'Map %K %V', HashSet: 'Set %T', Option: 'Maybe %T', Result: 'Either %E %T', Box: '%T', unit: '()' }),
    note: 'data ... = ...（原生和类型）',
  },
  {
    id: 'ocaml', name: 'OCaml', tier: 3, tierName: '函数式', ext: '.ml',
    lineComment: '(*', lineCommentClose: '*)', blockComment: ['(*', '*)'], body: 'contract',
    types: T({ String: 'string', char: 'char', bool: 'bool', i32: 'int32', i64: 'int64', u32: 'uint32', u64: 'uint64', usize: 'int', isize: 'int', f32: 'float', f64: 'float', Vec: '%T list', HashMap: '(%K, %V) Hashtbl.t', HashSet: '%T list', Option: '%T option', Result: '(%T, %E) result', Box: '%T', unit: 'unit' }),
    note: 'type ... = A | B（原生和类型）',
  },
  {
    id: 'fsharp', name: 'F#', tier: 3, tierName: '函数式', ext: '.fs',
    lineComment: '//', blockComment: ['(*', '*)'], body: 'contract',
    types: T({ String: 'string', char: 'char', bool: 'bool', i32: 'int32', i64: 'int64', u32: 'uint32', u64: 'uint64', usize: 'int', isize: 'int', f32: 'float32', f64: 'float', Vec: '%T list', HashMap: 'Map<%K, %V>', HashSet: 'Set<%T>', Option: 'Option<%T>', Result: 'Result<%T, %E>', Box: '%T', unit: 'unit' }),
    note: 'type ... = A of ... | B（原生 DU）',
  },

  // ===== Tier 4 · 系统与现代（8）=====
  {
    id: 'zig', name: 'Zig', tier: 4, tierName: '系统与现代', ext: '.zig',
    lineComment: '//', body: 'contract',
    types: T({ String: '[]const u8', char: 'u8', bool: 'bool', i32: 'i32', i64: 'i64', u32: 'u32', u64: 'u64', usize: 'usize', isize: 'isize', f32: 'f32', f64: 'f64', Vec: '[]%T', HashMap: 'std.AutoHashMap(%K, %V)', HashSet: 'std.AutoHashMap(%T, void)', Option: '?%T', Result: '%T', Box: '*%T', unit: 'void' }),
    note: 'tagged union（原生和类型）',
  },
  {
    id: 'nim', name: 'Nim', tier: 4, tierName: '系统与现代', ext: '.nim',
    lineComment: '#', body: 'contract',
    types: T({ String: 'string', char: 'char', bool: 'bool', i32: 'int32', i64: 'int64', u32: 'uint32', u64: 'uint64', usize: 'int', isize: 'int', f32: 'float32', f64: 'float64', Vec: 'seq[%T]', HashMap: 'Table[%K, %V]', HashSet: 'HashSet[%T]', Option: 'Option[%T]', Result: 'Result[%T, %E]', Box: 'ref %T', unit: 'void' }),
    note: 'object variants（原生和类型）',
  },
  {
    id: 'crystal', name: 'Crystal', tier: 4, tierName: '系统与现代', ext: '.cr',
    lineComment: '#', body: 'contract',
    types: T({ String: 'String', char: 'Char', bool: 'Bool', i32: 'Int32', i64: 'Int64', u32: 'UInt32', u64: 'UInt64', usize: 'Int64', isize: 'Int64', f32: 'Float32', f64: 'Float64', Vec: 'Array(%T)', HashMap: 'Hash(%K, %V)', HashSet: 'Set(%T)', Option: '%T | Nil', Result: '%T', Box: '%T', unit: 'Nil' }),
    note: 'abstract class + struct + case（原生联合）',
  },
  {
    id: 'dart', name: 'Dart', tier: 4, tierName: '系统与现代', ext: '.dart',
    lineComment: '//', blockComment: ['/*', '*/'], body: 'contract',
    types: T({ String: 'String', char: 'String', bool: 'bool', i32: 'int', i64: 'int', u32: 'int', u64: 'int', usize: 'int', isize: 'int', f32: 'double', f64: 'double', Vec: 'List<%T>', HashMap: 'Map<%K, %V>', HashSet: 'Set<%T>', Option: '%T?', Result: '%T', Box: '%T', unit: 'void' }),
    note: 'sealed class + factory（Dart 3）',
  },
  {
    id: 'groovy', name: 'Groovy', tier: 4, tierName: '系统与现代', ext: '.groovy',
    lineComment: '//', blockComment: ['/*', '*/'], body: 'contract',
    types: T({ String: 'String', char: 'char', bool: 'boolean', i32: 'int', i64: 'long', u32: 'int', u64: 'long', usize: 'long', isize: 'long', f32: 'float', f64: 'double', Vec: 'List<%T>', HashMap: 'Map<%K, %V>', HashSet: 'Set<%T>', Option: 'Optional<%T>', Result: '%T', Box: '%T', unit: 'void' }),
    note: 'class + @Canonical 契约',
  },
  {
    id: 'objectivec', name: 'Objective-C', tier: 4, tierName: '系统与现代', ext: '.m',
    lineComment: '//', blockComment: ['/*', '*/'], body: 'contract',
    types: T({ String: 'NSString *', char: 'unichar', bool: 'BOOL', i32: 'int32_t', i64: 'int64_t', u32: 'uint32_t', u64: 'uint64_t', usize: 'NSUInteger', isize: 'NSInteger', f32: 'float', f64: 'double', Vec: 'NSArray<%T> *', HashMap: 'NSDictionary<%K, %V> *', HashSet: 'NSSet<%T> *', Option: '%T', Result: '%T', Box: '%T', unit: 'void' }),
    note: 'interface + 实现壳契约',
  },
  {
    id: 'd', name: 'D', tier: 4, tierName: '系统与现代', ext: '.d',
    lineComment: '//', blockComment: ['/*', '*/'], body: 'contract',
    types: T({ String: 'string', char: 'char', bool: 'bool', i32: 'int', i64: 'long', u32: 'uint', u64: 'ulong', usize: 'size_t', isize: 'ptrdiff_t', f32: 'float', f64: 'double', Vec: '%T[]', HashMap: '%V[%K]', HashSet: '%T[int]', Option: '%T', Result: '%T', Box: '%T*', unit: 'void' }),
    note: 'struct + tagged union 契约',
  },
  {
    id: 'vb', name: 'Visual Basic', tier: 4, tierName: '系统与现代', ext: '.vb',
    lineComment: "'", body: 'contract',
    types: T({ String: 'String', char: 'Char', bool: 'Boolean', i32: 'Integer', i64: 'Long', u32: 'UInteger', u64: 'ULong', usize: 'ULong', isize: 'Long', f32: 'Single', f64: 'Double', Vec: 'List(Of %T)', HashMap: 'Dictionary(Of %K, %V)', HashSet: 'HashSet(Of %T)', Option: '%T', Result: '%T', Box: '%T', unit: 'Sub' }),
    note: 'Structure/Module 契约',
  },
];

// ---- 静态格式（6）----
export const STATIC_LANGS: LangSpec[] = [
  { id: 'yaml', name: 'YAML', tier: 0, tierName: '静态资源', ext: '.yml', lineComment: '#', body: 'raw', types: {} },
  { id: 'markdown', name: 'Markdown', tier: 0, tierName: '静态资源', ext: '.md', lineComment: '<!--', blockComment: ['<!--', '-->'], body: 'raw', types: {} },
  { id: 'json', name: 'JSON', tier: 0, tierName: '静态资源', ext: '.json', lineComment: '//', body: 'raw', types: {}, note: 'JSON 无注释：dhv 以独立 .map 边车文件记录围栏' },
  { id: 'toml', name: 'TOML', tier: 0, tierName: '静态资源', ext: '.toml', lineComment: '#', body: 'raw', types: {} },
  { id: 'ini', name: 'INI', tier: 0, tierName: '静态资源', ext: '.ini', lineComment: ';', body: 'raw', types: {} },
  { id: 'xml', name: 'XML', tier: 0, tierName: '静态资源', ext: '.xml', lineComment: '<!--', blockComment: ['<!--', '-->'], body: 'raw', types: {} },
];

export const ALL_LANGS: LangSpec[] = [...LANGS, ...STATIC_LANGS];

const byId = new Map(ALL_LANGS.map((l) => [l.id, l]));

export function getLang(id: string): LangSpec | undefined {
  return byId.get(id);
}

export function isStaticLangId(id: string): boolean {
  const l = byId.get(id);
  return l?.tier === 0;
}

export function listLangs(): LangSpec[] {
  return ALL_LANGS;
}

/** 旧别名兼容：ts → typescript，py → python，md → markdown，yml → yaml */
const ALIASES: Record<string, string> = {
  ts: 'typescript', js: 'javascript', py: 'python', md: 'markdown',
  yml: 'yaml', 'c++': 'cpp', 'objective-c': 'objectivec', shell: 'bash', sh: 'bash',
};

export function resolveLangId(id: string): string {
  const lower = id.toLowerCase();
  return ALIASES[lower] ?? lower;
}

export function codegenTier(lang: LangSpec): string {
  if (lang.tier === 0) return 'static';
  if (lang.body === 'full') return 'full';
  if (lang.body === 'logic') return 'logic';
  return 'contract';
}
