// ============================================================================
// dhv-ts/src/builtins.ts — HSL 标准库运行时（std 方法面 + 免费函数）
// ----------------------------------------------------------------------------
// 这是「std 预导入库」的运行时实现：String / Vec / HashMap / 数值 / char /
// Option / Result 的方法集，与 BNF v1.3 附录 A（std 方法面）一一对应。
// ============================================================================

import {
  HEnum, HValue, HRuntimeError, display, debug, deepEq, cloneValue,
  isEnum, isStruct, someV, noneV, okV, errV, isResult, isOption,
} from './values';

export interface MethodCtx {
  call: (closure: unknown, args: unknown[]) => Promise<unknown>;
  generics?: string[]; // turbofish 泛型实参的路径段（如 ["u32"]）
  setRecv?: (v: unknown) => void;
  where?: { file: string; line: number; col: number };
}

export interface BuiltinMethod {
  fn: (recv: unknown, args: unknown[], ctx: MethodCtx) => unknown | Promise<unknown>;
  mutating?: boolean;
}

const S = (v: unknown): string => {
  if (typeof v === 'string') return v;
  return display(v);
};
const N = (v: unknown): number => {
  if (typeof v === 'bigint') return Number(v);
  if (typeof v === 'number') return v;
  if (typeof v === 'string') {
    const n = Number(v);
    if (Number.isNaN(n)) throw new HRuntimeError(`期望数值，得到 "${v}"`);
    return n;
  }
  throw new HRuntimeError(`期望数值，得到 ${debug(v)}`);
};
const B = (v: unknown): boolean => {
  if (typeof v === 'boolean') return v;
  throw new HRuntimeError(`期望 bool，得到 ${debug(v)}（S1：零隐式转换）`);
};

// ============================== String ==============================
export const STRING_METHODS: Record<string, BuiltinMethod> = {
  len: { fn: (r) => [...S(r)].length },
  is_empty: { fn: (r) => S(r).length === 0 },
  push_str: {
    mutating: true,
    fn: (r, a, ctx) => {
      const nv = S(r) + S(a[0]);
      if (ctx.setRecv) { ctx.setRecv(nv); return undefined; }
      return nv;
    },
  },
  as_str: { fn: (r) => S(r) },
  clone: { fn: (r) => S(r) },
  to_string: { fn: (r) => S(r) },
  trim: { fn: (r) => S(r).trim() },
  trim_start: { fn: (r) => S(r).trimStart() },
  trim_end: { fn: (r) => S(r).trimEnd() },
  contains: { fn: (r, a) => S(r).includes(S(a[0])) },
  starts_with: { fn: (r, a) => S(r).startsWith(S(a[0])) },
  ends_with: { fn: (r, a) => S(r).endsWith(S(a[0])) },
  replace: { fn: (r, a) => S(r).split(S(a[0])).join(S(a[1])) },
  split: { fn: (r, a) => S(r).split(S(a[0])) },
  split_whitespace: { fn: (r) => S(r).trim().split(/\s+/).filter((s) => s.length > 0) },
  lines: { fn: (r) => S(r).split('\n') },
  to_lowercase: { fn: (r) => S(r).toLowerCase() },
  to_uppercase: { fn: (r) => S(r).toUpperCase() },
  chars: { fn: (r) => [...S(r)] },
  repeat: { fn: (r, a) => S(r).repeat(N(a[0])) },
  strip_prefix: {
    fn: (r, a) => {
      const s = S(r), p = S(a[0]);
      return s.startsWith(p) ? someV(s.slice(p.length)) : noneV();
    },
  },
  strip_suffix: {
    fn: (r, a) => {
      const s = S(r), p = S(a[0]);
      return s.endsWith(p) ? someV(s.slice(0, s.length - p.length)) : noneV();
    },
  },
  find: {
    fn: (r, a) => {
      const idx = S(r).indexOf(S(a[0]));
      return idx >= 0 ? someV(idx) : noneV();
    },
  },
  parse: {
    fn: (r, _a, ctx) => {
      const s = S(r).trim();
      const ty = ctx.generics?.[0] ?? 'f64';
      try {
        if (ty.startsWith('u') || ty.startsWith('i')) {
          if (!/^[+-]?\d+$/.test(s)) return errV(`parse_int 错误："${s}"`);
          const n = ty === 'u64' || ty === 'i64' || ty === 'usize' || ty === 'isize'
            ? BigInt(s) : Number(s);
          if (ty.startsWith('u') && BigInt(n) < 0n) return errV(`parse 负数到 ${ty}：${s}`);
          return okV(n);
        }
        // v1.4.9 修复：空串浮点 parse 此前经 JS Number("") === 0 隐患返回 Ok(0)（与
        // 整数路径 "" → Err 不一致，JS 语言怪癖泄漏）—— 统一为 Err（与 Rust 语义一致）
        if (s === '') return errV(`parse_float 错误：""`);
        const f = Number(s);
        if (Number.isNaN(f)) return errV(`parse_float 错误：${s}`);
        return okV(f);
      } catch (e) {
        return errV(`parse 错误：${(e as Error).message}`);
      }
    },
  },
  char_at: { fn: (r, a) => [...S(r)][N(a[0])] ?? '' },
  char_count: { fn: (r) => [...S(r)].length },
  take: { fn: (r, a) => [...S(r)].slice(0, N(a[0])).join('') },
  join: { fn: (r, a) => (Array.isArray(r) ? r.map(S).join(S(a[0])) : S(r)) },
};

// ============================== Vec / 数组 ==============================
export const VEC_METHODS: Record<string, BuiltinMethod> = {
  len: { fn: (r) => (r as unknown[]).length },
  is_empty: { fn: (r) => (r as unknown[]).length === 0 },
  push: { mutating: true, fn: (r, a) => { (r as unknown[]).push(a[0]); return undefined; } },
  pop: {
    fn: (r) => {
      const arr = r as unknown[];
      if (arr.length === 0) return noneV();
      return someV(arr.pop());
    },
  },
  clone: { fn: (r) => cloneValue(r) },
  first: { fn: (r) => ((r as unknown[]).length > 0 ? someV((r as unknown[])[0]) : noneV()) },
  last: { fn: (r) => ((r as unknown[]).length > 0 ? someV((r as unknown[])[(r as unknown[]).length - 1]) : noneV()) },
  get: {
    fn: (r, a) => {
      const i = N(a[0]);
      const arr = r as unknown[];
      return i >= 0 && i < arr.length ? someV(arr[i]) : noneV();
    },
  },
  contains: { fn: (r, a) => (r as unknown[]).some((x) => deepEq(x, a[0])) },
  join: { fn: (r, a) => (r as unknown[]).map(S).join(S(a[0])) },
  iter: { fn: (r) => r },
  map: {
    fn: async (r, a, ctx) => {
      const out: unknown[] = [];
      for (const x of r as unknown[]) out.push(await ctx.call(a[0]!, [x]));
      return out;
    },
  },
  filter: {
    fn: async (r, a, ctx) => {
      const out: unknown[] = [];
      for (const x of r as unknown[]) if (B(await ctx.call(a[0]!, [x]))) out.push(x);
      return out;
    },
  },
  for_each: {
    fn: async (r, a, ctx) => {
      for (const x of r as unknown[]) await ctx.call(a[0]!, [x]);
      return undefined;
    },
  },
  any: { fn: async (r, a, ctx) => { for (const x of r as unknown[]) if (B(await ctx.call(a[0]!, [x]))) return true; return false; } },
  all: { fn: async (r, a, ctx) => { for (const x of r as unknown[]) if (!B(await ctx.call(a[0]!, [x]))) return false; return true; } },
  fold: {
    fn: async (r, a, ctx) => {
      let acc = a[0];
      for (const x of r as unknown[]) acc = await ctx.call(a[1]!, [acc, x]);
      return acc;
    },
  },
  enumerate: { fn: (r) => (r as unknown[]).map((v, i) => [i, v]) },
  take: { fn: (r, a) => (r as unknown[]).slice(0, N(a[0])) },
  skip: { fn: (r, a) => (r as unknown[]).slice(N(a[0])) },
  rev: { fn: (r) => [...(r as unknown[])].reverse() },
  sort: {
    mutating: true,
    fn: (r) => { (r as unknown[]).sort((a, b) => (N(a) < N(b) ? -1 : N(a) > N(b) ? 1 : 0)); return undefined; },
  },
  sort_by: {
    mutating: true,
    fn: async (r, a, ctx) => {
      const arr = r as unknown[];
      const decorated = await Promise.all(arr.map(async (x) => ({ x, k: N(await ctx.call(a[0]!, [x])) })));
      decorated.sort((p, q) => p.k - q.k);
      for (let i = 0; i < arr.length; i++) arr[i] = decorated[i]!.x;
      return undefined;
    },
  },
  append: { mutating: true, fn: (r, a) => { (r as unknown[]).push(...(a[0] as unknown[])); return undefined; } },
  extend: { mutating: true, fn: (r, a) => { (r as unknown[]).push(...(a[0] as unknown[])); return undefined; } },
  sum: { fn: (r) => (r as unknown[]).reduce<number>((acc: number, x: unknown) => acc + N(x), 0) },
  collect: {
    fn: (r, _a, ctx) => {
      const ty = ctx.generics?.[0] ?? 'Vec';
      if (ty === 'String') return (r as unknown[]).map(S).join('');
      return r;
    },
  },
  clear: { mutating: true, fn: (r) => { (r as unknown[]).length = 0; return undefined; } },
  is_sorted: {
    fn: (r) => {
      const v = r as unknown[];
      for (let i = 0; i + 1 < v.length; i++) if (N(v[i]) > N(v[i + 1])) return false;
      return true;
    },
  },
  sort_desc: {
    mutating: true,
    fn: (r) => { (r as unknown[]).sort((a, b) => (N(a) < N(b) ? 1 : N(a) > N(b) ? -1 : 0)); return undefined; },
  },
  position: {
    fn: async (r, a, ctx) => {
      for (let i = 0; i < (r as unknown[]).length; i++) {
        if (B(await ctx.call(a[0]!, [(r as unknown[])[i]]))) return someV(i);
      }
      return noneV();
    },
  },
  insert: { mutating: true, fn: (r, a) => { (r as unknown[]).splice(N(a[0]), 0, a[1]); return undefined; } },
  remove: { mutating: true, fn: (r, a) => (r as unknown[]).splice(N(a[0]), 1)[0] },
};

// ============================== HashMap ==============================
export const MAP_METHODS: Record<string, BuiltinMethod> = {
  insert: { mutating: true, fn: (r, a) => { (r as Map<unknown, unknown>).set(a[0], a[1]); return undefined; } },
  get: {
    fn: (r, a) => {
      const m = r as Map<unknown, unknown>;
      return m.has(a[0]) ? someV(m.get(a[0])) : noneV();
    },
  },
  contains_key: { fn: (r, a) => (r as Map<unknown, unknown>).has(a[0]) },
  len: { fn: (r) => (r as Map<unknown, unknown>).size },
  is_empty: { fn: (r) => (r as Map<unknown, unknown>).size === 0 },
  remove: {
    fn: (r, a) => {
      const m = r as Map<unknown, unknown>;
      if (!m.has(a[0])) return noneV();
      const v = m.get(a[0]);
      m.delete(a[0]);
      return someV(v);
    },
  },
  clear: { mutating: true, fn: (r) => { (r as Map<unknown, unknown>).clear(); return undefined; } },
  keys: { fn: (r) => [...(r as Map<unknown, unknown>).keys()] },
  values: { fn: (r) => [...(r as Map<unknown, unknown>).values()] },
  clone: { fn: (r) => cloneValue(r) },
};

// ============================== 数值 ==============================
export const NUM_METHODS: Record<string, BuiltinMethod> = {
  to_string: { fn: (r) => display(r) },
  abs: { fn: (r) => Math.abs(N(r)) },
  pow: { fn: (r, a) => Math.pow(N(r), N(a[0])) },
  sqrt: { fn: (r) => Math.sqrt(N(r)) },
  floor: { fn: (r) => Math.floor(N(r)) },
  ceil: { fn: (r) => Math.ceil(N(r)) },
  round: { fn: (r) => Math.round(N(r)) },
  min: { fn: (r, a) => Math.min(N(r), N(a[0])) },
  max: { fn: (r, a) => Math.max(N(r), N(a[0])) },
  clamp: { fn: (r, a) => Math.min(Math.max(N(r), N(a[0])), N(a[1])) },
  clone: { fn: (r) => r },
};

// ============================== Option / Result ==============================
function enumPayload(v: HEnum): unknown {
  return v.payload?.tuple?.[0];
}

export const OPTION_METHODS: Record<string, BuiltinMethod> = {
  unwrap: { fn: (r) => (isOption(r) && r.variant === 'Some' ? enumPayload(r) : throwRuntime(' unwrap：None 值（S2 裸 unwrap）')) },
  expect: { fn: (r, a) => (isOption(r) && r.variant === 'Some' ? enumPayload(r) : throwRuntime(`${S(a[0])}：expect None`)) },
  unwrap_or: { fn: (r, a) => (isOption(r) && r.variant === 'Some' ? enumPayload(r) : a[0]) },
  unwrap_or_else: { fn: async (r, a, ctx) => (isOption(r) && r.variant === 'Some' ? enumPayload(r) : await ctx.call(a[0]!, [])) },
  is_some: { fn: (r) => isOption(r) && r.variant === 'Some' },
  is_none: { fn: (r) => isOption(r) && r.variant === 'None' },
  map: { fn: async (r, a, ctx) => (isOption(r) && r.variant === 'Some' ? someV(await ctx.call(a[0]!, [enumPayload(r)])) : noneV()) },
  and_then: { fn: async (r, a, ctx) => (isOption(r) && r.variant === 'Some' ? await ctx.call(a[0]!, [enumPayload(r)]) : noneV()) },
  ok_or: { fn: (r, a) => (isOption(r) && r.variant === 'Some' ? okV(enumPayload(r)) : errV(a[0])) },
  or: { fn: (r, a) => (isOption(r) && r.variant === 'Some' ? r : a[0]) },
  cloned: { fn: (r) => (isOption(r) && r.variant === 'Some' ? someV(cloneValue(enumPayload(r))) : noneV()) },
  // v1.4.9 新增：Option::filter（Rust 语义：Some(x) 且 f(x) 真 → 保留原 Some；否则 None）
  filter: {
    fn: async (r, a, ctx) => (isOption(r) && r.variant === 'Some' && B(await ctx.call(a[0]!, [enumPayload(r)])) ? r : noneV()),
  },
};

export const RESULT_METHODS: Record<string, BuiltinMethod> = {
  unwrap: { fn: (r) => (isResult(r) && r.variant === 'Ok' ? enumPayload(r) : throwRuntime(`unwrap：Err(${display(isResult(r) ? enumPayload(r) : r)})`)) },
  expect: { fn: (r, a) => (isResult(r) && r.variant === 'Ok' ? enumPayload(r) : throwRuntime(`${S(a[0])}：Err(${display(isResult(r) ? enumPayload(r) : r)})`)) },
  is_ok: { fn: (r) => isResult(r) && r.variant === 'Ok' },
  is_err: { fn: (r) => isResult(r) && r.variant === 'Err' },
  ok: { fn: (r) => (isResult(r) && r.variant === 'Ok' ? someV(enumPayload(r)) : noneV()) },
  err: { fn: (r) => (isResult(r) && r.variant === 'Err' ? someV(enumPayload(r)) : noneV()) },
  map: { fn: async (r, a, ctx) => (isResult(r) && r.variant === 'Ok' ? okV(await ctx.call(a[0]!, [enumPayload(r)])) : r) },
  map_err: { fn: async (r, a, ctx) => (isResult(r) && r.variant === 'Err' ? errV(await ctx.call(a[0]!, [enumPayload(r)])) : r) },
  unwrap_or: { fn: (r, a) => (isResult(r) && r.variant === 'Ok' ? enumPayload(r) : a[0]) },
  and_then: { fn: async (r, a, ctx) => (isResult(r) && r.variant === 'Ok' ? await ctx.call(a[0]!, [enumPayload(r)]) : r) },
  or_else: { fn: async (r, a, ctx) => (isResult(r) && r.variant === 'Err' ? await ctx.call(a[0]!, [enumPayload(r)]) : r) },
};

function throwRuntime(msg: string): never {
  throw new HRuntimeError(msg);
}

// ============================== 免费函数 ==============================
export const FREE_FNS: Record<string, (args: unknown[]) => unknown> = {
  min: (a) => Math.min(N(a[0]), N(a[1])),
  max: (a) => Math.max(N(a[0]), N(a[1])),
  abs: (a) => Math.abs(N(a[0])),
  to_string: (a) => display(a[0]),
  str: (a) => S(a[0]),
};

// char 方法（单字符 string）
export const CHAR_METHODS: Record<string, BuiltinMethod> = {
  to_string: { fn: (r) => S(r) },
  is_alphabetic: { fn: (r) => /[A-Za-z\u0080-\uFFFF]/.test(S(r)) },
  is_numeric: { fn: (r) => /[0-9]/.test(S(r)) },
  clone: { fn: (r) => S(r) },
};

export function builtinMethodFor(recv: unknown, name: string): BuiltinMethod | undefined {
  if (typeof recv === 'string') {
    return STRING_METHODS[name] ?? (recv.length <= 1 ? CHAR_METHODS[name] : undefined);
  }
  if (typeof recv === 'number' || typeof recv === 'bigint') return NUM_METHODS[name];
  if (Array.isArray(recv)) return VEC_METHODS[name];
  if (recv instanceof Map) return MAP_METHODS[name];
  if (isOption(recv)) return OPTION_METHODS[name];
  if (isResult(recv)) return RESULT_METHODS[name];
  if (isEnum(recv)) {
    // 用户枚举：提供通用方法
    if (name === 'clone') return { fn: (r) => cloneValue(r) };
    if (name === 'to_string') return { fn: (r) => display(r) };
    return undefined;
  }
  if (isStruct(recv)) {
    // 结构体默认方法（未由 impl 覆盖时）
    if (name === 'clone') return { fn: (r) => cloneValue(r) };
    if (name === 'to_string') return { fn: (r) => debug(r) };
    if (name === 'len') return { fn: () => Object.keys(recv).length - 1 };
    return undefined;
  }
  return undefined;
}

export type { HValue, HEnum };
