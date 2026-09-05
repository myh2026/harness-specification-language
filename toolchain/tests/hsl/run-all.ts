#!/usr/bin/env bun
// ============================================================================
// tests/hsl/run-all.ts — HSL/DHV 工具链严格测试套件（发布级）
// ----------------------------------------------------------------------------
// 用法：bun tests/hsl/run-all.ts
// 覆盖：回归基线 / 静态检查规则（正反例）/ 38 后端 emit / 双向 sync /
//       模糊测试（随机 token 汤 / Unicode / 病态输入）/ CLI 边界 / 压力测试
// ============================================================================

import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';
import { execFileSync } from 'node:child_process';
import { parseFileSource } from '../../dhv-ts/src/parser';
import { balanceCheck } from '../../dhv-ts/src/backends/validate';

const ROOT = path.resolve(import.meta.dir, '../..');
const DHV = path.join(ROOT, 'dhv-ts/src/main.ts');
const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'hsl-test-'));

/** 内嵌进生成 Python 源码的路径统一转正斜杠（Windows 反斜杠会触发 \U 等转义错误） */
function fwd(p: string): string {
  return p.split(path.sep).join('/');
}

interface Case { name: string; group: string; fn: () => Promise<void> | void }

const cases: Case[] = [];
let passed = 0;
let failed = 0;
const failures: { group: string; name: string; err: string }[] = [];

function test(group: string, name: string, fn: () => Promise<void> | void): void {
  cases.push({ group, name, fn });
}
function assert(cond: unknown, msg: string): asserts cond {
  if (!cond) throw new Error(msg);
}
function assertEq(a: unknown, b: unknown, msg: string): void {
  if (a !== b) throw new Error(`${msg}: ${String(a)} !== ${String(b)}`);
}
function run(args: string[], opts: { cwd?: string } = {}): { code: number; stdout: string; stderr: string } {
  try {
    const stdout = execFileSync('bun', [DHV, ...args], { cwd: opts.cwd ?? ROOT, encoding: 'utf-8', timeout: 120_000 }) as string;
    return { code: 0, stdout, stderr: '' };
  } catch (e) {
    const err = e as { status?: number; stdout?: string; stderr?: string };
    return { code: err.status ?? 1, stdout: err.stdout ?? '', stderr: err.stderr ?? '' };
  }
}

// ---------------------------------------------------------------------------
// 1. 回归基线
// ---------------------------------------------------------------------------
test('回归', 'smoke run（核心构件 + native 双语言）', () => {
  const r = run(['run', 'dhv-ts/examples/smoke.hsl', '--quiet']);
  assertEq(r.code, 0, `smoke 应通过（exit=${r.code}）`);
  assert(r.stdout.includes('smoke ✓'), 'smoke 缺少完成标记');
});

test('回归', 'std-tour run（10 个标准库全巡览）', () => {
  const r = run(['run', 'dhv-ts/examples/std-tour.hsl', '--quiet']);
  assertEq(r.code, 0, `std-tour 应通过（exit=${r.code}）`);
  assert(r.stdout.includes('std-tour ✓'), 'std-tour 缺少完成标记');
  assert(r.stdout.includes('levenshtein(kitten/sitting) = 3'), 'levenshtein 结果错误');
  assert(r.stdout.includes('gcd(12,18) = 6'), 'gcd 结果错误');
});

test('回归', 'pattern-tour run（if-let/while-let 模式扩展巡览）', () => {
  const r = run(['run', 'dhv-ts/examples/pattern-tour.hsl', '--quiet']);
  assertEq(r.code, 0, `pattern-tour 应通过（exit=${r.code}）\n${r.stdout}`);
  assert(r.stdout.includes('circle r=3'), '枚举 tuple 变体 if-let 错');
  assert(r.stdout.includes('square w=4'), '多 if-let 分支错');
  assert(r.stdout.includes('rect 2x5'), '多字段 tuple 变体错');
  assert(r.stdout.includes('point (1,2)'), 'struct 变体 if-let 错');
  assert(r.stdout.includes('unit'), '无负载变体 if-let 错');
  assert(r.stdout.includes('non-point'), 'if-let fallthrough 错');
  assert(r.stdout.includes('drain=15'), 'while-let + Vec.pop 错');
  assert(r.stdout.includes('first_ok=42'), '嵌套 while-let + if-let Result::Ok 错');
});

test('回归', 'pattern-tour check + emit（32 后端文件全过语法校验）', () => {
  const cr = run(['check', 'dhv-ts/examples/pattern-tour.hsl']);
  assertEq(cr.code, 0, `check 应通过：${cr.stdout}`);
  const dir = path.join(TMP, 'pt');
  fs.mkdirSync(dir, { recursive: true });
  const er = run(['emit', 'dhv-ts/examples/pattern-tour.hsl', '--out', dir]);
  assertEq(er.code, 0, `emit 应通过：${er.stdout}`);
  assert(er.stdout.includes('32 个文件'), `应投射 32 个文件（v0.2.6 新增 drain → cpp/go）：${er.stdout}`);
  assert(er.stdout.includes('32 个通过语法校验'), `应全部通过语法校验：${er.stdout}`);
  // cpp/go if-let/while-let 活体化验证：describe/classify/count_down/drain 应为 live
  for (const f of ['describe.cpp', 'classify.cpp', 'count_down.cpp', 'drain.cpp', 'count_down.go', 'classify.go', 'describe.go', 'drain.go']) {
    const p = path.join(dir, 'gen', f.includes('.go') ? 'go' : 'cpp', f);
    const src = fs.readFileSync(p, 'utf-8');
    assert(!src.includes('未翻译'), `${f} 应为活体翻译（非 contract 回退）`);
    assert(src.includes('(live)'), `${f} 应带 (live) 标记`);
  }
  // first_ok.cpp 含 Result::Ok 模式 —— cpp 无变体通道，诚实回退 contract
  const fk = fs.readFileSync(path.join(dir, 'gen', 'cpp', 'first_ok.cpp'), 'utf-8');
  assert(fk.includes('未翻译'), 'first_ok.cpp 的 Result 模式应诚实回退 contract（宁缺毋滥）');
});

test('回归', 'nova check（2000+ 行旗舰项目 · 15 模块）', () => {
  const r = run(['check', 'examples/nova/nova.hsl']);
  assertEq(r.code, 0, `nova 应 0 error（exit=${r.code}）\n${r.stdout}`);
});

test('回归', 'dsh check（10 模块）', () => {
  const r = run(['check', 'examples/dsh/dsh.hsl']);
  assertEq(r.code, 0, `dsh check 应通过`);
});

test('回归', 'dsh scripted 端到端（剧本 Agent 真实跑通）', () => {
  const r = run([
    'run', 'examples/dsh/dsh.hsl',
    '--fixture', 'examples/dsh/fixtures/fix-variance.json',
    '--task', '修复 stats.ts 方差',
    '--workspace', 'examples/dsh/workspace',
    '--quiet',
  ]);
  assertEq(r.code, 0, `dsh scripted 应 Ok（exit=${r.code}）\n${r.stdout.slice(-500)}`);
});

test('回归', '? 的 From 显式转换通道（ProviderError → HarnessError）', () => {
  const dir = path.join(TMP, 'from');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'from.hsl'), `struct ProviderError { message: String }
struct HarnessError { message: String }
impl From<ProviderError> for HarnessError {
    fn from(e: ProviderError) -> HarnessError {
        HarnessError { message: String::from("wrapped: ") + e.message }
    }
}
fn flaky(fail: bool) -> Result<i64, ProviderError> {
    if fail { Err(ProviderError { message: String::from("boom") }) } else { Ok(42) }
}
fn run(fail: bool) -> Result<i64, HarnessError> {
    let v = flaky(fail)?;
    Ok(v + 1)
}
fn main() {
    let bad = run(true);
    match bad {
        Result::Ok(n) => println!("ok {}", n),
        Result::Err(e) => println!("from-converted: {}", e.message),
    }
}`);
  const r = run(['run', path.join(dir, 'from.hsl'), '--quiet']);
  assertEq(r.code, 0, `From 回归应通过：${r.stdout}${r.stderr}`);
  assert(r.stdout.includes('from-converted: wrapped: boom'), `? 应经 From 转换错误值：${r.stdout}`);
});

test('回归', 'backends-demo check + run', () => {
  const r1 = run(['check', 'examples/backends-demo/agent.hsl']);
  assertEq(r1.code, 0, `backends-demo check`);
  const r2 = run(['run', 'examples/backends-demo/agent.hsl', '--quiet']);
  assertEq(r2.code, 0, `backends-demo run`);
  assert(r2.stdout.includes('1 / 2 工具调用成功'), 'summarize 输出错误');
});

test('回归', 'translator-tour run（v0.2.2 扩充方法映射巡览）', () => {
  const r = run(['run', 'dhv-ts/examples/translator-tour.hsl', '--quiet']);
  assertEq(r.code, 0, `translator-tour 应通过（exit=${r.code}）\n${r.stdout.slice(-300)}`);
  assert(r.stdout.includes('sorted = [1, 2, 3] is_sorted? true'), `sort/is_sorted：${r.stdout}`);
  assert(r.stdout.includes('first negative at 2'), `position Option 语义：${r.stdout}`);
  assert(r.stdout.includes('find harness at 7'), `find Option 语义：${r.stdout}`);
  assert(r.stdout.includes('and_then = 10'), `and_then：${r.stdout}`);
  assert(r.stdout.includes('expect = 42'), `expect：${r.stdout}`);
  assert(r.stdout.includes('char_count = 21'), `char_count：${r.stdout}`);
  assert(r.stdout.includes('translator-tour ✓'), '缺完成标记');
});

test('回归', 'String::new / Vec::new / HashMap::new 构造 + mut 检查', () => {
  const r = run(['run', writeTmp(`fn main() {
    let mut s = String::new();
    s.push_str("hello");
    let mut v: Vec<i64> = Vec::new();
    v.push(1);
    let m: HashMap<String, i64> = HashMap::new();
    println!("s={}" , s);
    println!("v={:?}", v);
    println!("m={}", m.len());
}`), '--quiet']);
  assertEq(r.code, 0, `构造器应可用：${r.stdout}${r.stderr}`);
  assert(r.stdout.includes('s=hello') && r.stdout.includes('v=[1]') && r.stdout.includes('m=0'), `输出：${r.stdout}`);
});

test('回归', 'S-6 通配 _ 兜底 = 穷尽（AgentLoop 外，Rust 语义）', () => {
  const out = checkSrc(`enum Action { Go, Stop }
fn f(a: Action) -> i64 {
    match a {
        Action::Go => 1,
        _ => 0,
    }
}
fn main() { println!("{}", f(Action::Go)); }`);
  assert(out.includes('0 error'), `AgentLoop 外 _ 应视为穷尽：${out.slice(0, 200)}`);
});

test('回归', 'S-6 缺变体且无 _ → 仍报错（AgentLoop 外）', () => {
  const out = checkSrc(`enum Action { Go, Stop }
fn f(a: Action) -> i64 {
    match a {
        Action::Go => 1,
    }
}
fn main() {}`);
  assert(out.includes('S-6') && out.includes('Stop'), `缺变体应报错：${out.slice(0, 200)}`);
});

// ---------------------------------------------------------------------------
// 2. 静态检查规则（正反例）
// ---------------------------------------------------------------------------
let caseCounter = 0;
function checkSrc(src: string): string {
  const p = path.join(TMP, `case-${caseCounter++}.hsl`);
  fs.writeFileSync(p, src, 'utf-8');
  return run(['check', p]).stdout;
}

test('检查规则', 'S-6 穷尽性：AgentLoop 内 _ 通配兜底报错', () => {
  const out = checkSrc(`enum Action { Go, Stop }\ngraph G { loop { let a = Action::Go; match a { Action::Go => continue, _ => break, } } }\nfn main() {}`);
  assert(out.includes('S-6'), `AgentLoop _ 应触发 S-6：${out.slice(0, 200)}`);
});
test('检查规则', 'S-7 未使用 let 报错', () => {
  const out = checkSrc(`fn main() { let unused_var = 5; println!("x"); }`);
  assert(out.includes('S-7'), `unused_var 应触发 S-7：${out.slice(0, 200)}`);
});
test('检查规则', 'S-7 _ 前缀豁免', () => {
  const out = checkSrc(`fn main() { let _ignored = 5; println!("x"); }`);
  assert(out.includes('0 error'), `_ 前缀应豁免：${out.slice(0, 200)}`);
});
test('检查规则', 'S-8 同作用域遮蔽报错', () => {
  const out = checkSrc(`fn main() { let x = 1; let x = 2; println!("{}", x); }`);
  assert(out.includes('S-8'), `重复声明应触发 S-8：${out.slice(0, 200)}`);
});
test('检查规则', 'P-3 投射目标未定义', () => {
  const out = checkSrc(`fn main() {}\nproject { Missing -> "x.py" : python }`);
  assert(out.includes('P-3'), `未定义投射应触发 P-3：${out.slice(0, 200)}`);
});
test('检查规则', 'P-4 未注册后端语言', () => {
  const out = checkSrc(`fn main() {}\nfn helper() -> i64 { 1 }\nproject { helper -> "x.cobol" : cobol }`);
  assert(out.includes('P-4'), `cobol 应触发 P-4：${out.slice(0, 200)}`);
});
test('检查规则', 'P-4 静态资源只能投静态格式', () => {
  const out = checkSrc(`fn main() {}\nblock cfg { a: 1 }\nproject { cfg -> "x.py" : python }`);
  assert(out.includes('P-4'), `block→python 应触发 P-4：${out.slice(0, 200)}`);
});
test('检查规则', 'P-4 合法：26 种 contract 后端全接受', () => {
  const langs = ['java', 'csharp', 'kotlin', 'swift', 'ruby', 'php', 'lua', 'perl', 'bash', 'powershell', 'r', 'julia', 'scala', 'elixir', 'erlang', 'haskell', 'ocaml', 'fsharp', 'zig', 'nim', 'crystal', 'dart', 'groovy', 'objectivec', 'd', 'vb'];
  let src = 'fn main() {}\n';
  for (let i = 0; i < langs.length; i++) {
    src += `fn f${i}() -> i64 { ${i} }\nproject { f${i} -> "f${i}.${langs[i]}" : ${langs[i]} }\n`;
  }
  const out = checkSrc(src);
  assert(out.includes('0 error'), `26 语言应全部合法：${out.slice(0, 300)}`);
});
test('检查规则', 'N-1 native 未注册语言', () => {
  const out = checkSrc(`fn main() { let x: i64 = native cobol { 1 }; }`);
  assert(out.includes('N-1'), `native cobol 应触发 N-1：${out.slice(0, 200)}`);
});
test('检查规则', 'G-1 graph 缺 AgentLoop', () => {
  const out = checkSrc(`graph G { let x: i64 = 1; }\nfn main() {}`);
  assert(out.includes('G-1'), `无 loop graph 应触发 G-1：${out.slice(0, 200)}`);
});
// ---- v0.2.53 S-13 / G-8（hsl-fuzz 第二轮锁定） ----
test('检查规则', 'S-13 整型域：i8=300 越界报错（跨后端漂移）', () => {
  const out = checkSrc(`fn main() { let x: i8 = 300; println!("{}", x); }`);
  assert(out.includes('S-13'), `i8=300 应触发 S-13：${out.slice(0, 200)}`);
});
test('检查规则', 'S-13 整型域：u8=-1 负值报错', () => {
  const out = checkSrc(`fn main() { let x: u8 = -1; println!("{}", x); }`);
  assert(out.includes('S-13'), `u8=-1 应触发 S-13：${out.slice(0, 200)}`);
});
test('检查规则', 'S-13 整型域：const 形态同判', () => {
  const out = checkSrc(`const BAD: i16 = 65535;\nfn main() { println!("{}", BAD); }`);
  assert(out.includes('S-13'), `const i16=65535 应触发 S-13：${out.slice(0, 200)}`);
});
test('检查规则', 'S-13 整型域：边界值合法（-128/127/i64MAX/0xFF@u8）', () => {
  const out = checkSrc(`fn main() { let _a: i8 = -128; let _b: i8 = 127; let _c: i64 = 9223372036854775807; let _d: u8 = 255; let _e: u8 = 0xFF; let _f: u64 = 18446744073709551615; println!("ok"); }`);
  assert(out.includes('0 error'), `边界值不应误报：${out.slice(0, 200)}`);
});
test('检查规则', 'S-13 整型域：无注解/非字面量不判（BigInt 任意精度既定设计）', () => {
  // v0.2.54：原样本 `big + 1`（i64::MAX+1）正是 S-15 的拦截目标（L-9 跨后端
  // 溢出漂移），已升级为专项用例；本用例保留原意图 —— 域内算术不触发 S-13。
  const out = checkSrc(`fn main() { let big: i64 = 9223372036854775807; let inner = big - 1; println!("{}", inner); }`);
  assert(out.includes('0 error'), `非字面量运算不应触发 S-13：${out.slice(0, 200)}`);
});
test('检查规则', 'G-8 重复边：同 (from,to,guard) 二次声明报错', () => {
  const out = checkSrc(`enum Ev { Tick }\ngraph G { node a: i64 = 1; node b: i64 = 2; edge a -> b on Ev::Tick; edge a -> b on Ev::Tick; loop { break; } }`);
  assert(out.includes('G-8'), `重复边应触发 G-8：${out.slice(0, 250)}`);
});
test('检查规则', 'G-8 合法：同向不同守卫（Vigil 惯用法）不误报', () => {
  const out = checkSrc(`enum Ev { Tick, Tock }\ngraph G { node a: i64 = 1; node b: i64 = 2; edge a -> b on Ev::Tick; edge a -> b on Ev::Tock; loop { break; } }`);
  assert(out.includes('0 error'), `同向多守卫不应误报：${out.slice(0, 250)}`);
});
// ---- v0.2.53 S-14（hsl-fuzz 第三轮锁定：L-8 三后端真机对拍） ----
test('检查规则', 'S-14 二元类型：纯字面量 str*int 报错', () => {
  const out = checkSrc(`fn main() { let x = "abc" * 3; println!("{}", x); }`);
  assert(out.includes('S-14'), `str*int 应触发 S-14：${out.slice(0, 200)}`);
});
test('检查规则', 'S-14 二元类型：变量中转 str*int 报错（v2 作用域追踪）', () => {
  const out = checkSrc(`fn main() { let s = "abc"; let x = s * 3; println!("{}", x); }`);
  assert(out.includes('S-14'), `变量中转应触发 S-14：${out.slice(0, 200)}`);
});
test('检查规则', 'S-14 二元类型：bool+int 报错', () => {
  const out = checkSrc(`fn main() { let x = true + 1; println!("{}", x); }`);
  assert(out.includes('S-14'), `bool+int 应触发 S-14：${out.slice(0, 200)}`);
});
test('检查规则', 'S-14 二元类型：跨类比较 str>int 报错', () => {
  const out = checkSrc(`fn main() { if "abc" > 1 { println!("gt"); } }`);
  assert(out.includes('S-14'), `跨类比较应触发 S-14：${out.slice(0, 200)}`);
});
test('检查规则', 'S-14 二元类型：合法运算族零误报（数值/str+str/同型比较/bool 逻辑）', () => {
  const out = checkSrc(`fn main() { let _a = 1 + 2; let _b = 1.5 * 2.5; let _c = "ab" + "cd"; let _d = 3 > 2; let _e = "x" == "y"; let _f = true && _d; let h = 10 / 2; println!("{}", h); }`);
  assert(out.includes('0 error'), `合法运算不应误报：${out.slice(0, 200)}`);
});
test('检查规则', 'S-14 二元类型：动态值（调用/方法链/重赋值变量）保守放行', () => {
  const out = checkSrc(`fn main() { let n = 3; let s = "abc"; let x = n + 1; let y = s.len(); println!("{} {}", x, y); }`);
  assert(out.includes('0 error'), `动态值不应误报：${out.slice(0, 200)}`);
});
test('检查规则', 'S-14 二元类型：int+float 混算报错（S1 零隐式转换下沉）', () => {
  const out = checkSrc(`fn main() { let x = 1 + 0.5; println!("{}", x); }`);
  assert(out.includes('S-14'), `int+float 混算应触发 S-14：${out.slice(0, 200)}`);
});

// ---- v0.2.54 S-15 / S-16 / S-14 v3 / L-10 / L-11（hsl-fuzz 第四轮锁定） ----
test('检查规则', 'S-15 注解域算术溢出：i64::MAX + 1 报错（四运行时漂移实录）', () => {
  const out = checkSrc(`fn main() { let a: i64 = 9223372036854775807; let b = a + 1; println!("{}", b); }`);
  assert(out.includes('S-15'), `i64::MAX+1 应触发 S-15：${out.slice(0, 200)}`);
});
test('检查规则', 'S-15 注解域算术溢出：u8 250 + 250 报错', () => {
  const out = checkSrc(`fn main() { let a: u8 = 250; let b = a + a; println!("{}", b); }`);
  assert(out.includes('S-15'), `u8 250+250 应触发 S-15：${out.slice(0, 200)}`);
});
test('检查规则', 'S-15 let 注解折叠：250 + 250 对 u8 报错', () => {
  const out = checkSrc(`fn main() { let b: u8 = 250 + 250; println!("{}", b); }`);
  assert(out.includes('S-15'), `注解折叠应触发 S-15：${out.slice(0, 200)}`);
});
test('检查规则', 'S-15 静态可证除零：5 / 0 报错', () => {
  const out = checkSrc(`fn main() { let y = 5 / 0; println!("{}", y); }`);
  assert(out.includes('S-15'), `静态除零应触发 S-15：${out.slice(0, 200)}`);
});
test('检查规则', 'S-15 赋值域越界：let mut u8 = 0; x = 300 报错', () => {
  const out = checkSrc(`fn main() { let mut x: u8 = 0; x = 300; println!("{}", x); }`);
  assert(out.includes('S-15'), `赋值域越界应触发 S-15：${out.slice(0, 200)}`);
});
test('检查规则', 'S-15 复合赋值域溢出：a: u8 = 250; a += 10 报错', () => {
  const out = checkSrc(`fn main() { let mut a: u8 = 250; a += 10; println!("{}", a); }`);
  assert(out.includes('S-15'), `复合赋值溢出应触发 S-15：${out.slice(0, 200)}`);
});
test('检查规则', 'S-14 v3 重赋值中转：let mut x = 3; x = "abc"; x * 2 报错', () => {
  const out = checkSrc(`fn main() { let mut x = 3; x = "abc"; let y = x * 2; println!("{}", y); }`);
  assert(out.includes('S-14'), `重赋值中转应触发 S-14：${out.slice(0, 200)}`);
});
test('检查规则', 'S-13 后缀字面量域：300u8 报错', () => {
  const out = checkSrc(`fn main() { let y = 300u8; println!("{}", y); }`);
  assert(out.includes('S-13'), `后缀域越界应触发 S-13：${out.slice(0, 200)}`);
});
test('检查规则', 'S-16 i128 静态容量：超容量字面量报错（L-10 归零实录）', () => {
  const out = checkSrc(`fn main() { let y = 170141183460469231731687303715884105728; println!("{}", y); }`);
  assert(out.includes('S-16'), `超容量字面量应触发 S-16：${out.slice(0, 200)}`);
});
test('检查规则', 'S-15 合法族零误报：域内算术/合法后缀/域内赋值', () => {
  const out = checkSrc(`fn main() { let a: i64 = 9223372036854775807; let b = a - 1; let c: u8 = 250; let d = c + 5; let e = 10 / 2; let mut f: u8 = 200; f += 50; let g = 250u8; println!("{} {} {} {} {} {} {}", b, d, e, f, g, a, c); }`);
  assert(out.includes('0 error'), `合法族不应误报：${out.slice(0, 250)}`);
});

// ---------------------------------------------------------------------------
// 3. 38 后端 emit + sync 闭环
// ---------------------------------------------------------------------------
test('emit', 'backends-demo 全 38 后端 emit（语法全过）', () => {
  const out = path.join(TMP, 'backends');
  const r = run(['emit', 'examples/backends-demo/agent.hsl', '--out', out]);
  assertEq(r.code, 0, `emit 应全部通过：\n${r.stdout.slice(-1500)}`);
  const manifest = JSON.parse(fs.readFileSync(path.join(out, 'manifest.json'), 'utf-8')) as { files: { path: string; lang: string }[] };
  const langs = new Set(manifest.files.map((f) => f.lang));
  assert(langs.size >= 32, `应覆盖 ≥32 编程语言，实际 ${langs.size}`);
  const staticLangs = new Set(manifest.files.filter((f) => ['yaml', 'markdown', 'json', 'toml', 'ini', 'xml'].includes(f.lang)).map((f) => f.lang));
  assertEq(staticLangs.size, 6, '应覆盖 6 静态格式');
  assert(!r.stdout.includes('语法✗'), `存在语法失败：\n${r.stdout.split('\n').filter((l) => l.includes('语法✗')).join('\n')}`);
  for (const f of ['gen/python/describe.py', 'gen/rust/describe.rs', 'gen/haskell/Agent.hs', 'gen/erlang/agent.erl', 'config/agent.yml', 'docs/AGENTS.md', 'config/agent.xml']) {
    assert(fs.existsSync(path.join(out, f)), `缺文件 ${f}`);
  }
});

test('emit', '静态 json 真校验（非法红灯 / 合法绿灯）· v0.2.52', () => {
  const dir = path.join(TMP, 'staticjson');
  fs.mkdirSync(dir, { recursive: true });
  // 负例：YAML 风格内容投 .json —— v0.2.51 前无条件标 pass（tool=embedded）的盲区
  fs.writeFileSync(path.join(dir, 'bad.hsl'), `block cfg {
    this is: not json
}
project {
    cfg -> "config/bad.json" : json,
}
fn main() -> i64 { 0 }
`, 'utf-8');
  const bad = run(['emit', path.join(dir, 'bad.hsl'), '--out', path.join(dir, 'bad-out')]);
  assertEq(bad.code, 1, '非法 JSON 投射 emit 应 exit 1');
  assert(bad.stdout.includes('语法✗') && bad.stdout.includes('json.parse'), `应标注 json.parse 失败：${bad.stdout.slice(0, 300)}`);
  const badManifest = JSON.parse(fs.readFileSync(path.join(dir, 'bad-out', 'manifest.json'), 'utf-8')) as { files: { lang: string; syntax_check: string; syntax_tool: string }[] };
  const bf = badManifest.files.find((f) => f.lang === 'json')!;
  assertEq(bf.syntax_check, 'fail', 'manifest 应记 fail');
  assertEq(bf.syntax_tool, 'json.parse', 'manifest 应记 tool=json.parse');
  // 正例：合法 JSON block（含 {{}} 插值）应绿灯且产物可真解析
  fs.writeFileSync(path.join(dir, 'good.hsl'), `const MAX_TURNS: i64 = 24;
block cfg {
    { "agent": { "max_turns": {{MAX_TURNS}} } }
}
project {
    cfg -> "config/agent.json" : json,
}
fn main() -> i64 { 0 }
`, 'utf-8');
  const good = run(['emit', path.join(dir, 'good.hsl'), '--out', path.join(dir, 'good-out')]);
  assertEq(good.code, 0, `合法 JSON emit 应 exit 0：${good.stdout.slice(-400)}`);
  const parsed = JSON.parse(fs.readFileSync(path.join(dir, 'good-out', 'config/agent.json'), 'utf-8')) as { agent: { max_turns: number } };
  assertEq(parsed.agent.max_turns, 24, '合法 JSON 应可解析且插值渲染正确');
});

test('emit', 'dsh emit（真实 harness 项目投射）', () => {
  const out = path.join(TMP, 'dsh');
  const r = run(['emit', 'examples/dsh/dsh.hsl', '--out', out]);
  assertEq(r.code, 0, `dsh emit 应通过：\n${r.stdout.slice(-500)}`);
  const mainPy = fs.readFileSync(path.join(out, 'src/dsh/main.py'), 'utf-8');
  assert(mainPy.includes('@dhv:source-map'), 'main.py 应含围栏');
  assert(mainPy.includes('dsh_plugins'), 'microkernel 脚手架应含插件注册表');
});

test('emit', 'scale=monolith 脚手架形态切换', () => {
  const out = path.join(TMP, 'mono');
  const r = run(['emit', 'examples/backends-demo/agent.hsl', '--out', out, '--scale', 'monolith']);
  assertEq(r.code, 0, 'monolith emit');
  const py = fs.readFileSync(path.join(out, 'gen/python/agent.py'), 'utf-8');
  assert(py.includes('demo_agent_run'), 'monolith 脚手架应含直调函数形态');
});

test('emit', '新方法映射活体翻译语义级验证（pop/sort/remove/repeat/expect/and_then）', () => {
  const dir = path.join(TMP, 'newmethods');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'gen.hsl'), `fn gen_vec_ops() -> i64 {
    let mut stack = vec![1, 2, 3];
    let top = stack.pop();
    stack.clear();
    let mut scores = vec![3, 1, 2];
    scores.sort();
    match top {
        Option::Some(v) => v + scores.len(),
        Option::None => 0,
    }
}
fn gen_map_ops() -> i64 {
    let mut counts: HashMap<String, i64> = HashMap::new();
    counts.insert(String::from("a"), 1);
    counts.insert(String::from("b"), 2);
    let removed = counts.remove(String::from("a"));
    match removed {
        Option::Some(v) => v + counts.len(),
        Option::None => 0,
    }
}
fn gen_option_ops() -> i64 {
    let must: i64 = Option::Some(42).expect(String::from("必须存在"));
    let fallback: i64 = Option::None.unwrap_or(-1);
    let maybe = Option::Some(5).and_then(|x| x * 2);
    match maybe {
        Option::Some(v) => must + fallback + v,
        Option::None => 0,
    }
}
fn main() {}
project {
    gen_vec_ops -> "gen_vec.py" : python,
    gen_map_ops -> "gen_map.py" : python,
    gen_option_ops -> "gen_option.py" : python,
}`);
  const r = run(['emit', path.join(dir, 'gen.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  // 语义级验证：执行生成的 python
  const pyCode = `
import sys
ns = {}
for mod in ['gen_vec', 'gen_map', 'gen_option']:
    src = open('${fwd(dir)}/' + mod + '.py').read()
    lines = src.splitlines()
    fn_start = next(i for i, l in enumerate(lines) if l.startswith('def '))
    exec(chr(10).join(lines[fn_start:]), ns)
assert ns['gen_vec_ops']() == 6, ns['gen_vec_ops']()
assert ns['gen_map_ops']() == 2, ns['gen_map_ops']()
assert ns['gen_option_ops']() == 51, ns['gen_option_ops']()
print('semantics-ok')
`;
  fs.writeFileSync(path.join(dir, 'verify.py'), pyCode);
  try {
    const stdout = execFileSync('python3', [path.join(dir, 'verify.py')], { encoding: 'utf-8', timeout: 30_000 });
    assert(stdout.includes('semantics-ok'), `语义验证输出异常：${stdout}`);
  } catch (e) {
    throw new Error(`生成代码语义验证失败：${(e as Error).message}`);
  }
});

test('emit', 'if-let 枚举变体模式活体翻译语义级验证（tuple/struct/Result）', () => {
  const dir = path.join(TMP, 'iflet');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'iflet.hsl'), `enum Color { Red(i32), Green(i32, i32), Named { tag: String, val: i32 } }
fn color_red(c: Color) -> i32 {
    if let Color::Red(r) = c { return r; }
    -1
}
fn color_green_sum(c: Color) -> i32 {
    if let Color::Green(a, b) = c { return a + b; }
    -1
}
fn color_named_val(c: Color) -> i32 {
    if let Color::Named { tag, val } = c { return val; }
    -1
}
fn ok_extract(r: Result<i32, String>) -> i32 {
    if let Result::Ok(v) = r { return v; }
    -1
}
fn err_extract(r: Result<i32, String>) -> i32 {
    if let Result::Err(_) = r { return 1; }
    0
}
fn main() {}
project {
    Color -> "color.py" : python,
    color_red -> "color_red.py" : python,
    color_green_sum -> "color_green_sum.py" : python,
    color_named_val -> "color_named_val.py" : python,
    ok_extract -> "ok_extract.py" : python,
    err_extract -> "err_extract.py" : python,
}`);
  const r = run(['emit', path.join(dir, 'iflet.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const pyCode = `
class Color: pass
class Red(Color):
    def __init__(self, f0): self.f0 = f0
class Green(Color):
    def __init__(self, f0, f1): self.f0 = f0; self.f1 = f1
class Named(Color):
    def __init__(self, tag, val): self.tag = tag; self.val = val
class Result: pass
class Ok(Result):
    def __init__(self, f0): self.f0 = f0
class Err(Result):
    def __init__(self, f0): self.f0 = f0

ns = {'Color': Color, 'Red': Red, 'Green': Green, 'Named': Named, 'Result': Result, 'Ok': Ok, 'Err': Err}
import os
for fn in ['color_red', 'color_green_sum', 'color_named_val', 'ok_extract', 'err_extract']:
    p = os.path.join('${fwd(dir)}', fn + '.py')
    src = open(p).read()
    lines = src.splitlines()
    fn_start = next(i for i, l in enumerate(lines) if l.startswith('def '))
    exec(chr(10).join(lines[fn_start:]), ns)

assert ns['color_red'](Red(7)) == 7, ns['color_red'](Red(7))
assert ns['color_red'](Green(1, 2)) == -1
assert ns['color_green_sum'](Green(3, 4)) == 7
assert ns['color_named_val'](Named('x', 99)) == 99
assert ns['ok_extract'](Ok(42)) == 42
assert ns['ok_extract'](Err('e')) == -1
assert ns['err_extract'](Err('e')) == 1
assert ns['err_extract'](Ok(42)) == 0
print('iflet-semantics-ok')
`;
  fs.writeFileSync(path.join(dir, 'verify.py'), pyCode);
  try {
    const stdout = execFileSync('python3', [path.join(dir, 'verify.py')], { encoding: 'utf-8', timeout: 30_000 });
    assert(stdout.includes('iflet-semantics-ok'), `iflet 语义验证输出异常：${stdout}`);
  } catch (e) {
    throw new Error(`iflet 语义验证失败：${(e as Error).message}`);
  }
});

test('emit', 'while-let 副作用 scrutinee 语义级验证（每迭代只求值一次）', () => {
  const dir = path.join(TMP, 'whilelet');
  fs.mkdirSync(dir, { recursive: true });
  // 关键：next() 有副作用（自增计数器）；若每迭代求值两次，counter 会翻倍
  fs.writeFileSync(path.join(dir, 'wl.hsl'), `fn drain_via_counter() -> i32 {
    let mut total = 0;
    let mut cur = vec![1, 2, 3];
    while let Some(x) = cur.pop() {
        total = total + x;
    }
    total
}
fn main() {}
project {
    drain_via_counter -> "drain.py" : python,
}`);
  const r = run(['emit', path.join(dir, 'wl.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const drainPy = fs.readFileSync(path.join(dir, 'drain.py'), 'utf-8');
  // 活体翻译应成功（无 NotImplementedError）
  assert(!drainPy.includes('未翻译'), `drain 不应回退 contract：\n${drainPy}`);
  assert(drainPy.includes('while True'), `应合成 while True 形式：\n${drainPy}`);
  // 验证 scrutinee 被缓存在临时变量（_wl_N）中，避免重复求值
  assert(/_wl_\d+/.test(drainPy), `应 hoist 到 _wl_N 临时变量：\n${drainPy}`);

  const pyCode = `
ns = {}
src = open('${fwd(dir)}/drain.py').read()
lines = src.splitlines()
fn_start = next(i for i, l in enumerate(lines) if l.startswith('def '))
exec(chr(10).join(lines[fn_start:]), ns)
assert ns['drain_via_counter']() == 6, ns['drain_via_counter']()
print('whilelet-semantics-ok')
`;
  fs.writeFileSync(path.join(dir, 'verify.py'), pyCode);
  try {
    const stdout = execFileSync('python3', [path.join(dir, 'verify.py')], { encoding: 'utf-8', timeout: 30_000 });
    assert(stdout.includes('whilelet-semantics-ok'), `whilelet 语义验证输出异常：${stdout}`);
  } catch (e) {
    throw new Error(`whilelet 语义验证失败：${(e as Error).message}`);
  }
});

test('emit', 'Some(x) / Ok(x) 单段简写归一化（与 Option::Some(x) / Result::Ok(x) 等价）', () => {
  const dir = path.join(TMP, 'shorthand');
  fs.mkdirSync(dir, { recursive: true });
  // 两版等价代码：一版用完整路径，一版用简写
  fs.writeFileSync(path.join(dir, 'long.hsl'), `fn drain_long() -> i32 {
    let mut total = 0;
    let mut cur = vec![1, 2, 3];
    while let Option::Some(x) = cur.pop() { total = total + x; }
    total
}
fn main() {}
project { drain_long -> "drain_long.py" : python }`);
  fs.writeFileSync(path.join(dir, 'short.hsl'), `fn drain_short() -> i32 {
    let mut total = 0;
    let mut cur = vec![1, 2, 3];
    while let Some(x) = cur.pop() { total = total + x; }
    total
}
fn main() {}
project { drain_short -> "drain_short.py" : python }`);
  const r1 = run(['emit', path.join(dir, 'long.hsl'), '--out', dir]);
  assertEq(r1.code, 0, `long emit 应通过：${r1.stdout}`);
  const r2 = run(['emit', path.join(dir, 'short.hsl'), '--out', dir]);
  assertEq(r2.code, 0, `short emit 应通过：${r2.stdout}`);
  const longPy = fs.readFileSync(path.join(dir, 'drain_long.py'), 'utf-8');
  const shortPy = fs.readFileSync(path.join(dir, 'drain_short.py'), 'utf-8');
  // 两版应都活体翻译（无 NotImplementedError）
  assert(!longPy.includes('未翻译'), `long 不应回退 contract`);
  assert(!shortPy.includes('未翻译'), `short 不应回退 contract`);
  // 执行两版 python 验证语义一致
  const pyVerify = `
ns = {}
for mod in ['drain_long', 'drain_short']:
    src = open('${fwd(dir)}/' + mod + '.py').read()
    lines = src.splitlines()
    fn_start = next(i for i, l in enumerate(lines) if l.startswith('def '))
    exec(chr(10).join(lines[fn_start:]), ns)
assert ns['drain_long']() == 6, ns['drain_long']()
assert ns['drain_short']() == 6, ns['drain_short']()
print('shorthand-semantics-ok')
`;
  fs.writeFileSync(path.join(dir, 'verify.py'), pyVerify);
  try {
    const stdout = execFileSync('python3', [path.join(dir, 'verify.py')], { encoding: 'utf-8', timeout: 30_000 });
    assert(stdout.includes('shorthand-semantics-ok'), `shorthand 语义验证输出异常：${stdout}`);
  } catch (e) {
    throw new Error(`shorthand 语义验证失败：${(e as Error).message}`);
  }
});

test('emit', '解析器回归：块表达式后跟尾表达式不再被误解析为二元', () => {
  // 这是 v0.3 修复的 parser bug：while let { ... } -1 应为两条语句（while-let + 尾表达式），
  // 而非 binary(whilelet, '-', 1)。验证方法：尾表达式 -1 应作为函数返回值（return -1），
  // 而非被吞掉
  const dir = path.join(TMP, 'blocktail');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'bt.hsl'), `fn first_ok() -> i32 {
    while true {
        if true { return 42; }
        break;
    }
    -1
}
fn main() {}
project { first_ok -> "first_ok.py" : python }`);
  const r = run(['emit', path.join(dir, 'bt.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const py = fs.readFileSync(path.join(dir, 'first_ok.py'), 'utf-8');
  // 应活体翻译
  assert(!py.includes('未翻译'), `不应回退 contract：\n${py}`);
  // 尾 -1 应作为 return 出现（即不被吞掉）
  assert(/return\s*\(?-1\)?/.test(py), `尾 -1 应翻译为 return -1：\n${py}`);
});

test('emit', 'if-let 函数尾值语义（分支产出 return）+ Vec::get 越界返回 None', () => {
  // v1.4.3：if-let 在尾位置现为值语义（对齐 match/if）；Vec::get 越界 → None（对齐 interp，闭合 #17）
  const dir = path.join(TMP, 'iflet-tail');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'it.hsl'), `fn pick(v: Vec<i64>) -> i64 {
    if let Option::Some(x) = v.get(0) { x + 100 } else { -1 }
}
fn main() {}
project { pick -> "pick.py" : python }`);
  const r = run(['emit', path.join(dir, 'it.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const py = fs.readFileSync(path.join(dir, 'pick.py'), 'utf-8');
  assert(!py.includes('未翻译'), `不应回退 contract：\n${py}`);
  assert(py.includes('_dhv_get'), `Vec::get 应映射到 _dhv_get 助手（Option 语义）：\n${py}`);
  const pyVerify = `
ns = {}
src = open('${fwd(dir)}/pick.py').read()
lines = src.splitlines()
fn_start = next(i for i, l in enumerate(lines) if l.startswith('def '))
exec(chr(10).join(lines[fn_start:]), ns)
assert ns['pick']([5, 6]) == 105, ns['pick']([5, 6])
assert ns['pick']([]) == -1, ns['pick']([])
print('iflet-tail-ok')
`;
  fs.writeFileSync(path.join(dir, 'verify.py'), pyVerify);
  const stdout = execFileSync('python3', [path.join(dir, 'verify.py')], { encoding: 'utf-8', timeout: 30_000 });
  assert(stdout.includes('iflet-tail-ok'), `if-let 尾语义验证异常：${stdout}`);
});

test('emit', 'match 臂内嵌套 if 保留值语义（回归：静默丢失 return 的 bug）', () => {
  // v1.4.3 修复：blockIntoValue 曾把 asValue 块尾的 if/match 降级为语句 → g(1) 返回 None 而非 10
  const dir = path.join(TMP, 'nested-if');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'n.hsl'), `fn g(x: i64) -> i64 {
    match x {
        1 => { if x > 0 { 10 } else { 20 } }
        _ => 30,
    }
}
fn main() {}
project { g -> "g.py" : python }`);
  const r = run(['emit', path.join(dir, 'n.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const py = fs.readFileSync(path.join(dir, 'g.py'), 'utf-8');
  assert(!py.includes('未翻译'), `不应回退 contract：\n${py}`);
  assert(/return\s*10/.test(py), `嵌套 if 分支应产出 return 10：\n${py}`);
  const pyVerify = `
ns = {}
src = open('${fwd(dir)}/g.py').read()
lines = src.splitlines()
fn_start = next(i for i, l in enumerate(lines) if l.startswith('def '))
exec(chr(10).join(lines[fn_start:]), ns)
assert ns['g'](1) == 10, ns['g'](1)
assert ns['g'](2) == 30, ns['g'](2)
print('nested-if-ok')
`;
  fs.writeFileSync(path.join(dir, 'verify.py'), pyVerify);
  const stdout = execFileSync('python3', [path.join(dir, 'verify.py')], { encoding: 'utf-8', timeout: 30_000 });
  assert(stdout.includes('nested-if-ok'), `嵌套 if 语义验证异常：${stdout}`);
});

test('emit', 'else if let / else if 链（elif 改写）', () => {
  const dir = path.join(TMP, 'elseif-let');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'e.hsl'), `fn h(a: Option<i64>, b: Option<i64>) -> String {
    if let Option::Some(x) = a { format!("first:{}", x) } else if let Option::Some(y) = b { format!("second:{}", y) } else { String::from("none") }
}
fn main() {}
project { h -> "h.py" : python }`);
  const r = run(['emit', path.join(dir, 'e.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const py = fs.readFileSync(path.join(dir, 'h.py'), 'utf-8');
  assert(!py.includes('未翻译'), `不应回退 contract：\n${py}`);
  assert(py.includes('elif'), `else-if-let 链应改写为 elif：\n${py}`);
  const pyVerify = `
ns = {}
src = open('${fwd(dir)}/h.py').read()
lines = src.splitlines()
fn_start = next(i for i, l in enumerate(lines) if l.startswith('def '))
exec(chr(10).join(lines[fn_start:]), ns)
assert ns['h'](7, None) == 'first:7', ns['h'](7, None)
assert ns['h'](None, 8) == 'second:8', ns['h'](None, 8)
assert ns['h'](None, None) == 'none', ns['h'](None, None)
print('elseif-ok')
`;
  fs.writeFileSync(path.join(dir, 'verify.py'), pyVerify);
  const stdout = execFileSync('python3', [path.join(dir, 'verify.py')], { encoding: 'utf-8', timeout: 30_000 });
  assert(stdout.includes('elseif-ok'), `else-if-let 语义验证异常：${stdout}`);
});

test('emit', '新方法映射语义级验证（clamp/floor/round/pow/fold/any/all/trim_start/char_at/or/get×2）', () => {
  const dir = path.join(TMP, 'methods-tour');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'm.hsl'), `fn num_tour(x: f64) -> i64 {
    let clamped = x.clamp(0.0, 10.0);
    let floored = clamped.floor() as i64;
    floored
}
fn agg_tour(v: Vec<i64>) -> i64 {
    let total = v.fold(0, |acc, n| acc + n);
    let doubled = v.map(|n| n * 2);
    total + doubled.sum() - v.len() as i64
}
fn pred_tour(v: Vec<i64>) -> bool {
    v.any(|n| n > 3) && v.all(|n| n >= 0)
}
fn str_tour(s: String) -> String {
    let padded = s.trim_start();
    let head = padded.char_at(0);
    format!("{}|{}", padded.to_uppercase(), head)
}
fn opt_tour(a: Option<i64>, b: Option<i64>) -> i64 {
    a.or(b).unwrap_or(-1)
}
fn get_tour(v: Vec<i64>, m: HashMap<String, i64>) -> i64 {
    let x = v.get(1).unwrap_or(0);
    let y = m.get("k").unwrap_or(7);
    x + y
}
fn main() {}
project {
    num_tour -> "num_tour.py" : python,
    agg_tour -> "agg_tour.py" : python,
    pred_tour -> "pred_tour.py" : python,
    str_tour -> "str_tour.py" : python,
    opt_tour -> "opt_tour.py" : python,
    get_tour -> "get_tour.py" : python,
}`);
  const r = run(['emit', path.join(dir, 'm.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const pyVerify = `
ns = {}
for fn in ['num_tour', 'agg_tour', 'pred_tour', 'str_tour', 'opt_tour', 'get_tour']:
    src = open('${fwd(dir)}/' + fn + '.py').read()
    lines = src.splitlines()
    fn_start = next(i for i, l in enumerate(lines) if l.startswith('def '))
    exec(chr(10).join(lines[fn_start:]), ns)
assert ns['num_tour'](15.7) == 10, ns['num_tour'](15.7)
assert ns['num_tour'](-2.5) == 0, ns['num_tour'](-2.5)
assert ns['agg_tour']([1, 2, 3]) == 15, ns['agg_tour']([1, 2, 3])
assert ns['pred_tour']([1, 4]) == True, ns['pred_tour']([1, 4])
assert ns['pred_tour']([1, 2]) == False, ns['pred_tour']([1, 2])
assert ns['str_tour']('  hi') == 'HI|h', repr(ns['str_tour']('  hi'))
assert ns['opt_tour'](None, None) == -1, ns['opt_tour'](None, None)
assert ns['get_tour']([5, 6], {}) == 13, ns['get_tour']([5, 6], {})
assert ns['get_tour']([], {'k': 3}) == 3, ns['get_tour']([], {'k': 3})
print('methods-tour-ok')
`;
  fs.writeFileSync(path.join(dir, 'verify.py'), pyVerify);
  const stdout = execFileSync('python3', [path.join(dir, 'verify.py')], { encoding: 'utf-8', timeout: 30_000 });
  assert(stdout.includes('methods-tour-ok'), `新方法语义验证异常：${stdout}`);
});

test('emit', 'Option::map 类型感知分发（与 Vec::map 同名不同义）', () => {
  const dir = path.join(TMP, 'opt-map');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'om.hsl'), `fn opt_map(o: Option<i64>) -> Option<i64> {
    o.map(|n| n * 10)
}
fn vec_map(v: Vec<i64>) -> Vec<i64> {
    v.map(|n| n + 1)
}
fn main() {}
project {
    opt_map -> "opt_map.py" : python,
    vec_map -> "vec_map.py" : python,
}`);
  const r = run(['emit', path.join(dir, 'om.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const pyVerify = `
ns = {}
for fn in ['opt_map', 'vec_map']:
    src = open('${fwd(dir)}/' + fn + '.py').read()
    lines = src.splitlines()
    fn_start = next(i for i, l in enumerate(lines) if l.startswith('def '))
    exec(chr(10).join(lines[fn_start:]), ns)
assert ns['opt_map'](5) == 50, ns['opt_map'](5)
assert ns['opt_map'](None) is None, ns['opt_map'](None)
assert ns['vec_map']([1, 2]) == [2, 3], ns['vec_map']([1, 2])
print('opt-map-ok')
`;
  fs.writeFileSync(path.join(dir, 'verify.py'), pyVerify);
  const stdout = execFileSync('python3', [path.join(dir, 'verify.py')], { encoding: 'utf-8', timeout: 30_000 });
  assert(stdout.includes('opt-map-ok'), `Option::map 分发验证异常：${stdout}`);
});

test('emit', 'java/kotlin/swift contract 声明语法质量（class 包装 + 类型后置冒号）', () => {
  const dir = path.join(TMP, 'contract-qual');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'c.hsl'), `struct Task {
    id: i64,
    title: String,
    done: bool,
}
enum Status {
    Pending,
    Running { progress: f64 },
    Failed(String),
}
trait Runner {
    fn run(&self, t: &Task) -> Result<Status, String>;
}
fn describe(t: &Task) -> String { t.title.clone() }
fn main() {}
project {
    describe -> "Cq.java" : java,
    describe -> "cq.kt" : kotlin,
    describe -> "Cq.swift" : swift,
    Task -> "Cq.java" : java,
    Status -> "Cq.java" : java,
    Runner -> "Cq.java" : java,
}`);
  const r = run(['emit', path.join(dir, 'c.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  // Java：类型顶层合法（record/sealed/interface）；顶层函数非法 → 宿主 class Dhv<Stem>
  // （Task 20 重构：旧版全项嵌进 public class 有两个非法点 —— public 类名不匹配文件名 +
  // 同模块多文件 wrapper 重名；新版类型顶层 → 同包裸名互见，跨文件引用无需限定）
  const java = fs.readFileSync(path.join(dir, 'Cq.java'), 'utf-8');
  assert(/^record Task\(/m.test(java), `Java struct 应为顶层 record：\n${java}`);
  assert(/^sealed interface Status/m.test(java), `Java enum 应为顶层 sealed interface：\n${java}`);
  assert(/^interface Runner/m.test(java), `Java trait 应为顶层 interface：\n${java}`);
  assert(/^class DhvCq \{/m.test(java), `Java fn 应有宿主 class DhvCq：\n${java}`);
  assert(!/^public class/m.test(java), `Java 不应有 public class（文件名不匹配即非法）：\n${java}`);
  assert(/static String describe\(Task t\)/.test(java), `Java fn 应为 static 成员方法：\n${java}`);
  // 跨文件引用裸名可见性：签名中 Task 未限定（同包顶层 record）
  assert(/describe\(Task t\)/.test(java), `签名引用 Task 应为裸名（顶层同包互见）：\n${java}`);
  // Kotlin：类型后置冒号 fun describe(t: Task): String
  const kt = fs.readFileSync(path.join(dir, 'cq.kt'), 'utf-8');
  assert(/fun describe\(t: Task\): String/.test(kt), `Kotlin 签名应为类型后置：\n${kt}`);
  // Swift：类型后置冒号 func describe(t: Task) -> String
  const swift = fs.readFileSync(path.join(dir, 'Cq.swift'), 'utf-8');
  assert(/func describe\(t: Task\) -> String/.test(swift), `Swift 签名应为类型后置：\n${swift}`);
});

test('emit', 'TypeScript get 类型感知（Vec 下标 / Map.get 均带 ?? null）', () => {
  const dir = path.join(TMP, 'ts-get');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 't.hsl'), `fn probe(v: Vec<i64>, m: HashMap<String, i64>) -> i64 {
    let a = v.get(0).unwrap_or(-1);
    let b = m.get("x").unwrap_or(-2);
    a + b
}
fn main() {}
project { probe -> "probe.ts" : typescript }`);
  const r = run(['emit', path.join(dir, 't.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const ts = fs.readFileSync(path.join(dir, 'probe.ts'), 'utf-8');
  assert(!ts.includes('未翻译'), `不应回退 contract：\n${ts}`);
  assert(/v\[0\] \?\? null/.test(ts), `Vec::get 应为下标 + ?? null（Option 语义）：\n${ts}`);
  assert(/m\.get\("x"\) \?\? null/.test(ts), `HashMap::get 应为 Map.get + ?? null：\n${ts}`);
});

// ---------------------------------------------------------------------------
// v1.4.4 扩面：新方法映射 / 元组下标 / 跨文件类型依赖 / cpp 编译级验证
// ---------------------------------------------------------------------------
function hasTool(tool: string): boolean {
  try { execFileSync('which', [tool], { encoding: 'utf-8', timeout: 5_000 }); return true; }
  catch { return false; }
}

/** g++ 是否支持 -std=c++23（生成物用 C++17 CTAD + C++20/23 特性；老编译器如实跳过而非失败） */
let cpp23Cache: boolean | undefined;
function hasCpp23(): boolean {
  if (cpp23Cache !== undefined) return cpp23Cache;
  const probe = fs.mkdtempSync(path.join(os.tmpdir(), 'hsl-cxx-'));
  const src = path.join(probe, 'p.cpp');
  fs.writeFileSync(src, '#include <variant>\nint main(){return std::holds_alternative<int>(std::variant<int,int>(1))?0:1;}\n');
  try {
    execFileSync('g++', ['-std=c++23', '-c', src, '-o', path.join(probe, 'p.o')], { timeout: 30_000, stdio: 'pipe' });
    cpp23Cache = true;
  } catch {
    cpp23Cache = false;
  } finally {
    fs.rmSync(probe, { recursive: true, force: true });
  }
  return cpp23Cache;
}

/** v1.4.10：go 工具链探测（PATH 或 ~/opt/go1.27 用户级安装）——真机编译级验证 */
function goBin(): string | null {
  for (const c of ['go', path.join(os.homedir(), 'opt/go1.27/bin/go')]) {
    try { execFileSync(c, ['version'], { encoding: 'utf-8', timeout: 5_000 }); return c; } catch { /* next */ }
  }
  return null;
}

/** v1.4.10：rustc/cargo 探测（PATH 或 ~/.cargo/bin） */
function rustcBin(): string | null {
  for (const c of ['rustc', path.join(os.homedir(), '.cargo/bin/rustc')]) {
    try { execFileSync(c, ['--version'], { encoding: 'utf-8', timeout: 5_000 }); return c; } catch { /* next */ }
  }
  return null;
}

test('emit', 'v1.4.4 新方法映射语义级验证（strip/find/position/enumerate/cloned/Vec insert/remove/Map remove Option）', () => {
  const dir = path.join(TMP, 'v144-methods');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'v.hsl'), `fn strip_tour(s: String) -> String {
    let a = s.strip_prefix("ab").unwrap_or("NONE");
    let b = s.strip_suffix("yz").unwrap_or("NONE");
    format!("{}|{}", a, b)
}
fn find_tour(s: String) -> i64 {
    s.find("ll").unwrap_or(-1)
}
fn pos_tour(v: Vec<i64>) -> i64 {
    v.position(|n| n > 3).unwrap_or(-1)
}
fn enum_tour(v: Vec<String>) -> String {
    let pairs = v.enumerate();
    let first = pairs[0];
    format!("{}:{}", first.0, first.1)
}
fn vec_ins_rem() -> i64 {
    let mut v: Vec<i64> = vec![1, 2, 5];
    v.insert(2, 3);
    let removed = v.remove(0);
    v[0] + removed
}
fn map_remove_opt(m: HashMap<String, i64>) -> i64 {
    let a = m.remove("a").unwrap_or(-1);
    let b = m.remove("zz").unwrap_or(-2);
    a + b
}
fn cloned_tour(o: Option<i64>) -> i64 {
    o.cloned().unwrap_or(-9)
}
fn main() {}
project {
    strip_tour -> "strip_tour.py" : python,
    find_tour -> "find_tour.py" : python,
    pos_tour -> "pos_tour.py" : python,
    enum_tour -> "enum_tour.py" : python,
    vec_ins_rem -> "vec_ins_rem.py" : python,
    map_remove_opt -> "map_remove_opt.py" : python,
    cloned_tour -> "cloned_tour.py" : python,
}`);
  const r = run(['emit', path.join(dir, 'v.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const pyVerify = `
ns = {}
for fn in ['strip_tour', 'find_tour', 'pos_tour', 'enum_tour', 'vec_ins_rem', 'map_remove_opt', 'cloned_tour']:
    src = open('${fwd(dir)}/' + fn + '.py').read()
    lines = src.splitlines()
    fn_start = next(i for i, l in enumerate(lines) if l.startswith('def '))
    exec(chr(10).join(lines[fn_start:]), ns)
assert ns['strip_tour']('abcdef') == 'cdef|NONE', repr(ns['strip_tour']('abcdef'))
assert ns['strip_tour']('abyz') == 'yz|ab', repr(ns['strip_tour']('abyz'))
assert ns['strip_tour']('xy') == 'NONE|NONE', repr(ns['strip_tour']('xy'))
assert ns['find_tour']('hello') == 2, ns['find_tour']('hello')
assert ns['find_tour']('helo') == -1, ns['find_tour']('helo')
assert ns['pos_tour']([1, 4, 2]) == 1, ns['pos_tour']([1, 4, 2])
assert ns['pos_tour']([1, 2]) == -1, ns['pos_tour']([1, 2])
assert ns['enum_tour'](['x', 'y']) == '0:x', repr(ns['enum_tour'](['x', 'y']))
assert ns['vec_ins_rem']() == 3, ns['vec_ins_rem']()
assert ns['map_remove_opt']({'a': 5}) == 3, ns['map_remove_opt']({'a': 5})
assert ns['map_remove_opt']({}) == -3, ns['map_remove_opt']({})
assert ns['cloned_tour'](None) == -9, ns['cloned_tour'](None)
assert ns['cloned_tour'](None) == -9
print('v144-methods-ok')
`;
  fs.writeFileSync(path.join(dir, 'verify.py'), pyVerify);
  const stdout = execFileSync('python3', [path.join(dir, 'verify.py')], { encoding: 'utf-8', timeout: 30_000 });
  assert(stdout.includes('v144-methods-ok'), `v1.4.4 方法语义验证异常：${stdout}`);
});

test('emit', '元组下标访问生成端修复（t.0 → py/ts 下标 t[0]；rust 原生 t.0）', () => {
  const dir = path.join(TMP, 'tuple-idx');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'tu.hsl'), `fn tup() -> i64 {
    let t = (10, 20);
    let a = t.0;
    let b = t.1;
    a * 10 + b
}
fn main() {}
project {
    tup -> "tup.py" : python,
    tup -> "tup.ts" : typescript,
    tup -> "tup.rs" : rust,
}`);
  const r = run(['emit', path.join(dir, 'tu.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const py = fs.readFileSync(path.join(dir, 'tup.py'), 'utf-8');
  const ts = fs.readFileSync(path.join(dir, 'tup.ts'), 'utf-8');
  const rs = fs.readFileSync(path.join(dir, 'tup.rs'), 'utf-8');
  assert(/t\[0\]/.test(py) && /t\[1\]/.test(py), `python 元组访问应为下标：\n${py}`);
  assert(py.includes("t = (10, 20)"), `python 元组字面量：\n${py}`);
  assert(/t\[0\]/.test(ts), `ts 元组访问应为下标：\n${ts}`);
  assert(/t\.0/.test(rs), `rust 应保持原生 t.0：\n${rs}`);
  // 语义级：python exec
  const pyVerify = `
ns = {}
src = open('${fwd(dir)}/tup.py').read()
lines = src.splitlines()
fn_start = next(i for i, l in enumerate(lines) if l.startswith('def '))
exec(chr(10).join(lines[fn_start:]), ns)
assert ns['tup']() == 120, ns['tup']()
print('tuple-idx-ok')
`;
  fs.writeFileSync(path.join(dir, 'verify.py'), pyVerify);
  const stdout = execFileSync('python3', [path.join(dir, 'verify.py')], { encoding: 'utf-8', timeout: 30_000 });
  assert(stdout.includes('tuple-idx-ok'), `元组语义验证异常：${stdout}`);
});

test('emit', 'manifest 诚实边界：full/logic 文件的 contract_fallbacks 逐块记录', () => {
  const dir = path.join(TMP, 'cbf');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'cbf.hsl'), `fn pure(x: i64) -> i64 { x * 2 }
fn with_native() -> i64 {
    let v: i64 = native typescript {
        return 6 * 7;
    };
    v
}
fn main() {}
project {
    pure       -> "pure.py"  : python,
    with_native -> "nat.py"  : python,
    with_native -> "nat.rs"  : rust,
}`);
  const r = run(['emit', path.join(dir, 'cbf.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const manifest = JSON.parse(fs.readFileSync(path.join(dir, 'manifest.json'), 'utf-8')) as {
    files: { path: string; tier: string; contract_fallbacks?: string[] }[];
  };
  const byPath = new Map(manifest.files.map((f) => [f.path, f]));
  const pure = byPath.get('pure.py')!;
  const natPy = byPath.get('nat.py')!;
  const natRs = byPath.get('nat.rs')!;
  // 活体文件：无 contract_fallbacks 字段
  assert(pure.tier === 'full' && !pure.contract_fallbacks, `pure.py 应为 full 且无回退：${JSON.stringify(pure)}`);
  // full 语言中回退的块被逐名记录（tier 仍是语言能力级）
  assert(natPy.tier === 'full' && natPy.contract_fallbacks?.includes('with_native'),
    `nat.py 应记录 contract_fallbacks=["with_native"]：${JSON.stringify(natPy)}`);
  // logic 语言同样记录
  assert(natRs.tier === 'logic' && natRs.contract_fallbacks?.includes('with_native'),
    `nat.rs 应记录 contract_fallbacks=["with_native"]：${JSON.stringify(natRs)}`);
  // 清单行也带回退标记（人类可读）
  assert(r.stdout.includes('with_native 回退 contract'), `emit 列表应显示回退标记：\n${r.stdout}`);
});

test('emit', '跨文件类型依赖接线（py from-import / ts import / rs use / cpp 内联）', () => {
  const out = path.join(TMP, 'xfile');
  const r = run(['emit', 'examples/backends-demo/agent.hsl', '--out', out]);
  assertEq(r.code, 0, `emit 应零警告通过：\n${r.stdout}`);
  assert(!r.stdout.includes('⚠'), `不应有依赖告警：\n${r.stdout.split('\n').filter((l) => l.includes('⚠')).join('\n')}`);
  const py = fs.readFileSync(path.join(out, 'gen/python/summarize.py'), 'utf-8');
  assert(py.includes('from toolresult import ToolResult'), `python 应有 from-import：\n${py.slice(0, 900)}`);
  const ts = fs.readFileSync(path.join(out, 'gen/typescript/summarize.ts'), 'utf-8');
  assert(ts.includes("import { ToolResult } from './toolresult';"), `ts 应有相对 import：\n${ts.slice(0, 900)}`);
  const rs = fs.readFileSync(path.join(out, 'gen/rust/summarize.rs'), 'utf-8');
  assert(rs.includes('use crate::'), `rust 应有 use crate:: 约定：\n${rs.slice(0, 600)}`);
  const cpp = fs.readFileSync(path.join(out, 'gen/cpp/summarize.cpp'), 'utf-8');
  assert(cpp.includes('struct ToolResult') && cpp.includes('跨文件类型依赖'), `cpp 应内联 ToolResult 声明：\n${cpp.slice(0, 1400)}`);
  const dcpp = fs.readFileSync(path.join(out, 'gen/cpp/describe.cpp'), 'utf-8');
  assert(dcpp.includes('using Action = std::variant'), `cpp describe 应内联未投射的 Action：\n${dcpp.slice(0, 1400)}`);
});

test('emit', 'X-1 警告回归（引用未投射类型 → 退出码 1 + 诚实告警）', () => {
  const dir = path.join(TMP, 'x1-warn');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'x.hsl'), `enum Color { Red }
fn paint(c: Color) -> String {
    match c {
        Color::Red => String::from("r"),
    }
}
fn main() {}
project { paint -> "paint.py" : python }`);
  const r = run(['emit', path.join(dir, 'x.hsl'), '--out', dir]);
  assertEq(r.code, 1, `X-1 警告应致退出码 1（exit=${r.code}）`);
  assert(r.stdout.includes('X-1'), `应含 X-1 告警：${r.stdout.slice(-400)}`);
  assert(r.stdout.includes('Color'), `告警应指名类型：${r.stdout.slice(-400)}`);
});

test('emit', 'cpp g++ 编译级验证（backends-demo 全 6 文件 + 链接语义对齐解释器）', () => {
  if (!hasTool('g++') || !hasCpp23()) return; // 无 g++ / 不支持 C++23 跳过（非失败）
  const out = path.join(TMP, 'cpp-compile');
  const r = run(['emit', 'examples/backends-demo/agent.hsl', '--out', out]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const cppFiles = fs.readdirSync(path.join(out, 'gen/cpp')).filter((f) => f.endsWith('.cpp'));
  assert(cppFiles.length >= 6, `应有 ≥6 个 cpp 文件，实际 ${cppFiles.length}`);
  const objs: string[] = [];
  for (const f of cppFiles) {
    const obj = path.join(out, f.replace(/\.cpp$/, '.o'));
    execFileSync('g++', ['-std=c++23', '-c', path.join(out, 'gen/cpp', f), '-o', obj], { timeout: 60_000 });
    objs.push(obj);
  }
  // 链接语义级：与解释器输出逐字对齐
  const mainCpp = `#include <string>
#include <vector>
#include <unordered_map>
#include <variant>
#include <cstdio>
#include "gen/cpp/describe.cpp"
#include "gen/cpp/clamp.cpp"
#include "gen/cpp/summarize.cpp"
int main() {
    std::string s1 = describe_action(Stop{});
    std::unordered_map<std::string, std::string> args;
    args["a"] = "1"; args["b"] = "2";
    std::string s2 = describe_action(CallTool{std::string("grep"), args});
    int64_t c = clamp_turns(99, 1, 24);
    std::vector<ToolResult> rs;
    rs.push_back(ToolResult{std::string("bash"), std::string("ok"), true});
    rs.push_back(ToolResult{std::string("grep"), std::string("miss"), false});
    std::string s3 = summarize(rs);
    printf("%s|%s|%lld|%s\\n", s1.c_str(), s2.c_str(), (long long)c, s3.c_str());
    return 0;
}
`;
  fs.writeFileSync(path.join(out, 'main_check.cpp'), mainCpp);
  execFileSync('g++', ['-std=c++23', path.join(out, 'main_check.cpp'), '-o', path.join(out, 'main_check')], { timeout: 120_000 });
  const stdout = execFileSync(path.join(out, 'main_check'), { encoding: 'utf-8', timeout: 30_000 });
  assertEq(stdout.trim(), 'stop|call grep with 2 args|24|1 / 2 工具调用成功', `cpp 生成代码语义应与解释器一致：${stdout}`);
});

test('emit', 'cpp g++ 编译级验证（pattern-tour：内联 Shape + 4 文件全编译）', () => {
  if (!hasTool('g++') || !hasCpp23()) return; // 无 g++ / 不支持 C++23 跳过（非失败）
  const out = path.join(TMP, 'pt-cpp');
  const r = run(['emit', 'dhv-ts/examples/pattern-tour.hsl', '--out', out]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const cppDir = path.join(out, 'gen/cpp');
  const cppFiles = fs.readdirSync(cppDir).filter((f) => f.endsWith('.cpp'));
  assert(cppFiles.length >= 4, `应有 ≥4 个 cpp 文件，实际 ${cppFiles.length}`);
  for (const f of cppFiles) {
    execFileSync('g++', ['-std=c++23', '-c', path.join(cppDir, f), '-o', path.join(out, f.replace(/\.cpp$/, '.o'))], { timeout: 60_000 });
  }
  // describe.cpp 应含内联的 Shape variant 结构（跨文件类型依赖机制）
  const dcpp = fs.readFileSync(path.join(cppDir, 'describe.cpp'), 'utf-8');
  assert(dcpp.includes('using Shape = std::variant'), `describe.cpp 应内联 Shape：\n${dcpp.slice(0, 1200)}`);
});

// ---------------------------------------------------------------------------
// Task 20 新增：cpp/go if-let/while-let 活体翻译 + Some/None 构造 + M3 静态化 + Java 重构
// ---------------------------------------------------------------------------

test('emit', 'cpp Some/None 构造 + if-let Option（g++ 编译级 + 链接运行语义）', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'cpp-some');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'sc.hsl'), `fn make(v: i64) -> Option<i64> {
    if v > 0 {
        Some(v * 2)
    } else {
        None
    }
}
fn use_opt(v: Option<i64>) -> i64 {
    if let Some(x) = v {
        x + 1
    } else {
        -1
    }
}
fn main() {}
project {
    make    -> "m.cpp"  : cpp,
    use_opt -> "uo.cpp" : cpp,
}`);
  const r = run(['emit', path.join(dir, 'sc.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const m = fs.readFileSync(path.join(dir, 'm.cpp'), 'utf-8');
  const uo = fs.readFileSync(path.join(dir, 'uo.cpp'), 'utf-8');
  // 结构断言：Some → _dhvSome 模板助手（含 include guard）；None → std::nullopt；
  // if-let Some → has_value() + *deref（非非法的 `!= null`）
  assert(m.includes('_dhvSome((v * 2))'), `Some 构造应为 _dhvSome 助手：\n${m}`);
  assert(m.includes('std::nullopt'), `None 应为 std::nullopt：\n${m}`);
  assert(m.includes('#ifndef DHV_SOME_HELPER'), `cpp 头应有 _dhvSome include guard（单 TU 拼接安全）：\n${m}`);
  assert(uo.includes('.has_value()'), `if-let Some 应为 has_value() 条件：\n${uo}`);
  assert(uo.includes('auto x = *'), `Some 绑定应为解引用：\n${uo}`);
  assert(!uo.includes('!= null'), `cpp 不应出现 null（非法 C++）：\n${uo}`);
  // 编译 + 链接 + 运行语义验证
  execFileSync('g++', ['-std=c++23', '-c', path.join(dir, 'm.cpp'), '-o', path.join(dir, 'm.o')], { timeout: 60_000 });
  execFileSync('g++', ['-std=c++23', '-c', path.join(dir, 'uo.cpp'), '-o', path.join(dir, 'uo.o')], { timeout: 60_000 });
  const main = `#include <cstdint>
#include <cstdio>
#include <optional>
template <typename T> std::optional<T> _dhvSome(const T& v) { return std::optional<T>(v); }
std::optional<int64_t> make(int64_t v);
int64_t use_opt(std::optional<int64_t> v);
int main() {
    auto a = make(5); auto b = make(-3);
    printf("%d %d %ld %ld\\n", a.has_value() ? 1 : 0, b.has_value() ? 1 : 0,
        (long)(a.has_value() ? *a : -999), (long)use_opt(_dhvSome(42)));
    printf("%ld\\n", (long)use_opt(std::nullopt));
    return 0;
}
`;
  fs.writeFileSync(path.join(dir, 'main.cpp'), main);
  execFileSync('g++', ['-std=c++23', path.join(dir, 'main.cpp'), path.join(dir, 'm.o'), path.join(dir, 'uo.o'), '-o', path.join(dir, 'sc-test')], { timeout: 60_000 });
  const stdout = execFileSync(path.join(dir, 'sc-test'), { encoding: 'utf-8', timeout: 30_000 });
  assert(stdout.includes('1 0 10 43'), `make/use_opt 语义应与解释器对齐（Some(10)/None/43）：${stdout}`);
  assert(stdout.trim().endsWith('-1'), `use_opt(None) 应为 -1：${stdout}`);
});

test('emit', 'cpp/go if-let 变体链 + while-let 变量循环（pattern-tour 巡览 + 结构断言）', () => {
  const out = path.join(TMP, 'pt-ilwl');
  const r = run(['emit', 'dhv-ts/examples/pattern-tour.hsl', '--out', out]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  // cpp：describe/classify 为活体（holds_alternative + get + 无绑定变体跳过 _v）
  const dcpp = fs.readFileSync(path.join(out, 'gen/cpp/describe.cpp'), 'utf-8');
  assert(dcpp.includes('std::holds_alternative<Circle>'), `cpp if-let 变体应为 holds_alternative：\n${dcpp}`);
  assert(dcpp.includes('auto& _v = std::get<Circle>'), `cpp 变体解包应为 std::get：\n${dcpp}`);
  // pattern-tour 的 describe 是顺序 if-let + 早 return 风格（非 else 链）——三个变体各一段
  assert((dcpp.match(/if \(std::holds_alternative</g) ?? []).length >= 3, `三个变体应各有一条 holds_alternative 判断：\n${dcpp}`);
  const ccpp = fs.readFileSync(path.join(out, 'gen/cpp/classify.cpp'), 'utf-8');
  assert(!ccpp.includes('未翻译'), `classify.cpp 应为活体：\n${ccpp}`);
  // go：classify 类型断言 init-statement + 大写字段 + 无绑定 blank 标识符
  const cgo = fs.readFileSync(path.join(out, 'gen/go/classify.go'), 'utf-8');
  assert(/if _ifv_\d+, _ok_\d+ := s\.\(Point\); _ok_\d+ \{/.test(cgo), `go if-let 变体应为类型断言 init-statement：\n${cgo}`);
  assert(cgo.includes('.X'), `go struct 变体字段应大写（F0/X/Y）：\n${cgo}`);
  assert(/if _, _ok_\d+ := s\.\(Unit\); _ok_\d+ \{/.test(cgo), `go 无绑定变体应用 blank 标识符（防 unused）：\n${cgo}`);
  // go/cpp while-let：count_down 循环内求值 + break + None 赋值
  const wgo = fs.readFileSync(path.join(out, 'gen/go/count_down.go'), 'utf-8');
  assert(wgo.includes('for {'), `go while-let 应为 for {}：\n${wgo}`);
  assert(wgo.includes('break'), `go while-let 应含 break：\n${wgo}`);
  assert(wgo.includes('cur = nil') || wgo.includes('fuel = nil'), `go None 赋值应为 nil（非非法 None）：\n${wgo}`);
  assert(wgo.includes('_dhvSome('), `go Some 构造应为 _dhvSome 泛型助手：\n${wgo}`);
  // v1.4.10：go 同 package 多文件顶级助手去重 —— 定义仅在首个 go 文件，调用点在各文件
  const goFiles = fs.readdirSync(path.join(out, 'gen/go')).filter((f) => f.endsWith('.go'));
  const goAll = goFiles.map((f) => fs.readFileSync(path.join(out, 'gen/go', f), 'utf-8')).join('\n');
  assert(goAll.includes('func _dhvSome[T any]'), `go package 应定义 _dhvSome（首文件）：\n${goFiles}`);
  assert(goAll.match(/func _dhvSome\[T any\]/g)!.length === 1, `go 助手应去重（同 package 单次定义）：\n${goFiles}`);
  const wcpp = fs.readFileSync(path.join(out, 'gen/cpp/count_down.cpp'), 'utf-8');
  assert(wcpp.includes('while (true)'), `cpp while-let 应为 while (true)：\n${wcpp}`);
  assert(wcpp.includes('std::nullopt'), `cpp None 赋值应为 std::nullopt：\n${wcpp}`);
  // python exec：count_down 生成代码语义级（4+3+2+1=10）
  const py = fs.readFileSync(path.join(out, 'gen/python/count_down.py'), 'utf-8');
  assert(!py.includes('未翻译'), `count_down.py 应为活体：\n${py}`);
  const runner = `import sys; sys.path.insert(0, ${JSON.stringify(path.join(out, 'gen/python'))})
from count_down import count_down
assert count_down(4) == 10, count_down(4)
assert count_down(None) == 0, count_down(None)
print('py-countdown-ok')
`;
  fs.writeFileSync(path.join(out, 'verify.py'), runner);
  const stdout = execFileSync('python3', [path.join(out, 'verify.py')], { encoding: 'utf-8', timeout: 30_000 });
  assert(stdout.includes('py-countdown-ok'), `python count_down 语义验证异常：${stdout}`);
});

test('emit', 'cpp Option match has_value（修复 v != null 非法代码）+ g++ 编译', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'cpp-optmatch');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'om.hsl'), `fn opt_test(v: Option<i64>) -> i64 {
    match v {
        Option::Some(x) => x * 2,
        Option::None => -1,
    }
}
fn main() {}
project { opt_test -> "om.cpp" : cpp }`);
  const r = run(['emit', path.join(dir, 'om.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const cpp = fs.readFileSync(path.join(dir, 'om.cpp'), 'utf-8');
  assert(cpp.includes('.has_value()'), `Option::Some 条件应为 has_value()：\n${cpp}`);
  assert(!cpp.includes('!= null'), `cpp 不应出现非法 null：\n${cpp}`);
  assert(!cpp.includes('const x'), `cpp 绑定应为 auto（非非法 const 无类型）：\n${cpp}`);
  execFileSync('g++', ['-std=c++23', '-c', path.join(dir, 'om.cpp'), '-o', path.join(dir, 'om.o')], { timeout: 60_000 });
});

test('emit', 'cpp String::to_string 字符串接收者（std::string 而非 std::to_string）', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'cpp-tostr');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'ts.hsl'), `fn label(s: String) -> String {
    s.to_string()
}
fn main() {}
project { label -> "lb.cpp" : cpp }`);
  const r = run(['emit', path.join(dir, 'ts.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const cpp = fs.readFileSync(path.join(dir, 'lb.cpp'), 'utf-8');
  assert(cpp.includes('std::string(s)'), `字符串接收者 to_string 应为 std::string(recv)：\n${cpp}`);
  assert(!cpp.includes('std::to_string(s)'), `std::to_string("x") 对字符串是非法 C++：\n${cpp}`);
  execFileSync('g++', ['-std=c++23', '-c', path.join(dir, 'lb.cpp'), '-o', path.join(dir, 'lb.o')], { timeout: 60_000 });
});

test('emit', 'go 变体模式大写字段（修复 f0 大小写错位——decls 大写 vs binds 小写）', () => {
  const out = path.join(TMP, 'pt-gofix');
  const r = run(['emit', 'dhv-ts/examples/pattern-tour.hsl', '--out', out]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  // describe.go 含 tuple 变体（Circle(f64)→F0/Rect(f64,f64)→F0+F1）与 struct 变体（Point→X/Y）
  const dgo = fs.readFileSync(path.join(out, 'gen/go/describe.go'), 'utf-8');
  // go 变体结构体字段是大写（decls capitalize）——绑定不应再引用小写 f0（大小写错位 = 编译失败）
  assert(/_ifv_\d+\.F0/.test(dgo), `go tuple 变体字段绑定应为大写 F0：\n${dgo}`);
  assert(!/\.f[01]\b/.test(dgo), `go 不应引用小写 f0/f1（字段不存在）：\n${dgo}`);
  const cgo2 = fs.readFileSync(path.join(out, 'gen/go/classify.go'), 'utf-8');
  assert(/_ifv_\d+\.X/.test(cgo2), `go struct 变体字段应大写 X：\n${cgo2}`);
});

test('emit', 'Java 顶层类型 + Dhv<Stem> 宿主（backends-demo 结构合法化）', () => {
  const out = path.join(TMP, 'bd-java');
  const r = run(['emit', 'examples/backends-demo/agent.hsl', '--out', out]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const tr = fs.readFileSync(path.join(out, 'gen/java/ToolResult.java'), 'utf-8');
  const sm = fs.readFileSync(path.join(out, 'gen/java/Summarize.java'), 'utf-8');
  const act = fs.readFileSync(path.join(out, 'gen/java/Action.java'), 'utf-8');
  // 类型顶层声明（record/sealed interface 不再嵌进 class）
  assert(/^record ToolResult\(/m.test(tr), `record 应顶层声明：\n${tr}`);
  assert(/^sealed interface Action permits/m.test(act), `sealed interface 应顶层：\n${act}`);
  // 无 public class（public 类名必须匹配文件名 —— 旧版非法）；宿主类 Dhv<Stem> 每文件唯一
  assert(!/^public class/m.test(sm), `不应有 public class（文件名不匹配即非法）：\n${sm}`);
  assert(/^class DhvSummarize \{/m.test(sm), `宿主类应为 DhvSummarize：\n${sm}`);
  // 跨文件引用裸名（顶层同包互见，无需 Model.ToolResult 限定）
  assert(sm.includes('List<ToolResult>'), `跨文件类型引用应为裸名：\n${sm}`);
  const hosts = [sm, fs.readFileSync(path.join(out, 'gen/java/Describe.java'), 'utf-8')];
  const hostNames = hosts.map((h) => h.match(/^class (\w+) \{/m)?.[1] ?? '');
  assert(new Set(hostNames).size === hostNames.length, `同模块多文件宿主类名必须唯一：${hostNames.join(', ')}`);
  // 零 X 告警（全部被引用类型已投射）
  const manifest = JSON.parse(fs.readFileSync(path.join(out, 'manifest.json'), 'utf-8'));
  assert((manifest.warnings ?? []).length === 0, `应零告警：${JSON.stringify(manifest.warnings)}`);
});

test('回归', 'M3 静态检查：import 未 export 名 → check 报错（不再等到 run/emit）', () => {
  const dir = path.join(TMP, 'm3-static');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'a.hsl'), `struct Internal { x: i64 }
export struct Good { y: i64 }
`);
  fs.writeFileSync(path.join(dir, 'b.hsl'), `import { Good } from "./a.hsl";
fn use_g(g: Good) -> i64 { g.y }
fn main() -> i64 { use_g(Good { y: 1 }) }
`);
  const ok = run(['check', path.join(dir, 'b.hsl')]);
  assertEq(ok.code, 0, `已 export 的 import 应通过：${ok.stdout}`);
  fs.writeFileSync(path.join(dir, 'b.hsl'), `import { Good, Internal } from "./a.hsl";
fn use_g(g: Good) -> i64 { g.y }
fn main() -> i64 { 0 }
`);
  const bad = run(['check', path.join(dir, 'b.hsl')]);
  assertEq(bad.code, 1, `未 export 的 import 应报错（exit=1）：${bad.stdout}`);
  assert(bad.stdout.includes('[M3]'), `应报 error[M3]：${bad.stdout}`);
  assert(bad.stdout.includes('Internal'), `M3 应点名 Internal：${bad.stdout}`);
});

test('回归', 'nova emit 回归（跨模块 import 全解析 + 零 X 告警）', () => {
  // Task 20 实录：nova 曾因 8 个定义缺 export 导致 run/emit 双失败（check 却全绿）——
  // 此用例防止回归
  const out = path.join(TMP, 'nova-emit');
  const r = run(['emit', 'examples/nova/nova.hsl', '--out', out]);
  assertEq(r.code, 0, `nova emit 应通过（缺 export 回归）：${r.stdout}${r.stderr}`);
  assert(r.stdout.includes('12 个文件'), `nova 应投射 12 个文件：${r.stdout}`);
  assert(r.stdout.includes('12 个通过语法校验'), `应全部通过语法校验：${r.stdout}`);
  const manifest = JSON.parse(fs.readFileSync(path.join(out, 'manifest.json'), 'utf-8'));
  assert((manifest.warnings ?? []).length === 0, `应零 X 告警：${JSON.stringify(manifest.warnings)}`);
});

test('emit', 'TypeScript Map::remove Option 语义（_dhvRemove 助手 + bun exec 实测）', () => {
  const dir = path.join(TMP, 'ts-remove');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'tr.hsl'), `fn mr(m: HashMap<String, i64>) -> i64 {
    let a = m.remove("a").unwrap_or(-1);
    let b = m.remove("zz").unwrap_or(-2);
    a + b
}
fn main() {}
project { mr -> "mr.ts" : typescript }`);
  const r = run(['emit', path.join(dir, 'tr.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const ts = fs.readFileSync(path.join(dir, 'mr.ts'), 'utf-8');
  assert(ts.includes('_dhvRemove('), `Map::remove 应使用 _dhvRemove 助手（Option 语义）：\n${ts}`);
  assert(ts.includes('function _dhvRemove'), `prelude 应定义 _dhvRemove：\n${ts}`);
  // bun exec 语义级验证
  const runner = `import { mr } from './mr.ts';
const m = new Map<string, number>([['a', 5]]);
if (mr(m) !== 3) { console.error('FAIL a=5: ' + mr(m)); process.exit(1); }
if (mr(new Map<string, number>()) !== -3) { console.error('FAIL empty'); process.exit(1); }
console.log('ts-remove-ok');
`;
  fs.writeFileSync(path.join(dir, 'runner.ts'), runner);
  const stdout = execFileSync('bun', [path.join(dir, 'runner.ts')], { encoding: 'utf-8', timeout: 30_000, cwd: dir });
  assert(stdout.includes('ts-remove-ok'), `ts remove 语义验证异常：${stdout}`);
});

test('emit', '副作用接收者单次求值（m.remove(k).unwrap_or(d) 不双重求值）', () => {
  const dir = path.join(TMP, 'single-eval');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'se.hsl'), `fn drain_map(m: HashMap<String, i64>) -> i64 {
    let a = m.remove("a").unwrap_or(-1);
    let b = m.remove("a").unwrap_or(-2);
    a + b
}
fn drain_vec(v: Vec<i64>) -> i64 {
    let x = v.pop().unwrap_or(-5);
    let y = v.pop().unwrap_or(-6);
    x * 10 + y
}
fn main() {}
project {
    drain_map -> "drain_map.py" : python,
    drain_vec -> "drain_vec.py" : python,
}`);
  const r = run(['emit', path.join(dir, 'se.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  // 语义级：remove/pop 副作用必须只发生一次 —— 第二次 remove 同键应得 None → 兜底值
  const pyVerify = `
ns = {}
for fn in ['drain_map', 'drain_vec']:
    src = open('${fwd(dir)}/' + fn + '.py').read()
    lines = src.splitlines()
    fn_start = next(i for i, l in enumerate(lines) if l.startswith('def '))
    exec(chr(10).join(lines[fn_start:]), ns)
# 第一次 remove('a')=5，第二次 remove('a') 应 None → -2（若双重求值：5 + 5 = 10 或 TypeError）
assert ns['drain_map']({'a': 5}) == 3, ns['drain_map']({'a': 5})
# pop 两次（弹出尾部）：[7, 8] → x=8, y=7 → 8*10+7 = 87
assert ns['drain_vec']([7, 8]) == 87, ns['drain_vec']([7, 8])
# pop 空表两次 → -5*10 + -6 = -56
assert ns['drain_vec']([]) == -56, ns['drain_vec']([])
print('single-eval-ok')
`;
  fs.writeFileSync(path.join(dir, 'verify.py'), pyVerify);
  const stdout = execFileSync('python3', [path.join(dir, 'verify.py')], { encoding: 'utf-8', timeout: 30_000 });
  assert(stdout.includes('single-eval-ok'), `副作用单次求值验证异常：${stdout}`);
});

test('回归', 'dhv Rust 源码与 BNF v1.4.x 一致性守护（宏尾 ! + S-6 通配 + 块表达式规则）', () => {
  // dhv 以源码形态交付（无本地 Rust 工具链），源码级断言防回退
  const pest = fs.readFileSync(path.join(ROOT, 'dhv/src/hsl.pest'), 'utf-8');
  assert(
    /macro_rules_definition\s*=\s*\{[^}]*"macro_rules"\s*~\s*"!"\s*~\s*identifier\s*~\s*"!"\?/.test(pest),
    'hsl.pest 宏定义应接受可选尾 !（BNF v1.4.2 容错）',
  );
  // v1.4.5：expression_with_block 此前被 expression 引用但从未定义 —— pest_derive 编译失败。
  // 现已定义：块表达式块尾自终止（块表达式不作二元 LHS，与 dhv-ts v1.4.2 #4 守卫同源）
  assert(
    /^expression_with_block\s*=\s*\{\s*block_primary\s*~\s*postfix_op\*/m.test(pest),
    'hsl.pest 应定义 expression_with_block（块表达式 + 后缀链；此前缺失致 pest_derive 编译失败）',
  );
  assert(
    /^block_primary\s*=\s*\{/m.test(pest) && pest.includes('| for_expression') && pest.includes('| match_expression'),
    'hsl.pest block_primary 应覆盖全部含块表达式（if/iflet/match/loop/while/whilelet/for/block/async/native）',
  );
  const tc = fs.readFileSync(path.join(ROOT, 'dhv/src/typecheck.rs'), 'utf-8');
  assert(
    tc.includes('!in_loop && wildcard_span.is_some()'),
    'typecheck.rs 应实现 AgentLoop 外 _ 通配 = 穷尽覆盖（BNF v1.4.1 S-6 修正）',
  );
  const langs = fs.readFileSync(path.join(ROOT, 'dhv/src/langs.rs'), 'utf-8');
  assert(langs.includes('38') || langs.match(/LangSpec\s*\{/g) !== null, 'langs.rs 注册表应存在');
  // v1.4.5：parser.rs 与新 Pair 树结构的契约（block_primary + postfix_op*，同 postfix_expression 处理）
  const parser = fs.readFileSync(path.join(ROOT, 'dhv/src/parser.rs'), 'utf-8');
  assert(
    /Rule::expression_with_block => \{[\s\S]*?apply_postfix/.test(parser),
    'parser.rs 应按 postfix 链处理 expression_with_block（块表达式 + 后缀）',
  );
});

test('解析', 'macro_rules! 定义名带尾 !（Rust 习惯迁移容错）+ 双形态展开', () => {
  const dir = path.join(TMP, 'macro-bang');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'mc.hsl'), `macro_rules! shout {
    ($msg:expr) => { println!("{}!!", $msg) }
}
macro_rules! whisper! {
    ($msg:expr) => { println!("{}...", $msg) }
}
fn main() {
    shout!("hello");
    whisper!("bye");
}`);
  const r = run(['run', path.join(dir, 'mc.hsl'), '--quiet']);
  assertEq(r.code, 0, `带尾 ! 的宏定义应可运行：\n${r.stdout}${r.stderr}`);
  assert(r.stdout.includes('hello!!'), `shout! 应展开（stdout=${r.stdout}）`);
  assert(r.stdout.includes('bye...'), `whisper!（尾 ! 定义）应展开（stdout=${r.stdout}）`);
});

// ---------------------------------------------------------------------------
// Task 21 新增：cpp/go Vec::pop/first/last/clone 活体映射 + matchDispatch 副作用 hoist
// ---------------------------------------------------------------------------

test('emit', 'cpp Vec::pop 活体映射（g++ 编译+链接+运行 drain=15）', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'cpp-pop');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'pop.hsl'), `fn drain(v: Vec<i32>) -> i32 {
    let mut total = 0;
    let mut cur = v;
    while let Some(x) = cur.pop() {
        total = total + x;
    }
    total
}
fn main() {}
project { drain -> "drain.cpp" : cpp }`);
  const r = run(['emit', path.join(dir, 'pop.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const cpp = fs.readFileSync(path.join(dir, 'drain.cpp'), 'utf-8');
  assert(cpp.includes('_dhvPop('), `cpp 应使用 _dhvPop 助手（Vec::pop → std::optional<T>）：\n${cpp}`);
  assert(cpp.includes('template <typename T> std::optional<T> _dhvPop'), `prelude 应定义 _dhvPop 模板助手：\n${cpp}`);
  assert(cpp.includes('#ifndef DHV_POP_HELPER'), `应带 include guard 防 ODR 冲突：\n${cpp}`);
  assert(cpp.includes('while (true)'), `cpp while-let 应为 while(true) + break：\n${cpp}`);
  assert(cpp.includes('.has_value()'), `cpp Option cond 应为 has_value()：\n${cpp}`);
  assert(cpp.includes('*_wl_'), `cpp Some 绑定应解引用临时 _wl_N：\n${cpp}`);
  assert(!cpp.includes('未翻译'), `drain.cpp 应为活体（非 contract 回退）：\n${cpp}`);
  // 编译 + 链接 + 运行
  const main = `#include <cstdint>
#include <vector>
#include <iostream>
#include <format>
int32_t drain(std::vector<int32_t> v);
int main() {
    std::cout << std::format("{}\\n", drain({1,2,3,4,5}));
    std::cout << std::format("{}\\n", drain({}));
    std::cout << std::format("{}\\n", drain({7}));
    return 0;
}`;
  fs.writeFileSync(path.join(dir, 'main.cpp'), main);
  execFileSync('g++', ['-std=c++23', path.join(dir, 'main.cpp'), path.join(dir, 'drain.cpp'), '-o', path.join(dir, 'pop-test')], { timeout: 60_000 });
  const stdout = execFileSync(path.join(dir, 'pop-test'), { encoding: 'utf-8', timeout: 30_000 });
  const lines = stdout.trim().split('\n');
  assertEq(lines[0], '15', `drain(1..5)=15（1+2+3+4+5）：${stdout}`);
  assertEq(lines[1], '0', `drain(empty)=0：${stdout}`);
  assertEq(lines[2], '7', `drain(7)=7：${stdout}`);
});

test('emit', 'cpp/go Vec::first/last/clone 活体映射（结构 + g++ 编译级）', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'cpp-flc');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'flc.hsl'), `fn first_or(v: Vec<i32>, d: i32) -> i32 {
    match v.first() {
        Option::Some(x) => x,
        Option::None => d,
    }
}
fn last_or(v: Vec<i32>, d: i32) -> i32 {
    match v.last() {
        Option::Some(x) => x,
        Option::None => d,
    }
}
fn cloned_first(v: Vec<i32>) -> i32 {
    let c = v.clone();
    match c.first() {
        Option::Some(x) => x,
        Option::None => -1,
    }
}
fn main() {}
project {
    first_or     -> "first_or.cpp"     : cpp,
    last_or      -> "last_or.cpp"      : cpp,
    cloned_first -> "cloned_first.cpp" : cpp,
    first_or     -> "first_or.go"      : go,
    last_or      -> "last_or.go"       : go,
}`);
  const r = run(['emit', path.join(dir, 'flc.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  // cpp first_or：_dhvFirst 助手 + match 内 _m_N hoist（单次求值）
  const foc = fs.readFileSync(path.join(dir, 'first_or.cpp'), 'utf-8');
  assert(foc.includes('_dhvFirst('), `first() 应映射 _dhvFirst 助手：\n${foc}`);
  assert(foc.includes('template <typename T> std::optional<T> _dhvFirst'), `prelude 应定义 _dhvFirst：\n${foc}`);
  assert(/auto _m_\d+ = _dhvFirst\(v\);/.test(foc), `match v.first() 应 hoist 到 _m_N（单次求值）：\n${foc}`);
  assert(!foc.match(/_dhvFirst\(v\)[\s\S]*_dhvFirst\(v\)/), `不应多次调用 _dhvFirst（双重求值）：\n${foc}`);
  // cpp last_or：_dhvLast 助手
  const loc = fs.readFileSync(path.join(dir, 'last_or.cpp'), 'utf-8');
  assert(loc.includes('_dhvLast('), `last() 应映射 _dhvLast 助手：\n${loc}`);
  assert(loc.includes('template <typename T> std::optional<T> _dhvLast'), `prelude 应定义 _dhvLast：\n${loc}`);
  // cpp cloned_first：clone 为值语义拷贝（cpp 拷贝构造）
  const cfc = fs.readFileSync(path.join(dir, 'cloned_first.cpp'), 'utf-8');
  assert(/let c = v;|auto c = v;/.test(cfc), `cpp clone 应为值拷贝（std::vector 拷贝构造）：\n${cfc}`);
  // g++ 编译全部 3 个 cpp 文件
  for (const f of ['first_or.cpp', 'last_or.cpp', 'cloned_first.cpp']) {
    execFileSync('g++', ['-std=c++23', '-c', path.join(dir, f), '-o', path.join(dir, f.replace(/\.cpp$/, '.o'))], { timeout: 60_000 });
  }
  // 链接 first_or + 运行
  const main = `#include <cstdint>
#include <vector>
#include <iostream>
#include <format>
int32_t first_or(std::vector<int32_t> v, int32_t d);
int main() {
    std::vector<int32_t> v{10, 20, 30};
    std::vector<int32_t> e{};
    std::cout << std::format("{} {} {} {}\\n", first_or(v, -1), first_or(e, 99), first_or({42}, -1), first_or({5, 6}, -1));
    return 0;
}`;
  fs.writeFileSync(path.join(dir, 'main.cpp'), main);
  execFileSync('g++', ['-std=c++23', path.join(dir, 'main.cpp'), path.join(dir, 'first_or.o'), '-o', path.join(dir, 'flc-test')], { timeout: 60_000 });
  const stdout = execFileSync(path.join(dir, 'flc-test'), { encoding: 'utf-8', timeout: 30_000 });
  assertEq(stdout.trim(), '10 99 42 5', `first_or 语义应与 interp 对齐：${stdout}`);
  // go 结构断言：_dhvFirst/_dhvLast 泛型助手 + 调用 & 取址
  const fgo = fs.readFileSync(path.join(dir, 'first_or.go'), 'utf-8');
  assert(fgo.includes('func _dhvFirst[T any]'), `go prelude 应定义 _dhvFirst 泛型助手：\n${fgo}`);
  assert(fgo.includes('func _dhvLast[T any]'), `go prelude 应定义 _dhvLast 泛型助手：\n${fgo}`);
  assert(fgo.includes('_dhvFirst(&v)'), `go first() 调用应为 _dhvFirst(&v)（指针传递副作用通道）：\n${fgo}`);
  assert(fgo.includes('!= nil'), `go Option cond 应为 != nil：\n${fgo}`);
  assert(!fgo.includes('未翻译'), `first_or.go 应为活体：\n${fgo}`);
});

test('emit', 'matchDispatch 副作用 scrutinee hoist（match v.pop() 单次求值）', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'match-hoist');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'pm.hsl'), `fn pop_sum(mut v: Vec<i32>) -> i32 {
    match v.pop() {
        Option::Some(x) => x + pop_sum(v),
        Option::None => 0,
    }
}
fn main() {}
project { pop_sum -> "pop_sum.cpp" : cpp, pop_sum -> "pop_sum.py" : python }`);
  const r = run(['emit', path.join(dir, 'pm.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  // 结构断言：cpp 应 hoist pop 到 _m_N（避免多次 pop 副作用）
  const cpp = fs.readFileSync(path.join(dir, 'pop_sum.cpp'), 'utf-8');
  assert(/auto _m_\d+ = _dhvPop\(v\);/.test(cpp), `cpp match v.pop() 应 hoist 到 _m_N：\n${cpp}`);
  assert(!cpp.match(/_dhvPop\(v\)[\s\S]+_dhvPop\(v\)/), `不应多次调用 _dhvPop（破坏 pop 副作用语义）：\n${cpp}`);
  // python：同样 hoist
  const py = fs.readFileSync(path.join(dir, 'pop_sum.py'), 'utf-8');
  assert(/_m_\d+ = _dhv_pop\(v\)/.test(py), `python match v.pop() 应 hoist：\n${py}`);
  // 计算函数体（跳过 prelude def）中的 _dhv_pop(v) 调用次数 —— 应恰好 1 次（hoist 处）
  const pyBody = py.replace(/^def _dhv_pop\(v\):[\s\S]*?^    return.*$/m, '');
  const pyPopCalls = (pyBody.match(/_dhv_pop\(v\)/g) ?? []).length;
  assertEq(pyPopCalls, 1, `python 函数体应只有 1 个 _dhv_pop(v) 调用（hoist），实际 ${pyPopCalls}：\n${pyBody}`);
  // 编译 + 链接 + 运行语义级
  const main = `#include <cstdint>
#include <vector>
#include <iostream>
#include <format>
int32_t pop_sum(std::vector<int32_t> v);
int main() {
    std::cout << std::format("{}\\n", pop_sum({10, 20, 30, 40}));
    std::cout << std::format("{}\\n", pop_sum({}));
    return 0;
}`;
  fs.writeFileSync(path.join(dir, 'main.cpp'), main);
  execFileSync('g++', ['-std=c++23', path.join(dir, 'main.cpp'), path.join(dir, 'pop_sum.cpp'), '-o', path.join(dir, 'pm-test')], { timeout: 60_000 });
  const stdout = execFileSync(path.join(dir, 'pm-test'), { encoding: 'utf-8', timeout: 30_000 });
  const lines = stdout.trim().split('\n');
  assertEq(lines[0], '100', `pop_sum(10..40)=100（10+20+30+40，递归 + 副作用 pop）：${stdout}`);
  assertEq(lines[1], '0', `pop_sum(empty)=0：${stdout}`);
  // python exec 语义级
  const pyVerify = `import sys; sys.path.insert(0, ${JSON.stringify(dir)})
from pop_sum import pop_sum
assert pop_sum([10, 20, 30, 40]) == 100, pop_sum([10, 20, 30, 40])
assert pop_sum([]) == 0, pop_sum([])
print('py-popsum-ok')
`;
  fs.writeFileSync(path.join(dir, 'verify.py'), pyVerify);
  const pyOut = execFileSync('python3', [path.join(dir, 'verify.py')], { encoding: 'utf-8', timeout: 30_000 });
  assert(pyOut.includes('py-popsum-ok'), `python pop_sum 语义验证异常：${pyOut}`);
});

test('emit', 'cpp/go Vec::pop 单次求值 + 副作用对接收者可见（pop + peek 模式）', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'pop-side');
  fs.mkdirSync(dir, { recursive: true });
  // 测试 v.pop().unwrap_or(d) 链式：pop 副作用对接收者 v 可见
  fs.writeFileSync(path.join(dir, 'ps.hsl'), `fn drain_and_peek(mut v: Vec<i32>) -> i32 {
    let popped = v.pop().unwrap_or(-1);
    let after = v.len();
    popped * 100 + (after as i32)
}
fn main() {}
project { drain_and_peek -> "dp.cpp" : cpp, drain_and_peek -> "dp.py" : python }`);
  const r = run(['emit', path.join(dir, 'ps.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  // 结构断言：pop 副作用对接收者 v 可见 —— cpp 应传递引用（_dhvPop(v) 内部 pop_back 修改 v）
  const cpp = fs.readFileSync(path.join(dir, 'dp.cpp'), 'utf-8');
  assert(cpp.includes('_dhvPop(v)'), `cpp pop 应通过引用传递（_dhvPop 修改 v）：\n${cpp}`);
  // 编译 + 链接 + 运行
  const main = `#include <cstdint>
#include <vector>
#include <iostream>
#include <format>
int32_t drain_and_peek(std::vector<int32_t> v);
int main() {
    std::cout << std::format("{}\\n", drain_and_peek({1, 2, 3}));
    std::cout << std::format("{}\\n", drain_and_peek({}));
    return 0;
}`;
  fs.writeFileSync(path.join(dir, 'main.cpp'), main);
  execFileSync('g++', ['-std=c++23', path.join(dir, 'main.cpp'), path.join(dir, 'dp.cpp'), '-o', path.join(dir, 'ps-test')], { timeout: 60_000 });
  const stdout = execFileSync(path.join(dir, 'ps-test'), { encoding: 'utf-8', timeout: 30_000 });
  // [1,2,3]: pop→3, v.len()=2 → 3*100+2 = 302
  // []: pop→None→-1, v.len()=0 → -1*100+0 = -100
  const lines = stdout.trim().split('\n');
  assertEq(lines[0], '302', `[1,2,3] popped=3 len=2 → 302：${stdout}`);
  assertEq(lines[1], '-100', `[] popped=-1 len=0 → -100：${stdout}`);
});

test('回归', 'validate balanceCheck 不再把 go/cpp 的 (*ptr) 解引用误判为注释', () => {
  // Task 21 修复：balanceCheck 之前对 (*v) 等解引用表达式误报 "(* ... *) 注释未闭合"（仅 ocaml/fsharp 方言应识别）
  const dir = path.join(TMP, 'val-fix');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'pf.hsl'), `fn peek(ptr_v: Option<i32>) -> i32 {
    match ptr_v {
        Option::Some(x) => x,
        Option::None => -1,
    }
}
fn main() {}
project { peek -> "peek.go" : go, peek -> "peek.cpp" : cpp }`);
  const r = run(['emit', path.join(dir, 'pf.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const go = fs.readFileSync(path.join(dir, 'peek.go'), 'utf-8');
  // go prelude 中 _dhvPop 内含 (*v)[n] —— balanceCheck 不应误判
  // （即使我们没在 peek 中用 pop，prelude 仍包含 _dhvPop 泛型助手）
  assert(go.includes('(*v)'), `go prelude 应含 (*v) 解引用：\n${go}`);
  // 验证 balanceCheck 直接对 (*v) 不报错
  const result = balanceCheck('func f(v *[]int) *int { return &(*v)[0] }', 'go');
  assert(result.ok === true, `balanceCheck 不应误报 go 的 (*v) 解引用：${result.detail ?? 'ok'}`);
  const cppResult = balanceCheck('int f(std::vector<int>& v) { return (*v)[0]; }', 'cpp');
  assert(cppResult.ok === true, `balanceCheck 不应误报 cpp 的 (*v) 解引用：${cppResult.detail ?? 'ok'}`);
  // ocaml/fsharp 仍应识别 (* *) 块注释
  const ocamlResult = balanceCheck('let x = (* this is a comment *) 42', 'ocaml');
  assert(ocamlResult.ok === true, `ocaml 应识别 (* *) 块注释：${ocamlResult.detail ?? 'ok'}`);
  const unclosedOcaml = balanceCheck('let x = (* unclosed', 'ocaml');
  assert(unclosedOcaml.ok === false, `ocaml 未闭合 (* 应报错：${unclosedOcaml.detail}`);
});

test('emit', 'C# 宿主类合法化（fn/const 入 internal static class Dhv<Stem>）', () => {
  const dir = path.join(TMP, 'csharp-wrap');
  fs.mkdirSync(dir, { recursive: true });
  // C# 顶层函数/常量非法 —— 必须包装进 class（与 Java 同构）
  fs.writeFileSync(path.join(dir, 'cs.hsl'), `struct Point { x: i64, y: i64 }
enum Shape { Circle(f64), Unit }
trait Area { fn area(self) -> f64; }
const MAX: i64 = 100;
fn clamp(v: i64, lo: i64, hi: i64) -> i64 {
    if v < lo { lo } else if v > hi { hi } else { v }
}
fn main() {}
project {
    Point  -> "Point.cs"  : csharp,
    Shape  -> "Shape.cs"  : csharp,
    Area   -> "Area.cs"   : csharp,
    MAX    -> "Max.cs"    : csharp,
    clamp  -> "Clamp.cs"  : csharp,
}`);
  const r = run(['emit', path.join(dir, 'cs.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  // 类型项（Point/Shape/Area）顶层声明（C# internal record/abstract record/interface 顶层合法）
  const pt = fs.readFileSync(path.join(dir, 'Point.cs'), 'utf-8');
  assert(/internal record Point\(/.test(pt), `Point 应为顶层 internal record（非嵌进 class）：\n${pt}`);
  assert(!/\bpublic class\b/.test(pt), `Point.cs 不应有 public class（C# public 类名必须匹配文件名）：\n${pt}`);
  const sp = fs.readFileSync(path.join(dir, 'Shape.cs'), 'utf-8');
  assert(/internal abstract record Shape/.test(sp), `Shape 应为顶层 internal abstract record：\n${sp}`);
  // fn/const 包装进 internal static class Dhv<Stem>
  const cl = fs.readFileSync(path.join(dir, 'Clamp.cs'), 'utf-8');
  assert(/internal static class DhvClamp \{/.test(cl), `fn clamp 应包装进 internal static class DhvClamp（C# 顶层函数非法）：\n${cl}`);
  assert(/public static long Clamp\(/.test(cl), `clamp fn 签名应为 public static long Clamp(...)：\n${cl}`);
  assert(cl.includes('throw new NotImplementedException'), `应含 NotImplementedException 未实现标记：\n${cl}`);
  const mx = fs.readFileSync(path.join(dir, 'Max.cs'), 'utf-8');
  assert(/internal static class DhvMax \{/.test(mx), `const MAX 应包装进 internal static class DhvMax：\n${mx}`);
  assert(/internal const long MAX = 100;/.test(mx), `const MAX 签名应正确：\n${mx}`);
  // 同模块多文件宿主类名唯一（防重名冲突）
  const hosts = [cl, mx];
  const hostNames = hosts.map((h) => h.match(/internal static class (\w+) \{/)?.[1] ?? '');
  assert(new Set(hostNames).size === hostNames.length, `同模块多文件宿主类名必须唯一：${hostNames.join(', ')}`);
});

test('emit', 'Kotlin/Swift contract 结构断言（backends-demo 全 5 文件）', () => {
  // 无 kotlin/swift 编译器，仅结构断言（sealed class / enum case 形态）
  const out = path.join(TMP, 'bd-kt-sw');
  const r = run(['emit', 'examples/backends-demo/agent.hsl', '--out', out]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  // Kotlin：sealed class + data class + object 子变体
  const act = fs.readFileSync(path.join(out, 'gen/kotlin/Action.kt'), 'utf-8');
  assert(/^sealed class Action \{/m.test(act), `Action 应为 sealed class：\n${act}`);
  assert(/data class CallTool\(val name: String, val args: Map<String, String>\) : Action\(\)/.test(act), `CallTool 应为 data class : Action()：\n${act}`);
  assert(/object Stop : Action\(\)/.test(act), `Stop 无字段应为 object : Action()：\n${act}`);
  // Kotlin fn：top-level 合法（无需 wrapper）
  const cl = fs.readFileSync(path.join(out, 'gen/kotlin/clamp.kt'), 'utf-8');
  assert(/^fun clamp_turns\(turns: Long, lo: Long, hi: Long\): Long \{/m.test(cl), `clamp_turns 应为 top-level fun（Kotlin 文件级函数合法）：\n${cl}`);
  assert(cl.includes('TODO("'), `clamp_turns 应含 TODO 未实现标记：\n${cl}`);
  // Swift：enum + case + 命名参数
  const asw = fs.readFileSync(path.join(out, 'gen/swift/Action.swift'), 'utf-8');
  assert(/^enum Action \{/m.test(asw), `Action 应为 enum：\n${asw}`);
  assert(/case CallTool\(name: String, args: \[String: String\]\)/.test(asw), `CallTool case 应带命名参数：\n${asw}`);
  assert(/^    case Stop$/m.test(asw), `Stop 无字段应为 case Stop：\n${asw}`);
  // Swift fn：top-level 合法
  const csw = fs.readFileSync(path.join(out, 'gen/swift/clamp.swift'), 'utf-8');
  assert(/^func clamp_turns\(turns: Int64, lo: Int64, hi: Int64\) -> Int64 \{/m.test(csw), `clamp_turns 应为 top-level func：\n${csw}`);
  assert(csw.includes('fatalError('), `应含 fatalError 未实现标记：\n${csw}`);
});

test('emit', '宏 token 树嵌套 delim 类型收集（vec![Tool {...}, ...] 跨文件接线）', () => {
  // Task 21 验证 collectTypeRefs 嵌套 delim 边界：vec![A {...}, B {...}] 中嵌套的 struct 字面量
  // 的类型名（A/B）应被收集并跨文件接线（python from / cpp 内联）
  const dir = path.join(TMP, 'macro-nest');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'mn.hsl'), `struct Tool { name: String }
struct Action { tool: String }

fn build_one() -> Tool {
    let xs = vec![Tool { name: "a" }, Tool { name: "b" }];
    xs.pop().unwrap_or(Tool { name: "default" })
}

fn build_act() -> Action {
    let xs = vec![Action { tool: "x" }, Action { tool: "y" }];
    xs.pop().unwrap_or(Action { tool: "default" })
}

fn main() {}
project {
    build_one -> "gen/python/build_one.py" : python,
    build_act -> "gen/python/build_act.py" : python,
    Tool     -> "gen/python/tool.py"       : python,
    Action   -> "gen/python/action.py"     : python,
}`);
  const r = run(['emit', path.join(dir, 'mn.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  // 零 X-1 告警：vec![...] 嵌套 struct 字面量中的 Tool/Action 应被收集 + 接线
  const manifest = JSON.parse(fs.readFileSync(path.join(dir, 'manifest.json'), 'utf-8'));
  assertEq((manifest.warnings ?? []).length, 0, `应零 X 告警（嵌套 delim 类型收集应工作）：${JSON.stringify(manifest.warnings)}`);
  // python：build_one.py 应 from tool import Tool（vec![Tool {...}] 引用 Tool）
  const bo = fs.readFileSync(path.join(dir, 'gen/python/build_one.py'), 'utf-8');
  assert(bo.includes('from tool import Tool'), `build_one.py 应 from-import Tool（vec![Tool {...}] 嵌套 delim 收集）：\n${bo}`);
  // build_act.py 应 from action import Action（不同类型不同接线）
  const ba = fs.readFileSync(path.join(dir, 'gen/python/build_act.py'), 'utf-8');
  assert(ba.includes('from action import Action'), `build_act.py 应 from-import Action：\n${ba}`);
  assert(!ba.includes('from tool import Tool'), `build_act.py 不应 import Tool（未引用）：\n${ba}`);
});

// ---------------------------------------------------------------------------
// Task 22 新增：String::contains 类型感知修复 + Vec::insert/remove + HashMap 全表面 +
//               Vec::get/HashMap::get Option 语义（cpp/go）+ let 块初始化 + String 方法族 cpp
// ---------------------------------------------------------------------------

test('emit', 'String::contains 类型感知分发（修复 cpp/go 编译错误代码）+ g++ 编译', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'str-contains');
  fs.mkdirSync(dir, { recursive: true });
  // 🔴 v1.4.7 修复实录：此前 cpp 生成 std::find(s.begin(), s.end(), "x")
  // （char 与 const char* 比较 = g++ 编译错误）、go 生成 slices.Contains(s, "x")
  // （string 非切片 = 编译错误）—— 均通过启发式平衡校验但真机编译必炸
  fs.writeFileSync(path.join(dir, 'sc.hsl'), `fn has_x(s: String) -> bool {
    s.contains("x")
}
fn vec_has(v: Vec<i32>, x: i32) -> bool {
    v.contains(x)
}
fn main() {}
project { has_x -> "hx.cpp" : cpp, has_x -> "hx.go" : go, vec_has -> "vh.cpp" : cpp }`);
  const r = run(['emit', path.join(dir, 'sc.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const cpp = fs.readFileSync(path.join(dir, 'hx.cpp'), 'utf-8');
  assert(cpp.includes('s.find("x") != std::string::npos'), `String::contains 应为 find != npos（子串语义）：\n${cpp}`);
  assert(!cpp.includes('std::find(s.begin()'), `不应再生成 Vec 式 std::find（char 与 const char* 比较是编译错误）：\n${cpp}`);
  const go = fs.readFileSync(path.join(dir, 'hx.go'), 'utf-8');
  assert(go.includes('strings.Contains(s, "x")'), `go String::contains 应为 strings.Contains：\n${go}`);
  assert(!go.includes('slices.Contains(s,'), `go 不应再生成 slices.Contains（string 非切片是编译错误）：\n${go}`);
  // Vec::contains 保持 Vec 式（std::find 迭代器）—— 类型感知分流不破坏既有 Vec 路径
  const vc = fs.readFileSync(path.join(dir, 'vh.cpp'), 'utf-8');
  assert(vc.includes('std::find(v.begin(), v.end(), x)'), `Vec::contains 应保持 std::find 迭代器式：\n${vc}`);
  // g++ 编译 + 链接 + 运行语义验证
  execFileSync('g++', ['-std=c++23', '-c', path.join(dir, 'hx.cpp'), '-o', path.join(dir, 'hx.o')], { timeout: 60_000 });
  execFileSync('g++', ['-std=c++23', '-c', path.join(dir, 'vh.cpp'), '-o', path.join(dir, 'vh.o')], { timeout: 60_000 });
  const main = `#include <cstdint>
#include <vector>
#include <iostream>
#include <format>
bool has_x(std::string s);
bool vec_has(std::vector<int32_t> v, int32_t x);
int main() {
    std::cout << std::format("{} {} {} {}\\n", has_x("oxo") ? 1 : 0, has_x("ab") ? 1 : 0,
        vec_has({1, 2, 3}, 2) ? 1 : 0, vec_has({1, 2, 3}, 9) ? 1 : 0);
    return 0;
}`;
  fs.writeFileSync(path.join(dir, 'main.cpp'), main);
  execFileSync('g++', ['-std=c++23', path.join(dir, 'main.cpp'), path.join(dir, 'hx.o'), path.join(dir, 'vh.o'), '-o', path.join(dir, 'sc-test')], { timeout: 60_000 });
  const stdout = execFileSync(path.join(dir, 'sc-test'), { encoding: 'utf-8', timeout: 30_000 });
  assertEq(stdout.trim(), '1 0 1 0', `contains 语义应与 interp 对齐：${stdout}`);
});

test('emit', 'cpp Vec::insert/Vec::remove + HashMap 全表面（g++ 编译+链接+运行语义级）', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'vecmap-cpp');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'vm.hsl'), `fn insert_and_remove(mut v: Vec<i32>) -> i32 {
    v.insert(1, 99);
    let removed = v.remove(0);
    removed
}
fn map_ops(mut m: HashMap<String, i32>) -> i32 {
    m.insert("a", 1);
    m.insert("b", 2);
    let has_a = m.contains_key("a");
    let nk = m.keys().len();
    let nv = m.values().len();
    let got = m.get("a").unwrap_or(-1);
    let old = m.remove("b").unwrap_or(-5);
    let base = if has_a { 100 } else { 0 };
    base + nk + nv + got + old
}
fn main() {}
project {
    insert_and_remove -> "ir.cpp" : cpp,
    map_ops           -> "mo.cpp" : cpp,
}`);
  const r = run(['emit', path.join(dir, 'vm.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  // 结构断言：Vec::insert/remove 走助手（返回元素非 iterator）
  const ir = fs.readFileSync(path.join(dir, 'ir.cpp'), 'utf-8');
  assert(ir.includes('_dhvInsert(v, 1, 99)'), `Vec::insert 应为 _dhvInsert 助手：\n${ir}`);
  assert(ir.includes('_dhvRemoveAt(v, 0)'), `Vec::remove 应为 _dhvRemoveAt 助手：\n${ir}`);
  assert(ir.includes('template <typename T> void _dhvInsert'), `prelude 应定义 _dhvInsert：\n${ir}`);
  assert(ir.includes('template <typename T> T _dhvRemoveAt'), `prelude 应定义 _dhvRemoveAt：\n${ir}`);
  // 结构断言：HashMap 全表面（insert/contains_key/keys/values/get/remove）
  const mo = fs.readFileSync(path.join(dir, 'mo.cpp'), 'utf-8');
  assert(mo.includes('m["a"] = 1;'), `HashMap::insert 应为 m[k] = v：\n${mo}`);
  assert(mo.includes('m.find("a") != m.end()'), `contains_key 应为 find != end：\n${mo}`);
  assert(mo.includes('_dhvKeys(m)'), `keys 应为 _dhvKeys 助手：\n${mo}`);
  assert(mo.includes('_dhvValues(m)'), `values 应为 _dhvValues 助手：\n${mo}`);
  assert(mo.includes('_dhvMapGet(m, "a")'), `get 应为 _dhvMapGet（Option 语义）：\n${mo}`);
  assert(mo.includes('_dhvMapRemove(m, "b")'), `remove 应为 _dhvMapRemove（Option 语义）：\n${mo}`);
  assert(!mo.includes('未翻译'), `map_ops 应为活体：\n${mo}`);
  // g++ 编译 + 链接 + 运行（与 interp 实测对齐：ir=10 / mo=107）
  execFileSync('g++', ['-std=c++23', '-c', path.join(dir, 'ir.cpp'), '-o', path.join(dir, 'ir.o')], { timeout: 60_000 });
  execFileSync('g++', ['-std=c++23', '-c', path.join(dir, 'mo.cpp'), '-o', path.join(dir, 'mo.o')], { timeout: 60_000 });
  const main = `#include <cstdint>
#include <string>
#include <vector>
#include <unordered_map>
#include <iostream>
#include <format>
int32_t insert_and_remove(std::vector<int32_t> v);
int32_t map_ops(std::unordered_map<std::string, int32_t> m);
int main() {
    std::cout << std::format("{}\\n", insert_and_remove({10, 20, 30}));
    std::unordered_map<std::string, int32_t> m;
    std::cout << std::format("{}\\n", map_ops(m));
    return 0;
}`;
  fs.writeFileSync(path.join(dir, 'main.cpp'), main);
  execFileSync('g++', ['-std=c++23', path.join(dir, 'main.cpp'), path.join(dir, 'ir.o'), path.join(dir, 'mo.o'), '-o', path.join(dir, 'vm-test')], { timeout: 60_000 });
  const stdout = execFileSync(path.join(dir, 'vm-test'), { encoding: 'utf-8', timeout: 30_000 });
  const lines = stdout.trim().split('\n');
  // [10,20,30]: insert(1,99) → [10,99,20,30]; remove(0) → 10
  assertEq(lines[0], '10', `insert_and_remove([10,20,30])=10：${stdout}`);
  // map: 100(has) + 2(keys) + 2(values) + 1(get a) + 2(remove b) = 107
  assertEq(lines[1], '107', `map_ops=107：${stdout}`);
});

test('emit', 'cpp/go Vec::get / HashMap::get Option 语义（越界/缺键 → 默认值）', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'get-opt');
  fs.mkdirSync(dir, { recursive: true });
  // v1.4.7：关闭 v1.4.3 遗留的「下标近似」—— get 走类型感知 Option 助手
  fs.writeFileSync(path.join(dir, 'go.hsl'), `fn vec_get(v: Vec<i32>, i: i32) -> i32 {
    v.get(i).unwrap_or(-1)
}
fn map_get(m: HashMap<String, i32>, k: String) -> i32 {
    m.get(k).unwrap_or(-7)
}
fn main() {}
project {
    vec_get -> "vg.cpp" : cpp,
    map_get -> "mg.cpp" : cpp,
    vec_get -> "vg.go"  : go,
    map_get -> "mg.go"  : go,
}`);
  const r = run(['emit', path.join(dir, 'go.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const vg = fs.readFileSync(path.join(dir, 'vg.cpp'), 'utf-8');
  assert(vg.includes('_dhvVecGet(v, i)'), `Vec::get 应为 _dhvVecGet（Option 助手，非下标近似）：\n${vg}`);
  // 函数体（跳过 prelude 助手定义）不应有裸下标 get —— 助手内部的 v[i] 是合法的带边界检查访问
  const vgBody = vg.split('int32_t vec_get')[1] ?? '';
  assert(!vgBody.includes('v[i]'), `函数体不应有裸下标近似（越界 UB 语义漂移）：\n${vgBody}`);
  const mg = fs.readFileSync(path.join(dir, 'mg.cpp'), 'utf-8');
  assert(mg.includes('_dhvMapGet(m, k)'), `HashMap::get 应为 _dhvMapGet：\n${mg}`);
  // go 结构断言
  const vgg = fs.readFileSync(path.join(dir, 'vg.go'), 'utf-8');
  assert(vgg.includes('_dhvVecGet(v, i)'), `go Vec::get 应为 _dhvVecGet：\n${vgg}`);
  assert(vgg.includes('func _dhvVecGet[T any]'), `go prelude 应定义 _dhvVecGet：\n${vgg}`);
  const mgg = fs.readFileSync(path.join(dir, 'mg.go'), 'utf-8');
  assert(mgg.includes('_dhvMapGet(m, k)'), `go HashMap::get 应为 _dhvMapGet：\n${mgg}`);
  // g++ 编译 + 链接 + 运行：OOB/缺键 → 默认值（与 interp 对齐）
  execFileSync('g++', ['-std=c++23', '-c', path.join(dir, 'vg.cpp'), '-o', path.join(dir, 'vg.o')], { timeout: 60_000 });
  execFileSync('g++', ['-std=c++23', '-c', path.join(dir, 'mg.cpp'), '-o', path.join(dir, 'mg.o')], { timeout: 60_000 });
  const main = `#include <cstdint>
#include <string>
#include <vector>
#include <unordered_map>
#include <iostream>
#include <format>
int32_t vec_get(std::vector<int32_t> v, int32_t i);
int32_t map_get(std::unordered_map<std::string, int32_t> m, std::string k);
int main() {
    std::vector<int32_t> v{5, 6, 7};
    std::unordered_map<std::string, int32_t> m{{"a", 1}};
    std::cout << std::format("{} {} {} {} {}\\n",
        vec_get(v, 1), vec_get(v, 9), vec_get(v, -1), map_get(m, "a"), map_get(m, "zz"));
    return 0;
}`;
  fs.writeFileSync(path.join(dir, 'main.cpp'), main);
  execFileSync('g++', ['-std=c++23', path.join(dir, 'main.cpp'), path.join(dir, 'vg.o'), path.join(dir, 'mg.o'), '-o', path.join(dir, 'go-test')], { timeout: 60_000 });
  const stdout = execFileSync(path.join(dir, 'go-test'), { encoding: 'utf-8', timeout: 30_000 });
  assertEq(stdout.trim(), '6 -1 -1 1 -7', `get Option 语义应与 interp 对齐（OOB/缺键 → 默认值）：${stdout}`);
});

test('emit', 'let 块初始化（let x = if/match/if-let）—— 声明 + 分支尾赋值模式', () => {
  const dir = path.join(TMP, 'let-block');
  fs.mkdirSync(dir, { recursive: true });
  // 🔴 v1.4.7 修复：此前全部 7 语言回退 contract（expr() 遇块直接 throw；interp 却支持）
  fs.writeFileSync(path.join(dir, 'lb.hsl'), `fn pick(b: bool) -> i32 {
    let base = if b { 100 } else { 0 };
    base
}
fn score(b: bool) -> i32 {
    let s = match b { true => 10, false => 20 };
    s
}
fn opt_val(o: Option<i32>) -> i32 {
    let v = if let Some(x) = o { x } else { -1 };
    v
}
fn main() {}
project {
    pick    -> "pick.py"    : python,
    pick    -> "pick.rs"    : rust,
    pick    -> "pick.ts"    : typescript,
    score   -> "score.py"   : python,
    score   -> "score.cpp"  : cpp,
    opt_val -> "ov.py"      : python,
    opt_val -> "ov.cpp"     : cpp,
}`);
  const r = run(['emit', path.join(dir, 'lb.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  // 全部活体（此前回退 contract）
  for (const f of ['pick.py', 'pick.rs', 'pick.ts', 'score.py', 'score.cpp', 'ov.py', 'ov.cpp']) {
    const src = fs.readFileSync(path.join(dir, f), 'utf-8');
    assert(!src.includes('未翻译'), `${f} 应为活体（let 块初始化）：\n${src}`);
    assert(src.includes('(live)'), `${f} 应带 (live) 标记：\n${src}`);
  }
  // python：分支内赋值（免声明）
  const py = fs.readFileSync(path.join(dir, 'pick.py'), 'utf-8');
  assert(/if b:\n\s+base = 100\n\s+else:\n\s+base = 0/.test(py), `python 应分支内赋值：\n${py}`);
  // rust：let 延迟初始化 + 分支赋值
  const rs = fs.readFileSync(path.join(dir, 'pick.rs'), 'utf-8');
  assert(/let base;/.test(rs), `rust 应延迟初始化声明 let base;：\n${rs}`);
  assert(/base = 100;/.test(rs), `rust 分支尾应赋值：\n${rs}`);
  // cpp：无注解时按分支值推导 int64_t 声明
  const cpp = fs.readFileSync(path.join(dir, 'score.cpp'), 'utf-8');
  assert(/int64_t s;/.test(cpp), `cpp 无注解 let 块初始化应推导 int64_t s;：\n${cpp}`);
  // python 语义级验证（pick/score/opt_val 与 interp 对齐）
  const runner = `import sys; sys.path.insert(0, ${JSON.stringify(dir)})
from pick import pick
from score import score
from ov import opt_val
assert pick(True) == 100 and pick(False) == 0, (pick(True), pick(False))
assert score(True) == 10 and score(False) == 20, (score(True), score(False))
assert opt_val(42) == 42 and opt_val(None) == -1, (opt_val(42), opt_val(None))
print('py-letblock-ok')
`;
  fs.writeFileSync(path.join(dir, 'verify.py'), runner);
  const stdout = execFileSync('python3', [path.join(dir, 'verify.py')], { encoding: 'utf-8', timeout: 30_000 });
  assert(stdout.includes('py-letblock-ok'), `python let 块初始化语义验证异常：${stdout}`);
});

test('emit', 'cpp String 方法族（trim/lower/upper/starts/ends/replace/split/splitWS/lines/repeat/char_count/join）', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'str-cpp');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'sm.hsl'), `fn str_ops(s: String) -> String {
    let t = s.trim();
    let l = t.to_lowercase();
    let r = l.replace("world", "hsl");
    r
}
fn str_checks(s: String) -> i32 {
    let a = if s.starts_with("he") { 1 } else { 0 };
    let b = if s.ends_with("ld") { 1 } else { 0 };
    let c = if s.contains("o w") { 1 } else { 0 };
    a + b + c
}
fn str_split(s: String) -> i32 {
    let parts = s.split(",");
    parts.len()
}
fn str_ws(s: String) -> i32 {
    let words = s.split_whitespace();
    words.len()
}
fn str_count(s: String) -> i32 {
    let n = s.char_count();
    let doubled = s.repeat(2);
    let dlen = doubled.char_count();
    n + dlen
}
fn str_lines(s: String) -> i32 {
    let ls = s.lines();
    ls.len()
}
fn str_join(v: Vec<String>) -> String {
    v.join("-")
}
fn main() {}
project {
    str_ops    -> "so.cpp"  : cpp,
    str_checks -> "sc.cpp"  : cpp,
    str_split  -> "ss.cpp"  : cpp,
    str_ws     -> "sw.cpp"  : cpp,
    str_count  -> "sct.cpp" : cpp,
    str_lines  -> "sl.cpp"  : cpp,
    str_join   -> "sj.cpp"  : cpp,
}`);
  const r = run(['emit', path.join(dir, 'sm.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  // 结构断言：助手映射（C++ 标准库无这些便捷函数）
  const so = fs.readFileSync(path.join(dir, 'so.cpp'), 'utf-8');
  assert(so.includes('_dhvTrim(s)'), `trim 应为 _dhvTrim 助手：\n${so}`);
  assert(so.includes('_dhvToLower('), `to_lowercase 应为 _dhvToLower：\n${so}`);
  assert(so.includes('_dhvReplaceAll('), `replace 应为 _dhvReplaceAll（std::string::replace 是下标式）：\n${so}`);
  const sc = fs.readFileSync(path.join(dir, 'sc.cpp'), 'utf-8');
  assert(sc.includes('s.starts_with("he")'), `starts_with 应为 C++20 starts_with：\n${sc}`);
  assert(sc.includes('s.ends_with("ld")'), `ends_with 应为 C++20 ends_with：\n${sc}`);
  const ss = fs.readFileSync(path.join(dir, 'ss.cpp'), 'utf-8');
  assert(ss.includes('_dhvSplit(s, ",")'), `split 应为 _dhvSplit 助手：\n${ss}`);
  const sw = fs.readFileSync(path.join(dir, 'sw.cpp'), 'utf-8');
  assert(sw.includes('_dhvSplitWS(s)'), `split_whitespace 应为 _dhvSplitWS：\n${sw}`);
  const sct = fs.readFileSync(path.join(dir, 'sct.cpp'), 'utf-8');
  assert(sct.includes('_dhvCharCount(s)'), `char_count 应为 _dhvCharCount（UTF-8 码点计数）：\n${sct}`);
  assert(sct.includes('_dhvRepeat(s, 2)'), `repeat 应为 _dhvRepeat：\n${sct}`);
  const sl = fs.readFileSync(path.join(dir, 'sl.cpp'), 'utf-8');
  assert(sl.includes('_dhvSplit(s, "\\n")'), `lines 应为 _dhvSplit(s, "\\n")（py 语义对齐）：\n${sl}`);
  const sj = fs.readFileSync(path.join(dir, 'sj.cpp'), 'utf-8');
  assert(sj.includes('_dhvJoin(v, "-")'), `join 应为 _dhvJoin（if constexpr string/数值分发）：\n${sj}`);
  // g++ 编译全部 + 链接运行语义（与 interp 实测逐字对齐）
  const objs: string[] = [];
  for (const f of ['so', 'sc', 'ss', 'sw', 'sct', 'sl', 'sj']) {
    const o = path.join(dir, `${f}.o`);
    execFileSync('g++', ['-std=c++23', '-c', path.join(dir, `${f}.cpp`), '-o', o], { timeout: 60_000 });
    objs.push(o);
  }
  const main = `#include <cstdint>
#include <string>
#include <vector>
#include <iostream>
#include <format>
std::string str_ops(std::string s);
int32_t str_checks(std::string s);
int32_t str_split(std::string s);
int32_t str_ws(std::string s);
int32_t str_count(std::string s);
int32_t str_lines(std::string s);
std::string str_join(std::vector<std::string> v);
int main() {
    std::cout << std::format("{}|{}|{}|{}|{}|{}|{}\\n",
        str_ops("  Hello world  "), str_checks("hello world"), str_split("a,b,c"),
        str_ws("  alpha beta  gamma "), str_count("abc"), str_lines("x\\ny\\n"), str_join({"a", "b", "c"}));
    return 0;
}`;
  fs.writeFileSync(path.join(dir, 'main.cpp'), main);
  execFileSync('g++', ['-std=c++23', path.join(dir, 'main.cpp'), ...objs, '-o', path.join(dir, 'sm-test')], { timeout: 120_000 });
  const stdout = execFileSync(path.join(dir, 'sm-test'), { encoding: 'utf-8', timeout: 30_000 });
  assertEq(stdout.trim(), 'hello hsl|3|3|3|9|3|a-b-c', `String 方法族语义应与 interp 逐字对齐：${stdout}`);
});

test('emit', 'go HashMap 助手族结构断言（keys/values/get/remove + Vec insert/remove/get）', () => {
  const dir = path.join(TMP, 'go-helpers');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'gh.hsl'), `fn map_ops(mut m: HashMap<String, i64>) -> i64 {
    m.insert("a", 1);
    let nk = m.keys().len();
    let nv = m.values().len();
    let got = m.get("a").unwrap_or(-1);
    let old = m.remove("a").unwrap_or(-5);
    nk + nv + got + old
}
fn vec_ops(mut v: Vec<i64>) -> i64 {
    v.insert(0, 9);
    let r = v.remove(0);
    let g = v.get(0).unwrap_or(-1);
    r + g
}
fn main() {}
project {
    map_ops -> "mo.go" : go,
    vec_ops -> "vo.go" : go,
}`);
  const r = run(['emit', path.join(dir, 'gh.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const mo = fs.readFileSync(path.join(dir, 'mo.go'), 'utf-8');
  assert(!mo.includes('未翻译'), `map_ops.go 应为活体：\n${mo}`);
  // go 助手族结构断言（泛型 + 指针 Option 表示）—— v1.4.10：同 package 多文件助手去重，
  // mo.go 为首个 go 文件（含全部助手定义）；vo.go 仅含 import + 调用点
  assert(mo.includes('func _dhvKeys[K comparable, V any]'), `prelude 应定义 _dhvKeys 泛型助手：\n${mo}`);
  assert(mo.includes('func _dhvValues[K comparable, V any]'), `prelude 应定义 _dhvValues：\n${mo}`);
  assert(mo.includes('func _dhvMapGet[K comparable, V any]'), `prelude 应定义 _dhvMapGet：\n${mo}`);
  assert(mo.includes('func _dhvMapRemove[K comparable, V any]'), `prelude 应定义 _dhvMapRemove：\n${mo}`);
  assert(mo.includes('func _dhvInsert[T any]'), `prelude 应定义 _dhvInsert：\n${mo}`);
  assert(mo.includes('func _dhvRemoveAt[T any]'), `prelude 应定义 _dhvRemoveAt：\n${mo}`);
  assert(mo.includes('func _dhvVecGet[T any]'), `prelude 应定义 _dhvVecGet：\n${mo}`);
  assert(mo.includes('_dhvKeys(m)'), `keys 应调用 _dhvKeys：\n${mo}`);
  assert(mo.includes('_dhvMapRemove(m, "a")'), `remove 应调用 _dhvMapRemove（*V 与 Option 指针一致，可链式 unwrap_or）：\n${mo}`);
  // 🔴 v1.4.7 修复断言：旧版匿名函数返回 any —— 链式 .unwrap_or(d) 解引用 any 是编译错误
  assert(!mo.includes('func() any'), `不应再使用匿名函数 any 表示（链式解引用编译错误）：\n${mo}`);
  const vo = fs.readFileSync(path.join(dir, 'vo.go'), 'utf-8');
  assert(vo.includes('_dhvInsert(&v, 0, 9)'), `go Vec::insert 应为 _dhvInsert(&v, ...)：\n${vo}`);
  assert(vo.includes('_dhvRemoveAt(&v, 0)'), `go Vec::remove 应为 _dhvRemoveAt(&v, ...)：\n${vo}`);
  assert(vo.includes('_dhvVecGet(v, 0)'), `go Vec::get 应为 _dhvVecGet：\n${vo}`);
  // v1.4.10：非首文件不再重复声明助手（import 按需裁剪后仅含实际引用的包）
  assert(!vo.includes('func _dhvInsert[T any]'), `非首 go 文件不应重复声明助手（同 package 去重）：\n${vo}`);
  // v1.4.10 新增：真机 go build 编译级验证（工具链可用时）
  const GO = goBin();
  if (GO) {
    const bdir = path.join(dir, 'gobuild');
    fs.mkdirSync(bdir, { recursive: true });
    fs.copyFileSync(path.join(dir, 'mo.go'), path.join(bdir, 'mo.go'));
    fs.copyFileSync(path.join(dir, 'vo.go'), path.join(bdir, 'vo.go'));
    execFileSync(GO, ['mod', 'init', 'gh'], { cwd: bdir, timeout: 30_000, stdio: 'pipe' });
    execFileSync(GO, ['build', './...'], { cwd: bdir, timeout: 120_000, stdio: 'pipe' });
  }
});

// ===========================================================================
// v1.4.8 新增测试（88→96）：Option 链式家族 + cpp/go Vec sort/clear/extend/append + 闭包翻译 + 数组字面量修复
// ===========================================================================

test('emit', 'cpp Option::map/and_then 链式家族（g++ 编译+运行语义级）', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'cpp-optchain');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'opt.hsl'), `fn opt_map(x: i64) -> i64 {
    let a: Option<i64> = Option::Some(x);
    let b = a.map(|v| v * 2);
    let r = b.unwrap_or(0);
    r
}
fn opt_and_then(x: i64) -> i64 {
    let a: Option<i64> = Option::Some(x);
    let b = a.and_then(|v| Option::Some(v + 1));
    b.unwrap_or(0)
}
fn opt_chain_none() -> i64 {
    let a: Option<i64> = Option::None;
    let b = a.map(|v| v * 2);
    b.unwrap_or(-1)
}
fn main() {}
project {
    opt_map -> "opt_map.cpp" : cpp,
    opt_and_then -> "opt_and_then.cpp" : cpp,
    opt_chain_none -> "opt_chain_none.cpp" : cpp
}`);
  const r = run(['emit', path.join(dir, 'opt.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  for (const fn of ['opt_map', 'opt_and_then', 'opt_chain_none']) {
    const cpp = fs.readFileSync(path.join(dir, fn + '.cpp'), 'utf-8');
    assert(!cpp.includes('未翻译'), `${fn}.cpp 应为活体（非 contract 回退）：\n${cpp}`);
    assert(cpp.includes('#ifndef DHV_OPT_HELPER'), `prelude 应定义 DHV_OPT_HELPER include guard：\n${cpp}`);
    assert(cpp.includes('template <typename T, typename F>'), `prelude 应含 _dhvOptMap 模板助手：\n${cpp}`);
  }
  // 结构断言：map → _dhvOptMap；and_then → _dhvOptAndThen；闭包 → [&] (auto _x) { return ...; }
  const om = fs.readFileSync(path.join(dir, 'opt_map.cpp'), 'utf-8');
  assert(om.includes('_dhvOptMap(a,'), `Option::map 应映射 _dhvOptMap：\n${om}`);
  assert(/_dhvOptMap\(a,\s*\[&\]\s*\(auto\s+\w+\)\s*\{\s*return/.test(om), `闭包应翻译为 cpp lambda [&] (auto x) { return ...; }：\n${om}`);
  const oan = fs.readFileSync(path.join(dir, 'opt_and_then.cpp'), 'utf-8');
  assert(oan.includes('_dhvOptAndThen(a,'), `Option::and_then 应映射 _dhvOptAndThen（recv=a）：\n${oan}`);
  assert(oan.includes('auto _dhvSome(') || oan.includes('_dhvSome(('), `and_then 闭包内 Option::Some 应映射 _dhvSome：\n${oan}`);
  const on = fs.readFileSync(path.join(dir, 'opt_chain_none.cpp'), 'utf-8');
  assert(on.includes('_dhvOptMap(a,'), `None.map() 也应映射 _dhvOptMap：\n${on}`);
  assert(on.includes('.value_or('), `unwrap_or 应映射 value_or：\n${on}`);
  // 编译 + 链接 + 运行（语义对齐 interp）
  const main = `#include <cstdint>
#include <iostream>
#include <format>
int64_t opt_map(int64_t x);
int64_t opt_and_then(int64_t x);
int64_t opt_chain_none();
int main() {
    std::cout << std::format("{}\\n", opt_map(5));
    std::cout << std::format("{}\\n", opt_and_then(7));
    std::cout << std::format("{}\\n", opt_chain_none());
    return 0;
}`;
  fs.writeFileSync(path.join(dir, 'main.cpp'), main);
  execFileSync('g++', ['-std=c++23', path.join(dir, 'main.cpp'), path.join(dir, 'opt_map.cpp'), path.join(dir, 'opt_and_then.cpp'), path.join(dir, 'opt_chain_none.cpp'), '-o', path.join(dir, 'opt-test')], { timeout: 60_000 });
  const stdout = execFileSync(path.join(dir, 'opt-test'), { encoding: 'utf-8', timeout: 30_000 });
  const lines = stdout.trim().split('\n');
  assertEq(lines[0], '10', `opt_map(5)=5*2=10：${stdout}`);
  assertEq(lines[1], '8', `opt_and_then(7)=7+1=8：${stdout}`);
  assertEq(lines[2], '-1', `opt_chain_none(None).map().unwrap_or(-1)=-1：${stdout}`);
});

test('emit', 'cpp Option::or/unwrap_or_else/expect（g++ 编译+运行语义级）', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'cpp-optmisc');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'opt.hsl'), `fn opt_or_some() -> i64 {
    let a: Option<i64> = Option::Some(42);
    let b: Option<i64> = Option::None;
    let c = a.or(b);
    c.expect(String::from("必须存在"))
}
fn opt_or_none() -> i64 {
    let a: Option<i64> = Option::None;
    let b: Option<i64> = Option::Some(99);
    let c = a.or(b);
    c.unwrap_or(0)
}
fn opt_unwrap_or_else() -> i64 {
    let a: Option<i64> = Option::None;
    a.unwrap_or_else(|| 7)
}
fn main() {}
project {
    opt_or_some -> "or_some.cpp" : cpp,
    opt_or_none -> "or_none.cpp" : cpp,
    opt_unwrap_or_else -> "uoe.cpp" : cpp
}`);
  const r = run(['emit', path.join(dir, 'opt.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  for (const f of ['or_some', 'or_none', 'uoe']) {
    const cpp = fs.readFileSync(path.join(dir, f + '.cpp'), 'utf-8');
    assert(!cpp.includes('未翻译'), `${f}.cpp 应为活体：\n${cpp}`);
  }
  const os = fs.readFileSync(path.join(dir, 'or_some.cpp'), 'utf-8');
  assert(os.includes('_dhvOptOr('), `Option::or 应映射 _dhvOptOr：\n${os}`);
  assert(os.includes('_dhvOptExpect('), `Option::expect 应映射 _dhvOptExpect：\n${os}`);
  const on = fs.readFileSync(path.join(dir, 'or_none.cpp'), 'utf-8');
  assert(on.includes('_dhvOptOr('), `Option::or 应映射 _dhvOptOr：\n${on}`);
  assert(on.includes('.value_or('), `unwrap_or 应映射 value_or：\n${on}`);
  const uoe = fs.readFileSync(path.join(dir, 'uoe.cpp'), 'utf-8');
  assert(uoe.includes('_dhvOptUnwrapOrElse('), `unwrap_or_else 应映射 _dhvOptUnwrapOrElse：\n${uoe}`);
  assert(/_dhvOptUnwrapOrElse\(a,\s*\[&\]\s*\(\)\s*\{\s*return\s+7;\s*\}\)/.test(uoe), `零参 lambda 应翻译为 [&] () { return 7; }：\n${uoe}`);
  const main = `#include <cstdint>
#include <iostream>
#include <format>
int64_t opt_or_some();
int64_t opt_or_none();
int64_t opt_unwrap_or_else();
int main() {
    std::cout << std::format("{}\\n", opt_or_some());
    std::cout << std::format("{}\\n", opt_or_none());
    std::cout << std::format("{}\\n", opt_unwrap_or_else());
    return 0;
}`;
  fs.writeFileSync(path.join(dir, 'main.cpp'), main);
  execFileSync('g++', ['-std=c++23', path.join(dir, 'main.cpp'), path.join(dir, 'or_some.cpp'), path.join(dir, 'or_none.cpp'), path.join(dir, 'uoe.cpp'), '-o', path.join(dir, 'opt-test')], { timeout: 60_000 });
  const stdout = execFileSync(path.join(dir, 'opt-test'), { encoding: 'utf-8', timeout: 30_000 });
  const lines = stdout.trim().split('\n');
  assertEq(lines[0], '42', `opt_or_some: Some(42).or(None).expect = 42：${stdout}`);
  assertEq(lines[1], '99', `opt_or_none: None.or(Some(99)).unwrap_or(0) = 99：${stdout}`);
  assertEq(lines[2], '7', `opt_unwrap_or_else: None.unwrap_or_else(|| 7) = 7：${stdout}`);
});

test('emit', 'go Option::or/expect 助手族结构断言（非闭包方法）', () => {
  const dir = path.join(TMP, 'go-opt');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'opt.hsl'), `fn opt_or_go(x: i64) -> i64 {
    let a: Option<i64> = Option::Some(x);
    let b: Option<i64> = Option::None;
    let c = a.or(b);
    c.expect(String::from("must exist"))
}
fn main() {}
project { opt_or_go -> "or.go" : go }`);
  const r = run(['emit', path.join(dir, 'opt.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const go = fs.readFileSync(path.join(dir, 'or.go'), 'utf-8');
  assert(!go.includes('未翻译'), `or.go 应为活体：\n${go}`);
  assert(go.includes('func _dhvOptOr[T any]'), `prelude 应定义 _dhvOptOr 泛型助手：\n${go}`);
  assert(go.includes('func _dhvOptExpect[T any]'), `prelude 应定义 _dhvOptExpect 泛型助手：\n${go}`);
  assert(go.includes('_dhvOptOr('), `Option::or 应调用 _dhvOptOr：\n${go}`);
  assert(go.includes('_dhvOptExpect('), `Option::expect 应调用 _dhvOptExpect：\n${go}`);
  // 🔴 闭包方法不应映射 go（HSL 闭包无类型注解，go func literal 需显式类型）
  assert(!go.includes('map(') || go.includes('_dhvOptMap(') === false, `go 不应误映射 Option::map/and_then/unwrap_or_else（缺类型推导）：\n${go}`);
});

test('emit', 'cpp Vec::sort/is_sorted/clear/extend/append（g++ 编译+运行语义级）', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'cpp-vecsort');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'vec.hsl'), `fn sort_check(v: Vec<i32>) -> i32 {
    let mut w = v;
    w.sort();
    let sorted_ok = w.is_sorted();
    let first = w.get(0).unwrap_or(-1);
    let last = w.get(w.len() - 1).unwrap_or(-1);
    if sorted_ok { first * 100 + last } else { -999 }
}
fn extend_append_clear() -> i32 {
    let mut a = vec![1, 2];
    let b = vec![3, 4];
    a.extend(b);
    a.append(vec![5]);
    let total = a.len();
    total
}
fn clear_zero(v: Vec<i32>) -> i32 {
    let mut w = v;
    w.clear();
    w.len()
}
fn main() {}
project {
    sort_check -> "sort.cpp" : cpp,
    extend_append_clear -> "ext.cpp" : cpp,
    clear_zero -> "clr.cpp" : cpp
}`);
  const r = run(['emit', path.join(dir, 'vec.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const sc = fs.readFileSync(path.join(dir, 'sort.cpp'), 'utf-8');
  assert(sc.includes('std::sort(w.begin(), w.end())'), `cpp sort 应映射 std::sort：\n${sc}`);
  assert(sc.includes('std::is_sorted(w.begin(), w.end())'), `cpp is_sorted 应映射 std::is_sorted：\n${sc}`);
  assert(sc.includes('_dhvVecGet(w,'), `cpp get 应映射 _dhvVecGet（v1.4.7 Option 语义）：\n${sc}`);
  const ex = fs.readFileSync(path.join(dir, 'ext.cpp'), 'utf-8');
  assert(ex.includes('_dhvExtend(a, b)'), `cpp extend 应映射 _dhvExtend 助手（recv=a，arg=b）：\n${ex}`);
  assert(ex.includes('_dhvExtend(a, std::vector{5})'), `cpp append（接收 iterable）应映射 _dhvExtend（与 extend 同语义）：\n${ex}`);
  assert(ex.includes('template <typename T> void _dhvExtend'), `prelude 应定义 _dhvExtend 模板助手（const ref 绑定临时防 length_error）：\n${ex}`);
  assert(ex.includes('std::vector{1, 2}'), `vec! 宏应映射 std::vector{...}（v1.4.8 CTAD 修复）：\n${ex}`);
  // 🔴 v1.4.8 修复断言：vec![1, 2] 旧版生成 [1, 2] 是 lambda 捕获语法（编译必炸）
  assert(!ex.match(/=\s*\[\d/), `不应再生成 [N, M] 数组字面量（lambda 捕获语法非法）：\n${ex}`);
  const cl = fs.readFileSync(path.join(dir, 'clr.cpp'), 'utf-8');
  assert(cl.includes('w.clear()'), `cpp clear 应映射 std::vector::clear：\n${cl}`);
  // 编译 + 链接 + 运行
  const main = `#include <cstdint>
#include <iostream>
#include <format>
#include <vector>
int32_t sort_check(std::vector<int32_t> v);
int32_t extend_append_clear();
int32_t clear_zero(std::vector<int32_t> v);
int main() {
    std::cout << std::format("{}\\n", sort_check({3,1,2,5,4}));
    std::cout << std::format("{}\\n", extend_append_clear());
    std::cout << std::format("{}\\n", clear_zero({1,2,3}));
    return 0;
}`;
  fs.writeFileSync(path.join(dir, 'main.cpp'), main);
  execFileSync('g++', ['-std=c++23', path.join(dir, 'main.cpp'), path.join(dir, 'sort.cpp'), path.join(dir, 'ext.cpp'), path.join(dir, 'clr.cpp'), '-o', path.join(dir, 'vec-test')], { timeout: 60_000 });
  const stdout = execFileSync(path.join(dir, 'vec-test'), { encoding: 'utf-8', timeout: 30_000 });
  const lines = stdout.trim().split('\n');
  assertEq(lines[0], '105', `sort_check({3,1,2,5,4}) → sorted {1,2,3,4,5} → first=1*100+last=5=105：${stdout}`);
  assertEq(lines[1], '5', `extend_append_clear: [1,2]+[3,4]+5 = 5 元素：${stdout}`);
  assertEq(lines[2], '0', `clear_zero: clear 后 len=0：${stdout}`);
});

test('emit', 'go Vec::sort/is_sorted/clear/extend/append 结构断言', () => {
  const dir = path.join(TMP, 'go-vecsort');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'vec.hsl'), `fn sort_check(mut v: Vec<i32>) -> i32 {
    v.sort();
    let sorted_ok = v.is_sorted();
    if sorted_ok { 1 } else { 0 }
}
fn extend_append(mut v: Vec<i32>) -> i32 {
    let b = vec![7, 8];
    v.extend(b);
    v.append(vec![9]);
    v.len()
}
fn clear_v(mut v: Vec<i32>) -> i32 {
    v.clear();
    v.len()
}
fn main() {}
project {
    sort_check -> "sort.go" : go,
    extend_append -> "ext.go" : go,
    clear_v -> "clr.go" : go
}`);
  const r = run(['emit', path.join(dir, 'vec.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const sg = fs.readFileSync(path.join(dir, 'sort.go'), 'utf-8');
  assert(sg.includes('slices.Sort(v)'), `go sort 应映射 slices.Sort：\n${sg}`);
  assert(sg.includes('slices.IsSorted(v)'), `go is_sorted 应映射 slices.IsSorted：\n${sg}`);
  const eg = fs.readFileSync(path.join(dir, 'ext.go'), 'utf-8');
  assert(eg.includes('v = append(v, (b)...)'), `go extend 应映射 append + ...：\n${eg}`);
  assert(eg.includes('v = append(v, ([]any{9})...)'), `go append（接收 iterable）应映射 append + ...（与 extend 同语义）：\n${eg}`);
  const cg = fs.readFileSync(path.join(dir, 'clr.go'), 'utf-8');
  assert(cg.includes('v = nil'), `go clear 应映射 v = nil：\n${cg}`);
});

test('emit', 'cpp vec! 宏字面量修复（CTAD std::vector{...}）', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'cpp-veclit');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'lit.hsl'), `fn sum_three() -> i32 {
    let v = vec![10, 20, 30];
    let a = v.first().unwrap_or(0);
    let b = v.last().unwrap_or(0);
    let c = v.get(1).unwrap_or(0);
    a + b + c
}
fn first_of_three() -> i32 {
    let v = vec![7, 8, 9];
    v.first().unwrap_or(-1)
}
fn main() {}
project {
    sum_three -> "sum.cpp" : cpp,
    first_of_three -> "first.cpp" : cpp
}`);
  const r = run(['emit', path.join(dir, 'lit.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const sc = fs.readFileSync(path.join(dir, 'sum.cpp'), 'utf-8');
  // 🔴 修复断言：v1.4.8 前 vec![10, 20, 30] 生成 [10, 20, 30] 是 lambda 捕获（非法）
  assert(sc.includes('std::vector{10, 20, 30}'), `vec! 宏应映射 std::vector{...}（CTAD）：\n${sc}`);
  assert(!sc.match(/=\s*\[\d+\s*,/), `不应再生成 [N, M, ...] 数组字面量（lambda 捕获非法）：\n${sc}`);
  const fc = fs.readFileSync(path.join(dir, 'first.cpp'), 'utf-8');
  assert(fc.includes('std::vector{7, 8, 9}'), `vec! 宏应映射 std::vector{...}：\n${fc}`);
  const main = `#include <cstdint>
#include <iostream>
#include <format>
int32_t sum_three();
int32_t first_of_three();
int main() {
    std::cout << std::format("{}\\n", sum_three());
    std::cout << std::format("{}\\n", first_of_three());
    return 0;
}`;
  fs.writeFileSync(path.join(dir, 'main.cpp'), main);
  execFileSync('g++', ['-std=c++23', path.join(dir, 'main.cpp'), path.join(dir, 'sum.cpp'), path.join(dir, 'first.cpp'), '-o', path.join(dir, 'lit-test')], { timeout: 60_000 });
  const stdout = execFileSync(path.join(dir, 'lit-test'), { encoding: 'utf-8', timeout: 30_000 });
  const lines = stdout.trim().split('\n');
  // sum_three: first=10 + last=30 + get(1)=20 = 60
  assertEq(lines[0], '60', `sum_three: first=10+get(1)=20+last=30=60：${stdout}`);
  assertEq(lines[1], '7', `first_of_three: 7：${stdout}`);
});

test('emit', 'cpp Option::or 链 + unwrap_or 兜底（g++ 编译+运行）', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'cpp-orchain');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'chain.hsl'), `fn three_opt(x: i64) -> i64 {
    let a: Option<i64> = Option::Some(x);
    let b: Option<i64> = Option::None;
    let c: Option<i64> = Option::Some(99);
    let r = a.or(b).or(c);
    r.unwrap_or(-1)
}
fn none_chain() -> i64 {
    let a: Option<i64> = Option::None;
    let b: Option<i64> = Option::None;
    let c: Option<i64> = Option::None;
    let r = a.or(b).or(c);
    r.unwrap_or(-7)
}
fn main() {}
project {
    three_opt -> "three.cpp" : cpp,
    none_chain -> "none.cpp" : cpp
}`);
  const r = run(['emit', path.join(dir, 'chain.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const tc = fs.readFileSync(path.join(dir, 'three.cpp'), 'utf-8');
  // 🔴 v1.4.7 matchDispatch hoist + v1.4.8 链式 or 助手：链式调用 .or().or().unwrap_or() 应无副作用双重求值问题
  // a.or(b).or(c).unwrap_or(-1) 应翻译为 _dhvOptOr(_dhvOptOr(a, b), c).value_or(-1)
  assert(tc.includes('_dhvOptOr('), `Option::or 应映射 _dhvOptOr：\n${tc}`);
  assert(tc.includes('.value_or('), `unwrap_or 应映射 value_or：\n${tc}`);
  // 编译 + 运行
  const main = `#include <cstdint>
#include <iostream>
#include <format>
int64_t three_opt(int64_t x);
int64_t none_chain();
int main() {
    std::cout << std::format("{}\\n", three_opt(42));
    std::cout << std::format("{}\\n", none_chain());
    return 0;
}`;
  fs.writeFileSync(path.join(dir, 'main.cpp'), main);
  execFileSync('g++', ['-std=c++23', path.join(dir, 'main.cpp'), path.join(dir, 'three.cpp'), path.join(dir, 'none.cpp'), '-o', path.join(dir, 'chain-test')], { timeout: 60_000 });
  const stdout = execFileSync(path.join(dir, 'chain-test'), { encoding: 'utf-8', timeout: 30_000 });
  const lines = stdout.trim().split('\n');
  assertEq(lines[0], '42', `three_opt(42): Some(42).or(None).or(Some(99)).unwrap_or(-1) = 42：${stdout}`);
  assertEq(lines[1], '-7', `none_chain: None.or(None).or(None).unwrap_or(-7) = -7：${stdout}`);
});

test('emit', 'cpp 全活体回归（Option 链 + Vec 方法族 综合场景，g++ 编译+运行）', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'cpp-combo');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'combo.hsl'), `fn process(mut v: Vec<i32>) -> i32 {
    v.sort();
    let sorted = v.is_sorted();
    if !sorted { return -1; }
    let first = v.first().unwrap_or(-1);
    let last = v.last().unwrap_or(-1);
    let mapped = Option::Some(first).map(|x| x * 10);
    let next = mapped.and_then(|x| Option::Some(x + last));
    next.unwrap_or(0)
}
fn build_pipeline() -> i32 {
    let mut v = vec![5, 3, 8, 1];
    v.extend(vec![9, 2]);
    v.sort();
    let n = v.len();
    if v.is_sorted() { n } else { -1 }
}
fn main() {}
project {
    process -> "proc.cpp" : cpp,
    build_pipeline -> "pipe.cpp" : cpp
}`);
  const r = run(['emit', path.join(dir, 'combo.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  for (const f of ['proc', 'pipe']) {
    const cpp = fs.readFileSync(path.join(dir, f + '.cpp'), 'utf-8');
    assert(!cpp.includes('未翻译'), `${f}.cpp 应为活体：\n${cpp}`);
  }
  const pc = fs.readFileSync(path.join(dir, 'proc.cpp'), 'utf-8');
  assert(pc.includes('std::sort') && pc.includes('std::is_sorted'), `process 应同时映射 sort + is_sorted：\n${pc}`);
  assert(pc.includes('_dhvFirst(') && pc.includes('_dhvLast('), `first/last 应映射助手：\n${pc}`);
  assert(pc.includes('_dhvOptMap(') && pc.includes('_dhvOptAndThen('), `Option 链 map+and_then 应同时出现：\n${pc}`);
  const pp = fs.readFileSync(path.join(dir, 'pipe.cpp'), 'utf-8');
  assert(pp.includes('std::vector{5, 3, 8, 1}'), `vec! 宏 CTAD：\n${pp}`);
  assert(pp.includes('_dhvExtend(v,'), `extend 应映射 _dhvExtend 助手：\n${pp}`);
  assert(pp.includes('std::sort'), `sort 应映射 std::sort：\n${pp}`);
  // 编译 + 运行
  const main = `#include <cstdint>
#include <iostream>
#include <format>
#include <vector>
int32_t process(std::vector<int32_t> v);
int32_t build_pipeline();
int main() {
    std::cout << std::format("{}\\n", process({3, 1, 2}));
    std::cout << std::format("{}\\n", build_pipeline());
    return 0;
}`;
  fs.writeFileSync(path.join(dir, 'main.cpp'), main);
  execFileSync('g++', ['-std=c++23', path.join(dir, 'main.cpp'), path.join(dir, 'proc.cpp'), path.join(dir, 'pipe.cpp'), '-o', path.join(dir, 'combo-test')], { timeout: 60_000 });
  const stdout = execFileSync(path.join(dir, 'combo-test'), { encoding: 'utf-8', timeout: 30_000 });
  const lines = stdout.trim().split('\n');
  // process({3,1,2}) -> sort to {1,2,3} -> first=1, last=3 -> Some(1).map(x*10)=10 -> and_then(x+3)=13 -> unwrap_or(0)=13
  assertEq(lines[0], '13', `process({3,1,2}): sorted→first=1*10=10+last=3=13：${stdout}`);
  // build_pipeline: vec![5,3,8,1] + extend [9,2] = [5,3,8,1,9,2] -> sort -> [1,2,3,5,8,9] -> is_sorted=true -> len=6
  assertEq(lines[1], '6', `build_pipeline: 6 元素：${stdout}`);
});

test('sync', '双向工程闭环：改镜像 → 回写 → 重 emit 更新', () => {
  const src = path.join(TMP, 'sync-src');
  fs.mkdirSync(src, { recursive: true });
  fs.copyFileSync(path.join(ROOT, 'examples/backends-demo/model.hsl'), path.join(src, 'model.hsl'));
  const agentSrc = fs.readFileSync(path.join(ROOT, 'examples/backends-demo/agent.hsl'), 'utf-8');
  fs.writeFileSync(path.join(src, 'agent.hsl'), agentSrc.replace(/"..\/model.hsl"/g, '"./model.hsl"'), 'utf-8');
  const out = path.join(TMP, 'sync-out');
  let r = run(['emit', path.join(src, 'agent.hsl'), '--out', out]);
  assertEq(r.code, 0, '初始 emit');
  const pyPath = path.join(out, 'gen/python/describe.py');
  let py = fs.readFileSync(pyPath, 'utf-8');
  py = py.replace('format!("respond: {}", text)', 'format!("respond [synced]: {}", text)');
  fs.writeFileSync(pyPath, py, 'utf-8');
  r = run(['sync', pyPath, '--root', src]);
  assertEq(r.code, 0, `sync 应成功：${r.stdout}`);
  assert(r.stdout.includes('回写：1 处'), `应回写 1 处：${r.stdout}`);
  const hsl = fs.readFileSync(path.join(src, 'model.hsl'), 'utf-8');
  assert(hsl.includes('respond [synced]'), 'HSL 源应包含回写内容');
  r = run(['emit', path.join(src, 'agent.hsl'), '--out', out]);
  assertEq(r.code, 0, '回写后 re-emit');
  py = fs.readFileSync(pyPath, 'utf-8');
  assert(py.includes('respond [synced]'), '活体翻译应反映回写内容');
});

// ---------------------------------------------------------------------------
// 4. 模糊测试（词法/语法器不崩：干净报错即可）
// ---------------------------------------------------------------------------
function mustFailCleanly(src: string, label: string): void {
  try {
    parseFileSource(src, '<fuzz>');
  } catch (e) {
    const msg = (e as Error).message;
    assert(typeof msg === 'string' && msg.length > 0, `${label}: 报错信息为空`);
    return;
  }
}

test('模糊', '随机 token 汤 ×200（不崩溃）', () => {
  const tokens = ['fn', 'let', 'mut', 'match', 'if', 'struct', 'enum', '{', '}', '(', ')', '::', '->', '=>', '&&', '||', '?', 'graph', 'loop', 'edge', 'node', 'native', 'project', 'scale', '=', '==', '1', '"str"', 'ident', 'r"raw"', '#[cap]', ';', ',', '.', '&', '|', '!', '_', '0x1F', "'c'", 'Vec', 'Result', '<', '>', '+', '*'];
  let seed = 12345;
  const rnd = (): number => { seed = (seed * 1103515245 + 12345) & 0x7fffffff; return seed / 0x7fffffff; };
  for (let i = 0; i < 200; i++) {
    const n = 1 + Math.floor(rnd() * 40);
    const parts: string[] = [];
    for (let j = 0; j < n; j++) parts.push(tokens[Math.floor(rnd() * tokens.length)]!);
    mustFailCleanly(parts.join(' '), `soup#${i}`);
  }
});

test('模糊', '病态输入：未闭合字符串/注释/块', () => {
  mustFailCleanly('fn main() { let s = "unterminated', 'unterminated-str');
  mustFailCleanly('fn main() { /* comment ', 'unterminated-block-comment');
  mustFailCleanly('fn main() { {{{{ ', 'unclosed-braces');
  mustFailCleanly('graph G { loop { ', 'unclosed-graph');
  mustFailCleanly('fn main() { let x = native python { import os', 'unclosed-native');
});

test('模糊', 'Unicode：CJK/emoji/全角/RTL', () => {
  parseFileSource('fn main() { println!("你好世界 🌍 ｈｅｌｌｏ"); }', '<u>');
  mustFailCleanly('fn main() { let 变量名 = 1; }', 'cjk-ident');
  mustFailCleanly('fn ‮main() {}', 'rtl-override');
});

test('模糊', 'BOM / CRLF / 空文件 / 纯注释', () => {
  mustFailCleanly('\ufefffn main() {}', 'bom');
  parseFileSource('fn main() {\r\n    println!("crlf");\r\n}\r\n', '<crlf>');
  mustFailCleanly('', 'empty');
  parseFileSource('// 只有注释', '<c>');
});

test('模糊', '深嵌套 500 层（不栈溢出崩溃）', () => {
  const deep = `fn main() { ${'('.repeat(500)} 1 ${')'.repeat(500)} }`;
  mustFailCleanly(deep, 'deep-parens');
  const deepBlock = `fn main() { ${'{'.repeat(400)} ${'}'.repeat(400)} }`;
  mustFailCleanly(deepBlock, 'deep-blocks');
});

test('模糊', '巨大字面量 / 长标识符', () => {
  mustFailCleanly(`fn main() { let x = ${'9'.repeat(5000)}; }`, 'huge-int-lit');
  mustFailCleanly(`fn main() { let ${'a'.repeat(10000)} = 1; }`, 'long-ident');
  parseFileSource('fn main() { let x = 123456789012345678901234567890; println!("{}", x); }', '<bigint>');
});

// ---------------------------------------------------------------------------
// 5. CLI 边界
// ---------------------------------------------------------------------------
test('CLI', '未知参数 → 报错退出', () => {
  const r = run(['check', 'dhv-ts/examples/smoke.hsl', '--no-such-flag']);
  assert(r.code !== 0, '未知参数应非零退出');
  assert(r.stderr.includes('未知参数') || r.stderr.includes('error'), `应有报错：${r.stderr}`);
});

test('CLI', '入口文件不存在 → 干净报错', () => {
  const r = run(['check', '/tmp/definitely-not-exist.hsl']);
  assert(r.code !== 0, '应非零退出');
  assert(r.stderr.includes('不存在') || r.stdout.includes('不存在') || r.stderr.includes('error'), `应有报错：${r.stderr}`);
});

test('CLI', 'check 缺参数 → usage + 退出码 2', () => {
  const r = run(['check']);
  assertEq(r.code, 2, '应退出码 2');
  assert(r.stdout.includes('用法'), '应有 usage');
});

test('CLI', '循环 import a→b→a（按引用补全，不挂起）', () => {
  const dir = path.join(TMP, 'cycle');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'a.hsl'), 'export fn fa() -> i64 { 1 }\nimport { fb } from "./b.hsl";\nfn main() { println!("{}", fa() + fb()); }');
  fs.writeFileSync(path.join(dir, 'b.hsl'), 'export fn fb() -> i64 { 2 }\nimport { fa } from "./a.hsl";\nfn use_fa() -> i64 { fa() }');
  const r = run(['run', path.join(dir, 'a.hsl'), '--quiet']);
  assertEq(r.code, 0, `循环 import 应可运行：${r.stdout}${r.stderr}`);
  assert(r.stdout.includes('3'), `fa()+fb()=3：${r.stdout}`);
});

test('CLI', 'import 不存在路径 → L-0 报错', () => {
  const dir = path.join(TMP, 'missing-import');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'a.hsl'), 'import { x } from "./nope.hsl";\nfn main() {}');
  const r = run(['check', path.join(dir, 'a.hsl')]);
  assert(r.code !== 0, '应失败');
  assert(r.stdout.includes('L-0') || r.stderr.includes('L-0') || r.stderr.includes('不存在'), `应有 L-0：${r.stdout}${r.stderr}`);
});

test('CLI', 'run 入口无 fn main → R-1 报错', () => {
  const dir = path.join(TMP, 'nomain');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'a.hsl'), 'fn helper() -> i64 { 1 }');
  const r = run(['run', path.join(dir, 'a.hsl')]);
  assert(r.code !== 0, '应失败');
  assert(r.stdout.includes('main') || r.stderr.includes('main'), '应提到 main 约定');
});

test('CLI', 'targets 命令输出 38 后端', () => {
  const r = run(['targets']);
  assertEq(r.code, 0, 'targets 应成功');
  for (const lang of ['python', 'typescript', 'rust', 'go', 'cpp', 'java', 'haskell', 'erlang', 'zig', 'vb', 'yaml', 'xml']) {
    assert(r.stdout.includes(lang), `targets 应含 ${lang}`);
  }
  assert(r.stdout.includes('32 编程语言') && r.stdout.includes('6 静态格式'), '应有总数说明');
});

// ---------------------------------------------------------------------------
// 5.5 v0.2.53 回归：import 别名（L-1）
// ---------------------------------------------------------------------------
// 实录：`import { Triage as TV } from ...` 后——
//   构造 `TV::Variant { .. }` 报「无法解析的结构体字面量」（evalStructExpr 按
//   原名查 this.enums）；match `TV::Variant { .. }` 却因 `enums.has(a)` 守卫
//   跳过而宽松通过 —— 构造位与模式位解析不对称。
// 修复：link 期把别名注册进全局类型注册表（enum + struct），模式位族名经
//   注册表解析（别名条目映射回原名 item）。
test('回归-L1', 'import 别名：枚举构造 + match 模式 + 结构体构造三通道一致', () => {
  const dir = path.join(TMP, 'alias-l1');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(
    path.join(dir, 'vocab.hsl'),
    `export enum Shape { Circle { radius: i64 }, Square { side: i64 } }
export struct Box { label: String }
`,
  );
  fs.writeFileSync(
    path.join(dir, 'main.hsl'),
    `import { Shape as S, Box as B } from "./vocab.hsl";

fn describe(s: S) -> String {
    match s {
        S::Circle { radius } => format!("circle r={}", radius),
        S::Square { side } => format!("square s={}", side),
    }
}

export fn main() -> i64 {
    let c = S::Circle { radius: 3 };
    let q = S::Square { side: 4 };
    let b = B { label: String::from("ok") };
    println!("{}", describe(c));
    println!("{}", describe(q));
    println!("{}", b.label);
    0
}
`,
  );
  const r = run(['run', path.join(dir, 'main.hsl'), '--quiet']);
  assertEq(r.code, 0, `别名构造应通过（exit=${r.code}）：${r.stdout}${r.stderr}`);
  assert(r.stdout.includes('circle r=3'), `别名 match 应命中：${r.stdout}`);
  assert(r.stdout.includes('square s=4'), `别名 match 第二分支：${r.stdout}`);
  assert(r.stdout.includes('ok'), `结构体别名构造应通过：${r.stdout}`);
});

test('回归-L1', 'import 别名：族名校验仍然严格（别名不产生类型混淆）', () => {
  const dir = path.join(TMP, 'alias-l1-strict');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(
    path.join(dir, 'vocab.hsl'),
    `export enum Shape { Dot, Circle { radius: i64 } }
export enum Color { Dot }
`,
  );
  fs.writeFileSync(
    path.join(dir, 'main.hsl'),
    `import { Shape as S, Color as C } from "./vocab.hsl";

export fn main() -> i64 {
    let d = S::Dot;
    // 错族：Shape 值 vs Color 别名模式（两族同名单元变体 Dot）
    // 修复前：族检查对未注册首段静默跳过 → 变体名撞车误匹配；
    // 修复后：族名经注册表解析 → 不匹配，落到正确分支
    match d {
        C::Dot => { println!("WRONG-FAMILY-MATCHED"); },
        S::Dot => { println!("right-family"); },
        S::Circle { radius: _ } => { println!("never"); },
    }
    0
}
`,
  );
  const r = run(['run', path.join(dir, 'main.hsl'), '--quiet']);
  assertEq(r.code, 0, `运行应通过（exit=${r.code}）：${r.stdout}${r.stderr}`);
  assert(r.stdout.includes('right-family') && !r.stdout.includes('WRONG-FAMILY'), `族名校验应严格：${r.stdout}`);
});

// ---------------------------------------------------------------------------
// 6. 压力测试
// ---------------------------------------------------------------------------
let stressCounter = 0;
function writeTmp(src: string): string {
  const p = path.join(TMP, `stress-${stressCounter++}.hsl`);
  fs.writeFileSync(p, src, 'utf-8');
  return p;
}

test('压力', '万级 Vec 操作（map/filter/sum 链）', () => {
  const r = run(['run', writeTmp(`fn main() -> i64 {
    let mut v: Vec<i64> = vec![];
    let mut i: i64 = 0;
    while i < 10000 {
        v.push(i);
        i = i + 1;
    }
    let evens = v.filter(|x| x % 2 == 0);
    let doubled = evens.map(|x| x * 2);
    println!("count={} sum={}", doubled.len(), doubled.sum());
    0
}`), '--quiet']);
  assertEq(r.code, 0, '万级 Vec 应可运行');
  assert(r.stdout.includes('count=5000 sum=49990000'), `结果错误：${r.stdout}`);
});

test('压力', '深递归 5000 层 → 干净错误（不崩溃进程）', () => {
  const r = run(['run', writeTmp(`fn rec(n: i64) -> i64 { if n == 0 { 0 } else { rec(n - 1) } }\nfn main() -> i64 { println!("{}", rec(5000)); 0 }`), '--quiet']);
  assert(r.code === 0 || (r.stderr.includes('运行期错误') || r.stdout.includes('运行期错误')), `应干净处理：exit=${r.code} ${r.stderr.slice(0, 200)}`);
});

test('压力', '长字符串拼接 10k 次', () => {
  const r = run(['run', writeTmp(`fn main() -> i64 {
    let mut s = String::from("");
    let mut i: i64 = 0;
    while i < 10000 {
        s.push_str("ab");
        i = i + 1;
    }
    println!("len={}", s.len());
    0
}`), '--quiet']);
  assertEq(r.code, 0, '长字符串应可运行');
  assert(r.stdout.includes('len=20000'), `长度错误：${r.stdout}`);
});

test('压力', '大 JSON 解析（std/json 本地解析器 2000 元素）', () => {
  const items: string[] = [];
  for (let i = 0; i < 2000; i++) items.push(`{"id":${i},"name":"item${i}","ok":${i % 2 === 0}}`);
  const json = `[${items.join(',')}]`;
  const p = writeTmp(`import { parse } from "std/json";\nfn main() -> i64 {
    let r = parse(String::from(${JSON.stringify(json)}));
    match r {
        Result::Ok(v) => { println!("ok"); },
        Result::Err(e) => { println!("err {}", e); },
    }
    0
}`);
  const r = run(['run', p, '--quiet']);
  assertEq(r.code, 0, '大 JSON 应可解析');
  assert(r.stdout.includes('ok'), `应解析成功：${r.stdout}`);
});

// ---------------------------------------------------------------------------
// v1.4.9 新增：parse::<T> / Option::filter / sort_by / 裸 None 链 / char 谓词
// ---------------------------------------------------------------------------

test('emit', 'String::parse::<T> turbofish 全语言活体（此前全部回退 contract）', () => {
  const dir = path.join(TMP, 'parse-all');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'pp.hsl'), `fn parse_probe() -> i64 {
    let a = "42".parse::<i64>().unwrap_or(0);
    let b = "3.5".parse::<f64>().unwrap_or(0.0);
    let c = "xyz".parse::<i64>().is_err();
    let e = "-5".parse::<u32>().is_err();
    a
}
fn main() {}
project {
    parse_probe -> "pp.py" : python,
    parse_probe -> "pp.ts" : typescript,
    parse_probe -> "pp.rs" : rust,
    parse_probe -> "pp.go" : go,
    parse_probe -> "pp.cpp" : cpp
}`);
  const r = run(['emit', path.join(dir, 'pp.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const py = fs.readFileSync(path.join(dir, 'pp.py'), 'utf-8');
  assert(!py.includes('未翻译'), `python 应活体翻译 parse：\n${py}`);
  assert(py.includes("_dhv_parse_int('42', 'i64')"), `py int parse 应带类型实参：\n${py}`);
  assert(py.includes('_dhv_parse_float(\'3.5\')'), `py float parse 应映射助手：\n${py}`);
  assert(py.includes("('xyz') is None") || py.includes('is None'), `py is_err 应为 None 检查：\n${py}`);
  const ts = fs.readFileSync(path.join(dir, 'pp.ts'), 'utf-8');
  assert(!ts.includes('未翻译'), `ts 应活体翻译 parse：\n${ts}`);
  assert(ts.includes("_dhvParseInt(s, 'i64')") || ts.includes("_dhvParseInt(\"42\", 'i64')") || ts.includes("_dhvParseInt('42', 'i64')"), `ts int parse 应映射助手：\n${ts}`);
  const rs = fs.readFileSync(path.join(dir, 'pp.rs'), 'utf-8');
  assert(rs.includes('"42".parse::<i64>()'), `rust 应原生 turbofish 直投：\n${rs}`);
  assert(rs.includes('.is_err()'), `rust is_err 原生：\n${rs}`);
  const go = fs.readFileSync(path.join(dir, 'pp.go'), 'utf-8');
  assert(!go.includes('未翻译'), `go 应活体翻译 parse：\n${go}`);
  assert(go.includes('_dhvParseInt("42", false)'), `go int parse 应映射助手：\n${go}`);
  assert(go.includes('_dhvParseInt("-5", true)'), `go u 型 parse 应传 unsigned=true：\n${go}`);
  assert(go.includes('_dhvParseFloat("3.5")'), `go float parse 应映射助手：\n${go}`);
  const cpp = fs.readFileSync(path.join(dir, 'pp.cpp'), 'utf-8');
  assert(!cpp.includes('未翻译'), `cpp 应活体翻译 parse：\n${cpp}`);
  assert(cpp.includes('_dhvParse<int64_t>("42")'), `cpp int parse 应模板实参：\n${cpp}`);
  assert(cpp.includes('_dhvParse<uint32_t>("-5")'), `cpp u 型 parse 应 unsigned 模板实参：\n${cpp}`);
  assert(cpp.includes('_dhvParse<double>("3.5")'), `cpp float parse 应模板实参：\n${cpp}`);
  assert(cpp.includes('template <typename T>') && cpp.includes('std::optional<T> _dhvParse'), `cpp prelude 应定义 _dhvParse 模板助手：\n${cpp}`);
});

test('emit', 'cpp String::parse g++ 编译+运行语义级（与 interp 逐字对齐）', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'parse-cpp');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'pp.hsl'), `fn parse_sem() -> i64 {
    let a = "42".parse::<i64>().unwrap_or(0);
    let b = "3.5".parse::<f64>().unwrap_or(0.0);
    let c = "xyz".parse::<i64>().is_err();
    let d = "  ".parse::<f64>().is_err();
    let e = "-5".parse::<u32>().is_err();
    let f = "42".parse::<i64>().is_ok();
    let g = "  99  ".parse::<i64>().unwrap_or(-1);
    let mut total = a + g;
    if c { total = total + 10; }
    if d { total = total + 100; }
    if e { total = total + 1000; }
    if f { total = total + 10000; }
    total
}
fn main() {}
project {
    parse_sem -> "pp.cpp" : cpp
}`);
  const r = run(['emit', path.join(dir, 'pp.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const main = `#include <cstdint>
#include <iostream>
#include <format>
int64_t parse_sem();
int main() { std::cout << std::format("{}\\n", parse_sem()); return 0; }`;
  fs.writeFileSync(path.join(dir, 'main.cpp'), main);
  execFileSync('g++', ['-std=c++23', path.join(dir, 'main.cpp'), path.join(dir, 'pp.cpp'), '-o', path.join(dir, 'pp-test')], { timeout: 60_000 });
  const stdout = execFileSync(path.join(dir, 'pp-test'), { encoding: 'utf-8', timeout: 30_000 });
  // interp 实测：a=42 g=99 c=+10 d=+100（空串修复后 Err）e=+1000 f=+10000 → 11251
  assertEq(stdout.trim(), '11251', `parse 语义应与 interp 对齐（42+99+10+100+1000+10000）：${stdout}`);
});

test('emit', 'python String::parse 语义级执行（python3 exec）', () => {
  const dir = path.join(TMP, 'parse-py');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'pp.hsl'), `fn parse_sem() -> i64 {
    let a = "42".parse::<i64>().unwrap_or(0);
    let c = "xyz".parse::<i64>().is_err();
    let e = "-5".parse::<u32>().is_err();
    let g = "  99  ".parse::<i64>().unwrap_or(-1);
    let h = "1_0".parse::<i64>().is_err();
    let mut total = a + g;
    if c { total = total + 10; }
    if e { total = total + 1000; }
    if h { total = total + 100000; }
    total
}
fn main() {}
project {
    parse_sem -> "pp.py" : python
}`);
  const r = run(['emit', path.join(dir, 'pp.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const pyCode = `
ns = {}
exec(open('${path.join(dir, 'pp.py').replace(/\\/g, '/')}').read(), ns)
assert ns['parse_sem']() == 101151, ns['parse_sem']()
print('parse-py-semantics-ok')
`;
  fs.writeFileSync(path.join(dir, 'verify.py'), pyCode);
  const stdout = execFileSync('python3', [path.join(dir, 'verify.py')], { encoding: 'utf-8', timeout: 30_000 });
  assert(stdout.includes('parse-py-semantics-ok'), `py parse 语义验证异常：${stdout}`);
});

test('回归', 'interp parse 空串语义修复（"".parse::<f64>() → Err）', () => {
  const r = run(['run', writeTmp(`fn main() -> i64 {
    let bad = "  ".parse::<f64>().is_err();
    let ok = "3.5".parse::<f64>().is_ok();
    let int_bad = "".parse::<i64>().is_err();
    let mut total = 0;
    if bad { total = total + 1; }
    if ok { total = total + 10; }
    if int_bad { total = total + 100; }
    println!("{}", total);
    0
}`), '--quiet']);
  assertEq(r.code, 0, `run 应通过：${r.stderr}`);
  // 空白串 float parse → Err（v1.4.9 修复：此前 JS Number("") === 0 隐患 → Ok(0)）
  assert(r.stdout.includes('111'), `空串 parse 应统一 Err（1+10+100=111）：${r.stdout}`);
});

test('emit', 'Option::filter interp + python exec + cpp 结构（v1.4.9 新增 builtin）', () => {
  const dir = path.join(TMP, 'optfilter');
  fs.mkdirSync(dir, { recursive: true });
  // interp 语义先行
  const ri = run(['run', writeTmp(`fn main() -> i64 {
    let g = Option::Some(5).filter(|x| x > 3).unwrap_or(-1);
    let h = Option::Some(2).filter(|x| x > 3).unwrap_or(-1);
    let i = Option::None.filter(|x| x > 3).unwrap_or(-1);
    println!("{}", g + h + i);
    0
}`), '--quiet']);
  assertEq(ri.code, 0, `interp 应支持 Option::filter：${ri.stderr}`);
  assert(ri.stdout.includes('3'), `interp filter 语义（5-1-1=3）：${ri.stdout}`);
  // 投射
  fs.writeFileSync(path.join(dir, 'of.hsl'), `fn opt_filter_sem() -> i64 {
    let g = Option::Some(5).filter(|x| x > 3).unwrap_or(-1);
    let h = Option::Some(2).filter(|x| x > 3).unwrap_or(-1);
    let i = Option::None.filter(|x| x > 3).unwrap_or(-2);
    g + h + i
}
fn main() {}
project {
    opt_filter_sem -> "of.py" : python,
    opt_filter_sem -> "of.cpp" : cpp,
    opt_filter_sem -> "of.rs" : rust
}`);
  const r = run(['emit', path.join(dir, 'of.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const py = fs.readFileSync(path.join(dir, 'of.py'), 'utf-8');
  assert(!py.includes('未翻译'), `py 应活体翻译 filter：\n${py}`);
  assert(py.includes('_dhv_filter('), `py filter 应映射 _dhv_filter 助手：\n${py}`);
  const pyCode = `
ns = {}
exec(open('${path.join(dir, 'of.py').replace(/\\/g, '/')}').read(), ns)
assert ns['opt_filter_sem']() == 2, ns['opt_filter_sem']()
print('filter-py-semantics-ok')
`;
  fs.writeFileSync(path.join(dir, 'verify.py'), pyCode);
  const stdout = execFileSync('python3', [path.join(dir, 'verify.py')], { encoding: 'utf-8', timeout: 30_000 });
  assert(stdout.includes('filter-py-semantics-ok'), `py filter 语义验证异常（5-1-2=2）：${stdout}`);
  const cpp = fs.readFileSync(path.join(dir, 'of.cpp'), 'utf-8');
  assert(!cpp.includes('未翻译'), `cpp 应活体翻译 filter：\n${cpp}`);
  assert(cpp.includes('_dhvOptFilter(_dhvSome(5),'), `cpp filter 应映射 _dhvOptFilter 助手：\n${cpp}`);
  assert(cpp.includes('template <typename T, typename F>') && cpp.includes('std::optional<T> _dhvOptFilter'), `cpp prelude 应定义 _dhvOptFilter：\n${cpp}`);
  const rs = fs.readFileSync(path.join(dir, 'of.rs'), 'utf-8');
  assert(rs.includes('.filter(|x| (x > 3))'), `rust 应原生 filter 直投：\n${rs}`);
});

test('emit', 'cpp 裸 Option::None 链式修复（g++ 编译+运行 113 + 回归断言）', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'none-chain');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'nc.hsl'), `fn none_chain() -> i64 {
    let a = Option::None.map(|x| x).unwrap_or(-1);
    let b = Option::None.filter(|x| x > 3).unwrap_or(-2);
    let c = Option::None.or(Option::Some(99)).unwrap_or(-3);
    let d = Option::None.is_none();
    let f = Option::None.unwrap_or(7);
    let mut total = a + b + c + f;
    if d { total = total + 10; }
    total
}
fn main() {}
project {
    none_chain -> "nc.cpp" : cpp
}`);
  const r = run(['emit', path.join(dir, 'nc.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const cpp = fs.readFileSync(path.join(dir, 'nc.cpp'), 'utf-8');
  assert(!cpp.includes('未翻译'), `cpp 应活体翻译裸 None 链：\n${cpp}`);
  // 🔴 v1.4.9 修复断言：此前生成 _dhvOptMap(std::nullopt, f)（模板推导失败）/ std::nullopt.value_or(0) 编译必炸
  assert(!cpp.includes('_dhvOptMap(std::nullopt'), `不应再对 nullopt_t 调用模板助手（推导失败）：\n${cpp}`);
  assert(!/std::nullopt\s*\.\s*value_or/.test(cpp), `不应再生成 nullopt.value_or（nullopt_t 无成员）：\n${cpp}`);
  assert(cpp.includes('_dhvNone.map('), `None.map 应映射 _dhvNone 包装器：\n${cpp}`);
  assert(cpp.includes('_dhvNone.filter('), `None.filter 应映射 _dhvNone 包装器：\n${cpp}`);
  assert(cpp.includes('struct _dhvNoneT'), `prelude 应定义 _dhvNoneT 链式包装器：\n${cpp}`);
  assert(cpp.includes('auto d = true;'), `None.is_none() 应常量折叠 true：\n${cpp}`);
  const main = `#include <cstdint>
#include <iostream>
#include <format>
int64_t none_chain();
int main() { std::cout << std::format("{}\\n", none_chain()); return 0; }`;
  fs.writeFileSync(path.join(dir, 'main.cpp'), main);
  execFileSync('g++', ['-std=c++23', path.join(dir, 'main.cpp'), path.join(dir, 'nc.cpp'), '-o', path.join(dir, 'nc-test')], { timeout: 60_000 });
  const stdout = execFileSync(path.join(dir, 'nc-test'), { encoding: 'utf-8', timeout: 30_000 });
  // interp 实测：-1-2+99+7+10(true) = 113
  assertEq(stdout.trim(), '113', `None 链语义应与 interp 对齐：${stdout}`);
});

test('emit', 'cpp Vec::sort_by 稳定排序（g++ 编译+运行 + std::stable_sort 断言）', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'sortby-cpp');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'sb.hsl'), `struct Item {
    name: String,
    score: i32,
}
fn sort_by_struct(mut v: Vec<Item>) -> i32 {
    v.sort_by(|it| it.score);
    v[0].score * 100 + v[1].score
}
fn sort_by_stable(mut v: Vec<i32>) -> i32 {
    v.sort_by(|x| x % 10);
    v[0] * 100 + v[1]
}
fn main() {}
project {
    sort_by_struct -> "sbs.cpp" : cpp,
    sort_by_stable -> "sbt.cpp" : cpp
}`);
  const r = run(['emit', path.join(dir, 'sb.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const sbs = fs.readFileSync(path.join(dir, 'sbs.cpp'), 'utf-8');
  assert(!sbs.includes('未翻译'), `cpp 应活体翻译 sort_by：\n${sbs}`);
  assert(sbs.includes('std::stable_sort(v.begin(), v.end()'), `cpp sort_by 应映射 std::stable_sort：\n${sbs}`);
  assert(!/std::sort\(v\.begin/.test(sbs), `不应使用非稳定 std::sort（稳定序与 interp/rust 对齐）：\n${sbs}`);
  const sbt = fs.readFileSync(path.join(dir, 'sbt.cpp'), 'utf-8');
  assert(sbt.includes('std::stable_sort'), `字面量 key 也应 stable_sort：\n${sbt}`);
  const main = `#include <cstdint>
#include <iostream>
#include <format>
#include <string>
#include <vector>
struct Item { std::string name; int32_t score; };
int32_t sort_by_struct(std::vector<Item> v);
int32_t sort_by_stable(std::vector<int32_t> v);
int main() {
    std::cout << std::format("{}\\n", sort_by_struct({{"a", 30}, {"b", 10}, {"c", 20}}));
    std::cout << std::format("{}\\n", sort_by_stable({13, 21, 12, 31}));
    return 0;
}`;
  fs.writeFileSync(path.join(dir, 'main.cpp'), main);
  execFileSync('g++', ['-std=c++23', path.join(dir, 'main.cpp'), path.join(dir, 'sbs.cpp'), path.join(dir, 'sbt.cpp'), '-o', path.join(dir, 'sb-test')], { timeout: 60_000 });
  const stdout = execFileSync(path.join(dir, 'sb-test'), { encoding: 'utf-8', timeout: 30_000 });
  const lines = stdout.trim().split('\n');
  // interp 实测：按 score 排序 {10,20,30} → 10*100+20 = 1020
  assertEq(lines[0], '1020', `sort_by 结构体 key 应与 interp 对齐：${stdout}`);
  // 稳定排序：key(13)=3, key(21)=1, key(12)=2, key(31)=1 → 稳定保序 [21,31,12,13] → 21*100+31 = 2131
  assertEq(lines[1], '2131', `sort_by 稳定序（21 在 31 前）应与 interp 对齐：${stdout}`);
});

test('emit', 'go Vec::sort_by 闭包内联替换结构断言（sort.SliceStable + v[i] 替换）', () => {
  const dir = path.join(TMP, 'sortby-go');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'sb.hsl'), `struct Item {
    name: String,
    score: i32,
}
fn sort_by_struct(mut v: Vec<Item>) -> i32 {
    v.sort_by(|it| it.score);
    v[0].score * 100 + v[1].score
}
fn main() {}
project {
    sort_by_struct -> "sbs.go" : go,
    Item -> "item.go" : go
}`);
  const r = run(['emit', path.join(dir, 'sb.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const sg = fs.readFileSync(path.join(dir, 'sbs.go'), 'utf-8');
  assert(!sg.includes('未翻译'), `go 应活体翻译 sort_by（闭包内联替换）：\n${sg}`);
  assert(sg.includes('sort.SliceStable(v,'), `go sort_by 应映射 sort.SliceStable：\n${sg}`);
  assert(!/sort\.Slice\(/.test(sg), `不应使用非稳定 sort.Slice：\n${sg}`);
  // 闭包体内联替换：|it| it.score → v[i].score < v[j].score（无 go 闭包值直投）
  assert(sg.includes('func(i, j int) bool { return v[i].score < v[j].score }'), `go comparator 应内联替换闭包体：\n${sg}`);
  assert(sg.includes('"sort"'), `go prelude 应导入 sort 包：\n${sg}`);
});

test('emit', 'char 谓词 is_alphabetic/is_numeric（cpp g++ 编译+运行 + ts 正则断言）', () => {
  if (!hasTool('g++') || !hasCpp23()) return;
  const dir = path.join(TMP, 'charpred');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'cp.hsl'), `fn char_pred(c: String, d: String, e: String, z: String) -> i32 {
    let a = c.is_alphabetic();
    let b = d.is_numeric();
    let g = e.is_alphabetic();
    let h = z.is_numeric();
    let mut total = 0;
    if a { total = total + 1; }
    if b { total = total + 10; }
    if g { total = total + 100; }
    if h { total = total + 1000; }
    total
}
fn main() {}
project {
    char_pred -> "cp.cpp" : cpp,
    char_pred -> "cp.go" : go,
    char_pred -> "cp.ts" : typescript
}`);
  const r = run(['emit', path.join(dir, 'cp.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const cpp = fs.readFileSync(path.join(dir, 'cp.cpp'), 'utf-8');
  assert(!cpp.includes('未翻译'), `cpp 应活体翻译 char 谓词：\n${cpp}`);
  assert(cpp.includes('_dhvIsAlpha(c)'), `cpp is_alphabetic 应映射 _dhvIsAlpha：\n${cpp}`);
  assert(cpp.includes('_dhvIsDigit(d)'), `cpp is_numeric 应映射 _dhvIsDigit：\n${cpp}`);
  assert(cpp.includes('inline bool _dhvIsAlpha'), `cpp prelude 应定义 _dhvIsAlpha：\n${cpp}`);
  const ts = fs.readFileSync(path.join(dir, 'cp.ts'), 'utf-8');
  assert(ts.includes('/[A-Za-z\\u0080-\\uFFFF]/.test(c)'), `ts is_alphabetic 应映射 interp 同源正则：\n${ts}`);
  assert(ts.includes('/[0-9]/.test(d)'), `ts is_numeric 应映射正则：\n${ts}`);
  const go = fs.readFileSync(path.join(dir, 'cp.go'), 'utf-8');
  assert(go.includes('_dhvIsAlpha(c)'), `go is_alphabetic 应映射 _dhvIsAlpha：\n${go}`);
  assert(go.includes('_dhvIsDigit(d)'), `go is_numeric 应映射 _dhvIsDigit：\n${go}`);
  const main = `#include <cstdint>
#include <iostream>
#include <format>
#include <string>
int32_t char_pred(std::string c, std::string d, std::string e, std::string z);
int main() {
    std::cout << std::format("{}\\n", char_pred("A", "5", "!", "z"));
    std::cout << std::format("{}\\n", char_pred("\\u00e9", "5", "A", "z"));
    return 0;
}`;
  fs.writeFileSync(path.join(dir, 'main.cpp'), main);
  execFileSync('g++', ['-std=c++23', path.join(dir, 'main.cpp'), path.join(dir, 'cp.cpp'), '-o', path.join(dir, 'cp-test')], { timeout: 60_000 });
  const stdout = execFileSync(path.join(dir, 'cp-test'), { encoding: 'utf-8', timeout: 30_000 });
  const lines = stdout.trim().split('\n');
  // interp 实测：'A'alpha=1 + '5'num=10 + '!'非alpha=0 + 'z'非num=0 → 11
  assertEq(lines[0], '11', `char 谓词应与 interp 对齐：${stdout}`);
  // 'é'(U+00E9 ≥ U+0080) alpha=1 + '5'num=10 + 'A'alpha=100 + 'z'非num=0 → 111（UTF-8 首字节 ≥ 0x80 精确对齐 interp 正则）
  assertEq(lines[1], '111', `非 ASCII 字符谓词应与 interp 对齐：${stdout}`);
});

// ===========================================================================
// v1.4.10 新增（105→109）：真机工具链编译级验证
// go 同 package 助手去重 + import 按需裁剪 / rust format 内联捕获修复 + HashMap 导入 /
// rustc+cargo 真机编译 / go vet / javac / kotlinc 探测式编译
// ===========================================================================

test('emit', 'rust format! 内联捕获修复（标识符内联 + 表达式位置参数，真机 rustc 编译）', () => {
  const RS = rustcBin();
  if (!RS) return;
  const dir = path.join(TMP, 'rs-fmt');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'fm.hsl'), `fn describe(name: String, count: i64) -> String {
    format!("tool {} with {} args", name, count)
}
fn vec_desc(v: Vec<i64>) -> String {
    format!("n={}", v.len())
}
fn main() {}
project { describe -> "fm.rs" : rust, vec_desc -> "vd.rs" : rust }`);
  const r = run(['emit', path.join(dir, 'fm.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const rs = fs.readFileSync(path.join(dir, 'fm.rs'), 'utf-8');
  assert(!rs.includes('未翻译'), `describe 应为活体：\n${rs}`);
  // v1.4.10 修复断言：纯标识符实参 → 内联捕获（合法且惯用），且不重复传参
  //（v1.4.10 前的双重形态：{name} 内联捕获 + 参数列表同时传 name = rustc invalid format string）
  assert(rs.includes('format!("tool {name} with {count} args")'), `纯标识符应为内联捕获形态：\n${rs}`);
  // 仅检查非注释代码行（hsl-mirror 镜像含原始 HSL 源码，允许含位置参数形态文本）
  const codeLines = rs.split('\n').filter((l) => !l.trim().startsWith('//')).join('\n');
  assert(!codeLines.includes('args", name, count'), `内联捕获不应重复传参：\n${codeLines}`);
  // 表达式实参（v.len()）→ 位置参数形态（内联捕获不支持方法调用）
  const vd = fs.readFileSync(path.join(dir, 'vd.rs'), 'utf-8');
  assert(vd.includes('format!("n={}", v.len())'), `表达式实参应为位置参数形态：\n${vd}`);
  assert(!vd.includes('{v.len()}'), `不应生成非法内联捕获 {v.len()}：\n${vd}`);
  // 真机编译级：单文件 lib crate
  execFileSync(RS, ['--edition', '2021', '--crate-type', 'lib', path.join(dir, 'fm.rs'), '-o', path.join(dir, 'fm.rlib')], { timeout: 120_000 });
  execFileSync(RS, ['--edition', '2021', '--crate-type', 'lib', path.join(dir, 'vd.rs'), '-o', path.join(dir, 'vd.rlib')], { timeout: 120_000 });
});

test('emit', 'rust HashMap 头部导入（真机 rustc 编译——此前 HashMap 未导入必炸）', () => {
  const RS = rustcBin();
  if (!RS) return;
  const dir = path.join(TMP, 'rs-map');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'mp.hsl'), `fn make() -> HashMap<String, i64> {
    let mut m: HashMap<String, i64> = HashMap::new();
    m.insert(String::from("a"), 1);
    m
}
fn main() {}
project { make -> "mp.rs" : rust }`);
  const r = run(['emit', path.join(dir, 'mp.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const rs = fs.readFileSync(path.join(dir, 'mp.rs'), 'utf-8');
  assert(!rs.includes('未翻译'), `make 应为活体：\n${rs}`);
  // v1.4.10 修复断言：HashMap 非 prelude，头部必须 use（此前 rustc E0425 cannot find type HashMap）
  assert(rs.includes('use std::collections::HashMap;'), `rust 头部应导入 HashMap：\n${rs}`);
  execFileSync(RS, ['--edition', '2021', '--crate-type', 'lib', path.join(dir, 'mp.rs'), '-o', path.join(dir, 'mp.rlib')], { timeout: 120_000 });
});

test('emit', 'go 多文件助手去重 + import 按需裁剪（真机 go build + go vet）', () => {
  const GO = goBin();
  if (!GO) return;
  const dir = path.join(TMP, 'go-dedup');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'dd.hsl'), `fn first_of(v: Vec<i64>) -> i64 {
    v.first().unwrap_or(-1)
}
fn last_of(v: Vec<i64>) -> i64 {
    v.last().unwrap_or(-9)
}
fn main() {}
project {
    first_of -> "fo.go" : go,
    last_of -> "lo.go" : go,
}`);
  const r = run(['emit', path.join(dir, 'dd.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  const f1 = fs.readFileSync(path.join(dir, 'fo.go'), 'utf-8');
  const f2 = fs.readFileSync(path.join(dir, 'lo.go'), 'utf-8');
  // v1.4.10 修复断言：同 package 助手单次声明 + 非首文件 import 按需（此前重复声明 + unused import 编译错误）
  const defs = [f1, f2].filter((t) => t.includes('func _dhvFirst[')).length;
  assertEq(defs, 1, `助手应只在一个文件声明（同 package 去重）：fo=${f1.includes('func _dhvFirst[')} lo=${f2.includes('func _dhvFirst[')}`);
  assert(f1.includes('func _dhvFirst[') || f2.includes('func _dhvFirst['), `首文件应声明 _dhvFirst 助手`);
  // 真机编译级 + vet
  const bdir = path.join(dir, 'gobuild');
  fs.mkdirSync(bdir, { recursive: true });
  for (const f of ['fo.go', 'lo.go']) fs.copyFileSync(path.join(dir, f), path.join(bdir, f));
  execFileSync(GO, ['mod', 'init', 'dd'], { cwd: bdir, timeout: 30_000, stdio: 'pipe' });
  execFileSync(GO, ['build', './...'], { cwd: bdir, timeout: 120_000, stdio: 'pipe' });
  execFileSync(GO, ['vet', './...'], { cwd: bdir, timeout: 120_000, stdio: 'pipe' });
});

test('emit', 'javac 编译级验证（java 后端真机编译——此前仅结构断言）', () => {
  let JAVAC: string | null = null;
  for (const c of ['javac', path.join(os.homedir(), 'opt/jdk-21.0.12.1+1/bin/javac')]) {
    try { execFileSync(c, ['--version'], { encoding: 'utf-8', timeout: 5_000 }); JAVAC = c; break; } catch { /* next */ }
  }
  if (!JAVAC) return;
  const dir = path.join(TMP, 'java-compile');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'jc.hsl'), `struct Point { x: i64, y: i64 }
fn clamp(v: i64, lo: i64, hi: i64) -> i64 {
    if v < lo { lo } else if v > hi { hi } else { v }
}
fn main() {}
project {
    Point -> "Point.java" : java,
    clamp -> "Clamp.java" : java,
}`);
  const r = run(['emit', path.join(dir, 'jc.hsl'), '--out', dir]);
  assertEq(r.code, 0, `emit 应通过：${r.stdout}`);
  // 真机编译级：javac 全部生成 .java（此前 java 后端从未真机编译过）
  const javaFiles = fs.readdirSync(dir).filter((f) => f.endsWith('.java')).map((f) => f);
  assert(javaFiles.length > 0, `应生成 .java 文件：${fs.readdirSync(dir).join(', ')}`);
  execFileSync(JAVAC, ['-d', dir, ...javaFiles], { cwd: dir, timeout: 120_000, stdio: 'pipe' });
});

// ---------------------------------------------------------------------------
// runner
// ---------------------------------------------------------------------------
async function main(): Promise<number> {
  console.log(`\n╔══════════════════════════════════════════════════════════════╗`);
  console.log(`║  HSL / DHV 严格测试套件 · ${cases.length} 个用例                       ║`);
  console.log(`╚══════════════════════════════════════════════════════════════╝\n`);
  const groups = [...new Set(cases.map((c) => c.group))];
  const t0 = Date.now();
  for (const group of groups) {
    console.log(`── ${group} ${'─'.repeat(Math.max(1, 58 - group.length))}`);
    for (const c of cases.filter((x) => x.group === group)) {
      const t1 = Date.now();
      try {
        await c.fn();
        passed++;
        console.log(`  ✓ ${c.name} (${Date.now() - t1}ms)`);
      } catch (err) {
        failed++;
        const msg = (err as Error).message.split('\n').slice(0, 6).join('\n        ');
        failures.push({ group, name: c.name, err: msg });
        console.log(`  ✗ ${c.name}`);
        console.log(`        ${msg}`);
      }
    }
  }
  console.log(`\n${'═'.repeat(64)}`);
  console.log(`  ${passed} 通过 · ${failed} 失败 · 共 ${cases.length} 用例 · ${Date.now() - t0}ms`);
  if (failed > 0) {
    console.log(`\n  失败清单：`);
    for (const f of failures) console.log(`    [${f.group}] ${f.name}`);
  }
  console.log('');
  fs.rmSync(TMP, { recursive: true, force: true });
  return failed > 0 ? 1 : 0;
}

void main().then((code) => process.exit(code));
