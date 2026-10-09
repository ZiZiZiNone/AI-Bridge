/**
 * bridge.ts 实测：native messaging 桥协议 v1（请求 / 响应 / 事件）。
 * 重点断言三类消息往返、方法与版本判别、非法消息逐类拒绝、
 * 以及 encode 与 decode 共用同一份校验（不会产生"能编码但解不开"的消息）。
 */
import test from "node:test";
import assert from "node:assert/strict";

import {
  BRIDGE_DIALECTS,
  BRIDGE_ERROR_CODES,
  BRIDGE_EVENTS,
  BRIDGE_MESSAGE_TYPES,
  BRIDGE_METHODS,
  BRIDGE_PROTOCOL_VERSION,
  BridgeProtocolError,
  decodeBridgeMessage,
  encodeBridgeMessage,
} from "../bridge.ts";
import type { BridgeEvent, BridgeMessage, BridgeRequest, BridgeResponse } from "../bridge.ts";
import { decodeEnvelope } from "../envelope.ts";
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

function captureRequest(): BridgeRequest {
  return {
    v: BRIDGE_PROTOCOL_VERSION,
    type: "request",
    id: "req-1",
    method: "session.capture",
    params: { envelope: cookieEnvelope() },
    sent_at: "2026-10-09T00:00:00Z",
  };
}

function hasCode(code: string): (error: unknown) => boolean {
  return (error: unknown) => error instanceof BridgeProtocolError && error.code === code;
}

/** 取出合法请求 JSON，按需改坏后返回解码动作。 */
function decodeOf(change: (message: Record<string, unknown>) => void): () => BridgeMessage {
  const raw = JSON.parse(encodeBridgeMessage(captureRequest())) as Record<string, unknown>;
  change(raw);
  const text = JSON.stringify(raw);
  return () => decodeBridgeMessage(text);
}

function bridgeTextWithout(key: string): string {
  const raw = JSON.parse(encodeBridgeMessage(captureRequest())) as Record<string, unknown>;
  const kept = Object.entries(raw).filter(([name]) => name !== key);
  return JSON.stringify(Object.fromEntries(kept));
}

function decodeRaw(value: unknown): BridgeMessage {
  return decodeBridgeMessage(JSON.stringify(value));
}

test("v1 词表固定：版本 / 消息类别 / 方法 / 事件 / 错误码 / 方言", () => {
  assert.equal(BRIDGE_PROTOCOL_VERSION, 1);
  assert.deepEqual(BRIDGE_MESSAGE_TYPES, ["request", "response", "event"]);
  assert.deepEqual(BRIDGE_METHODS, ["handshake", "session.capture", "session.inject", "credential.fill", "ping"]);
  assert.deepEqual(BRIDGE_EVENTS, ["state.changed", "broker.revoked", "host.disconnected"]);
  assert.deepEqual(BRIDGE_ERROR_CODES, ["unknown_method", "unsupported_version", "invalid_params", "denied", "internal"]);
  assert.deepEqual(BRIDGE_DIALECTS, ["chromium", "firefox"]);
});

test("request 往返：method 判别后 params 自动收窄", () => {
  const request = captureRequest();
  const decoded = decodeBridgeMessage(encodeBridgeMessage(request));
  assert.deepEqual(decoded, request);

  if (decoded.type !== "request") assert.fail("应为 request");
  assert.equal(decoded.id, "req-1");
  assert.equal(decoded.sent_at, "2026-10-09T00:00:00Z");
  if (decoded.method !== "session.capture") assert.fail("应为 session.capture");
  assert.deepEqual(decoded.params.envelope, cookieEnvelope());
});

test("response 成功往返：id 配对 + 回显 method + 结果计数", () => {
  const response: BridgeResponse = {
    v: BRIDGE_PROTOCOL_VERSION,
    type: "response",
    id: "req-1",
    ok: true,
    method: "session.inject",
    result: { injected: 3, skipped: 0 },
  };
  const decoded = decodeBridgeMessage(encodeBridgeMessage(response));
  assert.deepEqual(decoded, response);

  if (decoded.type !== "response" || !decoded.ok) assert.fail("应为成功响应");
  assert.equal(decoded.id, "req-1");
  if (decoded.method !== "session.inject") assert.fail("应为 session.inject");
  assert.equal(decoded.result.injected, 3);
  assert.equal(decoded.result.skipped, 0);
});

test("response 失败往返：错误码 + 文案", () => {
  const failure: BridgeResponse = {
    v: BRIDGE_PROTOCOL_VERSION,
    type: "response",
    id: "req-2",
    ok: false,
    error: { code: "denied", message: "未获授权" },
  };
  const decoded = decodeBridgeMessage(encodeBridgeMessage(failure));
  assert.deepEqual(decoded, failure);

  if (decoded.type !== "response" || decoded.ok) assert.fail("应为失败响应");
  assert.equal(decoded.error.code, "denied");
  assert.equal(decoded.error.message, "未获授权");
});

test("event 往返：broker.revoked 载荷", () => {
  const revoke: BridgeEvent = {
    v: BRIDGE_PROTOCOL_VERSION,
    type: "event",
    event: "broker.revoked",
    payload: { account_ref: "acc-1", revoked_at: "2026-10-09T00:00:00Z" },
    emitted_at: "2026-10-09T00:00:01Z",
  };
  const decoded = decodeBridgeMessage(encodeBridgeMessage(revoke));
  assert.deepEqual(decoded, revoke);

  if (decoded.type !== "event") assert.fail("应为 event");
  if (decoded.event !== "broker.revoked") assert.fail("应为 broker.revoked");
  assert.equal(decoded.payload.account_ref, "acc-1");
  assert.equal(decoded.emitted_at, "2026-10-09T00:00:01Z");
});

test("session.inject：协议层与契约层职责分离（信封独立校验）", () => {
  const envelope = cookieEnvelope();
  const request: BridgeRequest = {
    v: BRIDGE_PROTOCOL_VERSION,
    type: "request",
    id: "req-3",
    method: "session.inject",
    params: { envelope },
  };
  const decoded = decodeBridgeMessage(encodeBridgeMessage(request));
  if (decoded.type !== "request" || decoded.method !== "session.inject") {
    assert.fail("应为 session.inject 请求");
  }
  assert.deepEqual(decodeEnvelope(JSON.stringify(decoded.params.envelope)), envelope);
});

test("解码拒绝：版本 / 类别 / 方法 / id / 额外字段 / 参数形状", () => {
  assert.throws(decodeOf((raw) => {
    raw["v"] = 2;
  }), hasCode("unsupported_version"));
  assert.throws(decodeOf((raw) => {
    raw["type"] = "notification";
  }), hasCode("invalid_enum_value"));
  assert.throws(decodeOf((raw) => {
    raw["method"] = "vault.dump";
  }), hasCode("unknown_method"));
  assert.throws(decodeOf((raw) => {
    raw["id"] = "";
  }), hasCode("empty_field"));
  assert.throws(decodeOf((raw) => {
    raw["extra"] = true;
  }), hasCode("unexpected_field"));
  assert.throws(decodeOf((raw) => {
    raw["params"] = [];
  }), hasCode("invalid_type"));
  assert.throws(decodeOf((raw) => {
    raw["sent_at"] = 5;
  }), hasCode("invalid_type"));
  assert.throws(() => decodeBridgeMessage(bridgeTextWithout("params")), hasCode("missing_field"));
  assert.throws(() => decodeBridgeMessage(bridgeTextWithout("id")), hasCode("missing_field"));
  assert.throws(() => decodeBridgeMessage("{"), hasCode("invalid_json"));
  assert.throws(() => decodeBridgeMessage("[1,2]"), hasCode("invalid_type"));
  assert.throws(() => decodeBridgeMessage("\"request\""), hasCode("invalid_type"));
});

test("解码拒绝：响应与事件各自的形状", () => {
  assert.throws(() => decodeRaw({ v: 1, type: "response", id: "i", ok: false }), hasCode("missing_field"));
  assert.throws(() => decodeRaw({ v: 1, type: "response", id: "i", ok: false, error: { code: "boom", message: "x" } }), hasCode("invalid_enum_value"));
  assert.throws(() => decodeRaw({ v: 1, type: "response", id: "i", ok: false, error: { code: "denied" } }), hasCode("missing_field"));
  assert.throws(() => decodeRaw({ v: 1, type: "response", id: "i", ok: true, method: "ping", result: [] }), hasCode("invalid_type"));
  assert.throws(() => decodeRaw({ v: 1, type: "response", id: "i", ok: true, method: "vault.dump", result: {} }), hasCode("unknown_method"));
  assert.throws(() => decodeRaw({ v: 1, type: "response", id: "i", ok: "yes" }), hasCode("invalid_type"));
  assert.throws(() => decodeRaw({ v: 1, type: "response", id: "i", ok: true, method: "ping", result: {}, extra: 1 }), hasCode("unexpected_field"));
  assert.throws(() => decodeRaw({ v: 1, type: "event", event: "vault.opened", payload: {} }), hasCode("unknown_event"));
  assert.throws(() => decodeRaw({ v: 1, type: "event", event: "ping", payload: {} }), hasCode("unknown_event"));
  assert.throws(() => decodeRaw({ v: 1, type: "event", event: "state.changed", payload: "x" }), hasCode("invalid_type"));
  assert.throws(() => decodeRaw({ v: 1, type: "event", payload: {} }), hasCode("missing_field"));
});

test("encode 用同一个解码器校验线上字节", () => {
  const badVersion = { v: 2, type: "request", id: "req-1", method: "ping", params: {} } as unknown as BridgeMessage;
  assert.throws(() => encodeBridgeMessage(badVersion), hasCode("unsupported_version"));

  const badMethod = { v: 1, type: "request", id: "req-1", method: "vault.dump", params: {} } as unknown as BridgeMessage;
  assert.throws(() => encodeBridgeMessage(badMethod), hasCode("unknown_method"));

  const badEvent = { v: 1, type: "event", event: "vault.opened", payload: {} } as unknown as BridgeMessage;
  assert.throws(() => encodeBridgeMessage(badEvent), hasCode("unknown_event"));
});

test("四种方法的最小往返：handshake / credential.fill / ping", () => {
  const handshake: BridgeRequest = {
    v: BRIDGE_PROTOCOL_VERSION,
    type: "request",
    id: "req-h",
    method: "handshake",
    params: { protocol_version: 1, dialect: "firefox", extension_id: "aibridge-firefox" },
  };
  const fill: BridgeRequest = {
    v: BRIDGE_PROTOCOL_VERSION,
    type: "request",
    id: "req-c",
    method: "credential.fill",
    params: { target: { kind: "app", app_id: "qq" } },
  };
  const ping: BridgeRequest = {
    v: BRIDGE_PROTOCOL_VERSION,
    type: "request",
    id: "req-p",
    method: "ping",
    params: {},
  };

  for (const message of [handshake, fill, ping]) {
    assert.deepEqual(decodeBridgeMessage(encodeBridgeMessage(message)), message);
  }

  const stateChanged: BridgeEvent = {
    v: BRIDGE_PROTOCOL_VERSION,
    type: "event",
    event: "state.changed",
    payload: { account_ref: "acc-1", target: { kind: "container" } },
  };
  assert.deepEqual(decodeBridgeMessage(encodeBridgeMessage(stateChanged)), stateChanged);
});
