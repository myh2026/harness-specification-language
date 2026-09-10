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
  // v0.2.57 修复：String::push(char) 缺失（Rust 对等 API）——
  // `out.push('\n')` / `out.push(' ')` 是 Rust 风格源码的常见写法，
  // 此前运行期 "String 没有方法 push"。与 push_str 同为 mutating。
  push: {
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
  // v0.2.58 修复（实测复现）：find 的索引空间统一为码点 —— 此前 indexOf
  // 返回 UTF-16 码元索引，而 len/char_at/take/chars 全部是码点口径：
  // `"é😊x".find("x")` 得 3（码元），`char_at(2)` 却是 "x"（码点），
  // 两套索引空间静默组合必错位（`s.char_at(s.find(x)?)` 取错字符）。
  // 码点口径与 len/char_at 的既有行为一致，属破坏面最小的对齐方向。
  find: {
    fn: (r, a) => {
      const s = S(r), needle = S(a[0]);
      if (needle === '') return someV(0);
      const hay = [...s], nee = [...needle];
      const n = nee.length;
      for (let i = 0; i + n <= hay.length; i++) {
        let hit = true;
        for (let j = 0; j < n; j++) {
          if (hay[i + j] !== nee[j]) { hit = false; break; }
        }
        if (hit) return someV(i);
      }
      return noneV();
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
  // v0.2.58 修复（B-6）：split_once / rsplit_once 缺失 —— 「k: v」行拆键值是
  // Rust 最常用写法，此前只能 find+take+native slice 手工绕（check 过 / run 崩
  // 或写出别扭代码）。语义：返回 Option<(before, after)>；分隔符为空串报运行期
  // 错误（Rust panic 的对应物）。二元组以运行期元组（数组）承载，
  // `match s.split_once(":") { Some((k, v)) => ... }` 直接解构。
  split_once: {
    fn: (r, a) => {
      const s = S(r), sep = S(a[0]);
      if (sep === '') throw new HRuntimeError('split_once：空分隔符（Rust 语义为 panic）');
      const i = s.indexOf(sep);
      return i < 0 ? noneV() : someV([s.slice(0, i), s.slice(i + sep.length)]);
    },
  },
  rsplit_once: {
    fn: (r, a) => {
      const s = S(r), sep = S(a[0]);
      if (sep === '') throw new HRuntimeError('rsplit_once：空分隔符（Rust 语义为 panic）');
      const i = s.lastIndexOf(sep);
      return i < 0 ? noneV() : someV([s.slice(0, i), s.slice(i + sep.length)]);
    },
  },
  // v0.2.58（B-6）：clear / truncate / retain / insert / remove —— String 的
  // 就地变形五件套（Vec 侧此前已有 clear，String 侧缺失）。
  clear: {
    mutating: true,
    fn: (r, _a, ctx) => {
      if (ctx.setRecv) { ctx.setRecv(''); return undefined; }
      return '';
    },
  },
  truncate: {
    mutating: true,
    fn: (r, a, ctx) => {
      const n = N(a[0]);
      if (n < 0) throw new HRuntimeError(`truncate：负长度 ${n}`);
      const nv = [...S(r)].slice(0, n).join('');
      if (ctx.setRecv) { ctx.setRecv(nv); return undefined; }
      return nv;
    },
  },
  retain: {
    mutating: true,
    fn: async (r, a, ctx) => {
      const kept: string[] = [];
      for (const ch of [...S(r)]) if (B(await ctx.call(a[0]!, [ch]))) kept.push(ch);
      const nv = kept.join('');
      if (ctx.setRecv) { ctx.setRecv(nv); return undefined; }
      return nv;
    },
  },
  insert: {
    mutating: true,
    fn: (r, a, ctx) => {
      const chars = [...S(r)];
      const idx = N(a[0]);
      const ch = S(a[1]);
      if (ch.length !== 1) throw new HRuntimeError(`insert：期望单字符，得到 "${ch}"`);
      if (idx < 0 || idx > chars.length) throw new HRuntimeError(`insert：索引 ${idx} 越界（长度 ${chars.length}）`);
      chars.splice(idx, 0, ch);
      const nv = chars.join('');
      if (ctx.setRecv) { ctx.setRecv(nv); return undefined; }
      return nv;
    },
  },
  remove: {
    mutating: true,
    fn: (r, a, ctx) => {
      const chars = [...S(r)];
      const idx = N(a[0]);
      if (idx < 0 || idx >= chars.length) throw new HRuntimeError(`remove：索引 ${idx} 越界（长度 ${chars.length}）`);
      const [out] = chars.splice(idx, 1);
      const nv = chars.join('');
      if (ctx.setRecv) ctx.setRecv(nv);
      return out;
    },
  },
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
  // v0.2.57 修复：iter_mut 缺失于内建方法面 —— checker 接受 `.iter_mut()`（nova
  // 的 accept/complete_task 即用），但解释器未注册 → 运行期 "Vec 没有方法 iter_mut"
  // （check 过 / run 崩的静默断层）。语义：返回数组本体；struct 元素是 JS 对象引用，
  // `for t in v.iter_mut() { t.field = ... }` 的字段写按引用透传（与 interp 的对象
  // 透明共享模型一致）；primitive 元素的写不透传 —— 解释器透明性已记录的边界。
  iter_mut: { fn: (r) => r },
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
  // v0.2.58 修复（B-6）：迭代器对等方法缺失 —— find / filter_map / flat_map /
  // flatten / count / min / max / zip / chain / step_by（Rust Iterator 家族中
  // 高频前九名，此前 check 全过、run 全崩 —— B-1 类断层的最大聚集面）。
  // 语义对齐：find → Option<T>（首个满足谓词的元素）；filter_map → 保留
  // Some 载荷；flat_map → 闭包返回 Vec 拼接；flatten → 嵌套数组摊平；
  // min/max → Option（空 Vec 为 None，与 Rust 一致）；zip → 短边截断；
  // chain → 拷贝拼接（非原地）。
  find: {
    fn: async (r, a, ctx) => {
      for (const x of r as unknown[]) if (B(await ctx.call(a[0]!, [x]))) return someV(x);
      return noneV();
    },
  },
  filter_map: {
    fn: async (r, a, ctx) => {
      const out: unknown[] = [];
      for (const x of r as unknown[]) {
        const v = await ctx.call(a[0]!, [x]);
        if (isOption(v)) {
          if (v.variant === 'Some') out.push(enumPayload(v));
        } else {
          throw new HRuntimeError(`filter_map：闭包必须返回 Option，得到 ${debug(v)}`);
        }
      }
      return out;
    },
  },
  flat_map: {
    fn: async (r, a, ctx) => {
      const out: unknown[] = [];
      for (const x of r as unknown[]) {
        const v = await ctx.call(a[0]!, [x]);
        if (!Array.isArray(v)) throw new HRuntimeError(`flat_map：闭包必须返回 Vec，得到 ${debug(v)}`);
        out.push(...v);
      }
      return out;
    },
  },
  flatten: {
    fn: (r) => {
      const out: unknown[] = [];
      for (const x of r as unknown[]) {
        if (!Array.isArray(x)) throw new HRuntimeError(`flatten：元素必须是 Vec，得到 ${debug(x)}`);
        out.push(...x);
      }
      return out;
    },
  },
  count: { fn: (r) => (r as unknown[]).length },
  min: { fn: (r) => ordFold(r as unknown[], 'min') },
  max: { fn: (r) => ordFold(r as unknown[], 'max') },
  zip: {
    fn: (r, a) => {
      const xs = r as unknown[];
      const ys = a[0] as unknown[];
      if (!Array.isArray(ys)) throw new HRuntimeError(`zip：右操作数必须是 Vec，得到 ${debug(a[0])}`);
      const n = Math.min(xs.length, ys.length);
      const out: unknown[] = [];
      for (let i = 0; i < n; i++) out.push([xs[i], ys[i]]);
      return out;
    },
  },
  chain: {
    fn: (r, a) => {
      const ys = a[0] as unknown[];
      if (!Array.isArray(ys)) throw new HRuntimeError(`chain：右操作数必须是 Vec，得到 ${debug(a[0])}`);
      return [...(r as unknown[]), ...ys];
    },
  },
  step_by: {
    fn: (r, a) => {
      const step = N(a[0]);
      if (step < 1) throw new HRuntimeError(`step_by：步长必须 ≥ 1，得到 ${step}`);
      return (r as unknown[]).filter((_, i) => i % step === 0);
    },
  },
  // v0.2.58（B-6）：reverse / dedup / retain / truncate / chunks —— Vec 就地
  // 变形与分块。reverse 此前缺失时既不落到内建也不落到 foreign 直通
  // （foreign 分支显式排除数组）→ 必崩；dedup 按 Rust 语义去「连续」重复
  // （deepEq 比较）；chunks 返回 Vec<Vec<T>>（尾块允许不足 n）。
  reverse: { mutating: true, fn: (r) => { (r as unknown[]).reverse(); return undefined; } },
  dedup: {
    mutating: true,
    fn: (r) => {
      const arr = r as unknown[];
      let w = 0;
      for (let i = 0; i < arr.length; i++) {
        if (w > 0 && deepEq(arr[w - 1], arr[i])) continue;
        arr[w++] = arr[i];
      }
      arr.length = w;
      return undefined;
    },
  },
  retain: {
    mutating: true,
    fn: async (r, a, ctx) => {
      const arr = r as unknown[];
      let w = 0;
      for (let i = 0; i < arr.length; i++) {
        if (B(await ctx.call(a[0]!, [arr[i]]))) arr[w++] = arr[i];
      }
      arr.length = w;
      return undefined;
    },
  },
  truncate: {
    mutating: true,
    fn: (r, a) => {
      const n = N(a[0]);
      const arr = r as unknown[];
      if (n < 0) throw new HRuntimeError(`truncate：负长度 ${n}`);
      if (n < arr.length) arr.length = n;
      return undefined;
    },
  },
  chunks: {
    fn: (r, a) => {
      const n = N(a[0]);
      if (n < 1) throw new HRuntimeError(`chunks：块大小必须 ≥ 1，得到 ${n}`);
      const arr = r as unknown[];
      const out: unknown[] = [];
      for (let i = 0; i < arr.length; i += n) out.push(arr.slice(i, i + n));
      return out;
    },
  },
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
  // v0.2.58（B-6）：iter 缺失 —— Rust HashMap 遍历产出 (K, V) 二元组，
  // 此前只能 keys() + get() 两跳绕。返回 Vec<(K, V)>（运行期元组 = 数组），
  // `for kv in m.iter()` / `match kv.0` 均可用。
  iter: { fn: (r) => [...(r as Map<unknown, unknown>).entries()] },
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
  // v0.2.58（B-6）：and 缺失（or 已在，同族不对称）。Some(x) → 返回右操作数，
  // None → None（右侧惰性求值在解释器闭包模型下不做 —— 右侧已是值）。
  and: { fn: (r, a) => (isOption(r) && r.variant === 'Some' ? a[0] : noneV()) },
  cloned: { fn: (r) => (isOption(r) && r.variant === 'Some' ? someV(cloneValue(enumPayload(r))) : noneV()) },
  // v0.2.57 修复（Bug #4）：Option 缺 clone —— 自定义 derive(Clone) 枚举有
  // clone 而内置 Option/Result 没有（Rust 语义：Option<T: Clone> 实现 Clone；
  // .cloned() 是 Option<&T> 的另一方法，不能替代）。与用户枚举的 clone 同走
  // cloneValue 深拷贝。
  clone: { fn: (r) => cloneValue(r) },
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
  // v0.2.57（Bug #4 同修）：Result 缺 clone
  clone: { fn: (r) => cloneValue(r) },
  and_then: { fn: async (r, a, ctx) => (isResult(r) && r.variant === 'Ok' ? await ctx.call(a[0]!, [enumPayload(r)]) : r) },
  or_else: { fn: async (r, a, ctx) => (isResult(r) && r.variant === 'Err' ? await ctx.call(a[0]!, [enumPayload(r)]) : r) },
  // v0.2.58（B-6）：unwrap_or_else / unwrap_err 缺失 —— Option 侧早有
  // unwrap_or_else 而 Result 侧没有（同族不对称）；unwrap_err 是测试与错误
  // 检视路径的常用写法，Ok 时报运行期错误（Rust panic 的对应物）。
  unwrap_or_else: { fn: async (r, a, ctx) => (isResult(r) && r.variant === 'Ok' ? enumPayload(r) : await ctx.call(a[0]!, [enumPayload(r)])) },
  unwrap_err: { fn: (r) => (isResult(r) && r.variant === 'Err' ? enumPayload(r) : throwRuntime(`unwrap_err：Ok(${display(isResult(r) ? enumPayload(r) : r)})`)) },
};

function throwRuntime(msg: string): never {
  throw new HRuntimeError(msg);
}

// v0.2.58（B-6）：Vec::min / max 的 Ord 折叠 —— 全数值按数值序、全字符串按
// 字典序（码元序），混合类型报运行期错误（Rust 要求同构 Ord，不静默强转）；
// 空 Vec → None（Rust 语义）。
function ordFold(arr: unknown[], which: 'min' | 'max'): unknown {
  if (arr.length === 0) return noneV();
  const allNum = arr.every((x) => typeof x === 'number' || typeof x === 'bigint');
  const allStr = arr.every((x) => typeof x === 'string');
  if (!allNum && !allStr) {
    throw new HRuntimeError(`${which}：元素须同为数值或同为字符串（Rust Ord 同构约束）`);
  }
  let best = arr[0]!;
  for (const x of arr.slice(1)) {
    const better = allNum
      ? (which === 'min' ? N(x) < N(best) : N(x) > N(best))
      : (which === 'min' ? S(x) < S(best) : S(x) > S(best));
    if (better) best = x;
  }
  return someV(best);
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
