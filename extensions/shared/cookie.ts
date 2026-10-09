/**
 * cookie.ts —— 浏览器 cookie ↔ 契约 Cookie 的双向映射。
 *
 * 事实源：`contracts/standard-access.schema.json` 的 `$defs.cookie`
 *         （session-envelope 内联 cookie 与之同形），类型见 types.ts 的 `Cookie`。
 *
 * 会话保真度关键：http_only / secure / same_site / expires_at 与 domain / path 全程不丢；
 * 未知或畸形输入一律抛 CookieMappingError，绝不静默丢属性。
 *
 * 宿主无关：本文件不 import 任何浏览器 API 或 Node API，浏览器侧类型以结构化子集声明
 * （Chromium / Firefox 的 WebExtensions `cookies.Cookie` 同形），因此同一份实现可在
 * 扩展、单测、任意宿主里直接工作。
 */
import type { Cookie, SameSite } from "./types.ts";
import { DATE_TIME_PATTERN } from "./types.ts";

/**
 * WebExtensions 的 sameSite 取值。
 * `no_restriction` = 契约的 `none`；`unspecified` = 契约里省略 `same_site`。
 */
export type BrowserSameSite = "no_restriction" | "lax" | "strict" | "unspecified";

/**
 * 浏览器 cookie 的结构化子集（读取方向）。
 * 浏览器专属字段（storeId / partitionKey / firstPartyDomain / hostOnly 等）不在契约内，
 * 契约 `additionalProperties: false` 无法承载，由目标浏览器自行推导。
 */
export interface BrowserCookie {
  name: string;
  value: string;
  domain?: string;
  path?: string;
  secure?: boolean;
  httpOnly?: boolean;
  sameSite?: BrowserSameSite;
  /** 秒级 Unix 时间戳；会话 cookie 无此字段。 */
  expirationDate?: number;
  /** true 表示会话 cookie（关闭浏览器即失效）。 */
  session?: boolean;
}

/** `cookies.set` 需要的写入参数：cookie 原文 + 必填的 url。 */
export interface BrowserCookieSetDetails {
  url: string;
  name: string;
  value: string;
  domain?: string;
  path?: string;
  secure?: boolean;
  httpOnly?: boolean;
  sameSite?: BrowserSameSite;
  expirationDate?: number;
}

/** cookie 映射失败：字段不可承载、类型不符或值域越界。 */
export class CookieMappingError extends Error {
  field: string;

  constructor(field: string, detail: string) {
    super(`cookie 映射失败：${field} ${detail}`);
    this.name = "CookieMappingError";
    this.field = field;
  }
}

const SAME_SITE_FROM_BROWSER: Readonly<Record<BrowserSameSite, SameSite | undefined>> = {
  strict: "strict",
  lax: "lax",
  no_restriction: "none",
  unspecified: undefined,
};

const SAME_SITE_TO_BROWSER: Readonly<Record<SameSite, BrowserSameSite>> = {
  strict: "strict",
  lax: "lax",
  none: "no_restriction",
};

function isPresentString(value: unknown): value is string {
  return typeof value === "string" && value.length > 0;
}

function requireString(value: unknown, field: string): string {
  if (typeof value !== "string") {
    throw new CookieMappingError(field, `应为 string，实为 ${typeof value}`);
  }
  return value;
}

function secondsToDateTime(seconds: number): string {
  const date = new Date(Math.round(seconds * 1000));
  if (Number.isNaN(date.getTime())) {
    throw new CookieMappingError("expirationDate", "不是可识别的时间");
  }
  try {
    return date.toISOString();
  } catch {
    throw new CookieMappingError("expirationDate", "超出可表示的时间范围");
  }
}

function dateTimeToSeconds(expiresAt: string): number {
  if (!DATE_TIME_PATTERN.test(expiresAt) || Number.isNaN(Date.parse(expiresAt))) {
    throw new CookieMappingError("expires_at", `不是合法 date-time：${expiresAt}`);
  }
  return Math.round(Date.parse(expiresAt) / 1000);
}

/**
 * 浏览器 cookie → 契约 Cookie。
 * `session: true` 的会话 cookie 不产生 `expires_at`；`unspecified` 不产生 `same_site`。
 */
export function toContractCookie(raw: BrowserCookie): Cookie {
  const cookie: Cookie = {
    name: requireString(raw.name, "name"),
    value: requireString(raw.value, "value"),
  };

  if (isPresentString(raw.domain)) cookie.domain = raw.domain;
  if (isPresentString(raw.path)) cookie.path = raw.path;
  if (typeof raw.secure === "boolean") cookie.secure = raw.secure;
  if (typeof raw.httpOnly === "boolean") cookie.http_only = raw.httpOnly;

  if (raw.sameSite !== undefined) {
    if (!Object.prototype.hasOwnProperty.call(SAME_SITE_FROM_BROWSER, raw.sameSite)) {
      throw new CookieMappingError("sameSite", `取值不支持：${String(raw.sameSite)}`);
    }
    const mapped = SAME_SITE_FROM_BROWSER[raw.sameSite];
    if (mapped !== undefined) cookie.same_site = mapped;
  }

  if (raw.session !== true && raw.expirationDate !== undefined) {
    if (typeof raw.expirationDate !== "number" || !Number.isFinite(raw.expirationDate)) {
      throw new CookieMappingError("expirationDate", "不是有限数字");
    }
    cookie.expires_at = secondsToDateTime(raw.expirationDate);
  }

  return cookie;
}

/**
 * 契约 Cookie → 浏览器 cookie。
 * 缺 `expires_at` 即会话 cookie（不回写 `session` 字段，由浏览器按 expirationDate 缺省推导）。
 */
export function toBrowserCookie(cookie: Cookie): BrowserCookie {
  const out: BrowserCookie = {
    name: requireString(cookie.name, "name"),
    value: requireString(cookie.value, "value"),
  };

  if (isPresentString(cookie.domain)) out.domain = cookie.domain;
  if (isPresentString(cookie.path)) out.path = cookie.path;
  if (typeof cookie.secure === "boolean") out.secure = cookie.secure;
  if (typeof cookie.http_only === "boolean") out.httpOnly = cookie.http_only;

  if (cookie.same_site !== undefined) {
    if (!Object.prototype.hasOwnProperty.call(SAME_SITE_TO_BROWSER, cookie.same_site)) {
      throw new CookieMappingError("same_site", `取值不支持：${String(cookie.same_site)}`);
    }
    out.sameSite = SAME_SITE_TO_BROWSER[cookie.same_site];
  }

  if (cookie.expires_at !== undefined) {
    out.expirationDate = dateTimeToSeconds(cookie.expires_at);
  }

  return out;
}

/**
 * 契约 Cookie → `cookies.set` 写入参数（含 url）。
 * url 由 domain + path 推导：`secure` 为 true 用 https，domain 前导点（含子域）在 URL 中去掉。
 */
export function toBrowserCookieSetDetails(cookie: Cookie): BrowserCookieSetDetails {
  const mapped = toBrowserCookie(cookie);
  const domain = mapped.domain;
  if (!isPresentString(domain)) {
    throw new CookieMappingError("domain", "缺失，无法构造 cookie 写入 URL");
  }
  const host = domain.startsWith(".") ? domain.slice(1) : domain;
  const path = isPresentString(mapped.path) ? mapped.path : "/";

  const details: BrowserCookieSetDetails = {
    url: `${mapped.secure === true ? "https" : "http"}://${host}${path}`,
    name: mapped.name,
    value: mapped.value,
  };
  details.domain = domain;
  details.path = path;
  if (mapped.secure !== undefined) details.secure = mapped.secure;
  if (mapped.httpOnly !== undefined) details.httpOnly = mapped.httpOnly;
  if (mapped.sameSite !== undefined) details.sameSite = mapped.sameSite;
  if (mapped.expirationDate !== undefined) details.expirationDate = mapped.expirationDate;

  return details;
}
