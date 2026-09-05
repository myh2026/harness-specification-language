// ============================================================================
// dhv-ts/src/interp.ts — HSL 树遍历解释器
// ============================================================================

import * as A from './ast';
import * as path from 'node:path';
import { Token, Lexer } from './lexer';
import { Parser, parseExprsFromTokens } from './parser';
import {
  HValue, HStruct, HEnum, HRuntimeError, isStruct, isEnum, isClosure, isFn, isGraphVal, isBlockRes,
  HBlockRes,
  isOption, isResult, enumOf, someV, noneV, okV, errV, display, debug, deepEq, cloneValue, hslFormat,
} from './values';
import { builtinMethodFor, MethodCtx } from './builtins';
import { evalNativeBlock } from './native';
import { STD_MODULES, NativeFn, isStdPath } from './std';
import { JS_RESERVED } from './lexer';

/** std 原生函数值判定 */
function isNativeFnVal(v: unknown): v is NativeFn {
  return typeof v === 'object' && v !== null && '__nativefn' in (v as object);
}

// ---- 控制流信号 ----
export class EarlyReturn extends Error {
  constructor(public value: unknown) { super('EarlyReturn'); }
}
export class ReturnSignal extends Error {
  constructor(public value: unknown) { super('ReturnSignal'); }
}
export class BreakSignal extends Error {
  constructor(public label: string | undefined, public value: unknown) { super('BreakSignal'); }
}
export class ContinueSignal extends Error {
  constructor(public label: string | undefined) { super('ContinueSignal'); }
}

// ---- 环境 ----
export class Env {
  vars = new Map<string, { value: unknown; mut: boolean; floatTy?: boolean }>();
  constructor(public parent?: Env) {}
  lookup(name: string): { value: unknown; mut: boolean; floatTy?: boolean } | undefined {
    const hit = this.vars.get(name);
    if (hit) return hit;
    return this.parent?.lookup(name);
  }
  declare(name: string, value: unknown, mut: boolean, floatTy?: boolean): void {
    this.vars.set(name, { value, mut, floatTy });
  }
  set(name: string, value: unknown): boolean {
    const hit = this.vars.get(name);
    if (hit) {
      if (!hit.mut) throw new HRuntimeError(`不能对不可变绑定 "${name}" 赋值（S4：不可变优先，请用 let mut）`);
      hit.value = value;
      return true;
    }
    if (this.parent) return this.parent.set(name, value);
    return false;
  }
}

interface Frame {
  ret?: A.HType;
  module: string;
  name: string;
}

interface MethodEntry {
  fn: A.FnDef;
  module: string;
}

interface GraphCtx {
  name: string;
  edges: A.EdgeDecl[];
}

export interface ModuleInfo {
  file: string;
  ast: A.File;
  env: Env;
  exports: Set<string>;
}

export interface InterpOptions {
  hostApi: unknown;
  stdout: (line: string) => void;
  stderr: (line: string) => void;
  scale?: string;
}

export class Interp {
  modules = new Map<string, ModuleInfo>();
  structs = new Map<string, A.Item>();
  enums = new Map<string, A.Item>();
  traits = new Map<string, A.Item>();
  // impls: typeName -> { inherent: Map<method, FnDef>, traits: Map<trait, Map<method, FnDef>> }
  impls = new Map<string, { inherent: Map<string, MethodEntry>; traits: Map<string, Map<string, MethodEntry>> }>();
  fromImpls = new Map<string, Map<string, MethodEntry>>(); // target -> src -> from fn
  private frames: Frame[] = [];
  private graphStack: GraphCtx[] = [];
  hostApi: unknown;
  stdout: (line: string) => void;
  stderr: (line: string) => void;
  scale: string;

  constructor(opts: InterpOptions) {
    this.hostApi = opts.hostApi;
    this.stdout = opts.stdout;
    this.stderr = opts.stderr;
    this.scale = opts.scale ?? 'microkernel';
  }

  // ==========================================================================
  // 模块注册
  // ==========================================================================
  addModule(file: string, ast: A.File): ModuleInfo {
    const env = new Env();
    const exports = new Set<string>();
    const info: ModuleInfo = { file, ast, env, exports };
    this.modules.set(file, info);
    for (const item of ast.items) this.registerItem(item, info);
    return info;
  }

  /** 标准库虚拟模块（std/<mod>，BNF v1.4 附录 C）—— 在 linkProgram 前注册 */
  addStdModules(): void {
    for (const [modPath, mod] of Object.entries(STD_MODULES)) {
      if (this.modules.has(modPath)) continue;
      const env = new Env();
      const exports = new Set<string>();
      for (const [name, val] of Object.entries(mod)) {
        env.declare(name, val, false);
        exports.add(name);
      }
      const info: ModuleInfo = { file: modPath, ast: { file: modPath, items: [] }, env, exports };
      this.modules.set(modPath, info);
    }
  }

  registerItem(item: A.Item, info: ModuleInfo): void {
    const env = info.env;
    const mark = (name: string, exported: boolean) => { if (exported) info.exports.add(name); };
    switch (item.kind) {
      case 'struct':
        this.structs.set(item.name, item);
        env.declare(item.name, { __type: item.name }, false);
        mark(item.name, item.exported);
        break;
      case 'enum':
        this.enums.set(item.name, item);
        env.declare(item.name, { __type: item.name }, false);
        mark(item.name, item.exported);
        break;
      case 'trait':
        this.traits.set(item.name, item);
        env.declare(item.name, { __type: item.name }, false);
        mark(item.name, item.exported);
        break;
      case 'impl': {
        let m = this.impls.get(item.typeName);
        if (!m) {
          m = { inherent: new Map(), traits: new Map() };
          this.impls.set(item.typeName, m);
        }
        for (const fn of item.methods) {
          if (item.traitSegs) {
            const traitName = item.traitSegs.join('::');
            let tm = m.traits.get(traitName);
            if (!tm) { tm = new Map(); m.traits.set(traitName, tm); }
            tm.set(fn.name, { fn, module: info.file });
            // From 特化注册（§5.9）
            if (traitName === 'From') {
              const src = item.traitArgs?.[0];
              if (src && src.kind === 'path') {
                let fm = this.fromImpls.get(item.typeName);
                if (!fm) { fm = new Map(); this.fromImpls.set(item.typeName, fm); }
                fm.set(src.segs.join('::'), { fn, module: info.file });
              }
            }
          } else {
            m.inherent.set(fn.name, { fn, module: info.file });
          }
        }
        break;
      }
      case 'fn':
        env.declare(item.fn.name, { __fn: true, def: item.fn, module: info.file, name: item.fn.name }, false);
        mark(item.fn.name, item.exported);
        break;
      case 'graph':
        env.declare(item.graph.name, { __graph: true, def: item.graph, module: info.file, name: item.graph.name }, false);
        mark(item.graph.name, item.exported);
        break;
      case 'blockres':
        env.declare(item.name, { __blockres: true, item, module: info.file }, false);
        mark(item.name, item.exported);
        break;
      case 'const':
        // 延迟到第二遍（常量求值）
        mark(item.name, item.exported);
        break;
      case 'typealias':
        mark(item.name, item.exported);
        break;
      case 'import':
      case 'macrodef':
      case 'macrocallitem':
        break;
    }
  }

  // 常量求值 + import 绑定（所有模块注册完成后调用）
  async linkProgram(): Promise<void> {
    // 0. 标准库虚拟模块（先于 import 绑定）
    this.addStdModules();
    // 1. 常量（按模块加载顺序）
    for (const [, info] of this.modules) {
      for (const item of info.ast.items) {
        if (item.kind === 'const') {
          const v = await this.evalExpr(item.value, info.env);
          info.env.declare(item.name, v, false);
        }
      }
    }
    // 2. import 绑定（blockres 以标记对象按引用共享）
    //    L-1 修复（v0.2.53）：import 别名同步注册进全局类型注册表 ——
    //    此前 evalStructExpr / evalStructExpr 的枚举构造按「名字」查 this.enums，
    //    别名绑定只进了 env，导致 `import { T as A }` 后 `A::Variant {}` 构造失败、
    //    而 match 模式位却因 `this.enums.has(a)` 守卫跳过而宽松通过（解析不对称）。
    const registerTypeAlias = (alias: string, bind: unknown): void => {
      if (bind && typeof bind === 'object' && '__type' in (bind as object)) {
        const orig = (bind as { __type: string }).__type;
        if (this.enums.has(orig)) this.enums.set(alias, this.enums.get(orig)!);
        if (this.structs.has(orig)) this.structs.set(alias, this.structs.get(orig)!);
      }
    };
    for (const [, info] of this.modules) {
      for (const item of info.ast.items) {
        if (item.kind !== 'import') continue;
        const srcInfo = this.resolveImport(item.path, info.file);
        if (!srcInfo) throw new HRuntimeError(`import 路径无法解析：${item.path}（from ${info.file}）`);
        if (item.spec.t === 'glob') {
          info.env.declare(item.spec.alias, { __ns: true, env: srcInfo.env }, false);
        } else if (item.spec.t === 'single') {
          const name = item.spec.name;
          if (!srcInfo.exports.has(name)) {
            throw new HRuntimeError(`import 失败："${name}" 未被 ${srcInfo.file} export（M3）`);
          }
          const bind = srcInfo.env.lookup(name);
          info.env.declare(item.spec.alias ?? name, bind ? bind.value : undefined, false);
          if (item.spec.alias) registerTypeAlias(item.spec.alias, bind?.value);
        } else {
          for (const it of item.spec.items) {
            if (!srcInfo.exports.has(it.name)) {
              throw new HRuntimeError(`import 失败："${it.name}" 未被 ${srcInfo.file} export（M3）`);
            }
            const bind = srcInfo.env.lookup(it.name);
            info.env.declare(it.alias ?? it.name, bind ? bind.value : undefined, false);
            if (it.alias) registerTypeAlias(it.alias, bind?.value);
          }
        }
      }
    }
    // 3. 资源块渲染（{{}} 插值求值后固化为 String —— N5 编译期语义）。
    //    import 绑定与源绑定共享同一标记对象：按对象身份修补全部引用。
    for (const [, info] of this.modules) {
      for (const item of info.ast.items) {
        if (item.kind !== 'blockres') continue;
        const hit = info.env.lookup(item.name);
        if (hit && isBlockRes(hit.value)) {
          const marker = hit.value as HBlockRes;
          const text = await this.renderBlockres(marker as unknown as { __blockres: true; item: A.Item; module: string });
          for (const [, mod] of this.modules) {
            for (const [, binding] of mod.env.vars) {
              if (binding.value === marker) binding.value = text;
            }
          }
        }
      }
    }
  }

  private resolveImport(pathStr: string, fromFile: string): ModuleInfo | undefined {
    // 标准库虚拟模块
    if (isStdPath(pathStr)) return this.modules.get(pathStr);
    // 相对路径解析（与 linker 一致：path.resolve 归一化 .. 与 . 与平台分隔符）
    const base = path.dirname(fromFile);
    const abs = path.isAbsolute(pathStr) ? pathStr : path.resolve(base, pathStr);
    // 模块键是 loadProgram 的 path.resolve 形态；normalizePath 形态作兼容回退
    const norm = normalizePath(abs.split(path.sep).join('/'));
    return this.modules.get(abs)
      ?? this.modules.get(norm)
      ?? this.modules.get(norm.endsWith('.hsl') ? norm : norm + '.hsl')
      ?? this.modules.get(abs.endsWith('.hsl') ? abs : abs + '.hsl');
  }

  // ==========================================================================
  // 调用
  // ==========================================================================
  async callFn(fnDef: A.FnDef, args: unknown[], module: string, selfVal?: unknown): Promise<unknown> {
    const env = new Env(this.modules.get(module)?.env);
    let ai = 0;
    for (const p of fnDef.params) {
      if (p.self) {
        env.declare('self', selfVal, p.self === 'mutvalue' || p.self === 'refmut');
        continue;
      }
      const arg = args[ai++];
      if (arg === undefined && ai > args.length) throw new HRuntimeError(`函数 ${fnDef.name} 参数不足`);
      const binds = new Map<string, { value: unknown; mut: boolean }>();
      if (!this.matchPattern(p.pat, arg, binds, env)) {
        throw new HRuntimeError(`函数 ${fnDef.name} 第 ${ai} 个参数模式不匹配：${debug(arg)}`);
      }
      for (const [n, b] of binds) env.declare(n, b.value, b.mut || p.mut === true, isFloatTy(p.ty) && p.pat.kind === 'binding');
    }
    this.frames.push({ ret: fnDef.ret, module, name: fnDef.name });
    try {
      return await this.execBlock(fnDef.body ?? [], env, module);
    } catch (e) {
      if (e instanceof EarlyReturn) return e.value;
      if (e instanceof ReturnSignal) return e.value;
      throw e;
    } finally {
      this.frames.pop();
    }
  }

  async callClosure(closure: { params: { pat: A.Pattern; ty?: A.HType }[]; body: A.Expr; env: Env; isAsync: boolean }, args: unknown[]): Promise<unknown> {
    const env = new Env(closure.env);
    for (let i = 0; i < closure.params.length; i++) {
      const p = closure.params[i]!;
      const binds = new Map<string, { value: unknown; mut: boolean }>();
      if (!this.matchPattern(p.pat, args[i], binds, env)) {
        throw new HRuntimeError(`闭包第 ${i + 1} 个参数模式不匹配：${debug(args[i])}`);
      }
      for (const [n, b] of binds) env.declare(n, b.value, b.mut);
    }
    this.frames.push({ module: '<closure>', name: '<closure>' });
    try {
      return await this.evalExpr(closure.body, env);
    } catch (e) {
      if (e instanceof EarlyReturn) return e.value;
      if (e instanceof ReturnSignal) return e.value;
      throw e;
    } finally {
      this.frames.pop();
    }
  }

  async callGraph(graphDef: A.GraphDef, args: unknown[], module: string): Promise<unknown> {
    const env = new Env(this.modules.get(module)?.env);
    if (args.length !== graphDef.params.length && graphDef.params.length > 0) {
      // 参数可能省略尾参
      if (args.length > graphDef.params.length) {
        throw new HRuntimeError(`graph ${graphDef.name} 期望 ${graphDef.params.length} 个参数，得到 ${args.length}`);
      }
    }
    for (let i = 0; i < graphDef.params.length; i++) {
      const p = graphDef.params[i]!;
      env.declare(p.name, args[i], p.mut, isFloatTy(p.ty));
    }
    const edges: A.EdgeDecl[] = [];
    for (const gs of graphDef.body) if (gs.t === 'edge') edges.push(gs.decl);
    this.graphStack.push({ name: graphDef.name, edges });
    this.frames.push({ ret: graphDef.ret, module, name: graphDef.name });
    try {
      return await this.execGraphBody(graphDef.body, env, module);
    } catch (e) {
      if (e instanceof EarlyReturn) return e.value;
      if (e instanceof ReturnSignal) return e.value;
      throw e;
    } finally {
      this.graphStack.pop();
      this.frames.pop();
    }
  }

  private async execGraphBody(body: A.GraphStmt[], env: Env, module: string): Promise<unknown> {
    let lastValue: unknown = undefined;
    let lastIsTail = false;
    for (const gs of body) {
      if (gs.t === 'node') {
        const d = gs.decl;
        const v = d.init ? await this.evalExpr(d.init, env) : { __uninit: d.name };
        env.declare(d.name, v, d.mut);
        if (this.hostApi) {
          const host = this.hostApi as { events?: { emit: (n: string, d: unknown) => void } };
          host.events?.emit('node', { graph: this.graphStack[this.graphStack.length - 1]?.name, node: d.name, initialized: d.init !== undefined });
        }
      } else if (gs.t === 'edge') {
        // 已在 callGraph 收集
      } else if (gs.t === 'item') {
        const moduleInfo = this.modules.get(module)!;
        await this.registerItemLocal(gs.item, env, moduleInfo);
      } else {
        const v = await this.execStmt(gs.stmt, env, module);
        if (gs.stmt.kind === 'expr' && gs.stmt.hasSemi !== true) {
          lastValue = v;
          lastIsTail = true;
        } else {
          lastIsTail = false;
        }
      }
    }
    return lastIsTail ? lastValue : undefined;
  }

  private async registerItemLocal(item: A.Item, env: Env, moduleInfo: ModuleInfo): Promise<void> {
    switch (item.kind) {
      case 'fn':
        env.declare(item.fn.name, { __fn: true, def: item.fn, module: moduleInfo.file, name: item.fn.name }, false);
        break;
      case 'struct':
        this.structs.set(item.name, item);
        env.declare(item.name, { __type: item.name }, false);
        break;
      case 'enum':
        this.enums.set(item.name, item);
        env.declare(item.name, { __type: item.name }, false);
        break;
      case 'const': {
        const v = await this.evalExpr(item.value, env);
        env.declare(item.name, v, false);
        break;
      }
      case 'graph':
        env.declare(item.graph.name, { __graph: true, def: item.graph, module: moduleInfo.file, name: item.graph.name }, false);
        break;
      default:
        break;
    }
  }

  // ==========================================================================
  // 语句执行
  // ==========================================================================
  async execBlock(stmts: A.Stmt[], parentEnv: Env, module: string): Promise<unknown> {
    const env = new Env(parentEnv);
    let lastValue: unknown = undefined;
    let lastIsTail = false;
    for (const st of stmts) {
      const v = await this.execStmt(st, env, module);
      if (st.kind === 'expr' && st.hasSemi !== true) {
        lastValue = v;
        lastIsTail = true;
      } else {
        lastIsTail = false;
      }
    }
    return lastIsTail ? lastValue : undefined;
  }

  async execStmt(st: A.Stmt, env: Env, module: string): Promise<unknown> {
    switch (st.kind) {
      case 'let': {
        let value: unknown = undefined;
        if (st.init) value = await this.evalExpr(st.init, env);
        const binds = new Map<string, { value: unknown; mut: boolean }>();
        if (!this.matchPattern(st.pat, value, binds, env)) {
          if (st.elseBlock) {
            await this.execBlock(st.elseBlock, env, module);
            throw new HRuntimeError('let ... else 块必须发散（return/break/continue）');
          }
          throw new HRuntimeError(`let 模式不匹配：${debug(value)}（${st.pat.kind}）`);
        }
        for (const [n, b] of binds) env.declare(n, b.value, st.mut || b.mut, isFloatTy(st.ty) && st.pat.kind === 'binding');
        return undefined;
      }
      case 'expr':
        return await this.evalExpr(st.expr, env);
      case 'item': {
        const moduleInfo = this.modules.get(module)!;
        await this.registerItemLocal(st.item, env, moduleInfo);
        return undefined;
      }
      case 'empty':
        return undefined;
    }
  }

  // ==========================================================================
  // 表达式求值
  // ==========================================================================
  async evalExpr(e: A.Expr, env: Env): Promise<unknown> {
    switch (e.kind) {
      case 'lit': {
        const l = e.lit;
        if (l.t === 'int') return l.v;
        if (l.t === 'float') return l.v;
        if (l.t === 'str') return l.v;
        if (l.t === 'char') return l.v;
        return l.v;
      }
      case 'unit':
        return undefined;
      case 'path':
        return this.evalPath(e.segs, env);
      case 'binary':
        return this.evalBinary(e, env);
      case 'unary': {
        const v = await this.evalExpr(e.operand, env);
        if (e.op === '-') {
          if (typeof v === 'number') return -v;
          if (typeof v === 'bigint') return -v;
          throw new HRuntimeError(`一元 "-" 不能用于 ${typeNameOf(v)}`);
        }
        if (e.op === '!') {
          if (typeof v === 'boolean') return !v;
          if (typeof v === 'number') return ~v;
          throw new HRuntimeError(`一元 "!" 不能用于 ${typeNameOf(v)}`);
        }
        return v; // & / &mut / * —— 解释器透明
      }
      case 'assign': {
        const value = e.op === '='
          ? await this.evalExpr(e.value, env)
          : await this.evalCompound(e.op, e.target, e.value, env);
        this.assignPlace(e.target, value, env);
        return undefined;
      }
      case 'call':
        return this.evalCall(e, env);
      case 'method':
        return this.evalMethod(e, env);
      case 'field': {
        const recv = await this.evalExpr(e.recv, env);
        return this.readField(recv, e.name);
      }
      case 'index': {
        const recv = await this.evalExpr(e.recv, env);
        const idx = await this.evalExpr(e.index, env);
        if (Array.isArray(recv)) {
          const i = Number(idx);
          if (i < 0 || i >= recv.length) throw new HRuntimeError(`索引越界：${i}（len=${recv.length}）`);
          return recv[i];
        }
        if (recv instanceof Map) return recv.get(idx);
        throw new HRuntimeError(`${typeNameOf(recv)} 不支持索引`);
      }
      case 'slice': {
        const recv = await this.evalExpr(e.recv, env);
        const lo = e.lo ? Number(await this.evalExpr(e.lo, env)) : undefined;
        const hi = e.hi ? Number(await this.evalExpr(e.hi, env)) : undefined;
        if (typeof recv === 'string') {
          const chars = [...recv];
          const l = lo ?? 0;
          const h = hi ?? chars.length;
          return chars.slice(l, e.inclusive ? h + 1 : h).join('');
        }
        if (Array.isArray(recv)) {
          const l = lo ?? 0;
          const h = hi ?? recv.length;
          return recv.slice(l, e.inclusive ? h + 1 : h);
        }
        throw new HRuntimeError(`${typeNameOf(recv)} 不支持切片`);
      }
      case 'try': {
        const v = await this.evalExpr(e.expr, env);
        if (isOption(v)) {
          if (v.variant === 'Some') return v.payload?.tuple?.[0];
          throw new EarlyReturn(noneV());
        }
        if (isResult(v)) {
          if (v.variant === 'Ok') return v.payload?.tuple?.[0];
          const err = v.payload?.tuple?.[0];
          throw new EarlyReturn(errV(await this.convertErr(err)));
        }
        throw new HRuntimeError(`"?" 只能用于 Result/Option（S5），得到 ${typeNameOf(v)}`);
      }
      case 'await':
        return await this.evalExpr(e.expr, env);
      case 'cast': {
        const v = await this.evalExpr(e.expr, env);
        return this.castValue(v, e.ty);
      }
      case 'tuple': {
        const out: unknown[] = [];
        for (const it of e.items) out.push(await this.evalExpr(it, env));
        return out;
      }
      case 'array': {
        const out: unknown[] = [];
        for (const it of e.items) out.push(await this.evalExpr(it, env));
        return out;
      }
      case 'arrayrep': {
        const v = await this.evalExpr(e.value, env);
        const n = Number(await this.evalExpr(e.count, env));
        return Array.from({ length: n }, () => cloneValue(v));
      }
      case 'struct':
        return await this.evalStructExpr(e, env);
      case 'closure':
        return { __closure: true, params: e.params, body: e.body, env, isAsync: e.isAsync } as unknown as HValue;
      case 'if': {
        const c = await this.evalExpr(e.cond, env);
        if (typeof c !== 'boolean') throw new HRuntimeError(`if 条件必须是 bool（S1：零隐式转换），得到 ${debug(c)}`);
        if (c) return await this.evalExpr(e.then, env);
        if (e.els) return await this.evalExpr(e.els, env);
        return undefined;
      }
      case 'iflet': {
        const v = await this.evalExpr(e.expr, env);
        const binds = new Map<string, { value: unknown; mut: boolean }>();
        if (this.matchPattern(e.pat, v, binds, env)) {
          const child = new Env(env);
          for (const [n, b] of binds) child.declare(n, b.value, b.mut);
          return await this.evalExpr(e.then, child);
        }
        if (e.els) return await this.evalExpr(e.els, env);
        return undefined;
      }
      case 'match':
        return await this.evalMatch(e, env);
      case 'block':
        return await this.execBlock(e.stmts, env, this.currentModule(env));
      case 'asyncblock':
        return await this.execBlock(e.stmts, env, this.currentModule(env));
      case 'loop': {
        for (;;) {
          try {
            await this.evalExpr(e.body, env);
          } catch (err) {
            if (err instanceof BreakSignal) {
              if (!err.label || err.label === e.label) return err.value;
              throw err;
            }
            if (err instanceof ContinueSignal) {
              if (!err.label || err.label === e.label) continue;
              throw err;
            }
            throw err;
          }
        }
      }
      case 'while': {
        for (;;) {
          const c = await this.evalExpr(e.cond, env);
          if (typeof c !== 'boolean') throw new HRuntimeError(`while 条件必须是 bool（S1），得到 ${debug(c)}`);
          if (!c) return undefined;
          try {
            await this.evalExpr(e.body, env);
          } catch (err) {
            if (err instanceof BreakSignal) {
              if (!err.label || err.label === e.label) return err.value;
              throw err;
            }
            if (err instanceof ContinueSignal) {
              if (!err.label || err.label === e.label) continue;
              throw err;
            }
            throw err;
          }
        }
      }
      case 'whilelet': {
        for (;;) {
          const v = await this.evalExpr(e.expr, env);
          const binds = new Map<string, { value: unknown; mut: boolean }>();
          if (!this.matchPattern(e.pat, v, binds, env)) return undefined;
          const child = new Env(env);
          for (const [n, b] of binds) child.declare(n, b.value, b.mut);
          try {
            await this.evalExpr(e.body, child);
          } catch (err) {
            if (err instanceof BreakSignal) {
              if (!err.label || err.label === e.label) return err.value;
              throw err;
            }
            if (err instanceof ContinueSignal) {
              if (!err.label || err.label === e.label) continue;
              throw err;
            }
            throw err;
          }
        }
      }
      case 'for': {
        let items: unknown[];
        if (e.range) {
          const lo = Number(await this.evalExpr(e.range.lo, env));
          const hi = Number(await this.evalExpr(e.range.hi, env));
          items = [];
          for (let i = lo; e.range.inclusive ? i <= hi : i < hi; i++) items.push(i);
        } else {
          const iter = await this.evalExpr(e.iter, env);
          // 值语境 range 对象（`let r = a..b; for i in r`）
          if (iter && typeof iter === 'object' && '__range' in iter) {
            const r = iter as { __range: true; lo: number; hi?: number; inclusive: boolean };
            items = [];
            const hi = r.hi ?? Infinity;
            for (let i = r.lo; r.inclusive ? i <= hi : i < hi; i++) items.push(i);
          } else if (Array.isArray(iter)) items = iter;
          else if (typeof iter === 'string') items = [...iter];
          else if (iter instanceof Map) items = [...iter.entries()].map(([k, v]) => [k, v]);
          else if (typeof iter === 'number') throw new HRuntimeError('for-in 数值请使用范围表达式 a..b');
          else throw new HRuntimeError(`${typeNameOf(iter)} 不可迭代`);
        }
        for (const item of items) {
          const binds = new Map<string, { value: unknown; mut: boolean }>();
          if (!this.matchPattern(e.pat, item, binds, env)) {
            throw new HRuntimeError(`for 模式不匹配：${debug(item)}`);
          }
          const child = new Env(env);
          for (const [n, b] of binds) child.declare(n, b.value, b.mut);
          try {
            await this.evalExpr(e.body, child);
          } catch (err) {
            if (err instanceof BreakSignal) {
              if (!err.label || err.label === e.label) return err.value;
              throw err;
            }
            if (err instanceof ContinueSignal) {
              if (!err.label || err.label === e.label) continue;
              throw err;
            }
            throw err;
          }
        }
        return undefined;
      }
      case 'break':
        throw new BreakSignal(e.label, e.value ? await this.evalExpr(e.value, env) : undefined);
      case 'continue':
        throw new ContinueSignal(e.label);
      case 'return':
        throw new ReturnSignal(e.value ? await this.evalExpr(e.value, env) : undefined);
      case 'range': {
        // 值语境 range 产出一个描述对象，供 for-in 消费
        const lo = e.lo ? Number(await this.evalExpr(e.lo, env)) : 0;
        const hi = e.hi ? Number(await this.evalExpr(e.hi, env)) : undefined;
        return { __range: true, lo, hi, inclusive: e.inclusive };
      }
      case 'macro':
        return await this.evalMacro(e, env);
      case 'native':
        return await evalNativeBlock(e.lang, e.body, env, this.hostApi);
    }
  }

  private currentModule(_env: Env): string {
    return this.frames[this.frames.length - 1]?.module ?? '<main>';
  }

  // ---- 二元运算 ----
  private async evalBinary(e: A.Expr & { kind: 'binary' }, env: Env): Promise<unknown> {
    if (e.op === '&&' || e.op === '||') {
      const l = await this.evalExpr(e.lhs, env);
      if (typeof l !== 'boolean') throw new HRuntimeError(`逻辑运算左操作数必须是 bool（S1），得到 ${debug(l)}`);
      if (e.op === '&&' && !l) return false;
      if (e.op === '||' && l) return true;
      const r = await this.evalExpr(e.rhs, env);
      if (typeof r !== 'boolean') throw new HRuntimeError(`逻辑运算右操作数必须是 bool（S1），得到 ${debug(r)}`);
      return r;
    }
    const l = await this.evalExpr(e.lhs, env);
    const r = await this.evalExpr(e.rhs, env);
    switch (e.op) {
      case '+':
        if (typeof l === 'string' && typeof r === 'string') return l + r;
        if (typeof l === 'number' && typeof r === 'number') return l + r;
        if (typeof l === 'bigint' || typeof r === 'bigint') return BigInt(l as number) + BigInt(r as number);
        if (Array.isArray(l) && Array.isArray(r)) return [...l, ...r];
        throw new HRuntimeError(`"+" 不能用于 ${typeNameOf(l)} 与 ${typeNameOf(r)}`);
      case '-':
        if (typeof l === 'number' && typeof r === 'number') return l - r;
        if (typeof l === 'bigint' || typeof r === 'bigint') return BigInt(l as number) - BigInt(r as number);
        throw new HRuntimeError(`"-" 不能用于 ${typeNameOf(l)} 与 ${typeNameOf(r)}`);
      case '*':
        if (typeof l === 'number' && typeof r === 'number') return l * r;
        if (typeof l === 'bigint' || typeof r === 'bigint') return BigInt(l as number) * BigInt(r as number);
        throw new HRuntimeError(`"*" 不能用于 ${typeNameOf(l)} 与 ${typeNameOf(r)}`);
      case '/':
        if (typeof l === 'number' && typeof r === 'number') {
          if (r === 0) throw new HRuntimeError('除以零');
          // v0.2.51 修复：静态浮点性优先于动态截断启发 —— `1.0/7.0`、
          // `x as f64 / y as f64` 此前因 JS 里 1.0===1 被当作整数除法截断为 0。
          // 探测与 backends/body.ts exprKind 的 float 传播同构（字面量/cast/
          // 算术二元递归）；仅当双操作数均无静态浮点信号且运行时值均为整数时
          // 才按 i64 截断（整数值浮点变量是已知近似，完整类型推导归 dhv）。
          if (exprFloaty(e.lhs, env) || exprFloaty(e.rhs, env)) return l / r;
          return Number.isInteger(l) && Number.isInteger(r) ? Math.trunc(l / r) : l / r;
        }
        if (typeof l === 'bigint' || typeof r === 'bigint') {
          if (BigInt(r as number) === 0n) throw new HRuntimeError('除以零');
          return BigInt(l as number) / BigInt(r as number);
        }
        throw new HRuntimeError(`"/" 不能用于 ${typeNameOf(l)} 与 ${typeNameOf(r)}`);
      case '%':
        // v0.2.54 L-9 修复：`/` 有除零检查而 `%` 没有 —— `5 % 0` 在 number 路径
        // 静默返回 NaN（垃圾值污染数据流）、bigint 路径抛裸 RangeError（非干净
        // HRuntimeError）。rustc 后端 deny(unconditional_panic) 编译期拒绝 ——
        // 双路径统一为运行期 HRuntimeError（与 `/` 的口径对齐）。
        if (typeof l === 'number' && typeof r === 'number') {
          if (r === 0) throw new HRuntimeError('除以零（模运算）');
          return l % r;
        }
        if (typeof l === 'bigint' || typeof r === 'bigint') {
          if (BigInt(r as number) === 0n) throw new HRuntimeError('除以零（模运算）');
          return BigInt(l as number) % BigInt(r as number);
        }
        throw new HRuntimeError(`"%" 不能用于 ${typeNameOf(l)} 与 ${typeNameOf(r)}`);
      case '&': case '|': case '^': case '<<': case '>>': {
        const ln = Number(l), rn = Number(r);
        if (!Number.isFinite(ln) || !Number.isFinite(rn)) throw new HRuntimeError(`位运算不能用于 ${debug(l)} 与 ${debug(r)}`);
        if (e.op === '&') return ln & rn;
        if (e.op === '|') return ln | rn;
        if (e.op === '^') return ln ^ rn;
        if (e.op === '<<') return ln << rn;
        return ln >> rn;
      }
      case '==': return deepEq(l, r);
      case '!=': return !deepEq(l, r);
      case '<': case '>': case '<=': case '>=': {
        if (typeof l === 'string' && typeof r === 'string') {
          return e.op === '<' ? l < r : e.op === '>' ? l > r : e.op === '<=' ? l <= r : l >= r;
        }
        const ln = Number(l), rn = Number(r);
        if (Number.isNaN(ln) || Number.isNaN(rn)) throw new HRuntimeError(`比较不能用于 ${debug(l)} 与 ${debug(r)}`);
        return e.op === '<' ? ln < rn : e.op === '>' ? ln > rn : e.op === '<=' ? ln <= rn : ln >= rn;
      }
    }
    throw new HRuntimeError(`未知二元运算 ${e.op}`);
  }

  private async evalCompound(op: string, target: A.Expr, valueExpr: A.Expr, env: Env): Promise<unknown> {
    const cur = await this.evalExpr(target, env);
    const rhs = await this.evalExpr(valueExpr, env);
    const binOp = op.slice(0, -1); // "+=" -> "+"
    switch (binOp) {
      case '+':
        if (typeof cur === 'string' && typeof rhs === 'string') return cur + rhs;
        if (typeof cur === 'number' && typeof rhs === 'number') return cur + rhs;
        if (typeof cur === 'bigint' || typeof rhs === 'bigint') return BigInt(cur as number) + BigInt(rhs as number);
        throw new HRuntimeError(`"${op}" 不能用于 ${typeNameOf(cur)} 与 ${typeNameOf(rhs)}`);
      case '-':
        if (typeof cur === 'number' && typeof rhs === 'number') return cur - rhs;
        if (typeof cur === 'bigint' || typeof rhs === 'bigint') return BigInt(cur as number) - BigInt(rhs as number);
        throw new HRuntimeError(`"${op}" 不能用于 ${typeNameOf(cur)} 与 ${typeNameOf(rhs)}`);
      case '*':
        if (typeof cur === 'number' && typeof rhs === 'number') return cur * rhs;
        throw new HRuntimeError(`"${op}" 不能用于 ${typeNameOf(cur)} 与 ${typeNameOf(rhs)}`);
      case '/':
        if (typeof cur === 'number' && typeof rhs === 'number') {
          if (rhs === 0) throw new HRuntimeError('除以零');
          // v0.2.51：与 evalBinary '/' 同款静态浮点性探测（`x /= 7.0` 类）
          if (exprFloaty(valueExpr, env) || exprFloaty(target, env)) return cur / rhs;
          return Number.isInteger(cur) && Number.isInteger(rhs) ? Math.trunc(cur / rhs) : cur / rhs;
        }
        throw new HRuntimeError(`"${op}" 不能用于 ${typeNameOf(cur)} 与 ${typeNameOf(rhs)}`);
      case '%':
        if (typeof cur === 'number' && typeof rhs === 'number') return cur % rhs;
        throw new HRuntimeError(`"${op}" 不能用于 ${typeNameOf(cur)} 与 ${typeNameOf(rhs)}`);
      default:
        throw new HRuntimeError(`不支持的复合赋值 ${op}`);
    }
  }

  // ---- place 赋值 ----
  private assignPlace(target: A.Expr, value: unknown, env: Env): void {
    if (target.kind === 'path') {
      const name = target.segs.join('::');
      if (!env.set(name, value)) throw new HRuntimeError(`未绑定变量 "${name}"`);
      return;
    }
    if (target.kind === 'field') {
      const container = this.evalPlaceSync(target.recv, env);
      if (Array.isArray(container) && typeof target.name === 'number') {
        container[target.name] = value;
        return;
      }
      if (container && typeof container === 'object') {
        if (isEnum(container) && container.payload?.named) {
          container.payload.named[target.name as string] = value;
          return;
        }
        (container as Record<string, unknown>)[target.name as string] = value;
        return;
      }
      throw new HRuntimeError(`不能对 ${typeNameOf(container)} 的字段赋值`);
    }
    if (target.kind === 'index') {
      const container = this.evalPlaceSync(target.recv, env);
      // 索引需要异步求值 —— evalPlaceSync 对 index 退化为同步求值仅支持常量索引；
      // 这里直接同步计算（index 表达式按同步上下文处理）
      const idx = this.evalIndexSync(target.index, env);
      if (Array.isArray(container)) {
        const i = Number(idx);
        if (i < 0 || i > container.length) throw new HRuntimeError(`索引越界：${i}`);
        container[i] = value;
        return;
      }
      if (container instanceof Map) {
        container.set(idx, value);
        return;
      }
      throw new HRuntimeError(`不能对 ${typeNameOf(container)} 的下标赋值`);
    }
    throw new HRuntimeError(`赋值目标必须是 place 表达式（变量/字段/下标）`);
  }

  private evalIndexSync(e: A.Expr, env: Env): unknown {
    // 同步求值索引（仅支持简单情形：字面量/路径）
    if (e.kind === 'lit') return (e.lit as { v?: unknown }).v;
    if (e.kind === 'path') return this.evalPath(e.segs, env);
    throw new HRuntimeError('赋值索引仅支持字面量/变量（当前解释器限制）');
  }

  private evalPlaceSync(e: A.Expr, env: Env): unknown {
    if (e.kind === 'path') {
      const hit = env.lookup(e.segs.join('::'));
      if (!hit) throw new HRuntimeError(`未绑定变量 "${e.segs.join('::')}"`);
      return hit.value;
    }
    if (e.kind === 'field') {
      const recv = this.evalPlaceSync(e.recv, env);
      return this.readField(recv, e.name);
    }
    if (e.kind === 'index') {
      const recv = this.evalPlaceSync(e.recv, env);
      const idx = this.evalIndexSync(e.index, env);
      if (Array.isArray(recv)) return recv[Number(idx)];
      if (recv instanceof Map) return recv.get(idx);
      throw new HRuntimeError(`${typeNameOf(recv)} 不支持索引`);
    }
    throw new HRuntimeError('place 表达式仅支持 变量/字段链/下标链');
  }

  private readField(recv: unknown, name: string | number): unknown {
    if (recv === undefined || recv === null) throw new HRuntimeError(`对 unit/None 解引用字段 "${name}"`);
    if (Array.isArray(recv)) {
      if (typeof name === 'number') return recv[name];
      throw new HRuntimeError(`数组字段访问必须是数字下标`);
    }
    if (isStruct(recv)) return (recv as Record<string, unknown>)[name as string];
    if (isEnum(recv)) {
      if (recv.payload?.named && (name as string) in recv.payload.named) return recv.payload.named[name as string];
      if (recv.payload?.tuple && typeof name === 'number') return recv.payload.tuple[name];
      throw new HRuntimeError(`枚举 ${recv.__enum}::${recv.variant} 没有字段 "${name}"`);
    }
    if (recv instanceof Map) {
      if (recv.has(name)) return recv.get(name);
      throw new HRuntimeError(`HashMap 没有键 "${name}"（请用 .get() 获得 Option）`);
    }
    if (typeof recv === 'object') {
      const v = (recv as Record<string, unknown>)[name as string];
      if (v === undefined && !(name as string in (recv as object))) {
        throw new HRuntimeError(`对象没有字段 "${name}"`);
      }
      return v;
    }
    throw new HRuntimeError(`${typeNameOf(recv)} 不支持字段访问 "${name}"`);
  }

  // ---- 路径求值 ----
  private evalPath(segs: string[], env: Env): unknown {
    if (segs.length === 1) {
      const name = segs[0]!;
      const hit = env.lookup(name);
      if (hit) return hit.value;
      // 内建：None（Option::None 的糖在 check 时警告；运行期直接支持 Option::None 规范路径）
      if (name === 'None') return noneV();
      throw new HRuntimeError(`未绑定变量 "${name}"`);
    }
    // 命名空间：import * as m
    const first = env.lookup(segs[0]!);
    if (first && (first.value as { __ns?: boolean }).__ns) {
      const nsEnv = (first.value as { env: Env }).env;
      const hit = nsEnv.lookup(segs[1]!);
      if (!hit) throw new HRuntimeError(`模块 ${segs[0]} 没有 "${segs[1]}"`);
      return hit.value;
    }
    // 枚举单元变体 / 常量路径
    if (segs.length === 2) {
      const [a, b] = segs as [string, string];
      if (a === 'Option' && b === 'None') return noneV();
      if (a === 'Option' && b === 'Some') throw new HRuntimeError('Some 需要参数：Option::Some(v)');
      if (a === 'Result' && (b === 'Ok' || b === 'Err')) throw new HRuntimeError(`${b} 需要参数：Result::${b}(v)`);
      const enumItem = this.enums.get(a);
      if (enumItem && enumItem.kind === 'enum') {
        const variant = enumItem.variants.find((v) => v.name === b);
        if (variant) {
          if (variant.fields) throw new HRuntimeError(`变体 ${a}::${b} 携带数据，需用构造表达式`);
          // L-1：族名取注册表原名（a 可能是 import 别名）
          return enumOf(enumItem.name, b);
        }
      }
      // 两段常量路径：如 Kind::Fatal（const 命名空间）—— 查 env
      const v = env.lookup(segs.join('::'));
      if (v) return v.value;
      throw new HRuntimeError(`无法解析路径 "${segs.join('::')}"`);
    }
    const v = env.lookup(segs.join('::'));
    if (v) return v.value;
    throw new HRuntimeError(`无法解析路径 "${segs.join('::')}"`);
  }

  // ---- 调用 ----
  private async evalCall(e: A.Expr & { kind: 'call' }, env: Env): Promise<unknown> {
    const callee = e.callee;
    if (callee.kind === 'path') {
      const segs = callee.segs;
      const args: unknown[] = [];
      for (const a of e.args) args.push(await this.evalExpr(a, env));

      // 内建类型命名空间
      if (segs.length === 2) {
        const [a, b] = segs as [string, string];
        if (a === 'String' && b === 'from') return String(args[0] ?? '');
        if (a === 'Vec' && b === 'new') return [];
        if (a === 'Vec' && b === 'from') return Array.isArray(args[0]) ? [...args[0]] : [args[0]];
        if (a === 'HashMap' && b === 'new') return new Map();
        if (a === 'HashSet' && b === 'new') return new Map(); // set 以 map 表达
        if (a === 'String' && b === 'new') return '';
        if (a === 'String' && b === 'with_capacity') return '';
        if (a === 'Vec' && b === 'with_capacity') return [];
        if (a === 'HashMap' && b === 'with_capacity') return new Map();
        if (a === 'Option' && b === 'Some') return someV(args[0]);
        if (a === 'Option' && b === 'None') return noneV();
        if (a === 'Result' && b === 'Ok') return okV(args[0]);
        if (a === 'Result' && b === 'Err') return errV(args[0]);
        if (a === 'Box' && b === 'new') return args[0];
        // 枚举元组变体
        const enumItem = this.enums.get(a);
        if (enumItem && enumItem.kind === 'enum') {
          const variant = enumItem.variants.find((v) => v.name === b);
          if (variant) {
            // L-1：族名取注册表原名（a 可能是 import 别名）
            return enumOf(enumItem.name, b, { tuple: args });
          }
        }
        // graph 调用约定：GraphName::run(args)（BNF v1.3）
        const gval = env.lookup(a);
        if (gval && isGraphVal(gval.value) && b === 'run') {
          return await this.callGraph((gval.value as { def: A.GraphDef; module: string }).def, args, (gval.value as { module: string }).module);
        }
        // 结构体/ trait 静态方法（impl 中无 self 参数的 fn）
        const implEntry = this.impls.get(a);
        if (implEntry) {
          const entry = implEntry.inherent.get(b);
          if (entry) return await this.callFn(entry.fn, args, entry.module);
        }
        // trait 默认方法（静态调用）
        const traitItem = this.traits.get(a);
        if (traitItem && traitItem.kind === 'trait') {
          const ti = traitItem.items.find((it) => it.name === b);
          if (ti?.fn) return await this.callFn(ti.fn, args, this.moduleOfItem(traitItem));
        }
      }
      // 单段：内建变体构造 / 值调用（fn / graph / closure / native std fn）
      if (segs.length === 1) {
        const n0 = segs[0]!;
        if (n0 === 'Ok') return okV(args[0]);
        if (n0 === 'Err') return errV(args[0]);
        if (n0 === 'Some') return someV(args[0]);
        if (n0 === 'None') return noneV();
        const hit = env.lookup(n0);
        if (hit) {
          const v = hit.value;
          if (isGraphVal(v)) return await this.callGraph((v as { def: A.GraphDef; module: string }).def, args, (v as { module: string }).module);
          if (isFn(v)) return await this.callFn((v as { def: A.FnDef; module: string }).def, args, (v as { module: string }).module);
          if (isClosure(v)) return await this.callClosure(v as unknown as { params: { pat: A.Pattern }[]; body: A.Expr; env: Env; isAsync: boolean }, args);
          if (isNativeFnVal(v)) return await (v as NativeFn).fn(args, { hostApi: this.hostApi });
        }
        throw new HRuntimeError(`"${n0}" 不是可调用项`);
      }
      // 多段路径调用（Trait::method 形式等）
      const v = env.lookup(segs.join('::'));
      if (v) {
        if (isFn(v.value)) return await this.callFn((v.value as { def: A.FnDef; module: string }).def, args, (v.value as { module: string }).module);
        if (isClosure(v.value)) return await this.callClosure(v.value as unknown as { params: { pat: A.Pattern }[]; body: A.Expr; env: Env; isAsync: boolean }, args);
        if (isNativeFnVal(v.value)) return await (v.value as NativeFn).fn(args, { hostApi: this.hostApi });
      }
      throw new HRuntimeError(`无法调用 "${segs.join('::')}"`);
    }
    if (callee.kind === 'method') {
      // 方法链尾调用（罕见）：按方法派发
      return await this.evalMethod(callee, env, e.args);
    }
    if (callee.kind === 'field') {
      const recv = await this.evalExpr(callee.recv, env);
      const f = this.readField(recv, callee.name);
      const args: unknown[] = [];
      for (const a of e.args) args.push(await this.evalExpr(a, env));
      if (typeof f === 'function') return await (f as (...x: unknown[]) => unknown)(...args);
      if (isClosure(f)) return await this.callClosure(f as unknown as { params: { pat: A.Pattern }[]; body: A.Expr; env: Env; isAsync: boolean }, args);
      throw new HRuntimeError(`字段 "${callee.name}" 不是可调用项`);
    }
    const cv = await this.evalExpr(callee, env);
    const args: unknown[] = [];
    for (const a of e.args) args.push(await this.evalExpr(a, env));
    if (isClosure(cv)) return await this.callClosure(cv as unknown as { params: { pat: A.Pattern }[]; body: A.Expr; env: Env; isAsync: boolean }, args);
    if (isFn(cv)) return await this.callFn((cv as { def: A.FnDef; module: string }).def, args, (cv as { module: string }).module);
    throw new HRuntimeError(`不可调用的值：${typeNameOf(cv)}`);
  }

  private findMethodModule(fn: A.FnDef, typeName: string): string {
    void fn; void typeName;
    return this.frames[this.frames.length - 1]?.module ?? '<main>';
  }

  private moduleOfItem(item: A.Item): string {
    for (const [file, info] of this.modules) {
      if (info.ast.items.includes(item)) return file;
    }
    return '<main>';
  }

  // ---- 方法派发 ----
  private async evalMethod(e: A.Expr & { kind: 'method' }, env: Env, extraArgs?: A.Expr[]): Promise<unknown> {
    const recv = await this.evalExpr(e.recv, env);
    const argExprs = extraArgs ?? e.args;
    const args: unknown[] = [];
    for (const a of argExprs) args.push(await this.evalExpr(a, env));
    const generics = e.generics?.map((t) => (t.kind === 'path' ? t.segs.join('::') : '')).filter(Boolean) as string[];

    const name = e.name;

    // 1. 用户定义方法（struct / enum 的 impl）
    const tName = isStruct(recv) ? recv.__struct : isEnum(recv) ? recv.__enum : undefined;
    if (tName) {
      const entry = this.impls.get(tName);
      if (entry) {
        const inh = entry.inherent.get(name);
        if (inh) return await this.callFn(inh.fn, args, inh.module, recv);
        for (const [, methods] of entry.traits) {
          const m = methods.get(name);
          if (m) return await this.callFn(m.fn, args, m.module, recv);
        }
        // trait 默认方法回退（impl 未覆盖时）
        for (const traitName of entry.traits.keys()) {
          const tItem = this.traits.get(traitName);
          if (tItem?.kind !== 'trait') continue;
          const ti = tItem.items.find((it) => it.kind === 'fn' && it.fn?.name === name);
          if (ti?.fn) return await this.callFn(ti.fn, args, this.moduleOfItem(tItem), recv);
        }
      }
    }

    // 2. 内建方法
    const builtin = builtinMethodFor(recv, name);
    if (builtin) {
      const ctx: MethodCtx = {
        call: (closure, cargs) => this.callClosure(closure as unknown as { params: { pat: A.Pattern }[]; body: A.Expr; env: Env; isAsync: boolean }, cargs),
        generics,
      };
      if (builtin.mutating && isPlaceExpr(e.recv)) {
        ctx.setRecv = (v) => this.assignPlace(e.recv, v, env);
      }
      return await builtin.fn(recv, args, ctx);
    }

    // 3. foreign 对象方法直通
    if (recv && typeof recv === 'object' && !Array.isArray(recv) && !(recv instanceof Map)) {
      const f = (recv as Record<string, unknown>)[name];
      if (typeof f === 'function') return await (f as (...x: unknown[]) => unknown).apply(recv, args);
    }

    throw new HRuntimeError(`${typeNameOf(recv)} 没有方法 "${name}"`);
  }

  // ---- 结构体/枚举字面量 ----
  private async evalStructExpr(e: A.Expr & { kind: 'struct' }, env: Env): Promise<unknown> {
    const segs = e.segs;
    // 枚举变体（命名负载）
    if (segs.length === 2) {
      const [a, b] = segs as [string, string];
      const enumItem = this.enums.get(a);
      if (enumItem && enumItem.kind === 'enum') {
        const variant = enumItem.variants.find((v) => v.name === b);
        if (!variant) throw new HRuntimeError(`枚举 ${a} 没有变体 ${b}`);
        if (variant.fields && 'tuple' in variant.fields) throw new HRuntimeError(`元组变体 ${a}::${b} 请用调用语法`);
        const named: Record<string, unknown> = {};
        for (const f of e.fields) {
          if (f.base) {
            const base = await this.evalExpr(f.base, env);
            if (isEnum(base) && base.payload?.named) {
              for (const [k, v] of Object.entries(base.payload.named)) if (!(k in named)) named[k] = v;
            }
            continue;
          }
          if (f.value) {
            named[f.name] = await this.evalExpr(f.value, env);
          } else {
            const hit = env.lookup(f.name);
            if (!hit) throw new HRuntimeError(`简写字段 "${f.name}" 没有同名绑定`);
            named[f.name] = hit.value;
          }
        }
        if (variant.fields?.named) {
          for (const fd of variant.fields.named) {
            if (!(fd.name in named)) throw new HRuntimeError(`变体 ${a}::${b} 缺少字段 "${fd.name}"`);
          }
        }
        // L-1：族名取注册表 item 原名（a 可能是 import 别名，值必须归一到原名族）
        return enumOf(enumItem.name, b, Object.keys(named).length > 0 ? { named } : undefined);
      }
    }
    // 结构体
    if (segs.length === 1) {
      const name = segs[0]!;
      const structItem = this.structs.get(name);
      if (!structItem || structItem.kind !== 'struct') throw new HRuntimeError(`未知的结构体 "${name}"`);
      const out: Record<string, unknown> = { __struct: name };
      for (const f of e.fields) {
        if (f.base) {
          const base = await this.evalExpr(f.base, env);
          if (isStruct(base)) {
            for (const [k, v] of Object.entries(base)) if (k !== '__struct' && !(k in out)) out[k] = v;
          }
          continue;
        }
        if (f.value) {
          out[f.name] = await this.evalExpr(f.value, env);
        } else {
          const hit = env.lookup(f.name);
          if (!hit) throw new HRuntimeError(`简写字段 "${f.name}" 没有同名绑定`);
          out[f.name] = hit.value;
        }
      }
      for (const fd of structItem.fields) {
        if (!(fd.name in out)) throw new HRuntimeError(`结构体 ${name} 缺少字段 "${fd.name}"`);
      }
      return out;
    }
    throw new HRuntimeError(`无法解析的结构体字面量 "${segs.join('::')}"`);
  }

  // ---- match ----
  private async evalMatch(e: A.Expr & { kind: 'match' }, env: Env): Promise<unknown> {
    const val = await this.evalExpr(e.expr, env);
    for (const arm of e.arms) {
      const binds = new Map<string, { value: unknown; mut: boolean }>();
      if (this.matchPattern(arm.pattern, val, binds, env)) {
        const child = new Env(env);
        for (const [n, b] of binds) child.declare(n, b.value, b.mut);
        if (arm.guard) {
          const g = await this.evalExpr(arm.guard, child);
          if (typeof g !== 'boolean') throw new HRuntimeError(`match guard 必须是 bool（S1）`);
          if (!g) continue;
        }
        this.traceEdgeFire(arm.pattern);
        return await this.evalExpr(arm.body, child);
      }
    }
    throw new HRuntimeError(`match 不穷尽：值 ${debug(val)} 没有匹配的分支（S6）`);
  }

  // 边追踪（G6 运行期观测：microkernel 事件总线 / monolith 直接调用的可观测等价物）
  private traceEdgeFire(pat: A.Pattern): void {
    const ctx = this.graphStack[this.graphStack.length - 1];
    if (!ctx || !this.hostApi) return;
    const host = this.hostApi as { events?: { emit: (n: string, d: unknown) => void } };
    if (!host.events) return;
    const variants = new Set<string>();
    collectPatternVariants(pat, variants);
    if (variants.size === 0) return;
    for (const edge of ctx.edges) {
      const guardVariants = new Set<string>();
      if (edge.guardPattern) collectPatternVariants(edge.guardPattern, guardVariants);
      for (const v of variants) {
        if (guardVariants.has(v)) {
          for (let i = 0; i + 1 < edge.endpoints.length; i++) {
            host.events.emit('edge', {
              graph: ctx.name,
              from: edge.endpoints[i],
              to: edge.endpoints[i + 1],
              on: v,
              scale: this.scale,
            });
          }
        }
      }
    }
  }

  // ---- 模式匹配 ----
  matchPattern(pat: A.Pattern, val: unknown, binds: Map<string, { value: unknown; mut: boolean }>, env: Env): boolean {
    switch (pat.kind) {
      case 'wildcard':
        return true;
      case 'rest':
        return true;
      case 'literal':
        return deepEq(val, pat.value);
      case 'binding': {
        // 内建变体名在模式位置按变体解释（Ok/Err/Some/None）
        if (pat.name === 'None' || pat.name === 'Some' || pat.name === 'Ok' || pat.name === 'Err') {
          const family = pat.name === 'None' || pat.name === 'Some' ? 'Option' : 'Result';
          if (isEnum(val) && val.__enum === family && val.variant === pat.name) return true;
          return false;
        }
        const trial = new Map(binds);
        if (pat.sub) {
          const subBinds = new Map<string, { value: unknown; mut: boolean }>();
          if (!this.matchPattern(pat.sub, val, subBinds, env)) return false;
          for (const [k, v] of subBinds) trial.set(k, v);
        }
        trial.set(pat.name, { value: val, mut: pat.mut });
        for (const [k, v] of trial) binds.set(k, v);
        return true;
      }
      case 'or': {
        for (const alt of pat.alts) {
          if (this.matchPattern(alt, val, binds, env)) return true;
        }
        return false;
      }
      case 'path': {
        const segs = pat.segs;
        if (segs.length === 1) {
          const n0 = segs[0]!;
          if (n0 === 'Ok' || n0 === 'Err' || n0 === 'Some' || n0 === 'None') {
            const family = n0 === 'Ok' || n0 === 'Err' ? 'Result' : 'Option';
            if (!isEnum(val) || val.__enum !== family || val.variant !== n0) return false;
          } else {
            const cv0 = env.lookup(n0)?.value;
            if (cv0 === undefined) return false;
            if (!deepEq(val, cv0)) return false;
          }
          if (pat.sub?.kind === 'tuple') {
            const items = pat.sub.items;
            const tuple = (val as HEnum).payload?.tuple ?? [];
            if (!pat.sub.rest && items.length !== tuple.length) return false;
            if (pat.sub.rest && items.length > tuple.length) return false;
            for (let i = 0; i < items.length; i++) {
              if (!this.matchPattern(items[i]!, tuple[i], binds, env)) return false;
            }
            return true;
          }
          if (pat.sub?.kind === 'struct') {
            const named = (val as HEnum).payload?.named ?? {};
            for (const f of pat.sub.fields) {
              if (!(f.name in named)) return false;
              if (!this.matchPattern(f.pat, named[f.name], binds, env)) return false;
            }
            return true;
          }
          return true;
        }
        if (segs.length === 2) {
          const [a, b] = segs as [string, string];
          if (a === 'Option' || a === 'Result') {
            if (!isEnum(val) || val.__enum !== a || val.variant !== b) return false;
          } else {
            if (!isEnum(val) || val.variant !== b) return false;
            // L-1：经注册表解析枚举族名（别名条目映射到原名 item）
            const ei = this.enums.get(a);
            if (ei && ei.kind === 'enum' && val.__enum !== ei.name) return false;
          }
          if (pat.sub?.kind === 'tuple') {
            const items = pat.sub.items;
            const tuple = val.payload?.tuple ?? [];
            const hasRest = pat.sub.rest;
            if (!hasRest && items.length !== tuple.length) return false;
            if (hasRest && items.length > tuple.length) return false;
            for (let i = 0; i < items.length; i++) {
              if (!this.matchPattern(items[i]!, tuple[i], binds, env)) return false;
            }
            return true;
          }
          if (pat.sub?.kind === 'struct') {
            const fields = pat.sub.fields;
            const named = val.payload?.named ?? {};
            for (const f of fields) {
              if (!(f.name in named)) return false;
              if (!this.matchPattern(f.pat, named[f.name], binds, env)) return false;
            }
            return true;
          }
          // 单元变体
          return !val.payload || (val.payload.tuple?.length === 0) || Boolean(val.payload.named && Object.keys(val.payload.named).length === 0);
        }
        // 常量路径
        const cv = env.lookup(segs.join('::'))?.value;
        if (cv !== undefined) return deepEq(val, cv);
        return false;
      }
      case 'struct': {
        // 两段路径 = 枚举变体模式（Enum::Variant { fields }）——必须校验变体名！
        if (pat.segs.length === 2) {
          const [a, b] = pat.segs as [string, string];
          if (!isEnum(val)) return false;
          if (val.variant !== b) return false;
          if (a === 'Option' || a === 'Result') {
            if (val.__enum !== a) return false;
          } else {
            // L-1：经注册表解析枚举族名（别名条目映射到原名 item）
            const ei = this.enums.get(a);
            if (ei && ei.kind === 'enum' && val.__enum !== ei.name) return false;
          }
          const named = val.payload?.named ?? {};
          for (const f of pat.fields) {
            const v = named[f.name];
            if (v === undefined) return false;
            if (!this.matchPattern(f.pat, v, binds, env)) return false;
          }
          return true;
        }
        // 单段：结构体模式（按字段匹配）
        if (isStruct(val)) {
          for (const f of pat.fields) {
            const v = (val as Record<string, unknown>)[f.name];
            if (v === undefined) return false;
            if (!this.matchPattern(f.pat, v, binds, env)) return false;
          }
          return true;
        }
        return false;
      }
      case 'tuple': {
        if (!Array.isArray(val)) return false;
        if (!pat.rest && pat.items.length !== val.length) return false;
        if (pat.rest && pat.items.length > val.length) return false;
        for (let i = 0; i < pat.items.length; i++) {
          if (!this.matchPattern(pat.items[i]!, val[i], binds, env)) return false;
        }
        return true;
      }
      case 'range': {
        const n = Number(val);
        const lo = this.patternBoundValue(pat.lo, env);
        const hi = this.patternBoundValue(pat.hi, env);
        const lon = Number(lo), hin = Number(hi);
        if (Number.isNaN(n) || Number.isNaN(lon) || Number.isNaN(hin)) return false;
        return pat.inclusive ? n >= lon && n <= hin : n >= lon && n < hin;
      }
    }
  }

  private patternBoundValue(p: A.Pattern, env: Env): unknown {
    if (p.kind === 'literal') return p.value;
    if (p.kind === 'path') {
      const v = env.lookup(p.segs.join('::'))?.value;
      return v;
    }
    return undefined;
  }

  // ---- `?` 错误转换（From）----
  private async convertErr(err: unknown): Promise<unknown> {
    const frame = this.frames[this.frames.length - 1];
    if (!frame?.ret || frame.ret.kind !== 'path') return err;
    const retSegs = frame.ret.segs;
    if (retSegs[0] !== 'Result' || !frame.ret.args || frame.ret.args.length < 2) return err;
    const errTy = frame.ret.args[1]!;
    if (errTy.kind !== 'path') return err;
    const target = errTy.segs.join('::');
    const srcName = isStruct(err) ? err.__struct : isEnum(err) ? err.__enum : undefined;
    if (!srcName || srcName === target) return err;
    const fromEntry = this.fromImpls.get(target)?.get(srcName);
    if (!fromEntry) {
      throw new HRuntimeError(`缺少 From<${srcName}> for ${target} 实现（? 无法自动转换错误类型；§5.9）`);
    }
    return await this.callFn(fromEntry.fn, [err], fromEntry.module);
  }

  // ---- cast ----
  private castValue(v: unknown, ty: A.HType): unknown {
    if (ty.kind !== 'path') return v;
    const t = ty.segs.join('::');
    if (t === 'f64' || t === 'f32') return Number(v);
    if (t.startsWith('i') || t.startsWith('u')) {
      const n = Number(v);
      if (!Number.isFinite(n)) throw new HRuntimeError(`as ${t} 失败：${debug(v)}`);
      if (t === 'i64' || t === 'u64' || t === 'isize' || t === 'usize') return BigInt(Math.trunc(n));
      const bits = parseInt(t.slice(1), 10);
      const mod = bits === 8 ? 256 : bits === 16 ? 65536 : bits === 32 ? 4294967296 : Number.MAX_SAFE_INTEGER;
      const s = t.startsWith('i');
      let x = Math.trunc(n) % mod;
      if (s && x > mod / 2 - 1) x -= mod;
      if (s && x < -mod / 2) x += mod;
      if (!s && x < 0) x += mod;
      return x;
    }
    if (t === 'bool') return typeof v === 'boolean' ? v : Boolean(v);
    if (t === 'String' || t === 'str') return display(v);
    if (t === 'char') return String(v).slice(0, 1);
    return v;
  }

  // ---- 宏 ----
  private async evalMacro(e: A.Expr & { kind: 'macro' }, env: Env): Promise<unknown> {
    const name = e.path[e.path.length - 1]!;
    const toks = macroArgTokens(e.tree);
    switch (name) {
      case 'format':
      case 'println':
      case 'print':
      case 'eprintln':
      case 'panic': {
        const exprs = parseExprsFromTokens(toks, '<macro>');
        if (exprs.length === 0) {
          if (name === 'println' || name === 'eprintln') { (name === 'println' ? this.stdout : this.stderr)(''); return undefined; }
          if (name === 'panic') throw new HRuntimeError('panic!');
        }
        const first = exprs[0]!;
        if (first.kind !== 'lit' || (first.lit.t !== 'str')) {
          throw new HRuntimeError(`${name}! 第一个参数必须是字符串字面量`);
        }
        const fmt = first.lit.v as string;
        const args: unknown[] = [];
        for (let i = 1; i < exprs.length; i++) args.push(await this.evalExpr(exprs[i]!, env));
        const text = hslFormat(fmt, args);
        if (name === 'format') return text;
        if (name === 'panic') throw new HRuntimeError(`panic!: ${text}`);
        if (name === 'print') { this.stdout(text); return undefined; }
        (name === 'println' ? this.stdout : this.stderr)(text);
        return undefined;
      }
      case 'vec': {
        const exprs = parseExprsFromTokens(toks, '<macro>');
        const out: unknown[] = [];
        for (const x of exprs) out.push(await this.evalExpr(x, env));
        return out;
      }
      case 'assert': {
        const exprs = parseExprsFromTokens(toks, '<macro>');
        const v = await this.evalExpr(exprs[0]!, env);
        if (v !== true) throw new HRuntimeError(`assertion failed: ${debug(v)}`);
        return undefined;
      }
      case 'assert_eq': {
        const exprs = parseExprsFromTokens(toks, '<macro>');
        const a = await this.evalExpr(exprs[0]!, env);
        const b = await this.evalExpr(exprs[1]!, env);
        if (!deepEq(a, b)) throw new HRuntimeError(`assertion failed: ${debug(a)} != ${debug(b)}`);
        return undefined;
      }
      case 'dbg': {
        const exprs = parseExprsFromTokens(toks, '<macro>');
        const v = await this.evalExpr(exprs[0]!, env);
        this.stderr(`[dbg] = ${debug(v)}`);
        return v;
      }
      default:
        throw new HRuntimeError(`宏 "${name}" 未在此处展开（用户宏在链接期展开；内建宏：format/vec/println/print/eprintln/panic/assert/assert_eq/dbg）`);
    }
  }

  // ---- 资源块渲染 ----
  async renderBlockres(br: { __blockres: true; item: A.Item; module: string }, env?: Env): Promise<string> {
    const item = br.item as A.Item & { kind: 'blockres' };
    const moduleEnv = this.modules.get(br.module)?.env ?? env;
    let out = '';
    for (const part of item.parts) {
      if (part.t === 'text') out += part.text ?? '';
      else {
        // 插值求值（N5：编译期语义在解释器中为首次渲染求值）
        const toks = new Lexer(part.src ?? '', br.module).tokenize();
        const exprs = parseExprsFromTokens(toks, br.module);
        const v = await this.evalExpr(exprs[0] ?? { kind: 'unit', span: { line: 0, col: 0, file: '' } }, moduleEnv!);
        out += display(v);
      }
    }
    return out;
  }
}

// ---- 工具 ----
function normalizePath(p: string): string {
  const parts = p.split('/');
  const out: string[] = [];
  for (const seg of parts) {
    if (seg === '' || seg === '.') {
      if (out.length === 0 && seg === '') out.push('');
      continue;
    }
    if (seg === '..') {
      if (out.length > 0 && out[out.length - 1] !== '..' && out[out.length - 1] !== '') out.pop();
      else if (out.length === 0 || out[out.length - 1] === '') out.push('..');
      continue;
    }
    out.push(seg);
  }
  const joined = out.join('/');
  return joined.startsWith('/') ? joined : '/' + joined;
}

function isPlaceExpr(e: A.Expr): boolean {
  if (e.kind === 'path') return true;
  if (e.kind === 'field') return isPlaceExpr(e.recv);
  if (e.kind === 'index') return isPlaceExpr(e.recv);
  return false;
}

/**
 * 除法的静态浮点性探测（v0.2.51）—— 与 backends/body.ts exprKind 的 float
 * 传播同构：float 字面量 / as f32|f64 转换 / 算术二元递归 / 显式 f64·f32
 * 类型注解的绑定任一命中即浮点除法。
 * 背景：JS 中 1.0 === 1，整数值的浮点字面量在运行期丢失浮点性，此前的
 * 「双整数值 → 截断」启发把 `1.0/7.0` 静默算成 0。变量承载整数值且无
 * 注解时仍是已知近似（完整类型推导归 dhv Rust 编译器，BNF 已知限制 #1）。
 */
function exprFloaty(e: A.Expr, env?: Env): boolean {
  switch (e.kind) {
    case 'lit':
      return e.lit.t === 'float';
    case 'path': {
      // 显式 f64/f32 注解的绑定（类型在 Rust 语义下不可变更，可靠）
      if (!env) return false;
      return env.lookup(e.segs[0]!)?.floatTy === true;
    }
    case 'cast': {
      const ty = e.ty;
      if (ty.kind === 'path') {
        const last = ty.segs[ty.segs.length - 1]!;
        return last === 'f32' || last === 'f64';
      }
      return false;
    }
    case 'binary':
      if (['/', '*', '-', '+'].includes(e.op)) return exprFloaty(e.lhs, env) || exprFloaty(e.rhs, env);
      return false;
    default:
      return false;
  }
}

/** 类型注解是否为 f32/f64（浮点性追踪的静态信号） */
function isFloatTy(ty: A.HType | undefined): boolean {
  if (!ty || ty.kind !== 'path') return false;
  const last = ty.segs[ty.segs.length - 1]!;
  return last === 'f32' || last === 'f64';
}

function typeNameOf(v: unknown): string {
  if (typeof v === 'string') return 'String';
  if (typeof v === 'number' || typeof v === 'bigint') return Number.isInteger(v) ? 'int' : 'float';
  if (typeof v === 'boolean') return 'bool';
  if (Array.isArray(v)) return 'Vec';
  if (v instanceof Map) return 'HashMap';
  if (isStruct(v)) return v.__struct;
  if (isEnum(v)) return v.__enum;
  if (v === undefined) return 'unit';
  return 'foreign';
}

function collectPatternVariants(pat: A.Pattern, out: Set<string>): void {
  switch (pat.kind) {
    case 'path':
      if (pat.segs.length >= 2) out.add(pat.segs[pat.segs.length - 1]!);
      break;
    case 'struct':
      // Enum::Variant { fields } 结构模式的变体名
      if (pat.segs.length >= 2) out.add(pat.segs[pat.segs.length - 1]!);
      for (const f of pat.fields) collectPatternVariants(f.pat, out);
      break;
    case 'or':
      for (const a of pat.alts) collectPatternVariants(a, out);
      break;
    case 'tuple':
      for (const it of pat.items) collectPatternVariants(it, out);
      break;
    default:
      break;
  }
}

export function treeToTokens(tree: A.TokenTree): Token[] {
  const out: Token[] = [];
  const walk = (t: A.TokenTree) => {
    if (t.t === 'tok') {
      out.push({ kind: t.tok.kind as 'ident', text: t.tok.text, value: t.tok.value, line: t.tok.line, col: t.tok.col });
    } else {
      out.push({ kind: 'punct', text: t.open, line: 0, col: 0 });
      for (const x of t.items) walk(x);
      out.push({ kind: 'punct', text: t.close, line: 0, col: 0 });
    }
  };
  walk(tree);
  return out;
}

/** 宏实参 token：剥掉最外层定界符，保留内层结构；补 EOF 终止符 */
function macroArgTokens(tree: A.TokenTree): Token[] {
  const out: Token[] = [];
  if (tree.t === 'delim') {
    for (const x of tree.items) out.push(...treeToTokens(x));
  } else {
    out.push(...treeToTokens(tree));
  }
  out.push({ kind: 'eof', text: '<eof>', line: 0, col: 0 });
  return out;
}

export { scanCaptured } from './native';
