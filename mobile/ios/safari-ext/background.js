// AI-Bridge · Safari Web Extension 后台脚本（M-F2 / E1/E2 的 Safari 侧）。
//
// 职责：cookie 读取 / 注入（含 HttpOnly 与完整属性，A2 保真度零丢弃）；业务判断全在 core。
// 消息形态与 SafariWebExtensionHandler.swift 约定一致（字段名待联调校准，未验证）。
// 边界：不写站点专属逻辑；不做健康分级 / 去重（全在 core）。

const NATIVE_APP = "com.aibridge.mobile.SafariWebExtension"; // 待确认：native 消息目标标识

// 契约 cookie（snake_case，与 contracts/session-envelope.schema.json 逐字段一致）⇄
// WebExtensions cookies API；逐字段全量映射，任何环节零丢弃。
function contractCookie(apiCookie) {
    const cookie = {
        name: apiCookie.name,
        value: apiCookie.value,
        domain: apiCookie.domain,
        path: apiCookie.path,
        secure: apiCookie.secure,
        http_only: apiCookie.httpOnly,
    };
    // sameSite：API no_restriction|lax|strict|unspecified → 契约 none|lax|strict（unspecified = 不携带）
    if (apiCookie.sameSite === "strict") cookie.same_site = "strict";
    else if (apiCookie.sameSite === "lax") cookie.same_site = "lax";
    else if (apiCookie.sameSite === "no_restriction") cookie.same_site = "none";
    if (typeof apiCookie.expirationDate === "number") {
        cookie.expires_at = new Date(apiCookie.expirationDate * 1000).toISOString();
    }
    return cookie;
}

function apiCookieFromContract(cookie) {
    const host = (cookie.domain || "").replace(/^\./, "");
    const api = {
        url: "https://" + host + (cookie.path || "/"),
        name: cookie.name,
        value: cookie.value,
        domain: cookie.domain,
        path: cookie.path,
        secure: !!cookie.secure,
        httpOnly: !!cookie.http_only,
    };
    if (cookie.same_site === "strict") api.sameSite = "strict";
    else if (cookie.same_site === "lax") api.sameSite = "lax";
    else if (cookie.same_site === "none") api.sameSite = "no_restriction";
    if (cookie.expires_at) api.expirationDate = Date.parse(cookie.expires_at) / 1000;
    return api;
}

function sendNative(message) {
    return browser.runtime.sendNativeMessage(NATIVE_APP, message);
}

// 捕获：读指定域全部 cookie（含 HttpOnly），交付原生侧归一化为 SessionEnvelope。
async function captureDomain(domain) {
    const cookies = await browser.cookies.getAll({ domain: domain });
    await sendNative({ type: "capture", domain: domain, cookies: cookies.map(contractCookie) });
}

// 注入：拉取 core 经 SessionSource.inject 暂存的 cookie 并写回 Safari（属性全量）。
async function pullAndInject() {
    const response = await sendNative({ type: "pull-inject" });
    for (const cookie of (response && response.cookies) || []) {
        await browser.cookies.set(apiCookieFromContract(cookie));
    }
}

browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
    if (message && message.type === "capture") {
        captureDomain(message.domain).then(() => sendResponse({ ok: true }));
        return true;
    }
    if (message && message.type === "inject-pending") {
        pullAndInject().then(() => sendResponse({ ok: true }));
        return true;
    }
    return false;
});

// 启动 / 安装时拉取一次待注入会话（推送式注入在 iOS 扩展通道上不可用，采用拉取式）。
if (browser.runtime.onStartup) browser.runtime.onStartup.addListener(pullAndInject);
browser.runtime.onInstalled.addListener(pullAndInject);
