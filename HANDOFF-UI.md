# 交接单：桌面端 UI 接线（前端 → tauri command）

> **交接对象**：接手本仓库的 AI / 开发者
> **基线**：commit `dd370b7`（工作区含未提交改动，见文末「交接前状态」）
> **生成时间**：2026-10-16
> **配套文档**：`docs/缺陷报告-2026-10-15.md`（症状与证据，本单是它的施工版）

---

## 0. 交付物一句话

**把前端接通后端。** 后端 44 个 tauri command 已注册、已单测、已跑通；前端的 `invoke(` 出现 **0 次**。你的任务是让 8 个面板的按钮真正调起来。

不改后端。不重写 UI。只加那层「按钮 → invoke → core → 回填」的接线。

---

## 1. 先确认你接手的现状（别重复排查，这些已实测）

### 已经能用的（不要动）

| 项 | 实测证据 |
|---|---|
| 真实 Tauri 窗口能开、能渲染 | `aibridge://panel/library` → 窗口 776×619，标题 `AI-Bridge 库`，内容正确 |
| Tauri IPC 通道通 | 真实窗口内 `__TAURI_INTERNALS__.invoke('vault_is_unlocked')` **返回 `false`（成功）** |
| 后端 44 个 command 已注册 | `desktop/src-tauri/src/main.rs:55` 的 `generate_handler!` 块 |
| 后端有测试覆盖 | `cargo test --manifest-path desktop/src-tauri/Cargo.toml` 通过 |
| 窗口控制 | `置顶`/`关闭` 真实窗口内有效（`WS_EX_TOPMOST` 已验） |
| 4 个向导的步骤导航 | 逐级 `active` 递进、末步变「完成」、`上一步` 正确回退 |
| 13 个窗口的布局 | 按 `WINDOW_SPECS` 真实尺寸渲染，零溢出零裁切 |

### 不能用的（你的活儿）

| 症状 | 位置 | 说明 |
|---|---|---|
| 10 个 `<button>` 无 `@click` | 见 §3 清单 | 结构性缺失 |
| 3 个面板 `emit('retry')` 无人接收 | `StatusPanel.vue:14`、`ConverterPanel.vue:13`、`TemporarySessionPanel.vue:14` | `main.ts` 挂载时没挂监听 |
| 挂载点不传 props | `desktop/src/main.ts:26` | `createApp(view).mount(mountPoint)` — 第二个参数没给 |
| 前端 0 处 invoke | 整个 `desktop/src/` | 根本没接线 |

### 一条命令自证

```powershell
cd C:\Users\HKX\Desktop\AI-Bridge\desktop\src
Get-ChildItem -Recurse -File -Include *.vue,*.ts | Select-String -Pattern '\binvoke\s*\('
# 预期：无输出。出现任何一行都说明有人开始接线了。
```

---

## 2. 验收标准（怎么算做完）

做完后必须能**用一条命令证明**，不接受「代码看起来对了」：

```powershell
# 1. 起 dev server
cd C:\Users\HKX\Desktop\AI-Bridge\desktop
npm run dev

# 2. 浏览器打开（必须带 ?window= 参数，否则只显示「未知窗口标签：(空)」）
#    http://localhost:1420/?window=panel-library

# 3. 点「新建条目」→ 必须弹出表单；填完提交 → 列表出现该条目
# 4. 刷新页面 → 条目还在（证明真的写进了 vault，不是前端内存假象）
```

**硬性判据**：

1. `desktop/src/` 内 `invoke(` 调用点数 **> 0**，且覆盖 8 个面板的全部业务按钮
2. `npm run check` 仍全绿（`check:structure` 8/8 + `check:render` 15/15）
3. `npm run typecheck` 退出码 0
4. `cargo test --manifest-path desktop/src-tauri/Cargo.toml` 仍通过（你没改后端，应当不变）
5. **新增**死按钮检测（见 §5），且该检测在「故意删掉一个 `@click`」时能失败

第 5 条是防回归的关键：不加它，你修完这一轮，下一轮还会有人交出点不动的界面。

---

## 3. 施工路径（按序做，别跳）

### 第 1 步：建 invoke 封装层（新增 1 个文件）

按项目规范（`common/principles.md`「单向依赖原则」：`Page/Component → Hook → Logic → Service`），不要在 `.vue` 里直接调 `invoke`。新建：

```
desktop/src/hooks/useVault.ts        # 或按面板建 useLibrary.ts / useBackup.ts ...
```

职责：包 `invoke`、把 `CmdResult<T>` 的 `Result` 映射成 `UIState` 五态（`idle|loading|success|error|empty`，定义在 `desktop/src/types/ui.ts`）、统一错误文案。

参考同目录已存在的 `desktop/src/hooks/useWindowControls.ts`（27 行）——那就是本项目的 hook 写法基线。

### 第 2 步：打通 W3（库管理）——这是模板，其余照抄

W3 是最合适的第一个目标：3 个按钮、命令已注册、无外部依赖。

需要的命令（`desktop/src-tauri/src/commands/library.rs:387-456` 已定义）：

| 前端动作 | command | 参数 | 返回 |
|---|---|---|---|
| 判断库是否已解锁 | `vault_is_unlocked` | 无 | `bool` |
| 建库 | `vault_create` | `{ name: string, password: string }` | `InitView` |
| 解锁 | `vault_unlock` | `{ password: string }` | `null` |
| 锁定 | `vault_lock` | 无 | `null` |
| 列条目 | `vault_list_entries` | 无 | `EntryView[]` |
| 新增条目 | `vault_add_entry` | `{ input: EntryInput }` | `EntryView` |

**注意嵌套参数的写法**：`vault_add_entry` 的参数名是 `input`，Tauri 会做 camelCase→snake_case 转换，调用写成：

```ts
await invoke('vault_add_entry', { input: { title, account_ref, kind, ... } });
```

`EntryInput` 形状（`library.rs:68-84` 定义，全部 `snake_case`）：

```ts
interface EntryInput {
  title: string;            // 必填
  account_ref: string;      // 必填
  kind: 'password' | 'passkey' | 'totp';  // 必填
  username?: string;
  secret?: string;
  totp_seed?: string;
  url?: string;
  note?: string;
}
```

`EntryView`（列表用，`library.rs:18-29`）——**注意它不含 secret**：

```ts
interface EntryView {
  uuid: string;
  title: string;
  account_ref: string;
  kind: CredentialKind;
  username?: string;
  url?: string;
  has_secret: boolean;   // 只给「有没有」，不给值
  has_totp: boolean;
}
```

要看明文要用 `vault_get_entry`（返回 `EntryDetail`，含 `secret`/`totp_seed`）。**列表页不要取明文**——这是项目的 S6 明文纪律，不是可选项。

### 第 3 步：给 `main.ts` 挂上 props 与监听

`desktop/src/main.ts:26` 现在是：

```ts
createApp(view).mount(mountPoint);
```

要改成给组件传 props + 挂 `@retry`。**建议的实现方式**：不要让 `main.ts` 变厚（它是「薄入口」，见 `desktop/src/main.ts` 文件头注释），而是引入一个每面板的容器组件（`Page` 层），由容器持有 hook 与状态，把数据以 props 下传、把 `retry` 事件接住。

即：`window-registry.ts` 的 `WINDOW_VIEWS` 从「标签 → 面板」改为「标签 → 容器」，容器内部才渲染面板。这样面板本身仍是纯受控视图（它现在的 `props` / `emit` 设计是对的，别改面板的接口）。

### 第 4 步：其余 7 个面板照抄

命令映射见 §4。每个面板 30–60 行容器代码。

### 第 5 步：补门禁（§5）

---

## 4. 命令清单（44 个，按面板分组）

全部在 `desktop/src-tauri/src/main.rs:55` 起注册。`state: tauri::State<'_, HostState>` 是注入的，前端不用传。

**W3 库管理** — `library.rs`
`vault_create` `vault_unlock` `vault_lock` `vault_is_unlocked` `vault_list_entries` `vault_list_trash` `vault_get_entry` `vault_add_entry` `vault_update_entry` `vault_trash_entry` `vault_restore_entry` `vault_purge_trash`

**W4 状态清单** — `state.rs`
`state_matrix` `state_health` `state_snapshots` `state_rollback` `state_capture`

**W5 临时会话** — `sessions.rs`
`session_issue` `session_list` `session_renew` `session_release` `session_revoke` `session_revoke_all` `session_sweep`
（`session_revoke_all` 返回 `usize` = 吊销条数，可直接做「一键吊销全部」的成功提示）

**W6 审计** — `audit.rs`
`audit_query`（参数 `{ filter: AuditFilterDto }`）`audit_count`

**W7 备份** — `backup.rs`
`backup_run`（`{ password?: string }`）`backup_status` `backup_drill` `backup_restore_with_password` `backup_restore_with_recovery`

**W8 转换器** — `converters.rs`
`converter_list` `converter_install`

**W9 配对** — `pairing.rs`
`pairing_offer` `pairing_review` `pairing_confirm` `pairing_devices` `pairing_unbind` `sync_transfer` `sync_export_pack` `sync_import_pack`

**W2 候选条** — `candidate.rs`
`candidate_data`（`{ domain: string }`）

**W10 设置** — 无对应命令。
面板底部文案写「设置项将在 core 暴露配置接口后接入」（`SettingsPanel.vue:16`），**这是实情，不是偷懒**。不要为它造前端假配置。

**MCP** — `mcp.rs`：`mcp_start_http` `mcp_status`（非面板按钮，属 C1 通道）

**窗口管理** — `ui::window`：`open_window` `close_window` `toggle_window`（已由 `useWindowControls.ts` 使用）

---

## 5. 必做：给 `check:render` 加死按钮检测

现状：`desktop/scripts/render-check.mjs:129` 这类断言只检查**文案存在**：

```js
['src/ui/panels/LibraryPanel.vue', 'W3 库管理', ['AI-Bridge', '还没有条目']],
```

它用 SSR 渲染成字符串就通过，**从不点击**。所以骨架能拿 15/15。`check:structure` 同理（查结构、标签表、字段一致性，不碰交互）。

这两个检查被 `tools/verify-all.ps1:87-88` 收进全仓终验，于是「20/20 通过」与「界面完全点不动」可以同时成立。

**要求**：在 `render-check.mjs` 里增加断言——遍历 `desktop/src/ui/panels/*.vue` 的模板，凡 `<button>` 无 `@click`（且不属于 `WindowShell` / `StateBlock` 这类纯窗口控制组件）即判 FAIL。再加一条：凡组件声明了 `defineEmits`，要求 `main.ts` 侧存在对应监听。

**自测该检测有效**：故意删掉 `LibraryPanel.vue:9` 的 `@click`，检测必须 FAIL；恢复后必须 PASS。

---

## 6. 构建 / 运行 / 验证命令（本机已验，照抄）

### 环境（cargo 不在 PATH，必须注入）

```powershell
$env:PATH = "C:\Users\HKX\AppData\Local\puccinialin\puccinialin\Cache\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin;$env:PATH"
$env:CARGO_HOME = "C:\Users\HKX\Desktop\AI-Bridge\.cargo-home"   # 离线 vendored 依赖，2964 crates
```

### 前端

```powershell
cd C:\Users\HKX\Desktop\AI-Bridge\desktop
npm run dev              # dev server → http://localhost:1420
npm run typecheck        # vue-tsc --noEmit
npm run check            # check:structure + check:render
npm run build            # typecheck + vite build → desktop/dist/
```

### 桌面端（生产构建必须带 custom-protocol）

```powershell
cargo build --manifest-path desktop/src-tauri/Cargo.toml --offline --features custom-protocol
```

**`--features custom-protocol` 不能省。** 没有它，产物走 dev 模式去连 `http://localhost:1420`；dev server 不在时窗口显示 `ERR_CONNECTION_REFUSED`。

> **前提条件（2026-10-16 核实）**：`desktop/src-tauri/Cargo.toml` 当前**没有** `[features]` 段，
> 即该特性缺失。此时上面的命令会直接报错（cargo 找不到该 feature）。
> 先补回这一段（Tauri 2 标准写法），再执行构建：
>
> ```toml
> [features]
> # Tauri 2 标准特性开关：决定产物走「内嵌前端资源」还是「连 dev server」。
> # 生产构建必须带此特性，否则窗口加载 devUrl。
> custom-protocol = ["tauri/custom-protocol"]
> ```
>
> 补完后 `git status` 会把 `desktop/src-tauri/Cargo.toml` 标为已修改，属预期。

**改了 `dist/` 后必须让 build.rs 重跑**，否则 exe 不会重新内嵌前端资源（`tauri::generate_context!` 在编译期内嵌并 brotli 压缩）：

```powershell
(Get-Item desktop\src-tauri\build.rs).LastWriteTime = Get-Date
cargo build --manifest-path desktop\src-tauri\Cargo.toml --offline --features custom-protocol
```

不 touch build.rs 时 `cargo build` 只跑 1.97 秒、exe hash 不变（已实测）；touch 后 29 秒并更新 exe。

### 运行

```powershell
desktop\src-tauri\target\debug\ab-desktop.exe
```

**启动后不会有任何窗口** —— `desktop/src-tauri/tauri.conf.json` 的 `"windows": []`，这是设计（托盘常驻，窗口按需创建）。开窗口三种方式：

1. 托盘图标右键 → 菜单（`MENU_ITEMS` 11 项）
2. 命令行带 URI：`ab-desktop.exe aibridge://panel/library`
3. 已注册协议：地址栏 `aibridge://panel/library`

URI 短名：`candidate`（→候选条，也接受 `panel/candidate`）、`pair`（→配对向导）、`panel/<short>`，其中 short ∈ `library status sessions audit backup converters pairing settings`。

### 全仓终验

```powershell
powershell -File tools\verify-all.ps1                 # 预期 20 项 20 通过
powershell -File tools\verify-all.ps1 -Exclude desktop  # 改 desktop 时可排除
```

---

## 7. 已知坑（会浪费你时间的）

| 坑 | 现象 | 处理 |
|---|---|---|
| **WebView2 用户数据目录被占** | `failed to create webview: WebView2 error: HRESULT(0x800700AA) 请求的资源在使用中` | 已有实例在跑。杀掉所有 `ab-desktop.exe`，或设 `WEBVIEW2_USER_DATA_FOLDER` 指向独立目录 |
| **没有单实例机制** | 可以同时起多个 `ab-desktop.exe` | 别假设有锁；测试前先 `Get-Process ab-desktop \| Stop-Process -Force` |
| **裸 URL 显示「未知窗口标签：(空)」** | 不是 bug | 必须带 `?window=<label>`。15 个标签见 `window-registry.ts` |
| **dev 模式点「置顶/关闭」抛异常** | `TypeError: Cannot read properties of undefined (reading 'metadata')` at `@tauri-apps/api/window.js:593` | 纯浏览器无 `__TAURI_INTERNALS__`。**真实 Tauri 窗口内正常**。若要在 dev 里试界面，需自行短路 |
| **生产窗口 favicon** | dev server 下 `/favicon.ico` **404**（已实测）；生产窗口 **200**（已实测） | 想在 dev 里消掉 404 就加 `desktop/public/favicon.ico` |
| **协议登记报 0x80070005** | `AI-Bridge 协议登记未完成：协议键创建失败（错误码 0x80070005）` | HKCU 写入被拒（沙盒/权限），**非致命**，应用继续运行。真实用户环境通常无此问题 |
| **PowerShell 读写 UTF-8 中文** | `Get-Content`/`Set-Content` 往返会损坏中文 | 用 `[System.IO.File]::WriteAllText($p,$s,[System.Text.UTF8Encoding]::new($false))` 或 Node 的 `fs.writeFileSync(..., 'utf8')` |
| **lockfile 被 cargo 改写** | `desktop/src-tauri/Cargo.lock`、`tests/e2e-pipeline/Cargo.lock` 出现无关 diff | 提交前 `git checkout --` 还原 |

---

## 8. 禁止事项

1. **不要改后端命令签名**。前端适配后端，不是反过来——后端已有测试锁定契约。
2. **不要在 `.vue` 里直接 `invoke`**。走 hooks 层（§3 第 1 步），这是项目规范 `common/principles.md`「单向依赖原则」的要求。
3. **不要改面板现有的 props / emit 接口**。`StatusPanel.vue` 等的「受控视图」设计是对的，缺的是外面的容器。改面板接口会让问题从「没接线」变成「接错线」。
4. **不要在列表页取明文**。`EntryView` 故意不含 `secret`（只有 `has_secret`），这是 S6 明文纪律。需要明文用 `vault_get_entry` 单条取。
5. **不要为 W10 设置面板造假配置**。core 没有配置接口，面板文案已如实说明。
6. **不要为了「让检查通过」而放宽检查**。要加严（§5），不是绕过。
7. **不要顺手重构无关文件**。一次只做接线。

---

## 9. 交接前状态（工作区）

未提交改动（2026-10-16 清理后的 `git status --short`）：

```
D  .ab-git-log.txt
D  .ab-pre-broker.rs
D  .ab-pre-commands-mod.rs
D  .ab-pre-contract.rs
D  .ab-pre-host.rs
D  .ab-vendor-list.txt
D  ai-bridge-full-context.zip
A  USAGE.md
M  core/broker/src/lib.rs
M  desktop/src-tauri/src/commands/candidate.rs
M  desktop/src-tauri/src/commands/sessions.rs
M  desktop/src-tauri/src/main.rs
M  desktop/src-tauri/src/native/hello.rs
M  desktop/src-tauri/src/native/protocol.rs
?? HANDOFF-PROMPT.md
?? HANDOFF-UI.md
?? desktop/scripts/check-ui-acceptance.mjs
```

**保留项**：

- `desktop/src-tauri/src/main.rs` 里 `dispatch_protocol` 的三处 `eprintln!` —— 把原本被 `let _ =` 吞掉的窗口打开失败暴露出来，排障必需
- 已 staged 的 6 个源码修改是上一轮修复（测试夹具与 broker 语法），不是本次接线产生

**注意（2026-10-16 复核）**：`desktop/src-tauri/Cargo.toml` 的 `custom-protocol` 特性**已不在工作区**
（工作区与 HEAD 一致），但生产构建依赖它 —— 见 §6 的前置条件，构建前需补回。

**已清理的垃圾**（上一任留下，勿再制造）：6 个 `.ab-*` 文件（`.ab-pre-*.rs` 是源文件的
重构前旧副本，比当前源码还大，最易误导）、3.6 GB 临时构建目录与浏览器用户数据目录。
详见 `HANDOFF-PROMPT.md` 的「绝对禁止制造垃圾」一节。

---

## 10. 不确定时怎么办

- 命令的参数/返回形状：看 `desktop/src-tauri/src/commands/<面板>.rs` 的 `#[tauri::command]` 包装函数，签名即契约
- 前端类型：`desktop/src/types/contract.ts`（镜像 `contracts/*.schema.json`，由 `check:structure` 逐字段校验）
- 界面目标形态：`docs/功能点全景.md` §3.1 的 W1–W15
- 症状与完整实测证据：`docs/缺陷报告-2026-10-15.md`
- **不要凭记忆猜字段名**。本仓库契约由脚本校验，猜错会直接被 `check:structure` 拦下。

---

*本单为只读诊断的产物，未修改任何源码。所有位置引用基于 commit `dd370b7`；文件一旦被改动，行号需重新核对。*
