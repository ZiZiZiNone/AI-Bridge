/**
 * background.ts —— Chrome/Edge MV3 service worker（薄入口）。
 *
 * 职责与边界：
 * - 只做绑定与转发：把浏览器 API（cookies / runtime）绑定给 shared 的桥接调度
 *   （envelope_transport.startBridgeHost），无业务判断、无状态计算——业务判断回 core。
 * - cookie 读取 / 注入调度在 shared（envelope_transport + cookie_io），本文件不写调度逻辑。
 * - 会话原文（cookie 值）只在 shared → desktop/bridge 的契约消息里流动，不经 UI 层、不落日志。
 * - 安装时打开 X3 安装引导页（native messaging 握手引导 + 权限说明）。
 */
import { startBridgeHost } from "../shared/envelope_transport.ts";
import {
  buildClientEnv,
  resolvePlatform,
  BRIDGE_NATIVE_HOST_NAME,
} from "../shared/protocol.ts";

/** client_env 随行（安全模型 §7'）：捕获环境记录 UA/平台，随信封扩散。 */
const clientEnv = buildClientEnv({
  userAgent: navigator.userAgent,
  platform: resolvePlatform(navigator.userAgent),
});

startBridgeHost({
  cookies: chrome.cookies,
  runtime: chrome.runtime,
  nativeHostName: BRIDGE_NATIVE_HOST_NAME,
  dialect: "chromium",
  clientEnv,
});

chrome.runtime.onInstalled.addListener(() => {
  void chrome.tabs.create({ url: "onboarding/onboarding.html" });
});
