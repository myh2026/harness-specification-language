# fixtures/ml — HSL 机器识别语料

> 毕业论文「用 HSL 亲自操刀写机器识别项目」的语料：HSL 语言作为**一等公民**
> 实现机器学习算法（不是让 LLM 生成代码），与 `org 仓 fixtures/turing/`
> （rule110 / busy-beaver / bf）同哲学 —— **全程无 native 块**，数据解析、
> 距离度量、参数化训练循环、评估输出全部用 HSL 原语
> （`Vec` / `f64` / `match` / `for` / `while` / `format!` / `const`）。

## 语料：digit-recognizer.hsl

5×5 位图数字识别（0 / 1 / 8 三个易区分类），**数据集内嵌**在源码中：
每样本一串 25 字符（行优先，`#`=1.0 / `.`=0.0），运行时经 `parse_bitmap`
解析成 25 维 `Vec<f64>` 特征向量，并回显 `row0/row1/…/row4` 单行渲染自检。

| 划分 | 数量 | 构成 |
|---|---|---|
| 训练集 | 12 | 每类 4 变体：基准形 + 1 像素噪声（含对抗样本：`0` 中心填点向 `8` 靠近，逼迫 KNN 依赖多像素证据） |
| 测试集 | 6 | 每类 2 变体：2 像素噪声，全部不出训练集 |

### 算法一：KNN（k=3，欧氏距离，多数投票）

- 平方距离 `dist_sq` 手写循环（`{0,1}` 特征下为精确整数和，排序等价欧氏距离）；
  开方仅用于展示（`f64.sqrt()`）。
- 3 近邻表用**保序插入**维护（严格 `<` 才前插，同距先到者胜 —— 稳定排序语义），
  不依赖 `sort_by`（规避后端排序实现/类型差异）。
- 多数投票平票按类序 `0 < 1 < 8` 确定性破平。

### 算法二：感知器（is-8 二分类，margin 变体）

- 权重 `w ∈ R^26`（25 特征 + 偏置在下标 25），`[0.0; 26]` 数组重复字面量初始化。
- 标签 is-8 → `+1.0` / 其余 → `-1.0`；更新条件 `y·score < MARGIN`
  （经典感知器 `y·score ≤ 0` 的推广，间隔 1.0 —— 保证测试样本决策余量，
  避免输出停留在浮点噪声级别的边界上）；学习率 0.1，轮数上限 30，收敛即提前停。
- 输出实际轮数、累计更新次数、最终权重 L2 范数与逐测试样本得分。

### 跨后端确定性（四路径对拍的前提）

训练/推理运算全部是 IEEE 754 double 的加/乘（无超越函数参与），JS / Python /
Rust 三运行时**逐位一致**；仅展示用的开方按 `{:.3}` / `{:.4}` 定点打印，
屏蔽后端 `sqrt` 实现的 1 ulp 差异（python 后端投射为 `x ** 0.5`）。

## 四路径验证

复现命令（注意 **路径相对 CWD**，须在 `toolchain/` 下执行）：

```bash
cd toolchain
# a. 静态检查
bun dhv-ts/src/main.ts check ../fixtures/ml/digit-recognizer.hsl
# b. 解释执行（黄金输出）
bun dhv-ts/src/main.ts run ../fixtures/ml/digit-recognizer.hsl --quiet
# c. python 投射：ruff 0 error + 真实运行与 b 逐行一致
bun dhv-ts/src/main.ts emit ../fixtures/ml/digit-recognizer.hsl --out /tmp/ml-emit
ruff check --no-cache /tmp/ml-emit/digit_recognizer.py
python3 /tmp/ml-emit/digit_recognizer.py
# d. rust 投射：rustc 真实编译运行与 b 逐行一致
rustc /tmp/ml-emit/digit_recognizer.rs -o /tmp/ml-rs && /tmp/ml-rs
```

| 路径 | 结果（v0.2.70 实测） |
|---|---|
| a. `check` | ✅ 0 error / 0 warning（1 个模块） |
| b. `run`（黄金输出） | ✅ 42 行：`KNN acc=6/6` · `Perceptron acc=6/6` · `ML VERIFIED` |
| c. `emit` → python + ruff 0.16.8 + `python3` | ✅ ruff `All checks passed!`，输出与 b 非空行逐行一致 |
| d. `emit` → rust + `rustc 1.98.1` 编译运行 | ✅ 编译 0 error（33 条 emitter 风格 warning），输出与 b 非空行逐行一致 |

黄金输出摘要（完整输出见 b 路径）：

```text
--- 算法一：KNN（k=3，欧氏距离，多数投票）---
test 1: true=0 pred=0 nn=[1.000 1.414 1.732] ✓     （6 行全部 ✓）
KNN acc=6/6
--- 算法二：感知器（is-8 二分类，margin=1.0，lr=0.1）---
trained: epochs=7/30 updates=20 ||w||=1.7464
test 1: true=not8 pred=not8 score=-1.200 ✓          （6 行全部 ✓）
Perceptron acc=6/6
=== 汇总 ===
KNN acc=6/6, Perceptron acc=6/6
ML VERIFIED: KNN + perceptron both 6 / 6 on 5x5 digit bitmaps
```

## 本语料驱动的工具链修复与已知边界（诚实记录）

1. **v0.2.70 修复（本语料对拍驱动）**：`const MARGIN: f64 = 1.0` 这类
   **f64 整值 const 字面量**经 dhv-ts `exprLitText` 的 `String(v)` 归一成 `1`，
   rust 后端产出 `pub const MARGIN: f64 = 1;` 触发 rustc E0308（mismatched
   types）。dhv（Rust）侧用 `lit.raw` 保留 `1.0` —— dhv-ts 侧已对齐
   （无 `.`/`e` 的浮点表示补 `.0`）。最小复现：
   `const M: f64 = 1.0;` + rust 投射 → rustc E0308。
2. **已知双端分歧（非本批引入，org 图灵语料同状）**：dhv（Rust）对 `project{}`
   强制 **P-2 物理路径唯一**（一项一文件），dhv-ts 允许多项投射同一文件。
   本语料与 org 仓 `fixtures/turing/`（rule110 / busy-beaver / bf）一样采用
   多项单文件形态（rust 单文件可 rustc 直编），因此走 **dhv-ts 车道**；
   `dhv check` 对其报 22 × P-P2（对本语料仅此一类错误，无 S/G/N 违规）。
3. HSL 无 `vec![v; n]` 宏形态，但有等价的**数组重复字面量** `[0.0; 26]`
   （`arrayrep`，rust 投射 `vec![0.0; 26]` / python 投射 `[0.0] * 26`）。
