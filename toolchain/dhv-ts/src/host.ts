// ============================================================================
// dhv-ts/src/host.ts — 宿主 API（native 块的 $host 注入面）
// ----------------------------------------------------------------------------
// 运行时 ABI（BNF v1.3 附录 B）：
//   $host.config    运行配置（CLI 参数 + 默认值）
//   $host.llm       大模型网关（z-ai-web-dev-sdk；仅在解释器后端进程内使用）
//   $host.fs        工作区文件系统（路径监狱：所有操作限制在 workspace 内）
//   $host.shell     命令执行（首词白名单 + 超时 + 输出上限）
//   $host.json      JSON 桥（parse/stringify/fields —— fields 把顶层字段字符串化，
//                   以 HashMap<String,String> 形态进入 HSL，保持 BNF 类型纪律）
//   $host.artifacts 运行产物写出（不受工作区监狱限制，写入 --out 目录）
//   $host.events    事件总线（G6：microkernel 观测等价物）
//   $host.fixture   确定性模型剧本（ScriptedModel 的驱动装置）
//   $host.make      结构体/枚举变体构造通道（#L-22 修复 v0.2.56：
//                   native 返回的 plain object 是 foreign 值 —— 字段直通
//                   可用，clone/方法/模式派发全失效；$host.make("Entity",
//                   {...}) 产出带 __struct/__enum 运行时标记的合法值。
//                   实现位于 interp.linkProgram（需结构体注册表），此处
//                   仅登记 ABI 面）
//   $host.log       轨迹日志（stderr）
// ============================================================================

import * as fs from 'node:fs';
import * as path from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const execFileP = promisify(execFile);

export interface HostOptions {
  workspace: string;
  task: string;
  model: string; // deepseek | scripted
  fixturePath?: string;
  temperature: number;
  maxTurns: number;
  maxBashCalls: number;
  maxOutputChars: number;
  allow: string[];
  scale: string;
  outdir: string;
  quiet: boolean;
}

export interface HarnessEvent {
  seq: number;
  ts: string;
  name: string;
  data: unknown;
}

// ----------------------------------------------------------------------------
// Fixture v2（Gauntlet 扩展）：多轨道剧本 + 故障注入计划
// ----------------------------------------------------------------------------
// 兼容 dsh 时代的 { acts, reviews }；新增：
//   tracks: { <name>: string[] }   —— 任意命名轨道（$host.fixture.next("<name>")）
//   faults: FaultSpec[]            —— 在宿主 API 边界注入的确定性故障
// ----------------------------------------------------------------------------
export interface FaultSpec {
  /** 宿主 API 目标：fs.read / fs.write / fs.edit / fs.list / shell.run /
   *  json.parse / json.fields / llm.complete / fixture.next:<track> */
  target: string;
  /** 1 起算的第 N 次调用（默认 1） */
  nth?: number;
  /** error（抛错）/ deny（权限拒绝+事件）/ empty（空返回）/ corrupt（返回值截断）/ slow（延迟） */
  kind: 'error' | 'deny' | 'empty' | 'corrupt' | 'slow';
  message?: string;
  /** slow 延迟毫秒（仅异步目标可用） */
  delayMs?: number;
}

interface FixtureState {
  acts: string[];
  reviews: string[];
  tracks: Map<string, string[]>;
  faults: FaultSpec[];
  actIdx: number;
  reviewIdx: number;
  trackIdx: Map<string, number>;
}

export class Host {
  events: HarnessEvent[] = [];
  private seq = 0;
  private zai: unknown = null;
  private fixture: FixtureState | null = null;
  private faultCounts = new Map<string, number>();
  public api: Record<string, unknown>;

  constructor(public opts: HostOptions) {
    if (opts.fixturePath && fs.existsSync(opts.fixturePath)) {
      const raw = JSON.parse(fs.readFileSync(opts.fixturePath, 'utf-8')) as {
        acts?: string[];
        reviews?: string[];
        tracks?: Record<string, string[]>;
        faults?: FaultSpec[];
      };
      this.fixture = {
        acts: raw.acts ?? [],
        reviews: raw.reviews ?? [],
        tracks: new Map(Object.entries(raw.tracks ?? {})),
        faults: raw.faults ?? [],
        actIdx: 0,
        reviewIdx: 0,
        trackIdx: new Map(),
      };
      this.validateFaults();
    }
    this.api = {
      config: {
        model: opts.model,
        workspace: opts.workspace,
        task: opts.task,
        temperature: opts.temperature,
        maxTurns: opts.maxTurns,
        maxBashCalls: opts.maxBashCalls,
        maxOutputChars: opts.maxOutputChars,
        allow: opts.allow,
        scale: opts.scale,
        outdir: opts.outdir,
      },
      llm: {
        complete: async (req: {
          messages: { role: string; content: string }[];
          temperature?: number;
          maxTokens?: number;
        }): Promise<string> => this.withFaultsAsync('llm.complete', () => this.llmComplete(req)),
      },
      fs: {
        read: (p: string): string => this.withFaultsSync('fs.read', () => this.fsRead(p)),
        write: (p: string, content: string): number => this.withFaultsSync('fs.write', () => this.fsWrite(p, content)),
        edit: (p: string, oldText: string, newText: string): { ok: boolean; error?: string } =>
          this.withFaultsSync('fs.edit', () => this.fsEdit(p, oldText, newText)),
        list: (dir?: string): string => this.withFaultsSync('fs.list', () => this.fsList(dir ?? '.')),
      },
      shell: {
        run: async (cmd: string, o?: { cwd?: string; timeoutMs?: number }) =>
          this.withFaultsAsync('shell.run', () => this.shellRun(cmd, o)),
      },
      dhv: {
        // 嵌入执行面：无 bun 环境（如 ORG 单二进制）下嵌套 check / run 的进程内兜底。
        // 语义与子进程版一致（ fresh loadProgram → 状态隔离；输出合并为 ShellFlat 形态），
        // 差别仅在于不经过 bash —— 供 HSL 侧 shell_exec 的等价替代。
        check: async (file: string) => this.withFaultsAsync('dhv.check', () => this.dhvCheck(file)),
        run: async (args: string[]) => this.withFaultsAsync('dhv.run', () => this.dhvRunInproc(args)),
      },
      json: {
        parse: (s: string): unknown => this.withFaultsSync('json.parse', () => JSON.parse(s)),
        stringify: (v: unknown): string => JSON.stringify(v, null, 2),
        fields: (s: string): Map<string, string> => this.withFaultsSync('json.fields', () => this.jsonFields(s)),
      },
      artifacts: {
        write: (name: string, content: string): string => this.artifactWrite(name, content),
      },
      events: {
        emit: (name: string, data: unknown): void => this.emit(name, data),
      },
      fixture: {
        next: async (track: string): Promise<string> => this.fixtureNext(track),
        left: (track: string): number => this.fixtureLeft(track),
        nextAct: async (): Promise<string> => {
          if (!this.fixture) throw new Error('fixture 未配置（--fixture）');
          const i = this.fixture.actIdx++;
          if (i >= this.fixture.acts.length) throw new Error(`fixture acts 已耗尽（${this.fixture.acts.length} 条）`);
          return this.fixture.acts[i]!;
        },
        nextReview: async (): Promise<string> => {
          if (!this.fixture) throw new Error('fixture 未配置（--fixture）');
          const i = this.fixture.reviewIdx++;
          if (i >= this.fixture.reviews.length) return JSON.stringify({ verdict: 'accept' });
          return this.fixture.reviews[i]!;
        },
        actsLeft: (): number => (this.fixture ? this.fixture.acts.length - this.fixture.actIdx : 0),
      },
      log: (...args: unknown[]): void => {
        if (!this.opts.quiet) {
          process.stderr.write(args.map((a) => (typeof a === 'string' ? a : JSON.stringify(a))).join(' ') + '\n');
        }
      },
      env: {
        get: (name: string): string | undefined => process.env[name],
      },
    };
  }

  emit(name: string, data: unknown): void {
    this.events.push({ seq: this.seq++, ts: new Date().toISOString(), name, data });
  }

  // --------------------------------------------------------------------------
  // 故障注入（Gauntlet v2）：宿主 API 边界的确定性故障闸门。
  // 每个 (target, nth) 只触发一次；触发即发 fault_injected 事件（可观测）。
  // 同步 API 走 withFaultsSync（slow 不可用于同步目标——构造期校验剔除）。
  // --------------------------------------------------------------------------
  private faultPlan(): FaultSpec[] {
    return this.fixture?.faults ?? [];
  }

  /** 构造期校验：剔除不合法的故障规格（同步目标上的 slow / 未知 kind）。 */
  private validateFaults(): void {
    const SYNC_TARGETS = new Set(['fs.read', 'fs.write', 'fs.edit', 'fs.list', 'json.parse', 'json.fields']);
    this.fixture!.faults = this.fixture!.faults.filter((f) => {
      if (f.kind === 'slow' && SYNC_TARGETS.has(f.target)) {
        this.emit('fault_rejected', { target: f.target, reason: 'slow 只适用于异步目标（shell.run / llm.complete / fixture.next）' });
        return false;
      }
      return true;
    });
  }

  private matchFault(target: string): FaultSpec | null {
    const n = (this.faultCounts.get(target) ?? 0) + 1;
    this.faultCounts.set(target, n);
    const spec = this.faultPlan().find((f) => f.target === target && (f.nth ?? 1) === n);
    return spec ?? null;
  }

  private withFaultsSync<T>(target: string, real: () => T): T {
    const spec = this.matchFault(target);
    if (!spec) return real();
    this.emit('fault_injected', { target, nth: spec.nth ?? 1, kind: spec.kind, message: spec.message ?? '' });
    if (spec.kind === 'error' || spec.kind === 'deny') {
      if (spec.kind === 'deny') {
        this.emit('capability_denied', { target, reason: spec.message ?? `fault(deny): ${target}` });
      }
      throw new Error(spec.message ?? `fault(${spec.kind}): ${target}`);
    }
    const v = real();
    if (spec.kind === 'empty') {
      if (v instanceof Map) return new Map() as unknown as T;
      if (typeof v === 'string') return '' as unknown as T;
      if (typeof v === 'number') return 0 as unknown as T;
      return undefined as unknown as T;
    }
    if (spec.kind === 'corrupt') {
      if (typeof v === 'string') {
        const keep = Math.max(0, Math.floor(v.length / 2));
        return (v.slice(0, keep) + '\n[fault:corrupt]') as unknown as T;
      }
      if (v instanceof Map) {
        v.clear();
        return v;
      }
    }
    return v;
  }

  private async withFaultsAsync<T>(target: string, real: () => T | Promise<T>): Promise<T> {
    const spec = this.matchFault(target);
    if (!spec) return await real();
    this.emit('fault_injected', { target, nth: spec.nth ?? 1, kind: spec.kind, message: spec.message ?? '' });
    if (spec.kind === 'error' || spec.kind === 'deny') {
      if (spec.kind === 'deny') {
        this.emit('capability_denied', { target, reason: spec.message ?? `fault(deny): ${target}` });
      }
      throw new Error(spec.message ?? `fault(${spec.kind}): ${target}`);
    }
    if (spec.kind === 'slow') {
      await new Promise((r) => setTimeout(r, spec.delayMs ?? 100));
      return await real();
    }
    const v = await real();
    if (spec.kind === 'empty') {
      if (typeof v === 'string') return '' as unknown as T;
      if (typeof v === 'object' && v !== null && 'ok' in (v as object)) {
        return { ok: false, code: 0, stdout: '', stderr: spec.message ?? '[fault:empty]' } as unknown as T;
      }
      return undefined as unknown as T;
    }
    if (spec.kind === 'corrupt') {
      if (typeof v === 'string') {
        const keep = Math.max(0, Math.floor(v.length / 2));
        return (v.slice(0, keep) + '\n[fault:corrupt]') as unknown as T;
      }
      if (typeof v === 'object' && v !== null && 'stdout' in (v as object)) {
        const r = v as { ok: boolean; code: number; stdout: string; stderr: string };
        return { ok: r.ok, code: r.code, stdout: r.stdout.slice(0, Math.floor(r.stdout.length / 2)) + '\n[fault:corrupt]', stderr: r.stderr } as unknown as T;
      }
    }
    return v;
  }

  // ---- 多轨道剧本（Fixture v2） ----
  private async fixtureNext(track: string): Promise<string> {
    const target = `fixture.next:${track}`;
    const spec = this.matchFault(target);
    const consume = (): string => {
      if (!this.fixture) throw new Error('fixture 未配置（--fixture）');
      const list = this.fixture.tracks.get(track);
      if (!list) throw new Error(`fixture 轨道 "${track}" 不存在（可用：${[...this.fixture.tracks.keys()].join(', ') || '无'}）`);
      const i = this.fixture.trackIdx.get(track) ?? 0;
      this.fixture.trackIdx.set(track, i + 1);
      if (i >= list.length) throw new Error(`fixture 轨道 "${track}" 已耗尽（${list.length} 条）`);
      return list[i]!;
    };
    if (!spec) return consume();
    this.emit('fault_injected', { target, nth: spec.nth ?? 1, kind: spec.kind, message: spec.message ?? '' });
    if (spec.kind === 'error' || spec.kind === 'deny') {
      if (spec.kind === 'deny') {
        this.emit('capability_denied', { target, reason: spec.message ?? `fault(deny): ${target}` });
      }
      throw new Error(spec.message ?? `fixture 轨道 ${track} 故障：${spec.kind}`);
    }
    if (spec.kind === 'slow') {
      await new Promise((r) => setTimeout(r, spec.delayMs ?? 100));
      return consume();
    }
    const v = consume();
    if (spec.kind === 'empty') return '';
    if (spec.kind === 'corrupt') {
      const keep = Math.max(0, Math.floor(v.length / 2));
      return v.slice(0, keep) + '\n[fault:corrupt]';
    }
    return v;
  }

  private fixtureLeft(track: string): number {
    if (!this.fixture) return 0;
    const list = this.fixture.tracks.get(track);
    if (!list) return 0;
    return list.length - (this.fixture.trackIdx.get(track) ?? 0);
  }

  // ---- 路径监狱 ----
  private jail(p: string): string {
    const resolved = path.resolve(this.opts.workspace, p);
    const ws = path.resolve(this.opts.workspace);
    if (resolved !== ws && !resolved.startsWith(ws + path.sep)) {
      throw new Error(`路径越界（capability 违规）：${p} 逃出工作区 ${ws}`);
    }
    return resolved;
  }

  private fsRead(p: string): string {
    const abs = this.jail(p);
    if (!fs.existsSync(abs)) throw new Error(`文件不存在：${p}`);
    const stat = fs.statSync(abs);
    if (stat.size > 2 * 1024 * 1024) throw new Error(`文件过大（${stat.size} 字节）：${p}`);
    return fs.readFileSync(abs, 'utf-8');
  }

  private fsWrite(p: string, content: string): number {
    const abs = this.jail(p);
    fs.mkdirSync(path.dirname(abs), { recursive: true });
    fs.writeFileSync(abs, content, 'utf-8');
    return content.length;
  }

  private fsEdit(p: string, oldText: string, newText: string): { ok: boolean; error?: string } {
    const abs = this.jail(p);
    if (!fs.existsSync(abs)) return { ok: false, error: `文件不存在：${p}` };
    const src = fs.readFileSync(abs, 'utf-8');
    const count = src.split(oldText).length - 1;
    if (count === 0) return { ok: false, error: `old_text 未找到（0 处）` };
    if (count > 1) return { ok: false, error: `old_text 非唯一（${count} 处）` };
    fs.writeFileSync(abs, src.replace(oldText, newText), 'utf-8');
    return { ok: true };
  }

  private fsList(dir: string): string {
    const abs = this.jail(dir);
    const walk = (d: string, prefix: string): string[] => {
      const out: string[] = [];
      for (const e of fs.readdirSync(d, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
        const rel = prefix ? `${prefix}/${e.name}` : e.name;
        if (e.isDirectory()) {
          out.push(`${rel}/`);
          if (prefix.split('/').length < 2) out.push(...walk(path.join(d, e.name), rel));
        } else {
          out.push(rel);
        }
      }
      return out;
    };
    if (!fs.existsSync(abs)) throw new Error(`目录不存在：${dir}`);
    return walk(abs, '').join('\n');
  }

  private async shellRun(cmd: string, o?: { cwd?: string; timeoutMs?: number }): Promise<{ ok: boolean; code: number; stdout: string; stderr: string }> {
    const first = cmd.trim().split(/\s+/)[0] ?? '';
    if (!this.opts.allow.includes(first)) {
      this.emit('capability_denied', { command: cmd, reason: `首词 "${first}" 不在白名单 [${this.opts.allow.join(', ')}]` });
      return { ok: false, code: 126, stdout: '', stderr: `命令被安全策略拒绝："${first}" 不在白名单` };
    }
    const cwd = o?.cwd ? path.resolve(this.opts.workspace, o.cwd) : path.resolve(this.opts.workspace);
    try {
      const { stdout, stderr } = await execFileP('bash', ['-c', cmd], {
        cwd,
        timeout: o?.timeoutMs ?? 60_000,
        maxBuffer: 8 * 1024 * 1024,
      });
      return { ok: true, code: 0, stdout, stderr };
    } catch (err) {
      const e = err as { code?: number; stdout?: string; stderr?: string; killed?: boolean };
      return {
        ok: false,
        code: e.code ?? 1,
        stdout: e.stdout ?? '',
        stderr: (e.killed ? '进程超时被终止\n' : '') + (e.stderr ?? ''),
      };
    }
  }

  // ---- 嵌入执行面（$host.dhv.*）----
  // 无 bun 环境下嵌套 check / run 的进程内兜底：懒加载 main.ts 的 cliMain，
  // 临时接管 stdout/stderr 捕获输出，恢复后返回。fresh loadProgram 保证状态隔离。

  private async captureCli(args: string[]): Promise<{ code: number; stdout: string; stderr: string }> {
    // 懒加载（函数体内 import，规避 main.ts ↔ host.ts 的静态循环）
    const mod = (await import('./main.ts')) as { cliMain: (argv: string[]) => Promise<number> };
    const savedOut = process.stdout.write.bind(process.stdout);
    const savedErr = process.stderr.write.bind(process.stderr);
    const savedLog = { log: console.log, error: console.error, warn: console.warn, info: console.info };
    let stdout = '';
    let stderr = '';
    (process.stdout as unknown as { write: (c: unknown) => boolean }).write = (c: unknown) => {
      stdout += typeof c === 'string' ? c : String(c);
      return true;
    };
    (process.stderr as unknown as { write: (c: unknown) => boolean }).write = (c: unknown) => {
      stderr += typeof c === 'string' ? c : String(c);
      return true;
    };
    console.log = (...a: unknown[]) => { stdout += a.map(String).join(' ') + '\n'; };
    console.error = (...a: unknown[]) => { stderr += a.map(String).join(' ') + '\n'; };
    console.warn = console.error;
    console.info = console.log;
    try {
      const code = await mod.cliMain(args);
      return { code, stdout, stderr };
    } finally {
      (process.stdout as unknown as { write: (c: unknown) => boolean }).write = savedOut;
      (process.stderr as unknown as { write: (c: unknown) => boolean }).write = savedErr;
      console.log = savedLog.log;
      console.error = savedLog.error;
      console.warn = savedLog.warn;
      console.info = savedLog.info;
    }
  }

  private async dhvCheck(file: string): Promise<{ ok: boolean; output: string }> {
    const abs = path.isAbsolute(file) ? file : path.resolve(this.opts.workspace, file);
    const r = await this.captureCli(['check', abs]);
    return { ok: r.code === 0, output: (r.stdout + r.stderr).trim() };
  }

  private async dhvRunInproc(args: string[]): Promise<{ ok: boolean; code: number; stdout: string; stderr: string }> {
    const r = await this.captureCli(args);
    return { ok: r.code === 0, code: r.code, stdout: r.stdout, stderr: r.stderr };
  }

  private jsonFields(s: string): Map<string, string> {
    // IO 卫生：剥离模型常见的 markdown 围栏（```json ... ```）
    let t = s.trim();
    if (t.startsWith('```')) {
      t = t.replace(/^```[a-zA-Z]*\s*/, '').replace(/```\s*$/, '').trim();
    }
    const m = new Map<string, string>();
    const parsed = JSON.parse(t) as Record<string, unknown>;
    for (const [k, v] of Object.entries(parsed)) {
      if (v === null || v === undefined) m.set(k, '');
      else if (typeof v === 'object') m.set(k, JSON.stringify(v));
      else m.set(k, String(v));
    }
    return m;
  }

  private artifactWrite(name: string, content: string): string {
    const abs = path.resolve(this.opts.outdir, name);
    fs.mkdirSync(path.dirname(abs), { recursive: true });
    fs.writeFileSync(abs, content, 'utf-8');
    return abs;
  }

  private async llmComplete(req: { messages: { role: string; content: string }[]; temperature?: number; maxTokens?: number }): Promise<string> {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const mod: any = await import('z-ai-web-dev-sdk');
    const ZAICtor: { create: () => Promise<any> } = mod.default ?? mod;
    this.zai ??= await ZAICtor.create();
    const zai = this.zai as {
      chat: { completions: { create: (r: unknown) => Promise<{ choices?: { message?: { content?: string } }[] }> } };
    };
    const completion = await zai.chat.completions.create({
      messages: req.messages,
      temperature: req.temperature ?? 0.2,
      max_tokens: req.maxTokens ?? 1024,
      thinking: { type: 'disabled' },
    });
    const content = completion.choices?.[0]?.message?.content ?? '';
    return content;
  }

  // ---- 运行收尾：写事件流 ----
  flushArtifacts(): void {
    const abs = path.resolve(this.opts.outdir, 'events.jsonl');
    fs.mkdirSync(path.dirname(abs), { recursive: true });
    fs.writeFileSync(abs, this.events.map((e) => JSON.stringify(e)).join('\n') + '\n', 'utf-8');
  }
}
