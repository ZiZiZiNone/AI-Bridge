/**
 * types.ts 的契约一致性测试：把 TS 镜像与 contracts/*.schema.json 逐字段比对。
 *
 * 这是"契约字段与 schema 逐字段一致"的实跑证据：字段名、必填集合、枚举值域、
 * 正则 pattern、additionalProperties 全部对着 schema 原文断言。
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  ACCESS_LEVELS,
  CLIENT_ENV_KEYS,
  CLIENT_ENV_REQUIRED_KEYS,
  CONVERTER_MANIFEST_KEYS,
  CONVERTER_MANIFEST_REQUIRED_KEYS,
  COOKIE_KEYS,
  COOKIE_REQUIRED_KEYS,
  CREDENTIAL_KINDS,
  CREDENTIAL_RECORD_KEYS,
  CREDENTIAL_RECORD_REQUIRED_KEYS,
  DATE_TIME_PATTERN,
  ID_PATTERN,
  MODULE_SHA256_PATTERN,
  PAYLOAD_KEYS,
  PAYLOAD_REQUIRED_KEYS,
  PLAIN_CREDENTIAL_KEYS,
  PLAIN_CREDENTIAL_REQUIRED_KEYS,
  SAME_SITE_VALUES,
  SCHEMA_VERSION,
  SCOPE_KEYS,
  SCOPE_REQUIRED_KEYS,
  SESSION_ENVELOPE_KEYS,
  SESSION_ENVELOPE_REQUIRED_KEYS,
  SESSION_KINDS,
  SESSION_RECORD_KEYS,
  SESSION_RECORD_REQUIRED_KEYS,
  SIGNATURE_ALGORITHM,
  SIGNATURE_KEYS,
  SIGNATURE_REQUIRED_KEYS,
  SIGNATURE_VALUE_PATTERN,
  SOURCE_KEYS,
  SOURCE_REQUIRED_KEYS,
  SOURCE_TYPES,
  STANDARD_ACCESS_KEYS,
  STANDARD_ACCESS_REQUIRED_KEYS,
  TARGET_KEYS,
  TARGET_KINDS,
  TARGET_REQUIRED_KEYS,
  VERSION_PATTERN,
} from "../types.ts";

type JsonObject = Record<string, unknown>;

const CONTRACTS_DIR = new URL("../../../contracts/", import.meta.url);

function loadSchema(file: string): JsonObject {
  return JSON.parse(readFileSync(new URL(file, CONTRACTS_DIR), "utf8")) as JsonObject;
}

const standardAccess = loadSchema("standard-access.schema.json");
const converterManifest = loadSchema("converter-manifest.schema.json");
const sessionEnvelope = loadSchema("session-envelope.schema.json");

/** 按路径取 schema 节点，路径中断即断言失败。 */
function at(node: unknown, ...keys: string[]): unknown {
  let current = node;
  for (const key of keys) {
    assert.ok(
      current !== null && typeof current === "object",
      `schema 路径中断于 ${key}（${keys.join(".")}）`,
    );
    current = (current as JsonObject)[key];
  }
  return current;
}

/** 取对象的 properties 键名（保持 schema 声明顺序）。 */
function propKeys(node: unknown): string[] {
  const properties = at(node, "properties");
  assert.ok(properties !== null && typeof properties === "object", "schema 节点缺少 properties");
  return Object.keys(properties as JsonObject);
}

/** 取对象的 required 数组（缺省为空）。 */
function requiredKeys(node: unknown): string[] {
  const value = at(node, "required");
  assert.ok(Array.isArray(value), "schema 节点缺少 required 数组");
  return value as string[];
}

const cookieDef = at(standardAccess, "$defs", "cookie");
const envelopeCookie = at(
  sessionEnvelope,
  "properties",
  "payload",
  "properties",
  "cookies",
  "items",
);

test("contracts/ 三个 schema 都可读且版本与 SCHEMA_VERSION 一致", () => {
  assert.equal(at(standardAccess, "title"), "StandardAccess");
  assert.equal(at(converterManifest, "title"), "ConverterManifest");
  assert.equal(at(sessionEnvelope, "title"), "SessionEnvelope");
  assert.equal(SCHEMA_VERSION, at(standardAccess, "properties", "schema_version", "const"));
  assert.equal(SCHEMA_VERSION, at(sessionEnvelope, "properties", "schema_version", "const"));
});

test("枚举值域与 schema enum / const 逐值一致", () => {
  assert.deepEqual(ACCESS_LEVELS, at(standardAccess, "properties", "level", "enum"));
  assert.deepEqual(ACCESS_LEVELS, at(converterManifest, "properties", "level", "enum"));
  assert.deepEqual(CREDENTIAL_KINDS, at(standardAccess, "$defs", "credential", "properties", "kind", "enum"));
  assert.deepEqual(SESSION_KINDS, at(standardAccess, "$defs", "session", "properties", "kind", "enum"));
  assert.deepEqual(SESSION_KINDS, at(sessionEnvelope, "properties", "kind", "enum"));
  assert.deepEqual(TARGET_KINDS, at(standardAccess, "$defs", "target", "properties", "kind", "enum"));
  assert.deepEqual(TARGET_KINDS, at(sessionEnvelope, "properties", "target", "properties", "kind", "enum"));
  assert.deepEqual(SOURCE_TYPES, at(standardAccess, "$defs", "source", "properties", "type", "enum"));
  assert.deepEqual(SOURCE_TYPES, at(sessionEnvelope, "properties", "source", "properties", "type", "enum"));
  assert.deepEqual(SAME_SITE_VALUES, at(cookieDef, "properties", "same_site", "enum"));
  assert.deepEqual(SAME_SITE_VALUES, at(envelopeCookie, "properties", "same_site", "enum"));
  assert.equal(SIGNATURE_ALGORITHM, at(converterManifest, "properties", "signature", "properties", "algorithm", "const"));
});

test("字段集合与 schema properties 逐字段一致（含顺序）", () => {
  assert.deepEqual(COOKIE_KEYS, propKeys(cookieDef));
  assert.deepEqual(COOKIE_KEYS, propKeys(envelopeCookie));
  assert.deepEqual(TARGET_KEYS, propKeys(at(standardAccess, "$defs", "target")));
  assert.deepEqual(TARGET_KEYS, propKeys(at(sessionEnvelope, "properties", "target")));
  assert.deepEqual(PLAIN_CREDENTIAL_KEYS, propKeys(at(standardAccess, "$defs", "payload", "properties", "credential")));
  assert.deepEqual(PLAIN_CREDENTIAL_KEYS, propKeys(at(sessionEnvelope, "properties", "payload", "properties", "credential")));
  assert.deepEqual(PAYLOAD_KEYS, propKeys(at(standardAccess, "$defs", "payload")));
  assert.deepEqual(PAYLOAD_KEYS, propKeys(at(sessionEnvelope, "properties", "payload")));
  assert.deepEqual(SOURCE_KEYS, propKeys(at(standardAccess, "$defs", "source")));
  assert.deepEqual(SOURCE_KEYS, propKeys(at(sessionEnvelope, "properties", "source")));
  assert.deepEqual(SCOPE_KEYS, propKeys(at(sessionEnvelope, "properties", "scope")));
  assert.deepEqual(CLIENT_ENV_KEYS, propKeys(at(sessionEnvelope, "properties", "client_env")));
  assert.deepEqual(CLIENT_ENV_KEYS, propKeys(at(standardAccess, "properties", "client_env")));
  assert.deepEqual(CLIENT_ENV_KEYS, propKeys(at(standardAccess, "$defs", "session", "properties", "client_env")));
  assert.deepEqual(CREDENTIAL_RECORD_KEYS, propKeys(at(standardAccess, "$defs", "credential")));
  assert.deepEqual(SESSION_RECORD_KEYS, propKeys(at(standardAccess, "$defs", "session")));
  assert.deepEqual(STANDARD_ACCESS_KEYS, propKeys(standardAccess));
  assert.deepEqual(SESSION_ENVELOPE_KEYS, propKeys(sessionEnvelope));
  assert.deepEqual(SIGNATURE_KEYS, propKeys(at(converterManifest, "properties", "signature")));
  assert.deepEqual(CONVERTER_MANIFEST_KEYS, propKeys(converterManifest));
});

test("必填集合与 schema required 逐项一致", () => {
  assert.deepEqual(COOKIE_REQUIRED_KEYS, requiredKeys(cookieDef));
  assert.deepEqual(COOKIE_REQUIRED_KEYS, requiredKeys(envelopeCookie));
  assert.deepEqual(TARGET_REQUIRED_KEYS, requiredKeys(at(standardAccess, "$defs", "target")));
  assert.deepEqual(TARGET_REQUIRED_KEYS, requiredKeys(at(sessionEnvelope, "properties", "target")));
  assert.deepEqual(PLAIN_CREDENTIAL_REQUIRED_KEYS, requiredKeys(at(standardAccess, "$defs", "payload", "properties", "credential")));
  assert.deepEqual(PAYLOAD_REQUIRED_KEYS, []);
  assert.deepEqual(SOURCE_REQUIRED_KEYS, requiredKeys(at(standardAccess, "$defs", "source")));
  assert.deepEqual(SCOPE_REQUIRED_KEYS, requiredKeys(at(sessionEnvelope, "properties", "scope")));
  assert.deepEqual(CLIENT_ENV_REQUIRED_KEYS, requiredKeys(at(sessionEnvelope, "properties", "client_env")));
  assert.deepEqual(CLIENT_ENV_REQUIRED_KEYS, requiredKeys(at(standardAccess, "properties", "client_env")));
  assert.deepEqual(CLIENT_ENV_REQUIRED_KEYS, requiredKeys(at(standardAccess, "$defs", "session", "properties", "client_env")));
  assert.deepEqual(CREDENTIAL_RECORD_REQUIRED_KEYS, requiredKeys(at(standardAccess, "$defs", "credential")));
  assert.deepEqual(SESSION_RECORD_REQUIRED_KEYS, requiredKeys(at(standardAccess, "$defs", "session")));
  assert.deepEqual(STANDARD_ACCESS_REQUIRED_KEYS, requiredKeys(standardAccess));
  assert.deepEqual(SESSION_ENVELOPE_REQUIRED_KEYS, requiredKeys(sessionEnvelope));
  assert.deepEqual(SIGNATURE_REQUIRED_KEYS, requiredKeys(at(converterManifest, "properties", "signature")));
  assert.deepEqual(CONVERTER_MANIFEST_REQUIRED_KEYS, requiredKeys(converterManifest));
});

test("schema 对象一律 additionalProperties:false（严格解码的依据）", () => {
  const strictNodes: Array<[string, unknown]> = [
    ["standard-access", standardAccess],
    ["standard-access/$defs/credential", at(standardAccess, "$defs", "credential")],
    ["standard-access/$defs/cookie", cookieDef],
    ["standard-access/$defs/session", at(standardAccess, "$defs", "session")],
    ["standard-access/$defs/target", at(standardAccess, "$defs", "target")],
    ["standard-access/$defs/payload", at(standardAccess, "$defs", "payload")],
    ["standard-access/$defs/source", at(standardAccess, "$defs", "source")],
    ["standard-access/client_env", at(standardAccess, "properties", "client_env")],
    ["standard-access/$defs/session/client_env", at(standardAccess, "$defs", "session", "properties", "client_env")],
    ["converter-manifest", converterManifest],
    ["converter-manifest/signature", at(converterManifest, "properties", "signature")],
    ["session-envelope", sessionEnvelope],
    ["session-envelope/target", at(sessionEnvelope, "properties", "target")],
    ["session-envelope/payload", at(sessionEnvelope, "properties", "payload")],
    ["session-envelope/payload/credential", at(sessionEnvelope, "properties", "payload", "properties", "credential")],
    ["session-envelope/payload/cookies[]", envelopeCookie],
    ["session-envelope/source", at(sessionEnvelope, "properties", "source")],
    ["session-envelope/scope", at(sessionEnvelope, "properties", "scope")],
    ["session-envelope/client_env", at(sessionEnvelope, "properties", "client_env")],
  ];
  for (const [name, node] of strictNodes) {
    assert.equal(at(node, "additionalProperties"), false, `${name} 必须 additionalProperties:false`);
  }
});

test("pattern 与 schema 原文一致，且正则行为符合声明", () => {
  assert.equal(ID_PATTERN.source, at(standardAccess, "properties", "app_id", "pattern"));
  assert.equal(ID_PATTERN.source, at(converterManifest, "properties", "id", "pattern"));
  assert.equal(VERSION_PATTERN.source, at(converterManifest, "properties", "version", "pattern"));
  assert.equal(at(standardAccess, "properties", "app_id", "pattern"), "^[a-z0-9][a-z0-9-]{1,63}$");
  assert.equal(at(converterManifest, "properties", "version", "pattern"), "^[0-9]+\\.[0-9]+\\.[0-9]+$");

  for (const valid of ["qq", "chrome", "github", "a1", "a-b", "a".repeat(64)]) {
    assert.equal(ID_PATTERN.test(valid), true, `应接受 ${valid}`);
  }
  for (const invalid of ["", "a", "QQ", "-ab", "ab!", "中文", "a".repeat(65)]) {
    assert.equal(ID_PATTERN.test(invalid), false, `应拒绝 ${invalid}`);
  }
  for (const valid of ["1.2.0", "0.0.1", "10.20.30"]) {
    assert.equal(VERSION_PATTERN.test(valid), true, `应接受 ${valid}`);
  }
  for (const invalid of ["1.2", "v1.2.0", "1.2.0-beta", ""]) {
    assert.equal(VERSION_PATTERN.test(invalid), false, `应拒绝 ${invalid}`);
  }

  assert.equal(MODULE_SHA256_PATTERN.source, at(converterManifest, "properties", "module_sha256", "pattern"));
  assert.equal(MODULE_SHA256_PATTERN.source, "^[0-9a-f]{64}$");
  assert.equal(SIGNATURE_VALUE_PATTERN.source, at(converterManifest, "properties", "signature", "properties", "value", "pattern"));
  assert.equal(SIGNATURE_VALUE_PATTERN.source, "^[0-9a-f]{128}$");

  for (const valid of ["0".repeat(64), "a".repeat(64), "0123abcdef".repeat(6) + "0123"]) {
    assert.equal(MODULE_SHA256_PATTERN.test(valid), true, `应接受 ${valid}`);
  }
  for (const invalid of ["", "a".repeat(63), "a".repeat(65), "A".repeat(64), "g".repeat(64), "中文".repeat(32)]) {
    assert.equal(MODULE_SHA256_PATTERN.test(invalid), false, `应拒绝 ${invalid}`);
  }
  for (const invalid of ["", "a".repeat(127), "a".repeat(129), "A".repeat(128)]) {
    assert.equal(SIGNATURE_VALUE_PATTERN.test(invalid), false, `应拒绝 ${invalid}`);
  }
  // pattern 只锁形状；「全零占位值拒载」是加载方（core）的语义校验，不属本层。
  assert.equal(SIGNATURE_VALUE_PATTERN.test("0".repeat(128)), true);
});

test("DATE_TIME_PATTERN 覆盖契约 date-time 形状", () => {
  for (const valid of [
    "2026-10-09T00:00:00Z",
    "2026-10-09T00:00:00.123Z",
    "2026-10-09T00:00:00+08:00",
    "2026-10-09T00:00:00.5-05:30",
  ]) {
    assert.equal(DATE_TIME_PATTERN.test(valid), true, `应接受 ${valid}`);
  }
  for (const invalid of ["2026-10-09 00:00:00", "2026-10-09", "2026-10-09T00:00:00", ""]) {
    assert.equal(DATE_TIME_PATTERN.test(invalid), false, `应拒绝 ${invalid}`);
  }
});
