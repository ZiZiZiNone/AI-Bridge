/**
 * envelope.ts 实测：SessionEnvelope 编解码 + 结构校验镜像。
 * 重点断言 schema_version 值域、target 必填规则、kind/payload 一致性、额外字段拒绝、
 * scope / client_env 新字段的形状校验、cookie 属性在编解码往返中一个不丢。
 */
import test from "node:test";
import assert from "node:assert/strict";

import type { ContractErrorCode, ValidationResult } from "../envelope.ts";
import {
  EnvelopeError,
  decodeEnvelope,
  encodeEnvelope,
  validateSessionEnvelope,
  validateSessionRecord,
  validateTemporaryEnvelope,
} from "../envelope.ts";
import type { Cookie, SessionEnvelope } from "../types.ts";
import { SCHEMA_VERSION } from "../types.ts";

function cookie(): Cookie {
  return {
    name: "sid",
    value: "abc123",
    domain: "github.com",
    path: "/",
    secure: true,
    http_only: true,
    same_site: "lax",
    expires_at: "2027-01-01T00:00:00.000Z",
  };
}

function cookieEnvelope(): SessionEnvelope {
  return {
    schema_version: SCHEMA_VERSION,
    account_ref: "acc-1",
    target: { kind: "web", domain: "github.com" },
    kind: "cookie",
    payload: { cookies: [cookie()] },
    issued_at: "2026-10-09T00:00:00Z",
    source: { type: "extension", id: "chromium" },
  };
}

function expectOk(result: ValidationResult): void {
  assert.equal(result.ok, true, `应校验通过，实际：${JSON.stringify(result)}`);
}

function expectError(result: ValidationResult, code: ContractErrorCode, path: string): void {
  if (result.ok) assert.fail(`应校验失败（期望 ${code} @ ${path}），实际通过`);
  assert.equal(result.error.code, code);
  assert.equal(result.error.path, path);
}

function hasCode(code: string): (error: unknown) => boolean {
  return (error: unknown) => error instanceof EnvelopeError && error.code === code;
}

/** 取出合法信封 JSON，按需改坏后返回解码动作。 */
function decodeOf(change: (raw: Record<string, unknown>) => void): () => SessionEnvelope {
  const raw = JSON.parse(encodeEnvelope(cookieEnvelope())) as Record<string, unknown>;
  change(raw);
  const text = JSON.stringify(raw);
  return () => decodeEnvelope(text);
}

function envelopeTextWithout(key: string): string {
  const raw = JSON.parse(encodeEnvelope(cookieEnvelope())) as Record<string, unknown>;
  const kept = Object.entries(raw).filter(([name]) => name !== key);
  return JSON.stringify(Object.fromEntries(kept));
}

/** 带 scope 与 client_env 的信封（临时会话形态）。 */
function scopedEnvelope(): SessionEnvelope {
  return {
    ...cookieEnvelope(),
    expires_at: "2026-10-09T01:00:00Z",
    scope: {
      domain_allowlist: ["github.com", "gist.github.com"],
      read_only: true,
      single_use: true,
    },
    client_env: {
      user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64)",
      platform: "windows",
      device_id: "dev-1",
    },
  };
}

test("encode → decode 往返：字段顺序与 cookie 属性一个不丢", () => {
  const envelope = cookieEnvelope();
  const text = encodeEnvelope(envelope);
  assert.deepEqual(Object.keys(JSON.parse(text) as Record<string, unknown>), [
    "schema_version",
    "account_ref",
    "target",
    "kind",
    "payload",
    "issued_at",
    "source",
  ]);

  const decoded = decodeEnvelope(text);
  assert.deepEqual(decoded, envelope);

  const first = decoded.payload.cookies?.[0];
  assert.ok(first !== undefined, "应带回 cookie");
  assert.equal(first.http_only, true);
  assert.equal(first.secure, true);
  assert.equal(first.same_site, "lax");
  assert.equal(first.expires_at, "2027-01-01T00:00:00.000Z");
  assert.equal(first.domain, "github.com");
  assert.equal(first.path, "/");
});

test("带 expires_at 的信封按 schema 字段顺序编码并保留过期时间", () => {
  const envelope: SessionEnvelope = { ...cookieEnvelope(), expires_at: "2026-10-09T01:00:00Z" };
  const text = encodeEnvelope(envelope);
  assert.deepEqual(Object.keys(JSON.parse(text) as Record<string, unknown>), [
    "schema_version",
    "account_ref",
    "target",
    "kind",
    "payload",
    "issued_at",
    "expires_at",
    "source",
  ]);
  assert.equal(decodeEnvelope(text).expires_at, "2026-10-09T01:00:00Z");
});

test("scope / client_env 往返：按 schema 顺序编码、子字段一个不丢", () => {
  const envelope = scopedEnvelope();
  const text = encodeEnvelope(envelope);
  assert.deepEqual(Object.keys(JSON.parse(text) as Record<string, unknown>), [
    "schema_version",
    "account_ref",
    "target",
    "kind",
    "payload",
    "issued_at",
    "expires_at",
    "source",
    "scope",
    "client_env",
  ]);

  const decoded = decodeEnvelope(text);
  assert.deepEqual(decoded, envelope);
  assert.deepEqual(decoded.scope, envelope.scope);
  assert.deepEqual(decoded.client_env, envelope.client_env);
});

test("解码严格性：scope / client_env 是合法字段，子形状逐项校验", () => {
  // 新字段合法通过；不带也合法（可选字段，临时会话必填与否属业务语义，在 core 校验）
  expectOk(validateSessionEnvelope(cookieEnvelope()));
  expectOk(validateSessionEnvelope(scopedEnvelope()));
  expectOk(
    validateSessionEnvelope({
      ...cookieEnvelope(),
      scope: { domain_allowlist: [], read_only: false, single_use: false },
      client_env: { user_agent: "ua", platform: "web" },
    }),
  );

  // 拼错 / 未知子字段同样拒绝
  assert.throws(decodeOf((raw) => {
    raw["scope"] = { domain_allowlist: [], read_only: true, single_use: true, readonly: true };
  }), hasCode("unexpected_field"));
  assert.throws(decodeOf((raw) => {
    raw["client_env"] = { user_agent: "ua", platform: "web", deviceID: "d" };
  }), hasCode("unexpected_field"));
  // 缺必填子字段
  assert.throws(decodeOf((raw) => {
    raw["scope"] = { domain_allowlist: [], read_only: true };
  }), hasCode("missing_field"));
  assert.throws(decodeOf((raw) => {
    raw["client_env"] = { user_agent: "ua" };
  }), hasCode("missing_field"));
  // 子字段类型逐项校验
  assert.throws(decodeOf((raw) => {
    raw["scope"] = { domain_allowlist: "github.com", read_only: true, single_use: true };
  }), hasCode("invalid_type"));
  assert.throws(decodeOf((raw) => {
    raw["scope"] = { domain_allowlist: [1], read_only: true, single_use: true };
  }), hasCode("invalid_type"));
  assert.throws(decodeOf((raw) => {
    raw["scope"] = { domain_allowlist: [], read_only: "yes", single_use: true };
  }), hasCode("invalid_type"));
  assert.throws(decodeOf((raw) => {
    raw["client_env"] = [];
  }), hasCode("invalid_type"));
  assert.throws(decodeOf((raw) => {
    raw["client_env"] = { user_agent: "ua", platform: "web", device_id: 7 };
  }), hasCode("invalid_type"));
});

test("schema_version 不是 1.0 一律拒绝", () => {
  const raw = { ...cookieEnvelope(), schema_version: "0.9" };
  expectError(validateSessionEnvelope(raw), "unsupported_schema_version", "$.schema_version");
  assert.throws(() => decodeEnvelope(JSON.stringify(raw)), hasCode("unsupported_schema_version"));
});

test("target 必填规则：web 要 domain、app 要 app_id、container 无要求", () => {
  const base = cookieEnvelope();
  expectError(validateSessionEnvelope({ ...base, target: { kind: "web" } }), "target_field_missing", "target.domain");
  expectError(validateSessionEnvelope({ ...base, target: { kind: "web", domain: "" } }), "target_field_missing", "target.domain");
  expectError(validateSessionEnvelope({ ...base, target: { kind: "app" } }), "target_field_missing", "target.app_id");
  expectError(validateSessionEnvelope({ ...base, target: { kind: "app", app_id: "" } }), "target_field_missing", "target.app_id");
  expectOk(validateSessionEnvelope({ ...base, target: { kind: "app", app_id: "qq" } }));
  expectOk(validateSessionEnvelope({ ...base, target: { kind: "container" } }));
});

test("kind/payload 一致性镜像", () => {
  const base = cookieEnvelope();
  expectError(validateSessionEnvelope({ ...base, payload: {} }), "payload_mismatch", "payload.cookies");
  expectError(validateSessionEnvelope({ ...base, payload: { cookies: [] } }), "payload_mismatch", "payload.cookies");
  expectOk(validateSessionEnvelope({ ...base, kind: "token", payload: { token: "tok" } }));
  expectError(validateSessionEnvelope({ ...base, kind: "token", payload: {} }), "payload_mismatch", "payload.token");
  expectError(validateSessionEnvelope({ ...base, kind: "token", payload: { token: "" } }), "payload_mismatch", "payload.token");
  expectOk(
    validateSessionEnvelope({ ...base, kind: "credential", payload: { credential: { username: "u", secret: "s" } } }),
  );
  expectError(validateSessionEnvelope({ ...base, kind: "credential", payload: {} }), "payload_mismatch", "payload.credential");
  expectOk(
    validateSessionEnvelope({ ...base, kind: "token", payload: { token: "tok", local_storage: { access: "t" } } }),
  );
});

test("临时会话必须携带 expires_at（TTL 策略仍属 core）", () => {
  const envelope = cookieEnvelope();
  expectError(validateTemporaryEnvelope(envelope), "missing_expiry", "$.expires_at");
  expectError(validateTemporaryEnvelope({ ...envelope, expires_at: "" }), "missing_expiry", "$.expires_at");
  expectOk(validateTemporaryEnvelope({ ...envelope, expires_at: "2026-10-09T01:00:00Z" }));
});

test("会话记录校验可直接复用于 standard-access 的 sessions[]", () => {
  expectOk(validateSessionRecord({ target: { kind: "container" }, kind: "token", payload: { token: "t" } }));
  expectError(
    validateSessionRecord({ target: { kind: "web" }, kind: "token", payload: { token: "t" } }),
    "target_field_missing",
    "target.domain",
  );
  expectError(
    validateSessionRecord({ target: { kind: "container" }, kind: "cookie", payload: {} }),
    "payload_mismatch",
    "payload.cookies",
  );
});

test("解码严格性：额外字段 / 缺必填 / 类型不符 / 未知枚举 / 非法 JSON", () => {
  assert.throws(decodeOf((raw) => {
    raw["extra"] = 1;
  }), hasCode("unexpected_field"));
  assert.throws(() => decodeEnvelope(envelopeTextWithout("issued_at")), hasCode("missing_field"));
  assert.throws(decodeOf((raw) => {
    raw["kind"] = "session";
  }), hasCode("invalid_enum_value"));
  assert.throws(decodeOf((raw) => {
    raw["target"] = [];
  }), hasCode("invalid_type"));
  assert.throws(decodeOf((raw) => {
    raw["payload"] = "cookies";
  }), hasCode("invalid_type"));
  assert.throws(() => decodeEnvelope("{"), hasCode("invalid_json"));
  assert.throws(() => decodeEnvelope("[]"), hasCode("invalid_type"));
});

test("解码严格性：cookie 内部字段同样逐项校验", () => {
  assert.throws(decodeOf((raw) => {
    const payload = raw["payload"] as { cookies: Array<Record<string, unknown>> };
    payload.cookies[0]!["secure"] = "yes";
  }), hasCode("invalid_type"));
  assert.throws(decodeOf((raw) => {
    const payload = raw["payload"] as { cookies: Array<Record<string, unknown>> };
    payload.cookies[0]!["same_site"] = "weird";
  }), hasCode("invalid_enum_value"));
  assert.throws(decodeOf((raw) => {
    const payload = raw["payload"] as { cookies: Array<Record<string, unknown>> };
    payload.cookies[0]!["http_only"] = null;
  }), hasCode("invalid_type"));
  assert.throws(decodeOf((raw) => {
    const payload = raw["payload"] as { cookies: Array<Record<string, unknown>> };
    payload.cookies[0]!["storeId"] = "0";
  }), hasCode("unexpected_field"));
  assert.throws(decodeOf((raw) => {
    const payload = raw["payload"] as { cookies: Array<Record<string, unknown>> };
    payload.cookies[0] = { name: "sid" };
  }), hasCode("missing_field"));
});

test("编码前先校验：非法信封不产生任何字节", () => {
  const broken: SessionEnvelope = { ...cookieEnvelope(), schema_version: "2.0" };
  assert.throws(() => encodeEnvelope(broken), hasCode("unsupported_schema_version"));
  const noCookies: SessionEnvelope = { ...cookieEnvelope(), payload: {} };
  assert.throws(() => encodeEnvelope(noCookies), hasCode("payload_mismatch"));
});
