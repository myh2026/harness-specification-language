// ============================================================================
// dhv-ts/src/std.ts — HSL 标准库（C++ 风格多库组织，BNF v1.4 附录 C）
// ----------------------------------------------------------------------------
// 10 个库模块，经 import { f } from "std/<mod>"; 使用：
//   std/core       身份/断言/哈希/类型名
//   std/collections Vec 构建/zip/chunk/dedup/unique/sort_desc
//   std/text       split_once/大小写转换/pad/levenshtein/capitalize
//   std/math       三角/对数/gcd/lcm/isqrt/div_ceil/PI/E
//   std/io         文件读写（走宿主路径监狱，Result 语义）
//   std/json       本地 JSON 解析/序列化（无宿主依赖，确定性）
//   std/time       now_ms/now_iso/duration_desc
//   std/random     可复现 PRNG（mulberry32，默认种子 42）/shuffle/choice/uuid
//   std/env        环境变量/任务/模型名
//   std/iter       range/range_step/enumerate/chain/take/skip/min/max
// ============================================================================

import { display, debug, someV, noneV, okV, errV, cloneValue, deepEq } from './values';

export interface NativeFn {
  __nativefn: true;
  name: string;
  fn: (args: unknown[], ctx: StdCtx) => unknown | Promise<unknown>;
}

export interface StdCtx {
  hostApi: unknown;
}

export type StdModule = Record<string, NativeFn | number | string | boolean>;

const S = (v: unknown): string => (typeof v === 'string' ? v : display(v));
const N = (v: unknown): number => {
  if (typeof v === 'bigint') return Number(v);
  if (typeof v === 'number') return v;
  throw new Error(`std: 期望数值，得到 ${debug(v)}`);
};
const I = (v: unknown): bigint => {
  if (typeof v === 'bigint') return v;
  if (typeof v === 'number' && Number.isInteger(v)) return BigInt(v);
  throw new Error(`std: 期望整数，得到 ${debug(v)}`);
};
const V = (v: unknown): unknown[] => {
  if (Array.isArray(v)) return v;
  throw new Error(`std: 期望 Vec，得到 ${debug(v)}`);
};

function nat(name: string, fn: (args: unknown[], ctx: StdCtx) => unknown | Promise<unknown>): NativeFn {
  return { __nativefn: true, name, fn };
}

// ---------------------------------------------------------------------------
// std/core
// ---------------------------------------------------------------------------
const stdCore: StdModule = {
  identity: nat('identity', (a) => a[0]),
  todo: nat('todo', (a) => { throw new Error(`todo!: ${a[0] !== undefined ? S(a[0]) : '未实现'}`); }),
  unreachable: nat('unreachable', (a) => { throw new Error(`unreachable: ${a[0] !== undefined ? S(a[0]) : '逻辑上不可达的分支被执行'}`); }),
  type_name: nat('type_name', (a) => {
    const v = a[0];
    if (typeof v === 'string') return 'String';
    if (typeof v === 'boolean') return 'bool';
    if (typeof v === 'number') return Number.isInteger(v) ? 'i64' : 'f64';
    if (typeof v === 'bigint') return 'i64';
    if (Array.isArray(v)) return 'Vec';
    if (v instanceof Map) return 'HashMap';
    if (v !== null && typeof v === 'object' && '__struct' in (v as object)) return (v as { __struct: string }).__struct;
    if (v !== null && typeof v === 'object' && '__enum' in (v as object)) return (v as { __enum: string }).__enum;
    return 'unit';
  }),
  hash: nat('hash', (a) => {
    // FNV-1a 64（BigInt 输出）
    let h = 0xcbf29ce484222325n;
    for (const c of debug(a[0])) {
      h ^= BigInt(c.codePointAt(0) ?? 0);
      h = (h * 0x100000001b3n) & 0xffffffffffffffffn;
    }
    return h;
  }),
};

// ---------------------------------------------------------------------------
// std/collections
// ---------------------------------------------------------------------------
const stdCollections: StdModule = {
  vec: nat('vec', (a) => a),
  repeat_vec: nat('repeat_vec', (a) => new Array(N(a[1])).fill(cloneValue(a[0]))),
  zip: nat('zip', (a) => {
    const x = V(a[0]), y = V(a[1]);
    const n = Math.min(x.length, y.length);
    const out: unknown[] = [];
    for (let i = 0; i < n; i++) out.push([x[i], y[i]]);
    return out;
  }),
  chunk: nat('chunk', (a) => {
    const v = V(a[0]);
    const n = Math.max(1, N(a[1]));
    const out: unknown[] = [];
    for (let i = 0; i < v.length; i += n) out.push(v.slice(i, i + n));
    return out;
  }),
  dedup: nat('dedup', (a) => {
    const v = V(a[0]);
    const out: unknown[] = [];
    for (const x of v) {
      if (out.length === 0 || !deepEq(out[out.length - 1], x)) out.push(x);
    }
    return out;
  }),
  unique: nat('unique', (a) => {
    const v = V(a[0]);
    const out: unknown[] = [];
    for (const x of v) if (!out.some((y) => deepEq(y, x))) out.push(x);
    return out;
  }),
  flatten: nat('flatten', (a) => {
    const v = V(a[0]);
    const out: unknown[] = [];
    for (const x of v) {
      if (Array.isArray(x)) out.push(...x);
      else out.push(x);
    }
    return out;
  }),
  sort_desc: nat('sort_desc', (a) => [...V(a[0])].sort((x, y) => (N(y) - N(x)))),
  reverse: nat('reverse', (a) => [...V(a[0])].reverse()),
  swap_remove: nat('swap_remove', (a) => {
    const v = V(a[0]);
    const i = N(a[1]);
    if (i < 0 || i >= v.length) throw new Error(`swap_remove 索引越界：${i}`);
    const out = v[i];
    v[i] = v[v.length - 1]!;
    v.pop();
    return out;
  }),
};

// ---------------------------------------------------------------------------
// std/text
// ---------------------------------------------------------------------------
const stdText: StdModule = {
  split_once: nat('split_once', (a) => {
    const s = S(a[0]), sep = S(a[1]);
    const i = s.indexOf(sep);
    return i < 0 ? noneV() : someV([s.slice(0, i), s.slice(i + sep.length)]);
  }),
  rsplit_once: nat('rsplit_once', (a) => {
    const s = S(a[0]), sep = S(a[1]);
    const i = s.lastIndexOf(sep);
    return i < 0 ? noneV() : someV([s.slice(0, i), s.slice(i + sep.length)]);
  }),
  split_at: nat('split_at', (a) => {
    const s = S(a[0]), i = N(a[1]);
    return [s.slice(0, i), s.slice(i)];
  }),
  to_snake: nat('to_snake', (a) => S(a[0]).replace(/([a-z0-9])([A-Z])/g, '$1_$2').toLowerCase()),
  to_camel: nat('to_camel', (a) => S(a[0]).replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase())),
  to_pascal: nat('to_pascal', (a) => {
    const s = S(a[0]).replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase());
    return s ? s[0]!.toUpperCase() + s.slice(1) : s;
  }),
  to_kebab: nat('to_kebab', (a) => S(a[0]).replace(/([a-z0-9])([A-Z])/g, '$1-$2').toLowerCase()),
  pad_start: nat('pad_start', (a) => {
    const s = S(a[0]), n = N(a[1]), ch = a[2] !== undefined ? S(a[2]) : ' ';
    return s.length >= n ? s : ch.repeat(n - s.length).slice(0, n - s.length) + s;
  }),
  pad_end: nat('pad_end', (a) => {
    const s = S(a[0]), n = N(a[1]), ch = a[2] !== undefined ? S(a[2]) : ' ';
    return s.length >= n ? s : s + ch.repeat(n - s.length).slice(0, n - s.length);
  }),
  capitalize: nat('capitalize', (a) => {
    const s = S(a[0]);
    return s ? s[0]!.toUpperCase() + s.slice(1) : s;
  }),
  count: nat('count', (a) => S(a[0]).split(S(a[1])).length - 1),
  is_alpha: nat('is_alpha', (a) => /^[A-Za-z\u0080-\uFFFF]+$/.test(S(a[0]))),
  is_numeric: nat('is_numeric', (a) => /^[0-9]+(\.[0-9]+)?$/.test(S(a[0]))),
  is_alphanumeric: nat('is_alphanumeric', (a) => /^[A-Za-z0-9\u0080-\uFFFF]+$/.test(S(a[0]))),
  truncate: nat('truncate', (a) => {
    const s = S(a[0]), n = N(a[1]);
    const ell = a[2] !== undefined ? S(a[2]) : '…';
    return [...s].length <= n ? s : [...s].slice(0, Math.max(0, n - [...ell].length)).join('') + ell;
  }),
  levenshtein: nat('levenshtein', (a) => {
    const x = [...S(a[0])], y = [...S(a[1])];
    const dp: number[][] = Array.from({ length: x.length + 1 }, (_, i) => [i, ...new Array(y.length).fill(0)]);
    for (let j = 0; j <= y.length; j++) dp[0]![j] = j;
    for (let i = 1; i <= x.length; i++) {
      for (let j = 1; j <= y.length; j++) {
        dp[i]![j] = Math.min(
          dp[i - 1]![j]! + 1,
          dp[i]![j - 1]! + 1,
          dp[i - 1]![j - 1]! + (x[i - 1] === y[j - 1] ? 0 : 1),
        );
      }
    }
    return dp[x.length]![y.length]!;
  }),
};

// ---------------------------------------------------------------------------
// std/math
// ---------------------------------------------------------------------------
const stdMath: StdModule = {
  PI: Math.PI,
  E: Math.E,
  sin: nat('sin', (a) => Math.sin(N(a[0]))),
  cos: nat('cos', (a) => Math.cos(N(a[0]))),
  tan: nat('tan', (a) => Math.tan(N(a[0]))),
  asin: nat('asin', (a) => Math.asin(N(a[0]))),
  acos: nat('acos', (a) => Math.acos(N(a[0]))),
  atan: nat('atan', (a) => Math.atan(N(a[0]))),
  atan2: nat('atan2', (a) => Math.atan2(N(a[0]), N(a[1]))),
  exp: nat('exp', (a) => Math.exp(N(a[0]))),
  ln: nat('ln', (a) => Math.log(N(a[0]))),
  log2: nat('log2', (a) => Math.log2(N(a[0]))),
  log10: nat('log10', (a) => Math.log10(N(a[0]))),
  pow: nat('pow', (a) => Math.pow(N(a[0]), N(a[1]))),
  sqrt: nat('sqrt', (a) => Math.sqrt(N(a[0]))),
  gcd: nat('gcd', (a) => {
    let x = Math.abs(N(a[0])), y = Math.abs(N(a[1]));
    while (y !== 0) { const t = y; y = x % y; x = t; }
    return x;
  }),
  lcm: nat('lcm', (a) => {
    let x = Math.abs(N(a[0])), y = Math.abs(N(a[1]));
    while (y !== 0) { const t = y; y = x % y; x = t; }
    return x === 0 ? 0 : Math.abs(N(a[0]) * N(a[1])) / x;
  }),
  signum: nat('signum', (a) => Math.sign(N(a[0]))),
  isqrt: nat('isqrt', (a) => {
    const n = Math.floor(N(a[0]));
    if (n < 0) throw new Error('isqrt 负数');
    return Math.floor(Math.sqrt(n));
  }),
  div_ceil: nat('div_ceil', (a) => Math.ceil(N(a[0]) / N(a[1]))),
  div_floor: nat('div_floor', (a) => Math.floor(N(a[0]) / N(a[1]))),
  rem_euclid: nat('rem_euclid', (a) => {
    const x = N(a[0]), y = N(a[1]);
    const r = x % y;
    return r < 0 ? r + Math.abs(y) : r;
  }),
  hypot: nat('hypot', (a) => Math.hypot(N(a[0]), N(a[1]))),
  is_nan: nat('is_nan', (a) => Number.isNaN(N(a[0]))),
  is_infinite: nat('is_infinite', (a) => !Number.isFinite(N(a[0])) && !Number.isNaN(N(a[0]))),
  inf: nat('inf', () => Number.POSITIVE_INFINITY),
};

// ---------------------------------------------------------------------------
// std/io（宿主路径监狱；无宿主时返回 Err）
// ---------------------------------------------------------------------------
function hostOf(ctx: StdCtx): { fs: { read(p: string): string; write(p: string, c: string): number; list(d?: string): string } } | null {
  const h = ctx.hostApi as { fs?: unknown } | null;
  return h && typeof h === 'object' && 'fs' in h && h.fs ? (h as { fs: { read(p: string): string; write(p: string, c: string): number; list(d?: string): string } }) : null;
}

const stdIo: StdModule = {
  read_file: nat('read_file', (a, ctx) => {
    const h = hostOf(ctx);
    if (!h) return errV('std/io::read_file 需要宿主运行时（dhv-ts run 模式）');
    try { return okV(h.fs.read(S(a[0]))); } catch (e) { return errV((e as Error).message); }
  }),
  write_file: nat('write_file', (a, ctx) => {
    const h = hostOf(ctx);
    if (!h) return errV('std/io::write_file 需要宿主运行时（dhv-ts run 模式）');
    try { return okV(h.fs.write(S(a[0]), S(a[1]))); } catch (e) { return errV((e as Error).message); }
  }),
  append_file: nat('append_file', (a, ctx) => {
    const h = hostOf(ctx);
    if (!h) return errV('std/io::append_file 需要宿主运行时');
    try {
      const p = S(a[0]);
      let prev = '';
      try { prev = h.fs.read(p); } catch { /* 新文件 */ }
      return okV(h.fs.write(p, prev + S(a[1])));
    } catch (e) { return errV((e as Error).message); }
  }),
  list_dir: nat('list_dir', (a, ctx) => {
    const h = hostOf(ctx);
    if (!h) return errV('std/io::list_dir 需要宿主运行时');
    try { return okV(h.fs.list(S(a[0] ?? '.')).split('\n').filter(Boolean)); } catch (e) { return errV((e as Error).message); }
  }),
};

// ---------------------------------------------------------------------------
// std/json（本地实现，无宿主依赖）
// ---------------------------------------------------------------------------
const stdJson: StdModule = {
  parse: nat('parse', (a) => {
    try { return okV(localJsonParse(S(a[0]))); } catch (e) { return errV(`JSON 解析失败：${(e as Error).message}`); }
  }),
  stringify: nat('stringify', (a) => localJsonStringify(a[0])),
  get: nat('get', (a) => {
    const obj = a[0] as Record<string, unknown> | null;
    const key = S(a[1]);
    if (obj === null || obj === undefined || typeof obj !== 'object' || Array.isArray(obj) || (obj instanceof Map)) return noneV();
    return key in obj ? someV(obj[key]) : noneV();
  }),
};

// 极简确定 JSON 解析器（返回 plain 对象/数组/标量；数组与元组同构为 Vec）
function localJsonParse(src: string): unknown {
  let i = 0;
  const ws = (): void => { while (i < src.length && /\s/.test(src[i]!)) i++; };
  const value = (): unknown => {
    ws();
    const c = src[i]!;
    if (c === '{') {
      i++;
      const obj: Record<string, unknown> = {};
      ws();
      if (src[i] === '}') { i++; return obj; }
      for (;;) {
        ws();
        const k = string_();
        ws();
        if (src[i] !== ':') throw new Error(`期望 ':' 于 ${i}`);
        i++;
        obj[k] = value();
        ws();
        if (src[i] === ',') { i++; continue; }
        if (src[i] === '}') { i++; return obj; }
        throw new Error(`期望 ',' 或 '}' 于 ${i}`);
      }
    }
    if (c === '[') {
      i++;
      const arr: unknown[] = [];
      ws();
      if (src[i] === ']') { i++; return arr; }
      for (;;) {
        arr.push(value());
        ws();
        if (src[i] === ',') { i++; continue; }
        if (src[i] === ']') { i++; return arr; }
        throw new Error(`期望 ',' 或 ']' 于 ${i}`);
      }
    }
    if (c === '"') return string_();
    if (src.startsWith('true', i)) { i += 4; return true; }
    if (src.startsWith('false', i)) { i += 5; return false; }
    if (src.startsWith('null', i)) { i += 4; return null; }
    const numStart = i;
    while (i < src.length && /[-+0-9eE.]/.test(src[i]!)) i++;
    if (i === numStart) throw new Error(`非法字符 '${c}' 于 ${i}`);
    const text = src.slice(numStart, i);
    return Number(text);
  };
  const string_ = (): string => {
    ws();
    if (src[i] !== '"') throw new Error(`期望字符串于 ${i}`);
    i++;
    let out = '';
    while (i < src.length && src[i] !== '"') {
      if (src[i] === '\\') {
        const e = src[i + 1]!;
        out += e === 'n' ? '\n' : e === 't' ? '\t' : e === 'r' ? '\r' : e === 'u' ? String.fromCharCode(parseInt(src.slice(i + 2, i + 6), 16)) : e;
        i += e === 'u' ? 6 : 2;
        continue;
      }
      out += src[i]!;
      i++;
    }
    i++;
    return out;
  };
  const v = value();
  ws();
  if (i < src.length) throw new Error(`多余内容于 ${i}`);
  return v;
}

function localJsonStringify(v: unknown): string {
  if (v === undefined) return 'null';
  if (typeof v === 'string') return JSON.stringify(v);
  if (typeof v === 'number' || typeof v === 'boolean' || v === null) return String(v);
  if (typeof v === 'bigint') return String(v);
  if (Array.isArray(v)) return `[${v.map(localJsonStringify).join(',')}]`;
  if (v instanceof Map) {
    return `{${[...v.entries()].map(([k, val]) => `${JSON.stringify(String(k))}:${localJsonStringify(val)}`).join(',')}}`;
  }
  if (typeof v === 'object' && v !== null && '__struct' in (v as object)) {
    const entries = Object.entries(v as Record<string, unknown>).filter(([k]) => k !== '__struct');
    return `{${entries.map(([k, val]) => `${JSON.stringify(k)}:${localJsonStringify(val)}`).join(',')}}`;
  }
  if (typeof v === 'object' && v !== null && '__enum' in (v as object)) {
    const e = v as { __enum: string; variant: string; payload?: { named?: Record<string, unknown>; tuple?: unknown[] } };
    if (!e.payload) return JSON.stringify(e.variant);
    if (e.payload.tuple && e.payload.tuple.length > 0) return localJsonStringify(e.payload.tuple);
    if (e.payload.named) return `{${Object.entries(e.payload.named).map(([k, val]) => `${JSON.stringify(k)}:${localJsonStringify(val)}`).join(',')}}`;
    return JSON.stringify(e.variant);
  }
  try { return JSON.stringify(v); } catch { return JSON.stringify(String(v)); }
}

// ---------------------------------------------------------------------------
// std/time
// ---------------------------------------------------------------------------
const stdTime: StdModule = {
  now_ms: nat('now_ms', () => Date.now()),
  now_iso: nat('now_iso', () => new Date().toISOString()),
  duration_desc: nat('duration_desc', (a) => {
    const ms = N(a[0]);
    if (ms < 1000) return `${Math.round(ms)}ms`;
    if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
    if (ms < 3_600_000) return `${Math.floor(ms / 60_000)}m${Math.round((ms % 60_000) / 1000)}s`;
    return `${Math.floor(ms / 3_600_000)}h${Math.floor((ms % 3_600_000) / 60_000)}m`;
  }),
};

// ---------------------------------------------------------------------------
// std/random（可复现 PRNG：mulberry32，默认种子 42）
// ---------------------------------------------------------------------------
let rngState = 42;
function rngNext(): number {
  rngState = (rngState + 0x6d2b79f5) | 0;
  let t = rngState;
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}

const stdRandom: StdModule = {
  seed: nat('seed', (a) => { rngState = N(a[0]) | 0; return undefined; }),
  random: nat('random', () => rngNext()),
  int_in: nat('int_in', (a) => {
    const lo = Math.ceil(N(a[0])), hi = Math.floor(N(a[1]));
    return lo + Math.floor(rngNext() * (hi - lo + 1));
  }),
  choice: nat('choice', (a) => {
    const v = V(a[0]);
    if (v.length === 0) return noneV();
    return someV(v[Math.floor(rngNext() * v.length)]);
  }),
  shuffle: nat('shuffle', (a) => {
    const v = [...V(a[0])];
    for (let i = v.length - 1; i > 0; i--) {
      const j = Math.floor(rngNext() * (i + 1));
      [v[i], v[j]] = [v[j]!, v[i]!];
    }
    return v;
  }),
  uuid_v4: nat('uuid_v4', () => {
    const hex = '0123456789abcdef';
    let out = '';
    for (let i = 0; i < 36; i++) {
      if (i === 8 || i === 13 || i === 18 || i === 23) { out += '-'; continue; }
      if (i === 14) { out += '4'; continue; }
      out += hex[Math.floor(rngNext() * 16)];
    }
    return out;
  }),
};

// ---------------------------------------------------------------------------
// std/env
// ---------------------------------------------------------------------------
const stdEnv: StdModule = {
  env_get: nat('env_get', (a) => {
    const v = process.env[S(a[0])];
    return v === undefined ? noneV() : someV(v);
  }),
  task_text: nat('task_text', (_a, ctx) => {
    const h = ctx.hostApi as { config?: { task?: string } } | null;
    return h?.config?.task ?? '';
  }),
  model_name: nat('model_name', (_a, ctx) => {
    const h = ctx.hostApi as { config?: { model?: string } } | null;
    return h?.config?.model ?? 'scripted';
  }),
  workspace: nat('workspace', (_a, ctx) => {
    const h = ctx.hostApi as { config?: { workspace?: string } } | null;
    return h?.config?.workspace ?? process.cwd();
  }),
};

// ---------------------------------------------------------------------------
// std/iter
// ---------------------------------------------------------------------------
const stdIter: StdModule = {
  range: nat('range', (a) => {
    const lo = N(a[0]), hi = N(a[1]);
    if (hi - lo > 1_000_000) throw new Error(`std/iter::range 范围过大（${hi - lo} > 1e6）`);
    const out: unknown[] = [];
    for (let i = lo; i < hi; i++) out.push(i);
    return out;
  }),
  range_step: nat('range_step', (a) => {
    const lo = N(a[0]), hi = N(a[1]), step = Math.abs(N(a[2])) || 1;
    const out: unknown[] = [];
    if (step === 0 || Math.abs(hi - lo) / step > 1_000_000) throw new Error('range_step 非法（step=0 或范围过大）');
    if (lo <= hi) for (let i = lo; i < hi; i += step) out.push(i);
    else for (let i = lo; i > hi; i -= step) out.push(i);
    return out;
  }),
  enumerate: nat('enumerate', (a) => V(a[0]).map((v, i) => [i, v])),
  chain: nat('chain', (a) => [...V(a[0]), ...V(a[1])]),
  take: nat('take', (a) => V(a[0]).slice(0, N(a[1]))),
  skip: nat('skip', (a) => V(a[0]).slice(N(a[1]))),
  min_of: nat('min_of', (a) => {
    const v = V(a[0]);
    if (v.length === 0) return noneV();
    return someV(v.reduce((m, x) => (N(x) < N(m) ? x : m)));
  }),
  max_of: nat('max_of', (a) => {
    const v = V(a[0]);
    if (v.length === 0) return noneV();
    return someV(v.reduce((m, x) => (N(x) > N(m) ? x : m)));
  }),
};

// ---------------------------------------------------------------------------
// 注册表
// ---------------------------------------------------------------------------
export const STD_MODULES: Record<string, StdModule> = {
  'std/core': stdCore,
  'std/collections': stdCollections,
  'std/text': stdText,
  'std/math': stdMath,
  'std/io': stdIo,
  'std/json': stdJson,
  'std/time': stdTime,
  'std/random': stdRandom,
  'std/env': stdEnv,
  'std/iter': stdIter,
};

export function isStdPath(p: string): boolean {
  return p === 'std' || p.startsWith('std/');
}

export const STD_MODULE_COUNT = Object.keys(STD_MODULES).length;
export const STD_FN_COUNT = Object.values(STD_MODULES).reduce(
  (n, m) => n + Object.values(m).filter((v) => typeof v === 'object' && v !== null && '__nativefn' in (v as object)).length, 0);
