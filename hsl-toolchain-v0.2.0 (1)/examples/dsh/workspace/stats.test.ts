import { mean, variance, median } from "./stats.ts";

let failures = 0;
function check(name: string, got: number, want: number) {
  const ok = Math.abs(got - want) < 1e-9;
  console.log(`${ok ? "PASS" : "FAIL"} ${name}: got=${got} want=${want}`);
  if (!ok) failures++;
}

check("mean", mean([1, 2, 3, 4]), 2.5);
check("variance (sample, n-1)", variance([2, 4, 4, 4, 5, 5, 7, 9]), 32 / 7);
check("median odd", median([3, 1, 2]), 2);
check("median even", median([4, 1, 3, 2]), 2.5);

if (failures > 0) {
  console.error(`\n${failures} test(s) failed`);
  process.exit(1);
}
console.log("\nall tests passed");
