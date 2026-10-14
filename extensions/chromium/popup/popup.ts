/**
 * popup.ts —— X1 扩展弹窗（薄入口）。
 *
 * 职责与边界：
 * - 只做组装与转发：取当前域名 → 组装动作（protocol.ts 的 ExtensionAction）→ 转发给
 *   background / content → 渲染结构化结果。禁止业务判断、数据转换、状态计算：
 *   筛选 / 去重 / 健康分级 / 账号匹配都属 core，本层给什么显示什么。
 * - 敏感数据（凭据 secret）只在动作瞬间到达并立即使用，不落存储、不打日志。
 * - 文案遵守《功能点全景》第 5 节边界速查的禁用词约束；跨设备一律写「设备同步（局域网）」。
 *
 * 量化说明（FE-101 超标处理）：本文件直接定义的渲染/转发函数约 6 个，超过「一般页面 ≤3 个」
 * 的量化线；保留原因——X1 单文件承载 4 个独立区块（状态 / 列表 / 推送 / 消息）且无组件层，
 * 每个函数职责单一（渲染一个区块或转发一个动作），拆成组件反而增加文件间传参复杂度。
 */
import type { PushTarget } from "../../shared/bridge.ts";
import { createActionSender } from "../../shared/envelope_transport.ts";
import { domainFromUrl, webTarget } from "../../shared/protocol.ts";
import type { CredentialRecord, StandardAccess, Target } from "../../shared/types.ts";

const sendAction = createActionSender(chrome.runtime);

const statusEl = document.getElementById("bridge-status");
const domainEl = document.getElementById("domain-label");
const listEl = document.getElementById("account-list");
const emptyEl = document.getElementById("accounts-empty");
const messageEl = document.getElementById("message-box");
const targetsEl = document.getElementById("push-targets");
const accountEl = document.getElementById("push-account");
const pushButton = document.getElementById("push-button");

function requiredElement(element: HTMLElement | null): HTMLElement {
  if (element === null) throw new Error("popup 元素缺失");
  return element;
}

const statusNode = requiredElement(statusEl);
const domainNode = requiredElement(domainEl);
const listNode = requiredElement(listEl);
const emptyNode = requiredElement(emptyEl);
const messageNode = requiredElement(messageEl);
const targetsNode = requiredElement(targetsEl) as HTMLSelectElement;
const accountNode = requiredElement(accountEl) as HTMLSelectElement;
const pushNode = requiredElement(pushButton) as HTMLButtonElement;

/** 当前页目标（由浏览器取数机械装配，不含业务判断）。 */
let currentTarget: Target | undefined;
let activeTabId: number | undefined;

function showMessage(text: string): void {
  messageNode.textContent = text;
  messageNode.hidden = text.length === 0;
}

function setBridgeStatus(connected: boolean): void {
  statusNode.dataset.state = connected ? "ok" : "fail";
  statusNode.textContent = connected ? "本机桥已连接" : "本机桥未连接（请启动 AI-Bridge 主程序）";
}

function renderAccounts(access: StandardAccess): void {
  listNode.replaceChildren();
  accountNode.replaceChildren();
  const placeholder = document.createElement("option");
  placeholder.value = "";
  placeholder.textContent = "（请选择）";
  accountNode.append(placeholder);

  const rows = access.credentials ?? [];
  emptyNode.hidden = rows.length > 0;
  for (const record of rows) {
    const name = record.username ?? record.account_ref;

    const row = document.createElement("li");
    const label = document.createElement("span");
    label.className = "account-name";
    label.textContent = name;
    row.append(label);
    for (const spec of [
      { action: "fill", text: "填充" },
      { action: "apply", text: "注入" },
      { action: "copy", text: "复制" },
    ]) {
      const button = document.createElement("button");
      button.type = "button";
      button.dataset.action = spec.action;
      button.dataset.accountRef = record.account_ref;
      button.textContent = spec.text;
      row.append(button);
    }
    listNode.append(row);

    const option = document.createElement("option");
    option.value = record.account_ref;
    option.textContent = name;
    accountNode.append(option);
  }
}

function renderTargets(targets: PushTarget[]): void {
  targetsNode.replaceChildren();
  const placeholder = document.createElement("option");
  placeholder.value = "";
  placeholder.textContent = "（请选择）";
  targetsNode.append(placeholder);
  for (const target of targets) {
    const option = document.createElement("option");
    option.value = target.id;
    option.textContent = target.label;
    targetsNode.append(option);
  }
}

/** 行按钮动作：填充（→ content）/ 注入 / 复制。结果只展示数量或成败，不展示值。 */
async function runRowAction(button: HTMLButtonElement): Promise<void> {
  const accountRef = button.dataset.accountRef ?? "";
  const target = currentTarget;
  if (target === undefined) return;
  showMessage("");

  if (button.dataset.action === "fill") {
    if (activeTabId === undefined) return;
    await chrome.tabs.sendMessage(activeTabId, {
      kind: "content.fill",
      account_ref: accountRef,
    });
    return;
  }

  if (button.dataset.action === "apply") {
    const response = await sendAction({ kind: "session.apply", account_ref: accountRef, target });
    if (response.ok) {
      showMessage(`注入完成：成功 ${response.result.injected} 条，跳过 ${response.result.skipped} 条`);
    } else {
      showMessage(`注入失败：${response.error.message}`);
    }
    return;
  }

  const response = await sendAction({ kind: "credential.fill", target, account_ref: accountRef });
  if (!response.ok) {
    showMessage(`取用失败：${response.error.message}`);
    return;
  }
  const records: CredentialRecord[] = response.result.credentials;
  const record = records.find((item) => item.account_ref === accountRef) ?? records[0];
  const value = record?.secret ?? record?.username ?? "";
  if (value.length === 0) {
    showMessage("无可复制内容");
    return;
  }
  await navigator.clipboard.writeText(value);
  showMessage("已复制（请尽快使用并清空剪贴板）");
}

async function runPush(): Promise<void> {
  const destination = targetsNode.value;
  const accountRef = accountNode.value;
  const target = currentTarget;
  showMessage("");
  if (target === undefined || destination.length === 0 || accountRef.length === 0) {
    showMessage("请选择目标设备与账号");
    return;
  }
  const response = await sendAction({
    kind: "session.capture.push",
    account_ref: accountRef,
    target,
    destination,
  });
  if (response.ok) {
    showMessage(response.result.delivered === true ? "已推送到目标设备" : "已上报主程序");
  } else {
    showMessage(`推送失败：${response.error.message}`);
  }
}

async function init(): Promise<void> {
  const tabs = await chrome.tabs.query({ active: true, currentWindow: true });
  const tab = tabs[0];
  activeTabId = tab?.id;
  const domain = tab?.url === undefined ? "" : domainFromUrl(tab.url);
  domainNode.textContent = domain.length > 0 ? domain : "（当前页面无域名）";
  if (domain.length > 0) currentTarget = webTarget(domain);

  const ping = await sendAction({ kind: "bridge.ping" });
  setBridgeStatus(ping.ok && ping.result.connected);

  if (currentTarget !== undefined) {
    const response = await sendAction({ kind: "session.query", target: currentTarget });
    if (response.ok) renderAccounts(response.result.access);
    else showMessage(`候选获取失败：${response.error.message}`);
  }

  const targets = await sendAction({ kind: "push.targets" });
  if (targets.ok) renderTargets(targets.result.targets);
}

listNode.addEventListener("click", (event) => {
  const target = event.target;
  if (!(target instanceof HTMLElement)) return;
  const button = target.closest("button[data-action]");
  if (!(button instanceof HTMLButtonElement)) return;
  void runRowAction(button).catch((error: unknown) => {
    showMessage(error instanceof Error ? error.message : "操作失败");
  });
});

pushNode.addEventListener("click", () => {
  void runPush().catch((error: unknown) => {
    showMessage(error instanceof Error ? error.message : "推送失败");
  });
});

void init().catch((error: unknown) => {
  setBridgeStatus(false);
  showMessage(error instanceof Error ? error.message : "初始化失败");
});
