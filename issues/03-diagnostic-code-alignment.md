# Issue 文案：双端诊断码体系对齐（码集合对拍暴露的三类差异）

> 粘贴到 GitHub → New issue。**尚未修复**；由 v0.2.63 新增的 conformance 第 7 段「诊断码集合一致性」暴露并已加 codes-exempt 豁免（结论级对拍仍生效），逐项对齐后解除豁免。

---

**标题**：[双端一致性] 诊断码命名/层级体系差异三处（M-E1 vs E-1 / E0001 vs E-0 / P5 parse vs check 层）

**标签**：conformance / dual-engine / cleanup

**正文**：

## 背景

conformance 原先只对拍「通过/失败」结论（exit code），允许诊断文案与**码**任意差异。S-4 双端分歧（v0.2.63，两端都 fail 但 ts 缺 S-4 码）证明结论级对拍存在盲区。为此新增第 7 段「诊断码集合一致性」：对 `errors/` 全量语料比对归一化后的诊断码集合（`ERROR/WARNING[X-Yn]` 大小写与 Rust 端 `X-Xn` 重复前缀归一为 ts 端 `X-n` 形态）。

升级后 100 项全过，其中暴露三类**已知且被豁免**的差异（本 issue 的修复清单）：

## 差异清单

1. **E1_duplicate_top_item.hsl**：rust 报 `M-E1`（模块域前缀拼接），ts 报 `E-1`（`checker.ts:240` `err('E-1', 重复定义顶层项)`）。
   → 对齐方向：同一诊断码跨端必须同构。建议 rust 端去掉 `M-` 域前缀（或 ts 补前缀），以 BNF/规范文档的码表为准统一。
2. **L12_unicode_escape_overflow.hsl**：rust 在 parse 层报通用 `E0001`（"期望 item_kind，得到文件结束"类解析失败），ts 在 lexer 层报 `E-0`（`\u{110000}` 溢出）。
   → 对齐方向：`\u` 溢出应在两端 lexer 层以 L-12 专用码拦截（rust parser 对 `\u{...}` 字面量的词法处理需补域校验）。
3. **P5_unknown_kind_rule.hsl**：rust parse 层 `E0001` 拦截 vs ts check 层 `P-5` 拦截（`widget -> "..." : rust` 裸规则名语法：rust parser 语法上不识别，ts 解析成功后语义层报未注册）。
   → 对齐方向：两端 parser 对 `project.rules` 裸规则名语法对齐（都接受 → 语义层 P-5；都不接受 → 语料重写），并让码集合一致。

## 豁免机制

语料首 3 行内含 `codes-exempt` 标记 → 码级对拍跳过、结论级对拍继续生效（脚本输出 `⊘ 码对拍豁免`）。**豁免不掩盖问题**：差异记录于此 issue，逐项修复后应移除标记。

## 验收标准

- `bash tests/run_conformance.sh` 7 段全过且 `⊘` 豁免数为 0；
- 上述 3 个语料移除 codes-exempt 标记后码集合对拍仍全绿。
