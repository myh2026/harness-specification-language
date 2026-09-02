#!/usr/bin/env bun
// ============================================================================
// tests/verify_backends.ts — 38 后端全量覆盖校验（quarter / daily 定时任务共用）
// ----------------------------------------------------------------------------
// 用法：bun tests/verify_backends.ts <emit-out-dir>
// 校验：
//   1. manifest 全部文件 syntax_check == pass
//   2. emit 零告警
//   3. 注册表 ALL_LANGS（32 编程语言 + 6 静态格式 = 38）↔ manifest 语言集合
//      双向相等 —— 每个后端都被真实投射过，产物中也不出现未注册语言
//   4. 静态 JSON 后端产物内容级校验（真实 JSON.parse）
// 退出码：0 = 全部通过；1 = 任一失败；2 = 用法错误。
// ============================================================================
import * as fs from 'node:fs';
import * as path from 'node:path';
import { ALL_LANGS } from '../dhv-ts/src/backends/registry';

const outDir = process.argv[2];
if (!outDir) {
  console.error('用法: bun tests/verify_backends.ts <emit-out-dir>');
  process.exit(2);
}

const manifest = JSON.parse(fs.readFileSync(path.join(outDir, 'manifest.json'), 'utf8'));

let failed = 0;
const ok = (m: string) => console.log('  ✓ ' + m);
const bad = (m: string) => { failed++; console.error('  ✗ ' + m); };

// ---- 1. 全部文件语法校验通过 ----
{
  const badSyntax = manifest.files.filter((f: any) => f.syntax_check !== 'pass');
  if (badSyntax.length === 0) ok(`全部 ${manifest.files.length} 个文件语法校验通过`);
  else bad(`${badSyntax.length} 个文件语法校验未通过：${badSyntax.map((f: any) => f.path).join(', ')}`);
}

// ---- 2. 零告警 ----
{
  if (Array.isArray(manifest.warnings) && manifest.warnings.length === 0) ok('emit 零告警');
  else bad('emit 告警：' + JSON.stringify(manifest.warnings));
}

// ---- 3. 注册表 ↔ 产物语言集合双向相等 ----
{
  const got: Set<string> = new Set(manifest.files.map((f: any) => f.lang));
  const expect: Set<string> = new Set(ALL_LANGS.map((l) => l.id));
  const missing = [...expect].filter((l) => !got.has(l));
  const extra = [...got].filter((l) => !expect.has(l));
  if (missing.length === 0 && extra.length === 0) {
    ok(`38 后端全覆盖（32 编程语言 + 6 静态格式），产物语言数 ${got.size}，与注册表双向一致`);
  } else {
    if (missing.length) bad(`未被投射的后端（${missing.length} 个）: ${missing.join(', ')}`);
    if (extra.length) bad(`产物中出现未注册语言: ${extra.join(', ')}`);
  }
}

// ---- 4. 静态 JSON 后端内容级校验 ----
{
  let parsed = 0;
  for (const f of manifest.files.filter((f: any) => f.lang === 'json')) {
    try { JSON.parse(fs.readFileSync(path.join(outDir, f.path), 'utf8')); parsed++; }
    catch (e) { bad(`JSON 产物解析失败: ${f.path} — ${(e as Error).message}`); }
  }
  if (parsed > 0) ok(`静态 JSON 后端内容级校验：${parsed} 个文件全部可解析`);
}

if (failed === 0) console.log('✓ 38 后端全量覆盖校验通过');
else { console.error(`✗ ${failed} 项失败`); process.exit(1); }
