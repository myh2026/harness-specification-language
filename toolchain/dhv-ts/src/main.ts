#!/usr/bin/env bun
// ============================================================================
// dhv-ts/src/main.ts — CLI 入口
// ----------------------------------------------------------------------------
// 用法：
//   bun dhv-ts/src/main.ts check   <entry.hsl>
//   bun dhv-ts/src/main.ts run     <entry.hsl> [options]
//   bun dhv-ts/src/main.ts emit    <entry.hsl> --out DIR
//   bun dhv-ts/src/main.ts targets
//   bun dhv-ts/src/main.ts sync    <generated-file> [--root DIR]
//   bun dhv-ts/src/main.ts watch   <entry.hsl> --out DIR
// ============================================================================

import * as fs from 'node:fs';
import * as path from 'node:path';
import { loadProgram, LinkError, parseHslFile } from './linker';
import { Interp } from './interp';
import { Host } from './host';
import { checkProgram, formatDiags } from './checker';
import { isEnum, display, debug } from './values';
import * as A from './ast';
import { listLangs, LANGS, STATIC_LANGS } from './backends/registry';
import { emitProgram } from './backends/emit';
import { syncFile } from './backends/sync';
import { VERSION } from './version';

interface CliArgs {
  cmd: string;
  entry?: string;
  workspace: string;
  task: string;
  model: string;
  fixture?: string;
  temperature: number;
  maxTurns: number;
  maxBashCalls: number;
  maxOutputChars: number;
  allow: string[];
  scale: string;
  scaleExplicit?: boolean;
  out?: string;
  quiet: boolean;
  root?: string;
  noValidate?: boolean;
}

function parseArgs(argv: string[]): CliArgs {
  const args: CliArgs = {
    cmd: 'help',
    workspace: process.cwd(),
    task: '',
    model: 'scripted',
    temperature: 0.2,
    maxTurns: 24,
    maxBashCalls: 12,
    maxOutputChars: 4000,
    allow: ['bun', 'node', 'ls', 'cat', 'grep', 'diff'],
    scale: 'microkernel',
    quiet: false,
  };
  const rest: string[] = [];
  let i = 0;
  while (i < argv.length) {
    const a = argv[i]!;
    switch (a) {
      case 'check': case 'run': case 'emit': case 'help': case 'targets': case 'sync': case 'watch':
        args.cmd = a;
        break;
      case '--workspace': args.workspace = path.resolve(argv[++i]!); break;
      case '--task': args.task = argv[++i]!; break;
      case '--model': args.model = argv[++i]!; break;
      case '--temperature': args.temperature = parseFloat(argv[++i]!); break;
      case '--fixture': args.fixture = path.resolve(argv[++i]!); break;
      case '--max-turns': args.maxTurns = parseInt(argv[++i]!, 10); break;
      case '--max-bash': args.maxBashCalls = parseInt(argv[++i]!, 10); break;
      case '--max-output': args.maxOutputChars = parseInt(argv[++i]!, 10); break;
      case '--allow': args.allow = argv[++i]!.split(',').map((s) => s.trim()).filter(Boolean); break;
      case '--scale': args.scale = argv[++i]!; args.scaleExplicit = true; break;
      case '--out': args.out = path.resolve(argv[++i]!); break;
      case '--root': args.root = path.resolve(argv[++i]!); break;
      case '--quiet': args.quiet = true; break;
      case '--no-validate': args.noValidate = true; break;
      case '--help': case '-h': args.cmd = 'help'; break;
      default:
        if (a.startsWith('--')) {
          throw new Error(`未知参数 ${a}`);
        }
        rest.push(a);
    }
    i++;
  }
  if (rest.length > 0) args.entry = path.resolve(rest[0]!);
  return args;
}

function banner(args: CliArgs): string {
  return [
    '',
    '  ┌─────────────────────────────────────────────────┐',
    '  │  dhv-ts — HSL 参考解释器          v' + VERSION + '        │',
    '  │  Harness Specification Language · BNF v1.4.5    │',
    '  │  38 后端：32 编程语言 + 6 静态格式               │',
    '  └─────────────────────────────────────────────────┘',
    '',
  ].join('\n');
}

function usage(): void {
  console.log(banner({ cmd: 'help' } as CliArgs));
  console.log(`用法：
  bun dhv-ts/src/main.ts check <entry.hsl>
      静态检查（S/G/P/N 规则 + 模块链接）

  bun dhv-ts/src/main.ts run <entry.hsl> [options]
      解释执行（入口 = 入口文件中名为 main 的 export fn）

  bun dhv-ts/src/main.ts emit <entry.hsl> --out DIR [--scale MODE] [--no-validate]
      投射工程仓库：38 后端代码生成 + 静态资源 + manifest.json
      代码目标：struct/enum/trait/impl/fn/graph → 真实目标语言声明 + @dhv:source-map 围栏
      静态目标：block/static → yaml/md/json/toml/ini/xml（{{}} 插值渲染）
      --no-validate：跳过目标语言交叉语法校验

  bun dhv-ts/src/main.ts targets
      列出全部 38 个后端语言（tier / 能力级 / 扩展名）

  bun dhv-ts/src/main.ts sync <generated-file> [--root DIR]
      双向工程：读取生成文件中 @dhv:source-map 围栏的 HSL 镜像，
      若与 .hsl 源码不同则回写（回写后重新解析校验，失败即回滚）

  bun dhv-ts/src/main.ts watch <entry.hsl> --out DIR
      监听 .hsl 源变化 → 自动 check + emit（总纲 §6 File Watcher）

options:
  --workspace DIR     Agent 工作区（默认 cwd；路径监狱限制）
  --task TEXT         任务描述
  --model NAME        scripted（确定性剧本）| deepseek（真实 LLM）
  --temperature F     采样温度（默认 0.2）
  --fixture FILE      剧本 JSON（scripted 模式）
  --max-turns N       主循环轮数上限（默认 24）
  --max-bash N        bash 调用上限（默认 12）
  --max-output N      工具输出截断字符数（默认 4000）
  --allow a,b,c       shell 首词白名单（默认 bun,node,ls,cat,grep,diff）
  --scale MODE        microkernel | monolith
  --out DIR           产物目录（默认 .hsl-runs/<timestamp>）
  --quiet             静默轨迹
`);
}

async function cmdCheck(args: CliArgs): Promise<number> {
  if (!args.entry) { usage(); return 2; }
  try {
    const program = loadProgram(args.entry);
    const diags = checkProgram(program);
    console.log(formatDiags(diags));
    if (diags.length === 0) console.log(`✓ ${program.order.length} 个模块全部通过检查`);
    return diags.some((d) => d.severity === 'error') ? 1 : 0;
  } catch (err) {
    console.error(`error[E-0]: ${(err as Error).message}`);
    return 1;
  }
}

async function cmdEmit(args: CliArgs): Promise<number> {
  if (!args.entry || !args.out) { usage(); return 2; }
  const t0 = Date.now();
  const program = loadProgram(args.entry);
  const report = await emitProgram(program, args.out, {
    scale: args.scaleExplicit ? args.scale : undefined,
    validate: !args.noValidate,
  });
  const entryScale = program.files.get(program.entry)?.scale?.mode;
  const scale = report.scale;
  console.log(`\n投射模式：scale = ${scale}${entryScale ? '' : '（未声明，默认）'} · 入口 ${path.basename(program.entry)}`);
  for (const f of report.files) {
    const check = f.validation.ok
      ? (f.validation.tool === 'none' ? '' : ` · 语法✓ ${f.validation.tool}`)
      : ` · 语法✗ ${f.validation.tool}${f.validation.detail ? '：' + f.validation.detail.slice(0, 120) : ''}`;
    const fb = f.contract_fallbacks ? `（${f.contract_fallbacks.join(', ')} 回退 contract）` : '';
    console.log(`  ${f.path.padEnd(34)} ${f.lang.padEnd(12)} ${f.tier.padEnd(9)} ${String(f.bytes).padStart(7)} B  ← ${f.items.join(', ')}${fb}${check}`);
  }
  for (const w of report.warnings) console.log(`  ⚠ ${w}`);
  const passCount = report.files.filter((f) => f.validation.ok).length;
  console.log(`\n✓ emit 完成：${report.files.length} 个文件（${passCount} 个通过语法校验）+ manifest.json → ${args.out}（${Date.now() - t0} ms）`);
  if (report.warnings.length > 0) return 1;
  return report.files.some((f) => !f.validation.ok) ? 1 : 0;
}

async function cmdTargets(): Promise<number> {
  console.log(banner({ cmd: 'help' } as CliArgs));
  console.log(`后端语言注册表（BNF v1.4 §5.2）—— ${LANGS.length} 编程语言 + ${STATIC_LANGS.length} 静态格式\n`);
  const tiers: [string, string, typeof LANGS][] = [
    ['Tier 1', 'Harness 核心（活体/语句子集翻译优先）', LANGS.filter((l) => l.tier === 1)],
    ['Tier 2', '脚本与动态', LANGS.filter((l) => l.tier === 2)],
    ['Tier 3', '函数式', LANGS.filter((l) => l.tier === 3)],
    ['Tier 4', '系统与现代', LANGS.filter((l) => l.tier === 4)],
    ['Static', '静态资源格式', STATIC_LANGS as unknown as typeof LANGS],
  ];
  for (const [label, desc, langs] of tiers) {
    console.log(`  ${label} · ${desc}`);
    for (const l of langs) {
      const cap = l.body === 'full' ? 'full 活体翻译' : l.body === 'logic' ? 'logic 语句子集' : l.body === 'raw' ? '原文+插值' : 'contract 类型契约';
      const extra = [l.nativeRuntime ? 'native 可执行' : '', l.validateWith ? `语法校验:${l.validateWith}` : ''].filter(Boolean).join(' · ');
      console.log(`    ${l.id.padEnd(13)} ${l.name.padEnd(13)} ${l.ext.padEnd(5)} ${cap.padEnd(14)}${extra ? '  ' + extra : ''}${l.note ? '  — ' + l.note : ''}`);
    }
    console.log('');
  }
  console.log(`  project { Item -> "path${path.sep}file.ext" : <lang-id> } 中的 <lang-id> 取 id 列。`);
  console.log(`  别名：ts→typescript · js→javascript · py→python · md→markdown · yml→yaml · c++→cpp · sh/bash→bash`);
  return 0;
}

async function cmdSync(args: CliArgs): Promise<number> {
  if (!args.entry) { usage(); return 2; }
  const root = args.root ?? path.dirname(path.resolve(args.entry));
  const t0 = Date.now();
  const result = syncFile(args.entry, root);
  console.log(`\n${result.file}`);
  console.log(`  围栏：${result.blocks.length} 个 · 回写：${result.written.length} 处 · 错误：${result.errors.length} 个（${Date.now() - t0} ms）`);
  for (const b of result.blocks) {
    const changed = result.written.some((w) => w.module === b.module && w.block === b.block);
    console.log(`    ${changed ? '↩ 回写' : '  无变化'} ${b.module}:${b.line} block:${b.block}（镜像 ${b.mirror.length} 行）`);
  }
  for (const e of result.errors) console.log(`    ✗ ${e}`);
  if (result.written.length > 0) {
    console.log(`\n✓ 已回写 ${result.written.length} 处 HSL 源码 —— 运行 emit 重新生成活体翻译区`);
  }
  return result.errors.length > 0 ? 1 : 0;
}

async function cmdWatch(args: CliArgs): Promise<number> {
  if (!args.entry || !args.out) { usage(); return 2; }
  console.log(banner(args));
  console.log(`  watch 模式（总纲 §6 File Watcher）`);
  console.log(`  入口    ${args.entry}`);
  console.log(`  产物    ${args.out}`);
  console.log(`  监听 .hsl 变化 → check + emit；Ctrl-C 退出\n`);

  const runOnce = async (reason: string): Promise<void> => {
    const t0 = Date.now();
    try {
      const program = loadProgram(args.entry!);
      const diags = checkProgram(program);
      const errs = diags.filter((d) => d.severity === 'error');
      if (errs.length > 0) {
        console.log(`\n[${new Date().toLocaleTimeString()}] ${reason} → check 失败（${errs.length} errors，不 emit）`);
        console.log(formatDiags(errs).split('\n').slice(0, 12).join('\n'));
        return;
      }
      const report = await emitProgram(program, args.out!, { validate: false });
      console.log(`\n[${new Date().toLocaleTimeString()}] ${reason} → check ✓ · emit ${report.files.length} 文件（${Date.now() - t0} ms）`);
    } catch (err) {
      console.log(`\n[${new Date().toLocaleTimeString()}] ${reason} → 失败：${(err as Error).message}`);
    }
  };

  await runOnce('初始');
  // 收集所有依赖的 .hsl 文件
  const watchFiles = new Set<string>();
  const collect = (entry: string): void => {
    const abs = path.resolve(entry);
    if (watchFiles.has(abs)) return;
    watchFiles.add(abs);
    try {
      const ast = parseHslFile(abs);
      for (const item of ast.items) {
        if (item.kind === 'import') collect(path.resolve(path.dirname(abs), item.path));
      }
    } catch { /* 解析失败忽略 */ }
  };
  collect(args.entry);
  let timer: ReturnType<typeof setTimeout> | null = null;
  const watchers = [...watchFiles].map((f) => fs.watch(f, () => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => { void runOnce(`变更 ${path.basename(f)}`); }, 250);
  }));
  console.log(`\n  监听 ${watchFiles.size} 个 .hsl 文件中…`);
  await new Promise(() => { /* 永续 */ });
  void watchers;
  return 0;
}

async function cmdRun(args: CliArgs): Promise<number> {
  if (!args.entry) { usage(); return 2; }
  const outdir = args.out ?? path.resolve(process.cwd(), '.hsl-runs', new Date().toISOString().replace(/[:.]/g, '-'));
  fs.mkdirSync(outdir, { recursive: true });

  const host = new Host({
    workspace: args.workspace,
    task: args.task,
    model: args.model,
    fixturePath: args.fixture,
    temperature: args.temperature,
    maxTurns: args.maxTurns,
    maxBashCalls: args.maxBashCalls,
    maxOutputChars: args.maxOutputChars,
    allow: args.allow,
    scale: args.scale,
    outdir,
    quiet: args.quiet,
  });

  const interp = new Interp({
    hostApi: host.api,
    stdout: (line) => process.stdout.write(line + '\n'),
    stderr: (line) => process.stderr.write(line + '\n'),
    scale: args.scale,
  });

  if (!args.quiet) {
    console.log(banner(args));
    console.log(`  入口      ${path.relative(process.cwd(), args.entry)}`);
    console.log(`  模型      ${args.model}${args.fixture ? '（剧本 ' + path.basename(args.fixture) + '）' : ''}`);
    console.log(`  工作区    ${args.workspace}`);
    console.log(`  任务      ${args.task || '(未指定)'}`);
    console.log(`  尺度      ${args.scale}（G6：${args.scale === 'microkernel' ? '边事件 → 事件总线' : '边事件 → 直接调用轨迹'}）`);
    console.log(`  产物目录  ${outdir}`);
    console.log('');
  }

  try {
    const program = loadProgram(args.entry);
    for (const f of program.order) interp.addModule(f, program.files.get(f)!);
    await interp.linkProgram();

    // 入口约定：入口文件中名为 main 的 fn（入口文件自身可见即可，无需 export）
    const entryAst = program.files.get(program.entry)!;
    const mainItem = entryAst.items.find((it) => it.kind === 'fn' && it.fn.name === 'main');
    if (!mainItem || mainItem.kind !== 'fn') {
      throw new Error(`入口文件没有 fn main()（运行约定：BNF v1.3 §R-1）`);
    }
    host.emit('run_start', { entry: args.entry, model: args.model, task: args.task, scale: args.scale });
    const t0 = Date.now();
    const result = await interp.callFn(mainItem.fn, [], program.entry);
    const elapsed = Date.now() - t0;
    host.emit('run_end', { elapsed_ms: elapsed, ok: !isErrResult(result) });

    if (isErrResult(result)) {
      const errPayload = ((result as { payload?: { tuple?: unknown[] } })?.payload)?.tuple?.[0];
      console.error(`\n✗ harness 返回 Err：${displayErr(errPayload)}`);
      host.flushArtifacts();
      writeRunJson(outdir, { ok: false, error: displayErr(errPayload), elapsed_ms: elapsed, model: args.model, task: args.task });
      return 1;
    }
    const okPayload = ((result as { payload?: { tuple?: unknown[] } })?.payload)?.tuple?.[0];
    console.log(`\n✓ harness 返回 Ok（${elapsed} ms）`);
    if (okPayload !== undefined) {
      console.log(debug(okPayload).length > 2000 ? debug(okPayload).slice(0, 2000) + ' …' : debug(okPayload));
    }
    host.flushArtifacts();
    writeRunJson(outdir, { ok: true, elapsed_ms: elapsed, model: args.model, task: args.task, events: host.events.length });
    console.log(`\n产物：${outdir}/report.md · transcript.jsonl · events.jsonl · run.json`);
    return 0;
  } catch (err) {
    const e = err as Error;
    console.error(`\n✗ 运行期错误：${e.message}`);
    if (process.env.HSL_DEBUG) console.error(e.stack);
    host.emit('run_panic', { message: e.message });
    host.flushArtifacts();
    writeRunJson(outdir, { ok: false, panic: e.message, model: args.model, task: args.task });
    return 1;
  }
}

function isErrResult(v: unknown): boolean {
  return isEnum(v) && v.__enum === 'Result' && v.variant === 'Err';
}

function displayErr(e: unknown): string {
  if (e === null || e === undefined) return '(unit)';
  if (typeof e === 'object' && '__struct' in (e as object)) {
    const s = e as { __struct: string; message?: unknown; kind?: unknown };
    if (s.message !== undefined) return `${s.__struct} { kind: ${display(s.kind)}, message: ${display(s.message)} }`;
  }
  return display(e);
}

function writeRunJson(outdir: string, data: Record<string, unknown>): void {
  try {
    fs.writeFileSync(path.resolve(outdir, 'run.json'), JSON.stringify({ ts: new Date().toISOString(), ...data }, null, 2), 'utf-8');
  } catch { /* 尽力而为 */ }
}

// ---- main ----
const [,, ...argv] = process.argv;
try {
  const args = parseArgs(argv);
  let code = 0;
  if (args.cmd === 'check') code = await cmdCheck(args);
  else if (args.cmd === 'run') code = await cmdRun(args);
  else if (args.cmd === 'emit') code = await cmdEmit(args);
  else if (args.cmd === 'targets') code = await cmdTargets();
  else if (args.cmd === 'sync') code = await cmdSync(args);
  else if (args.cmd === 'watch') code = await cmdWatch(args);
  else usage();
  process.exit(code);
} catch (err) {
  if (err instanceof LinkError) {
    console.error(`error[L-0]: ${err.message}`);
  } else {
    console.error(`error[E-0]: ${(err as Error).message}`);
  }
  process.exit(1);
}
