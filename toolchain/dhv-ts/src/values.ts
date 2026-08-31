// ============================================================================
// dhv-ts/src/values.ts — 运行时值系统
// ----------------------------------------------------------------------------
// 值表示：
//   number / bigint   整数（bigint 超过 2^53）
//   number            浮点
//   boolean           bool
//   string            String / &str / char（char = 单字符 string）
//   Array             Vec<T> / 元组（统一数组）
//   Map               HashMap<K,V>
//   HStruct           { __struct, ...fields }
//   HEnum             { __enum, variant, payload? }   （含内建 Option / Result）
//   HClosure          { __closure }
//   HForeign          native 块返回的宿主对象（动态字段访问）
//   undefined         unit ()
// ============================================================================

export interface HStruct {
  __struct: string;
  [k: string]: unknown;
}
export interface HEnum {
  __enum: string;
  variant: string;
  payload?: { named?: Record<string, unknown>; tuple?: unknown[] };
}
export interface HClosure {
  __closure: true;
  params: { pat: unknown; ty?: unknown }[];
  body: unknown;
  env: unknown;
  isAsync: boolean;
}
export interface HFn {
  __fn: true;
  def: unknown;
  module: string;
  name: string;
}
export interface HGraph {
  __graph: true;
  def: unknown;
  module: string;
  name: string;
}
export interface HBlockRes {
  __blockres: true;
  item: unknown;
  module: string;
}
export interface HNamespace {
  __ns: true;
  env: unknown;
}

export type HValue = undefined | null | number | bigint | boolean | string | HValue[]
  | Map<unknown, unknown> | HStruct | HEnum | HClosure | HFn | HGraph | HBlockRes | HNamespace
  | Record<string, unknown>;

export class HRuntimeError extends Error {
  constructor(msg: string, public line?: number, public col?: number, public file?: string) {
    super(msg);
  }
}

export function isStruct(v: unknown): v is HStruct {
  return typeof v === 'object' && v !== null && !Array.isArray(v) && !(v instanceof Map) && '__struct' in (v as object);
}
export function isEnum(v: unknown): v is HEnum {
  return typeof v === 'object' && v !== null && !Array.isArray(v) && !(v instanceof Map) && '__enum' in (v as object);
}
export function isClosure(v: unknown): v is HClosure {
  return typeof v === 'object' && v !== null && '__closure' in (v as object);
}
export function isFn(v: unknown): v is HFn {
  return typeof v === 'object' && v !== null && '__fn' in (v as object);
}
export function isGraphVal(v: unknown): v is HGraph {
  return typeof v === 'object' && v !== null && '__graph' in (v as object);
}
export function isBlockRes(v: unknown): v is HBlockRes {
  return typeof v === 'object' && v !== null && '__blockres' in (v as object);
}
export function isForeign(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v) && !(v instanceof Map)
    && !isStruct(v) && !isEnum(v) && !isClosure(v) && !isFn(v) && !isGraphVal(v) && !isBlockRes(v) && !(v instanceof HRuntimeError) && !(v instanceof Error) && !(v as { __ns?: boolean }).__ns;
}

export function enumOf(enumName: string, variant: string, payload?: HEnum['payload']): HEnum {
  return { __enum: enumName, variant, payload };
}

export function someV(v: unknown): HEnum {
  return enumOf('Option', 'Some', { tuple: [v] });
}
export function noneV(): HEnum {
  return enumOf('Option', 'None');
}
export function okV(v: unknown): HEnum {
  return enumOf('Result', 'Ok', { tuple: [v] });
}
export function errV(e: unknown): HEnum {
  return enumOf('Result', 'Err', { tuple: [e] });
}

export function isOption(v: unknown): v is HEnum {
  return isEnum(v) && v.__enum === 'Option';
}
export function isResult(v: unknown): v is HEnum {
  return isEnum(v) && v.__enum === 'Result';
}

export function typeName(v: unknown): string {
  if (typeof v === 'string') return 'String';
  if (typeof v === 'number' || typeof v === 'bigint') return Number.isInteger(v) ? 'i64' : 'f64';
  if (typeof v === 'boolean') return 'bool';
  if (Array.isArray(v)) return 'Vec';
  if (v instanceof Map) return 'HashMap';
  if (isStruct(v)) return v.__struct;
  if (isEnum(v)) return v.__enum;
  if (isClosure(v)) return 'closure';
  if (isFn(v)) return 'fn';
  if (isGraphVal(v)) return 'graph';
  if (isBlockRes(v)) return 'block';
  if (v === undefined || v === null) return '()';
  return 'foreign';
}

// ---- 显示（Display）/ 调试（Debug）----
export function display(v: unknown): string {
  if (v === undefined || v === null) return '()';
  if (typeof v === 'string') return v;
  if (typeof v === 'number') return Number.isInteger(v) ? String(v) : String(v);
  if (typeof v === 'bigint') return String(v);
  if (typeof v === 'boolean') return v ? 'true' : 'false';
  if (Array.isArray(v)) return v.map(display).join(', ');
  if (v instanceof Map) return debug(v);
  if (isStruct(v)) return debug(v);
  if (isEnum(v)) {
    const p = v.payload;
    if (!p) return v.variant;
    if (p.tuple && p.tuple.length > 0) return `${v.variant}(${p.tuple.map(display).join(', ')})`;
    if (p.named) {
      const entries = Object.entries(p.named);
      if (entries.length === 0) return v.variant;
      return `${v.variant} { ${entries.map(([k, val]) => `${k}: ${display(val)}`).join(', ')} }`;
    }
    return v.variant;
  }
  return String(v);
}

export function debug(v: unknown): string {
  if (typeof v === 'string') return JSON.stringify(v);
  if (v === undefined || v === null) return '()';
  if (typeof v === 'number' || typeof v === 'bigint' || typeof v === 'boolean') return display(v);
  if (Array.isArray(v)) return `[${v.map(debug).join(', ')}]`;
  if (v instanceof Map) return `HashMap { ${[...v.entries()].map(([k, val]) => `${debug(k)}: ${debug(val)}`).join(', ')} }`;
  if (isStruct(v)) {
    const entries = Object.entries(v).filter(([k]) => k !== '__struct');
    return `${v.__struct} { ${entries.map(([k, val]) => `${k}: ${debug(val)}`).join(', ')} }`;
  }
  if (isEnum(v)) {
    const p = v.payload;
    if (!p) return v.variant;
    if (p.tuple && p.tuple.length > 0) return `${v.variant}(${p.tuple.map(debug).join(', ')})`;
    if (p.named) {
      const entries = Object.entries(p.named);
      if (entries.length === 0) return v.variant;
      return `${v.variant} { ${entries.map(([k, val]) => `${k}: ${debug(val)}`).join(', ')} }`;
    }
    return v.variant;
  }
  if (isForeign(v)) {
    try { return JSON.stringify(v); } catch { return String(v); }
  }
  return display(v);
}

// ---- 深比较 ----
export function deepEq(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a === 'number' && typeof b === 'number') return a === b;
  if (typeof a === 'bigint' || typeof b === 'bigint') {
    try { return BigInt(a as number) === BigInt(b as number); } catch { return false; }
  }
  if (typeof a === 'string' && typeof b === 'string') return a === b;
  if (typeof a === 'boolean' && typeof b === 'boolean') return a === b;
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) if (!deepEq(a[i], b[i])) return false;
    return true;
  }
  if (a instanceof Map && b instanceof Map) {
    if (a.size !== b.size) return false;
    for (const [k, v] of a) {
      if (!b.has(k)) return false;
      if (!deepEq(v, b.get(k))) return false;
    }
    return true;
  }
  if (isEnum(a) && isEnum(b)) {
    if (a.variant !== b.variant) return false;
    const pa = a.payload, pb = b.payload;
    if (!pa && !pb) return true;
    if (pa?.tuple && pb?.tuple) return pa.tuple.length === pb.tuple.length && pa.tuple.every((x, i) => deepEq(x, pb.tuple![i]));
    if (pa?.named && pb?.named) {
      const ka = Object.keys(pa.named), kb = Object.keys(pb.named);
      if (ka.length !== kb.length) return false;
      return ka.every((k) => k in pb.named! && deepEq(pa.named![k], pb.named![k]));
    }
    return false;
  }
  if (isStruct(a) && isStruct(b)) {
    const ka = Object.keys(a).filter((k) => k !== '__struct');
    const kb = Object.keys(b).filter((k) => k !== '__struct');
    if (ka.length !== kb.length) return false;
    return ka.every((k) => k in b && deepEq(a[k], b[k]));
  }
  if (isForeign(a) && isForeign(b)) {
    try { return JSON.stringify(a) === JSON.stringify(b); } catch { return false; }
  }
  return false;
}

// ---- 深克隆 ----
export function cloneValue(v: unknown): unknown {
  if (v === null || v === undefined) return v;
  if (typeof v !== 'object') return v;
  if (Array.isArray(v)) return v.map(cloneValue);
  if (v instanceof Map) {
    const m = new Map();
    for (const [k, val] of v) m.set(cloneValue(k), cloneValue(val));
    return m;
  }
  if (isStruct(v)) {
    const out: Record<string, unknown> = { __struct: v.__struct };
    for (const [k, val] of Object.entries(v)) if (k !== '__struct') out[k] = cloneValue(val);
    return out as HStruct;
  }
  if (isEnum(v)) {
    const p = v.payload;
    if (!p) return enumOf(v.__enum, v.variant);
    if (p.tuple) return enumOf(v.__enum, v.variant, { tuple: p.tuple.map(cloneValue) });
    if (p.named) {
      const named: Record<string, unknown> = {};
      for (const [k, val] of Object.entries(p.named)) named[k] = cloneValue(val);
      return enumOf(v.__enum, v.variant, { named });
    }
    return enumOf(v.__enum, v.variant);
  }
  if (isClosure(v) || isFn(v) || isGraphVal(v) || isBlockRes(v)) return v;
  // foreign：浅 JSON 克隆
  try { return JSON.parse(JSON.stringify(v)); } catch { return v; }
}

// ---- Rust 风格 format! 引擎 ----
// 支持：{} 位置参数 / {0} 索引 / {:?} 调试 / {{ }} 转义
export function hslFormat(fmt: string, args: unknown[]): string {
  let out = '';
  let ai = 0;
  let i = 0;
  while (i < fmt.length) {
    const c = fmt[i]!;
    if (c === '{' && fmt[i + 1] === '{') { out += '{'; i += 2; continue; }
    if (c === '}' && fmt[i + 1] === '}') { out += '}'; i += 2; continue; }
    if (c === '{') {
      const end = fmt.indexOf('}', i);
      if (end < 0) throw new HRuntimeError(`format! 模板缺少 "}"：${fmt}`);
      const spec = fmt.slice(i + 1, end);
      i = end + 1;
      if (spec === '' || spec === ':?' || spec === ':e' || /^:.*$/.test(spec) === false && /^\d+$/.test(spec)) {
        // {} / {:?} / {0}
        let idx = ai;
        if (/^\d+$/.test(spec)) idx = parseInt(spec, 10);
        else ai++;
        const v = args[idx];
        out += spec.includes(':?') ? debug(v) : display(v);
        continue;
      }
      // {name:...} 不支持；按位置处理
      let idx = ai;
      if (/^\d+$/.test(spec.split(':')[0] ?? '')) idx = parseInt(spec.split(':')[0]!, 10);
      else ai++;
      const v = args[idx];
      out += spec.includes(':?') ? debug(v) : display(v);
      continue;
    }
    out += c;
    i++;
  }
  return out;
}
