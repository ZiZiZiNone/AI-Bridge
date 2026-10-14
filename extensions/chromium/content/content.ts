/**
 * content.ts —— X2 表单浮标 + 表单填充（B3.1 b 级退路）。
 *
 * 职责与边界：
 * - 只做 DOM 取数与注入：检测登录表单、渲染浮标、把结构化凭据填入字段。
 * - 禁止业务判断：填哪个账号由用户选择（account_ref 透传，未选时由 core 匹配）；
 *   凭据内容一律来自 core（经 background 的桥调度），本层不生成、不暂存、不打日志。
 * - OTP 填充未实现（待确认）：TOTP 计算属 core（ab-vault），契约未定义「当前验证码」载体，
 *   b 级退路暂只填用户名 / 密码；快捷键键位分配（Ctrl+1/2）亦待确认。
 * - 表单识别启发式（无事实源，最小实现——待确认）：找可见的密码输入框；
 *   用户名 = 同表单内位于密码框之前的文本/邮箱输入框。
 */
import { createActionSender } from "../../shared/envelope_transport.ts";
import { webTarget } from "../../shared/protocol.ts";
import type { CredentialRecord } from "../../shared/types.ts";

const sendAction = createActionSender(chrome.runtime);

const WIDGET_ID = "aibridge-form-widget";

interface LoginForm {
  username: HTMLInputElement | undefined;
  password: HTMLInputElement;
}

function isVisible(input: HTMLInputElement): boolean {
  return input.type !== "hidden" && input.offsetParent !== null;
}

function findLoginForm(): LoginForm | undefined {
  const password = Array.from(
    document.querySelectorAll<HTMLInputElement>('input[type="password"]'),
  ).filter(isVisible)[0];
  if (password === undefined) return undefined;

  const scope: ParentNode = password.form ?? document;
  const username = Array.from(
    scope.querySelectorAll<HTMLInputElement>('input[type="text"], input[type="email"]'),
  )
    .filter(isVisible)
    .find((input) => isBefore(input, password));

  return { username, password };
}

function isBefore(a: HTMLElement, b: HTMLElement): boolean {
  if (a === b) return false;
  return (a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
}

function fillInput(input: HTMLInputElement, value: string): boolean {
  if (value.length === 0) return false;
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
  return true;
}

/** 取凭据（动作瞬间传递、用完即弃）：account_ref 由调用方透传，未提供时由 core 匹配。 */
async function fetchCredential(
  accountRef: string | undefined,
): Promise<CredentialRecord | undefined> {
  const target = webTarget(location.hostname);
  const response =
    accountRef === undefined
      ? await sendAction({ kind: "credential.fill", target })
      : await sendAction({ kind: "credential.fill", target, account_ref: accountRef });
  if (!response.ok) return undefined;
  return response.result.credentials[0];
}

/** B3.1 填表：用户名 + 密码（OTP 待确认，见文件头）。返回是否至少填入一项。 */
async function fillForm(accountRef: string | undefined): Promise<boolean> {
  const form = findLoginForm();
  if (form === undefined) return false;
  const record = await fetchCredential(accountRef);
  if (record === undefined) return false;
  let filled = false;
  if (form.username !== undefined) {
    filled = fillInput(form.username, record.username ?? "") || filled;
  }
  filled = fillInput(form.password, record.secret ?? "") || filled;
  return filled;
}

function renderWidget(): void {
  if (document.getElementById(WIDGET_ID) !== null) return;
  const widget = document.createElement("button");
  widget.id = WIDGET_ID;
  widget.type = "button";
  widget.textContent = "AI-Bridge 填充";
  widget.style.cssText =
    "position:fixed;z-index:2147483647;right:16px;bottom:16px;padding:6px 10px;font-size:12px;";
  widget.addEventListener("click", () => {
    void fillForm(undefined);
  });
  document.body.append(widget);
}

/** popup → content：按选中账号填充（X1「填充」按钮）。 */
chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (
    typeof message !== "object" ||
    message === null ||
    (message as { kind?: unknown }).kind !== "content.fill"
  ) {
    return undefined;
  }
  const accountRef = (message as { account_ref?: unknown }).account_ref;
  void fillForm(typeof accountRef === "string" ? accountRef : undefined).then((filled) => {
    sendResponse({ ok: true, result: { filled } });
  });
  return true;
});

/** 快捷键（B-F3）：Ctrl+1 填用户名、Ctrl+2 填密码（键位分配待确认）。 */
document.addEventListener("keydown", (event) => {
  if (!event.ctrlKey || event.altKey || event.metaKey) return;
  const form = findLoginForm();
  if (form === undefined) return;
  if (event.key === "1") {
    event.preventDefault();
    void fetchCredential(undefined).then((record) => {
      if (record !== undefined && form.username !== undefined) {
        fillInput(form.username, record.username ?? "");
      }
    });
  } else if (event.key === "2") {
    event.preventDefault();
    void fetchCredential(undefined).then((record) => {
      if (record !== undefined) fillInput(form.password, record.secret ?? "");
    });
  }
});

if (findLoginForm() !== undefined) renderWidget();
