# AI-Bridge

本地优先的登录态管理器：**密码存储 + 登录状态管理 + 登录状态扩散**。Windows 桌面为中枢，浏览器扩展与 iOS / Android 为两端，可向 AI 签发临时会话。

## 特性

- **加密凭据库**：账密 / passkey / TOTP，强密码生成，会话快照与回滚
- **登录状态扩散**：浏览器会话注入、跨浏览器桥、跨设备双向（局域网 + 面对面 QR，端到端加密；离线加密传送包兜底）
- **临时会话**：本地 MCP server 向 AI 签发短期、限域名、可撤销、可审计的临时凭证，用完/到期自动释放
- **标准接入**：外部 app 以「数据产物 + 标准转换器」接入（官方签发、验签启用），核心零适配

## 设计约束（硬承诺）

本地、不联网、无账号、无遥测；无云同步、无跨网中继；Android Chrome 与微信类会话注入不支持（完整边界见 docs/功能点全景.md）。

## 技术栈

- 桌面：Tauri 2 + Rust（核心共享，内嵌 MCP server）
- 浏览器扩展：TypeScript，MV3（Chrome / Edge / Firefox）
- 移动端：iOS / Android 原生（SwiftUI / Compose）+ Rust 核心经 UniFFI
- 加密：KDBX4 格式级兼容（生态工具可开，灾备逃生），内部 Argon2id + AES-256-GCM；DPAPI / Windows Hello 作解锁因子

## 仓库结构

```
contracts/   标准接入契约（3 个 schema，唯一事实源）
core/        Rust 核心（纯逻辑，桌面/移动共享）
desktop/     Windows 端（Tauri 2）
extensions/  浏览器扩展
mobile/      iOS / Android（原生 UI + 系统扩展）
converters/  标准转换器（QQ 首接）
tools/ tests/
```

## 设计文档（本地开发文档，不入库）

设计过程文档位于 `docs/`，经 `.gitignore` 排除在版本库外（非最终产物）：

- `docs/功能点全景.md` — 全部功能点 + 27 个界面清单（UI 设计依据）
- `docs/目录结构.md` — 仓库结构与模块职责
- `docs/架构与契约.md` — 代码模块结构与契约（实现依据）
- `docs/流程/` — 浏览器 / 桌面 / 移动 / AI 四端流程与流程图提示词
- `docs/调研/` — 三份可行性调研报告（含来源 URL）

## 开发约定

- 工程规范库：`C:\Users\HKX\Desktop\AI-Specs`，开发前按其 README「加载策略」必读四件并遵循
- **所有 commit 须经项目所有者确认后方可提交**；AI 产出的改动先展示再入库
- 构建 / 类型检查 / 测试必须实际执行；执行不了须显式声明未验证项

## 状态

命名：项目 **AI-Bridge**，代码标识 `aibridge`。

**实现进度（2026-10-12 时点快照）**：契约层三 schema + core 九 crate（ab-contract/crypto/vault/state/broker/audit/sync/mcp/converter-host，cargo test 全绿）+ ab-ffi（UniFFI 镜像，Kotlin/Swift 绑定已生成）已落地；转换器 SDK + qq 首接（占位签名，发布链拒载属设计内）；桌面 Tauri 2 宿主已接线 core（47 命令面）；浏览器扩展 chromium/firefox（MV3）；移动端 android（Compose + FFI 接线）与 iOS（SwiftUI 全套，无 Xcode 未编译）。测试基线：core ~144 + tests/ 四套 + desktop 63 + 扩展 38 + qq 管线 30 项。验收一键入口 `tools/verify-all.ps1`。安全模型 §10 状态表为逐项口径。

> 早期版本曾记「代码实现未开工」——该状态已过时，实现进展以本节 + `docs/安全模型.md` §10 为准。
