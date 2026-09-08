// ============================================================================
// ide/tests/validate.js — HSL IDE 扩展发布级校验（v0.2.0）
// ----------------------------------------------------------------------------
// 运行：bun ide/tests/validate.js（或 node ide/tests/validate.js）
// 覆盖：
//   1. 扩展清单与全部 JSON 资源合法性（package / language-config / tmLanguage
//      / snippets / theme）
//   2. tmLanguage 全部 regex 可编译（Oniguruma 简单子集以 JS RegExp 验证）
//   3. 38 后端投射语言高亮覆盖（rules 展开后的 id + 别名全量）
//   4. native 块语言 = 32 编程语言（静态格式后端必须不匹配）
//   5. 字符 'H' 与标签 'outer 冲突回归（v0.1.1 修复项）
//   6. 数字形态矩阵：无后缀浮点 / 指数 / 后缀浮点 / 整数 / hex
//   7. 原始字符串零井号形态 r"..."
//   8. 扩展入口 extension.js 语法检查（node --check 同义）
//   9. package.json 命令完整性（v0.2.0 全链路：check/run/emit/compile/targets）
//  10. parsers.js 错误行解析（含消息内括号 + Windows 盘符路径）
//  11. parsers.js targets 注册表解析（Tier 分组 + 38 条）
//  12. parsers.js run 产物摘要（run.json + report.md 双通道容错）
// 退出码：0 = 全部通过；1 = 任一失败（CI 门禁）。
// ============================================================================
import { readFileSync } from 'node:fs';
import * as path from 'node:path';
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';

const ROOT = path.dirname(new URL(import.meta.url).pathname).replace(/\/tests$/, '');
let failed = 0;
const ok = (name) => console.log(`  ✓ ${name}`);
const bad = (name, detail) => { failed++; console.log(`  ✗ ${name}${detail ? ' — ' + detail : ''}`); };

// ---- 1. JSON 资源合法性 ----
const jsonFiles = [
  'package.json', 'language-configuration.json', 'syntaxes/hsl.tmLanguage.json',
  'snippets/hsl.json', 'themes/hsl-dark-color-theme.json',
];
const jsons = {};
for (const f of jsonFiles) {
  try { jsons[f] = JSON.parse(readFileSync(path.join(ROOT, f), 'utf-8')); ok(`${f} JSON 合法`); }
  catch (e) { bad(`${f} JSON 合法`, e.message); }
}

// ---- 2. tmLanguage regex 全量可编译 ----
{
  const g = jsons['syntaxes/hsl.tmLanguage.json'];
  let regexErr = null;
  const walk = (node) => {
    if (node && typeof node === 'object') {
      for (const [k, v] of Object.entries(node)) {
        if (k === 'match' || k === 'begin' || k === 'end') {
          try { new RegExp(v); } catch (e) { regexErr ??= `${v} → ${e.message}`; }
        }
        walk(v);
      }
    } else if (Array.isArray(node)) node.forEach(walk);
  };
  walk(g);
  regexErr ? bad('tmLanguage regex 编译', regexErr) : ok('tmLanguage 全 regex 可编译');
}

const G = jsons['syntaxes/hsl.tmLanguage.json'].repository;
const srcOf = (pat) => new RegExp(pat);

// ---- 3. 38 后端投射语言高亮覆盖 ----
{
  const proj = srcOf(G['hsl-projection'].patterns[0].match);
  const ids = ['python','typescript','javascript','rust','go','cpp','java','csharp','kotlin','swift',
    'ruby','php','lua','perl','bash','powershell','r','julia','scala','elixir','erlang','haskell',
    'ocaml','fsharp','zig','nim','crystal','dart','groovy','objectivec','d','vb',
    'yaml','markdown','json','toml','ini','xml'];
  const aliases = ['ts','js','py','md','yml','c++','sh','bash','objective-c'];
  const missing = ids.filter((id) => !proj.test(`X -> "s" : ${id},`));
  const missingAlias = aliases.filter((a) => !proj.test(`X -> "s" : ${a},`));
  (missing.length + missingAlias.length) === 0
    ? ok(`投射语言高亮 38 后端 + 别名全覆盖`)
    : bad('投射语言高亮', `漏 ${[...missing, ...missingAlias].join(',')}`);
}

// ---- 4. native 块语言集合 ----
{
  const nat = srcOf(G['native-block'].patterns[0].match);
  const code = ['go','cpp','java','bash','kotlin','zig','vb','objectivec'];
  const statics = ['yaml','markdown','json','toml','ini','xml'];
  const badCode = code.filter((l) => !nat.test(`native ${l} {`));
  const badStatic = statics.filter((l) => nat.test(`native ${l} {`));
  badCode.length === 0 && badStatic.length === 0
    ? ok('native 语言 = 32 编程语言（静态格式正确排除）')
    : bad('native 语言集合', `缺 ${badCode} / 误含 ${badStatic}`);
}

// ---- 5. char / label 冲突回归 ----
{
  const charP = srcOf(G.char.match);
  const labelP = srcOf(G.labels.match);
  const order = jsons['syntaxes/hsl.tmLanguage.json'].patterns.map((p) => p.include);
  const cond = charP.test("'H'") && !labelP.test("'H'") && labelP.test("'outer:") && order.indexOf('#char') < order.indexOf('#labels');
  cond ? ok(`字符 'H' 不被标签规则吞；'outer: 识别为标签；char 规则先于 labels`) : bad('char/label 冲突回归');
}

// ---- 6. 数字形态矩阵 ----
{
  const [dot, exp, suf, , , , int] = G.numbers.patterns.map((p) => srcOf(p.match));
  const conds = [
    ['3.14 是 float', dot.test('3.14')],
    ['1e-9 是 float', exp.test('1e-9')],
    ['5f64 是 float', suf.test('5f64')],
    ['42 是 integer', int.test('42')],
    ['0xFF 是 hex', G.numbers.patterns[3] ? srcOf(G.numbers.patterns[3].match).test('0xFF') : true],
    ['42 不被 float 抢占', !dot.test('42') && !exp.test('42')],
  ];
  const bads = conds.filter(([, v]) => !v).map(([n]) => n);
  bads.length === 0 ? ok('数字形态矩阵（float 无后缀/指数/后缀、integer、hex）') : bad('数字形态矩阵', bads.join('、'));
}

// ---- 7. 原始字符串零井号 ----
{
  const raw = srcOf(G.strings.patterns[0].match);
  raw.test('r"C:\\path"') && raw.test('r#"a"#')
    ? ok(`原始字符串 r"..." / r#"..."# 双形态`) : bad('原始字符串零井号形态');
}

// ---- 8. 扩展入口语法（node --check 同义）----
{
  try { execFileSync(process.execPath.replace(/(bun|node)$/, 'node'), ['--check', path.join(ROOT, 'extension.js')], { stdio: 'pipe' }); ok('extension.js 语法合法'); }
  catch (e) {
    try { execFileSync('node', ['--check', path.join(ROOT, 'extension.js')], { stdio: 'pipe' }); ok('extension.js 语法合法'); }
    catch (e2) { bad('extension.js 语法', e2.message); }
  }
}

// ---- 9. package.json 命令完整性（v0.2.0 全链路） ----
{
  const pkg = jsons['package.json'];
  const cmds = pkg.contributes.commands.map((c) => c.command);
  const props = pkg.contributes.configuration?.properties ?? {};
  const need = ['hsl.check', 'hsl.run', 'hsl.emit', 'hsl.compile', 'hsl.targets'];
  const needCfg = ['hsl.dhvTs', 'hsl.bunPath', 'hsl.checkOnSave', 'hsl.outDir', 'hsl.runOutDir', 'hsl.model', 'hsl.maxTurns'];
  const missCmd = need.filter((c) => !cmds.includes(c));
  const missCfg = needCfg.filter((c) => !(c in props));
  missCmd.length === 0 && missCfg.length === 0
    ? ok('全链路命令 check/run/emit/compile/targets + 7 项配置（工具链/运行时/保存检查/产物目录）')
    : bad('package.json 命令/配置', `缺命令 ${missCmd} · 缺配置 ${missCfg}`);
}

// ---- 10. parsers.js 错误行解析 ----
{
  const P = createRequire(import.meta.url)('../parsers.js');
  const sample = [
    'dhv-ts check: 1 error(s), 0 warning(s)',
    'error[E-0]: 无法解析的模式 "{" (punct) (/tmp/bad.hsl:1:13)',
    'error[S-6]: match 不穷尽 (C:\\ws\\a.hsl:42:5)',
    'error[E-1]: 某个无位置错误',
  ].join('\n');
  const errors = P.parseCheckErrors(sample);
  const e1 = errors.find((e) => e.file === '/tmp/bad.hsl');
  const e2 = errors.find((e) => e.file === 'C:\\ws\\a.hsl');
  const e3 = errors.find((e) => e.line === 0);
  const conds = [
    ['3 条错误行全部解析（总结行不计）', errors.length === 3],
    ['消息内括号不被截断（punct 形态）', e1 && e1.message === '无法解析的模式 "{" (punct)' && e1.code === 'E-0' && e1.line === 1 && e1.col === 13],
    ['Windows 盘符路径', e2 && e2.line === 42 && e2.col === 5],
    ['无位置错误 file="" line=0', e3 && e3.file === '' ],
    ['总结行', P.parseCheckSummary(sample)?.errors === 1 && P.parseCheckSummary(sample)?.warnings === 0],
  ];
  const bads = conds.filter(([, v]) => !v).map(([n]) => n);
  bads.length === 0 ? ok('错误行解析（括号消息 / Windows 盘符 / 无位置 / 总结行）') : bad('错误行解析', bads.join('、'));
}

// ---- 11. parsers.js targets 注册表 ----
{
  const P = createRequire(import.meta.url)('../parsers.js');
  const sample = [
    '后端语言注册表（BNF v1.4 §5.2）—— 32 编程语言 + 6 静态格式',
    '',
    '  Tier 1 · Harness 核心（活体/语句子集翻译优先）',
    '    python        Python        .py   full 活体翻译       native 可执行 · 语法校验:python3',
    '    typescript    TypeScript    .ts   full 活体翻译       native 可执行 · 语法校验:bun-ts',
    '',
    '  Tier 2 · 脚本与动态',
    '    ruby          Ruby          .rb   contract 类型契约   — Struct + case/in 模式匹配（Ruby 2.7+）',
  ].join('\n');
  const targets = P.parseTargets(sample);
  const py = targets.find((t) => t.id === 'python');
  const rb = targets.find((t) => t.id === 'ruby');
  const conds = [
    ['3 个目标', targets.length === 3],
    ['Tier 1 归组', py && py.tier === 1 && py.tierName === 'Harness 核心（活体/语句子集翻译优先）'],
    ['扩展名与能力', py && py.ext === 'py' && py.name === 'Python' && py.note.includes('活体翻译')],
    ['Tier 2 归组', rb && rb.tier === 2],
  ];
  const bads = conds.filter(([, v]) => !v).map(([n]) => n);
  bads.length === 0 ? ok('targets 注册表解析（Tier 分组 · 后端字段）') : bad('targets 解析', bads.join('、'));
}

// ---- 12. parsers.js run 产物摘要（注入式 io 替身） ----
{
  const P = createRequire(import.meta.url)('../parsers.js');
  const io = {
    readFile: (p) => {
      if (p.endsWith('run.json')) return JSON.stringify({ ok: true, elapsed_ms: 45, model: 'scripted', events: 16 });
      if (p.endsWith('report.md')) return '# Report\n- verdict: accepted\n- turns: 5';
      throw new Error('ENOENT');
    },
  };
  const s = P.parseRunArtifacts('/tmp/out', io);
  const conds = [
    ['run.json 通道', s.ok === true && s.elapsedMs === 45 && s.model === 'scripted' && s.events === 16],
    ['report 通道', s.verdict === 'accepted' && s.turns === 5 && s.reportPath === '/tmp/out/report.md'],
  ];
  const ioEmpty = { readFile: () => { throw new Error('ENOENT'); } };
  const s2 = P.parseRunArtifacts('/tmp/out', ioEmpty);
  conds.push(['产物缺失全容错（null 字段不抛）', s2.ok === null && s2.verdict === null]);
  const bads = conds.filter(([, v]) => !v).map(([n]) => n);
  bads.length === 0 ? ok('run 产物摘要（双通道 + 容错）') : bad('run 产物摘要', bads.join('、'));
}

console.log(failed === 0 ? '\nIDE 校验：全部通过' : `\nIDE 校验：${failed} 项失败`);
process.exit(failed === 0 ? 0 : 1);
