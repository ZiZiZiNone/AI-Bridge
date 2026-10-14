/**
 * envelope_transport.ts —— 与 desktop/bridge 的 native messaging 消息层：收发 + 错误处理 + 调度。
 *
 * 职责与边界：
 * - 消息信封与方法面定义在 `bridge.ts`（唯一实现）；本文件只做端口收发、请求-响应配对、
 *   错误归一、扩展内部动作 → 桥方法的**协议翻译**（架构与契约 §3：扩展「只做协议翻译」），
 *   以及 cookie 读取 / 注入调度（转发给 `cookie_io.ts`）。
 * - **bridge 协议待与 desktop/bridge 联调时定稿**（见 `extensions/shared/README.md`）。
 *   消息只承载契约类型（SessionEnvelope / StandardAccess）与少量控制字段。
 * - 禁止业务规则：scope / 去重 / 健康分级 / TTL / 能否扩散均在 core；本层不判断。
 * - 日志与错误文案不含 cookie 值（明文纪律）；收发内容本身即契约消息，也不落日志。
 * - 宿主 API（cookies / runtime / native port）全部以结构化接口显式传入（模块解耦），
 *   不 import 浏览器全局，可在单测里用假端口直接工作。
 */
import {
  BRIDGE_PROTOCOL_VERSION,
  decodeBridgeMessage,
  encodeBridgeMessage,
} from "./bridge.ts";
import type {
  BridgeDialect,
  BridgeError,
  BridgeMethod,
  BridgeMethodMap,
  BridgeRequest,
  BridgeRequestOf,
  BridgeResponse,
} from "./bridge.ts";
import type { InjectCounts, CookiesApi } from "./cookie_io.ts";
import { injectCookies, readDomainCookies } from "./cookie_io.ts";
import type {
  ExtensionAction,
  ExtensionActionError,
  ExtensionActionErrorCode,
  ExtensionActionResponse,
  ExtensionActionResponseOf,
} from "./protocol.ts";
import { buildSessionEnvelope, isExtensionAction } from "./protocol.ts";
import type { ClientEnv, SessionEnvelope } from "./types.ts";

/* ---------------- 宿主 API 的结构化子集（显式注入） ---------------- */

/** WebExtensions runtime 端口（connectNative 返回值）的结构化子集。 */
export interface PortLike {
  postMessage(message: unknown): void;
  disconnect(): void;
  onMessage: { addListener(listener: (message: unknown) => void): void };
  onDisconnect: { addListener(listener: (port: PortLike) => void): void };
}

/** WebExtensions runtime API 的结构化子集（Promise 形态）。 */
export interface RuntimeLike {
  readonly id: string;
  sendMessage(message: unknown): Promise<unknown>;
  onMessage: {
    addListener(
      listener: (
        message: unknown,
        sender: unknown,
        sendResponse: (response?: unknown) => void,
      ) => boolean | undefined,
    ): void;
  };
  connectNative(name: string): PortLike;
}

/* ---------------- 错误归一 ---------------- */

/** 扩展侧传输 / 调度错误（词表与协议.ts 的内部动作错误码一致）。 */
export class BridgeTransportError extends Error {
  readonly code: ExtensionActionErrorCode;

  constructor(code: ExtensionActionErrorCode, message: string) {
    super(message);
    this.name = "BridgeTransportError";
    this.code = code;
  }
}

function toActionError(error: unknown): ExtensionActionError {
  if (error instanceof BridgeTransportError) {
    return { code: error.code, message: error.message };
  }
  if (error instanceof Error) {
    return { code: "internal", message: error.message };
  }
  return { code: "internal", message: "未知错误" };
}

/* ---------------- 桥客户端（端口收发 + 配对） ---------------- */

interface PendingCall {
  method: BridgeMethod;
  resolve: (value: unknown) => void;
  reject: (error: unknown) => void;
}

/** 宿主 → 扩展请求处理器（如 session.inject）：返回契约结果，失败抛错。 */
export type HostRequestHandler = (request: BridgeRequest) => Promise<unknown>;

/**
 * 桥客户端：一条 native messaging 端口上的请求-响应配对与错误处理。
 * - request() 同端口内 id 唯一；响应按 id 配对，超时/断开由 disconnect 兜底（无定时器）。
 * - 收发都过 bridge.ts 的编解码器（同一份形状规则，杜绝「能编码但解不开」）。
 * - 事件面（event）v1 消费者待联调定稿：解码校验后不派发（不发明消费者）。
 */
export class BridgeClient {
  readonly #port: PortLike;
  readonly #pending = new Map<string, PendingCall>();
  #counter = 0;
  #connected = true;
  #hostRequestHandler: HostRequestHandler | undefined;

  constructor(port: PortLike) {
    this.#port = port;
    port.onMessage.addListener((message) => {
      this.#handleMessage(message);
    });
    port.onDisconnect.addListener(() => {
      this.#handleDisconnect();
    });
  }

  get connected(): boolean {
    return this.#connected;
  }

  /** 注册宿主 → 扩展请求处理器；重复注册以最后一次为准。 */
  onRequest(handler: HostRequestHandler): void {
    this.#hostRequestHandler = handler;
  }

  /** 扩展 → 宿主请求：结果按方法收窄；断开 / 宿主报错以 BridgeTransportError 拒绝。 */
  request<M extends BridgeMethod>(
    method: M,
    params: BridgeMethodMap[M]["params"],
  ): Promise<BridgeMethodMap[M]["result"]> {
    if (!this.#connected) {
      return Promise.reject(new BridgeTransportError("disconnected", "本机桥未连接"));
    }
    const id = `ext-${++this.#counter}`;
    const request: BridgeRequestOf<M> = {
      v: BRIDGE_PROTOCOL_VERSION,
      type: "request",
      id,
      method,
      params,
    };
    // 泛型 M 收窄到联合类型桥消息：断言经 encodeBridgeMessage 内部的解码校验兜底。
    const outbound = JSON.parse(encodeBridgeMessage(request as unknown as BridgeRequest)) as unknown;
    return new Promise<BridgeMethodMap[M]["result"]>((resolve, reject) => {
      this.#pending.set(id, {
        method,
        resolve: resolve as (value: unknown) => void,
        reject,
      });
      try {
        this.#port.postMessage(outbound);
      } catch (error) {
        this.#pending.delete(id);
        reject(new BridgeTransportError("disconnected", `端口发送失败：${String(error)}`));
      }
    });
  }

  #handleMessage(raw: unknown): void {
    let message: ReturnType<typeof decodeBridgeMessage>;
    try {
      message = decodeBridgeMessage(JSON.stringify(raw));
    } catch {
      // 形状不合法的入站消息：不解析、不转发、不记录内容（明文纪律），直接丢弃。
      return;
    }

    if (message.type === "response") {
      this.#settle(message);
      return;
    }
    if (message.type === "request") {
      void this.#dispatchHostRequest(message);
      return;
    }
    // event：v1 事件面消费者待联调定稿，解码校验通过后不派发。
  }

  #settle(response: BridgeResponse): void {
    const pending = this.#pending.get(response.id);
    if (pending === undefined) return; // 无主响应（重复 / 迟到），丢弃。
    this.#pending.delete(response.id);
    if (response.ok) {
      pending.resolve(response.result);
    } else {
      const error: BridgeError = response.error;
      pending.reject(new BridgeTransportError("host_error", `${error.code}：${error.message}`));
    }
  }

  async #dispatchHostRequest(request: BridgeRequest): Promise<void> {
    let response: BridgeResponse;
    const handler = this.#hostRequestHandler;
    try {
      if (handler === undefined) {
        throw new Error(`未注册宿主请求处理器：${request.method}`);
      }
      const result = await handler(request);
      response = {
        v: BRIDGE_PROTOCOL_VERSION,
        type: "response",
        id: request.id,
        ok: true,
        method: request.method,
        result,
      } as BridgeResponse;
    } catch (error) {
      const detail: BridgeError = {
        code: "internal",
        message: error instanceof Error ? error.message : "未知错误",
      };
      response = {
        v: BRIDGE_PROTOCOL_VERSION,
        type: "response",
        id: request.id,
        ok: false,
        error: detail,
      };
    }
    try {
      this.#port.postMessage(JSON.parse(encodeBridgeMessage(response)) as unknown);
    } catch {
      // 端口已断：响应无法回送，交给 disconnect 流程处理。
    }
  }

  #handleDisconnect(): void {
    this.#connected = false;
    for (const pending of this.#pending.values()) {
      pending.reject(new BridgeTransportError("disconnected", "本机桥未连接"));
    }
    this.#pending.clear();
  }
}

/* ---------------- 扩展内部动作的发送端（popup / content 用） ---------------- */

/**
 * 构造内部动作发送函数：popup / content 只组装动作对象并转发（薄入口）。
 * 响应形状做最小结构检查，返回值按动作种类收窄。
 */
export function createActionSender(runtime: RuntimeLike) {
  return async function sendAction<A extends ExtensionAction>(
    action: A,
  ): Promise<ExtensionActionResponseOf<A["kind"]>> {
    const raw = await runtime.sendMessage(action);
    if (
      typeof raw !== "object" ||
      raw === null ||
      typeof (raw as { ok?: unknown }).ok !== "boolean"
    ) {
      throw new BridgeTransportError("protocol_error", "内部动作响应形状不合法");
    }
    return raw as ExtensionActionResponseOf<A["kind"]>;
  };
}

/* ---------------- background 调度（协议翻译 + cookie 读写调度） ---------------- */

/** startBridgeHost 依赖面：全部显式注入（浏览器 API 由 background 薄入口绑定）。 */
export interface BridgeHostDeps {
  cookies: CookiesApi;
  runtime: RuntimeLike;
  /** native messaging 宿主名（见 protocol.ts，取值待与 desktop/bridge 对齐——待确认）。 */
  nativeHostName: string;
  dialect: BridgeDialect;
  /** 捕获随行的 client_env（UA/平台，安全模型 §7'）。 */
  clientEnv: ClientEnv;
}

function errorResponse(error: unknown): ExtensionActionResponse {
  return { ok: false, error: toActionError(error) };
}

/**
 * 启动 background 桥接调度：内部动作 → 桥方法翻译 + cookie 读取 / 注入调度。
 * 无业务判断：动作与桥方法一一对应，会话内容只透传（cookie 原文不经过 UI 层）。
 */
export function startBridgeHost(deps: BridgeHostDeps): void {
  let client: BridgeClient | undefined;

  function ensureClient(): BridgeClient {
    const existing = client;
    if (existing !== undefined && existing.connected) return existing;
    const next = new BridgeClient(deps.runtime.connectNative(deps.nativeHostName));
    next.onRequest(handleHostRequest);
    client = next;
    return next;
  }

  function callHost<M extends BridgeMethod>(
    method: M,
    params: BridgeMethodMap[M]["params"],
  ): Promise<BridgeMethodMap[M]["result"]> {
    return ensureClient().request(method, params);
  }

  /** 宿主 → 扩展请求：v1 只定义 session.inject（B2 目标侧 / E1 注入）。 */
  async function handleHostRequest(request: BridgeRequest): Promise<unknown> {
    if (request.method !== "session.inject") {
      throw new Error(`未定义的宿主请求方法：${request.method}`);
    }
    const envelope: SessionEnvelope = request.params.envelope;
    const counts: InjectCounts = await injectCookies(
      deps.cookies,
      envelope.payload.cookies ?? [],
    );
    return counts;
  }

  async function dispatchAction(message: unknown): Promise<ExtensionActionResponse> {
    if (!isExtensionAction(message)) {
      const error: ExtensionActionError = {
        code: "invalid_action",
        message: "无法识别的内部动作",
      };
      return { ok: false, error };
    }
    switch (message.kind) {
      case "session.query": {
        const result = await callHost("session.query", { target: message.target });
        return { ok: true, result };
      }
      case "session.apply": {
        const fetched = await callHost("session.fetch", {
          account_ref: message.account_ref,
          target: message.target,
        });
        const counts = await injectCookies(
          deps.cookies,
          fetched.envelope.payload.cookies ?? [],
        );
        return { ok: true, result: counts };
      }
      case "session.capture.push": {
        const domain = message.target.domain;
        if (domain === undefined || domain.length === 0) {
          throw new BridgeTransportError("invalid_action", "推送目标缺少 domain");
        }
        const cookies = await readDomainCookies(deps.cookies, domain);
        const envelope = buildSessionEnvelope({
          accountRef: message.account_ref,
          target: message.target,
          cookies,
          clientEnv: deps.clientEnv,
          sourceId: deps.dialect,
        });
        const result = await callHost("session.capture", {
          envelope,
          destination: message.destination,
        });
        return { ok: true, result };
      }
      case "push.targets": {
        const result = await callHost("push.targets", {});
        return { ok: true, result };
      }
      case "credential.fill": {
        const params: BridgeMethodMap["credential.fill"]["params"] = { target: message.target };
        if (message.account_ref !== undefined) params.account_ref = message.account_ref;
        const result = await callHost("credential.fill", params);
        return { ok: true, result };
      }
      case "bridge.handshake": {
        // P1-9 注：extension_id 为自报值（运行时环境 ID，非密码学身份），安全语义
        // 待与 desktop/bridge 的 allowed_origins/allowed_extensions 联调定稿。
        const result = await callHost("handshake", {
          protocol_version: BRIDGE_PROTOCOL_VERSION,
          dialect: deps.dialect,
          extension_id: deps.runtime.id,
        });
        return { ok: true, result };
      }
      case "bridge.ping": {
        try {
          await callHost("ping", {});
          return { ok: true, result: { connected: true } };
        } catch {
          return { ok: true, result: { connected: false } };
        }
      }
      case "content.fill": {
        // content.fill 走 tabs 通道（popup → content），不应到达 background。
        throw new BridgeTransportError("invalid_action", "content.fill 不经 background 转发");
      }
    }
  }

  deps.runtime.onMessage.addListener((message, _sender, sendResponse) => {
    void dispatchAction(message).then(
      (response) => {
        sendResponse(response);
      },
      (error: unknown) => {
        sendResponse(errorResponse(error));
      },
    );
    return true;
  });
}
