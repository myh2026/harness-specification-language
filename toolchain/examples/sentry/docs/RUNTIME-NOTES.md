# RUNTIME-NOTES — HSL harness runtime, host API & toolchain on Windows

Repo: `D:\下载\hsl` · HSL **v0.2.72**, BNF **v1.5**, interpreter **dhv-ts** (Bun), Rust compiler **dhv 0.2.61**.
All facts below were produced by actually running the commands shown on this Windows box (PowerShell 5.1,
bun 1.4.3). Probe sources live in `toolchain/examples/sentry/docs/probe/`; captured outputs are quoted verbatim.
Unverified items are marked **[UNTESTED]** — do not cite them as fact.

---

## 1. Canonical commands (works on Windows)

From the repo root `D:\下载\hsl` (PowerShell). `bun` = `C:\Users\86186\.bun\bin\bun.exe`.

```powershell
cd 'D:\下载\hsl'

# 1) static check
bun toolchain/dhv-ts/src/main.ts check toolchain/examples/sentry/sentry.hsl

# 2) scripted run (no network; fixture supplies model output)
bun toolchain/dhv-ts/src/main.ts run toolchain/examples/sentry/sentry.hsl `
  --workspace toolchain/examples/sentry/docs/probe/ws `
  --fixture  toolchain/examples/sentry/fixtures/triage-demo.json `
  --out      toolchain/examples/sentry/docs/probe/out-run

# 3) projection (emitted code + manifest.json)
bun toolchain/dhv-ts/src/main.ts emit toolchain/examples/sentry/sentry.hsl `
  --out toolchain/examples/sentry/docs/probe/out-emit

# 4) backend registry / capability table
bun toolchain/dhv-ts/src/main.ts targets
```

Run without `--quiet` prints a banner; `--quiet` suppresses banner + `$host.log`.
Every path flag is `path.resolve`'d, so relative paths are relative to the **shell cwd** (not `--workspace`).
Chinese characters in paths (`D:\下载\...`) work; UTF-8 probe outputs round-trip correctly.

`run` last lines (verbatim, note the misleading first line):

```
✓ harness 返回 Ok（1026 ms）

产物：D:\...\out-run/report.md · transcript.jsonl · events.jsonl · run.json
```

Only `events.jsonl` + `run.json` are written by the CLI; see §6.

---

## 2. CLI surface, options, exit codes

Commands: `check <entry.hsl>` · `run <entry.hsl> [opts]` · `emit <entry.hsl> --out DIR [--scale MODE] [--no-validate]`
· `targets` · `sync <generated-file> [--root DIR]` · `watch <entry.hsl> --out DIR` · `help | -h`.

`run` defaults: `--workspace .` (cwd) · `--task ""` · `--model scripted` · `--temperature 0.2` · `--fixture` none
· `--max-turns 24` · `--max-bash 12` · `--max-output 4000` · `--allow bun,node,ls,cat,grep,diff`
· `--scale microkernel` · `--out .hsl-runs/<ISO timestamp>` · `--quiet`.

Exit codes / diagnostics actually observed:

| situation | output | exit |
|---|---|---|
| `check` clean | `✓ N 个模块全部通过检查` (plus `dhv-ts check: 0 error(s), 0 warning(s)`) | 0 |
| `check` has errors | `error[<CODE>]: <msg>` + `  --> <abs path>:L:C` + `dhv-ts check: N error(s), M warning(s)` | 1 |
| unknown flag | `error[E-0]: 未知参数 --xxx` | 1 |
| missing entry | usage text | 2 |
| `run` Ok | `✓ harness 返回 Ok（N ms）` + debug payload (truncated 2000 chars) | 0 |
| `run` Err result | `✗ harness 返回 Err：<...>` | 1 |
| `run` runtime panic | `✗ 运行期错误：<msg>` (single line; stack only with `HSL_DEBUG=1`) | 1 |
| `emit` warnings>0 **or** any validation failure | prints `⚠ ...`, still writes files | 1 |
| `emit` clean | `✓ emit 完成：N 个文件（M 个通过语法校验）+ manifest.json → DIR（ms）` | 0 |
| `sync` wrote something | `✓ 已回写 N 处 HSL 源码 —— 运行 emit 重新生成活体翻译区` | 0 |
| `sync` error (source module missing) | `✗ 模块文件不存在：<file>（root=<dir>）` | 1 |

Check diagnostic examples (verbatim, from probes):

```
error[G-1]: graph Flow 缺少 AgentLoop（graph body 必须恰含至少一个 loop）
  --> D:\下载\hsl\toolchain\examples\sentry\docs\probe\p5-emit.hsl:22:13
```

`check` also lints unused bindings: `error[S-7]: 绑定 "cfg" 声明后未使用（_ 前缀可豁免）`. This lint can fire with
a **misleading** location/message when the real failure is a lexer keyword used as a field name (see §9, `scale`).

`emit` **does not run `check`** — a program with G-1 (or other checker errors) can still emit files successfully.
Run `check` in CI before `emit` if you care.

`targets` prints 32 languages + 6 static formats (38 backends) in tiers (`full` live translation / `logic` statement
subset / `contract` type contract / static `原文+插值`). Note the Windows footer renders the path example with a
backslash: `project { Item -> "path\file.ext" : <lang-id> }` (it uses `path.sep`).

---

## 3. Fixture v2 — exact schema (acts / reviews / tracks / faults)

`--fixture FILE` JSON file. All four keys are **optional** (defaults `[]` / `{}`). `tracks` is `Record<string, string[]>`.

```json
{
  "acts":    ["act-1", "act-2"],
  "reviews": ["{\"verdict\":\"accept\"}"],
  "tracks":  { "plan": ["plan-a", "plan-b"], "audit": ["{\"verdict\":\"accept\"}"] },
  "faults":  [
    { "target": "shell.run", "nth": 1, "kind": "error", "message": "transient exec failure" },
    { "target": "fs.read",   "nth": 2, "kind": "corrupt" },
    { "target": "fixture.next:plan", "nth": 1, "kind": "empty" },
    { "target": "llm.complete", "nth": 1, "kind": "slow", "delayMs": 200 }
  ]
}
```

Semantics (all verified):

* `$host.fixture.nextAct()` consumes `acts` in order; `nextReview()` consumes `reviews`; `next(track)` consumes
  `tracks[track]`. `actsLeft()` / `left(track)` return remaining counts (0 for unknown/missing track, no throw).
* `tracks` values are opaque strings — typically a JSON **string** that the harness parses with
  `$host.json.parse`/`fields`. No implicit parsing is done for you.
* Real-world example (sentry, scripted run): `toolchain/examples/sentry/fixtures/triage-demo.json` uses
  `tracks.triage` (9 model turns) + `tracks.audit` (reviewer verdicts) + one `shell.run` error fault on the first call.
* `faults[].nth` is **1-based, per-target call counter** (default 1). Each `(target, nth)` fires **once**; later calls
  hit the real API. Counters are per `Host` instance (one per `run`).
* `faults[].kind` ∈ `error | deny | empty | corrupt | slow`. `delayMs` used by `slow` (default 100).

Fault targets: `fs.read`, `fs.write`, `fs.edit`, `fs.list`, `shell.run`, `json.parse`, `json.fields`,
`llm.complete`, `fixture.next:<track>` (also implemented but undocumented: `dhv.check`, `dhv.run`).

Exhaustion / config errors — verbatim (all exit 1, wrapped by the native-block prefix when thrown from `native typescript`):

```
fixture 未配置（--fixture）
fixture acts 已耗尽（1 条）
fixture reviews 已耗尽（0 条）—— 审查判定不可静默伪造（accept 不是安全缺省；需要恒 accept 请显式写足剧本条目）
fixture 轨道 "nope" 不存在（可用：plan, obs）
fixture 轨道 "plan" 已耗尽（1 条）
```

Fault behaviour matrix (verified against `docs/probe/faults/f1.json`, `f2.json`, `f3.json`):

| kind | sync API (`fs.*`, `json.*`) | async API (`shell.run`, `llm.complete`, `fixture.next:*`) |
|---|---|---|
| `error` | throws `Error(message ?? "fault(error): <target>")` | same (async reject) |
| `deny` | throws + emits `capability_denied {target, reason}` | same |
| `empty` | Map→empty Map, string→`""`, number→`0`, other→`undefined` (unit) | string→`""`; `{ok,...}`→`{ok:false,code:0,stdout:'',stderr:message??'[fault:empty]'}`; else `undefined` |
| `corrupt` | string→first half + `"\n[fault:corrupt]"`; Map→cleared; other→unchanged | string→first half + marker; shell result→`stdout` half + marker; fixture track→first half + marker |
| `slow` | **rejected at Host construction** (`fault_rejected` event); call runs normally | waits `delayMs ?? 100` then runs the real call (fixture.next: delay then consume) |

Every trigger emits an event first: `fault_injected {target, nth, kind, message}`. `slow` on a synchronous target emits
`fault_rejected {target, reason:"slow 只适用于异步目标（shell.run / llm.complete / fixture.next）"}` **before
`run_start`** (Host is constructed first ⇒ `events.jsonl` seq 0 is that rejection).

Observed outputs (fixture `f1.json`, verbatim):

```
R1_EMPTY len=0 val=""
R2_CORRUPT val="alpha content h\n[fault:corrupt]"
R3_NORMAL val="alpha content here\nsecond line\n"
W_EMPTY ret=0
E_EMPTY ret=()
L_CORRUPT val="a.txt\nbig.txt\nf1-out.txt\n[fault:corrupt]"
J_CORRUPT kind_obj field=1
F_CORRUPT len=0 has_verdict=None
```

(`json.parse` corrupt on an **object** result is a no-op — only strings/Maps are handled. `json.fields` corrupt clears
the Map. `fs.write` empty returns `0`. `fs.edit` empty returns unit, so reading `.error` afterwards panics.)

Async fault outputs (`f2.json`):

```
S1_EMPTY ok=false code=0 out="" err="[fault:empty]"
S2_CORRUPT ok=true code=0 out="payload\n[fault:corrupt]" err=""
S3_SLOW ok=true out="payload-12345\n" elapsed_ms=448
P1_EMPTY left=2 val=""
P2_CORRUPT val="pla\n[fault:corrupt]"
P3_SLOW val="plan-c" elapsed_ms=159
LLM_ERR val="CAUGHT:llm boom"
```

---

## 4. `$host` API surface (injected only inside `native blocks`)

**Critical:** `$host` is not HSL syntax. `let c = $host.config;` fails `check` with
`error[E-0]: 无法解析的表达式起点 "$" (punct)`. Every host call must be inside a `native typescript { ... }` block
(that is the only injection point). Example: `let n: i64 = native typescript { return $host.fs.write("a.txt", "hi"); };`

`$host.config` object (no `fixturePath`):
`{ model, workspace, task, temperature, maxTurns, maxBashCalls, maxOutputChars, allow, scale, outdir }`.
Read it **inside** native code — dot-accessing `.scale` from HSL is a parse error because `scale` is an HSL keyword
(§9). Observed: `CONFIG model=scripted scale=microkernel task="probe task 中文" temperature=0.2 maxTurns=24 maxBashCalls=12 maxOutputChars=4000 allow=bun,node,ls,cat,grep,diff`.

| API | signature | return shape / behaviour |
|---|---|---|
| `$host.fs.read(path)` | `(string) => string` | UTF-8 text. Jail: only inside `--workspace`. Errors: `文件不存在：<p>` / `文件过大（N 字节）：<p>` (limit 2 MiB) / `路径越界（capability 违规）：<p> 逃出工作区 <ws>` |
| `$host.fs.write(path, content)` | `(string, string) => number` | UTF-8 write, mkdir -p; returns **`content.length` = UTF-16 code units, not bytes**. `"hello 世界\nsecond line\n"` → `21` (UTF-8 bytes = 25) |
| `$host.fs.edit(path, oldText, newText)` | `(string,string,string) => {ok:boolean, error?:string}` | success = `{"ok":true}` with **no `error` key**. `{ok:false,error:"old_text 未找到（0 处）"}` / `"old_text 非唯一（N 处）"`. CRLF-tolerant retry (normalizes to LF, writes CRLF back if file was CRLF) → `old_text 非唯一（CRLF 归一化后 N 处）` |
| `$host.fs.list(dir?, depth?)` | `(string?, number?) => string` | newline-joined, dirs end with `/`, sorted by name (`localeCompare`), depth default 8 max 32 (clamped + `fs_list_depth_clamped` event), 20 000-entry cap + `... [fs.list 截断：超过 20000 条]`. Missing dir → `目录不存在：<dir>` |
| `$host.shell.run(cmd, opts?)` | `(string, {cwd?:string, timeoutMs?:number}?) => Promise<{ok,code,stdout,stderr}>` | see §4.1 |
| `$host.json.parse(text)` | `(string) => unknown` | raw `JSON.parse`; nested plain objects stay foreign (field access works), numbers/bools cross as JS values; invalid → `JSON Parse error: Unexpected identifier "not"` |
| `$host.json.stringify(v)` | `(unknown) => string` | `JSON.stringify(v, null, 2)` (2-space pretty) |
| `$host.json.fields(text)` | `(string) => Map<string,string>` | trims, strips ```` ```json ```` fences; values: null→`""`, object→`JSON.stringify`, else `String(v)`; map access returns `Option`: `verdict=Some("accept")`, numeric `score=Some("3")` |
| `$host.events.emit(name, data)` | `(string, unknown) => void` | pushes `{seq, ts, name, data}`; seq starts at 0 per run |
| `$host.artifacts.write(name, content)` | `(string, string) => string` | writes under **`--out` dir** (not workspace-jailed), mkdir -p, UTF-8; returns absolute path e.g. `D:\...\out-run\probe\art.md` |
| `$host.fixture.next(track)` | `(string) => Promise<string>` | consumes track list; exhaustion errors in §3 |
| `$host.fixture.nextAct()` / `nextReview()` | `() => Promise<string>` | consume `acts` / `reviews` |
| `$host.fixture.left(track)` / `actsLeft()` | `() => number` | remaining count; 0 for unknown track (no throw) |
| `$host.env.get(name)` | `(string) => string \| undefined` | process env; missing → `undefined` (prints `()`) |
| `$host.log(...args)` | `(...unknown[]) => void` | space-joined to **stderr**, suppressed by `--quiet`; non-strings `JSON.stringify` |
| `$host.make(name, payload)` | `(string, unknown) => HSL value` | struct (`"Session"`) or enum variant (`"Option::Some"`, `"Result::Ok"`, `"Status::Busy"`); validates exact fields; produces real HSL struct/enum values with `__struct`/`__enum` markers. Needs the type declared in a linked module, else `$host.make：未知结构体 "Point"（可用 this.structs 中注册的名字；枚举变体请用 "Family::Variant"）` |
| `$host.dhv.check(file)` | `(string) => Promise<{ok,output}>` | in-process nested `check`; `file` resolved against workspace; `output` = trimmed stdout+stderr |
| `$host.dhv.run(args)` | `(string[]) => Promise<{ok,code,stdout,stderr}>` | in-process nested `cliMain`; **hangs in our probe** (see §9) — treat as unreliable on Windows/bun 1.4.3 |
| `$host.llm.complete(req)` | `({messages, temperature?, maxTokens?, thinking?, stream?, track?}) => Promise<string>` | defaults temp 0.2 / maxTokens 1024. Needs `DHV_LLM_GATEWAY` (OpenAI-compatible `/chat/completions`) or the bundled SDK; `DHV_LLM_DISABLE_SDK=1` throws `llm: SDK 车道已禁用（DHV_LLM_DISABLE_SDK=1，零外联模式）—— scripted 剧本车道或显式网关（DHV_LLM_GATEWAY）二选一`. With gateway: `DHV_LLM_API_KEY`, `DHV_LLM_MODEL`, `DHV_LLM_TIMEOUT_MS` (default 180000). Errors: `LLM gateway <status>: <200 chars>`, `empty completion (finish_reason=<f>...)` |

`$host.make` needs a plain JS object, so build the payload in native code:

```hsl
// works: struct must be declared in a module of the same program
let payload = native typescript { return { x: 3, y: 4 }; };
let p = native typescript { return $host.make("Point", payload); };   // p debug => Point { x: 3, y: 4 }
```

### 4.1 `$host.shell.run` — whitelist + real `bash` (the Windows trap)

* **Whitelist matches the first whitespace token EXACTLY**: `cmd.trim().split(/\s+/)[0]` must be in `--allow`
  (default `bun,node,ls,cat,grep,diff`). `"/usr/bin/ls"` → denied; anything with a path prefix never matches.
  Denial returns `{ok:false, code:126, stdout:"", stderr:"命令被安全策略拒绝：\"<first>\" 不在白名单"}` and emits
  `capability_denied {command, reason:"首词 \"<first>\" 不在白名单 [<allow>]";}`.
  Important: the emitted `capability_denied` reason contains `[bun, node, ls, cat, grep, diff]` (comma+space),
  while the returned `stderr` reads `不在白名单` with no list.
* **It spawns `execFile('bash', ['-c', cmd])`** — on this Windows box exactly one thing resolves:
  `C:\Windows\System32\bash.exe` is the **WSL launcher stub**, and WSL has no installed distro. Result: `ok:false`,
  `code:1`, **stdout is UTF-16LE WSL noise**, stderr empty — looks like a broken command.
  Real POSIX bash (Git for Windows: `D:\学习\py学习\Git\bin\bash.exe`) works if **prepended to PATH** before bun starts:

  ```powershell
  $env:PATH = 'D:\学习\py学习\Git\bin;' + $env:PATH
  ```

  Verified: `echo hello-from-shell` → `{ok:true,code:0,stdout:"hello-from-shell\n",stderr:""}`; `cat`, `ls`, `true` all work.
* `cwd` is resolved against `--workspace` **but is NOT jailed** — `{cwd:"../../.."}` escapes to the repo/toolchain dir
  and runs there (verified listing `backends-demo dsh nova sentry`). Do not treat cwd as a capability boundary.
* Nonzero exit → `{ok:false, code:<real exit code>, stdout, stderr}` (`ls /no/such/path` → `code:2`,
  `stderr:"ls: cannot access ..."`). `timeoutMs` (default 60000) → `{ok:false, code:1, stderr:"进程超时被终止\n"}`.
* >8 MiB stdout → `{ok:false, code:"ERR_CHILD_PROCESS_STDIO_MAXBUFFER", stdout:<first 8 MiB>, stderr:""}` —
  `code` is a **string here**, not a number.
* Verified `--allow 'ls,echo,cat,sleep,true'` probe output:

```
A_OK ok=true code=0 out="hello-from-shell\n" err=""
B_LS ok=true code=0 out="a.txt\n" err=""
C_CAT ok=true code=0 out="HELLO 世界\nsecond LINE\n"
D_TRUE ok=true code=0 out=""
E_NONZERO ok=false code=2 err="ls: cannot access '/no/such/path': No such file or directory\n"
F_CWD ok=true code=0 out="HELLO 世界\nsecond LINE\n"
G_CWD_ESCAPE ok=true code=0 out_head="backends-demo\ndsh\nnova\nsentry\n"
H_TIMEOUT ok=false code=1 err="进程超时被终止\n"
I_MAXBUF ok=false code="ERR_CHILD_PROCESS_STDIO_MAXBUFFER" out_len=8388608 err=""
J_ABS_TOKEN ok=false code=126 err="命令被安全策略拒绝：\"/usr/bin/ls\" 不在白名单"
```

Path jail internals worth knowing: lexical `path.resolve` containment + symlink realpath walk-up.
`fs.read("../escape.txt")` → `路径越界（capability 违规）：../escape.txt 逃出工作区 <ws>` (121 chars incl. full ws path).

---

## 5. Native blocks — value bridge

* Supported at **runtime** by dhv-ts: `native typescript`, `native javascript`, `native python`.
  Other languages → `native <lang> 后端未接入解释器（dhv-ts 运行期支持 typescript / python；...）`.
* The block is compiled with `new Function('$host', '$ctx', <body>)`, async; capture set = identifiers found in the
  HSL scope, injected as `const {...} = ($ctx ?? {})`. `Ok/Err/Some/None` shims are injected (can be shadowed).
* Body wrapping rule: if the body contains no `return` keyword, the whole body is wrapped as a single return
  expression. Consequence: statements-only bodies (e.g. `$host.events.emit(...);`) need `return null;` or they fail
  at construction with `native typescript 块语法非法：...`. `throw` at top level is therefore also rejected:
  `throw`-only body → `native typescript 块语法非法：Unexpected keyword 'throw'`. Use `try/catch` with `return`.
* Unit literal trap: `Ok(())` inside native → `native typescript 块语法非法：Unexpected token ')'` (also caught at check
  time by N-6). Use `null` / `$host.make("Result::Ok", [null])`.
* Value bridge (verified): plain JS objects crossing into HSL are **foreign** — `.field` access works recursively
  (`r.nested.deep.v === 99`), arrays behave as HSL `Vec` (`tags.len()`, `tags[2]`), numbers/bools pass through,
  function-valued props are callable as methods (`r.show("z")` → `show:z`), `null` → unit `()`. But clone / pattern
  matching / struct patterns fail on foreign values — use `$host.make` when you need real structs/enums.
* Errors from native execution are re-wrapped by the interpreter:
  `✗ 运行期错误：native typescript 块执行失败：<original message>` (exec) or
  `✗ 运行期错误：native typescript 块语法非法：<msg>\n  源码摘录：...\n  高频成因：...` (construction).
* `native python` **[UNTESTED]** in this session: implementation is `python3` then `python` fallback,
  `PYTHONUTF8=1`, 30 s timeout, JSON via env ctx / `__HSL_OUT__` marker. Expect the marker-based protocol.

---

## 6. Events & artifacts — what `run` actually writes

`flushArtifacts` writes **exactly two files** into `--out`:
`events.jsonl` (one compact JSON per line, trailing newline) and `run.json` (pretty, 2-space).
`report.md` / `transcript.jsonl` appear **only if the harness itself calls `$host.artifacts.write(...)`**.
The CLI's `产物：` line always lists all four names — it is aspirational, not a directory listing.

`run.json` (Ok path): `{ "ts": UTC ISO, "ok": true, "elapsed_ms": N, "model": "scripted", "task": "...", "events": N }`.
Err path: `{ ts, ok:false, error, elapsed_ms, model, task }`; panic path: `{ ts, ok:false, panic, model, task }`.
`task` preserves UTF-8 (e.g. `"probe task 中文"`); timestamps are UTC with `Z`.

Built-in events:

| event | data | when |
|---|---|---|
| `run_start` | `{entry (abs path), model, task, scale}` | always, seq 0 (except `fault_rejected` from Host ctor can precede it) |
| `node` | `{graph, node, initialized}` | each `node` declaration executes inside a `graph` |
| `edge` | `{graph, from, to, on, scale}` | every `match` arm whose pattern variant matches an `edge ... on <Variant>` guard; one event per consecutive endpoint pair |
| `capability_denied` | `{command, reason}` (shell whitelist) or `{target, reason}` (fault deny) | denial |
| `fault_injected` / `fault_rejected` | see §3 | fault trigger / rejected spec |
| `run_end` | `{elapsed_ms, ok}` | always (ok = harness returned `Ok`) |
| `run_panic` | `{message}` | uncaught runtime error |

Verified `edge`/`node` sample (probe `p4-graph.hsl`, graph `Loop` with 3 nodes):

```json
{"seq":1,"ts":"...","name":"node","data":{"graph":"Loop","node":"model","initialized":true}}
{"seq":2,"ts":"...","name":"node","data":{"graph":"Loop","node":"executor","initialized":true}}
{"seq":4,"ts":"...","name":"edge","data":{"graph":"Loop","from":"executor","to":"model","on":"Observed","scale":"microkernel"}}
{"seq":5,"ts":"...","name":"edge","data":{"graph":"Loop","from":"model","to":"executor","on":"Tool","scale":"microkernel"}}
```

Event order is the actual execution order (calls interleave with `fault_injected`/`capability_denied`);
`seq` is contiguous 0-based for the whole run.

---

## 7. Projection — `emit`, `project{}`, v1.5 `rules{}`, `manifest.json`

`project { Item -> "path/file.ext" : <lang-id>, rules { <kind> -> "path/{name}.ext" : <lang-id>, } }`.
`<lang-id>` from `targets` (aliases: `ts→typescript`, `py→python`, `md→markdown`, `yml→yaml`, `sh→bash`, `c++→cpp`).

Rules semantics (BNF v1.5 §3.4), as implemented:

* **R1 shadowing** — explicit item mapping wins over rules expansion (verified: `handle -> src/main.rs` and
  `rules { fn -> ... }` coexisted; `handle` landed in `src/main.rs`).
* **R2 placeholder** — only `{name}`; anything else → `error[P-5]: 投射规则路径占位符 {bad} 未注册（v1 仅支持 {name}）`.
* **R3 uniqueness** — one rule per kind → `error[P-5]: 投射规则类型 "fn" 重复声明（R3：同一类型只允许一条规则）`.
* **R4 kind registry** — `graph/fn/struct/enum/trait/const/type/block/static`; unknown →
  `error[P-5]: 投射规则类型 "bogus" 未注册（支持 graph/fn/struct/enum/trait/const/type/block/static）`. All are **check errors** (exit 1).
* **R5 expansion pool** — implemented broader than spec: **exported *and* non-exported** named items of imported/linked
  modules are expanded (verified: `hidden` — not imported, only defined in the imported module — was emitted to
  `gen/fn/hidden.rs`). Plan file names accordingly.
* **R6 consistency** — expanded items participate in the same path-uniqueness / backend-tier checks.
* `main` is special: when a rule maps `fn`, `main` still lands as `main（main 回退 contract）` and its generated file
  lists `"contract_fallbacks": ["main"]` in the manifest.
* `emit` prints warnings like `⚠ X-2：gen/main.py 引用函数/常量/变体 helper（位于 gen/fn/helper.py，跨目录 python 导入需手动接线）`
  and **exits 1** when warnings > 0.
* `--no-validate` removes the `· 语法✓ <tool>` suffix but the summary still says `N 个通过语法校验` (files counted
  regardless) — don't trust that phrase when validating was skipped.
* Validation tools: python → `python3 -m py_compile` (falls back to `python`; `PYTHONUTF8=1`, 15 s default,
  override `DHV_VALIDATE_TIMEOUT_MS`); typescript/javascript → in-process `Bun.Transpiler`; bash → `bash -n`;
  json static → real `JSON.parse`; yaml/toml/ini/xml/markdown → `embedded` (unchecked); everything else →
  `heuristic:balanced` bracket/quote check.

`manifest.json` shape (verified, 0.2.72):

```json
{
  "dhv": "dhv-ts 0.2.72",
  "entry": "p5-emit.hsl",
  "scale": "microkernel",
  "backends": 38,
  "generated_at": "2026-11-14T01:18:08.523Z",
  "files": [
    { "path": "src/graphs/Flow.rs", "lang": "rust", "tier": "logic", "bytes": 1530,
      "items": ["Flow"], "syntax_check": "pass", "syntax_tool": "heuristic:balanced" }
  ],
  "warnings": [],
  "protocol": {
    "fence": "@dhv:source-map / @dhv:hsl-mirror / @dhv:end-source-map",
    "sync": "编辑围栏内 HSL 镜像 → dhv sync <file> 回写 .hsl → dhv emit 重新生成",
    "honesty": { "full": "...", "logic": "...", "contract": "...", "contract_fallbacks": "..." }
  }
}
```

`tier` = `full` (live statement translation: python/typescript/javascript) · `logic` (statement subset: rust/go/cpp)
· `contract` · `static` (block/static resources). `bytes` = UTF-8 byte size. `items` = source item names.

Generated files carry a header (`本文件由 DHV v0.2.72 ...`), `@dhv:generated` markers (do not hand-edit) and editable
`@dhv:source-map` / `@dhv:hsl-mirror` / `@dhv:end-source-map` fenced mirror blocks.

### `sync` — write edits back into HSL

```powershell
bun toolchain/dhv-ts/src/main.ts sync <generated-file> --root <dir-with-hsl-source>
```

* Default `--root` = **dirname of the generated file** — almost always wrong for nested `--out` trees. The source-map
  line (e.g. `@dhv:source-map: entry.hsl:3, block: helper (live)`) is resolved against `--root`.
* Verified: editing the `#     x + 1` mirror line to `x + 5` and running sync with `--root` gave
  `围栏：1 个 · 回写：1 处 · 错误：0 个（55 ms）`, `↩ 回写 entry.hsl:3 block:helper（镜像 5 行）`,
  `✓ 已回写 1 处 HSL 源码 —— 运行 emit 重新生成活体翻译区`, exit 0, and the `.hsl` source line became `x + 5`.
  Re-running sync → `  无变化 entry.hsl:3 block:helper（镜像 5 行）`, exit 0.
* Wrong root → `✗ 模块文件不存在：entry.hsl（root=<dir>）`, exit 1 (`围栏：1 个 · 回写：0 处 · 错误：1 个`).

---

## 8. Scale — `microkernel` vs `monolith`

`--scale` (and `scale = ...;` in the projection file) is **semantic in the event tag only** for dhv-ts runs:
verified by running the same graph twice — the `events.jsonl` streams are byte-identical except the `scale` field in
`run_start` and in every `edge` event; the emitted Rust scaffolding does differ (Plugin/event-bus topology with
`microkernel`, direct-call topology with `monolith`). Do not claim behavioural differences at runtime:
`--scale monolith` still emits `edge` events, one per fired guarded pattern.

Emit header reflects the mode: `... 从 HSL 源码投射生成（Rust 后端 · microkernel 尺度）`, and `manifest.scale`.

---

## 9. Windows-specific quirks (all hit in practice)

1. **UTF-8 BOM breaks `.hsl` parsing.** PS 5.1 `Set-Content -Encoding UTF8` / `Out-File` write a BOM; lexer sees
   `U+FEFF` as an ident → `✗ 运行期错误：期望项定义，得到 "﻿" (ident) (file:1:2)`. Write with
   `[System.IO.File]::WriteAllText($p, $text, (New-Object System.Text.UTF8Encoding($false)))`; read with
   `[System.IO.File]::ReadAllText($p)` then strip a leading `U+FEFF` if present.
2. **`bash` is the WSL stub, not POSIX bash.** See §4.1. Prepend Git for Windows' `bin` to `$env:PATH` before starting
   bun. `where.exe bash` order matters: `D:\学习\py学习\Git\bin\bash.exe` must be first.
3. **`$host.dhv.check` / `$host.dhv.run` hung** (>60 s, killed) in probe `p6a-dhv-check.hsl` on this box
   (bun 1.4.3, in-process nested CLI capture). Treat nested-CLI injection as unreliable here; shell out to the CLI
   from the host side instead. **[bug/pitfall — needs a proper reproducer]**
4. **Capturing native stderr in PS 5.1**: `& bun ... 2> file` writes the file as **UTF-16LE** and dropped the last
   2 chars of a line in our test (119 vs 121 chars). Prefer `& bun ... 2>&1 | Select-String -Pattern '...'` /
   `Out-String` and read `$_.Line`. PS wraps native stderr in `bun.exe :` + `所在位置`/`CategoryInfo` frames; the real
   message is the last `✗ ...` / `error[...]` line.
5. **`[Environment]::CurrentDirectory` is not changed by `cd`.** .NET static `File`/`Path` calls resolve relative paths
   against the stale process CWD (`D:\下载`), while child processes such as bun get the new `cd` directory. Use
   absolute paths for all .NET APIs.
6. **HSL keywords are reserved**: `scale`, `graph`, `node`?, `edge`, `project`, `native`, `match`, `loop`, `async`,
   `await`, `move`, etc. Dot-accessing a field named `scale` (e.g. from `$host.config`) is unparseable and `check`
   misreports it as `error[S-7]: 绑定 "cfg" 声明后未使用`; `run` fails
   `✗ 运行期错误：期望标识符，得到 "scale" (kw) (<macro>:4:35)`. Read/flatten config inside native code.
7. **Anonymous object literals are not HSL expressions** (`{ x: 3 }` is not a value). Options objects for host calls
   must be built inside `native typescript`, or passed as a declared struct instance.
8. **`targets` footer prints a Windows path example with `\`** (`"path\file.ext"`); don't copy it into `.hsl` — use
   forward slashes in `project{}` paths (as all examples do).
9. Chinese paths/contents work end-to-end: `D:\下载\hsl` as cwd and workspace, `--task "probe task 中文"`, UTF-8
   read/write/edit round-trips, Chinese in generated reports.

---

## 10. Top runtime pitfalls (ranked by debugging pain)

1. `$host` outside a `native typescript` block — parse error, not a runtime "undefined" error.
2. `bash` resolves to the WSL stub → `shell.run` "mysteriously" returns UTF-16 noise + `ok:false`. Prepend Git bash.
3. Whitelist matches the **first raw token**: `./script.sh`, `/usr/bin/ls`, `node -e ...` style commands are denied
   unless the first token is literally listed in `--allow` (`node` is allowed by default, `node -e` also works).
4. `run` does not create `report.md`/`transcript.jsonl`; harness must `$host.artifacts.write` them.
5. `fs.edit` success has no `error` field → reading `.error` panics `对象没有字段 "error"`; always branch on `.ok`
   or print the whole struct with `{:?}`.
6. Native body without `return` is wrapped as one expression → `emit(...);` alone is a syntax error; add `return null;`.
7. `Ok(())`/`Err(())` inside native TS is invalid JS → construction error; use `null` / `$host.make`.
8. `emit` exit code 1 on warnings (X-2 cross-file imports, etc.) — CI green requires zero warnings, not just files.
9. `sync` default root = generated file's dir → pass `--root` explicitly.
10. `faults` `slow` on sync targets is silently rejected at Host construction (event `fault_rejected`, call unaffected);
    look at `events.jsonl` seq 0 if a slow fault "doesn't work".
11. Fixture exhaustion is a **hard error**, not `""` — `nextReview` deliberately refuses to default to `accept`
    (message explains "accept 不是安全缺省").
12. `--max-bash`, `--max-turns`, `--max-output` are policy values for *your harness* to enforce (e.g. read them from
    `$host.config` / `$host.config.maxBashCalls`); the interpreter does not enforce bash call counts itself.

---

## 11. Recipes (copy-paste, Windows PowerShell)

```powershell
# --- env -------------------------------------------------------------------
cd 'D:\下载\hsl'
$env:PATH = 'D:\学习\py学习\Git\bin;' + $env:PATH      # real POSIX bash for $host.shell.run
$BUN = 'C:\Users\86186\.bun\bin\bun.exe'
$CLI = Join-Path (Get-Location) 'toolchain\dhv-ts\src\main.ts'
$SENTRY = Join-Path (Get-Location) 'toolchain\examples\sentry'

# --- 1. check --------------------------------------------------------------
& $BUN $CLI check (Join-Path $SENTRY 'sentry.hsl')

# --- 2. scripted run (copy the demo workspace first; harness mutates it) ---
$ws  = Join-Path $env:TEMP 'sentry-ws'
$out = Join-Path $env:TEMP 'sentry-out'
Remove-Item $ws,$out -Recurse -Force -ErrorAction SilentlyContinue
Copy-Item (Join-Path $SENTRY 'workspace') $ws -Recurse
& $BUN $CLI run (Join-Path $SENTRY 'sentry.hsl') `
  --workspace $ws `
  --fixture (Join-Path $SENTRY 'fixtures\triage-demo.json') `
  --out $out
"exit=$LASTEXITCODE"
Get-ChildItem $out                     # events.jsonl + run.json (+ anything the harness wrote)
Get-Content (Join-Path $out 'run.json') -Encoding UTF8

# --- 3. re-run with a per-run task, low turn budget ------------------------
& $BUN $CLI run (Join-Path $SENTRY 'sentry.hsl') --workspace $ws `
  --fixture (Join-Path $SENTRY 'fixtures\triage-demo.json') `
  --max-turns 12 --task 'payments are timing out' --out $out

# --- 4. emit ---------------------------------------------------------------
& $BUN $CLI emit (Join-Path $SENTRY 'sentry.hsl') --out (Join-Path $env:TEMP 'sentry-emit')
# inspect manifest.json: files[].{path,lang,tier,bytes,items,syntax_check}

# --- 5. loop budget fixture (must terminate by harness policy, not model) --
& $BUN $CLI run (Join-Path $SENTRY 'sentry.hsl') --workspace $ws `
  --fixture (Join-Path $SENTRY 'fixtures\loop-forever.json') --max-turns 3 --out $out

# --- 6. fault injection smoke ---------------------------------------------
# put docs\probe\faults\f2.json-style faults into a fixture and watch events.jsonl
Select-String -Path (Join-Path $out 'events.jsonl') -Pattern 'fault_injected|capability_denied'

# --- 7. write probe files without BOM -------------------------------------
$text = "fn main() -> Result<(), String> { Ok(()) }`n"
[System.IO.File]::WriteAllText('D:\tmp\x.hsl', $text, (New-Object System.Text.UTF8Encoding($false)))

# --- 8. targets table ------------------------------------------------------
& $BUN $CLI targets
```

Minimal harness skeleton that runs (verified patterns only):

```hsl
// entry.hsl — main is fn main() (no export needed), return Result
fn main() -> Result<(), String> {
    let cfg = native typescript { return $host.config; };                 // foreign object
    let out = native typescript { return $host.fs.read("report.md"); };   // jailed to --workspace
    native typescript {                                                   // side effects: return null
        $host.events.emit("probe_marker", { n: 1, s: "two", b: true });
        $host.artifacts.write("note.md", "hello 世界\n");
        return null;
    };
    println!("model={} out_len={}", native typescript { return $host.config.model; }, out.len());
    Ok(())
}
```

---

## 12. Verification ledger

Verified in this session (commands/outputs quoted above): CLI `check/run/emit/targets/sync`; exit codes;
`events.jsonl` + `run.json` shapes and event kinds; `$host.config/fs.*/shell.run/json.*/events/artifacts/fixture/env/log/make`;
fault v2 kinds/targets/exhaustion messages; graph `node`/`edge` events and `--scale` effect; `project{}` + all six
rules diagnostics + `manifest.json` + validation tools + `--no-validate`; native TS bridge and its failure modes;
PS/Windows/BOM/bash quirks.

Not verified here: **[UNTESTED]** `$host.python` native backend; `$host.llm.complete` against a real gateway/SDK
(only the fault path and the zero-outbound error were observed); `$host.dhv.*` (hung); watch mode; `dhv` (Rust
compiler v0.2.61) compiling/executing these harnesses — only `dhv.exe --version` → `dhv 0.2.61` was confirmed.
