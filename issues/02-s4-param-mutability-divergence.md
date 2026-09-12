# Issue 文案：S-4 双端分歧——graph/fn 非 mut 参数被赋值，dhv-ts 放行、dhv(Rust) 拦截

> 粘贴到 GitHub → New issue。已随 commit `1f03a4c`（分支 `fix/practical-fixes-0.2.62`，v0.2.63）修复，合并后可关闭。

---

**标题**：[双端分歧] 非 mut 参数被赋值：dhv-ts 漏报 S-4（exit 0），dhv(Rust) 正确拦截（exit 1）

**标签**：bug / conformance / dual-engine

**正文**：

## 复现（最小化）

`/tmp/s4-gparam2.hsl`：

```hsl
graph G(x: Int) -> Int {
    x = 5;
    loop {
        let action = Action::Stop;
        match action {
            Action::Stop => { break; }
        }
    }
    return x;
}
```

双端 check 结果：

| 编译器 | 结果 | 诊断 |
|---|---|---|
| dhv-ts | **exit 0（通过）** | 0 error, 0 warning |
| dhv(Rust) | **exit 1** | `ERROR[S-S4] 2:5: 不能对不可变绑定 \`x\` 赋值（S4 不可变优先）` |

无 loop 的变体（`graph G(x: Int) -> Int { x = 5; return x; }`）两端都 fail，但 dhv-ts 只报 G-1 缺 AgentLoop、**仍不报 S-4** —— 失败原因集合不同，对拍结论却相同，暴露 conformance「只对拍通过/失败结论」的盲区。

## 根因（已定位）

`toolchain/dhv-ts/src/checker.ts` `declareParam()`（v0.2.62 行号 ~537-541）：

```ts
/** 声明参数绑定：参与 S-4/E-2 作用域解析，但豁免 S-7（未使用参数是合法风格） */
function declareParam(scope: Scope, name: string): void {
  if (name === '_' || name.startsWith('_')) return;
  if (scope.vars.has(name)) return;
  scope.vars.set(name, { mut: true, span: { line: 0, col: 0, file: '' }, param: true });
}
```

**所有参数一律 `mut: true`**，无视声明处是否带 `mut`（如 `graph Researcher(mut task: Task, question: String)` 中 `question` 未声明 mut）。而 S-4 检查依赖作用域表 `hit.mut` —— 参数恒可变 → 对非 mut 参数赋值不触发。

对照 dhv(Rust) `parser.rs`：`mut fuel` / `mut state` 的可变性正确传入模式绑定，默认不可变。

语言语义以 BNF 为准：**参数默认不可变，显式 `mut` 才可变**（nova/dsh 语料均按此惯用法书写）。

## 影响面

- `graph` 参数与 `fn` 参数（`declareParam` 共用）在 dhv-ts 端都恒可变；
- dhv-ts 是参考解释器 —— 语义以它为锚的下游（38 后端投射、org 铸出专家闸门）会把非法赋值放行到生成工程里。

## 修复方案（已实施，commit 1f03a4c / v0.2.63）

1. ✅ AST 参数节点携带 `mut` 标记（parser 原已带：`FnParam.mut` / `GraphParam.mut`）；
2. ✅ `declareParam(scope, name, mut)`：`mut: false` 默认，显式 `mut` 参数才 `mut: true`；graph 参数取 `GraphParam.mut`，fn/impl/trait 参数经新增 `fnParamBindings()`（self 按 kind 映射：mutvalue/refmut 可变，value/ref 不可变）；
3. ✅ 回归 4 例：`graph` / `fn` 各一例非 mut 参数赋值 → dhv-ts 报 S-4；显式 mut 参数赋值 → 两端都通过；
4. ✅ v0.2.62 两个 G-8 回归 fixture 此前依赖本漏报才通过（dhv(Rust) 同源码本就报 S-S4），参数改 `mut x`；
5. ⬜ conformance 语料补「graph 非 mut 参数被赋值」+ 对拍粒度从「通过/失败结论」升级为「诊断码集合」——**待办**（本轮仅记录于 CHANGELOG）。

## 佐证：conformance 语料盲区

现语料两端结论一致（均为通过或均为失败但同因），「graph/fn 参数可变性」维度无用例。本次实测（沙盒全链路装机对拍）系偶然触碰。
