# AI-Bridge · iOS 源树（mobile/ios/）

> 交付范围：iOS 18+ 原生源码树（SwiftUI App + Credential Provider Extension + Safari Web Extension + WKWebView 容器），
> 承载 `docs/功能点全景.md` §3.3 的 M1–M9。移动 UI 纯原生 SwiftUI（`docs/架构与契约.md` §3 mobile 行）。
> **不含**工程文件（`xcodeproj` / `pbxproj`）与任何编译产物：工程由 Xcode 按 §5 生成，生成物不入库。
> 显示名一律 **AI-Bridge**（代码标识 `aibridge`）。

## 1. 目录结构与文件清单

```
mobile/ios/
├─ README.md                       本文件
├─ app/                            SwiftUI App（M1–M8；薄入口）
│  ├─ AIBridgeApp.swift            @main 入口 + 组合根装配点 + M1 锁屏门（RootFlowView）
│  ├─ AppEnvironment.swift         组合根依赖集合（构造注入，无单例）
│  ├─ RootView.swift               M2 主屏：底部五 tab（库 / 生成器 / 浏览器 / 设备 / 设置）
│  ├─ Core/                        core trait 的宿主实现 + 装配（多 target 共享，见 §2）
│  │  ├─ CoreAssembly.swift        组合根装配（newCore 调用点；App 与扩展各进程各实例化）
│  │  ├─ KeychainVaultStore.swift  VaultStore 实现（Keychain 存取不透明字节）
│  │  ├─ SystemClock.swift         Clock 实现
│  │  ├─ SystemEntropy.swift       Entropy 实现（SecRandomCopyBytes）
│  │  ├─ LanTransport.swift        Transport 实现（仅局域网直传；Bonjour + TCP）
│  │  ├─ QuickUnlockStore.swift    快速解锁因子（Keychain 生物门控，仅解锁因子）
│  │  ├─ AppSettings.swift         非秘密偏好（UserDefaults；秘密一律不进）
│  │  └─ Labels.swift              UI 文案口径集中处（类型 / 设备 / 同步项标签）
│  ├─ WebView/                     M5 容器（多 target 共享，见 §2）
│  │  ├─ WebViewContainer.swift        WKWebView 承载（导航回调注入）
│  │  ├─ ContainerSessionBridge.swift  SessionSource 实现（WKHTTPCookieStore，含 HttpOnly）
│  │  └─ CookieFidelity.swift          契约 Cookie ⇄ HTTPCookie 全属性映射（零丢弃）
│  └─ Pages/
│     ├─ PageScaffold.swift        页面脚手架（M 编号锚点 + 承载功能点）
│     ├─ UnlockView.swift          M1 锁屏 / 解锁
│     ├─ VaultListView.swift       M2 库列表 + 搜索（含回收站）
│     ├─ EntryDetailView.swift     M3 条目详情 + 编辑表单
│     ├─ GeneratorView.swift       M4 生成器（默认 20 位 · 不存历史）
│     ├─ ContainerBrowserView.swift M5 容器浏览器（捕获 / 扩散发送 / A8 导出）
│     ├─ SessionStatusView.swift   M6 状态 / 临时会话（含新建会话表单）
│     ├─ PairingView.swift         M7 配对与设备（QR 扫描 + 指纹确认屏 + A8 导入）
│     ├─ FingerprintConfirmView.swift M7 指纹确认屏
│     ├─ QRScanView.swift          M7 QR 扫描（AVFoundation）
│     └─ SettingsBackupView.swift  M8 备份与设置
├─ credential-provider/            M9 Credential Provider Extension
│  ├─ CredentialProviderViewController.swift  ASCredentialProviderViewController
│  ├─ CredentialListView.swift     解锁 → 选账号 → 系统填入
│  └─ Info.plist                   扩展点声明（值待校验，见 §6）
└─ safari-ext/                     Safari Web Extension（会话桥）
   ├─ SafariWebExtensionHandler.swift  NSExtensionRequestHandling（JS 消息 ⇄ core）
   ├─ SafariCookieBridge.swift     SessionSource 实现（Safari 侧）+ 契约 Cookie ⇄ JSON
   ├─ background.js                cookie 读取 / 注入（含 HttpOnly 与完整属性）
   ├─ manifest.json                扩展清单（cookies 权限）
   └─ Info.plist                   扩展点声明（值待校验，见 §6）
```

### M1–M9 映射

| # | 界面 | iOS 文件 | 承载功能点 |
|---|---|---|---|
| M1 | 锁屏 / 解锁 | `app/Pages/UnlockView.swift`（门：`app/AIBridgeApp.swift` RootFlowView） | S1 |
| M2 | 库列表 + 搜索 | `app/Pages/VaultListView.swift` + `app/RootView.swift` | A1.2 |
| M3 | 条目详情 + 编辑表单 | `app/Pages/EntryDetailView.swift` | A1.1、B5.1、E5 |
| M4 | 生成器 | `app/Pages/GeneratorView.swift` | A5（默认 20 位、不存历史、实时强度条、口令词模式） |
| M5 | 容器浏览器 | `app/Pages/ContainerBrowserView.swift` + `app/WebView/` | 登录、会话捕获、E1/E2 |
| M6 | 状态 / 临时会话 | `app/Pages/SessionStatusView.swift` | A2、C 视图 |
| M7 | 配对与设备 | `app/Pages/PairingView.swift` + `FingerprintConfirmView.swift` + `QRScanView.swift` | E3、A8 |
| M8 | 备份与设置 | `app/Pages/SettingsBackupView.swift` | A6、S1/S2/S4/S8 |
| M9 | 系统自动填充 UI | `credential-provider/` | E4 |

### Target 归属（工程生成时的源码归属）

| target | 源码 |
|---|---|
| App（AIBridge） | `app/` 全部 |
| Credential Provider Extension | `credential-provider/` + `app/Core/` + `app/WebView/` + `app/AppEnvironment.swift` |
| Safari Web Extension handler | `safari-ext/` + `app/Core/` + `app/WebView/CookieFidelity.swift`（如需）+ `app/AppEnvironment.swift` |

（扩展是独立进程，复用同一份装配与 trait 实现源码——"同一实现全仓只写一份"。）

## 2. 与 android/ 的职责对齐

| 职责 | iOS | Android | 说明 |
|---|---|---|---|
| 宿主入口 + 组合根装配点 | `app/AIBridgeApp.swift` | `MainActivity.kt` | 均只装配与渲染 |
| 组合根依赖集合 | `app/AppEnvironment.swift` | `AppDependencies.kt` | 构造注入，无单例 |
| core trait 实现 | `app/Core/*`、`app/WebView/ContainerSessionBridge.swift` | `webview/ContainerSessionBridge.kt`（骨架） | iOS 侧为完整实现，Android 侧为骨架 |
| 容器 WebView | `app/WebView/WebViewContainer.swift` | `webview/ContainerWebView.kt` | 默认持久化 cookie 存储，不丢属性 |
| 五 tab 主结构 | `app/RootView.swift` | `ui/RootNav.kt` | tab 一致：库 / 生成器 / 浏览器 / 设备 / 设置 |
| M1–M8 页面 | `app/Pages/*View.swift` | `ui/pages/*Screen.kt` | 页面编号锚点一致 |
| M9 系统填充 | `credential-provider/`（AuthenticationServices） | `autofill/` + `credential/` | 仅账号类条目；解锁 → 选账号 → 填入 |
| 会话桥（扩展侧） | `safari-ext/` | —（Android Chrome 无扩展通道，明确不支持） | Safari 侧走 WebExtension cookies API |

## 3. FFI 依赖说明

- Swift 调用 `mobile/ffi/aibridge.udl` 经 uniffi-bindgen 生成的 `aibridge` 模块（`uniffi.toml` 的 `module_name`）；
  生成步骤见 `mobile/README.md` §6（`uniffi-bindgen generate ... --language swift`）。**生成物不入库、禁止手改**。
- core 公开面：`newCore(config:vaultStore:sessionSource:transport:clock:entropy:)`（UDL namespace 函数，按 UniFFI
  惯例生成为模块级自由函数）、`AibridgeCore` 六个子对象（vault/state/broker/sync/converterHost/audit）。
- 五个宿主 trait 由本树实现：`VaultStore`（Keychain）、`SessionSource`（容器桥 / Safari 桥）、
  `Transport`（局域网）、`Clock`、`Entropy`。
- 契约字段以 `contracts/*.schema.json` 的 snake_case 为准；cookie 8 字段（name/value/domain/path/
  secure/http_only/same_site/expires_at）在容器桥、Safari 桥、JS 三处映射均全量携带（A2 零丢弃）。
- **命名假设（未验证）**：UDL 的 trait / dictionary / 方法名按 UniFFI 惯例转为 Swift 协议与 camelCase
  （如 `listEntries(query:)`、`ScopeSpec(domainAllowlist:readOnly:singleUse:)`）；生成后需校准。
  A5 面的 Swift 侧符号同样待核对：`PassphraseSpec(words:wordCount:separator:)`、
  `Vault.generatePassphrase(spec:)`、自由函数 `generatorEntropyBits(spec:)` / `passphraseEntropyBits(spec:)`
  （UDL 对应 `Vault.generate_passphrase` 与 namespace 级 `generator_entropy_bits` / `passphrase_entropy_bits`；
  dictionary 不能挂方法，故熵函数镜像为自由函数——见 aibridge.udl 该处注释）。

## 4. 未验证声明（本机 Windows，无 Xcode / 无 Apple 工具链）

以下均**未执行**，不得视为已验证：

1. **未编译**：全部 Swift / SwiftUI / WebKit / AuthenticationServices / SafariServices / AVFoundation /
   Network 代码未经过任何编译或类型检查（需 macOS + Xcode 16+ / iOS 18 SDK）。
2. **UDL 生成与符号命名未验证**：`aibridge` 生成模块的类型名、方法名、trait 协议形态、错误枚举形态
   以 UniFFI 惯例书写（见 §3 命名假设），生成后逐一校准。
3. **平台集成未验证**：两份扩展 `Info.plist`（扩展点标识 / PrincipalClass）、entitlements、
   App 的 `NSCameraUsageDescription`（QR 扫描）未经真实构建 / 安装校验。
4. **WKHTTPCookieStore 同步桥未验证**：trait 为同步接口、平台 API 为异步，现以信号量桥接；
   在主线程调用 capture/inject 有死锁风险（core 侧应在工作线程调用；见待确认清单）。
5. **same_site 属性读取未验证**：Foundation `HTTPCookie` 无公开 sameSite 属性，现从
   `properties["SameSite"]` 读取；平台不透传时该字段为 nil（不臆造，其余 7 字段不受影响）。
6. **Safari 扩展消息通道未验证**：`sendNativeMessage` 目标标识、消息载体（userInfo/附件）形态
   为骨架约定，待真机联调校准。
7. **LanTransport 线协议未验证**：Bonjour 服务类型 `_aibridge._tcp`、TXT 键（device_kind/fingerprint）、
   4 字节大端长度帧为骨架约定，待与桌面端对齐。
8. **界面与交互未验证**：M1–M9 无真机视觉与交互验收；文案口径仅做源码级检查。
9. **端到端流程未验证**：M-F1–M-F5 依赖 core（ab-ffi）与绑定落地后联调。

## 5. 工程生成步骤（替代工程文件；不手写 xcodeproj）

前置：macOS + Xcode 16+（iOS 18 SDK）+ Apple Developer 账号（两个扩展需真机 + 签名）+ 真机 iOS 18+。

1. Xcode → File → New → Project → iOS → App：Product Name `AIBridge`，Display Name `AI-Bridge`，
   Interface = SwiftUI，Minimum Deployment = iOS 18.0；删除模板 `ContentView.swift` / `AIBridgeApp.swift`。
2. 把 `mobile/ios/app/` 全部源码加入 App target（`AIBridgeApp.swift` 为入口）。
3. File → New → Target → iOS → Credential Provider Extension：源码指向 `mobile/ios/credential-provider/`，
   Info.plist 用目录内已备文件；按 §1「Target 归属」把 `app/Core/`、`app/WebView/`、`AppEnvironment.swift`
   一并加入该 target。
4. File → New → Target → iOS → Safari Web Extension（仅 handler，Web 资源用 `mobile/ios/safari-ext/`
   的 `manifest.json` / `background.js`）：源码指向 `mobile/ios/safari-ext/`，Info.plist 用目录内已备文件。
5. 按 `mobile/README.md` §6 生成 Swift 绑定与静态库，加入各 target；Build Settings → Framework Search Paths
   指向绑定产物目录。
6. App target Info.plist 补 `NSCameraUsageDescription`（QR 扫描用途说明）；entitlements（App Group /
   Keychain Access Group）按团队签名要求配置——**最终值待确认（§6）**。
7. 运行：真机 → 设置 → 密码 → 密码选项 → 启用 AI-Bridge；Safari → 扩展 → 启用 AI-Bridge。

## 6. 待确认清单

1. **Info.plist / entitlements 最终值**：两扩展的扩展点标识与 PrincipalClass、App Group / Keychain
   Access Group、`NSCameraUsageDescription` 文案，待 Xcode 真实工程与签名校验。
2. **FFI 生成物命名**：`newCore` 自由函数、trait 协议名、错误枚举 case 名（如 `notFound`）待生成后校准；
   任务描述中的 `AibridgeCore.new_core(...)` 写法按 UDL 命名空间函数落地为模块级自由函数。
3. **接收侧注入入口**：UDL 公开面上没有「envelopes → 容器注入」的显式入口（M-F2 反向：PC 扩散来会话 →
   注入容器）；现按设计为 core 经 `SessionSource.inject` 回写，具体触发 API 待 core 侧确认。
4. **新建条目 account_ref 分配语义**：UDL 无 allocate API，现由宿主生成 UUID 暂代。
5. ~~A5.2 口令词模式~~ **已实现**（UDL 已镜像 `PassphraseSpec` / `generate_passphrase` / `*_entropy_bits`）：
   词表 / 词数 / 分隔符由 UI 配置（core 无默认、无内置词表）；词表为会话内配置、未持久化
   （core 无词表存储面，如需跨会话保存待定）。
6. **A6 备份接线**：UDL 已补 A6 面（`Vault.save/recovery_copy/from_backup/open_copy` + `RecoveryKey`），
   但 M8 尚未接线；M8 行仍显示「待接入」，且该页「A6 接口不在 FFI 公开面」文案已过时，需随接线任务一并更新。
7. **设备命名 / 解绑（E3）**：UDL 无设备管理 API，M7 设备列表仅展示 `Transport.peers()`。
8. **passkey / TOTP 的系统填充形态**（M9）：UDL 无填充形状定义，现仅填充 username/secret。
9. **快速解锁密钥包装形态**：现为 Keychain 生物门控直接保存主口令（仅解锁因子），需与 core 密钥层次对齐。
10. **同步 trait 的调用线程**：core 应在工作线程调用 `SessionSource.capture/inject`（见 §4.4）。

## 7. 与 mobile/README.md / android/ 的出入

1. **目录命名**：本任务交付指令给定 `app/` / `credential-provider/` / `safari-ext/`，与 `mobile/README.md`
   §1 的 `App/` / `CredentialProvider/` / `SafariWebExtension/` 命名不同（内容职责一致）；
   `WebViewContainer/` 并入 `app/WebView/`。按用户指令（COM-008）执行，如需对齐文档字面路径移动目录即可。
2. **mobile/README.md 状态行**称"iOS 源树已清空、iOS 实现整体延后"，本任务按新指令重建并实现 M1–M9；
   README 的"页面空壳"描述已被本实现超出（页面为可交互实现，业务判断仍全在 core）。
3. **iOS 侧 trait 实现为完整实现**（Keychain / WKHTTPCookieStore / Bonjour+TCP / SecRandom），
   android/ 侧同类为骨架 no-op；结构职责对齐（见 §2），实现深度不同。
4. **A8 导出/导入分置**：导出（pack）入口在 M5 捕获会话后，导入（unpack）入口在 M7（A8 归属 E3/E 域）；
   `docs/功能点全景.md` §3.3 将 A8 列在 M7，导出侧从会话来源侧进入属实现落点选择。
