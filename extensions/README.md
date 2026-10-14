# extensions/ —— 浏览器扩展层（Chrome / Edge / Firefox，MV3 + TypeScript）

> 时点：2026-10-12（实现快照）。事实源：`docs/功能点全景.md` §3.2（X1/X2/X3）与 §2.2 B（B1–B3）、
> `docs/架构与契约.md` §3/§5、`docs/流程/浏览器端流程.md`、`docs/安全模型.md` §7'、
> `contracts/*.schema.json`、`core/contract/src/lib.rs`。
> 本目录只做**浏览器取数 / 注入 / 协议翻译**；一切业务判断回 core。

## 1. 结构

```
extensions/
├── tsconfig.json          全层类型检查（strict；npx tsc --noEmit -p extensions/tsconfig.json）
├── package.json           仅声明 ESM 模块形态（无依赖）
├── webext.d.ts            WebExtensions API 最小环境声明（不引入 @types/chrome）
├── shared/                共享层（宿主无关，见 shared/README.md）
│   ├── types.ts           contracts/ 三 schema 的 TS 镜像（既有）
│   ├── cookie.ts          浏览器 cookie ↔ 契约 Cookie 双向映射（既有，完整属性零丢弃）
│   ├── cookie_io.ts       cookies API 读写封装：读取（含 HttpOnly）/ 注入 / 清除（单域名、整账号）/ 目标登录态查询
│   ├── envelope.ts        SessionEnvelope 编解码 + 结构校验（既有）
│   ├── bridge.ts          桥协议 v1 消息定义（待与 desktop/bridge 联调时定稿）
│   ├── envelope_transport.ts  桥收发 + 错误处理 + 协议翻译调度（BridgeClient / startBridgeHost / createActionSender）
│   ├── protocol.ts        协议装配：client_env 随行、Target/信封装配、扩展内部动作消息
│   └── test/              node --test 单测（38 项）
├── chromium/              Chrome / Edge MV3
│   ├── manifest.json      MV3 最小权限（逐项用途见 manifest.notes.md）
│   ├── manifest.notes.md  权限/键位用途注释（manifest.json 须严格 JSON，注释移至旁注文件）
│   ├── background.ts      service worker 薄入口：绑定 shared 桥接调度 + 安装时打开引导页
│   ├── popup/             X1：当前站点账号（含空状态）、填充/注入/复制、B2 推送（含目标设备选择）、桥状态
│   ├── content/           X2：表单浮标 + 表单填充（B3.1 b 级退路）
│   └── onboarding/        X3：native messaging 握手引导 + 权限说明
└── firefox/               Firefox MV3 适配层
    ├── manifest.json      差异：background.scripts（event page）、browser_specific_settings.gecko.id
    ├── background.ts      差异：browser.* 命名空间 + firefox 方言；其余复用 shared/
    └── README.md          差异点说明与打包复用关系
```

**职责分界（与 core/）**：扩展不持密钥、不做业务判断——scope 校验、去重、健康分级、TTL、
能否扩散、账号匹配一律在 core（`ab-broker` / `ab-state` / `ab-vault` / `ab-mcp`）；
UI 层（popup/content/onboarding）是薄入口：只组装动作、转发、渲染结构化结果。
cookie 原文只在 shared ↔ desktop/bridge 的契约消息里流动，不经 UI 层、不写日志（明文纪律）。

## 2. bridge 协议现状（最小设计）

**bridge 协议待与 desktop/bridge 联调时定稿。** 消息只承载契约类型
（`SessionEnvelope` / `StandardAccess`，及其构件 `Cookie` / `Target` / `CredentialRecord`）
与少量控制字段；禁止自造业务字段。定义与编解码在 `shared/bridge.ts`，收发在
`shared/envelope_transport.ts`。

消息三类：`request`（任一侧发起）/ `response`（按 `id` 配对）/ `event`（v1 消费者待定）。

| 方法 | 方向 | 参数（控制字段） | 结果 |
|---|---|---|---|
| `handshake` | 扩展→宿主 | protocol_version / dialect / extension_id | protocol_version / host_version |
| `session.query` | 扩展→宿主 | `target: Target` | `{ access: StandardAccess }`（候选形状待确认） |
| `session.fetch` | 扩展→宿主 | account_ref / `target: Target` | `{ envelope: SessionEnvelope }`（动作瞬间传递，用完即弃） |
| `session.capture` | 扩展→宿主 | `{ envelope, destination? }`（destination=B2 推送目标） | `{ stored, delivered? }` |
| `session.inject` | 宿主→扩展 | `{ envelope }` | `{ injected, skipped }` |
| `push.targets` | 扩展→宿主 | — | `{ targets: [{id,label}] }`（形状待确认） |
| `credential.fill` | 扩展→宿主 | `target` / account_ref? | `{ credentials: CredentialRecord[] }` |
| `ping` | 扩展→宿主 | — | `{ ok: true }` |

另有**扩展内部动作**（popup/content/background 之间，不跨 desktop 桥）：
`session.query` / `session.apply`（fetch+注入）/ `session.capture.push`（读 cookie→capture）/
`push.targets` / `credential.fill` / `bridge.handshake` / `bridge.ping` / `content.fill`，
定义在 `shared/protocol.ts`，background 做动作 → 桥方法的协议翻译。

`client_env`（UA/平台）随行（安全模型 §7'）：捕获时由 `protocol.buildClientEnv` 装配
（UA 原文 + UA→平台词表机械映射），随 `SessionEnvelope` / `StandardAccess` 扩散；
注入容器的对齐逻辑与风控提示在 core / 宿主层。

## 3. 权限（最小权限原则）

只申请 `cookies`、`nativeMessaging`、`host_permissions: <all_urls>` 三项，逐项用途见
[`chromium/manifest.notes.md`](chromium/manifest.notes.md)。文案口径：跨设备一律写
「设备同步（局域网）」。

## 4. chromium 与 firefox 差异点

| 差异点 | chromium | firefox | 复用情况 |
|---|---|---|---|
| background 入口 | `service_worker` + `type: module` | `scripts`（event page）+ `type: module`（Firefox 对 background 模块脚本的支持未验证——未验证项） | 逻辑全部在 shared（`startBridgeHost`） |
| API 命名空间 | `chrome.*` | `browser.*` | 共用 `webext.d.ts` 同形声明 |
| 方言标识 | `dialect/source.id = "chromium"` | `"firefox"` | 仅常量不同 |
| 扩展 id | 商店分配 | `browser_specific_settings.gecko.id`（native messaging 需要；取值待与 desktop/bridge 的 `allowed_extensions` 对齐——待确认） | — |
| 宿主注册（desktop 侧） | host manifest 用 `allowed_origins` | host manifest 用 `allowed_extensions` | 属 desktop/bridge，不在本目录 |
| popup / content / onboarding | 源码在 `chromium/` | **复用 chromium 源码**，打包期汇入 firefox 包（不复制第二份；打包脚本属 `tools/`，待实现） | 完全复用 |

## 5. 构建与加载

- 仓库只含 `.ts` 源码，**不提交编译产物**；manifest 引用的 `background.js` 等是构建期产物。
- 构建期需把相对导入的 `.ts` 后缀重写为 `.js`（如 TS 5.7+ `rewriteRelativeImportExtensions`）
  或由 `tools/` 打包后加载解包扩展——构建链属 `tools/`，待实现——待确认。
- 类型检查（已实测）：`npx tsc --noEmit -p extensions/tsconfig.json`；
  shared 单测：`node --test --experimental-test-isolation=none extensions/shared/test/*.test.ts`。

## 6. 未验证项

- **浏览器运行时行为未验证**：`chrome.cookies` / `chrome.runtime` 的 Promise 形态、
  MV3 service worker 挂起后 `connectNative` 端口重连（本层按「下次动作时重建端口」处理）、
  Firefox background 的 `type: module` 支持——均未在真机验证。
- **manifest 加载未验证**：Chrome/Firefox 管理页加载（需先构建产物）。
- **desktop/bridge 未联调**：握手、方法面、错误码、推送目标清单均未与真实宿主对过。
- 类型检查与 shared 单测已实测通过（输出见交付报告）。

## 7. 待确认清单

1. **bridge 协议定稿**（方法面 / request 方向 / 结果形状），与 `desktop/bridge` 联调确认。
2. `session.query` 的「结构化候选」返回形状：docs 未定义，暂以 `StandardAccess` 承载。
3. `push.targets` 的推送目标清单形状（`{id,label}` 为控制字段）。
4. native messaging 宿主名（`com.aibridge.bridge` 为占位）与 Firefox 扩展 id，以 desktop/bridge 注册为准。
5. B1.3 目标登录态的桥方法面：`queryTargetState` 只回元数据（无 value），「Cookie 元数据」非契约类型，承载格式待定。
6. OTP 填充（B3.1/B-F3）：TOTP 计算属 core，契约无「当前验证码」载体，b 级退路暂只填用户名/密码。
7. 快捷键键位分配（B-F3 的 Ctrl+1/2 对应用户名/密码为暂定）。
8. `client_env.device_id` 扩展侧未采集（无稳定标识来源）。
9. 平台标识推导规则（UA → 契约 platform 词表的映射）待与 core 对齐。
10. 复制按钮的剪贴板清空（B5.2，默认 30 秒）：popup 关闭即销毁，扩展侧无法可靠定时清空，策略待与主程序（desktop 剪贴板）对齐。
11. X1 账号列表以 `access.credentials` 为行来源（同目标下 sessions-only 账号不显示），候选结构定稿后复核。
12. 构建产物生成方式（`.ts`→`.js` 重写/打包）属 `tools/`，待实现。

## 8. 与设计文档的出入（时点快照：2026-10-12，规范/文档更新后本节作废）

- **manifest 注释位置**：任务书要求 manifest 内注释权限用途；manifest.json 必须是严格 JSON，
  注释移至 `chromium/manifest.notes.md`（用途表与 manifest 一一对应）。
- **shared 文件命名**：任务书列 `cookie_io.ts / envelope_transport.ts / protocol.ts`；
  本目录此前已有一波实现（`types.ts / cookie.ts / envelope.ts / bridge.ts` + 38 项单测），
  按「同一业务规则只实现一份」在其上补齐：cookie 映射保留在 `cookie.ts`，`cookie_io.ts` 为
  cookies API 封装；桥消息类型保留在 `bridge.ts`，`envelope_transport.ts` 为收发层；
  `protocol.ts` 承载 client_env 随行与内部动作消息。
- **B2「推送目标」词表**：X1 写「目标设备选择」，B2 语义是同机浏览器互转、B-F4 是设备推送；
  两者共用 `push.targets` 一个清单（id/label 控制字段），目标粒度待联调定稿。
- B1.3 目标状态的「显示目标浏览器当前登录态」载体是桌面 W4（状态清单），扩展只提供
  `queryTargetState` 数据面；其桥方法面未定（见待确认 5）。
