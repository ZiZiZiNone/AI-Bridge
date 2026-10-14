#!/usr/bin/env node
/**
 * tests/converter-fixtures/check-fixtures.mjs —— 转换器夹具脱敏检查。
 *
 * 依据：converters/sdk/README.md §8「脱敏夹具要求（强制）」。
 * 只查 §8 的脱敏与夹具位置/命名：
 *   - 位置与命名：tests/fixtures/raw/<声明路径 basename>.json、
 *     tests/fixtures/expected/standard-access.<level>.json
 *   - 规则 1/4：禁止真实凭据形状（JWT / AWS / sk- / ghp_ / xoxb- / PEM / 40+ 不透明串）
 *   - 规则 2：主机名只用保留域名（.example / .invalid / .test / .localhost），禁真实品牌串
 *   - 规则 3：敏感键（value/token/secret/totp_seed/password/…）的字符串值必须 fixture- 前缀
 *   - 规则 5：时间戳固定字面量（RFC 3339 UTC，Z 结尾），不用本地时间/偏移/动态生成
 *   - 规则 6：不掺杂真实环境路径（用 {APPDATA} 这类占位符）
 *
 * 不做的事：字段结构 schema 校验（归 converters/sdk/tests/schema-check.py）、
 * golden 逐字段比对与行为断言（归 converters/sdk/tests/conformance.mjs）。
 *
 * 用法：
 *   node tests/converter-fixtures/check-fixtures.mjs [转换器目录 ...]
 *   （缺省 = 仓库内 converters/qq 与 converters/_template）
 * 退出码：0 全部通过；1 存在违规。
 */
import { readdirSync, readFileSync, existsSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const REPO_ROOT = path.resolve(HERE, "..", "..");
export const DEFAULT_TARGETS = ["converters/qq", "converters/_template"];

/* --------------------------------------------------------- §8 规则常量 */

const RESERVED_TLDS = new Set(["example", "invalid", "test", "localhost"]);

const FILE_EXT = new Set([
  "exe", "dll", "json", "wasm", "mjs", "cjs", "js", "ts", "sqlite", "sqlite3", "db", "log", "txt",
  "so", "dylib", "bin", "dat", "ini", "toml", "yaml", "yml", "md", "csv", "pem", "key", "crt",
  "png", "jpg", "jpeg", "ico", "svg", "zip",
]);

/** 规则 4：真实凭据形状 + 真实品牌串（§8 规则 1/2/4 共用）。 */
const FORBIDDEN_SHAPES = [
  { name: "JWT 形状（eyJ…）", re: /eyJ[a-z0-9_-]{10,}\./i },
  { name: "AWS AccessKey 形状（AKIA…）", re: /akia[0-9a-z]{16}/i },
  { name: "OpenAI 密钥形状（sk-…）", re: /sk-[a-z0-9]{16,}/i },
  { name: "GitHub token 形状（ghp_/gho_/ghu_/ghs_/ghr_…）", re: /gh[posur]_[a-z0-9]{16,}/i },
  { name: "Slack token 形状（xox[baprs]-…）", re: /xox[baprs]-/i },
  { name: "PEM 私钥块（-----BEGIN…）", re: /-----begin/i },
  { name: "40 位以上不透明串", re: /^[a-z0-9+/=_-]{40,}$/i },
];

/** §8 规则 2：真实品牌串禁入夹具。 */
const BRAND_DENYLIST = [
  "qq.com", "weixin", "wechat", "github.com", "google.com", "microsoft.com", "apple.com",
  "amazon.com", "taobao", "alipay", "bilibili", "douyin",
];

/** §8 规则 3：敏感键的字符串值必须 fixture- 前缀虚构。 */
const SENSITIVE_KEYS = new Set([
  "value", "token", "secret", "totp_seed", "password", "access_token", "refresh_token",
  "client_secret",
]);
const FIXTURE_VALUE = /^fixture[-_a-z0-9.]*$/i;

/** §8 规则 5：时间戳键；值必须是 null（缺省语义）或 RFC 3339 UTC 固定字面量。 */
const TIME_KEY_RE = /(_at|_time|expires|expiry|timestamp)$/i;
const STRICT_RFC3339_UTC = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/;
const ISO_LIKE_RE = /^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}/;

/** §8 规则 6：真实环境路径禁入（路径用 {APPDATA} 等占位符）。 */
const REAL_ENV_PATH_RE = /\\users\\|\/users\/|\/home\/|[a-z]:\\users\\/i;

/* --------------------------------------------------------------- 工具 */

function listJsonFiles(dir) {
  if (!existsSync(dir) || !statSync(dir).isDirectory()) return [];
  const out = [];
  for (const name of readdirSync(dir).sort()) {
    const full = path.join(dir, name);
    if (statSync(full).isDirectory()) out.push(...listJsonFiles(full));
    else if (name.endsWith(".json")) out.push(full);
  }
  return out;
}

function hostCandidates(text) {
  const out = [];
  for (const raw of String(text).split(/\s+/)) {
    const token = raw
      .replace(/^[a-z][a-z0-9+.-]*:\/\//i, "")
      .replace(/^[^/@]*@/, "")
      .split(/[/?#]/)[0]
      .replace(/^\*\./, "")
      .toLowerCase();
    if (!/^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)*\.[a-z]{2,}$/.test(token)) continue;
    if (FILE_EXT.has(token.split(".").pop())) continue;
    out.push(token);
  }
  return out;
}

function isStrictRfc3339Utc(text) {
  if (!STRICT_RFC3339_UTC.test(text)) return false;
  const parsed = new Date(text);
  return !Number.isNaN(parsed.getTime()) && parsed.toISOString().replace(/\.000Z$/, "Z") === text;
}

/* ------------------------------------------------- §8 脱敏 lint（逐字符串） */

function lintString(file, text, jsonPath, key, violations) {
  const where = `${path.basename(path.dirname(file))}/${path.basename(file)}#${jsonPath.join(".")}`;
  const lower = text.toLowerCase();

  for (const brand of BRAND_DENYLIST) {
    if (lower.includes(brand)) violations.push(`${where}：命中真实品牌串「${brand}」（§8 规则 2）`);
  }
  for (const { name, re } of FORBIDDEN_SHAPES) {
    if (re.test(text) && !FIXTURE_VALUE.test(text)) {
      violations.push(`${where}：命中${name}（§8 规则 4）`);
    }
  }
  for (const host of hostCandidates(text)) {
    if (!RESERVED_TLDS.has(host.split(".").pop())) {
      violations.push(`${where}：主机名 ${host} 未用保留域名 .example/.invalid/.test/.localhost（§8 规则 2）`);
    }
  }
  if (key && SENSITIVE_KEYS.has(key.toLowerCase()) && !FIXTURE_VALUE.test(text)) {
    violations.push(`${where}：敏感键 ${key} 的值必须 fixture- 前缀虚构（§8 规则 3）`);
  }
  if (key && TIME_KEY_RE.test(key)) {
    if (!isStrictRfc3339Utc(text)) {
      violations.push(`${where}：时间戳必须是 RFC 3339 UTC 固定字面量（Z 结尾），当前为「${text}」（§8 规则 5）`);
    }
  } else if (ISO_LIKE_RE.test(text) && !isStrictRfc3339Utc(text)) {
    violations.push(`${where}：疑似时间戳字符串非 RFC 3339 UTC 固定字面量：「${text}」（§8 规则 5）`);
  }
  if (REAL_ENV_PATH_RE.test(text)) {
    violations.push(`${where}：夹带真实环境路径，应改用 {{APPDATA}} 类占位符（§8 规则 6）`);
  }
}

function lintNode(file, node, jsonPath, key, violations) {
  if (typeof node === "string") return lintString(file, node, jsonPath, key, violations);
  if (Array.isArray(node)) {
    node.forEach((item, index) => lintNode(file, item, [...jsonPath, String(index)], key, violations));
    return;
  }
  if (node && typeof node === "object") {
    for (const [childKey, child] of Object.entries(node)) {
      lintNode(file, child, [...jsonPath, childKey], childKey, violations);
    }
    return;
  }
  if (key && TIME_KEY_RE.test(key) && node !== null) {
    violations.push(
      `${path.basename(file)}#${jsonPath.join(".")}：时间戳键 ${key} 的值必须是 RFC 3339 UTC 字面量或 null（§8 规则 5）`,
    );
  }
}

/* ------------------------------------------ §8 位置与命名 + 文件级检查 */

function checkLayout(converterDir, violations) {
  const fixtureRoot = path.join(converterDir, "tests", "fixtures");
  const rawDir = path.join(fixtureRoot, "raw");
  const expectedDir = path.join(fixtureRoot, "expected");
  const rawFiles = listJsonFiles(rawDir);
  const expectedFiles = listJsonFiles(expectedDir);

  if (rawFiles.length === 0) violations.push("tests/fixtures/raw：至少需要一份取数输入夹具（§8 位置与命名）");
  if (expectedFiles.length === 0) {
    violations.push("tests/fixtures/expected：至少需要一份期望产物夹具（§8 位置与命名）");
  }
  for (const file of expectedFiles) {
    if (!/^standard-access\.[cba]\.json$/.test(path.basename(file))) {
      violations.push(`${path.basename(file)}：expected 夹具命名必须是 standard-access.<level>.json（§8 位置与命名）`);
    }
  }

  // raw/<声明路径 basename>.json：与 manifest.readable_paths 的 basename 对齐（§8 位置与命名）。
  const manifestPath = path.join(converterDir, "manifest.json");
  if (existsSync(manifestPath)) {
    let manifest = {};
    try {
      manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
    } catch (error) {
      violations.push(`manifest.json：无法解析（${error.message}）`);
    }
    const declared = Array.isArray(manifest.readable_paths) ? manifest.readable_paths : [];
    const accepted = new Set();
    for (const entry of declared) {
      const base = String(entry).split(/[\\/]/).pop();
      accepted.add(base);
      accepted.add(`${base.replace(/\.json$/i, "")}.json`);
    }
    for (const file of rawFiles) {
      const name = path.basename(file);
      if (accepted.size > 0 && !accepted.has(name)) {
        violations.push(`raw/${name}：文件名不是任何声明路径的 basename（§8 位置与命名）`);
      }
    }
  }
  return { rawFiles, expectedFiles };
}

/* --------------------------------------------------------------- 入口 */

/**
 * 校验单个转换器目录的夹具。
 * @returns {{ dir: string, files: string[], violations: string[] }}
 */
export function checkConverterFixtures(converterDir) {
  const violations = [];
  const { rawFiles, expectedFiles } = checkLayout(converterDir, violations);
  const files = [...rawFiles, ...expectedFiles];
  for (const file of files) {
    let data;
    try {
      data = JSON.parse(readFileSync(file, "utf8"));
    } catch (error) {
      violations.push(`${path.basename(file)}：JSON 解析失败（${error.message}）`);
      continue;
    }
    lintNode(file, data, [], null, violations);
  }
  return { dir: converterDir, files, violations };
}

/* CLI：直接运行时对参数（或缺省两处）目录执行检查，退出码 0/1。 */
if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const args = process.argv.slice(2);
  const targets = (args.length > 0 ? args : DEFAULT_TARGETS).map((p) => path.resolve(p));
  let failed = false;
  for (const target of targets) {
    const result = checkConverterFixtures(target);
    const label = path.relative(REPO_ROOT, target) || target;
    console.log(`\n${label}：检查 ${result.files.length} 个夹具 JSON`);
    if (result.violations.length === 0) {
      console.log("  PASS：符合 sdk/README.md §8 脱敏夹具要求");
    } else {
      failed = true;
      console.log(`  FAIL：${result.violations.length} 处违规`);
      for (const v of result.violations) console.log(`    - ${v}`);
    }
  }
  process.exit(failed ? 1 : 0);
}
