# HSL IDE for VSCode — v0.1.0

> HSL 语言的官方 IDE 支持包。**基于 VSCode**：不重造编辑器，砍掉无关冗余，
> 专注 HSL / DHV 工程体验。语法高亮、语言配置、代码片段、琥珀 Keynote 暗色主题。

## 安装

### 方式一：装入现有 VSCode（推荐）

```bash
# 若拿到 .vsix 包：
code --install-extension hsl-ide-0.1.0.vsix

# 或直接以扩展开发模式运行本目录：
code --extensionDevelopmentPath=/path/to/vscode-hsl
```

打开任意 `.hsl` 文件即获得完整高亮与片段。

### 方式二：构建 HSL IDE 独立发行版（VSCode OSS fork）

HSL IDE 的完整形态基于 [Code OSS](https://github.com/microsoft/vscode) 构建，
砍掉遥测/市场/账号等不需要的区块，内置 HSL 扩展与主题：

```bash
git clone https://github.com/microsoft/vscode hsl-ide
cd hsl-ide
# 1. 把本扩展内置为默认扩展
mkdir -p extensions/hsl-ide && cp -r ../vscode-hsl/* extensions/hsl-ide/
# 2. 品牌化（product.json：name → HSL IDE，applicationName → hsl-ide）
# 3. 构建并打包各平台安装包
npm install && gulp vscode-darwin-arm64   # 或 vscode-linux-x64 / vscode-win32-x64
```

保持 VSCode 原生工作台观感（不引入花哨界面）——命令面板、侧栏、终端、
源代码管理一概保留，仅面向 HSL 工程做默认配置优化。

## 功能清单

| 功能 | 状态 |
|:---|:---|
| `.hsl` 语法高亮（graph / edge / project / scale / native / block / 插值 / 宏 / 属性 / 全进制数字） | ✅ |
| 语言配置（括号配对、auto-close `{{ }}`、缩进规则、折叠） | ✅ |
| 代码片段（graph 骨架 / project 投射 / native 逃生舱 / block / edge / capability） | ✅ |
| HSL Dark（琥珀 Keynote）主题 | ✅ |
| `HSL: Type Check`（调用 dhv check） | ✅ 骨架 |
| `HSL: Compile`（调用 dhv emit） | ✅ 骨架 |
| 状态栏 scale 指示（monolith / microkernel） | ✅ |
| LSP（诊断 / 跳转 / 悬停 / 补全，dhv --lsp） | 🗓 路线图 P9 |
| project 投射的多语言文件树视图 | 🗓 路线图 P8 |
| SourceMap 围栏可视化（可编辑区高亮） | 🗓 路线图 P6 |

## 语法高亮要点

与 `hsl-spec/BNF.md` 对齐的 TextMate 文法（`syntaxes/hsl.tmLanguage.json`）：

- **HSL 专属构件加粗琥珀**：`graph` `edge` `node` `on` `with` `project` `scale` `native`
- **投射行专用高亮**：`Planner -> "src/planner.py" : python`（路径暖黄、语言常量粉色）
- **插值块**：`{{ expr }}` 以琥珀加粗定界
- **内建类型**（`Result` `Option` `Box` `Vec` …）emerald 绿
- 全进制数字（`0xFF` `0o77` `0b1010` + `u8`/`i64`/`f32` 后缀）

## License

MIT
