// ============================================================================
// dhv-ts/src/macro.ts — macro_rules! token 级展开（BNF §2.13）
// ----------------------------------------------------------------------------
// 在 parse 之前对 token 流做一遍展开：宏定义文本先于使用（Rust 惯例）。
// 支持 frag spec：ident / literal / tt / ty / path / expr / stmt / block / item / meta
// 重复语法 $(...)sep* 支持基础形态。
// ============================================================================

import { Token } from './lexer';

const BUILTIN_MACROS = new Set(['format', 'vec', 'println', 'print', 'eprintln', 'panic', 'assert', 'assert_eq', 'dbg']);

interface MacroRule2 {
  matcher: Token[];
  transcriber: Token[];
}

function isDelim(t: Token | undefined): boolean {
  return t?.kind === 'punct' && ['(', ')', '[', ']', '{', '}'].includes(t.text);
}

function matchDelim(toks: Token[], openIdx: number): number {
  const open = toks[openIdx]!.text;
  const close = open === '(' ? ')' : open === '[' ? ']' : '}';
  let depth = 0;
  for (let i = openIdx; i < toks.length; i++) {
    const t = toks[i]!;
    if (t.kind === 'punct' && t.text === open) depth++;
    else if (t.kind === 'punct' && t.text === close) {
      depth--;
      if (depth === 0) return i;
    }
  }
  return -1;
}

const FRAG_SPECS = new Set(['ident', 'path', 'expr', 'ty', 'pat', 'stmt', 'block', 'item', 'literal', 'tt', 'meta']);

/** 展开 token 流中的用户宏；返回新 token 流（macro_rules 定义被移除） */
export function expandMacros(toks: Token[], file: string): Token[] {
  const rules = new Map<string, MacroRule2[]>();
  const out: Token[] = [];
  let i = 0;
  let guard = 0;
  while (i < toks.length) {
    if (guard++ > 1_000_000) throw new Error(`宏展开失控（${file}）`);
    const t = toks[i]!;
    // macro_rules ! name [!] { ... }（定义名带尾 ! 为容错形态，与 parser.parseMacroDef 同步）
    if (t.kind === 'ident' && t.text === 'macro_rules' && toks[i + 1]?.kind === 'punct' && toks[i + 1].text === '!' && toks[i + 2]?.kind === 'ident') {
      let braceIdx = i + 3;
      if (toks[braceIdx]?.kind === 'punct' && toks[braceIdx].text === '!') braceIdx++;
      if (toks[braceIdx]?.kind === 'punct' && toks[braceIdx].text === '{') {
        const name = toks[i + 2]!.text;
        const close = matchDelim(toks, braceIdx);
        if (close < 0) throw new Error(`macro_rules! ${name} 未闭合（${file}）`);
        const inner = toks.slice(braceIdx + 1, close);
        rules.set(name, parseMacroRules(inner, file));
        i = close + 1;
        continue;
      }
    }
    // name ! ( ... ) —— 用户宏调用（内建宏留给解释器）
    if (t.kind === 'ident' && !BUILTIN_MACROS.has(t.text) && rules.has(t.text)
      && toks[i + 1]?.kind === 'punct' && toks[i + 1].text === '!' && isDelim(toks[i + 2]) && ['(', '['].includes(toks[i + 2]!.text)) {
      const name = t.text;
      const close = matchDelim(toks, i + 2);
      if (close < 0) throw new Error(`宏调用 ${name}! 未闭合（${file}）`);
      const callItems = toks.slice(i + 3, close);
      const expanded = applyMacro(rules.get(name)!, callItems, file);
      // 展开结果递归处理（宏产生宏调用）
      out.push(...expandMacros(expanded, file));
      i = close + 1;
      continue;
    }
    out.push(t);
    i++;
  }
  return out;
}

function parseMacroRules(inner: Token[], file: string): MacroRule2[] {
  const rules: MacroRule2[] = [];
  let i = 0;
  while (i < inner.length) {
    if (inner[i]!.kind === 'punct' && inner[i]!.text === ';') { i++; continue; }
    if (inner[i]!.kind !== 'punct' || inner[i]!.text !== '(') throw new Error(`宏规则期望 "("（${file}）`);
    const mClose = matchDelim(inner, i);
    const matcher = inner.slice(i + 1, mClose);
    i = mClose + 1;
    if (inner[i]!.text !== '=>' ) throw new Error(`宏规则期望 "=>"（${file}）`);
    i++;
    if (!isDelim(inner[i]) || !['{', '('].includes(inner[i]!.text)) throw new Error(`宏规则 transcriber 期望定界树（${file}）`);
    const tClose = matchDelim(inner, i);
    const transcriber = inner.slice(i + 1, tClose);
    i = tClose + 1;
    rules.push({ matcher, transcriber });
  }
  return rules;
}

interface Captures {
  vars: Map<string, Token[]>;
  reps: Map<string, Token[][]>;
}

/** 尝试用 matcher 匹配 call tokens；成功返回捕获，失败返回 null */
function matchTokens(matcher: Token[], call: Token[]): Captures | null {
  const caps: Captures = { vars: new Map(), reps: new Map() };
  let mi = 0, ci = 0;
  const matchesTok = (m: Token, c: Token): boolean => m.text === c.text && m.kind === c.kind;

  const trySeq = (mSeq: Token[], cStart: number): { caps: Captures; next: number } | null => {
    // 序列匹配（独立捕获空间；重复外层合并）
    const local: Captures = { vars: new Map(), reps: new Map() };
    let m = 0, c = cStart;
    while (m < mSeq.length) {
      // 重复：$ ( ... ) op
      if (mSeq[m]!.kind === 'punct' && mSeq[m]!.text === '$' && mSeq[m + 1]?.kind === 'punct' && mSeq[m + 1].text === '(') {
        const repClose = matchDelim(mSeq, m + 1);
        if (repClose < 0) return null;
        const body = mSeq.slice(m + 2, repClose);
        let after = repClose + 1;
        const op = mSeq[after]?.text ?? '*';
        if (op === '*' || op === '+' || op === '?') after++;
        // 分隔符：op 后面的 token（若不是 frag/宏语法）
        let sep: Token | null = null;
        if (mSeq[after] && !(mSeq[after]!.kind === 'punct' && mSeq[after]!.text === '$') && !isDelim(mSeq[after]) === false) {
          // 分隔符只可能是普通 token
        }
        if (mSeq[after] && mSeq[after]!.kind !== 'punct') { sep = mSeq[after]!; after++; }
        else if (mSeq[after] && mSeq[after]!.kind === 'punct' && !['$', '(', ')', '{', '}', '[', ']'].includes(mSeq[after]!.text) && !(after + 1 < mSeq.length && mSeq[after + 1]?.kind === 'punct' && mSeq[after + 1].text === ':')) {
          sep = mSeq[after]!; after++;
        }
        // 收集重复
        const iterCaps: Captures[] = [];
        for (;;) {
          if (op === '?' && iterCaps.length >= 1) break;
          // 分隔符检查
          if (iterCaps.length > 0 && sep) {
            if (call[c] && matchesTok(sep, call[c]!)) c++;
            else break;
          }
          const r = trySeq(body, c);
          if (!r) break;
          iterCaps.push(r.caps);
          c = r.next;
          if (c >= call.length) break;
        }
        if (op === '+' && iterCaps.length === 0) return null;
        // 合并重复捕获：reps[name] = 各次捕获的 tokens
        const names = new Set<string>();
        for (const ic of iterCaps) { for (const k of ic.vars.keys()) names.add(k); }
        for (const n of names) {
          local.reps.set(n, iterCaps.map((ic) => ic.vars.get(n) ?? []));
        }
        m = after;
        continue;
      }
      // $name:frag
      if (mSeq[m]!.kind === 'punct' && mSeq[m]!.text === '$' && mSeq[m + 1]?.kind === 'ident' && mSeq[m + 2]?.kind === 'punct' && mSeq[m + 2].text === ':' && mSeq[m + 3]?.kind === 'ident' && FRAG_SPECS.has(mSeq[m + 3]!.text)) {
        const name = mSeq[m + 1]!.text;
        const spec = mSeq[m + 3]!.text;
        m += 4;
        if (['ident', 'literal', 'ty', 'path', 'meta', 'pat'].includes(spec)) {
          if (c >= call.length) return null;
          local.vars.set(name, [call[c]!]);
          c++;
        } else if (spec === 'tt') {
          if (c >= call.length) return null;
          if (isDelim(call[c])) {
            const dc = matchDelim(call, c);
            local.vars.set(name, call.slice(c, dc + 1));
            c = dc + 1;
          } else {
            local.vars.set(name, [call[c]!]);
            c++;
          }
        } else {
          // expr/stmt/block/item：贪婪捕获到下一个 matcher 字面 token 或结尾
          const nextLiteral = mSeq[m];
          const frag: Token[] = [];
          let depth = 0;
          for (;;) {
            if (c >= call.length) break;
            const ct = call[c]!;
            if (depth === 0 && nextLiteral && matchesTok(nextLiteral, ct)) break;
            if (isDelim(ct)) {
              const open = ct.text;
              if (['(', '[', '{'].includes(open)) {
                const dc = matchDelim(call, c);
                frag.push(...call.slice(c, dc + 1));
                c = dc + 1;
                continue;
              }
            }
            frag.push(ct);
            c++;
          }
          if (frag.length === 0) return null;
          local.vars.set(name, frag);
        }
        continue;
      }
      // 普通字面 token
      if (c >= call.length || !matchesTok(mSeq[m]!, call[c]!)) return null;
      m++;
      c++;
    }
    return { caps: local, next: c };
  };

  const r = trySeq(matcher, 0);
  if (!r || r.next !== call.length) return null;
  for (const [k, v] of r.caps.vars) caps.vars.set(k, v);
  for (const [k, v] of r.caps.reps) caps.reps.set(k, v);
  return caps;
}

function applyMacro(rules: MacroRule2[], call: Token[], file: string): Token[] {
  for (const rule of rules) {
    const caps = matchTokens(rule.matcher, call);
    if (caps) return transcribe(rule.transcriber, caps);
  }
  throw new Error(`宏调用与任何规则都不匹配：${call.map((t) => t.text).join(' ')}（${file}）`);
}

function transcribe(transcriber: Token[], caps: Captures): Token[] {
  const out: Token[] = [];
  let i = 0;
  while (i < transcriber.length) {
    const t = transcriber[i]!;
    // $var
    if (t.kind === 'punct' && t.text === '$' && transcriber[i + 1]?.kind === 'ident') {
      const name = transcriber[i + 1]!.text;
      if (caps.vars.has(name)) {
        out.push(...caps.vars.get(name)!);
        i += 2;
        continue;
      }
    }
    // $( ...)op —— 重复转录
    if (t.kind === 'punct' && t.text === '$' && transcriber[i + 1]?.kind === 'punct' && transcriber[i + 1].text === '(') {
      const close = matchDelim(transcriber, i + 1);
      const body = transcriber.slice(i + 2, close);
      let after = close + 1;
      const op = transcriber[after]?.text ?? '*';
      if (op === '*' || op === '+' || op === '?') after++;
      const sepTok = transcriber[after];
      if (sepTok && (op === '*' || op === '+' || op === '?')) after++;
      // 找出 body 中的 $var 名
      const names: string[] = [];
      for (let b = 0; b < body.length; b++) {
        if (body[b]!.kind === 'punct' && body[b]!.text === '$' && body[b + 1]?.kind === 'ident') names.push(body[b + 1]!.text);
      }
      const repLen = Math.max(0, ...names.map((n) => caps.reps.get(n)?.length ?? 0));
      for (let r = 0; r < repLen; r++) {
        if (r > 0 && sepTok) out.push(sepTok);
        const scoped: Captures = { vars: new Map(), reps: new Map() };
        for (const n of names) {
          const arr = caps.reps.get(n);
          if (arr && r < arr.length) scoped.vars.set(n, arr[r]!);
        }
        out.push(...transcribe(body, scoped));
      }
      i = after;
      continue;
    }
    out.push(t);
    i++;
  }
  return out;
}
