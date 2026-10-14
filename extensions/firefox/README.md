# firefox/ —— Firefox MV3 适配层

只承载 Firefox 与 Chromium 的**差异部分**；其余全部复用：

- **逻辑复用**：`../shared/` 同一份实现（`startBridgeHost` 桥接调度、`cookie_io` 读写、
  `protocol` 装配、`bridge` 消息定义）。
- **UI 复用**：popup / content / onboarding 源码在 `../chromium/`，打包期汇入 firefox 包，
  不复制第二份（打包脚本属 `tools/`，待实现——待确认）。

## 差异点

| 文件 | 差异 |
|---|---|
| `manifest.json` | ① `background.scripts`（event page）替代 `service_worker`；② 增加 `browser_specific_settings.gecko.id`（native messaging 注册需要，取值待与 desktop/bridge 的 `allowed_extensions` 对齐——待确认） |
| `background.ts` | ① 命名空间用 `browser.*`（Promise 形态）而非 `chrome.*`；② 方言标识 `firefox`（handshake.dialect / 信封 source.id） |

宿主侧（desktop/bridge 的 native messaging host manifest）差异不在本目录：
Chromium 用 `allowed_origins`，Firefox 用 `allowed_extensions`。

## 未验证项

- Firefox MV3 background 的 `type: "module"`（ES 模块脚本）支持未真机验证。
- `browser.cookies` / `browser.runtime` 的实际行为未真机验证（环境声明见 `../webext.d.ts`）。
