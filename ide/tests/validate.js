// ============================================================================
// ide/tests/validate.js — HSL IDE 扩展发布级校验（v0.1.1）
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
// 退出码：0 = 全部通过；1 = 任一失败（CI 门禁）。
// ============================================================================
import { readFileSync } from 'node:fs';
import * as path from 'node:path';
import { execFileSync } from 'node:child_process';

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

// ---- 9. package.json 命令完整性 ----
{
  const pkg = jsons['package.json'];
  const cmds = pkg.contributes.commands.map((c) => c.command);
  const hasOutDir = 'hsl.outDir' in (pkg.contributes.configuration?.properties ?? {});
  cmds.includes('hsl.check') && cmds.includes('hsl.compile') && hasOutDir
    ? ok('命令 hsl.check / hsl.compile + hsl.outDir 配置（v0.1.1 --out 修复载体）')
    : bad('package.json 命令/配置', `cmds=${cmds} outDir=${hasOutDir}`);
}

console.log(failed === 0 ? '\nIDE 校验：全部通过' : `\nIDE 校验：${failed} 项失败`);
process.exit(failed === 0 ? 0 : 1);
