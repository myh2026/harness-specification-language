// ============================================================================
// ide/extension.js — HSL IDE 扩展入口（v0.2.0 全链路）
// ----------------------------------------------------------------------------
// 定位：打开 IDE → 直接编写 HSL → check / run / emit 全链路就地完成。
//   HSL: Type Check   —— dhv-ts check，错误行解析进「问题」面板（诊断标记）
//   HSL: Run          —— dhv-ts run（任务/模型/剧本交互式收集，产物摘要 +
//                          report.md 一键打开；支持 deepseek 真实模型）
//   HSL: Emit         —— 38 后端 QuickPick（Tier 分组）→ dhv-ts emit →
//                          资源管理器揭示产物目录 + manifest 打开
//   HSL: Show Targets —— 注册表浏览器（后端 / 扩展名 / 能力级）
//   保存自动 check（hsl.checkOnSave，默认开）
// 工具链解析（五级）：hsl.dhvTs 配置 → DHV_TS 环境变量 → vsix 捆绑副本
// （extension 同目录 dhv-ts/）→ 工作区 toolchain/dhv-ts → 兄弟仓库克隆。
// 运行时：bun（hsl.bunPath 可覆盖；缺失时给出安装指引）。
// 纯解析逻辑在 ./parsers.js（无 vscode 依赖，validate.js 单测）。
// ============================================================================
'use strict';

const vscode = require('vscode');
const cp = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const { parseCheckErrors, parseCheckSummary, parseTargets, parseRunArtifacts } = require('./parsers');

const OUTPUT_CHANNEL = 'HSL / DHV';
const isHsl = (f) => typeof f === 'string' && f.endsWith('.hsl');

let output = null;
let diagnostics = null;
let statusItem = null;
let bunDetected = null; // null = 未检测；true/false = 结果

function cfg() { return vscode.workspace.getConfiguration('hsl'); }

// ---------- 工具链解析（五级候选，返回 {main, source} 或 null） ----------
function resolveToolchain() {
  const extDir = path.dirname(__dirname);
  const wsRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  const candidates = [];
  const explicit = cfg().get('dhvTs', 'auto');
  if (explicit && explicit !== 'auto') candidates.push({ main: explicit, source: '配置 hsl.dhvTs' });
  if (process.env.DHV_TS) candidates.push({ main: process.env.DHV_TS, source: '环境变量 DHV_TS' });
  candidates.push({ main: path.join(extDir, 'dhv-ts', 'src', 'main.ts'), source: 'vsix 捆绑工具链' });
  if (wsRoot) {
    candidates.push({ main: path.join(wsRoot, 'toolchain', 'dhv-ts', 'src', 'main.ts'), source: '工作区 toolchain/' });
    candidates.push({ main: path.join(path.dirname(wsRoot), 'harness-specification-language', 'toolchain', 'dhv-ts', 'src', 'main.ts'), source: '兄弟仓库 harness-specification-language' });
    candidates.push({ main: path.join(path.dirname(wsRoot), 'hsl', 'toolchain', 'dhv-ts', 'src', 'main.ts'), source: '兄弟仓库 hsl' });
  }
  for (const c of candidates) {
    try { if (fs.existsSync(c.main)) return c; } catch { /* 权限等异常 → 下一候选 */ }
  }
  return null;
}

function bunPath() { return cfg().get('bunPath', 'bun'); }

/** 检测 bun 可用（结果缓存；扩展生命周期内只探一次）。 */
function detectBun() {
  if (bunDetected !== null) return Promise.resolve(bunDetected);
  return new Promise((resolve) => {
    cp.execFile(bunPath(), ['--version'], { timeout: 5000 }, (err, stdout) => {
      bunDetected = !err && /^\d+\./.test(String(stdout).trim());
      if (!bunDetected) {
        vscode.window.showErrorMessage(
          `HSL IDE 需要运行时 bun（当前命令「${bunPath()}」不可用）。安装：https://bun.sh ，或用 hsl.bunPath 配置指向已有 bun。`,
        );
      }
      resolve(bunDetected);
    });
  });
}

// ---------- 子进程执行（行级流式 → 输出面板） ----------
function runTool(args, opts = {}) {
  const tool = opts.tool ?? resolveToolchain();
  return new Promise((resolve) => {
    if (!tool) {
      resolve({ code: 2, stdout: '', stderr: '找不到 dhv-ts 工具链：安装 HSL IDE vsix（内置捆绑）或打开 HSL 仓库，或设 hsl.dhvTs / DHV_TS。' });
      return;
    }
    const proc = cp.spawn(bunPath(), [tool.main, ...args], { cwd: opts.cwd });
    let stdout = '';
    let stderr = '';
    const onChunk = (buf, isErr) => {
      const text = buf.toString();
      if (isErr) stderr += text; else stdout += text;
      if (!opts.quiet) output.append(text);
    };
    proc.stdout.on('data', (b) => onChunk(b, false));
    proc.stderr.on('data', (b) => onChunk(b, true));
    proc.on('error', (e) => resolve({ code: 1, stdout, stderr: stderr + String(e) }));
    proc.on('close', (code) => resolve({ code: code ?? 1, stdout, stderr }));
  });
}

// ---------- 诊断（check → Problems 面板） ----------
function applyDiagnostics(docUri, errors) {
  const byFile = new Map();
  for (const e of errors) {
    let uri = docUri;
    if (e.file) {
      try {
        const abs = path.isAbsolute(e.file) ? e.file : path.resolve(path.dirname(docUri.fsPath), e.file);
        if (fs.existsSync(abs)) uri = vscode.Uri.file(abs);
      } catch { /* 路径解析失败 → 挂到当前文件 */ }
    }
    if (!byFile.has(uri)) byFile.set(uri, []);
    const line = Math.max(0, (e.line || 1) - 1);
    const col = Math.max(0, (e.col || 1) - 1);
    const range = new vscode.Range(line, col, line, col + 1);
    byFile.get(uri).push(new vscode.Diagnostic(range, `${e.message} [${e.code}]`, vscode.DiagnosticSeverity.Error));
  }
  diagnostics.clear();
  for (const [uri, list] of byFile) diagnostics.set(uri, list);
}

async function doCheck(uri, opts = {}) {
  const file = uri ? uri.fsPath : vscode.window.activeTextEditor?.document.uri.fsPath;
  if (!file || !isHsl(file)) {
    vscode.window.showWarningMessage('HSL: 请先打开一个 .hsl 文件');
    return;
  }
  if (!opts.quiet) { output.show(true); output.appendLine(`$ dhv-ts check ${path.basename(file)}`); }
  const r = await runTool(['check', file], { quiet: opts.quiet });
  const errors = parseCheckErrors(r.stdout + r.stderr);
  applyDiagnostics(vscode.Uri.file(file), errors);
  const summary = parseCheckSummary(r.stdout);
  if (summary && summary.errors === 0) {
    statusItem.text = `$(check) HSL 0 errors`;
    statusItem.tooltip = 'dhv-ts check 通过';
    statusItem.color = undefined;
  } else if (errors.length > 0 || (summary && summary.errors > 0)) {
    const n = Math.max(errors.length, summary ? summary.errors : 0);
    statusItem.text = `$(error) HSL ${n} error${n === 1 ? '' : 's'}`;
    statusItem.tooltip = 'dhv-ts check 未通过 —— 详见问题面板';
    statusItem.color = new vscode.ThemeColor('errorForeground');
    if (!opts.quiet) vscode.window.showErrorMessage(`HSL: check 未通过（${n} 个错误），详见问题面板`);
  } else {
    statusItem.text = '$(clock) HSL';
  }
  if (!opts.quiet) output.appendLine(`dhv-ts 退出码: ${r.code}`);
}

// ---------- run（交互式选项 → 流式执行 → 产物摘要） ----------
async function doRun(uri) {
  const file = uri ? uri.fsPath : vscode.window.activeTextEditor?.document.uri.fsPath;
  if (!file || !isHsl(file)) { vscode.window.showWarningMessage('HSL: 请先打开一个 .hsl 文件'); return; }
  if (!(await detectBun())) return;

  // 选项收集：模型 → 任务 →（scripted 时）剧本
  const model = await vscode.window.showQuickPick(
    [
      { label: 'scripted', description: '确定性剧本（CI 可复现；需 --fixture）', picked: cfg().get('model', 'scripted') === 'scripted' },
      { label: 'deepseek', description: '真实 LLM（$host.llm 网关 · GLM-4.5）', picked: cfg().get('model', 'scripted') === 'deepseek' },
    ],
    { placeHolder: '运行模式（模型）' },
  );
  if (!model) return;
  const task = await vscode.window.showInputBox({
    prompt: '任务描述（--task；AgentLoop 型 harness 必填）',
    value: 'demo task',
  });
  if (task === undefined) return;
  let fixture = '';
  if (model.label === 'scripted') {
    fixture = await vscode.window.showInputBox({
      prompt: '剧本 JSON 路径（--fixture；scripted 模式的模型响应录制，可留空）',
      placeHolder: '例如 examples/dsh/fixtures/fix-variance.json',
    }) ?? '';
    if (fixture === undefined) return;
  }

  const wsRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? path.dirname(file);
  const workspace = path.dirname(file); // 工作区 = 文件所在目录（路径监狱边界）
  const outDir = path.join(wsRoot, cfg().get('runOutDir', '.hsl-runs'), new Date().toISOString().replace(/[-:TZ.]/g, '').slice(0, 14));
  const args = [
    'run', file,
    '--workspace', workspace,
    '--task', task,
    '--model', model.label,
    '--out', outDir,
    '--max-turns', String(cfg().get('maxTurns', 24)),
  ];
  if (fixture && fixture.trim().length > 0) args.push('--fixture', fixture.trim());

  output.show(true);
  output.appendLine(`$ dhv-ts ${args.join(' ')}`);
  vscode.window.withProgress({ location: vscode.ProgressLocation.Notification, title: 'HSL run 执行中…' }, () => new Promise((res) => setTimeout(res, 50)));
  const r = await runTool(args);
  const summary = parseRunArtifacts(outDir, { readFile: (p) => fs.readFileSync(p, 'utf-8') });
  output.appendLine(`dhv-ts 退出码: ${r.code}`);

  const bits = [];
  if (summary.ok !== null) bits.push(summary.ok ? 'ok' : 'Err');
  if (summary.verdict) bits.push(`verdict=${summary.verdict}`);
  if (summary.turns !== null) bits.push(`${summary.turns} turns`);
  if (summary.elapsedMs !== null) bits.push(`${summary.elapsedMs} ms`);
  const line = bits.length > 0 ? bits.join(' · ') : `退出码 ${r.code}`;
  const ok = r.code === 0;
  const choice = await vscode.window.showInformationMessage(
    `HSL run ${ok ? '✓' : '✗'} ${line}`,
    ...(summary.reportPath ? ['查看报告 (report.md)'] : []),
    ...(ok ? [] : ['查看输出']),
  );
  if (choice?.startsWith('查看报告')) {
    vscode.window.showTextDocument(vscode.Uri.file(summary.reportPath));
  } else if (choice === '查看输出') {
    output.show(true);
  }
}

// ---------- emit（38 后端 QuickPick → 产物揭示） ----------
let targetsCache = null;

async function loadTargets() {
  if (targetsCache) return targetsCache;
  if (!(await detectBun())) return null;
  const r = await runTool(['targets'], { quiet: true });
  targetsCache = parseTargets(r.stdout);
  return targetsCache;
}

async function doEmit(uri) {
  const file = uri ? uri.fsPath : vscode.window.activeTextEditor?.document.uri.fsPath;
  if (!file || !isHsl(file)) { vscode.window.showWarningMessage('HSL: 请先打开一个 .hsl 文件'); return; }
  if (!(await detectBun())) return;

  const targets = await loadTargets();
  const wsRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? path.dirname(file);
  const outDir = path.join(wsRoot, cfg().get('outDir', '.hsl-gen'));

  let target = null;
  if (targets && targets.length > 0) {
    const items = [];
    let lastTier = -1;
    for (const t of targets) {
      if (t.tier !== lastTier) {
        items.push({ label: `Tier ${t.tier} · ${t.tierName}`, kind: vscode.QuickPickItemKind.Separator });
        lastTier = t.tier;
      }
      items.push({ label: t.id, description: `${t.name} · .${t.ext}`, detail: t.note, target: t });
    }
    const picked = await vscode.window.showQuickPick(items, {
      placeHolder: '选择投射目标（38 后端；↑↓ 浏览，直接键入过滤）',
      matchOnDescription: true,
      matchOnDetail: true,
    });
    if (!picked) return;
    target = picked.target;
  } else {
    // 解析失败回退：手动输入目标 id
    const id = await vscode.window.showInputBox({ prompt: '投射目标 id（如 rust / python / yaml）' });
    if (!id) return;
    target = { id };
  }

  const args = ['emit', file, '--out', outDir, '--scale', cfg().get('scale', 'monolith')];
  output.show(true);
  output.appendLine(`$ dhv-ts ${args.join(' ')}`);
  const r = await runTool(args);
  output.appendLine(`dhv-ts 退出码: ${r.code}`);
  if (r.code !== 0) {
    vscode.window.showErrorMessage(`HSL: emit 失败（退出码 ${r.code}），详见输出面板`);
    return;
  }
  // 揭示产物目录 + 打开 manifest
  try {
    const manifest = vscode.Uri.file(path.join(outDir, 'manifest.json'));
    if (fs.existsSync(manifest.fsPath)) {
      await vscode.window.showTextDocument(manifest);
      const reveal = vscode.Uri.file(outDir);
      vscode.commands.executeCommand('revealInExplorer', reveal).then(undefined, () => {});
    }
  } catch { /* manifest 缺失 → 不阻塞成功提示 */ }
  vscode.window.setStatusBarMessage(`HSL emit ✓ ${target.id} → ${path.relative(wsRoot, outDir)}（manifest 已打开）`, 6000);
}

async function doTargets() {
  if (!(await detectBun())) return;
  const targets = await loadTargets();
  if (!targets || targets.length === 0) {
    output.show(true);
    output.appendLine('targets 解析失败（原始输出见上）');
    await runTool(['targets']);
    return;
  }
  const items = targets.map((t) => ({ label: t.id, description: `${t.name} · .${t.ext}`, detail: t.note }));
  const picked = await vscode.window.showQuickPick(items, {
    placeHolder: `38 后端注册表（Tier 1 活体翻译 → Tier N 契约/格式）`,
    matchOnDescription: true,
    matchOnDetail: true,
  });
  if (picked) {
    output.show(true);
    output.appendLine(`后端 ${picked.label}：${picked.description ?? ''} —— ${picked.detail ?? ''}`);
  }
}

// ---------- 生命周期 ----------
function activate(context) {
  output = vscode.window.createOutputChannel(OUTPUT_CHANNEL);
  diagnostics = vscode.languages.createDiagnosticCollection('hsl');
  context.subscriptions.push(output, diagnostics);

  statusItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
  statusItem.command = 'hsl.check';
  statusItem.name = 'HSL IDE';
  context.subscriptions.push(statusItem);

  // 状态栏显示工具链来源（五级解析的结果）
  const tool = resolveToolchain();
  statusItem.text = '$(symbol-misc) HSL';
  statusItem.tooltip = tool
    ? `HSL IDE · 工具链：${tool.source}\n${tool.main}\n（点击运行 check）`
    : 'HSL IDE · 工具链未解析（装 vsix 捆绑版或打开 HSL 仓库）';
  statusItem.show();
  output.appendLine(`HSL IDE v${context.extension.packageJSON.version} · 工具链：${tool ? tool.source + ' → ' + tool.main : '未找到'}`);

  context.subscriptions.push(
    vscode.commands.registerCommand('hsl.check', (uri) => doCheck(uri)),
    vscode.commands.registerCommand('hsl.run', (uri) => doRun(uri)),
    vscode.commands.registerCommand('hsl.emit', (uri) => doEmit(uri)),
    // v0.1.x 兼容别名：compile = emit（同一全链路终点）
    vscode.commands.registerCommand('hsl.compile', (uri) => doEmit(uri)),
    vscode.commands.registerCommand('hsl.targets', () => doTargets()),
  );

  // 保存自动 check（防抖 400ms；静默：只更新诊断与状态栏）
  let saveTimer = null;
  context.subscriptions.push(vscode.workspace.onDidSaveTextDocument((doc) => {
    if (!isHsl(doc.fileName)) return;
    if (!cfg().get('checkOnSave', true)) return;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      saveTimer = null;
      doCheck(doc.uri, { quiet: true });
    }, 400);
  }));
}

function deactivate() {}

module.exports = { activate, deactivate };
