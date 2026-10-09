#!/usr/bin/env node
/**
 * 转换器规范一致性检查（Node ≥ 22，零第三方依赖）。
 *
 * 覆盖：ABI 往返、沙盒白名单拒绝、extractor/normalizer 职责分界、逐等级 golden 对比、
 *       等级输出义务、日志脱敏、夹具脱敏 lint、清单占位签名、契约版本常量漂移。
 *
 * 用法：
 *   node converters/sdk/tests/conformance.mjs [转换器目录] [--expect-placeholder-signature]
 * 默认目标：converters/_template，默认参数等价于
 *   node converters/sdk/tests/conformance.mjs converters/_template --expect-placeholder-signature
 */

import { readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { AB_MAX_REQUEST_BYTES, STANDARD_ACCESS_SCHEMA_VERSION } from "../abi.mjs";
import { SandboxError, createLocalHost, readJsonFile as readJson } from "../host-shim.mjs";

/* ------------------------------------------------------------------ 参数 */

const argv = process.argv.slice(2);
const flags = new Set(argv.filter((a) => a.startsWith("--")));
const positional = argv.filter((a) => !a.startsWith("--"));
const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "..", "..", "..");
const converterArg = positional[0] ?? "converters/_template";
const converterDir = path.isAbsolute(converterArg) ? converterArg : path.join(repoRoot, converterArg);
const expectPlaceholder = flags.has("--expect-placeholder-signature") || positional.length === 0;

/* ------------------------------------------------------------- 迷你工具 */

const results = [];
function assert(condition, message) {
  if (!condition) throw new Error(message);
}
function truncate(text, limit = 700) {
  const s = String(text);
  return s.length <= limit ? s : `${s.slice(0, limit)}…（共 ${s.length} 字节）`;
}
function canon(value) {
  if (Array.isArray(value)) return value.map(canon);
  if (value && typeof value === "object") {
    const out = {};
    for (const key of Object.keys(value).sort()) out[key] = canon(value[key]);
    return out;
  }
  return value;
}
const canonText = (value) => JSON.stringify(canon(value));
function firstDiff(expected, actual) {
  const n = Math.min(expected.length, actual.length);
  let i = 0;
  while (i < n && expected[i] === actual[i]) i += 1;
  const from = Math.max(0, i - 60);
  return `偏移 ${i}\n  期望: …${expected.slice(from, i + 60)}…\n  实际: …${actual.slice(from, i + 60)}…`;
}
function assertCanonEqual(actual, expected, label) {
  const a = canonText(actual);
  const e = canonText(expected);
  if (a !== e) throw new Error(`${label} 不一致（${firstDiff(e, a)}）`);
}
function deepFreeze(value) {
  if (Array.isArray(value)) value.forEach(deepFreeze);
  else if (value && typeof value === "object") Object.values(value).forEach(deepFreeze);
  return value && typeof value === "object" ? Object.freeze(value) : value;
}
function rel(file) {
  return path.relative(repoRoot, file).replaceAll("\\", "/");
}
function listJson(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const full = path.join(dir, name);
    if (statSync(full).isDirectory()) out.push(...listJson(full));
    else if (name.endsWith(".json")) out.push(full);
  }
  return out;
}
async function check(name, fn) {
  try {
    await fn();
    results.push({ name, ok: true });
    console.log(`  PASS  ${name}`);
  } catch (error) {
    results.push({ name, ok: false, error });
    console.log(`  FAIL  ${name}`);
    console.log(`        ${String(error?.message ?? error).split("\n").join("\n        ")}`);
  }
}
function section(title) {
  console.log(`\n${title}`);
}

/* --------------------------------------------------------- 脱敏 lint 规则 */

const RESERVED_TLDS = new Set(["example", "invalid", "test", "localhost"]);
const FILE_EXT = new Set([
  "exe", "dll", "json", "wasm", "mjs", "cjs", "js", "ts", "sqlite", "sqlite3", "db", "log", "txt",
  "so", "dylib", "bin", "dat", "ini", "toml", "yaml", "yml", "md", "csv", "pem", "key", "crt",
  "png", "jpg", "jpeg", "ico", "svg", "zip",
]);
const BRAND_DENYLIST = [
  "qq.com", "weixin", "wechat", "github.com", "google.com", "microsoft.com", "apple.com",
  "amazon.com", "taobao", "alipay", "bilibili", "douyin", "eyJ", "AKIA", "sk-", "ghp_", "xoxb-",
  "-----begin",
];
const SENSITIVE_KEYS = new Set([
  "value", "token", "secret", "totp_seed", "password", "access_token", "refresh_token", "client_secret",
]);
const FIXTURE_VALUE = /^fixture[-_a-z0-9.]*$/i;
const OPAQUE_BLOB = /^[A-Za-z0-9+/=_-]{40,}$/;

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

function lintFixture(file, violations) {
  function walk(node, jsonPath, key) {
    if (typeof node === "string") return lintString(node, jsonPath, key);
    if (Array.isArray(node)) return node.forEach((item, index) => walk(item, [...jsonPath, String(index)], key));
    if (node && typeof node === "object") {
      for (const [childKey, child] of Object.entries(node)) walk(child, [...jsonPath, childKey], childKey);
    }
    return undefined;
  }
  function lintString(text, jsonPath, key) {
    const where = `${rel(file)}#${jsonPath.join(".")}`;
    const lower = text.toLowerCase();
    for (const brand of BRAND_DENYLIST) {
      if (lower.includes(brand)) violations.push(`${where}：命中禁用串「${brand}」`);
    }
    for (const host of hostCandidates(text)) {
      const tld = host.split(".").pop();
      if (!RESERVED_TLDS.has(tld)) {
        violations.push(`${where}：主机名 ${host} 未使用保留域名（.example/.invalid/.test/.localhost）`);
      }
    }
    if (key && SENSITIVE_KEYS.has(key.toLowerCase()) && !FIXTURE_VALUE.test(text)) {
      violations.push(`${where}：敏感值必须以 fixture- 前缀虚构，当前为「${truncate(text, 60)}」`);
    }
    if (OPAQUE_BLOB.test(text) && !FIXTURE_VALUE.test(text)) {
      violations.push(`${where}：疑似真实不透明串（长度 ${text.length}）`);
    }
  }
  walk(readJson(file), [], null);
}

/* -------------------------------------------------------------------- main */

console.log("AI-Bridge 转换器规范一致性检查（conformance）");
console.log(`  Node            : ${process.version}`);
console.log(`  目标转换器目录  : ${rel(converterDir)}`);

let host;
let manifest;
let fixtureRoot;
let expectedDir;
let rawFiles;
let levels;
let extractorModule;
let normalizerModule;
let primaryRawFile;
let rawFixtureText;
let goldenByLevel;
let clock;
const allLogs = [];

try {
  host = await createLocalHost({ converterDir, now: () => "1970-01-01T00:00:00Z" });
  manifest = host.manifest;
  fixtureRoot = path.join(converterDir, "tests", "fixtures");
  expectedDir = path.join(fixtureRoot, "expected");
  rawFiles = listJson(path.join(fixtureRoot, "raw"));
  levels = readdirSync(expectedDir)
    .filter((name) => /^standard-access\.[cba]\.json$/.test(name))
    .map((name) => name.replace(/^standard-access\./, "").replace(/\.json$/, ""))
    .sort();
  if (rawFiles.length === 0) throw new Error("tests/fixtures/raw 下没有夹具");
  extractorModule = await import(pathToFileURL(path.join(converterDir, "extractor.mjs")).href);
  normalizerModule = await import(pathToFileURL(path.join(converterDir, "normalizer.mjs")).href);
  primaryRawFile = rawFiles[0];
  rawFixtureText = new TextDecoder().decode(host.readDeclared(path.basename(primaryRawFile)));
  goldenByLevel = new Map(
    levels.map((level) => [level, readJson(path.join(expectedDir, `standard-access.${level}.json`))]),
  );
  clock = goldenByLevel.get(manifest.level)?.generated_at ?? "1970-01-01T00:00:00Z";
} catch (error) {
  console.log(`\n准备阶段失败（清单 / 夹具不可读）：${String(error?.message ?? error)}`);
  process.exit(1);
}

/* [1] 清单约定（schema 之外的 SDK 约定） */
section("[1/9] 清单约定");
await check("manifest.id / entry 形态符合 SDK 约定", () => {
  assert(/^[a-z0-9][a-z0-9-]{1,63}$/.test(manifest.id), `id 非法：${manifest.id}`);
  assert(/^[a-z0-9][a-z0-9._-]*\.wasm$/.test(manifest.entry), `entry 必须是同目录 .wasm 文件名：${manifest.entry}`);
  assert(["c", "b", "a"].includes(manifest.level), `level 非法：${manifest.level}`);
});
await check("readable_paths 每项都能展开为绝对路径且无通配", () => {
  assert(host.declaredPaths.length === manifest.readable_paths.length, "存在无法展开的声明路径");
  for (const item of host.declaredPaths) {
    assert(path.isAbsolute(item.expanded), `未展开为绝对路径：${item.template}`);
  }
});
if (expectPlaceholder) {
  await check("模板清单必须是未签名占位（防止模板被误当成品发布）", () => {
    assert(
      /^0{64}$/.test(manifest.signature.value),
      `模板 signature.value 必须是 64 个 0，当前为 ${manifest.signature.value}`,
    );
  });
}

/* [2] 契约版本常量漂移 */
section("[2/9] 契约常量一致性");
await check("sdk 的 STANDARD_ACCESS_SCHEMA_VERSION 与契约 schema 的 const 一致", () => {
  const schema = readJson(path.join(repoRoot, "contracts", "standard-access.schema.json"));
  const declared = schema.properties.schema_version.const;
  assert(
    declared === STANDARD_ACCESS_SCHEMA_VERSION,
    `漂移：schema=${declared} sdk=${STANDARD_ACCESS_SCHEMA_VERSION}`,
  );
});

/* [3] ABI 冒烟 */
section("[3/9] WASM ABI 往返");
const manifestLevelResult = await host.collect();
allLogs.push(...manifestLevelResult.logs);
await check("ab_run(op=convert) 经线性内存往返返回 JSON 文档", () => {
  assert(manifestLevelResult.ok, `失败：${manifestLevelResult.reason}`);
  assert(manifestLevelResult.json && typeof manifestLevelResult.json === "object", "响应不是 JSON 对象");
  assert(manifestLevelResult.json.level === manifest.level, "响应 level 与清单不一致");
});
await check("未知 op 返回失败码（-1）", () => {
  const result = host.runRaw({ op: "unsupported", level: manifest.level });
  assert(!result.ok, "未知 op 竟然成功");
});
await check("非法 level 返回失败码（-1）", () => {
  const result = host.runRaw(host.buildRequest({ level: "z" }));
  assert(!result.ok, "非法 level 竟然成功");
});
await check(`宿主侧拒绝超过 ABI 上限（${AB_MAX_REQUEST_BYTES} 字节）的请求`, () => {
  const result = host.runRaw({ ...host.buildRequest(), pad: "x".repeat(AB_MAX_REQUEST_BYTES) });
  assert(!result.ok, "超大请求竟然成功");
});
await check("同一入参两次采集结果一致（无隐藏状态）", () => {
  const first = host.collect({ now: "2026-01-15T08:30:00Z" });
  const second = host.collect({ now: "2026-01-15T08:30:00Z" });
  assert(first.ok && second.ok, "采集失败");
  assertCanonEqual(second.json, first.json, "两次采集产物");
});

/* [4] 沙盒白名单 */
section("[4/9] 只读沙盒白名单");
await check("读取未在 readable_paths 声明的路径被拒绝", () => {
  let thrown = null;
  try {
    host.readDeclared("undeclared-file.json");
  } catch (error) {
    thrown = error;
  }
  assert(thrown instanceof SandboxError, `未拒绝未声明路径（得到 ${String(thrown)}）`);
});
await check("读取声明路径成功且夹具按 basename 挂载", () => {
  assert(rawFiles.length > 0, "tests/fixtures/raw 下没有夹具");
  for (const file of rawFiles) host.readDeclared(path.basename(file));
});

/* [5] extractor 职责分界 */
section("[5/9] extractor / normalizer 职责分界");
const ALLOWED_RAW_KEYS = new Set(["collected_at", "source_files", "accounts", "cookies", "tokens", "plain_credentials", "launch"]);
const CONTRACT_KEYS = new Set(["schema_version", "level", "sessions", "credentials", "meta"]);

function runExtractor(level) {
  const logs = [];
  const raw = extractorModule.extract(
    { readFile: () => rawFixtureText, log: (lvl, message) => logs.push({ level: lvl, message }) },
    { level, now: clock },
  );
  return { raw, logs };
}

for (const level of ["c", "b", "a"]) {
  await check(`extractor(level=${level}) 只产出中间格式，不含契约字段`, () => {
    const { raw, logs } = runExtractor(level);
    allLogs.push(...logs);
    for (const key of Object.keys(raw)) {
      assert(!CONTRACT_KEYS.has(key), `中间格式出现契约字段：${key}`);
      assert(ALLOWED_RAW_KEYS.has(key), `中间格式出现未约定字段：${key}`);
    }
    assert(Array.isArray(raw.source_files), "中间格式缺少 source_files");
    assert(raw.collected_at === clock, "中间格式的 collected_at 必须取宿主时钟");
  });
}
await check("level=a 的中间格式不含任何账号/会话/凭据数据", () => {
  const { raw } = runExtractor("a");
  for (const key of ["accounts", "cookies", "tokens", "plain_credentials"]) {
    assert(raw[key] === undefined, `level=a 竟然产出了 ${key}`);
  }
  assert(raw.launch?.app_id, "level=a 缺少启动信息");
});

/* [6] normalizer 纯函数 */
section("[6/9] normalizer 纯函数约束");
await check("normalize 不改动入参、可重复求值且结果一致", () => {
  const { raw } = runExtractor(manifest.level);
  const ctx = {
    level: manifest.level,
    app_id: manifest.id,
    manifest_id: manifest.id,
    converter_version: manifest.version,
    now: clock,
  };
  deepFreeze(raw);
  deepFreeze(ctx);
  const before = canonText(raw);
  const first = normalizerModule.normalize(raw, ctx);
  const second = normalizerModule.normalize(raw, ctx);
  assertCanonEqual(second, first, "两次 normalize 结果");
  assert(canonText(raw) === before, "normalize 改写了入参");
});

/* [7] 逐等级 golden 对比 */
section("[7/9] 逐等级 golden 对比");
assert(levels.includes(manifest.level), `缺少 manifest.level=${manifest.level} 的样例 fixtures/expected/standard-access.${manifest.level}.json`);
for (const level of levels) {
  const golden = goldenByLevel.get(level);
  await check(`level=${level} 的实跑产物与样例逐字段一致`, () => {
    assert(golden.level === level, `样例 level 字段为 ${golden.level}`);
    assert(
      golden.meta?.collected_at === golden.generated_at,
      "样例内 generated_at 与 meta.collected_at 必须取同一宿主时钟",
    );
    const result = host.collect({ level, now: golden.generated_at });
    allLogs.push(...result.logs);
    assert(result.ok, `采集失败：${result.reason}`);
    assertCanonEqual(result.json, golden, `level=${level} 产物`);
  });
  await check(`level=${level} 样例与清单的 id/version 交叉一致`, () => {
    assert(golden.app_id === manifest.id, `app_id=${golden.app_id} 与 manifest.id=${manifest.id} 不一致`);
    assert(
      golden.meta?.converter_version === manifest.version,
      `meta.converter_version=${golden.meta?.converter_version} 与 manifest.version=${manifest.version} 不一致`,
    );
    for (const session of golden.sessions ?? []) {
      assert(session.source?.type === "converter", "会话 source.type 必须是 converter");
      assert(session.source?.id === manifest.id, `会话 source.id=${session.source?.id} 与 manifest.id 不一致`);
    }
  });
}

/* [8] 等级输出义务 */
section("[8/9] 接入等级输出义务");
await check("level=c：至少一条会话、cookie 属性完整、不得携带凭据", () => {
  const doc = goldenByLevel.get("c");
  assert((doc.sessions ?? []).length > 0, "c 级必须输出至少一条 session");
  assert((doc.credentials ?? []).length === 0, "c 级不得输出 credentials");
  for (const session of doc.sessions) {
    assert(["cookie", "token"].includes(session.kind), `c 级 session.kind 非法：${session.kind}`);
    assert(session.target?.kind === "web" && session.target.domain, "c 级 session 缺少 target.domain");
    if (session.kind === "cookie") {
      const cookies = session.payload?.cookies ?? [];
      assert(cookies.length > 0, "cookie 会话没有 payload.cookies");
      for (const cookie of cookies) {
        for (const attr of ["name", "value", "domain", "path", "secure", "http_only", "same_site"]) {
          assert(cookie[attr] !== undefined, `cookie 丢失属性 ${attr}（保真度关键）`);
        }
      }
      assert(session.expires_at !== undefined, "cookie 会话应给出最早过期时间");
    }
  }
});
await check("level=b：至少一条凭据、password 带 url、不得携带会话", () => {
  const doc = goldenByLevel.get("b");
  const credentials = doc.credentials ?? [];
  assert(credentials.length > 0, "b 级必须输出 credentials");
  assert((doc.sessions ?? []).length === 0, "b 级不得输出 sessions");
  for (const item of credentials) {
    assert(item.account_ref, "凭据缺少 account_ref");
    assert(["password", "passkey", "totp"].includes(item.kind), `凭据 kind 非法：${item.kind}`);
    assert(item.username, "凭据缺少 username");
    if (item.kind === "password") assert(item.url, "password 凭据必须带 url");
    if (item.kind === "passkey") assert(item.secret === undefined, "passkey 不得输出密钥材料");
  }
});
await check("level=a：不产出凭据与会话，只给启动信息", () => {
  const doc = goldenByLevel.get("a");
  assert((doc.credentials ?? []).length === 0, "a 级不得输出 credentials");
  assert((doc.sessions ?? []).length === 0, "a 级不得输出 sessions");
  assert(doc.meta?.launch?.app_id, "a 级必须给出 meta.launch.app_id");
});

/* [9] 脱敏 */
section("[9/9] 脱敏（日志 + 夹具）");
await check("运行日志不含任何敏感字段原值", () => {
  const secrets = [];
  (function collect(node, key) {
    if (typeof node === "string") {
      if (key && SENSITIVE_KEYS.has(key.toLowerCase())) secrets.push(node);
      return;
    }
    if (Array.isArray(node)) return node.forEach((item) => collect(item, key));
    if (node && typeof node === "object") {
      for (const [childKey, child] of Object.entries(node)) collect(child, childKey);
    }
  })(readJson(primaryRawFile), null);
  assert(secrets.length > 0, "夹具里没有可校验的敏感值，检查无意义");
  for (const { message } of allLogs) {
    for (const secret of secrets) {
      assert(!String(message).includes(secret), `日志泄漏敏感值：${message}`);
    }
  }
});
const fixtureFiles = listJson(fixtureRoot);
await check(`夹具脱敏 lint（${fixtureFiles.length} 个 JSON 文件）`, () => {
  const violations = [];
  for (const file of fixtureFiles) lintFixture(file, violations);
  assert(violations.length === 0, `违规 ${violations.length} 处：\n${violations.join("\n")}`);
});

/* ------------------------------------------------------------------ 汇总 */

const failed = results.filter((item) => !item.ok);
console.log(`\n结果：${results.length} 项检查，${failed.length} 项失败`);
if (failed.length > 0) {
  for (const item of failed) console.log(`  - ${item.name}: ${String(item.error?.message ?? item.error).split("\n")[0]}`);
  process.exitCode = 1;
}
