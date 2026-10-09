/**
 * bridge.ts —— native messaging 桥消息协议 v1（扩展侧）。
 *
 * 位置：桌面 `desktop/bridge/`（messaging_host / ext_protocol）与 `extensions/chromium|firefox`
 * 之间只走这一份消息定义，双方不引对方代码（契约层 ← 核心层 ← 宿主层的单向依赖）。
 *
 * 三条纪律：
 * 1. 三类消息：request（扩展→宿主）/ response（宿主→扩展，与 request.id 配对）/ event（宿主→扩展，无配对）。
 * 2. 消息体只承载契约类型（SessionEnvelope / Cookie / Target / CredentialRecord），不发明业务字段。
 * 3. 本层只做协议编解码与形状校验：参数与结果里的**业务**合法性（信封能否解码、目标是否获授权）
 *    由方法处理器用 envelope.ts / core 规则校验。
 *
 * 传输层边界：native messaging 的 4 字节小端长度前缀由宿主侧 `desktop/bridge/messaging_host`
 * 负责，扩展侧由浏览器 `runtime.connectNative` 端口负责，本文件不实现分帧。
 */
import type { CredentialRecord, SessionEnvelope, Target } from "./types.ts";
import type { ShapeErrorCode } from "./envelope.ts";
import { checkShape, isPlainObject } from "./envelope.ts";

/** 协议版本；`v` 与之不符一律拒绝。 */
export const BRIDGE_PROTOCOL_VERSION = 1;

export const BRIDGE_MESSAGE_TYPES = ["request", "response", "event"] as const;
export type BridgeMessageType = (typeof BRIDGE_MESSAGE_TYPES)[number];

/** 扩展方言：同一份协议被 Chromium 与 Firefox 两侧共用。 */
export const BRIDGE_DIALECTS = ["chromium", "firefox"] as const;
export type BridgeDialect = (typeof BRIDGE_DIALECTS)[number];

/**
 * v1 方法面（覆盖四条数据流里扩展参与的部分）：
 * handshake=版本/方言协商；session.capture=扩展读 cookie → 归一化上报；
 * session.inject=宿主下发信封 → 扩展注入；credential.fill=取候选填表；ping=存活探测。
 */
export const BRIDGE_METHODS = [
  "handshake",
  "session.capture",
  "session.inject",
  "credential.fill",
  "ping",
] as const;
export type BridgeMethod = (typeof BRIDGE_METHODS)[number];

/** v1 事件面：状态变化 / 临时会话撤销 / 宿主断开。 */
export const BRIDGE_EVENTS = ["state.changed", "broker.revoked", "host.disconnected"] as const;
export type BridgeEventName = (typeof BRIDGE_EVENTS)[number];

/** response 里 `ok: false` 时的错误码。 */
export const BRIDGE_ERROR_CODES = [
  "unknown_method",
  "unsupported_version",
  "invalid_params",
  "denied",
  "internal",
] as const;
export type BridgeErrorCode = (typeof BRIDGE_ERROR_CODES)[number];

export interface BridgeError {
  code: BridgeErrorCode;
  message: string;
}

/** 方法 → 参数/结果类型。结果只回契约类型或计数，不回业务判断。 */
export interface BridgeMethodMap {
  handshake: {
    params: { protocol_version: number; dialect: BridgeDialect; extension_id: string };
    result: { protocol_version: number; host_version: string };
  };
  "session.capture": {
    params: { envelope: SessionEnvelope };
    result: { stored: boolean };
  };
  "session.inject": {
    params: { envelope: SessionEnvelope };
    result: { injected: number; skipped: number };
  };
  "credential.fill": {
    params: { target: Target };
    result: { credentials: CredentialRecord[] };
  };
  ping: {
    params: Record<string, never>;
    result: { ok: true };
  };
}

/** 事件 → 载荷类型。 */
export interface BridgeEventPayloadMap {
  "state.changed": { account_ref: string; target: Target };
  "broker.revoked": { account_ref: string; revoked_at: string };
  "host.disconnected": { reason: string };
}

/* ---------------- 三类消息 ---------------- */

export interface BridgeRequestOf<M extends BridgeMethod = BridgeMethod> {
  v: typeof BRIDGE_PROTOCOL_VERSION;
  type: "request";
  /** 请求标识，response 用它配对；同一端口内唯一。 */
  id: string;
  method: M;
  params: BridgeMethodMap[M]["params"];
  sent_at?: string;
}

export interface BridgeSuccessResponseOf<M extends BridgeMethod = BridgeMethod> {
  v: typeof BRIDGE_PROTOCOL_VERSION;
  type: "response";
  id: string;
  ok: true;
  /** 回显方法名，便于响应自描述。 */
  method: M;
  result: BridgeMethodMap[M]["result"];
}

export interface BridgeErrorResponse {
  v: typeof BRIDGE_PROTOCOL_VERSION;
  type: "response";
  id: string;
  ok: false;
  error: BridgeError;
}

export interface BridgeEventOf<E extends BridgeEventName = BridgeEventName> {
  v: typeof BRIDGE_PROTOCOL_VERSION;
  type: "event";
  event: E;
  payload: BridgeEventPayloadMap[E];
  emitted_at?: string;
}

/** 按 method 判别：`request.method === "session.inject"` 后 params 自动收窄到该方法的形状。 */
export type BridgeRequest = { [M in BridgeMethod]: BridgeRequestOf<M> }[BridgeMethod];
export type BridgeSuccessResponse = { [M in BridgeMethod]: BridgeSuccessResponseOf<M> }[BridgeMethod];
export type BridgeResponse = BridgeSuccessResponse | BridgeErrorResponse;
export type BridgeEvent = { [E in BridgeEventName]: BridgeEventOf<E> }[BridgeEventName];

export type BridgeMessage = BridgeRequest | BridgeResponse | BridgeEvent;

/* ---------------- 编解码 ---------------- */

/** 编解码错误码：JSON/形状层（与 response 里的 BridgeErrorCode 是两套词表）。 */
export type BridgeCodecErrorCode =
  | ShapeErrorCode
  | "invalid_json"
  | "unsupported_version"
  | "invalid_enum_value"
  | "invalid_type"
  | "empty_field"
  | "unknown_method"
  | "unknown_event";

/** 桥消息不合法：字段路径 + 错误码，供调用方按结构分支。 */
export class BridgeProtocolError extends Error {
  code: BridgeCodecErrorCode;
  path: string;

  constructor(code: BridgeCodecErrorCode, path: string, detail: string) {
    super(`桥消息不合法：${code} @ ${path} —— ${detail}`);
    this.name = "BridgeProtocolError";
    this.code = code;
    this.path = path;
  }
}

const REQUEST_KEYS = ["v", "type", "id", "method", "params", "sent_at"] as const;
const REQUEST_REQUIRED_KEYS = ["v", "type", "id", "method", "params"] as const;
const SUCCESS_RESPONSE_KEYS = ["v", "type", "id", "ok", "method", "result"] as const;
const SUCCESS_RESPONSE_REQUIRED_KEYS = ["v", "type", "id", "ok", "method", "result"] as const;
const ERROR_RESPONSE_KEYS = ["v", "type", "id", "ok", "error"] as const;
const ERROR_RESPONSE_REQUIRED_KEYS = ["v", "type", "id", "ok", "error"] as const;
const EVENT_KEYS = ["v", "type", "event", "payload", "emitted_at"] as const;
const EVENT_REQUIRED_KEYS = ["v", "type", "event", "payload"] as const;
const BRIDGE_ERROR_KEYS = ["code", "message"] as const;

function fail(code: BridgeCodecErrorCode, path: string, detail: string): never {
  throw new BridgeProtocolError(code, path, detail);
}

/** 按 schema 式形状检查（与 envelope.ts 共用同一实现）。 */
function requireShape(
  value: Record<string, unknown>,
  path: string,
  keys: readonly string[],
  required: readonly string[],
): void {
  const problem = checkShape(value, path, keys, required);
  if (problem !== undefined) fail(problem.code, problem.path, problem.detail);
}

function requireObject(value: unknown, path: string): Record<string, unknown> {
  if (!isPlainObject(value)) fail("invalid_type", path, `应为对象，实为 ${typeof value}`);
  return value;
}

function requireString(value: unknown, path: string): string {
  if (typeof value !== "string") {
    fail("invalid_type", path, `应为 string，实为 ${typeof value}`);
  }
  return value;
}

function requireNonEmptyString(value: unknown, path: string): string {
  const text = requireString(value, path);
  if (text.length === 0) fail("empty_field", path, "不能为空字符串");
  return text;
}

function requireEnum<T extends string>(value: unknown, values: readonly T[], path: string): T {
  if (typeof value !== "string") {
    fail("invalid_type", path, `应为 string，实为 ${typeof value}`);
  }
  if (!(values as readonly string[]).includes(value)) {
    fail("invalid_enum_value", path, `取值须为 ${values.join(" | ")}，收到 ${String(value)}`);
  }
  return value as T;
}

/** 方法名单独一枚错误码：调用方按码区分"协议里没这个方法"与其他形状错误。 */
function requireMethod(value: unknown): BridgeMethod {
  if (typeof value !== "string") {
    fail("invalid_type", "$.method", `应为 string，实为 ${typeof value}`);
  }
  if (!(BRIDGE_METHODS as readonly string[]).includes(value)) {
    fail("unknown_method", "$.method", `v1 未定义该方法：${value}`);
  }
  return value as BridgeMethod;
}

/** 事件名单独一枚错误码。 */
function requireEventName(value: unknown): BridgeEventName {
  if (typeof value !== "string") {
    fail("invalid_type", "$.event", `应为 string，实为 ${typeof value}`);
  }
  if (!(BRIDGE_EVENTS as readonly string[]).includes(value)) {
    fail("unknown_event", "$.event", `v1 未定义该事件：${value}`);
  }
  return value as BridgeEventName;
}

function requireProtocolVersion(value: unknown): void {
  if (value !== BRIDGE_PROTOCOL_VERSION) {
    fail(
      "unsupported_version",
      "$.v",
      `仅支持协议版本 ${BRIDGE_PROTOCOL_VERSION}，收到 ${String(value)}`,
    );
  }
}

function decodeRequest(value: Record<string, unknown>): BridgeRequest {
  requireShape(value, "$", REQUEST_KEYS, REQUEST_REQUIRED_KEYS);
  const id = requireNonEmptyString(value.id, "$.id");
  const method = requireMethod(value.method);
  const params = requireObject(value.params, "$.params");

  // params 的内容形状按方法由处理器校验（文件头第 3 条），此处只做协议层收窄。
  const request = {
    v: BRIDGE_PROTOCOL_VERSION,
    type: "request",
    id,
    method,
    params,
  } as unknown as BridgeRequest;
  if (value.sent_at !== undefined) {
    request.sent_at = requireString(value.sent_at, "$.sent_at");
  }
  return request;
}

function decodeResponse(value: Record<string, unknown>): BridgeResponse {
  const ok = value.ok;
  if (typeof ok !== "boolean") fail("invalid_type", "$.ok", `应为 boolean，实为 ${typeof ok}`);
  const id = requireNonEmptyString(value.id, "$.id");

  if (ok) {
    requireShape(value, "$", SUCCESS_RESPONSE_KEYS, SUCCESS_RESPONSE_REQUIRED_KEYS);
    const method = requireMethod(value.method);
    const result = requireObject(value.result, "$.result");
    return {
      v: BRIDGE_PROTOCOL_VERSION,
      type: "response",
      id,
      ok: true,
      method,
      result,
    } as unknown as BridgeSuccessResponse;
  }

  requireShape(value, "$", ERROR_RESPONSE_KEYS, ERROR_RESPONSE_REQUIRED_KEYS);
  const raw = requireObject(value.error, "$.error");
  requireShape(raw, "$.error", BRIDGE_ERROR_KEYS, BRIDGE_ERROR_KEYS);
  const code = requireEnum(raw.code, BRIDGE_ERROR_CODES, "$.error.code");
  const message = requireString(raw.message, "$.error.message");
  return {
    v: BRIDGE_PROTOCOL_VERSION,
    type: "response",
    id,
    ok: false,
    error: { code, message },
  };
}

function decodeEvent(value: Record<string, unknown>): BridgeEvent {
  requireShape(value, "$", EVENT_KEYS, EVENT_REQUIRED_KEYS);
  const event = requireEventName(value.event);
  const payload = requireObject(value.payload, "$.payload");

  const message = {
    v: BRIDGE_PROTOCOL_VERSION,
    type: "event",
    event,
    payload,
  } as unknown as BridgeEvent;
  if (value.emitted_at !== undefined) {
    message.emitted_at = requireString(value.emitted_at, "$.emitted_at");
  }
  return message;
}

/**
 * 解码：JSON → 版本 → 消息类别 → 该类别的形状。
 * 非法消息抛 BridgeProtocolError，绝不返回半个对象。
 */
export function decodeBridgeMessage(text: string): BridgeMessage {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch (cause) {
    fail("invalid_json", "$", `不是合法 JSON：${String(cause)}`);
  }

  const message = requireObject(value, "$");
  requireProtocolVersion(message.v);
  const type = requireEnum(message.type, BRIDGE_MESSAGE_TYPES, "$.type");

  if (type === "request") return decodeRequest(message);
  if (type === "response") return decodeResponse(message);
  return decodeEvent(message);
}

/**
 * 编码：先序列化，再用**同一个解码器**校验线上字节。
 * 一份规则两处使用，杜绝"能编码但解不开"的消息。
 */
export function encodeBridgeMessage(message: BridgeMessage): string {
  const text = JSON.stringify(message);
  decodeBridgeMessage(text);
  return text;
}
