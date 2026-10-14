# chromium/manifest.json 逐项用途（最小权限原则）

manifest.json 必须是严格 JSON（浏览器加载器不接受注释），逐项用途记在本文件，
与 manifest.json 一一对应。

## permissions

| 权限 | 用途（功能必需） | 消费点 |
|---|---|---|
| `cookies` | 读取（含 HttpOnly）/ 注入 / 清除站点登录态 | B1.1 会话读取、B1.2 注入与清除、B2 跨浏览器桥源侧读取 |
| `nativeMessaging` | 与本机 AI-Bridge 主程序（desktop/bridge）通信 | B-F1 候选查询、B-F2 会话桥、X3 握手引导 |

## host_permissions

| 权限 | 用途（功能必需） | 消费点 |
|---|---|---|
| `<all_urls>` | 在任意站点读写该站点 cookie（cookies API 按宿主权限授权）；content script 填表 | 本扩展的功能本体是任意站点的登录态管理（B1）；B3.1 表单填充退路 |

> 只申请以上三项；未申请 tabs / storage / clipboardWrite 等：
> 当前页 URL 经 activeTab 之外的 host 权限即可读取；复制走 popup 内 `navigator.clipboard`（用户手势）；
> 无持久化存储需求（状态在 core）。

## 其他键

| 键 | 用途 |
|---|---|
| `background.service_worker` | MV3 常驻事件入口（绑定 shared 桥接调度） |
| `action.default_popup` | X1 扩展弹窗 |
| `content_scripts` | X2 表单浮标 + 表单填充（b 级退路），`document_idle` 注入 |
| `description` | 商店/管理页展示；跨设备文案统一为「设备同步（局域网）」 |

## 编译产物说明

manifest 引用的 `background.js` / `content/content.js` 为构建期产物（源码是 `.ts`）；
仓库不提交编译产物（见 extensions/README.md「构建与加载」）。
