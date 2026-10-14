/**
 * background.ts —— Firefox MV3 background（薄入口，适配层）。
 *
 * 职责与边界：与 chromium/background.ts 相同（绑定 + 转发，无业务判断），差异只在：
 * 1. 命名空间用 Firefox 原生的 `browser`（Promise 形态）而非 `chrome`；
 * 2. 方言标识为 firefox（source.id / handshake.dialect）；
 * 3. manifest 形态差异（event page scripts、browser_specific_settings）见 manifest.json。
 * 复用：桥接调度（envelope_transport）、cookie 读写（cookie_io）、协议装配（protocol）全部
 * 走 extensions/shared/ 同一份实现；popup / content / onboarding 源码复用 chromium/（打包期汇入）。
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
  cookies: browser.cookies,
  runtime: browser.runtime,
  nativeHostName: BRIDGE_NATIVE_HOST_NAME,
  dialect: "firefox",
  clientEnv,
});

browser.runtime.onInstalled.addListener(() => {
  void browser.tabs.create({ url: "onboarding/onboarding.html" });
});
