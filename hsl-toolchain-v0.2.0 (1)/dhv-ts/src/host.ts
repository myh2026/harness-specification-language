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

export class Host {
  events: HarnessEvent[] = [];
  private seq = 0;
  private zai: unknown = null;
  private fixture: { acts: string[]; reviews: string[]; actIdx: number; reviewIdx: number } | null = null;
  public api: Record<string, unknown>;

  constructor(public opts: HostOptions) {
    if (opts.fixturePath && fs.existsSync(opts.fixturePath)) {
      const raw = JSON.parse(fs.readFileSync(opts.fixturePath, 'utf-8')) as { acts?: string[]; reviews?: string[] };
      this.fixture = { acts: raw.acts ?? [], reviews: raw.reviews ?? [], actIdx: 0, reviewIdx: 0 };
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
        }): Promise<string> => this.llmComplete(req),
      },
      fs: {
        read: (p: string): string => this.fsRead(p),
        write: (p: string, content: string): number => this.fsWrite(p, content),
        edit: (p: string, oldText: string, newText: string): { ok: boolean; error?: string } => this.fsEdit(p, oldText, newText),
        list: (dir?: string): string => this.fsList(dir ?? '.'),
      },
      shell: {
        run: async (cmd: string, o?: { cwd?: string; timeoutMs?: number }) => this.shellRun(cmd, o),
      },
      json: {
        parse: (s: string): unknown => JSON.parse(s),
        stringify: (v: unknown): string => JSON.stringify(v, null, 2),
        fields: (s: string): Map<string, string> => this.jsonFields(s),
      },
      artifacts: {
        write: (name: string, content: string): string => this.artifactWrite(name, content),
      },
      events: {
        emit: (name: string, data: unknown): void => this.emit(name, data),
      },
      fixture: {
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
