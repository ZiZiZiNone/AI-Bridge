# mobile/ · AI-Bridge 移动端（iOS SwiftUI / Android Compose + UniFFI）

原生移动端源码树与 UniFFI 接口声明。显示名统一为 **AI-Bridge**，包标识 `com.aibridge.mobile` / `aibridge`。

## 1. 目录结构

```
mobile/
├─ README.md                     移动端结构与构建说明
├─ ios/
│  └─ App/
│     ├─ Core/
│     │  ├─ LanTransport.swift       Bonjour 局域网传输（TXT fingerprint 校验 + 64 帧有界队列）
│     │  └─ SafariCookieBridge.swift Safari Cookie 桥（精确或子域后缀 domain 过滤）
│     └─ Pages/
│        ├─ PairingView.swift        M7 配对与 X25519 公钥 SHA-256 指纹核对
│        └─ QRScanView.swift         扫码解析与 64 位 hex 指纹验证
├─ android/
│  └─ app/src/main/
│     ├─ AndroidManifest.xml     宿主 Activity、AutofillService、CredentialProviderService、局域网权限
│     ├─ res/                    strings.xml · xml/autofill_service.xml
│     └─ java/com/aibridge/mobile/
│        ├─ MainActivity.kt      宿主入口与组合根装配
│        ├─ AppDependencies.kt   构造注入依赖集合
│        ├─ core/
│        │  ├─ LanTransport.kt       NSD 局域网传输（TXT fingerprint 认证 + 64 帧有界队列）
│        │  ├─ CookieFidelity.kt     Set-Cookie 构造与 CR/LF/NUL/; 注入拦截
│        │  └─ QuickUnlockStore.kt   Keystore 生物识别绑定（setUserAuthenticationRequired + 显式开关）
│        ├─ ui/                  RootNav、Labels（枚举穷尽映射）与 M1–M9 页面
│        ├─ autofill/            M9：AIBridgeAutofillService（锁定态安全回退）
│        ├─ credential/          M9：AIBridgeCredentialProviderService + CredentialManagerBridge
│        └─ webview/             ContainerWebView + ContainerSessionBridge
└─ ffi/
   ├─ aibridge.udl               UniFFI 接口声明（唯一事实源）
   ├─ uniffi.toml                绑定生成配置
   └─ gen/                       生成物目录（由 uniffi-bindgen 产出，.gitignore 排除）
```

## 2. M1–M9 界面落点

| # | 界面 | iOS | Android |
|---|---|---|---|
| M1 | 锁屏 / 解锁 | `ios/App/Pages/UnlockView.swift` | `.../ui/pages/UnlockScreen.kt` |
| M2 | 库列表 + 搜索（五 tab） | `ios/App/Pages/VaultListView.swift` | `.../ui/pages/VaultListScreen.kt` + `ui/RootNav.kt` |
| M3 | 条目详情 + 编辑表单 | `ios/App/Pages/EntryDetailView.swift` | `.../ui/pages/EntryDetailScreen.kt` |
| M4 | 生成器（默认 20 位 · 不存历史） | `ios/App/Pages/GeneratorView.swift` | `.../ui/pages/GeneratorScreen.kt` |
| M5 | 容器浏览器（全屏 WebView） | `ios/App/Pages/ContainerBrowserView.swift` | `.../ui/pages/ContainerBrowserScreen.kt` + `.../webview/` |
| M6 | 状态 / 临时会话 | `ios/App/Pages/SessionStatusView.swift` | `.../ui/pages/SessionStatusScreen.kt` |
| M7 | 配对与设备（含指纹确认屏） | `ios/App/Pages/PairingView.swift` + `QRScanView.swift` | `.../ui/pages/PairingScreen.kt` |
| M8 | 备份与设置 | `ios/App/Pages/SettingsBackupView.swift` | `.../ui/pages/SettingsBackupScreen.kt` |
| M9 | 系统自动填充 UI | `ios/CredentialProvider/` | `.../autofill/` + `.../credential/` |

## 3. FFI 绑定同步与生成

修改 `mobile/ffi/aibridge.udl` 后，必须在仓库根目录同步并重新生成绑定：

```powershell
powershell -ExecutionPolicy Bypass -File tools/sync-ffi-udl.ps1
cargo run -p ab-ffi --bin uniffi-bindgen -- generate core\ffi\src\aibridge.udl --crate ab_ffi -l kotlin -l swift -o mobile\ffi\gen --no-format
```
