// ============================================================================
// dhv-ts/src/version.ts — 工具链版本单一来源
// ----------------------------------------------------------------------------
// 此前版本串在 main.ts（横幅）/ emit.ts（manifest）/ decls.ts（文件头注释）
// 三处硬编码，已漂移到 0.2.10（package.json 实为 0.2.5x）——manifest 声称的
// 工具链版本与真实二进制不符。现统一从 package.json 读取，删一处忘一处不再可能。
// ============================================================================
import * as fs from 'node:fs';
import * as path from 'node:path';

export const VERSION: string = (JSON.parse(
  fs.readFileSync(path.join(import.meta.dir, '..', 'package.json'), 'utf-8'),
) as { version: string }).version;
