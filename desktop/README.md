# desktop/ · AI-Bridge Windows 桌面端（Tauri 2 宿主层）

Windows 交互面 + 系统集成。**薄入口**：只做「接收输入 → 调用下层 → 返回结果」，
无业务判断、无数据转换、无状态计算；业务规则一律在 core（见 `docs/架构与契约.md` §0）。
显示名一律 **AI-Bridge**；代码标识 `ab-desktop` / `aibridge`。

## 1. 分层与依赖方向

```
contracts/*.schema.json ──→ core/（ab-*，唯一业务规则处）
                                   ↑ 仅公开 API
                        desktop/（本目录：宿主层，薄入口）
```

- 单向依赖，禁反向与循环：`desktop` 只依赖 `core` 与 `contracts`，不反向被依赖。
- 无全局单例 / 静态门面 / 服务定位器：Rust 侧依赖一律经参数注入；
  `src-tauri/src/native/` 是 core 注入 trait 的**实现骨架**（core trait 的唯一事实源在 core，
  本层不重复声明 trait，core 落地后补 `impl` 头，方法签名已在骨架固定）。
- `bridge/` 只做协议翻译，**禁止** `bridge → ui` 反向依赖；cookie / 会话原文不经过 UI 层。
- 前端只认契约字段：`src/types/contract.ts` 与 `contracts/*.schema.json` 逐字段一致（含必填性），
  由 `npm run check:structure` 校验，禁止在前端另写一套结构。

## 2. 目录结构

```
desktop/
├─ index.html                 唯一 HTML 入口（窗口按 ?window=<label> 加载）
├─ package.json               npm 脚本 / 依赖（Vue 3 + TS + Vite）
├─ tsconfig.json vite.config.ts
├─ env.d.ts
├─ src/                       前端 UI（Vue 3 + TypeScript）
│  ├─ main.ts                 入口：窗口标签 → 视图组件 → 挂载（无业务逻辑）
│  ├─ styles/app.css          基础样式与设计 token 占位
│  ├─ types/contract.ts       契约镜像（逐字段对齐三个 schema，零逻辑）
│  ├─ types/ui.ts             UI 五态（idle/loading/success/error/empty）
│  ├─ hooks/useWindowControls.ts  UI 侧唯一调用窗口 API 处（关闭/隐藏/销毁/置顶）
│  └─ ui/
│     ├─ window-registry.ts   窗口标签 → 视图组件（13 个窗口 + 2 个预览入口）
│     ├─ components/          WindowShell（标题条/关闭/置顶）、StateBlock（五态容器）
│     ├─ tray/                W1 托盘菜单、W15 通知 toast
│     ├─ candidate/           W2 候选条
│     ├─ panels/              W3–W10 弹出面板
│     └─ wizard/              W11–W14 一次性向导
├─ src-tauri/
│  ├─ Cargo.toml              包 ab-desktop（独立 workspace；tauri 2.12.3，已编译验证）
│  ├─ build.rs                tauri-build（标准形态）
│  ├─ tauri.conf.json         productName / devUrl 1420 / frontendDist ../dist / CSP
│  ├─ capabilities/default.json  最小能力集（仅窗口显示类权限）
│  ├─ icons/                  bundle 与托盘用图标（占位，见 scripts/make-icons.py）
│  └─ src/
│     ├─ main.rs              装配入口：命令注册 + 托盘 + native messaging 分流
│     ├─ ui/                  宿主 UI 层
│     │  ├─ window.rs         窗口清单（唯一事实源）+ 打开/隐藏/销毁/切换 + 3 个命令
│     │  ├─ tray/{mod,menu}.rs  W1 托盘三态 + 菜单清单（11 项）
│     │  ├─ candidate/mod.rs  W2 唤起与显示锚点（B6.1）
│     │  ├─ panels/mod.rs     W3–W10 面板清单与开/关语义
│     │  └─ wizard/mod.rs     W11–W14 一次性窗口语义（finish = 销毁）
│     ├─ native/              系统能力：core 注入 trait 的实现骨架
│     │  ├─ dpapi.rs          VaultStore（DPAPI 存储）+ Entropy（CNG 随机源）
│     │  ├─ hello.rs          Windows Hello 解锁因子（仅解锁，不作密钥派生）
│     │  ├─ clipboard.rs      剪贴板原语（set/read/clear/sequence；30 秒策略在 core）
│     │  ├─ uia_typing.rs     UIA 填表与前台焦点读取（站点逻辑属转换器）
│     │  └─ protocol.rs       aibridge:// 解析（纯函数）+ 注册/反注册骨架
│     └─ bridge/              浏览器扩展桥（只做协议翻译）
│        ├─ ext_protocol.rs   4 字节小端长度前缀 + JSON 帧编解码（1 MiB 上限）
│        └─ messaging_host.rs stdio 逐帧循环（处理函数经参数注入）
└─ scripts/                   校验与图标脚本（Node / Python，非运行时依赖）
```

## 3. W1–W15 界面 ↔ 文件 ↔ 窗口标签

| 编号 | 界面 | 形态 | 视图文件 | 窗口标签 |
|---|---|---|---|---|
| W1 | 托盘图标 + 菜单 | 原生托盘（非窗口） | `src/ui/tray/TrayMenu.vue` | `preview-tray-menu`（仅 dev 预览） |
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
| W15 | 通知 toast | 系统通知（非窗口） | `src/ui/tray/NotificationToast.vue` | `preview-notification-toast`（仅 dev 预览） |

窗口几何 / 形态的**唯一声明处**是 `src-tauri/src/ui/window.rs` 的 `WINDOW_SPECS`（13 个窗口）；
前端 `window-registry.ts` 只做「标签 → 组件」，两处标签由 `check:structure` 强制双向相等。

## 4. 构建前置

| 项 | 要求 | 本机状态 |
|---|---|---|
| Rust 工具链 | `rustup` + MSVC 目标（`x86_64-pc-windows-msvc`），Tauri 2 要求 Rust ≥ 1.77 | **未安装**（禁止执行 cargo 命令） |
| Tauri CLI | `cargo install tauri-cli --version "^2"` 或 `npm i -D @tauri-apps/cli@^2` | 未安装 |
| Node.js | ≥ 20（Vite 6 要求） | v22.22.1 ✅ |
| npm | 随 Node | 10.9.4 ✅ |
| WebView2 Runtime | Win11 自带；Win10 由 bundle 的 `downloadBootstrapper` 安装 | 未验证 |
| Python + Pillow | 仅 `scripts/make-icons.py` 需要 | 3.11 + Pillow 12.3.0 ✅ |

开发顺序（Rust 工具链就绪后）：

```powershell
npm install                      # 只装前端依赖
npm run dev                      # 仅前端预览：http://localhost:1420/?window=panel-library
cargo tauri dev                  # 完整桌面应用（会拉起上面的 dev server）
cargo tauri build                # 打包 NSIS / MSI
```

沙盒/受限环境注意（本仓库实测）：
- `npm install` 需要 `--cache <工作区内目录>`（默认缓存目录在用户目录，可能被文件沙盒拒绝），
  且可能需要 `--ignore-scripts`（esbuild 的 postinstall 会派生带管道 stdio 的子进程）。
  本仓库用：`npm install --ignore-scripts --cache .npm-cache`（`.npm-cache/` 已被根 `.gitignore` 忽略）。
- `vite build` 在该沙盒内不可执行（esbuild 服务进程 EPERM）。等效替换验证见下节 `check:render`。

## 5. 校验与测试（实跑结果）

| 命令 | 内容 | 实跑结果 |
|---|---|---|
| `npm run typecheck` | `vue-tsc --noEmit`：全部 `.vue` / `.ts` 严格类型检查 | **通过**（exit 0，无输出） |
| `npm run check:structure` | 8 项结构硬规则（见下） | **8/8 通过** |
| `npm run check:render` | 15 个界面真渲染（`@vue/compiler-sfc` + SSR，Node 内） | **15/15 通过** |
| `cargo test` | Rust 单测（窗口清单 / 菜单 / 协议解析 / 帧编解码） | **通过 22/22**（2026-10-12 实跑；tauri 2.12.3 经 `tools/fetch-vendor.py` vendored 依赖离线解析编译） |
| `npm run build` / `cargo tauri dev` | 完整构建与联调 | **未执行**（见未验证声明） |

`check:structure` 的 8 项：① Rust 模块结构与 `docs/目录结构.md` 一致；② W1–W15 各一个视图且分组数量
（托盘 2 / 候选条 1 / 面板 8 / 向导 4）；③ 显示名口径与禁用名；④ 窗口标签 Rust ↔ TS 双向相等；
⑤ 托盘菜单 Rust ↔ Vue 全等；⑥ 契约 11 个类型 57 个字段与 schema 逐字段一致（含必填性）；
⑦ 依赖方向（native/bridge 不碰 tauri 与 ui，desktop 不引 core crate）；⑧ SFC 块顺序与 `lang="ts"`。

`check:render` 说明：用纯 JS 链路（`@vue/compiler-sfc` 编译 SFC → `typescript` 去类型 →
Node 动态 `import` → `@vue/server-renderer` 渲染）真正执行模板、`defineProps`/`withDefaults`、
`StateBlock` 五态分支；`@tauri-apps/api/window` 在检查中被替换为测试替身（Node 无宿主窗口 API）。
临时目录 `.render-check/` 在脚本结束时删除。

## 6. 未验证声明

以下各项**未执行 / 未验证**（`vite build` 的 esbuild 派生进程与真实 WebView 均不可得；Rust 侧已于
2026-10-12 实跑 `cargo test` 22/22 通过，此前的「无工具链」限制经 vendored 依赖解除）：

1. ~~cargo 未运行~~ **已解除**：`cargo test` 通过（22/22）；`Cargo.toml` 的 Tauri 2.x 版本线
   解析为 tauri 2.12.3 并编译通过。`cargo clippy` 未运行（25 条 warning 多为骨架未用代码）。
2. Tauri 2 API 形态**已编译验证**（`WebviewWindowBuilder` 链式调用、`AppHandle::get_webview_window`、
   `tauri::generate_handler!` / `generate_context!` 等均通过编译）；**运行时行为**（窗口生命周期、
   置顶、IPC 往返）仍未经真实 WebView 验证。
3. `tauri.conf.json`（`app.windows`、`security.csp`、`bundle.*`、`frontendDist`）与
   `capabilities/default.json` 的权限标识符**运行时未验证**（`core:default` 与窗口类权限按 Tauri 2 文档写）。
4. 托盘实现为骨架：`ui::tray::install` / `set_state` 是 `todo!()`，托盘图标三态资源
   （`tray-locked/ready/alert.png`）**未生成**（只有 bundle 用 `icons/*`）；托盘菜单因此暂无运行时效果。
5. `native/` 全部方法为 `todo!()` 骨架：DPAPI（`CryptProtectData` 等）、Windows Hello（WinRT
   `UserConsentVerifier`）、剪贴板 API、UIA / `SendInput`、注册表协议注册，**均未实现、未验证**。
   另：core 已落地（`core/` 各 ab-* crate），但桌面侧**尚未接线**（无 `impl <core trait> for <类型>`、
   无 ab-* 依赖），骨架只有与 trait 方法对应的同签名方法与接口注释，接线属后续任务。
6. `bridge/ext_protocol.rs` / `messaging_host.rs` 的单测**已运行，22/22 通过**（帧编解码、
   边界帧、协议错误路径）；native messaging 的真实 stdio 往返与浏览器侧握手未验证；
   `--native-messaging-host` 分流未经进程级验证（Chromium 清单不支持带参数的 path，
   实现期需独立可执行文件）。
7. 前端：`vite build`（Rollup 打包）与 `cargo tauri dev` 联调**未执行**；`@tauri-apps/api`
   在真实 WebView 中的行为（窗口关闭/销毁/置顶、CSP 与 IPC 连通）**未验证**；
   界面数据接入（core 命令）尚未接线，面板显示空态。
   另：**未有组件交互测试**（`@vue/test-utils` / vitest 依赖 esbuild 派生进程，本沙盒同样不可用）；
   按 `frontend/frameworks/vue3/testing.md`，补组件测试时交互元素需加 `data-test` 属性，
   骨架期未加（避免为不存在的测试预留选择器）。
8. 图标为脚本生成的占位图（品牌色方块 + 连线 + 文字），**非设计稿**，需按设计替换后重跑
   `python scripts/make-icons.py`。
9. Node 依赖以 `npm install --ignore-scripts --cache .npm-cache` 安装（默认缓存目录与
   esbuild postinstall 均被本沙盒拒绝）；`package-lock.json` 为该次安装生成，
   正常开发机应执行标准 `npm install` 重新生成并提交 lockfile。
