/**
 * cookie_io.ts —— 浏览器 cookies API 读写封装：完整属性读取 / 注入 / 清除 / 目标登录态查询。
 *
 * 职责与边界：
 * - 对上层（background 调度）提供 cookie 数据操作；对下只依赖 `cookie.ts` 的双向映射
 *   （映射层已锁定完整属性：name/value/domain/path/secure/http_only/same_site/expires_at
 *   一个不丢——Cookie「丢属性 = 会话损坏」，见 core/contract）。
 * - **禁止业务规则**：哪些 cookie 该读/该注入/算不算登录态，属 core（ab-state）判断；
 *   本层只按调用方给出的 domain / cookie 清单做机械读写。
 * - 日志与错误文案不含 cookie 值（明文纪律）；目标登录态查询只返回元数据，不返回 value。
 * - 浏览器 API 以结构化接口 `CookiesApi` 显式传入（模块解耦），本文件不 import
 *   浏览器全局，可在单测里用假实现直接工作。
 */
import type { BrowserCookie, BrowserCookieSetDetails } from "./cookie.ts";
import { toBrowserCookieSetDetails, toContractCookie } from "./cookie.ts";
import type { Cookie } from "./types.ts";

/** WebExtensions cookies API 的结构化子集（Promise 形态，由调用方注入浏览器实现）。 */
export interface CookiesApi {
  getAll(details: CookieQueryDetails): Promise<BrowserCookie[]>;
  set(details: BrowserCookieSetDetails): Promise<unknown>;
  remove(details: CookieRemovalDetails): Promise<unknown>;
}

export interface CookieQueryDetails {
  url?: string;
  domain?: string;
  name?: string;
}

export interface CookieRemovalDetails {
  url: string;
  name: string;
  path?: string;
}

/** 注入计数（与桥方法 session.inject 的结果同形：控制字段，只报数量）。 */
export interface InjectCounts {
  injected: number;
  skipped: number;
}

/** 清除计数（控制字段，只报数量，不回传值）。 */
export interface ClearCounts {
  removed: number;
  failed: number;
}

/**
 * 目标登录态元数据（B1.3 展示用）：契约 Cookie 去掉 value。
 * 保真度要求（A2）适用于会话传输路径（读取上报 / 注入）；状态展示不需要值，
 * 也不把值送到 UI 层（内存最小化）。
 */
export type CookieFacts = Omit<Cookie, "value">;

function factsOf(cookie: Cookie): CookieFacts {
  // value 有意丢弃：状态展示元数据不含值（与传输路径的零丢弃是两回事）。
  const { value: _omitted, ...facts } = cookie;
  return facts;
}

/**
 * 读取域名下的全部 cookie（含 HttpOnly），映射为契约 Cookie（完整属性）。
 * domain 过滤按浏览器语义包含子域。B1.1 会话读取。
 */
export async function readDomainCookies(api: CookiesApi, domain: string): Promise<Cookie[]> {
  const raw = await api.getAll({ domain });
  return raw.map(toContractCookie);
}

/**
 * 目标登录态查询（B1.3）：只返回元数据（无 value），供上层展示 / core 判断。
 */
export async function queryTargetState(api: CookiesApi, domain: string): Promise<CookieFacts[]> {
  const cookies = await readDomainCookies(api, domain);
  return cookies.map(factsOf);
}

/**
 * 注入契约 cookie 列表（B1.2）：完整属性写回（含 HttpOnly / SameSite / 过期时间）。
 * 单条失败不中断（跳过并计数），失败原因只含字段名，不含值。
 */
export async function injectCookies(api: CookiesApi, cookies: Cookie[]): Promise<InjectCounts> {
  const counts: InjectCounts = { injected: 0, skipped: 0 };
  for (const cookie of cookies) {
    try {
      const details: BrowserCookieSetDetails = toBrowserCookieSetDetails(cookie);
      await api.set(details);
      counts.injected += 1;
    } catch {
      counts.skipped += 1;
    }
  }
  return counts;
}

async function removeOne(api: CookiesApi, cookie: Cookie): Promise<boolean> {
  try {
    const details = toBrowserCookieSetDetails(cookie);
    const removal: CookieRemovalDetails = { url: details.url, name: details.name };
    if (details.path !== undefined) removal.path = details.path;
    await api.remove(removal);
    return true;
  } catch {
    return false;
  }
}

/** 清除指定 cookie 清单（B1.2 整账号清除：跨域名按清单逐条删）。 */
export async function clearCookies(api: CookiesApi, cookies: Cookie[]): Promise<ClearCounts> {
  const counts: ClearCounts = { removed: 0, failed: 0 };
  for (const cookie of cookies) {
    const removed = await removeOne(api, cookie);
    if (removed) counts.removed += 1;
    else counts.failed += 1;
  }
  return counts;
}

/** 清除单域名下全部 cookie（B1.2 单域名清除）：先读（含 HttpOnly）再逐条删。 */
export async function clearDomainCookies(api: CookiesApi, domain: string): Promise<ClearCounts> {
  const cookies = await readDomainCookies(api, domain);
  return clearCookies(api, cookies);
}
