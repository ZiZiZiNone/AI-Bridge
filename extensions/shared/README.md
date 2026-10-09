# extensions/shared —— 扩展共享层（TypeScript，宿主无关）

浏览器扩展侧与桌面桥之间共享的一份实现：**契约类型镜像 + cookie 映射 + SessionEnvelope 编解码 +
native messaging 桥协议 v1**。零 UI、零宿主依赖，同一份代码可在扩展、单测、任意宿主里直接工作。

## 1. 依赖规则（单向依赖）

- 只依赖 `contracts/` 的形状（本层以 TS 类型镜像），**不 import** `core/`、`desktop/`、`extensions/chromium|firefox/` 的任何代码。
- 本层 **不 import 浏览器 API 与 Node API**：浏览器 cookie 以结构化子集 `BrowserCookie` 由调用方传入。
  该约束由编译配置锁死（`lib: ["ES2023"]` 无 DOM + `types: []` 无 Node 全局），误用全局即编译失败。
- 业务规则（TTL 策略、scope 授权、能否扩散、注入决策）属于 core（`ab-broker` / `ab-mcp` / `ab-state`），
  本层只负责数据形状与编解码。
- 编译配置把这些约束固化（`tsconfig.json`，刻意保持纯 JSON，便于其他工具扫描）：

  | 选项 | 作用 |
  |---|---|
  | `lib: ["ES2023"]` | 无 DOM：用浏览器全局即编译失败 |
  | `types: []` | 无 Node 全局：源码里用 `process` / `Buffer` 即编译失败 |
  | `exactOptionalPropertyTypes` | 缺省字段"不存在"，而不是存在且为 `undefined`（对齐 Rust `skip_serializing_if`） |
  | `erasableSyntaxOnly` | 禁 `enum` / `namespace` / 参数属性，保证 Node 22 能直跑 `.ts` |
  | `verbatimModuleSyntax` + `isolatedModules` | 类型导入必须 `import type`，无运行时残留导入 |
  | `noUncheckedIndexedAccess` / `strict` | 索引与空值必须显式处理（cookie 属性不得被 `undefined` 蒙混过去） |

## 2. 文件职责

| 文件 | 职责 |
|---|---|
| `types.ts` | `contracts/` 三个 schema 的 TS 镜像：枚举值域、`*_KEYS` / `*_REQUIRED_KEYS` 字段清单、接口、pattern |
| `cookie.ts` | 浏览器 cookie ↔ 契约 `Cookie` 双向映射（含 `cookies.set` 写入参数）；http_only / secure / same_site / expires_at 一个不丢 |
| `envelope.ts` | `SessionEnvelope` 编解码 + 结构校验镜像（schema_version、target 必填、kind/payload 一致性） |
| `bridge.ts` | native messaging 桥协议 v1：request / response / event 三类消息、方法面、事件面、错误码 |
| `test/*.test.ts` | `node --test` 单测（含与 `contracts/*.schema.json` 逐字段比对） |
| `tsconfig.json` | 库源码编译配置（只覆盖源码） |
| `tsconfig.test.json` | 含测试的编译配置（需要 `@types/node`） |

## 3. 与 Rust 侧（`core/contract/src/lib.rs` · ab-contract）的关系

- `types.ts` 与 `contracts/*.schema.json` 逐字段一致（字段名 snake_case），与 `ab-contract` 的结构体一一对应。
- `envelope.ts` 的校验规则镜像 `check_schema_version` / `check_target` / `check_payload`，
  错误码与 `ContractError` 变体同名：`unsupported_schema_version`、`target_field_missing`、
  `payload_mismatch`、`missing_expiry` 等。
- **有意的差异（3 条，均有测试锁定）**：
  1. 本层拒绝额外字段（schema 是 `additionalProperties: false`）；Rust serde 默认忽略未知键。
     宁可报错也不静默丢字段——cookie 属性丢失是会话保真度的直接事故。
  2. 不校验 date-time 的 `format`（与 `ab-contract` 一致：format 是断言而非结构）。`cookie.ts` 例外，
     因为金标方向要做时间换算，必须能解析。
  3. `encode` 输出字段顺序固定为 schema 声明顺序，且**不裁剪**空数组（Rust 的 `skip_serializing_if`
     会省略空 `cookies` / `local_storage`）。两者互相可解析，语义一致，仅字节不同。
- 版本化：`schema_version` 必填、新增字段一律可选。新增字段时必须同步 `types.ts` 的 `*_KEYS`
  常量——否则 `test/types.test.ts` 立刻失败，且 `*KeysComplete` 断言会让 `tsc` 报错。

## 4. 用法

读 cookie 上报（浏览器 → 契约 → 信封 → 桥）：

```ts
import { toContractCookie } from "./cookie.ts";
import { encodeEnvelope } from "./envelope.ts";
import { encodeBridgeMessage } from "./bridge.ts";

const raw = await browser.cookies.get({ url, name: "sid" }); // 浏览器 API，由调用方持有
const request = {
  v: 1,
  type: "request",
  id: crypto.randomUUID(),
  method: "session.capture",
  params: {
    envelope: {
      schema_version: "1.0",
      account_ref: accountRef,
      target: { kind: "web", domain: "github.com" },
      kind: "cookie",
      payload: { cookies: [toContractCookie(raw)] },
      issued_at: new Date().toISOString(),
      source: { type: "extension", id: "chromium" },
    },
  },
} as const;

const wire = encodeBridgeMessage(request); // 已按协议 v1 校验过的线上文本
port.postMessage(JSON.parse(wire));        // 端口自己会序列化：传对象，别把字符串再编码一层
```

注入（信封 → cookie 写回）：

```ts
import { decodeEnvelope } from "./envelope.ts";
import { toBrowserCookieSetDetails } from "./cookie.ts";

const envelope = decodeEnvelope(text); // schema_version / target / kind+payload 任一不合规即抛 EnvelopeError
for (const cookie of envelope.payload.cookies ?? []) {
  await browser.cookies.set(toBrowserCookieSetDetails(cookie));
}
```

收到桥消息（判别收窄）：

```ts
import { decodeBridgeMessage } from "./bridge.ts";

const message = decodeBridgeMessage(rawText);
if (message.type === "request" && message.method === "session.inject") {
  // message.params.envelope 已被类型收窄为 SessionEnvelope
}
```

## 5. 运行（本机实测命令）

```powershell
# 单测（36 项）
node --test --experimental-test-isolation=none extensions/shared/test/types.test.ts `
  extensions/shared/test/cookie.test.ts extensions/shared/test/envelope.test.ts extensions/shared/test/bridge.test.ts
# 等价：npm test --prefix extensions/shared

# 类型检查
npx tsc -p extensions/shared/tsconfig.json        # 库源码（无 Node/DOM 全局）
npx tsc -p extensions/shared/tsconfig.test.json   # 含测试（需要 @types/node）
```

`--experimental-test-isolation=none` 让测试在同一进程内执行；受限沙盒禁止子进程管道时，
`node --test` 默认"每个文件起一个子进程"会 `spawn EPERM`，需带该开关。普通开发环境直接
`node --test` 即可。Node 22 原生擦除类型直跑 `.ts`，因此源码 import 一律写显式 `.ts` 后缀。

## 6. 已知边界

- 浏览器专属字段（`storeId` / `hostOnly` / `partitionKey` / `firstPartyDomain`）不在契约内
  （`additionalProperties: false` 无法承载），注入端由目标浏览器自行推导。
- `sameSite` 映射用 WebExtensions 通用取值（`none` ↔ `no_restriction`）；Chromium 与 Firefox
  都接受 `no_restriction`，故不做方言分支。
- native messaging 的 4 字节小端长度前缀/分帧不在本层，属 `desktop/bridge/messaging_host`；
  扩展侧由浏览器 `runtime.connectNative` 端口负责。
- `params` / `result` 的内部形状由各方法处理器校验；本层只保证消息外壳与所引用的契约类型可解码。
- 本波只镜像了 `SessionEnvelope` 的校验（`ab-contract` 的 `SessionEnvelope::validate` /
  `validate_temporary` / `SessionRecord::validate`）。`StandardAccess::validate` 与
  `ConverterManifest::validate` 未实现：类型与 `*_KEYS` 字段清单已就绪，下一波可直接复用
  `validateSessionRecord`（standard-access 的 `sessions[]` 与信封共用同一份规则）。
- 本波不含 `extensions/chromium/` 与 `extensions/firefox/` 的 manifest / popup / content / background。
