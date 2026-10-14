/**
 * onboarding.ts —— X3 安装引导页（薄入口）。
 *
 * 职责与边界：
 * - 只做两件事：发起 native messaging 握手测试（转发给 background 的桥调度）、
 *   展示静态权限说明（文案在 onboarding.html）。无业务判断、无状态计算。
 * - 文案遵守《功能点全景》第 5 节边界速查的禁用词约束；跨设备一律写「设备同步（局域网）」。
 */
import { createActionSender } from "../../shared/envelope_transport.ts";

const sendAction = createActionSender(chrome.runtime);

const button = document.getElementById("handshake-button");
const result = document.getElementById("handshake-result");

if (button === null || result === null) {
  throw new Error("onboarding 元素缺失");
}

button.addEventListener("click", () => {
  result.dataset.state = "unknown";
  result.textContent = "连接中…";
  void sendAction({ kind: "bridge.handshake" })
    .then((response) => {
      if (response.ok) {
        result.dataset.state = "ok";
        result.textContent = `已连接主程序（协议版本 ${response.result.protocol_version}）`;
      } else {
        result.dataset.state = "fail";
        result.textContent = `连接失败：${response.error.message}`;
      }
    })
    .catch((error: unknown) => {
      result.dataset.state = "fail";
      result.textContent = `连接失败：${error instanceof Error ? error.message : "未知错误"}`;
    });
});
