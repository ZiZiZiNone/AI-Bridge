/**
 * webext.d.ts —— WebExtensions API 最小环境声明（MV3 Promise 形态）。
 *
 * 职责与边界：
 * - 只声明本层实际用到的成员，形状与 shared 的结构化接口（CookiesApi / RuntimeLike / PortLike）
 *   对齐；不引入 @types/chrome 等依赖（无构建链依赖，tsc 可编译为准）。
 * - 真实浏览器运行时行为（如 chrome.cookies 的 Promise 形态）未经真机验证——未验证项，
 *   见 extensions/README.md。
 * - 全局脚本声明（无顶层 import/export）：`chrome` 与 `browser` 同形，Chromium 与 Firefox 共用；
 *   两者的宿主差异只在 background 薄入口绑定处体现。
 */

type ExtCookiesApi = import("./shared/cookie_io.ts").CookiesApi;
type ExtRuntimeLike = import("./shared/envelope_transport.ts").RuntimeLike;

/** 扩展可见的标签页信息子集。 */
interface ExtTab {
  id?: number;
  url?: string;
  title?: string;
}

/** tabs API 子集：popup 取当前页 / 通知 content 填表 / onboarding 引导页。 */
interface ExtTabsApi {
  query(queryInfo: { active: boolean; currentWindow: boolean }): Promise<ExtTab[]>;
  sendMessage(tabId: number, message: unknown): Promise<unknown>;
  create(createProperties: { url: string }): Promise<ExtTab>;
}

interface ExtRuntimeApi extends ExtRuntimeLike {
  onInstalled: {
    addListener(listener: (details: { reason: string }) => void): void;
  };
}

interface ExtExtensionApi {
  cookies: ExtCookiesApi;
  runtime: ExtRuntimeApi;
  tabs: ExtTabsApi;
}

declare const chrome: ExtExtensionApi;
declare const browser: ExtExtensionApi;
