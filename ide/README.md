# HSL IDE for VSCode — v0.2.0（全链路）

> HSL 语言的官方 IDE 支持包。**基于 VSCode**：不重造编辑器，砍掉无关冗余，
> 专注 HSL / DHV 工程体验。v0.2.0 起为**全链路形态**：打开 IDE → 直接编写
> HSL → **check（问题面板诊断）→ run（agent 执行 + 报告）→ emit（38 后端
> 投射）** 全部就地完成。vsix 内置捆绑 dhv-ts 解释器（零依赖纯 TS）——
> 安装扩展即得完整工具链，唯一运行时前置是 [bun](https://bun.sh)。

## 安装（开箱即用）

### 方式一：装入现有 VSCode（推荐）

```bash
# 下载 hsl-ide-<ver>.vsix（GitHub Release 附带产物）后：
code --install-extension hsl-ide-0.2.0.vsix

# 前置：安装 bun（工具链运行时）
curl -fsSL https://bun.sh/install | bash    # macOS / Linux
powershell -c "irm bun.sh/install.ps1|iex"  # Windows
```

装完打开任意 `.hsl` 文件即获得完整体验：高亮、片段、保存自动 check、
命令面板里的 Run / Emit。

### 方式二：源码扩展开发模式

```bash
git clone https://github.com/myh2026/harness-specification-language
cd harness-specification-language
code --extensionDevelopmentPath=$PWD/ide .
# 工具链自动解析到 <仓库>/toolchain/dhv-ts（无需打包）
```

### 方式三：本地打 vsix

```bash
bun scripts/package-ide.ts
# → dist-ide/hsl-ide-0.2.0.vsix（223 KB · 45 文件 · 含捆绑 dhv-ts）
```

### 方式四：构建 HSL IDE 独立发行版（VSCode OSS fork）

HSL IDE 的完整形态基于 [Code OSS](https://github.com/microsoft/vscode) 构建，
砍掉遥测/市场/账号等不需要的区块，内置 HSL 扩展与主题：

```bash
git clone https://github.com/microsoft/vscode hsl-ide
cd hsl-ide
mkdir -p extensions/hsl-ide && cp -r ../harness-specification-language/ide/* extensions/hsl-ide/
npm install && gulp vscode-darwin-arm64   # 或 vscode-linux-x64 / vscode-win32-x64
```

## 全链路命令（v0.2.0）

| 命令 | 快捷键 | 做什么 |
|:---|:---|:---|
| `HSL: Type Check` | `Ctrl+Alt+F7` | `dhv-ts check`：错误行解析进**问题面板**（诊断标记带行列）；状态栏实时 ✓/✗ |
| `HSL: Run` | `Ctrl+Alt+F8` | `dhv-ts run`：交互收集 模型（scripted/deepseek）→ 任务 → 剧本；输出流式进输出面板；结束弹出摘要（verdict / turns / 耗时）+ **一键打开 report.md** |
| `HSL: Emit to Backend…` | `Ctrl+Alt+F9` | `dhv-ts emit`：**38 后端 QuickPick**（Tier 分组 + 能力级详情）→ 产物写入 `.hsl-gen/` → 资源管理器揭示 + manifest.json 打开 |
| `HSL: Show Targets` | — | 38 后端注册表浏览器（tier / 扩展名 / 能力级 / 语法校验器） |
| `HSL: Compile` | — | emit 兼容别名（v0.1.x 命令保留） |
| 保存自动 check | — | `hsl.checkOnSave`（默认开，防抖 400ms，静默只更新诊断） |

## 工具链解析（五级）

`bun <main.ts>` 的 `main.ts` 按以下顺序解析（状态栏 tooltip 可见当前命中）：

1. 配置 `hsl.dhvTs`（显式路径）
2. 环境变量 `DHV_TS`
3. **vsix 捆绑副本**（`<扩展目录>/dhv-ts/src/main.ts` —— 安装即用）
4. 工作区 `<ws>/toolchain/dhv-ts/src/main.ts`（打开了 HSL 仓库时）
5. 兄弟仓库克隆（`../harness-specification-language` / `../hsl`）

运行时 bun 不在场时：给出安装指引（https://bun.sh），可用 `hsl.bunPath`
指向已有二进制。

## 配置项

| 键 | 默认 | 说明 |
|:---|:---|:---|
| `hsl.dhvTs` | `"auto"` | 工具链入口路径（auto = 五级解析） |
| `hsl.bunPath` | `"bun"` | bun 可执行文件 |
| `hsl.checkOnSave` | `true` | 保存自动 check |
| `hsl.model` | `"scripted"` | Run 默认模型 |
| `hsl.maxTurns` | `24` | Run 的 AgentLoop 上限 |
| `hsl.scale` | `"monolith"` | Emit 编译尺度 |
| `hsl.outDir` | `".hsl-gen"` | Emit 产物目录 |
| `hsl.runOutDir` | `".hsl-runs"` | Run 产物目录（report / transcript / events） |

## 功能清单

| 功能 | 状态 |
|:---|:---|
| `.hsl` 语法高亮（graph / edge / project / scale / native / block / 插值 / 宏 / 属性 / 全进制数字） | ✅ |
| 语言配置（括号配对、auto-close `{{ }}`、缩进规则、折叠） | ✅ |
| 代码片段（graph 骨架 / project 投射 / native 逃生舱 / block / edge / capability） | ✅ |
| HSL Dark（琥珀 Keynote）主题 | ✅ |
| **check → 问题面板诊断**（错误行 file:line:col 解析；含消息内括号 / Windows 盘符路径） | ✅ v0.2.0 |
| **run → 流式执行 + 摘要 + report.md 一键打开** | ✅ v0.2.0 |
| **emit → 38 后端 QuickPick（Tier 分组）+ 产物揭示** | ✅ v0.2.0 |
| **保存自动 check**（防抖 · 静默） | ✅ v0.2.0 |
| **vsix 捆绑工具链**（安装即用，仅需 bun） | ✅ v0.2.0 |
| LSP（诊断 / 跳转 / 悬停 / 补全，dhv --lsp） | 🗓 路线图 P9 |
| project 投射的多语言文件树视图 | 🗓 路线图 P8 |
| SourceMap 围栏可视化（可编辑区高亮） | 🗓 路线图 P6 |

## 语法高亮要点

与 `hsl-spec/BNF.md` 对齐的 TextMate 文法（`syntaxes/hsl.tmLanguage.json`）：

- **HSL 专属构件加粗琥珀**：`graph` `edge` `node` `on` `with` `project` `scale` `native`
- **投射行专用高亮**：`Planner -> "src/planner.py" : python`（路径暖黄、语言常量粉色）
- **插值块**：`{{ expr }}` 以琥珀加粗定界
- **内建类型**（`Result` `Option` `Box` `Vec` …）emerald 绿

## 测试与打包

```bash
bun ide/tests/validate.js   # 发布级校验（12 项：JSON / regex / 解析器单测 / 命令完整性）
bun scripts/package-ide.ts  # 打 vsix（捆绑 dhv-ts → vsce → 清理）
```

CI：`ci.yml` 的 `ide` job 每次推送跑校验 + 打包 artifact；`release.yml`
自动把 vsix 附到 GitHub Release（与 dhv 四平台二进制同批）。
