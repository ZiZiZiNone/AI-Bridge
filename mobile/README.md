# AI-Bridge · 移动端源树骨架

> **状态更新（2026）：iOS 源树已清空（`mobile/ios/` 仅留空文件夹），iOS 实现整体延后、后续有需要再做。本文中所有 iOS 路径与生成步骤为规划存档，不代表当前文件树；`android/` 与 `ffi/` 为现状。**
> 交付范围：原生双移动源树（iOS SwiftUI / Android Compose + 系统扩展 + UniFFI 接口声明）。
> **不含** `xcodeproj` / `pbxproj` / `build.gradle(.kts)` / `settings.gradle(.kts)` / `gradlew` 等工程文件——工程由本地工具链按其规范生成，本目录只提供源码树与生成指引（见 §4–§6）。
> 显示名一律 **AI-Bridge**（项目名与 UI 显示名同一；代码标识 `aibridge`，旧的 "SecureBox" 已作废）。

## 1. 目录结构（本任务交付）

```
mobile/
├─ README.md                     本文件：工程生成方式与构建前置
├─ ios/
│  ├─ app/                        SwiftUI App（薄入口）+ M1–M8 页面 + 组合根
│  │  ├─ AIBridgeApp.swift       @main 入口 + 组合根装配点
│  │  ├─ AppEnvironment.swift    组合根依赖集合（构造注入，无单例）
│  │  ├─ RootView.swift          M2 主屏：底部五 tab（库/生成器/浏览器/设备/设置）
│  │  ├─ Pages/                  M1–M8 页面（PageScaffold + 各页面）
│  │  ├─ WebView/                WKWebView 容器 + 会话搬运（实现 core 的 SessionSource）
│  │  └─ Core/                   宿主 trait 实现（KeychainVaultStore/时钟/熵/局域网传输）
│  ├─ credential-provider/       M9：Credential Provider 扩展（含 Info.plist）
│  └─ safari-ext/                Safari 扩展 handler + 会话桥（含 Info.plist）
├─ android/
│  └─ app/src/main/
│     ├─ AndroidManifest.xml     宿主 + 两个系统服务声明
│     ├─ res/                    strings.xml（app_name=AI-Bridge）· xml/autofill_service.xml
│     └─ java/com/aibridge/mobile/
│        ├─ MainActivity.kt      宿主入口 + 组合根装配点
│        ├─ AppDependencies.kt   组合根依赖集合（构造注入，无单例）
│        ├─ ui/                  RootNav（五 tab）+ PlaceholderScreen + M1–M9 页面空壳
│        ├─ autofill/            M9：AutofillService 骨架
│        ├─ credential/          M9：CredentialProviderService + CredentialManager 桥骨架
│        └─ webview/             Android WebView 容器 + 会话搬运（实现 core 的 SessionSource）
└─ ffi/
   ├─ aibridge.udl               UniFFI 接口声明（= core API 镜像，声明骨架）
   └─ uniffi.toml                绑定生成配置（Kotlin 包名 / Swift 模块名）
```

依赖方向（与 `docs/目录结构.md` 一致）：`contracts/ ← core/ ← mobile/`。`mobile/` 只经 `mobile/ffi` 声明的 core 公开面与契约类型通信，不引 `desktop/`、`extensions/` 的任何代码。

### 路径映射说明（与 `docs/目录结构.md` §3 的差异）

`docs/目录结构.md` 把移动 UI 归为 `mobile/app/`（ios/android 两组）。本任务按交付指令把 UI 层与各平台树同置：SwiftUI App 壳在 `mobile/ios/app/`，Compose 页面壳在 `mobile/android/app/src/main/java/com/aibridge/mobile/ui/`。**同一实现全仓只写一份**，故不再另建 `mobile/app/` 目录（避免重复实现或纯转发层）。若后续需要对齐文档字面路径，移动 `app` 目录即可，代码内部无路径假设。

## 2. 界面清单落点（M1–M9，见 `docs/功能点全景.md` §3.3）

| # | 界面 | iOS | Android |
|---|---|---|---|
| M1 | 锁屏 / 解锁 | `ios/app/Pages/UnlockView.swift` | `.../ui/pages/UnlockScreen.kt` |
| M2 | 库列表 + 搜索（主屏五 tab） | `ios/app/Pages/VaultListView.swift` + `app/RootView.swift` | `.../ui/pages/VaultListScreen.kt` + `ui/RootNav.kt` |
| M3 | 条目详情 + 编辑表单 | `ios/app/Pages/EntryDetailView.swift` | `.../ui/pages/EntryDetailScreen.kt` |
| M4 | 生成器（默认 20 位 · 不存历史） | `ios/app/Pages/GeneratorView.swift` | `.../ui/pages/GeneratorScreen.kt` |
| M5 | 容器浏览器（全屏 WebView） | `ios/app/Pages/ContainerBrowserView.swift` + `ios/app/WebView/` | `.../ui/pages/ContainerBrowserScreen.kt` + `.../webview/` |
| M6 | 状态 / 临时会话 | `ios/app/Pages/SessionStatusView.swift` | `.../ui/pages/SessionStatusScreen.kt` |
| M7 | 配对与设备（含指纹确认屏） | `ios/app/Pages/PairingView.swift` | `.../ui/pages/PairingScreen.kt` |
| M8 | 备份与设置 | `ios/app/Pages/SettingsBackupView.swift` | `.../ui/pages/SettingsBackupScreen.kt` |
| M9 | 系统自动填充 UI（系统层） | `ios/credential-provider/` | `.../autofill/` + `.../credential/` |

文案口径：同步项只写「设备同步（局域网）」，不出现"云同步"；设备仅 Windows / iOS / Android；条目仅账号密码 / passkey / TOTP，不含支付信息与身份分类。

## 3. 依赖注入（工程硬规则落点）

- 组合根唯一：iOS = `AIBridgeApp.init`（`AppEnvironment`），Android = `MainActivity.bootstrapDependencies`（`AppDependencies`）。两者都只做装配，不含业务判断。
- 全仓无全局单例 / 静态门面 / 服务定位器：不设 Swift `static let shared`、不设 Kotlin `object` / `companion object`、不使用 `@EnvironmentObject`、不引入 Application 级依赖容器或 Hilt 类框架。依赖逐层以构造参数传递；服务类（AutofillService 等）用**实例级** `by lazy` 装配，不是全局状态。
- core 的外部能力一律 trait 注入（`VaultStore` / `SessionSource` / `Transport` / `Clock` / `Entropy`），宿主只实现 trait：iOS 见 `ios/WebViewContainer/ContainerSessionBridge.swift`，Android 见 `.../webview/ContainerSessionBridge.kt`。
- 页面空壳（M1–M9）只提供编号锚点与承载功能点，不含业务判断与数据转换；业务规则全在 core。
- 骨架阶段策略：**装配点 fail-fast**（`fatalError` / `TODO`），语义实现点 no-op——不以占位依赖静默启动。

## 4. iOS：工程生成方式与构建前置

前置：

1. **macOS**（Xcode 只能在 macOS 上运行）。
2. **Xcode** 16 或更高（SwiftUI + iOS 18 SDK）。
3. **Apple Developer 账号**：Credential Provider 扩展与 Safari Web Extension 需要真机 + 开发者签名与 provisioning profile；模拟器不提供完整的系统凭据提供方 / Safari 扩展链路，M9 与 E4 只能在真机验收。
4. 真机 iOS 18+。

工程生成（不手写工程文件）：

1. Xcode → File → New → Project → iOS → App：Product Name `AIBridge`，Display Name `AI-Bridge`，Interface = SwiftUI，Minimum Deployment = iOS 18.0。
2. 删除模板生成的 `ContentView.swift` / `AIBridgeApp.swift`，把 `mobile/ios/app/` 全部源码加入 App target（`AIBridgeApp.swift` 即入口）。
3. 新建 target：iOS → Credential Provider Extension，源码指向 `mobile/ios/CredentialProvider/`，Info.plist 使用目录内已备文件；再新建 Safari Web Extension handler target，源码指向 `mobile/ios/SafariWebExtension/`（Web 资源包随该 target 提供，本任务只交付 handler 骨架）。
4. 加入 UniFFI 生成的 Swift 绑定与静态库（§6），并在 target 的 `Build Settings → Framework Search Paths` 指向绑定产物目录。生成物不入库。
5. 运行：真机 → 设置 → 密码 → 密码选项 → 启用 AI-Bridge（Credential Provider）；Safari → 扩展 → 启用 AI-Bridge。

## 5. Android：工程生成方式与构建前置

前置：

1. JDK 17。
2. **Android SDK**（platform-tools、platform 34+、build-tools 34+）与 Android Studio（Ladybug 或更高）。
3. **Gradle** 8.x：使用 Android Studio 本地生成的 Gradle wrapper（`gradlew` 为本地产物，不入库）。
4. 设备 / 模拟器 Android 10+（API 29+）；`CredentialProviderService` 通道需 Android 14+（API 34+）。

工程生成（不手写工程文件）：

1. Android Studio → New Project → Phone and Tablet → Empty Activity (Compose)：Package name `com.aibridge.mobile`，Minimum SDK API 29，Build configuration language = Kotlin DSL，工程根目录指向 `mobile/android/`。
2. 用本目录的 `mobile/android/app/src/` 源码树替换模板 `app/src/`（模板的 `MainActivity.kt`、`ui/theme/`、`AndroidManifest.xml`、`res/` 由本目录对应文件取代；`Theme.AIBridge` 由模板主题改名而来）。
3. 依赖（Compose BOM 统一版本）：`androidx.compose.material3`、`androidx.activity:activity-compose`、`androidx.credentials:credentials`；若需读取含属性的 cookie，追加 `androidx.webkit`。不引入 DI 框架（见 §3）。
4. 加入 UniFFI 生成的 Kotlin 绑定与 JNA/NDK 库（§6）：生成物落到 `app/src/main/java/com/aibridge/mobile/ffi/`，`.so` 落到 `app/src/main/jniLibs/`。生成物不入库。
5. 构建与安装：`gradlew :app:assembleDebug` → `gradlew :app:installDebug`；随后在系统设置 → 密码与账号 → 自动填充服务中选择 AI-Bridge。

## 6. FFI 生成（uniffi-bindgen）

`mobile/ffi/aibridge.udl` 是 **core API 的镜像声明**（含 `contracts/*.schema.json` 全部字段与五个宿主 trait）。绑定为生成物，禁止手改；生成前置是 core 侧存在 `ab-ffi` crate（本任务未实现，故绑定尚未生成）。

```bash
# 前置：core 落地 ab-ffi（cdylib），并使其导出 UniFFI scaffolding —— 本任务不含 Rust 代码，
#       也未修改根 Cargo.toml（其 members = ["core/*"]，mobile/ 不参与 Rust workspace）。

# iOS 绑定（Swift）
uniffi-bindgen generate mobile/ffi/aibridge.udl --language swift --out-dir mobile/ffi/generated/swift

# Android 绑定（Kotlin）
uniffi-bindgen generate mobile/ffi/aibridge.udl --language kotlin --out-dir mobile/ffi/generated/kotlin
```

- `mobile/ffi/uniffi.toml` 与 UDL 同目录，由 uniffi-bindgen 自动拾取（Kotlin 包名 `com.aibridge.mobile.ffi`，Swift 模块名 `aibridge`）——这正是 `MainActivity.kt` / `AppEnvironment.swift` 里 import 的来源。
- 契约字段以 schema 的 snake_case 为准；生成器可能把方法 / 属性名按语言惯例转成 camelCase，但跨端 JSON 载荷由 serde 输出，字段名不变。
- `mobile/ffi/generated/` 属生成物，不入库。

## 7. 未验证项（本机为 Windows，无 Apple / Android 工具链）

以下均**未执行**，不得视为已验证：

1. **未编译**：Swift（需 macOS + Xcode）、Kotlin/Compose（需 Android SDK + Gradle）、UDL 生成（需 `uniffi-bindgen`）。本机未执行任何移动端构建。
2. **UniFFI 语法与生成器版本适配未验证**：`aibridge.udl` 的 trait 接口、带字段错误枚举、`record<K, V>` 映射等写法未经 `uniffi-bindgen` 解析。
3. **FFI 生成物名称未验证**：`AibridgeCore` / `VaultStore` / `SessionSource` / `Entry` 等 Swift/Kotlin 侧符号名按 UniFFI 惯例书写，待生成后校准；`CredentialManagerBridge` 中 `Entry` 的字段取舍也待 core 模型落地。
4. **平台集成未验证**：两个 iOS `Info.plist`（扩展点标识、PrincipalClass）与 `AndroidManifest.xml`（两个服务 + 权限 + autofill 配置资源）未经真实构建/安装校验；`res/xml` 能力声明按 `androidx.credentials` 所引版本补齐。
5. **界面与交互未验证**：M1–M9 均为空壳，无真机视觉与交互验收；文案口径仅做了源码级检查。
6. **端到端流程未验证**：M-F1–M-F5（解锁 / 容器扩散 / 系统填充 / 扫码配对 / 一键跳转带态）依赖 core 与绑定落地后方可联调。
7. **App 装配为 fail-fast 占位**：`AIBridgeApp.init` 与 `MainActivity.bootstrapDependencies` 在 core 绑定与宿主适配层落地前会立即报错，属预期。
