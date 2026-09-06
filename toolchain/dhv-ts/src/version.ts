// ============================================================================
// dhv-ts/src/version.ts — 工具链版本单一来源
// ----------------------------------------------------------------------------
// 此前版本串在 main.ts（横幅）/ emit.ts（manifest）/ decls.ts（文件头注释）
// 三处硬编码，已漂移到 0.2.10（package.json 实为 0.2.5x）——manifest 声称的
// 工具链版本与真实二进制不符。现统一从 package.json 读取，删一处忘一处不再可能。
//
// 嵌入执行边界（ORG 单二进制分发）：当本模块被静态打包进宿主二进制时，
// import.meta.dir 指向打包器虚拟文件系统，package.json 不可达 —— 此时回退
// 到 env.DHV_VERSION（宿主在解包运行时资源后注入），最终回退 0.0.0。
// ============================================================================
import * as fs from 'node:fs';
import * as path from 'node:path';

function readVersion(): string {
  try {
    return (JSON.parse(
      fs.readFileSync(path.join(import.meta.dir, '..', 'package.json'), 'utf-8'),
    ) as { version: string }).version;
  } catch {
    if (typeof process !== 'undefined' && process.env?.DHV_VERSION) return process.env.DHV_VERSION;
    return '0.0.0';
  }
}

export const VERSION: string = readVersion();
