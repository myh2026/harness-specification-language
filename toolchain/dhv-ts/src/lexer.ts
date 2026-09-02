// ============================================================================
// dhv-ts/src/lexer.ts — HSL 词法器（BNF v1.2 §1）
// ----------------------------------------------------------------------------
// 覆盖：§1.1-1.8 通用 token；§1.9 模式 A（block/static 原始体 + {{}} 插值）
//       与模式 B（native lang 原始体，按目标语言字符串/注释感知扫描）。
// ============================================================================

export type TokKind =
  | 'ident'
  | 'kw'
  | 'int'
  | 'float'
  | 'string'
  | 'rawstr'
  | 'char'
  | 'label'
  | 'punct'
  | 'nativeraw' // native lang { ... } 的原始体（模式 B）
  | 'rawblock' // block/static { ... } 的原始体 + 插值部件（模式 A）
  | 'eof';

export interface RawPart {
  t: 'text' | 'expr';
  text?: string;
  src?: string;
  line: number;
  col: number;
}

export interface Token {
  kind: TokKind;
  text: string; // 原始文本（ident/关键字/punct 名）
  value?: unknown; // 解析值：number|bigint|string|boolean
  suffix?: string; // 整数/浮点后缀
  parts?: RawPart[]; // rawblock 专用
  lang?: string; // nativeraw 专用：目标语言标识
  line: number;
  col: number;
}

export const KEYWORDS = new Set([
  'as', 'async', 'await', 'block', 'break', 'const', 'continue', 'dyn', 'edge',
  'else', 'enum', 'export', 'false', 'fn', 'for', 'graph', 'if', 'impl',
  'import', 'in', 'let', 'loop', 'match', 'mod', 'mut', 'native', 'on',
  'project', 'return', 'scale', 'static', 'struct', 'trait', 'true', 'type',
  'while', 'where', 'move',
  // 注：`from` 在 BNF v1.3 中降级为上下文关键字（仅 import 子句），否则与
  // String::from / From trait 生态冲突 —— 见 hsl-spec/BNF.md §8 变更记录。
]);

// 上下文关键字（§1.4）：不是保留字，按普通 ident 处理，由 parser 在特定位置识别
export const LANG_IDENTS = new Set([
  // 38 后端注册表（与 backends/registry.ts 一致，BNF v1.4 §5.2）
  // Tier 1 Harness 核心
  'python', 'typescript', 'javascript', 'rust', 'go', 'cpp', 'java', 'csharp', 'kotlin', 'swift',
  // Tier 2 脚本与动态
  'ruby', 'php', 'lua', 'perl', 'bash', 'powershell', 'r', 'julia',
  // Tier 3 函数式
  'scala', 'elixir', 'erlang', 'haskell', 'ocaml', 'fsharp',
  // Tier 4 系统与现代
  'zig', 'nim', 'crystal', 'dart', 'groovy', 'objectivec', 'd', 'vb',
  // 静态格式
  'yaml', 'markdown', 'json', 'toml', 'ini', 'xml',
]);

const PUNCTS3 = ['>>=', '<<=', '..='];
const PUNCTS2 = ['&&', '||', '==', '!=', '<=', '>=', '->', '=>', '::', '..', '+=', '-=', '*=', '/=', '%=', '&=', '|=', '^=', '<<', '>>'];
const PUNCTS1 = '+-*/%^!&|<>=,;:?@#$()[]{}.'.split('');

const JS_RESERVED = new Set([
  'const', 'let', 'var', 'function', 'return', 'if', 'else', 'for', 'while',
  'class', 'new', 'await', 'async', 'typeof', 'instanceof', 'import', 'export',
  'true', 'false', 'null', 'undefined', 'this', 'throw', 'try', 'catch',
  'finally', 'switch', 'case', 'break', 'continue', 'do', 'delete', 'in', 'of',
  'void', 'yield', 'super', 'extends', 'default', 'with', 'static', 'enum',
]);

function isIdentStart(c: string): boolean {
  return /[A-Za-z_\u0080-\uFFFF]/.test(c);
}
function isIdentCont(c: string): boolean {
  return /[A-Za-z0-9_\u0080-\uFFFF]/.test(c);
}
function isDigit(c: string): boolean {
  return c >= '0' && c <= '9';
}
function isHex(c: string): boolean {
  return /[0-9a-fA-F]/.test(c);
}

export class LexError extends Error {
  constructor(msg: string, public line: number, public col: number) {
    super(`${msg} (line ${line}, col ${col})`);
  }
}

export class Lexer {
  private pos = 0;
  private line = 1;
  private col = 1;
  private depthMode: 'none' | 'afterNative' | 'afterResource' = 'none';
  private pendingLang = '';

  constructor(private src: string, public file = '<memory>') {}

  /**
   * 形状前瞻：当前位置（`block`/`static` 关键字之后）是否为 `NAME {` 资源声明形态。
   * 仅做原始字符扫描（跳过空白与注释），不消费任何输入。
   */
  private looksLikeResourceDecl(): boolean {
    const isIdStart = (c: string) => /[A-Za-z_]/.test(c);
    const isIdCont = (c: string) => /[A-Za-z0-9_]/.test(c);
    let i = this.pos;
    for (;;) {
      while (i < this.src.length && /\s/.test(this.src[i])) i++;
      if (this.src[i] === '/' && this.src[i + 1] === '/') {
        while (i < this.src.length && this.src[i] !== '\n') i++;
        continue;
      }
      if (this.src[i] === '/' && this.src[i + 1] === '*') {
        let depth = 1;
        i += 2;
        while (i < this.src.length && depth > 0) {
          if (this.src[i] === '/' && this.src[i + 1] === '*') { depth++; i += 2; }
          else if (this.src[i] === '*' && this.src[i + 1] === '/') { depth--; i += 2; }
          else i++;
        }
        continue;
      }
      break;
    }
    if (i >= this.src.length || !isIdStart(this.src[i])) return false;
    while (i < this.src.length && isIdCont(this.src[i])) i++;
    while (i < this.src.length && /\s/.test(this.src[i])) i++;
    return i < this.src.length && this.src[i] === '{';
  }

  tokenize(): Token[] {
    const toks: Token[] = [];
    for (;;) {
      const t = this.next();
      toks.push(t);
      if (t.kind === 'eof') break;
    }
    return toks;
  }

  private peek(off = 0): string {
    return this.src[this.pos + off] ?? '';
  }

  private advance(): string {
    const c = this.src[this.pos++];
    if (c === '\n') {
      this.line++;
      this.col = 1;
    } else {
      this.col++;
    }
    return c;
  }

  private mk(kind: TokKind, text: string, value?: unknown, suffix?: string): Token {
    return { kind, text, value, suffix, line: this.line, col: this.col };
  }

  private skipTrivia(): void {
    for (;;) {
      const c = this.peek();
      if (c === ' ' || c === '\t' || c === '\n' || c === '\r' || c === '\f') {
        this.advance();
      } else if (c === '/' && this.peek(1) === '/') {
        while (this.pos < this.src.length && this.peek() !== '\n') this.advance();
      } else if (c === '/' && this.peek(1) === '*') {
        // 嵌套块注释（§1.2）
        let depth = 0;
        while (this.pos < this.src.length) {
          if (this.peek() === '/' && this.peek(1) === '*') {
            depth++;
            this.advance();
            this.advance();
          } else if (this.peek() === '*' && this.peek(1) === '/') {
            depth--;
            this.advance();
            this.advance();
            if (depth === 0) break;
          } else {
            this.advance();
          }
        }
      } else {
        break;
      }
    }
  }

  private next(): Token {
    this.skipTrivia();
    const line = this.line;
    const col = this.col;
    if (this.pos >= this.src.length) return { kind: 'eof', text: '<eof>', line, col };

    // 模式 B：native lang { 原始体 } —— 先读 lang 标识符，再扫描原始区
    if (this.depthMode === 'afterNative') {
      this.depthMode = 'none';
      this.skipTrivia();
      let lang = '';
      while (this.pos < this.src.length && isIdentCont(this.peek())) lang += this.advance();
      this.skipTrivia();
      if (this.peek() !== '{') throw new LexError('native 块缺少 "{"', line, col);
      this.advance();
      const body = this.scanRawBody('native:' + lang);
      this.advance(); // 结束 '}'
      return { kind: 'nativeraw', text: body, value: body, lang, line, col };
    }
    // 模式 A：block/static NAME { 原始体 + 插值 } —— 先读资源名，再扫描原始区
    if (this.depthMode === 'afterResource') {
      this.depthMode = 'none';
      this.skipTrivia();
      let name = '';
      while (this.pos < this.src.length && isIdentCont(this.peek())) name += this.advance();
      if (name.length === 0) throw new LexError('资源块缺少名称', line, col);
      this.skipTrivia();
      if (this.peek() !== '{') throw new LexError('资源块缺少 "{"', line, col);
      this.advance();
      const parts = this.scanBlockBody();
      this.advance(); // 结束 '}'
      return { kind: 'rawblock', text: name, parts, line, col };
    }

    const c = this.peek();

    // ---- 标识符 / 关键字 / raw ident / raw string ----
    if (isIdentStart(c)) {
      // r"..." r#"..."# 原始字符串
      if (c === 'r' && (this.peek(1) === '"' || (this.peek(1) === '#' && this.peek(2) === '"'))) {
        return this.lexRawString();
      }
      // r#ident 原始标识符
      if (c === 'r' && this.peek(1) === '#' && isIdentStart(this.peek(2))) {
        this.advance();
        this.advance();
        let name = '';
        while (this.pos < this.src.length && isIdentCont(this.peek())) name += this.advance();
        const t = this.mk('ident', name);
        return t;
      }
      let name = '';
      while (this.pos < this.src.length && isIdentCont(this.peek())) name += this.advance();
      // 检查是否触发原始代码区模式
      if (KEYWORDS.has(name)) {
        if (name === 'native') this.depthMode = 'afterNative';
        if (name === 'static' || name === 'block') {
          // 仅当形状为 `block NAME {` / `static NAME {` 时进入原始区模式；
          // §2.15（BNF v1.5）rules { block -> ... } 中 `block` 是规则类型，其后不是 NAME {
          if (this.looksLikeResourceDecl()) this.depthMode = 'afterResource';
        }
        return this.mk('kw', name);
      }
      return this.mk('ident', name);
    }

    // ---- 数字 ----
    if (isDigit(c)) return this.lexNumber();

    // ---- 字符串 / 字符 / 标签 ----
    if (c === '"') return this.lexString();
    if (c === "'") return this.lexCharOrLabel();

    // ---- 标点 ----
    const three = this.src.slice(this.pos, this.pos + 3);
    if (PUNCTS3.includes(three)) {
      this.advance(); this.advance(); this.advance();
      return this.mk('punct', three);
    }
    const two = this.src.slice(this.pos, this.pos + 2);
    if (PUNCTS2.includes(two)) {
      // '{{' / '}}' 由 block 扫描器处理，此处不会遇到（模式 A 已截获）
      this.advance(); this.advance();
      return this.mk('punct', two);
    }
    if (PUNCTS1.includes(c)) {
      this.advance();
      return this.mk('punct', c);
    }

    throw new LexError(`意外字符 "${c}"`, line, col);
  }

  private lexNumber(): Token {
    const line = this.line, col = this.col;
    let raw = '';
    // 进制
    if (this.peek() === '0' && (this.peek(1) === 'x' || this.peek(1) === 'o' || this.peek(1) === 'b')) {
      const radix = this.peek(1) === 'x' ? 16 : this.peek(1) === 'o' ? 8 : 2;
      this.advance(); this.advance();
      while (this.pos < this.src.length && (isIdentCont(this.peek()))) raw += this.advance();
      const digits = raw.replace(/_/g, '');
      if (digits.length === 0) throw new LexError('进制字面量缺少数字', line, col);
      const prefix = radix === 16 ? '0x' : radix === 8 ? '0o' : '0b';
      const val = BigInt(prefix + digits);
      return { kind: 'int', text: prefix + raw, value: val, line, col };
    }
    let isFloat = false;
    while (this.pos < this.src.length && (isDigit(this.peek()) || this.peek() === '_')) raw += this.advance();
    // 浮点：'.' 后跟数字（L2：`1..3` 不并入）
    if (this.peek() === '.' && isDigit(this.peek(1))) {
      isFloat = true;
      raw += this.advance();
      while (this.pos < this.src.length && (isDigit(this.peek()) || this.peek() === '_')) raw += this.advance();
    }
    // 指数
    if ((this.peek() === 'e' || this.peek() === 'E') && (isDigit(this.peek(1)) || ((this.peek(1) === '+' || this.peek(1) === '-') && isDigit(this.peek(2))))) {
      isFloat = true;
      raw += this.advance();
      if (this.peek() === '+' || this.peek() === '-') raw += this.advance();
      while (this.pos < this.src.length && (isDigit(this.peek()) || this.peek() === '_')) raw += this.advance();
    }
    // 后缀（i32/u64/f32/...）
    let suffix = '';
    if (isIdentStart(this.peek())) {
      let s = '';
      let p = this.pos;
      while (p < this.src.length && isIdentCont(this.src[p]!)) { s += this.src[p]; p++; }
      if (/^(i8|i16|i32|i64|i128|isize|u8|u16|u32|u64|u128|usize|f32|f64)$/.test(s)) {
        suffix = s;
        while (s.length > 0) { s = s.slice(1); this.advance(); }
      }
    }
    if (isFloat) {
      return { kind: 'float', text: raw, value: parseFloat(raw.replace(/_/g, '')), suffix, line, col };
    }
    const clean = raw.replace(/_/g, '');
    const n = Number(clean);
    const v = Number.isSafeInteger(n) ? n : BigInt(clean);
    return { kind: 'int', text: raw, value: v, suffix, line, col };
  }

  private lexEscape(): string {
    // 进入时 this.peek() === '\\'
    this.advance();
    const e = this.advance();
    switch (e) {
      case 'n': return '\n';
      case 'r': return '\r';
      case 't': return '\t';
      case '\\': return '\\';
      case '0': return '\0';
      case '\'': return '\'';
      case '"': return '"';
      case 'x': {
        let h = '';
        h += this.advance(); h += this.advance();
        return String.fromCharCode(parseInt(h, 16));
      }
      case 'u': {
        if (this.peek() !== '{') throw new LexError('\\u 转义缺少 {', this.line, this.col);
        this.advance();
        let h = '';
        while (this.peek() !== '}' && this.pos < this.src.length) h += this.advance();
        this.advance();
        return String.fromCodePoint(parseInt(h.replace(/_/g, ''), 16));
      }
      default:
        throw new LexError(`未知转义 \\${e}`, this.line, this.col);
    }
  }

  private lexString(): Token {
    const line = this.line, col = this.col;
    this.advance(); // "
    let out = '';
    while (this.pos < this.src.length) {
      const c = this.peek();
      if (c === '"') { this.advance(); return { kind: 'string', text: out, value: out, line, col }; }
      if (c === '\\') { out += this.lexEscape(); continue; }
      if (c === '\n') throw new LexError('字符串字面量未闭合（含换行）', line, col);
      out += this.advance();
    }
    throw new LexError('字符串字面量未闭合', line, col);
  }

  private lexRawString(): Token {
    const line = this.line, col = this.col;
    this.advance(); // r
    let hashes = '';
    while (this.peek() === '#') { hashes += this.advance(); }
    if (this.peek() !== '"') throw new LexError('原始字符串缺少 "', line, col);
    this.advance();
    let out = '';
    const closer = '"' + hashes;
    while (this.pos < this.src.length) {
      if (this.src.startsWith(closer, this.pos)) {
        for (let i = 0; i < closer.length; i++) this.advance();
        return { kind: 'rawstr', text: out, value: out, line, col };
      }
      out += this.advance();
    }
    throw new LexError('原始字符串未闭合', line, col);
  }

  private lexCharOrLabel(): Token {
    const line = this.line, col = this.col;
    this.advance(); // '
    const c = this.peek();
    if (c === '\\') {
      const ch = this.lexEscape();
      if (this.peek() !== "'") throw new LexError('字符字面量未闭合', line, col);
      this.advance();
      return { kind: 'char', text: ch, value: ch, line, col };
    }
    // 'x' → 字符；'name → 标签（§1.6）
    const second = this.peek(1);
    if (c !== '' && second === "'") {
      this.advance(); this.advance();
      return { kind: 'char', text: c, value: c, line, col };
    }
    if (c !== '' && isIdentStart(c)) {
      let name = '';
      while (this.pos < this.src.length && isIdentCont(this.peek())) name += this.advance();
      return { kind: 'label', text: name, value: name, line, col };
    }
    throw new LexError(`无法解析的 ' 开头 token（其后内容："${this.src.slice(this.pos, this.pos + 12).replace(/\n/g, '\\n')}…"）`, line, col);
  }

  // ==========================================================================
  // 模式 B：native 体扫描（目标语言字符串/注释感知 + 大括号深度计数）
  // ==========================================================================
  private skipLangStringOrComment(lang: string): string | null {
    const startPos = this.pos;
    const c = this.peek();
    const py = lang === 'python';
    const rs = lang === 'rust';
    const ts = lang === 'typescript' || lang === 'javascript';
    const done = (): string | null => this.src.slice(startPos, this.pos);
    if (py) {
      if (c === '#') { while (this.pos < this.src.length && this.peek() !== '\n') this.advance(); return done(); }
      if (this.src.startsWith('"""', this.pos) || this.src.startsWith("'''", this.pos)) {
        const q = this.src.slice(this.pos, this.pos + 3);
        this.advance(); this.advance(); this.advance();
        while (this.pos < this.src.length && !this.src.startsWith(q, this.pos)) this.advance();
        for (let i = 0; i < 3 && this.pos < this.src.length; i++) this.advance();
        return done();
      }
    }
    if ((ts || rs) && c === '/' && this.peek(1) === '/') {
      while (this.pos < this.src.length && this.peek() !== '\n') this.advance();
      return done();
    }
    if ((ts || rs) && c === '/' && this.peek(1) === '*') {
      let depth = 0;
      while (this.pos < this.src.length) {
        if (this.peek() === '/' && this.peek(1) === '*') { depth++; this.advance(); this.advance(); }
        else if (this.peek() === '*' && this.peek(1) === '/') { depth--; this.advance(); this.advance(); if (depth === 0) break; }
        else this.advance();
      }
      return done();
    }
    if (rs && c === 'r' && (this.peek(1) === '"' || this.peek(1) === '#')) {
      // r"..." / r#"..."#（rust 原始字符串）
      this.advance();
      let hashes = '';
      while (this.peek() === '#') { hashes += this.advance(); }
      if (this.peek() === '"') {
        this.advance();
        const closer = '"' + hashes;
        while (this.pos < this.src.length && !this.src.startsWith(closer, this.pos)) this.advance();
        for (let i = 0; i < closer.length && this.pos < this.src.length; i++) this.advance();
        return done();
      }
      // r 开头但不是字符串（如普通标识符）——回退
      this.pos = startPos;
      return null;
    }
    if (c === '"' || c === "'" || (ts && c === '`')) {
      const q = c;
      this.advance();
      while (this.pos < this.src.length) {
        if (this.peek() === '\\') { this.advance(); this.advance(); continue; }
        if (this.peek() === q) { this.advance(); break; }
        // TS 模板串允许跨行
        this.advance();
      }
      return done();
    }
    return null;
  }

  scanRawBody(kind: string): string {
    // kind = 'native:<lang>' 或 'block'
    const lang = kind.startsWith('native:') ? kind.slice('native:'.length) : '';
    let depth = 1;
    let out = '';
    while (this.pos < this.src.length) {
      if (lang) {
        const skipped = this.skipLangStringOrComment(lang);
        if (skipped !== null) { out += skipped; continue; }
      }
      const c = this.peek();
      if (c === '{') { depth++; out += this.advance(); continue; }
      if (c === '}') {
        depth--;
        if (depth === 0) return out;
        out += this.advance();
        continue;
      }
      out += this.advance();
    }
    throw new LexError('原始代码区未闭合', this.line, this.col);
  }

  // ==========================================================================
  // 模式 A：block/static 体（原始文本 + {{ }} 编译期插值；字符串内大括号不计数）
  // ==========================================================================
  scanBlockBody(): RawPart[] {
    const parts: RawPart[] = [];
    let text = '';
    let depth = 1;
    const flush = () => {
      if (text.length > 0) {
        parts.push({ t: 'text', text, line: this.line, col: this.col });
        text = '';
      }
    };
    while (this.pos < this.src.length) {
      const c = this.peek();
      // 双引号字符串字面量：原样搬运（YAML/JSON 中的 "..."），其内大括号不参与计数。
      // 注意：单引号在 Markdown 文本中是撇号（user's），不作为定界符（模式 A 仅 "..."）。
      if (c === '"') {
        text += this.advance();
        while (this.pos < this.src.length) {
          if (this.peek() === '\\') { text += this.advance(); text += this.advance(); continue; }
          const ch = this.advance();
          text += ch;
          if (ch === '"') break;
        }
        continue;
      }
      // 插值 {{ expr }}
      if (c === '{' && this.peek(1) === '{') {
        this.advance(); this.advance();
        const eline = this.line, ecol = this.col;
        flush();
        let src = '';
        let d = 0;
        while (this.pos < this.src.length) {
          if (this.peek() === '"') { // 表达式内的字符串
            src += this.advance();
            while (this.pos < this.src.length) {
              if (this.peek() === '\\') { src += this.advance(); src += this.advance(); continue; }
              src += this.advance();
              if (src.endsWith('"')) break;
            }
            continue;
          }
          if (this.peek() === '{') { d++; src += this.advance(); continue; }
          if (this.peek() === '}') {
            if (d === 0 && this.peek(1) === '}') {
              this.advance(); this.advance();
              parts.push({ t: 'expr', src, line: eline, col: ecol });
              break;
            }
            d--; src += this.advance();
            continue;
          }
          src += this.advance();
        }
        continue;
      }
      if (c === '{') { depth++; text += this.advance(); continue; }
      if (c === '}') {
        depth--;
        if (depth === 0) { flush(); return parts; }
        text += this.advance();
        continue;
      }
      text += this.advance();
    }
    throw new LexError('资源块未闭合', this.line, this.col);
  }
}

export function tokenize(src: string, file = '<memory>'): Token[] {
  return new Lexer(src, file).tokenize();
}

export { JS_RESERVED };
