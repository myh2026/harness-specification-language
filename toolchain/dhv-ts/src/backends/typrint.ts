// ============================================================================
// dhv-ts/src/backends/typrint.ts — HSL 类型 → 目标语言类型
// ============================================================================

import { HType } from '../ast';
import { LangSpec } from './registry';

/** 打印 HSL 类型为目标语言类型串。未识别的类型名原样输出（P2 匹配由 checker 管）。 */
export function printType(ty: HType | undefined, lang: LangSpec): string {
  if (!ty) return '';
  switch (ty.kind) {
    case 'path': {
      const segs = ty.segs;
      const name = segs.join('::');
      const args = (ty.args ?? []).map((a) => printType(a, lang));
      return applyTypeMap(name, args, lang);
    }
    case 'ref':
      return printType(ty.inner, lang);
    case 'paren':
      return printType(ty.inner, lang);
    case 'tuple':
      return `(${ty.items.map((t) => printType(t, lang)).join(', ')})`;
    case 'array':
      return applyTypeMap('Vec', [printType(ty.elem, lang)], lang);
    case 'slice':
      return applyTypeMap('Vec', [printType(ty.elem, lang)], lang);
    case 'fnptr': {
      const params = ty.params.map((t) => printType(t, lang)).join(', ');
      const ret = ty.ret ? printType(ty.ret, lang) : lang.types.unit ?? 'void';
      if (lang.id === 'rust') return `fn(${params}) -> ${ret}`;
      if (lang.id === 'typescript' || lang.id === 'javascript') return `(${params}) => ${ret}`;
      return `${ret} (*)(${params})`;
    }
    case 'dyn': {
      const bounds = ty.bounds.join(' + ');
      if (lang.id === 'rust') return `dyn ${bounds}`;
      return bounds;
    }
    case 'implt':
      return ty.bounds.join(' + ');
    case 'infer':
      return lang.id === 'rust' ? '_' : 'any';
    case 'never':
      return lang.id === 'rust' ? '!' : 'never';
  }
}

const GENERIC_ARITY: Record<string, number> = {
  Vec: 1, HashSet: 1, Option: 1, Box: 1, Rc: 1, Arc: 1,
  HashMap: 2, BTreeMap: 2, Result: 2,
};

function applyTypeMap(name: string, args: string[], lang: LangSpec): string {
  const tm = lang.types as Record<string, string | undefined>;
  const head = segs0(name);
  const mapped = tm[head];
  if (mapped !== undefined) {
    const arity = GENERIC_ARITY[head] ?? 0;
    if (arity === 0 || mapped.indexOf('%') < 0) return mapped;
    let out = mapped;
    if (arity === 1) out = out.replace('%T', args[0] ?? 'any');
    if (arity === 2) {
      out = out.replace('%K', args[0] ?? 'any');
      out = out.replace('%V', args[1] ?? 'any');
      // Result 映射习惯是 %T %E（先值后错）——registry 中 Result 用 %T %E
      out = out.replace('%T', args[0] ?? 'any').replace('%E', args[1] ?? 'any');
    }
    return out;
  }
  // 未映射的自定义类型：拼接泛型参数
  if (args.length > 0) {
    if (lang.id === 'rust') return `${name}<${args.join(', ')}>`;
    if (lang.id === 'typescript') return `${name}<${args.join(', ')}>`;
    if (lang.id === 'go') return `${name}[${args.join(', ')}]`;
    return `${name}<${args.join(', ')}>`;
  }
  return name;
}

function segs0(name: string): string {
  const idx = name.indexOf('::');
  return idx >= 0 ? name.slice(0, idx) : name;
}
