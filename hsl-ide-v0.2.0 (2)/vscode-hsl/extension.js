/**
 * HSL IDE — VSCode 扩展入口（v0.1.0 骨架）
 *
 * 提供命令：hsl.check（dhv check）/ hsl.compile（dhv emit）
 * 语法高亮 / 语言配置 / 片段 / 主题由 contributes 声明静态注册。
 */
const vscode = require('vscode');

function activate(context) {
  const config = () => vscode.workspace.getConfiguration('hsl');
  const dhvPath = () => config().get('dhvPath', 'dhv');

  const output = vscode.window.createOutputChannel('HSL / DHV');
  context.subscriptions.push(output);

  async function runDhv(args, fileUri) {
    const file = fileUri ? fileUri.fsPath : vscode.window.activeTextEditor?.document.uri.fsPath;
    if (!file || !file.endsWith('.hsl')) {
      vscode.window.showWarningMessage('HSL: 请先打开一个 .hsl 文件');
      return;
    }
    output.show(true);
    output.appendLine(`$ ${dhvPath()} ${args} "${file}"`);
    // 骨架：通过终端任务调用 dhv；LSP 集成在路线图 P9 之后
    const task = new vscode.Task(
      { type: 'hsl', args },
      vscode.TaskScope.Workspace,
      `dhv ${args}`,
      'HSL',
      new vscode.ShellExecution(dhvPath(), [...args.split(' '), file])
    );
    const execution = await vscode.tasks.executeTask(task);
    const disposable = vscode.tasks.onDidEndTaskProcess((e) => {
      if (e.execution === execution) {
        const code = e.exitCode;
        output.appendLine(`dhv 退出码: ${code}`);
        if (code === 0) {
          vscode.window.setStatusBarMessage(`HSL ✓ (${code})`, 4000);
        } else {
          vscode.window.showErrorMessage(`HSL: dhv ${args} 失败（退出码 ${code}），详见输出面板`);
        }
        disposable.dispose();
      }
    });
  }

  context.subscriptions.push(
    vscode.commands.registerCommand('hsl.check', (uri) => runDhv('check', uri)),
    vscode.commands.registerCommand('hsl.compile', (uri) => runDhv('emit', uri))
  );

  // 状态栏：当前编译尺度
  const statusItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
  statusItem.text = `HSL · scale: ${config().get('scale', 'monolith')}`;
  statusItem.tooltip = 'HSL 编译尺度（hsl.scale 配置）';
  statusItem.show();
  context.subscriptions.push(statusItem);
}

function deactivate() {}

module.exports = { activate, deactivate };
