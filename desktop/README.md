# desktop/ · AI-Bridge Windows 桌面端（Tauri 2 宿主层）

Windows 交互面与系统集成入口。显示名统一为 **AI-Bridge**，包标识 `ab-desktop` / `aibridge`。

## 1. 目录结构

```
desktop/
├─ index.html                 HTML 入口（窗口按 ?window=<label> 加载）
├─ package.json               前端脚本与依赖（Vue 3 + TS + Vite）
├─ tsconfig.json vite.config.ts
├─ src/                       前端 UI（Vue 3 + TypeScript）
│  ├─ main.ts                 窗口标签 → 视图组件挂载
│  ├─ styles/app.css          基础样式与设计 token
│  ├─ types/contract.ts       契约类型定义（对齐 contracts/*.schema.json）
│  ├─ types/ui.ts             UI 五态（idle/loading/success/error/empty）
│  ├─ hooks/useWindowControls.ts  窗口控制 Hook（关闭/隐藏/销毁/置顶）
│  └─ ui/
│     ├─ window-registry.ts   窗口标签 → 视图组件（13 个窗口 + 2 个预览入口）
│     ├─ components/          WindowShell、StateBlock
│     ├─ tray/                W1 托盘菜单、W15 通知 toast
│     ├─ candidate/           W2 候选条
│     ├─ panels/              W3–W10 弹出面板
│     └─ wizard/              W11–W14 一次性向导
├─ src-tauri/
│  ├─ Cargo.toml              独立 workspace 包 ab-desktop
│  ├─ tauri.conf.json         窗口与安全配置（CSP / capabilities）
│  └─ src/
│     ├─ main.rs              桌面入口与 --native-messaging-host 分流
│     ├─ host.rs              宿主状态（AIBRIDGE_DATA_DIR 优先）与转换器验签
│     ├─ commands/            Tauri 命令（converters 路径包含性校验、mcp 回环控制）
│     ├─ ui/                  窗口清单、托盘、候选条（UIA caret / 鼠标定位）、面板与向导
│     ├─ native/              DPAPI（含 optional entropy）、Hello、Clipboard（4MiB 上限）、UIA、Protocol
│     └─ bridge/              浏览器扩展 native messaging 帧编解码（1MiB 上限）
└─ scripts/                   结构检查与 SSR 渲染检查脚本
```

## 2. W1–W15 界面与窗口标签映射

| 编号 | 界面 | 形态 | 视图文件 | 窗口标签 |
|---|---|---|---|---|
| W1 | 托盘图标 + 菜单 | 原生托盘 | `src/ui/tray/TrayMenu.vue` | `preview-tray-menu` |
| W2 | 候选条 | 无边框置顶窗口 | `src/ui/candidate/CandidateBar.vue` | `candidate` |
| W3 | 库管理面板 | 弹出面板 | `src/ui/panels/LibraryPanel.vue` | `panel-library` |
| W4 | 状态清单面板 | 弹出面板（双视图） | `src/ui/panels/StatusPanel.vue` | `panel-status` |
| W5 | 临时会话面板 | 弹出面板 | `src/ui/panels/TemporarySessionPanel.vue` | `panel-sessions` |
| W6 | 审计面板 | 弹出面板 | `src/ui/panels/AuditPanel.vue` | `panel-audit` |
| W7 | 备份面板 | 弹出面板 | `src/ui/panels/BackupPanel.vue` | `panel-backup` |
| W8 | 转换器面板 | 弹出面板 | `src/ui/panels/ConverterPanel.vue` | `panel-converters` |
| W9 | 配对面板 | 弹出面板 | `src/ui/panels/PairingPanel.vue` | `panel-pairing` |
| W10 | 设置面板 | 弹出面板 | `src/ui/panels/SettingsPanel.vue` | `panel-settings` |
| W11 | 初始化向导 | 一次性窗口 | `src/ui/wizard/InitWizard.vue` | `wizard-init` |
| W12 | 配对向导 | 一次性窗口 | `src/ui/wizard/PairingWizard.vue` | `wizard-pairing` |
| W13 | 恢复向导 | 一次性窗口 | `src/ui/wizard/RestoreWizard.vue` | `wizard-restore` |
| W14 | 恢复演练向导 | 一次性窗口 | `src/ui/wizard/RecoveryDrillWizard.vue` | `wizard-drill` |
| W15 | 通知 toast | 系统通知 | `src/ui/tray/NotificationToast.vue` | `preview-notification-toast` |

## 3. 校验与测试命令

在 `desktop/` 目录执行：

```powershell
node scripts/check-structure.mjs   # 8/8 结构与契约字段一致性检查
node scripts/render-check.mjs      # 15/15 视图 SSR 渲染检查
npm run typecheck                  # vue-tsc --noEmit 严格类型检查
```

在仓库根目录执行桌面端 Rust 测试（支持通过 `AIBRIDGE_DATA_DIR` 指定受控可写目录）：

```powershell
$env:AIBRIDGE_DATA_DIR = "$PWD\target\test-data-dir"
cargo test --manifest-path desktop/src-tauri/Cargo.toml
```
