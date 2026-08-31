// 小型统计库（待修复：variance 分母错误 + median 缺失）
export function mean(xs: number[]): number {
  if (xs.length === 0) return 0;
  const s = xs.reduce((a, x) => a + x, 0);
  return s / xs.length;
}

// BUG: 样本方差应为 n-1 分母，这里误用了 n
export function variance(xs: number[]): number {
  if (xs.length === 0) return 0;
  const m = mean(xs);
  const s = xs.reduce((a, x) => a + (x - m) ** 2, 0);
  return s / (xs.length - 1);
}

export function median(xs: number[]): number {
  if (xs.length === 0) return 0;
  const sorted = [...xs].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 1 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}
