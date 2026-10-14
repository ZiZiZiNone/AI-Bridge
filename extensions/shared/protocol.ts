/**
 * protocol.ts —— 扩展侧协议装配：client_env（UA/平台）随行、Target 装配、扩展内部动作消息。
 *
 * 职责与边界：
 * - 桥消息信封与方法面在 `bridge.ts`（既有唯一实现），本文件不重复定义、不做转发导出。
 *   **bridge 协议待与 desktop/bridge 联调时定稿**（见 `extensions/shared/README.md`）。
 * - 扩展内部动作（popup / content / background 之间）在本文件定义：只含契约字段
 *   （SessionEnvelope / StandardAccess / Target / account_ref）与少量控制字段，禁止自造业务字段；
 *   内部动作不跨 desktop 桥（desktop 桥只承载 bridge.ts 的消息）。
 * - client_env 随行（安全模型 §7'）：捕获会话时记录 UA/平台并随信封扩散；注入容器的
 *   对齐逻辑与风控提示属 core / 宿主层，本层只做机械采集与映射。
 * - 纯函数与常量：不依赖浏览器 API / Node API，可在任意宿主直接工作（极致解耦判据）。
 * - 禁止业务规则：scope / 去重 / 健康分级 / TTL / 能否扩散均属 core，本层只做结构装配。
 */
import type { PushTarget } from "./bridge.ts";
import { isPlainObject } from "./envelope.ts";
import type {
  ClientEnv,
  Cookie,
  CredentialRecord,
  SessionEnvelope,
  StandardAccess,
  Target,
} from "./types.ts";
import { SCHEMA_VERSION } from "./types.ts";

/**
 * native messaging 宿主名（扩展侧 connectNative 用）。
 * 最终取值待与 desktop/bridge 的宿主注册文件对齐——待确认。
 */
export const BRIDGE_NATIVE_HOST_NAME = "com.aibridge.bridge";

/* ---------------- Target 装配（纯机械） ---------------- */

/** 构造 web 目标；domain 为空视为装配错误（kind=web 的 domain 必填，契约语义）。 */
export function webTarget(domain: string): Target {
  const host = domain.trim().toLowerCase();
  if (host.length === 0) {
    throw new Error("webTarget：domain 不能为空");
  }
  return { kind: "web", domain: host };
}

/**
 * 从 URL 提取主机名（机械解析，小写）。
 * 不引入 URL 全局（shared 层无 DOM / Node 依赖）；只处理常规绝对 URL，
 * 解析失败返回空串，由调用方决定如何呈现（UI 显示空状态，不在本层判断）。
 */
export function domainFromUrl(url: string): string {
  let rest = url.trim();
  const schemeIndex = rest.indexOf("://");
  if (schemeIndex >= 0) rest = rest.slice(schemeIndex + 3);
  for (const stop of ["/", "?", "#"]) {
    const index = rest.indexOf(stop);
    if (index >= 0) rest = rest.slice(0, index);
  }
  const atIndex = rest.lastIndexOf("@");
  if (atIndex >= 0) rest = rest.slice(atIndex + 1);
  const colonIndex = rest.indexOf(":");
  if (colonIndex >= 0) rest = rest.slice(0, colonIndex);
  return rest.trim().toLowerCase().replace(/\.$/, "");
}

/* ---------------- SessionEnvelope 装配（捕获随行） ---------------- */

/** 会话信封装配入参（全部来自调用方显式传入，本层不取数、不判断）。 */
export interface SessionEnvelopeInput {
  accountRef: string;
  target: Target;
  cookies: Cookie[];
  clientEnv: ClientEnv;
  /** source.id：扩展方言（chromium / firefox）。 */
  sourceId: string;
  issuedAt?: string;
}

/**
 * 装配 cookie 类会话信封（B1.1 读取 → 扩散载体）。
 * client_env 随行（安全模型 §7'）；cookie 完整属性由 cookie.ts 映射层保证。
 * account_ref 必须来自 core（库内条目引用），本层不生成、不猜测。
 */
export function buildSessionEnvelope(input: SessionEnvelopeInput): SessionEnvelope {
  const envelope: SessionEnvelope = {
    schema_version: SCHEMA_VERSION,
    account_ref: input.accountRef,
    target: input.target,
    kind: "cookie",
    payload: { cookies: input.cookies },
    issued_at: input.issuedAt ?? new Date().toISOString(),
    source: { type: "extension", id: input.sourceId },
    client_env: input.clientEnv,
  };
  return envelope;
}

/* ---------------- client_env（UA / 平台）随行 ---------------- */

/** 契约 client_env.platform 词表（contracts/*.schema.json 的 client_env.description）。 */
export const CLIENT_ENV_PLATFORMS = ["windows", "macos", "linux", "android", "ios", "web"] as const;
export type ClientEnvPlatform = (typeof CLIENT_ENV_PLATFORMS)[number];

/**
 * UA → 平台标识的机械映射（词表来源：契约 client_env.platform 描述）。
 * 顺序敏感：android / ios 的 UA 含 linux / Mac OS X 字样，必须先判。
 * UA 不可判时兜底 "web"（契约词表内的兜底值）。推导规则待与 core 对齐——待确认。
 */
const PLATFORM_PATTERNS: ReadonlyArray<readonly [ClientEnvPlatform, RegExp]> = [
  ["android", /android/i],
  ["ios", /iphone|ipad|ipod/i],
  ["windows", /windows|win32|win64|wow64/i],
  ["macos", /macintosh|mac os x/i],
  ["linux", /linux|x11|cros/i],
];

export function resolvePlatform(userAgent: string): ClientEnvPlatform {
  for (const entry of PLATFORM_PATTERNS) {
    const platform = entry[0];
    const pattern = entry[1];
    if (pattern.test(userAgent)) return platform;
  }
  return "web";
}

/** buildClientEnv 入参：UA 原文 + 已解析的平台标识（显式传入，不在函数内藏推导）。 */
export interface ClientEnvInput {
  userAgent: string;
  platform: ClientEnvPlatform;
  /** 可选设备指纹标识；扩展侧无稳定标识来源时留空（契约语义：尽力记录，不承诺可对齐）。 */
  deviceId?: string;
}

/** 组装契约 ClientEnv（捕获随行 / 注入对齐的载体，安全模型 §7'）。 */
export function buildClientEnv(input: ClientEnvInput): ClientEnv {
  const env: ClientEnv = {
    user_agent: input.userAgent,
    platform: input.platform,
  };
  if (input.deviceId !== undefined && input.deviceId.length > 0) env.device_id = input.deviceId;
  return env;
}

/* ---------------- 扩展内部动作（不跨 desktop 桥） ---------------- */

/**
 * popup / content / background 之间的动作消息。每条都对应一处 UI 消费点：
 * session.query=X1 账号列表、session.apply=B1 注入按钮、session.capture.push=B2 推送、
 * push.targets=B2 目标设备选择、credential.fill=B1 复制与 B3.1 填表数据、bridge.ping=X1 桥状态、
 * content.fill=X1 填充按钮 → X2 浮标填充。
 */
export type ExtensionAction =
  | { kind: "session.query"; target: Target }
  | { kind: "session.apply"; account_ref: string; target: Target }
  | { kind: "session.capture.push"; account_ref: string; target: Target; destination: string }
  | { kind: "push.targets" }
  | { kind: "credential.fill"; target: Target; account_ref?: string }
  | { kind: "bridge.handshake" }
  | { kind: "bridge.ping" }
  | { kind: "content.fill"; account_ref: string };

export type ExtensionActionKind = ExtensionAction["kind"];

/** 动作结果映射：只回契约类型或计数/标志（控制字段），不回业务判断。 */
export interface ExtensionActionResultMap {
  "session.query": { access: StandardAccess };
  "session.apply": { injected: number; skipped: number };
  "session.capture.push": { stored: boolean; delivered?: boolean };
  "push.targets": { targets: PushTarget[] };
  "credential.fill": { credentials: CredentialRecord[] };
  "bridge.handshake": { protocol_version: number; host_version: string };
  "bridge.ping": { connected: boolean };
  "content.fill": { filled: boolean };
}

/** 内部动作错误码（与 bridge 响应错误码是两套词表：这套描述扩展自身通路失败）。 */
export const EXTENSION_ACTION_ERROR_CODES = [
  "disconnected",
  "host_error",
  "protocol_error",
  "invalid_action",
  "internal",
] as const;
export type ExtensionActionErrorCode = (typeof EXTENSION_ACTION_ERROR_CODES)[number];

export interface ExtensionActionError {
  code: ExtensionActionErrorCode;
  message: string;
}

export type ExtensionActionResponseOf<K extends ExtensionActionKind> =
  | { ok: true; result: ExtensionActionResultMap[K] }
  | { ok: false; error: ExtensionActionError };

export type ExtensionActionResponse = {
  [K in ExtensionActionKind]: ExtensionActionResponseOf<K>;
}[ExtensionActionKind];

function hasStringField(value: Record<string, unknown>, key: string): boolean {
  return typeof value[key] === "string" && (value[key] as string).length > 0;
}

function isTargetLike(value: unknown): boolean {
  return isPlainObject(value) && typeof value.kind === "string";
}

/** 结构性判别（形状检查，不含业务合法性：业务校验在 core）。 */
export function isExtensionAction(value: unknown): value is ExtensionAction {
  if (!isPlainObject(value)) return false;
  switch (value.kind) {
    case "session.query":
      return isTargetLike(value.target);
    case "session.apply":
      return hasStringField(value, "account_ref") && isTargetLike(value.target);
    case "session.capture.push":
      return (
        hasStringField(value, "account_ref") &&
        isTargetLike(value.target) &&
        hasStringField(value, "destination")
      );
    case "push.targets":
    case "bridge.handshake":
    case "bridge.ping":
      return true;
    case "credential.fill":
      // account_ref 可选（浮标路径由 core 匹配账号）；有值时必须非空。
      return (
        isTargetLike(value.target) &&
        (value.account_ref === undefined || hasStringField(value, "account_ref"))
      );
    case "content.fill":
      return hasStringField(value, "account_ref");
    default:
      return false;
  }
}
