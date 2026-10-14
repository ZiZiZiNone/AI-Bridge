#!/usr/bin/env node
/**
 * tests/converter-fixtures/run.mjs —— 入口脚本：把 converters/qq 与 converters/_template
 * 两处夹具的 §8 脱敏检查串起来（converters/sdk/README.md §8「脱敏夹具要求（强制）」）。
 *
 * 用法：node tests/converter-fixtures/run.mjs
 * 退出码：0 = 两处夹具全部通过；1 = 任一处存在违规。
 *
 * 职责边界：本套件只查夹具脱敏（fixture- 前缀 / 保留域名 / 固定时间 / 无真实凭据形状等）；
 * schema 字段结构归 converters/sdk/tests/schema-check.py，不在此重复。
 */
import path from "node:path";
import {
  checkConverterFixtures,
  REPO_ROOT,
  DEFAULT_TARGETS,
} from "./check-fixtures.mjs";

let failed = false;
console.log("AI-Bridge 转换器夹具脱敏检查（sdk/README.md §8）");

for (const rel of DEFAULT_TARGETS) {
  const dir = path.resolve(REPO_ROOT, rel);
  const { files, violations } = checkConverterFixtures(dir);
  console.log(`\n${rel}：检查 ${files.length} 个夹具 JSON（raw + expected）`);
  if (violations.length === 0) {
    console.log("  PASS：符合 §8 脱敏夹具要求");
  } else {
    failed = true;
    console.log(`  FAIL：${violations.length} 处违规`);
    for (const violation of violations) console.log(`    - ${violation}`);
  }
}

console.log(`\n结果：${failed ? "FAIL（退出码 1）" : "PASS（退出码 0）"}`);
process.exit(failed ? 1 : 0);
