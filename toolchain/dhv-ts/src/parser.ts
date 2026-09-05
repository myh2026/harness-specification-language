// ============================================================================
// dhv-ts/src/parser.ts — HSL 递归下降解析器（BNF v1.2 §2/§3）
// ============================================================================

import { Token, Lexer } from './lexer';
import { TokenTree } from './ast';
import * as A from './ast';

export class ParseError extends Error {
  constructor(msg: string, public line: number, public col: number, public file: string) {
    super(`${msg} (${file}:${line}:${col})`);
  }
}

const ASSIGN_OPS = ['=', '+=', '-=', '*=', '/=', '%=', '&=', '|=', '^=', '<<=', '>>='];
const ITEM_KWS = ['fn', 'struct', 'enum', 'trait', 'impl', 'const', 'type', 'import', 'export', 'graph', 'static', 'async', 'mod', 'use'];

export class Parser {
  private i = 0;
  constructor(private toks: Token[], private file: string) {}

  // ---- 基础游标 ----
  private peek(off = 0): Token {
    return this.toks[this.i + off] ?? this.toks[this.toks.length - 1]!;
  }
  private next(): Token {
    const t = this.toks[this.i] ?? this.toks[this.toks.length - 1]!;
    this.i++;
    return t;
  }
  at(kind: string, text?: string): boolean {
    const t = this.peek();
    return t.kind === kind && (text === undefined || t.text === text);
  }
  eatIdentText(text: string): boolean {
    if (this.peek().kind === 'ident' && this.peek().text === text) { this.next(); return true; }
    return false;
  }
  expectIdentText(text: string): Token {
    if (!(this.peek().kind === 'ident' && this.peek().text === text)) this.err(`期望 "${text}"`);
    return this.next();
  }
  eatComma(): boolean {
    return this.eatP(',');
  }
  private atP(text: string): boolean {
    return this.peek().kind === 'punct' && this.peek().text === text;
  }
  private atKw(text: string): boolean {
    return this.peek().kind === 'kw' && this.peek().text === text;
  }
  private atIdent(text?: string): boolean {
    return this.peek().kind === 'ident' && (text === undefined || this.peek().text === text);
  }
  private eatP(text: string): boolean {
    if (this.atP(text)) { this.next(); return true; }
    return false;
  }
  private eatKw(text: string): boolean {
    if (this.atKw(text)) { this.next(); return true; }
    return false;
  }
  private expectP(text: string): Token {
    if (!this.atP(text)) this.err(`期望 "${text}"，得到 ${this.describe()}`);
    return this.next();
  }
  private expectKw(text: string): Token {
    if (!this.atKw(text)) this.err(`期望关键字 "${text}"，得到 ${this.describe()}`);
    return this.next();
  }
  private expectIdent(): Token {
    if (!this.atIdent() && this.peek().kind !== 'kw') this.err(`期望标识符，得到 ${this.describe()}`);
    // 关键字在字段/方法名等位置按原样接受（BNF 上下文允许时）
    if (!this.atIdent()) this.err(`期望标识符，得到 ${this.describe()}`);
    return this.next();
  }
  private describe(): string {
    const t = this.peek();
    return t.kind === 'eof' ? '文件结束' : `"${t.text}" (${t.kind})`;
  }
  private err(msg: string): never {
    const t = this.peek();
    throw new ParseError(msg, t.line, t.col, this.file);
  }
  private sp(t: { line: number; col: number }): A.Span {
    return { line: t.line, col: t.col, file: this.file };
  }

  // `>>` 拆分（L4：嵌套泛型）
  private splitGt(): void {
    const t = this.peek();
    if (t.kind === 'punct' && (t.text === '>>' || t.text === '>>=')) {
      const first = t.text.slice(0, 1);
      const rest = t.text.slice(1);
      const t1: Token = { ...t, text: first };
      const t2: Token = { ...t, text: rest, col: t.col + 1 };
      this.toks.splice(this.i, 1, t1, t2);
    }
  }

  // ---- 文件 ----
  parseFile(): A.File {
    const items: A.Item[] = [];
    let project: A.ProjectBlock | undefined;
    let scale: { mode: string; span: A.Span } | undefined;
    while (!this.at('eof')) {
      if (this.atKw('scale')) {
        const sp = this.sp(this.peek());
        this.next();
        this.expectP('=');
        const mode = this.next();
        this.expectP(';');
        scale = { mode: mode.text, span: sp };
        continue;
      }
      if (this.atKw('project')) {
        project = this.parseProject();
        continue;
      }
      items.push(this.parseItem());
    }
    return { file: this.file, items, project, scale };
  }

  parseProject(): A.ProjectBlock {
    const start = this.next(); // project
    this.expectP('{');
    const items: A.ProjectionItem[] = [];
    const rules: A.ProjectionRule[] = [];
    while (!this.atP('}')) {
      // §2.15（BNF v1.5）：rules { kind -> "path/{name}.ext" : lang } 投射规则组
      // （`rules` 为上下文标识符：仅 `rules {` 形态视为规则组；`rules -> ...` 仍为普通映射）
      if (this.peek().text === 'rules' && this.peek(1).text === '{') {
        this.next(); // rules
        this.expectP('{');
        while (!this.atP('}')) {
          const rsp = this.sp(this.peek());
          // 规则类型多为关键字（graph/fn/struct/...），kw 或 ident 均可
          const kindTok = this.next();
          if (kindTok.kind !== 'kw' && kindTok.kind !== 'ident') {
            this.err('rules 规则类型必须是标识符或关键字');
          }
          const kind = kindTok.text;
          this.expectP('->');
          const rulePathTok = this.next();
          if (rulePathTok.kind !== 'string') this.err('rules 路径模板必须是字符串');
          this.expectP(':');
          const rlang = this.next().text;
          this.eatP(',');
          rules.push({ kind, path: rulePathTok.value as string, lang: rlang, span: rsp });
        }
        this.expectP('}');
        continue;
      }
      const sp = this.sp(this.peek());
      const target: string[] = [];
      for (;;) {
        target.push(this.expectIdent().text);
        if (this.atP('::')) { this.next(); continue; }
        break;
      }
      this.expectP('->');
      const pathTok = this.next();
      if (pathTok.kind !== 'string') this.err('project 投射目标必须是字符串路径');
      this.expectP(':');
      const lang = this.next().text;
      this.eatP(',');
      items.push({ target, path: pathTok.value as string, lang, span: sp });
    }
    this.expectP('}');
    return { items, rules, span: this.sp(start) };
  }

  // ---- 属性 ----
  private parseOuterAttrs(): A.Attribute[] {
    const attrs: A.Attribute[] = [];
    while (this.atP('#')) {
      const hash = this.next();
      if (!this.atP('[')) this.err('属性期望 "["');
      this.next();
      const path: string[] = [];
      path.push(this.expectIdent().text);
      while (this.atP('::')) { this.next(); path.push(this.expectIdent().text); }
      let argsTokens: TokenTree[] = [];
      let argsRaw = '';
      if (this.atP('(')) {
        const tree = this.parseTokenTree();
        argsTokens = [tree];
        argsRaw = treeText(tree);
      } else if (this.atP('=')) {
        this.next();
        const lit = this.next();
        argsRaw = String(lit.value ?? lit.text);
      }
      this.expectP(']');
      attrs.push({ path, argsTokens, argsRaw });
      void hash;
    }
    return attrs;
  }

  // ---- 项 ----
  parseItem(): A.Item {
    const attrs = this.parseOuterAttrs();
    let exported = false;
    if (this.atKw('export')) { this.next(); exported = true; }

    if (this.atKw('struct')) return this.parseStruct(attrs, exported);
    if (this.atKw('enum')) return this.parseEnum(attrs, exported);
    if (this.atKw('trait')) return this.parseTrait(attrs, exported);
    if (this.atKw('impl')) return this.parseImpl(attrs, exported);
    if (this.atKw('fn')) return this.parseFn(attrs, exported);
    if (this.atKw('async')) return this.parseFn(attrs, exported);
    if (this.atKw('const')) return this.parseConst(attrs, exported);
    if (this.atKw('type')) return this.parseTypeAlias(exported);
    if (this.atKw('import')) return this.parseImport(exported);
    if (this.atKw('graph')) return this.parseGraph(attrs, exported);
    if (this.atKw('static') || this.atKw('block')) {
      // static/block NAME rawblock —— 词法器已把 name 并入 rawblock token
      const kw = this.next();
      const raw = this.peek();
      if (raw.kind !== 'rawblock') this.err('资源块缺少原始体');
      this.next();
      return {
        kind: 'blockres', name: raw.text, resKind: kw.text === 'block' ? 'block' : 'static',
        parts: raw.parts!, attrs, exported,
        span: this.sp(raw),
      };
    }
    if (this.atIdent('macro_rules') && this.peek(1).kind === 'punct' && this.peek(1).text === '!') {
      return this.parseMacroDef();
    }
    if (this.atIdent()) {
      // 语句级宏调用项：name ! tree ;
      if (this.peek(1).kind === 'punct' && this.peek(1).text === '!') {
        const sp = this.sp(this.peek());
        const path = [this.next().text];
        this.next(); // !
        const tree = this.parseTokenTree();
        this.eatP(';');
        return { kind: 'macrocallitem', path, tree, span: sp };
      }
    }
    this.err(`期望项定义，得到 ${this.describe()}`);
  }

  private parseGenerics(): string[] {
    const names: string[] = [];
    if (!this.atP('<')) return names;
    this.next();
    while (!this.atP('>')) {
      this.splitGt();
      if (this.atKw('const')) {
        this.next();
        names.push(this.expectIdent().text);
        this.expectP(':');
        this.parseType();
      } else {
        names.push(this.expectIdent().text);
        if (this.atP(':')) {
          this.next();
          // 跳过 bound：Type (+ Type)*
          for (;;) {
            this.parseType();
            if (this.atP('+')) { this.next(); continue; }
            break;
          }
        }
        if (this.atP('=')) { this.next(); this.parseType(); } // 默认类型
      }
      this.eatP(',');
    }
    this.expectP('>');
    return names;
  }

  private parseStruct(attrs: A.Attribute[], exported: boolean): A.Item {
    const kw = this.expectKw('struct');
    const name = this.expectIdent().text;
    this.parseGenerics();
    if (this.atP('(')) {
      // 元组结构体
      this.next();
      const tupleFields: A.HType[] = [];
      while (!this.atP(')')) {
        tupleFields.push(this.parseType());
        this.eatP(',');
      }
      this.expectP(')');
      this.expectP(';');
      return { kind: 'struct', name, fields: [], tupleFields, attrs, exported, span: this.sp(kw) };
    }
    if (this.eatP(';')) {
      return { kind: 'struct', name, fields: [], attrs, exported, span: this.sp(kw) };
    }
    this.expectP('{');
    const fields: A.FieldDef[] = [];
    while (!this.atP('}')) {
      const fattrs = this.parseOuterAttrs();
      const fname = this.expectIdent().text;
      this.expectP(':');
      const ty = this.parseType();
      this.eatP(',');
      fields.push({ name: fname, ty, attrs: fattrs });
    }
    this.expectP('}');
    return { kind: 'struct', name, fields, attrs, exported, span: this.sp(kw) };
  }

  private parseEnum(attrs: A.Attribute[], exported: boolean): A.Item {
    const kw = this.expectKw('enum');
    const name = this.expectIdent().text;
    this.parseGenerics();
    this.expectP('{');
    const variants: A.VariantDef[] = [];
    while (!this.atP('}')) {
      const vattrs = this.parseOuterAttrs();
      const vname = this.expectIdent().text;
      let fields: { named: A.FieldDef[] } | { tuple: A.HType[] } | undefined;
      if (this.atP('(')) {
        this.next();
        const tuple: A.HType[] = [];
        while (!this.atP(')')) {
          tuple.push(this.parseType());
          this.eatP(',');
        }
        this.expectP(')');
        fields = { tuple };
      } else if (this.atP('{')) {
        this.next();
        const named: A.FieldDef[] = [];
        while (!this.atP('}')) {
          const fname = this.expectIdent().text;
          this.expectP(':');
          const ty = this.parseType();
          this.eatP(',');
          named.push({ name: fname, ty, attrs: [] });
        }
        this.expectP('}');
        fields = { named };
      }
      let discr: number | bigint | undefined;
      if (this.atP('=')) {
        this.next();
        const t = this.next();
        discr = t.value as number | bigint;
      }
      this.eatP(',');
      variants.push({ name: vname, fields, discr, attrs: vattrs });
    }
    this.expectP('}');
    return { kind: 'enum', name, variants, attrs, exported, span: this.sp(kw) };
  }

  private parseTrait(attrs: A.Attribute[], exported: boolean): A.Item {
    const kw = this.expectKw('trait');
    const name = this.expectIdent().text;
    this.parseGenerics();
    const supers: string[] = [];
    if (this.atP(':')) {
      this.next();
      for (;;) {
        supers.push(this.parseTypePathSegs().join('::'));
        if (this.atP('+')) { this.next(); continue; }
        break;
      }
    }
    this.skipWhere();
    this.expectP('{');
    const items: A.TraitItem[] = [];
    while (!this.atP('}')) {
      const iattrs = this.parseOuterAttrs();
      if (this.atKw('fn') || this.atKw('async')) {
        const fn = this.parseFnDef();
        fn.fn.attrs = iattrs;
        if (fn.fn.body) items.push({ kind: 'fn', fn: fn.fn, name: fn.fn.name });
        else items.push({ kind: 'sig', fn: fn.fn, name: fn.fn.name });
      } else if (this.atKw('const')) {
        const c = this.parseConst(iattrs, false);
        const ci = c as unknown as { name: string; ty?: A.HType; value: A.Expr };
        items.push({ kind: 'const', name: ci.name, ty: ci.ty, value: ci.value });
      } else if (this.atKw('type')) {
        const ta = this.parseTypeAlias(false) as unknown as { name: string; value: A.HType };
        items.push({ kind: 'type', name: ta.name, ty: ta.value });
      } else {
        this.err(`trait 内不支持 ${this.describe()}`);
      }
    }
    this.expectP('}');
    return { kind: 'trait', name, supers, items, attrs, exported, span: this.sp(kw) };
  }

  private parseImpl(attrs: A.Attribute[], exported: boolean): A.Item {
    const kw = this.expectKw('impl');
    this.parseGenerics();
    // impl Trait for Type 或 impl Type
    let first = this.parseType();
    let traitSegs: string[] | undefined;
    let traitArgs: A.HType[] | undefined;
    let typeName: string;
    if (this.atKw('for')) {
      this.next();
      traitSegs = typeSegs(first);
      traitArgs = first.kind === 'path' ? first.args : undefined;
      const second = this.parseType();
      typeName = typeSegs(second).join('::');
    } else {
      typeName = typeSegs(first).join('::');
    }
    this.skipWhere();
    this.expectP('{');
    const methods: A.FnDef[] = [];
    while (!this.atP('}')) {
      const mattrs = this.parseOuterAttrs();
      if (this.atKw('fn') || this.atKw('async')) {
        const fn = this.parseFnDef();
        fn.fn.attrs = mattrs;
        methods.push(fn.fn);
      } else if (this.atKw('const')) {
        this.parseConst(mattrs, false); // impl 内 const：解析并忽略
      } else if (this.atKw('type')) {
        this.parseTypeAlias(false);
      } else {
        this.err(`impl 内不支持 ${this.describe()}`);
      }
    }
    this.expectP('}');
    return { kind: 'impl', traitSegs, traitArgs, typeName, methods, attrs, exported, span: this.sp(kw) };
  }

  private parseFn(attrs: A.Attribute[], exported: boolean): A.Item {
    const fn = this.parseFnDef();
    return { kind: 'fn', fn: fn.fn, attrs, exported, span: fn.span };
  }

  private parseFnDef(): { fn: A.FnDef; span: A.Span } {
    const start = this.peek();
    let isAsync = false;
    if (this.atKw('async')) { this.next(); isAsync = true; }
    this.expectKw('fn');
    const name = this.expectIdent().text;
    const generics = this.parseGenerics();
    this.expectP('(');
    const params: A.FnParam[] = [];
    while (!this.atP(')')) {
      const pattrs = this.parseOuterAttrs();
      // self 是上下文标识符（不在严格关键字表），此处按位置识别
      const peekText = (off: number): string | undefined => {
        const t = this.peek(off);
        // mut 是严格关键字（kw kind）；self 是上下文标识符（ident kind）
        return t.kind === 'ident' || t.kind === 'kw' ? t.text : undefined;
      };
      // BUGFIX(2025-09): BNF v1.5 SelfParam ::= "&" "mut"? "self" | "mut"? "self"
      // 此前未识别无引用符前缀的 `mut self` —— 它被误入常规参数分支
      // （parsePattern 把 self 解析为普通绑定），导致运行期「参数不足」。
      const isSelfHere = peekText(0) === 'self' || (peekText(0) === 'mut' && peekText(1) === 'self');
      const isRefSelf = this.atP('&') && (peekText(1) === 'self' || (peekText(1) === 'mut' && peekText(2) === 'self'));
      if (isSelfHere || isRefSelf) {
        let self: 'value' | 'mutvalue' | 'ref' | 'refmut';
        if (this.eatP('&')) {
          if (this.eatKw('mut') || this.eatIdentText('mut')) self = 'refmut';
          else self = 'ref';
          this.expectIdentText('self');
        } else {
          if (this.eatKw('mut') || this.eatIdentText('mut')) self = 'mutvalue';
          else self = 'value';
          this.expectIdentText('self');
        }
        params.push({ self, pat: { kind: 'binding', name: 'self', mut: false, span: this.sp(start) } });
      } else {
        let mut = false;
        if (this.atKw('mut')) { this.next(); mut = true; }
        const pat = this.parsePattern();
        let ty: A.HType | undefined;
        if (this.eatP(':')) ty = this.parseType();
        params.push({ mut, pat, ty });
      }
      this.eatP(',');
      void pattrs;
    }
    this.expectP(')');
    let ret: A.HType | undefined;
    if (this.atP('->')) {
      this.next();
      ret = this.parseType();
    }
    this.skipWhere();
    let body: Stmt2[] | undefined;
    if (!this.atP(';')) {
      body = this.parseBlockStmts();
    } else {
      this.next(); // ;
    }
    return {
      fn: { name, params, ret, body, isAsync, generics, span: this.sp(start), attrs: [] },
      span: this.sp(start),
    };
  }

  private parseConst(attrs: A.Attribute[], exported: boolean): A.Item {
    const kw = this.expectKw('const');
    const name = this.expectIdent().text;
    this.expectP(':');
    const ty = this.parseType();
    this.expectP('=');
    const value = this.parseExpr();
    this.expectP(';');
    return { kind: 'const', name, ty, value, attrs, exported, span: this.sp(kw) };
  }

  private parseTypeAlias(exported: boolean): A.Item {
    const kw = this.expectKw('type');
    const name = this.expectIdent().text;
    this.parseGenerics();
    this.expectP('=');
    const value = this.parseType();
    this.expectP(';');
    return { kind: 'typealias', name, value, exported, span: this.sp(kw) };
  }

  private parseImport(exported: boolean): A.Item {
    const kw = this.expectKw('import');
    let spec: A.ImportSpec;
    if (this.atP('*')) {
      this.next();
      this.expectKw('as');
      spec = { t: 'glob', alias: this.expectIdent().text };
    } else if (this.atP('{')) {
      this.next();
      const items: { name: string; alias?: string }[] = [];
      while (!this.atP('}')) {
        const name = this.expectIdent().text;
        let alias: string | undefined;
        if (this.eatKw('as')) alias = this.expectIdent().text;
        items.push({ name, alias });
        this.eatP(',');
      }
      this.expectP('}');
      spec = { t: 'items', items };
    } else {
      const name = this.expectIdent().text;
      let alias: string | undefined;
      if (this.eatKw('as')) alias = this.expectIdent().text;
      spec = { t: 'single', name, alias };
    }
    // `from` 是上下文关键字（BNF v1.3：不再是保留字，String::from 可用）
    if (!this.atIdent('from')) this.err('import 缺少 "from"');
    this.next(); // from
    const pathTok = this.next();
    if (pathTok.kind !== 'string') this.err('import 路径必须是字符串');
    this.expectP(';');
    return { kind: 'import', spec, path: pathTok.value as string, exported, span: this.sp(kw) };
  }

  private parseMacroDef(): A.Item {
    const sp = this.sp(this.peek());
    this.next(); // macro_rules
    this.expectP('!');
    const name = this.expectIdent().text;
    // 容错：macro_rules! shout! { ... } —— 定义名带尾 !（Rust 习惯迁移），按 shout 处理
    this.eatP('!');
    this.expectP('{');
    const rules: A.MacroRule[] = [];
    while (!this.atP('}')) {
      // ( matcher ) => { transcriber } ;
      const mTree = this.parseTokenTree();
      if (mTree.t !== 'delim' || mTree.open !== '(') this.err('宏规则 matcher 必须是 (...)');
      this.expectP('=>');
      const tTree = this.parseTokenTree();
      if (tTree.t !== 'delim') this.err('宏规则 transcriber 必须是定界树');
      this.eatP(';');
      rules.push({
        matcher: mTree.items,
        transcriber: tTree.items,
      });
    }
    this.expectP('}');
    return { kind: 'macrodef', name, rules, span: sp };
  }

  // ---- graph ----
  private parseGraph(attrs: A.Attribute[], exported: boolean): A.Item {
    const kw = this.expectKw('graph');
    const name = this.expectIdent().text;
    this.parseGenerics();
    const params: A.GraphParam[] = [];
    if (this.atP('(')) {
      this.next();
      while (!this.atP(')')) {
        let mut = false;
        if (this.atKw('mut')) { this.next(); mut = true; }
        const pname = this.expectIdent().text;
        this.expectP(':');
        const ty = this.parseType();
        params.push({ mut, name: pname, ty });
        this.eatP(',');
      }
      this.expectP(')');
    }
    let ret: A.HType | undefined;
    if (this.atP('->')) {
      this.next();
      ret = this.parseType();
    }
    this.skipWhere();
    this.expectP('{');
    const body: A.GraphStmt[] = [];
    while (!this.atP('}')) {
      if (this.atIdent('node')) {
        body.push({ t: 'node', decl: this.parseNodeDecl() });
      } else if (this.atKw('edge')) {
        body.push({ t: 'edge', decl: this.parseEdgeDecl() });
      } else if (this.atKw('let')) {
        body.push({ t: 'stmt', stmt: this.parseLet() });
      } else if (this.atP(';')) {
        this.next();
      } else if (this.atItemStart()) {
        body.push({ t: 'item', item: this.parseItem() });
      } else {
        body.push({ t: 'stmt', stmt: this.parseExprStmt() });
      }
    }
    this.expectP('}');
    return {
      kind: 'graph',
      graph: { name, params, ret, body, span: this.sp(kw) },
      attrs, exported, span: this.sp(kw),
    };
  }

  private parseNodeDecl(): A.NodeDecl {
    const kw = this.next(); // node
    let mut = false;
    if (this.atKw('mut')) { this.next(); mut = true; }
    const name = this.expectIdent().text;
    this.expectP(':');
    const ty = this.parseType();
    let init: A.Expr | undefined;
    if (this.eatP('=')) init = this.parseExpr();
    this.expectP(';');
    return { name, mut, ty, init, span: this.sp(kw) };
  }

  private parseEdgeDecl(): A.EdgeDecl {
    const kw = this.next(); // edge
    const endpoints: string[] = [];
    for (;;) {
      const segs: string[] = [];
      for (;;) {
        segs.push(this.expectIdent().text);
        if (this.atP('::')) { this.next(); continue; }
        break;
      }
      endpoints.push(segs.join('::'));
      if (this.atP('->')) { this.next(); continue; }
      break;
    }
    let guardExpr: A.Expr | undefined;
    let guardPattern: A.Pattern | undefined;
    if (this.atKw('on')) {
      this.next();
      // 先尝试模式（回溯），失败则按表达式
      const save = this.i;
      try {
        guardPattern = this.parsePattern();
        if (!this.atP(';') && !this.atIdent('with')) throw new Error('not pattern');
      } catch {
        this.i = save;
        guardPattern = undefined;
        guardExpr = this.parseExpr();
      }
    }
    const attrs: { name: string; value?: string | number | boolean }[] = [];
    if (this.atIdent('with')) {
      this.next();
      for (;;) {
        const name = this.expectIdent().text;
        let value: string | number | boolean | undefined;
        if (this.eatP('=')) {
          const t = this.next();
          value = t.value as string | number | boolean;
        }
        attrs.push({ name, value });
        if (!this.eatP(',')) break;
      }
    }
    this.expectP(';');
    return { endpoints, guardExpr, guardPattern, attrs, span: this.sp(kw) };
  }

  // ---- 语句 ----
  private atItemStart(): boolean {
    if (this.peek().kind === 'kw' && ITEM_KWS.includes(this.peek().text)) return true;
    if (this.atIdent('macro_rules') && this.peek(1).text === '!') return true;
    if (this.atIdent('block') && this.peek(1).kind === 'ident' && this.peek(2).kind === 'rawblock') return true;
    return false;
  }

  private parseBlockStmts(): A.Stmt[] {
    this.expectP('{');
    const stmts: A.Stmt[] = [];
    while (!this.atP('}')) {
      if (this.atP(';')) { this.next(); continue; }
      if (this.atKw('let')) { stmts.push(this.parseLet()); continue; }
      if (this.atItemStart()) { stmts.push({ kind: 'item', item: this.parseItem(), span: this.sp(this.peek()) }); continue; }
      stmts.push(this.parseExprStmt());
    }
    this.expectP('}');
    return stmts;
  }

  private parseLet(): A.Stmt {
    const kw = this.expectKw('let');
    let mut = false;
    if (this.atKw('mut')) { this.next(); mut = true; }
    const pat = this.parsePattern();
    let ty: A.HType | undefined;
    if (this.eatP(':')) ty = this.parseType();
    let init: A.Expr | undefined;
    if (this.eatP('=')) init = this.parseExpr();
    let elseBlock: A.Stmt[] | undefined;
    if (this.atKw('else')) {
      this.next();
      elseBlock = this.parseBlockStmts();
    }
    this.expectP(';');
    return { kind: 'let', mut, pat, ty, init, elseBlock, span: this.sp(kw) };
  }

  private parseExprStmt(): A.Stmt {
    const sp = this.sp(this.peek());
    const expr = this.parseExpr();
    if (this.eatP(';')) return { kind: 'expr', expr, hasSemi: true, span: sp };
    // 块尾无分号表达式 = 块的值（§2.12）
    if (this.atP('}')) return { kind: 'expr', expr, hasSemi: false, span: sp };
    // 带块表达式（if/match/loop/for/while/block/...）单独成句时无需分号（§2.12）
    if (exprIsWithBlock(expr)) return { kind: 'expr', expr, hasSemi: true, span: sp };
    this.err(`表达式语句缺少 ";"（得到 ${this.describe()}）`);
  }

  // ---- 表达式 ----
  parseExpr(): A.Expr {
    return this.parseAssign();
  }

  private parseAssign(): A.Expr {
    const lhs = this.parseRange();
    // 块表达式不能作赋值 LHS（Rust 语义）—— 否则 `while {...} = x` 会被错解析为赋值
    if (exprIsWithBlock(lhs)) return lhs;
    const t = this.peek();
    if (t.kind === 'punct' && ASSIGN_OPS.includes(t.text)) {
      this.next();
      const rhs = this.parseAssign();
      return { kind: 'assign', op: t.text, target: lhs, value: rhs, span: this.sp(t) };
    }
    return lhs;
  }

  /** 值语境 range：a..b / a..=b / n.. / ..n（BNF v1.5 §2.11.7，对齐 dhv） */
  private parseRange(): A.Expr {
    // ..n（前无操作数）
    if (this.atP('..') || this.atP('..=')) {
      const t = this.next();
      let inclusive = t.text === '..=';
      const hi = this.parseOr();
      return { kind: 'range', lo: undefined, hi, inclusive, span: this.sp(t) };
    }
    const lhs = this.parseOr();
    if (exprIsWithBlock(lhs)) return lhs;
    if (this.atP('..') || this.atP('..=')) {
      const t = this.next();
      let inclusive = t.text === '..=';
      // 检查是否有 hi 操作数（排除 struct 拆解 `..base` 和 spread `..` 消耗场景）
      // 值语境 range 的 RHS 必须是表达式起始符
      if (this.exprStarts()) {
        const hi = this.parseOr();
        return { kind: 'range', lo: lhs, hi, inclusive, span: lhs.span };
      }
      // n..（无 hi）
      return { kind: 'range', lo: lhs, hi: undefined, inclusive, span: lhs.span };
    }
    return lhs;
  }

  private binRhs(fn: () => A.Expr, ops: string[]): A.Expr | undefined {
    const t = this.peek();
    if (t.kind === 'punct' && ops.includes(t.text)) {
      this.next();
      return fn();
    }
    return undefined;
  }

  private parseOr(): A.Expr {
    let lhs = this.parseAnd();
    if (exprIsWithBlock(lhs)) return lhs;  // 块表达式不能作二元 LHS（Rust 语义）
    for (;;) {
      const rhs = this.binRhs(() => this.parseAnd(), ['||']);
      if (!rhs) return lhs;
      lhs = { kind: 'binary', op: '||', lhs, rhs, span: lhs.span };
    }
  }
  private parseAnd(): A.Expr {
    let lhs = this.parseBitOr();
    if (exprIsWithBlock(lhs)) return lhs;
    for (;;) {
      const rhs = this.binRhs(() => this.parseBitOr(), ['&&']);
      if (!rhs) return lhs;
      lhs = { kind: 'binary', op: '&&', lhs, rhs, span: lhs.span };
    }
  }
  private parseBitOr(): A.Expr {
    let lhs = this.parseBitXor();
    if (exprIsWithBlock(lhs)) return lhs;
    for (;;) {
      const rhs = this.binRhs(() => this.parseBitXor(), ['|']);
      if (!rhs) return lhs;
      lhs = { kind: 'binary', op: '|', lhs, rhs, span: lhs.span };
    }
  }
  private parseBitXor(): A.Expr {
    let lhs = this.parseBitAnd();
    if (exprIsWithBlock(lhs)) return lhs;
    for (;;) {
      const rhs = this.binRhs(() => this.parseBitAnd(), ['^']);
      if (!rhs) return lhs;
      lhs = { kind: 'binary', op: '^', lhs, rhs, span: lhs.span };
    }
  }
  private parseBitAnd(): A.Expr {
    let lhs = this.parseEquality();
    if (exprIsWithBlock(lhs)) return lhs;
    for (;;) {
      const rhs = this.binRhs(() => this.parseEquality(), ['&']);
      if (!rhs) return lhs;
      lhs = { kind: 'binary', op: '&', lhs, rhs, span: lhs.span };
    }
  }
  private parseEquality(): A.Expr {
    let lhs = this.parseRelational();
    if (exprIsWithBlock(lhs)) return lhs;
    for (;;) {
      const op = this.atP('==') ? '==' : this.atP('!=') ? '!=' : null;
      if (!op) return lhs;
      this.next();
      const rhs = this.parseRelational();
      lhs = { kind: 'binary', op, lhs, rhs, span: lhs.span };
    }
  }
  private parseRelational(): A.Expr {
    let lhs = this.parseShift();
    if (exprIsWithBlock(lhs)) return lhs;
    for (;;) {
      const t = this.peek();
      const op = ['<', '>', '<=', '>='].includes(t.text) && t.kind === 'punct' ? t.text : null;
      if (!op) return lhs;
      this.next();
      const rhs = this.parseShift();
      lhs = { kind: 'binary', op, lhs, rhs, span: lhs.span };
    }
  }
  private parseShift(): A.Expr {
    let lhs = this.parseAdditive();
    if (exprIsWithBlock(lhs)) return lhs;
    for (;;) {
      const t = this.peek();
      if (!(t.kind === 'punct' && ['<<', '>>'].includes(t.text))) return lhs;
      this.next();
      const rhs = this.parseAdditive();
      lhs = { kind: 'binary', op: t.text, lhs, rhs, span: lhs.span };
    }
  }
  private parseAdditive(): A.Expr {
    let lhs = this.parseMultiplicative();
    if (exprIsWithBlock(lhs)) return lhs;
    for (;;) {
      const t = this.peek();
      if (!(t.kind === 'punct' && ['+', '-'].includes(t.text))) return lhs;
      this.next();
      const rhs = this.parseMultiplicative();
      lhs = { kind: 'binary', op: t.text, lhs, rhs, span: lhs.span };
    }
  }
  private parseMultiplicative(): A.Expr {
    let lhs = this.parseCast();
    if (exprIsWithBlock(lhs)) return lhs;
    for (;;) {
      const t = this.peek();
      if (!(t.kind === 'punct' && ['*', '/', '%'].includes(t.text))) return lhs;
      this.next();
      const rhs = this.parseCast();
      lhs = { kind: 'binary', op: t.text, lhs, rhs, span: lhs.span };
    }
  }
  private parseCast(): A.Expr {
    let expr = this.parseUnary();
    if (exprIsWithBlock(expr)) return expr;  // 块表达式不能作 as 转换的 LHS
    while (this.atKw('as')) {
      const t = this.next();
      const ty = this.parseType();
      expr = { kind: 'cast', expr, ty, span: this.sp(t) };
    }
    return expr;
  }
  private parseUnary(): A.Expr {
    const t = this.peek();
    if (t.kind === 'punct' && ['-', '!', '*', '&'].includes(t.text)) {
      // &mut expr
      if (t.text === '&' && this.peek(1).kind === 'kw' && this.peek(1).text === 'mut') {
        this.next(); this.next();
        const operand = this.parseUnary();
        return { kind: 'unary', op: '&mut', operand, span: this.sp(t) };
      }
      this.next();
      const operand = this.parseUnary();
      return { kind: 'unary', op: t.text, operand, span: this.sp(t) };
    }
    return this.parsePostfix();
  }

  private parsePostfix(): A.Expr {
    let expr = this.parsePrimary();
    for (;;) {
      if (this.atP('?')) {
        const t = this.next();
        expr = { kind: 'try', expr, span: this.sp(t) };
        continue;
      }
      if (this.atP('.')) {
        this.next();
        if (this.atKw('await')) {
          this.next();
          expr = { kind: 'await', expr, span: expr.span };
          continue;
        }
        if (this.peek().kind === 'int') {
          const idx = this.next();
          expr = { kind: 'field', recv: expr, name: Number(idx.value), span: expr.span };
          continue;
        }
        const name = this.expectIdent().text;
        // turbofish：name ::<T>(args)
        let generics: A.HType[] | undefined;
        if (this.atP('::') && this.peek(1).kind === 'punct' && this.peek(1).text === '<') {
          this.next(); // ::
          generics = this.parseTypeArgs();
        }
        if (this.atP('(')) {
          this.next();
          const args: A.Expr[] = [];
          while (!this.atP(')')) {
            args.push(this.parseExpr());
            this.eatP(',');
          }
          this.expectP(')');
          expr = { kind: 'method', recv: expr, name, generics, args, span: expr.span };
        } else {
          expr = { kind: 'field', recv: expr, name, span: expr.span };
        }
        continue;
      }
      if (this.atP('(')) {
        // v0.2.57 修复（Bug #3 换行粘调用）：表达式结束后，若 `(` 位于新行，
        // 不再把 `(...)` 粘成对前一表达式的调用 —— 否则块型表达式语句
        // （if/match/loop/原生块）后跟元组字面量会被解析成
        // 「调用 if 结果」，运行期爆「不可调用的值：unit」：
        //   if cond { ... }      // 语句（自终结）
        //   (docs, versions)     // ← 此前被误粘为 if结果(docs, versions)
        // 合法多行调用不受影响：`(` 与被调者同行（foo(\n  arg\n)）依旧成立；
        // 以 `.` 开头的新行方法链、`[` 下标同理不受影响。
        const prevTok = this.i > 0 ? this.toks[this.i - 1]! : null;
        if (prevTok && this.peek().line > prevTok.line) return expr;
        this.next();
        const args: A.Expr[] = [];
        while (!this.atP(')')) {
          args.push(this.parseExpr());
          this.eatP(',');
        }
        this.expectP(')');
        expr = { kind: 'call', callee: expr, args, span: expr.span };
        continue;
      }
      if (this.atP('[')) {
        this.next();
        // v1.5：`..=` 单 token 识别（下标切片闭区间）
        if (this.atP('..') || this.atP('..=')) {
          let inclusive = false;
          if (this.atP('..=')) { this.next(); inclusive = true; }
          else { this.next(); if (this.atP('=')) { this.next(); inclusive = true; } }
          let hi: A.Expr | undefined;
          if (!this.atP(']')) hi = this.parseExpr();
          this.expectP(']');
          expr = { kind: 'slice', recv: expr, lo: undefined, hi, inclusive, span: expr.span };
          continue;
        }
        const index = this.parseExpr();
        if (this.atP('..') || this.atP('..=')) {
          let inclusive = false;
          if (this.atP('..=')) { this.next(); inclusive = true; }
          else { this.next(); if (this.atP('=')) { this.next(); inclusive = true; } }
          let hi: A.Expr | undefined;
          if (!this.atP(']')) hi = this.parseExpr();
          this.expectP(']');
          expr = { kind: 'slice', recv: expr, lo: index, hi, inclusive, span: expr.span };
          continue;
        }
        this.expectP(']');
        expr = { kind: 'index', recv: expr, index, span: expr.span };
        continue;
      }
      return expr;
    }
  }

  private parseTypeArgs(): A.HType[] {
    this.expectP('<');
    const args: A.HType[] = [];
    this.splitGt();
    while (!this.atP('>')) {
      if (this.peek().kind === 'int' || this.peek().kind === 'float') {
        this.next(); // const 泛型实参（忽略）
      } else if (this.atP('{')) {
        this.parseBlockStmts(); // const 泛型块实参（忽略）
      } else {
        args.push(this.parseType());
      }
      this.eatP(',');
      this.splitGt();
    }
    this.expectP('>');
    return args;
  }

  private parseExprNoStruct(): A.Expr {
    // 结构体字面量禁用位置（Rust 规则）：if/while 条件、match scrutinee、for 迭代对象
    this.noStructLit++;
    try {
      return this.parseExpr();
    } finally {
      this.noStructLit--;
    }
  }
  private noStructLit = 0;

  private parsePrimary(): A.Expr {
    const t = this.peek();
    const sp = this.sp(t);

    // 字面量
    if (t.kind === 'int') {
      this.next();
      return { kind: 'lit', lit: { t: 'int', v: t.value as number | bigint, suffix: t.suffix }, span: sp };
    }
    if (t.kind === 'float') {
      this.next();
      return { kind: 'lit', lit: { t: 'float', v: t.value as number, suffix: t.suffix }, span: sp };
    }
    if (t.kind === 'string') {
      this.next();
      return { kind: 'lit', lit: { t: 'str', v: t.value as string }, span: sp };
    }
    if (t.kind === 'rawstr') {
      this.next();
      return { kind: 'lit', lit: { t: 'str', v: t.value as string }, span: sp };
    }
    if (t.kind === 'char') {
      this.next();
      return { kind: 'lit', lit: { t: 'char', v: t.value as string }, span: sp };
    }
    if (this.atKw('true') || this.atKw('false')) {
      this.next();
      return { kind: 'lit', lit: { t: 'bool', v: t.text === 'true' }, span: sp };
    }

    // native 块：词法器把 lang 并入 nativeraw token
    if (this.atKw('native')) {
      this.next();
      const raw = this.next();
      if (raw.kind !== 'nativeraw') this.err('native 块缺少原始体');
      return { kind: 'native', lang: raw.lang ?? 'typescript', body: raw.text, span: sp };
    }

    // 路径 / 宏调用 / 结构体字面量
    if (t.kind === 'ident') {
      const segs: string[] = [];
      for (;;) {
        segs.push(this.expectIdent().text);
        if (this.atP('::')) {
          this.next();
          continue;
        }
        break;
      }
      // 宏调用：path ! delim
      if (this.atP('!') && (this.peek(1).kind === 'punct' && ['(', '[', '{'].includes(this.peek(1).text))) {
        this.next(); // !
        const tree = this.parseTokenTree();
        return { kind: 'macro', path: segs, tree, span: sp };
      }
      // 结构体字面量启发式：'{' 后是 ident(:|,|}) 或 '..' 或 '}'；禁用位置除外
      if (this.atP('{') && this.noStructLit === 0 && this.looksLikeStructLiteral()) {
        this.next(); // {
        const fields: A.StructExprField[] = [];
        while (!this.atP('}')) {
          if (this.atP('..')) {
            this.next();
            const base = this.parseExpr();
            fields.push({ name: '', base });
          } else {
            const fname = this.expectIdent().text;
            if (this.eatP(':')) {
              const value = this.parseExpr();
              fields.push({ name: fname, value });
            } else {
              fields.push({ name: fname }); // 简写
            }
          }
          this.eatP(',');
        }
        this.expectP('}');
        return { kind: 'struct', segs, fields, span: sp };
      }
      return { kind: 'path', segs, span: sp };
    }

    // 分组 / 元组 / unit
    if (this.atP('(')) {
      this.next();
      if (this.atP(')')) {
        this.next();
        return { kind: 'unit', span: sp };
      }
      const first = this.parseExpr();
      if (this.atP(',')) {
        const items: A.Expr[] = [first];
        while (this.eatP(',')) {
          if (this.atP(')')) break;
          items.push(this.parseExpr());
        }
        this.expectP(')');
        return { kind: 'tuple', items, span: sp };
      }
      this.expectP(')');
      return first; // 分组
    }

    // 数组
    if (this.atP('[')) {
      this.next();
      if (this.atP(']')) { this.next(); return { kind: 'array', items: [], span: sp }; }
      const first = this.parseExpr();
      if (this.atP(';')) {
        this.next();
        const count = this.parseExpr();
        this.expectP(']');
        return { kind: 'arrayrep', value: first, count, span: sp };
      }
      const items: A.Expr[] = [first];
      while (this.eatP(',')) {
        if (this.atP(']')) break;
        items.push(this.parseExpr());
      }
      this.expectP(']');
      return { kind: 'array', items, span: sp };
    }

    // 块
    if (this.atP('{')) {
      const stmts = this.parseBlockStmts();
      return { kind: 'block', stmts, span: sp };
    }

    // async 块
    if (this.atKw('async')) {
      this.next();
      this.eatKw('move');
      const stmts = this.parseBlockStmts();
      return { kind: 'asyncblock', stmts, span: sp };
    }

    // 闭包
    if (this.atKw('move') || this.atP('||') || (this.atP('|'))) {
      let isAsync = false;
      if (this.eatKw('move')) { /* move 标记：捕获语义与 JS 一致 */ }
      if (this.atKw('async')) { this.next(); isAsync = true; }
      const params: { pat: A.Pattern; ty?: A.HType }[] = [];
      if (this.eatP('||')) {
        // 无参闭包
      } else {
        this.expectP('|');
        while (!this.atP('|')) {
          const pat = this.parseParamPattern();
          let ty: A.HType | undefined;
          if (this.eatP(':')) ty = this.parseType();
          params.push({ pat, ty });
          this.eatP(',');
        }
        this.expectP('|');
      }
      let ret: A.HType | undefined;
      if (this.atP('->')) {
        this.next();
        ret = this.parseType();
      }
      if (this.atP('{')) {
        const stmts = this.parseBlockStmts();
        return { kind: 'closure', params, ret, body: { kind: 'block', stmts, span: sp }, isAsync, span: sp };
      }
      const body = this.parseExpr();
      return { kind: 'closure', params, ret, body, isAsync, span: sp };
    }

    // if / if let
    if (this.atKw('if')) {
      this.next();
      if (this.atKw('let')) {
        this.next();
        const pat = this.parsePattern();
        this.expectP('=');
        const expr = this.parseExprNoStruct();
        const then = this.parseBlockOrExpr();
        let els: A.Expr | undefined;
        if (this.atKw('else')) {
          this.next();
          els = this.parseElseBranch();
        }
        return { kind: 'iflet', pat, expr, then, els, span: sp };
      }
      const cond = this.parseExprNoStruct();
      const then = this.parseBlockOrExpr();
      let els: A.Expr | undefined;
      if (this.atKw('else')) {
        this.next();
        els = this.parseElseBranch();
      }
      return { kind: 'if', cond, then, els, span: sp };
    }

    // match
    if (this.atKw('match')) {
      this.next();
      const expr = this.parseExprNoStruct();
      this.expectP('{');
      const arms: A.MatchArm[] = [];
      while (!this.atP('}')) {
        this.parseOuterAttrs();
        const asp = this.sp(this.peek());
        const pattern = this.parsePattern();
        let guard: A.Expr | undefined;
        if (this.atKw('if')) {
          this.next();
          guard = this.parseExpr();
        }
        this.expectP('=>');
        let body: A.Expr;
        if (this.atP('{')) {
          body = { kind: 'block', stmts: this.parseBlockStmts(), span: asp };
        } else {
          body = this.parseExpr();
        }
        this.eatP(',');
        arms.push({ pattern, guard, body, span: asp });
      }
      this.expectP('}');
      return { kind: 'match', expr, arms, span: sp };
    }

    // 循环族
    let label: string | undefined;
    if (t.kind === 'label' && this.peek(1).kind === 'punct' && this.peek(1).text === ':') {
      label = t.text;
      this.next(); this.next();
      return this.parseLoopLike(label, sp);
    }
    if (this.atKw('loop') || this.atKw('while') || this.atKw('for')) {
      return this.parseLoopLike(undefined, sp);
    }

    if (this.atKw('break')) {
      this.next();
      let lbl: string | undefined;
      if (this.peek().kind === 'label') lbl = this.next().text;
      let value: A.Expr | undefined;
      if (this.exprStarts()) value = this.parseExpr();
      return { kind: 'break', label: lbl, value, span: sp };
    }
    if (this.atKw('continue')) {
      this.next();
      let lbl: string | undefined;
      if (this.peek().kind === 'label') lbl = this.next().text;
      return { kind: 'continue', label: lbl, span: sp };
    }
    if (this.atKw('return')) {
      this.next();
      let value: A.Expr | undefined;
      if (this.exprStarts()) value = this.parseExpr();
      return { kind: 'return', value, span: sp };
    }

    this.err(`无法解析的表达式起点 ${this.describe()}`);
  }

  private parseLoopLike(label: string | undefined, sp: A.Span): A.Expr {
    if (this.atKw('loop')) {
      this.next();
      const body = this.parseBlockOrExpr();
      return { kind: 'loop', label, body, span: sp };
    }
    if (this.atKw('while')) {
      this.next();
      if (this.atKw('let')) {
        this.next();
        const pat = this.parsePattern();
        this.expectP('=');
        const expr = this.parseExprNoStruct();
        const body = this.parseBlockOrExpr();
        return { kind: 'whilelet', label, pat, expr, body, span: sp };
      }
      const cond = this.parseExprNoStruct();
      const body = this.parseBlockOrExpr();
      return { kind: 'while', label, cond, body, span: sp };
    }
    if (this.atKw('for')) {
      this.next();
      const pat = this.parsePattern();
      this.expectKw('in');
      const iter = this.parseExprNoStruct();
      let range: { lo: A.Expr; hi: A.Expr; inclusive: boolean } | undefined;
      // v1.5：`..=` 由词法层产出单 token（PUNCTS3），需直接识别（此前 `a..=b` 解析失败）
      if (this.atP('..') || this.atP('..=')) {
        let inclusive = false;
        if (this.atP('..=')) { this.next(); inclusive = true; }
        else { this.next(); if (this.atP('=')) { this.next(); inclusive = true; } }
        const hi = this.parseExpr();
        range = { lo: iter, hi, inclusive };
      }
      const body = this.parseBlockOrExpr();
      return { kind: 'for', label, pat, iter, range, body, span: sp };
    }
    this.err('期望循环表达式');
  }

  private parseElseBranch(): A.Expr {
    if (this.atKw('if')) {
      return this.parsePrimary(); // else if / else if let（primary 处理 if）
    }
    return this.parseBlockOrExpr();
  }

  private parseBlockOrExpr(): A.Expr {
    const sp = this.sp(this.peek());
    if (this.atP('{')) {
      return { kind: 'block', stmts: this.parseBlockStmts(), span: sp };
    }
    this.err('期望 "{" 块');
  }

  private exprStarts(): boolean {
    const t = this.peek();
    if (['int', 'float', 'string', 'rawstr', 'char', 'ident', 'label'].includes(t.kind)) return true;
    if (t.kind === 'kw' && ['true', 'false'].includes(t.text)) return true;
    if (t.kind === 'kw') return false; // 其余关键字不能开始表达式
    if (t.kind === 'punct' && ['(', '[', '{', '-', '!', '&', '*'].includes(t.text)) return true;
    return false;
  }

  private looksLikeStructLiteral(): boolean {
    // 当前 token 是 '{'；看内部
    const t1 = this.peek(1);
    if (t1.kind === 'punct' && t1.text === '}') return true; // Foo {}
    if (t1.kind === 'punct' && t1.text === '..') return true; // Foo { ..base }
    if (t1.kind === 'ident') {
      const t2 = this.peek(2);
      if (t2.kind === 'punct' && [':', ','].includes(t2.text)) return true; // Foo { x: .. } / Foo { x, .. }
      if (t2.kind === 'punct' && t2.text === '}') return true; // Foo { x }
    }
    return false;
  }

  // ---- 类型 ----
  parseType(): A.HType {
    const t = this.peek();
    const sp = this.sp(t);
    if (this.atP('(')) {
      this.next();
      if (this.atP(')')) { this.next(); return { kind: 'tuple', items: [] }; }
      const first = this.parseType();
      if (this.atP(',')) {
        const items: A.HType[] = [first];
        while (this.eatP(',')) {
          if (this.atP(')')) break;
          items.push(this.parseType());
        }
        this.expectP(')');
        return { kind: 'tuple', items };
      }
      this.expectP(')');
      return { kind: 'paren', inner: first };
    }
    if (this.atP('&')) {
      this.next();
      let mut = false;
      if (this.eatKw('mut')) mut = true;
      return { kind: 'ref', mut, inner: this.parseType() };
    }
    if (this.atP('[')) {
      this.next();
      const elem = this.parseType();
      if (this.atP(';')) {
        this.next();
        // const 长度：字面量或块
        if (this.peek().kind === 'int' || this.peek().kind === 'float') this.next();
        else if (this.atP('{')) this.parseBlockStmts();
        else this.parseExpr();
        this.expectP(']');
        return { kind: 'array', elem };
      }
      this.expectP(']');
      return { kind: 'slice', elem };
    }
    if (this.atP('!')) {
      this.next();
      return { kind: 'never' };
    }
    if (this.atP('_')) {
      this.next();
      return { kind: 'infer' };
    }
    if (this.atKw('dyn')) {
      this.next();
      const bounds: string[] = [];
      for (;;) {
        bounds.push(this.parseTypePathSegs().join('::'));
        if (this.atP('+')) { this.next(); continue; }
        break;
      }
      return { kind: 'dyn', bounds };
    }
    if (this.atKw('impl')) {
      this.next();
      const bounds: string[] = [];
      for (;;) {
        bounds.push(this.parseTypePathSegs().join('::'));
        if (this.atP('+')) { this.next(); continue; }
        break;
      }
      return { kind: 'implt', bounds };
    }
    if (this.atKw('fn')) {
      this.next();
      this.expectP('(');
      const params: A.HType[] = [];
      while (!this.atP(')')) {
        // fn 指针参数可带名字：name: Type 或裸 Type
        if (this.peek().kind === 'ident' && this.peek(1).kind === 'punct' && this.peek(1).text === ':') {
          this.next(); this.next();
        }
        params.push(this.parseType());
        this.eatP(',');
      }
      this.expectP(')');
      let ret: A.HType | undefined;
      if (this.atP('->')) { this.next(); ret = this.parseType(); }
      return { kind: 'fnptr', params, ret };
    }
    if (t.kind === 'ident') {
      const p = this.parseTypePath();
      return p.args && p.args.length > 0 ? { kind: 'path', segs: p.segs, args: p.args } : { kind: 'path', segs: p.segs };
    }
    this.err(`无法解析的类型 ${this.describe()}`);
  }

  /** 类型路径：段名 + 末段泛型实参（From<A> 的 args 由此保留 —— ? 的 From 转换依赖） */
  private parseTypePath(): { segs: string[]; args?: A.HType[] } {
    if (this.atP('::')) this.next();
    const segs: string[] = [];
    let args: A.HType[] | undefined;
    for (;;) {
      segs.push(this.expectIdent().text);
      if (this.atP('<')) {
        args = this.parseTypeArgs();
      }
      if (this.atP('::')) {
        this.next();
        continue;
      }
      break;
    }
    return { segs, args };
  }

  private parseTypePathSegs(): string[] {
    return this.parseTypePath().segs;
  }

  private skipWhere(): void {
    if (!this.atKw('where')) return;
    this.next();
    let depth = 0;
    while (this.pos < this.toks.length) {
      const t = this.peek();
      if (depth === 0 && (this.atP('{') || this.atP(';'))) return;
      if (t.kind === 'punct' && ['(', '[', '{'].includes(t.text)) depth++;
      if (t.kind === 'punct' && [')', ']', '}'].includes(t.text)) depth--;
      this.next();
    }
  }
  private get pos(): number {
    return this.i;
  }

  // ---- 模式 ----
  // 闭包参数模式：禁用顶层 or-（`|` 与闭包定界歧义，与 Rust 一致；需 or 请加括号）
  parseParamPattern(): A.Pattern {
    return this.parseSinglePattern();
  }

  parsePattern(): A.Pattern {
    const alts: A.Pattern[] = [this.parseSinglePattern()];
    while (this.atP('|')) {
      this.next();
      alts.push(this.parseSinglePattern());
    }
    if (alts.length === 1) return alts[0]!;
    return { kind: 'or', alts, span: alts[0]!.span };
  }

  private parseSinglePattern(): A.Pattern {
    const t = this.peek();
    const sp = this.sp(t);

    if (this.atP('_')) {
      this.next();
      return { kind: 'wildcard', span: sp };
    }
    if (this.atP('..')) {
      this.next();
      return { kind: 'rest', span: sp };
    }
    if (t.kind === 'int' || t.kind === 'float' || t.kind === 'string' || t.kind === 'rawstr' || t.kind === 'char' || this.atKw('true') || this.atKw('false')) {
      const lit = this.next();
      let value: number | bigint | string | boolean;
      if (lit.kind === 'int' || lit.kind === 'float') value = lit.value as number | bigint;
      else if (lit.kind === 'string' || lit.kind === 'rawstr' || lit.kind === 'char') value = lit.value as string;
      else value = lit.text === 'true';
      // 范围模式
      if (this.atP('..') || this.atP('..=')) {
        const inclusive = this.next().text === '..=';
        const hi = this.parseRangeBound();
        return { kind: 'range', lo: { kind: 'literal', value, span: sp }, hi, inclusive, span: sp };
      }
      return { kind: 'literal', value, span: sp };
    }
    if (this.atP('-') && (this.peek(1).kind === 'int' || this.peek(1).kind === 'float')) {
      this.next();
      const lit = this.next();
      const value = -(lit.value as number);
      if (this.atP('..') || this.atP('..=')) {
        const inclusive = this.next().text === '..=';
        const hi = this.parseRangeBound();
        return { kind: 'range', lo: { kind: 'literal', value, span: sp }, hi, inclusive, span: sp };
      }
      return { kind: 'literal', value, span: sp };
    }
    if (this.atKw('mut')) {
      this.next();
      const name = this.expectIdent().text;
      let sub: A.Pattern | undefined;
      if (this.eatP('@')) sub = this.parseSinglePattern();
      return { kind: 'binding', name, mut: true, sub, span: sp };
    }
    if (t.kind === 'ident') {
      const segs: string[] = [];
      for (;;) {
        segs.push(this.expectIdent().text);
        if (this.atP('::')) { this.next(); continue; }
        break;
      }
      // 路径模式：枚举变体 / 常量
      if (this.atP('{')) {
        this.next();
        const fields: { name: string; pat: A.Pattern }[] = [];
        let rest = false;
        while (!this.atP('}')) {
          if (this.atP('..')) { this.next(); rest = true; this.eatP(','); continue; }
          if (this.atKw('mut')) {
            this.next();
            const fname = this.expectIdent().text;
            fields.push({ name: fname, pat: { kind: 'binding', name: fname, mut: true, span: sp } });
          } else {
            const fname = this.expectIdent().text;
            if (this.eatP(':')) {
              fields.push({ name: fname, pat: this.parsePattern() });
            } else {
              fields.push({ name: fname, pat: { kind: 'binding', name: fname, mut: false, span: sp } });
            }
          }
          this.eatP(',');
        }
        this.expectP('}');
        return { kind: 'struct', segs, fields, rest, span: sp };
      }
      if (this.atP('(')) {
        this.next();
        const items: A.Pattern[] = [];
        let rest = false;
        while (!this.atP(')')) {
          if (this.atP('..')) { this.next(); rest = true; this.eatP(','); continue; }
          items.push(this.parsePattern());
          this.eatP(',');
        }
        this.expectP(')');
        return { kind: 'path', segs, sub: { kind: 'tuple', items, rest }, span: sp };
      }
      // 范围模式上界处理：路径字面量作边界
      if (this.atP('..') || this.atP('..=')) {
        const inclusive = this.next().text === '..=';
        const hi = this.parseRangeBound();
        return { kind: 'range', lo: { kind: 'path', segs, span: sp }, hi, inclusive, span: sp };
      }
      // 单段非路径 → 绑定；多段 → 枚举/常量路径模式
      if (segs.length === 1) {
        let sub: A.Pattern | undefined;
        if (this.eatP('@')) sub = this.parseSinglePattern();
        return { kind: 'binding', name: segs[0]!, mut: false, sub, span: sp };
      }
      return { kind: 'path', segs, span: sp };
    }
    if (this.atP('(')) {
      this.next();
      if (this.atP(')')) { this.next(); return { kind: 'tuple', items: [], rest: false, span: sp }; }
      const items: A.Pattern[] = [this.parsePattern()];
      let rest = false;
      while (this.eatP(',')) {
        if (this.atP(')')) break;
        if (this.atP('..')) { this.next(); rest = true; continue; }
        items.push(this.parsePattern());
      }
      this.expectP(')');
      if (items.length === 1 && !rest) {
        return { kind: 'tuple', items, rest: false, span: sp }; // 单元素需尾逗号；宽松处理
      }
      return { kind: 'tuple', items, rest, span: sp };
    }
    this.err(`无法解析的模式 ${this.describe()}`);
  }

  private parseRangeBound(): A.Pattern {
    const t = this.peek();
    const sp = this.sp(t);
    if (this.atP('-') && (this.peek(1).kind === 'int' || this.peek(1).kind === 'float')) {
      this.next();
      const lit = this.next();
      return { kind: 'literal', value: -(lit.value as number), span: sp };
    }
    if (t.kind === 'int' || t.kind === 'float') {
      this.next();
      return { kind: 'literal', value: t.value as number | bigint, span: sp };
    }
    if (t.kind === 'ident') {
      const segs: string[] = [];
      for (;;) {
        segs.push(this.expectIdent().text);
        if (this.atP('::')) { this.next(); continue; }
        break;
      }
      return { kind: 'path', segs, span: sp };
    }
    this.err('范围模式边界必须是字面量或路径');
  }

  // ---- token 树（宏 / 属性参数）----
  parseTokenTree(): TokenTree {
    const t = this.peek();
    if (t.kind === 'punct' && ['(', '[', '{'].includes(t.text)) {
      const open = t.text;
      const close = open === '(' ? ')' : open === '[' ? ']' : '}';
      this.next();
      const items: TokenTree[] = [];
      while (!this.atP(close)) {
        if (this.at('eof')) this.err('定界树未闭合');
        items.push(this.parseTokenTree());
      }
      this.next(); // close
      return { t: 'delim', open, close, items };
    }
    this.next();
    // v0.2.56：保留 suffix（值级对拍宏 token 需要与 dhv Token::Literal 的
    // Int{suffix} 对齐 —— 此前 TokenTree 丢弃后缀）
    return { t: 'tok', tok: { kind: t.kind, text: t.text, value: t.value, suffix: (t as { suffix?: string }).suffix, line: t.line, col: t.col } };
  }
}

type Stmt2 = A.Stmt;

function exprIsWithBlock(e: A.Expr): boolean {
  return ['block', 'asyncblock', 'if', 'iflet', 'match', 'loop', 'while', 'whilelet', 'for', 'native'].includes(e.kind);
}

export function treeText(tree: TokenTree): string {
  if (tree.t === 'tok') return tree.tok.text;
  return tree.items.map(treeText).join(' ');
}

function typeSegs(t: A.HType): string[] {
  if (t.kind === 'path') return t.segs;
  if (t.kind === 'paren') return typeSegs(t.inner);
  return [JSON.stringify(t)];
}

export function parseExprsFromTokens(toks: Token[], file: string): A.Expr[] {
  const p = new Parser(toks, file);
  const out: A.Expr[] = [];
  while (!p.at('eof')) {
    out.push(p.parseExpr());
    p.eatComma();
  }
  return out;
}

export function parseFileSource(src: string, file: string): A.File {
  const toks = new Lexer(src, file).tokenize();
  return new Parser(toks, file).parseFile();
}
