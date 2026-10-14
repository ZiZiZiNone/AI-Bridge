# AI-Bridge · Android 源树（mobile/android/）

> 交付范围：Android 原生源码树（Compose App + Autofill / Credential Provider 系统服务 + WebView 容器），
> 承载 `docs/功能点全景.md` §3.3 的 M1–M9，与 `mobile/ios/` 职责对齐（iOS README §「android 职责对齐」表反向同构）。
> **不含** Gradle 工程文件与任何编译产物：`build.gradle.kts` / `settings.gradle.kts` 由宿主环境按
> `mobile/README.md` §FFI 生成步骤接线；生成物不入库。
> 显示名一律 **AI-Bridge**（代码标识 `aibridge`）。

## 1. 目录结构

```
mobile/android/app/src/main/
├─ AndroidManifest.xml              宿主 + 两个系统服务声明（exported=true 为平台要求，
│                                    受 BIND_AUTOFILL_SERVICE / BIND_CREDENTIAL_PROVIDER_SERVICE 保护；
│                                    allowBackup=false）
├─ java/com/aibridge/mobile/
│  ├─ MainActivity.kt               宿主入口 + 组合根装配点
│  ├─ AppDependencies.kt            组合根依赖集合（构造注入，无单例）
│  ├─ core/                         宿主 trait 实现与能力
│  │  ├─ CoreAssembly.kt            newCore 调用点（App 与扩展各进程各实例化）
│  │  ├─ KeystoreVaultStore.kt      VaultStore（Android Keystore AES-GCM 包裹，密钥不可导出）
│  │  ├─ QuickUnlockStore.kt        快速解锁因子（Keystore 包裹 + setUserAuthenticationRequired，
│  │                                P0-2 修复后与 iOS kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly 同口径）
│  │  ├─ BiometricGate.kt           生物识别提示门（失败 fail-closed）
│  │  ├─ LanTransport.kt            Transport（NSD + TCP；认证缺口见 §4 待确认）
│  │  ├─ SystemClock.kt / SystemEntropy.kt   Clock / Entropy（SecureRandom）
│  │  ├─ AppSettings.kt             非秘密偏好（SharedPreferences；秘密一律不进；
│  │                                含 S1 quickUnlockEnabled 开关，默认关）
│  │  └─ Labels.kt                  UI 文案口径集中处
│  ├─ ui/                           Compose 页面（M1–M8 + RootNav 五 tab）
│  │  └─ pages/                     UnlockScreen / VaultListScreen / EntryDetailScreen /
│  │                                GeneratorScreen / ContainerBrowserScreen / SessionStatusScreen /
│  │                                PairingScreen + QRScanScreen + FingerprintConfirmScreen /
│  │                                SettingsBackupScreen / SystemAutoFillScreen
│  ├─ autofill/                     M9：AutofillService（仅账号类条目）
│  ├─ credential/                   M9：CredentialProviderService + CredentialManagerBridge
│  └─ webview/                      M5：ContainerWebView + ContainerSessionBridge + CookieFidelity
│                                    （cookie 8 属性映射，含 HttpOnly；Set-Cookie 拼接的分隔符
│                                    校验属审查报告 P2，见 §4）
└─ res/                             strings.xml（app_name=AI-Bridge）· xml/autofill_service.xml
```

## 2. FFI 依赖与绑定生成物核对状态

Kotlin 绑定来自 `com.aibridge.mobile.ffi`（`mobile/ffi/uniffi.toml` 的 `package_name`），
由 `mobile/ffi/gen/aibridge.kt` 生成物提供。核对状态（2026-10-12）：绑定已生成，但
**Gradle 工程未建立、无 Android SDK，全部 Kotlin/Compose 未编译**；UniFFI 惯例命名
（newCore(...)、对象方法 camelCase）待首次编译核对。宿主 trait 实现抛非声明异常时
UniFFI 映射为 INTERNAL 类错误——真机首验优先验证「无 blob 首次解锁」路径。

## 3. 与 iOS 的职责对齐（差异表）

| 面 | iOS | Android |
|---|---|---|
| M1 快速解锁开关 | AppSettings.quickUnlockEnabled（默认关） | 同（P0-2 同批修复） |
| 因子存储 | Keychain + biometryCurrentSet | Keystore + setUserAuthenticationRequired(true) |
| M5 容器 | WKWebView + WKHTTPCookieStore | WebView + CookieManager（桥实现 SessionSource） |
| M9 | Credential Provider Extension | AutofillService + CredentialProviderService 双服务 |
| 生物识别门 | LocalAuthentication | BiometricPrompt（BiometricGate 封装） |
| FFI | aibridge Swift 模块（staticlib） | com.aibridge.mobile.ffi（cdylib/staticlib，见 §4-3） |

## 4. 未验证声明与待确认（审查报告 2026-10-12 P2 相关项 + 环境边界）

1. **未编译**：无 Gradle/SDK，Kotlin/Compose/系统服务全部未编译；`AIBridgeAutofillService` /
   `AIBridgeCredentialProviderService` 中的 TODO 位于可实例化服务上（首验须清）。
2. **LanTransport 无认证**（审查报告 P1-14）：NSD 发现忽略 TXT fingerprint、ServerSocket
   绑 0.0.0.0、inbound 无界队列——载荷机密性由 core X25519+AEAD 保障，缺口在认证/可用性；
   与 iOS LanTransport 同批修复属待办（见 docs/审查报告 §7）。
3. **crate-type 缺 staticlib**（`core/ffi/Cargo.toml`）：与 iOS 链接步骤冲突，待补
   `crate-type = ["cdylib", "staticlib", "rlib"]`——改后须重验 cargo test。
4. **CookieFidelity Set-Cookie 拼接**无 `\r\n`/`;` 校验：cookie 属性注入面，待加校验。
5. **AndroidManifest 权限**：NSD/TCP 需 `INTERNET`/`ACCESS_NETWORK_STATE`，清单未声明（未编译故
   未暴露）——工程接线时补。
6. Labels.kt 的 else 分支破坏枚举穷尽性（编译期会暴露，接线时修）。
7. 审查报告 §2.3 P2 的其余 Android 项（`TODO(...)` 服务、fail-silent catch）与上同批处理。
