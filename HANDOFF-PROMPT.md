# 目标模式交接提示词

> 用法：把下面 `---` 之间的全文整段粘贴给接手的 AI（目标模式 / goal mode）。
> 提示词自带验收标准与约束，接手方无需额外追问即可开工。
> 配套文件：`HANDOFF-UI.md`（施工单）、`docs/缺陷报告-2026-10-15.md`（证据）。

---

## 目标

把 `C:\Users\HKX\Desktop\AI-Bridge` 桌面端的前端接通后端：让 8 个面板的业务按钮真正调用已注册的 tauri command。

判据不是"代码写对了"，而是**验收脚本退出码为 0**。

## 你的约束（先读这条，它决定你怎么验证）

**你没有视觉能力。不要请求或依赖截图**——仓库里的 PNG 对你没有意义。

你的唯一验收手段是文本断言：

```powershell
cd C:\Users\HKX\Desktop\AI-Bridge\desktop
npm run dev                 # 另开一个终端常驻（后台运行）
node scripts/check-ui-acceptance.mjs
```

这个脚本输出 PASS/FAIL 文本行，**退出码 0 = 通过，1 = 有失败**。它会真的用浏览器逐个点击每一个按钮，检测"点了没反应"。不用它，你就无法证明自己修好了。

## 第一步：建立基线（不要跳过，不要凭猜测开工）

1. 读 `HANDOFF-UI.md` —— 施工单：命令清单、payload 形状、禁止事项、已知坑
2. 读 `docs/缺陷报告-2026-10-15.md` —— 症状、证据、精确行号
3. 起 `npm run dev`，跑 `node scripts/check-ui-acceptance.mjs`，**确认当前有 3 项 FAIL**：

```
FAIL  A1 前端存在 invoke 调用  — 0 处
FAIL  A2 所有 button 都绑定了 @click  — 10/26 个死按钮
FAIL  B3 无静默失效按钮  — 13 个按钮点了无任何反应
```

这 3 项就是你要消灭的目标。跑不出这个基线，先解决环境问题，别开始改代码。

## 已实测的现状（不要重复排查，这些已经验过）

- 后端 **44 个** tauri command 已注册（`desktop/src-tauri/src/main.rs:55` 的 `generate_handler!` 块），且有测试覆盖
- **Tauri IPC 通道是通的**：在生产窗口内直接执行 `invoke('vault_is_unlocked')` **成功返回 `false`**
- 前端 `desktop/src/` 内 `invoke(` 出现 **0 次** —— 这是根因
- 13 个静默按钮的构成：10 个 `<button>` 完全没有 `@click`，另 3 个面板 `emit('retry')` 但无人接收（`main.ts:26` 挂载时既没传 props 也没挂监听）
- 真实 Tauri 窗口渲染正常、13 个窗口布局零溢出、4 个向导的步骤切换正常

**结论：通道是通的，只是前端一个都没调。你的工作是把水管接上龙头。**

## 验收标准（全部满足才算完成）

1. `node scripts/check-ui-acceptance.mjs` **退出码 0**（8 项全 PASS）
2. `npm run typecheck` 退出码 0
3. `npm run check` 全绿（`check:structure` 8/8 + `check:render` 15/15）
4. `cargo test --manifest-path desktop/src-tauri/Cargo.toml` 仍通过（你没改后端，应当不变）
5. 端到端：在**真实 Tauri 窗口**内点「新建条目」能建条目，刷新后条目仍在

前 4 条是自动化的，第 5 条按 `HANDOFF-UI.md` §6 的命令构建运行：

```powershell
$env:PATH = "C:\Users\HKX\AppData\Local\puccinialin\puccinialin\Cache\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin;$env:PATH"
$env:CARGO_HOME = "C:\Users\HKX\Desktop\AI-Bridge\.cargo-home"
cargo build --manifest-path desktop/src-tauri/Cargo.toml --offline --features custom-protocol
# 启动后没有窗口（设计如此），用 URI 唤起：
desktop\src-tauri\target\debug\ab-desktop.exe aibridge://panel/library
```

> **动手前先确认一条**：`desktop/src-tauri/Cargo.toml` 目前**没有** `[features]` 段，
> 即 `custom-protocol` 特性缺失（2026-10-16 核实，工作区与 HEAD 一致）。
> 缺它时上面的 `--features custom-protocol` 会直接报错，且不带该特性构建出的 exe 会去连
> `http://localhost:1420`，dev server 不在时窗口显示 `ERR_CONNECTION_REFUSED`。
> 补回这一段即可（这是 Tauri 2 标准写法）：
>
> ```toml
> [features]
> # Tauri 2 标准特性开关：决定产物走「内嵌前端资源」还是「连 dev server」。
> # 生产构建必须带此特性，否则窗口加载 devUrl。
> custom-protocol = ["tauri/custom-protocol"]
> ```
>
> 补完后 `git status` 会把 `desktop/src-tauri/Cargo.toml` 显示为已修改，这是预期的。

## 施工顺序

**第 1 步 — hooks 层（新增，不放在 .vue 里）**

在 `desktop/src/hooks/` 下新增（参照已有的 `useWindowControls.ts`，27 行，那是本项目 hook 写法基线）。职责：包 `invoke`、把 `CmdResult<T>` 映射成 `idle|loading|success|error|empty` 五态（类型在 `desktop/src/types/ui.ts`）、统一错误文案。

**第 2 步 — 打通 W3 库管理（这是模板，其余照抄）**

命令（已定义在 `desktop/src-tauri/src/commands/library.rs:387-456`）：

| 动作 | command | 参数 | 返回 |
|---|---|---|---|
| 判断是否已解锁 | `vault_is_unlocked` | 无 | `bool` |
| 建库 | `vault_create` | `{ name: string, password: string }` | `InitView` |
| 解锁 | `vault_unlock` | `{ password: string }` | `null` |
| 锁定 | `vault_lock` | 无 | `null` |
| 列条目 | `vault_list_entries` | 无 | `EntryView[]` |
| 新增条目 | `vault_add_entry` | `{ input: EntryInput }` | `EntryView` |

注意两处易错点：
- `vault_add_entry` 的参数名是 **`input`（嵌套一层）**，写成 `invoke('vault_add_entry', { input: {...} })`
- `EntryView` **故意不含 secret**（只有 `has_secret: boolean`）。列表页禁止取明文，要看明文用 `vault_get_entry` 单条取。这是项目的 S6 明文纪律，不是可选项。

`EntryInput` 形状（`library.rs:68-84`，全 snake_case）：

```ts
{ title: string; account_ref: string; kind: 'password'|'passkey'|'totp';
  username?: string; secret?: string; totp_seed?: string; url?: string; note?: string }
```

**第 3 步 — 给挂载点接上 props 与监听**

`desktop/src/main.ts:26` 现在是 `createApp(view).mount(mountPoint)` —— 既没传 props 也没挂监听。

推荐做法：引入**每面板的容器组件（Page 层）**，由容器持有 hook 与状态，把数据 props 下传、把 `retry` 事件接住；`window-registry.ts` 从"标签 → 面板"改为"标签 → 容器"。

**不要把 main.ts 撑厚** —— 它的文件头明确规定自己是薄入口。

**第 4 步 — 其余 7 个面板照抄**，命令映射见 `HANDOFF-UI.md` §4。

**第 5 步 — 每个面板做完就跑一次验收脚本**，B3 的静默按钮数必须单调下降，做完应为 0。

## 明确不做（后端没有对应命令，如实降级，禁止造假）

| 界面 | 缺失的能力 | 后端现状 |
|---|---|---|
| W6 审计「加密导出」 | 导出审计流 | `audit.rs` 只有 `audit_query`/`audit_count` |
| W10 设置全部项 | 配置读写 | **一个命令都没有** |
| W3 生成器 / 检索 / 体检 | A5 / A1.2 / A1.3 | `library.rs:6-7` 注释已写明 `ab-vault` 无公开 API |
| W8 转换器「启用开关」 | 启用/停用 | 只有 `converter_list`/`converter_install`，无 toggle |
| W9 配对 QR 渲染 | 二维码图形 | 项目依赖只有 `vue` + `@tauri-apps/api`，无 QR 库 |

这些按钮的正确处理：**保留现状，让文案如实说明未接入**。写前端内存假实现让检查变绿，是本任务最严重的失败模式——本仓库已经因此交付过一次"20/20 全绿但点不动"的骨架。

**需要新增依赖（如 QR 库）或改后端时，停下来问，不要自行决定。**

## 绝对禁止

1. 不要修改后端命令签名（前端适配后端，不是反过来；后端已有测试锁定契约）
2. 不要修改面板现有的 props / emit 接口 —— 它们的设计是对的，缺的是外层容器
3. 不要在 `.vue` 里直接 `invoke` —— 走 hooks 层
4. 不要放宽、绕过或"简化" `check-ui-acceptance.mjs` 的判定逻辑
5. 不要为没有后端支持的界面造假实现
6. 不要顺手重构无关文件（一次只做接线）
7. 不要凭记忆猜字段名 —— 契约由 `check:structure` 逐字段校验，猜错会被拦下

## 已知坑（会浪费你时间，逐条照做）

| 坑 | 处理 |
|---|---|
| 构建不带 `--features custom-protocol` | 产物会去连 `http://localhost:1420`，dev server 不在时窗口 `ERR_CONNECTION_REFUSED` |
| 改了 `dist/` 但 exe 没变 | 必须 touch `desktop/src-tauri/build.rs` 才会重新内嵌前端资源。实测：不 touch 时 `cargo build` 只跑 1.97 秒且 exe hash 不变 |
| `WebView2 error: 0x800700AA 请求的资源在使用中` | 残留实例锁住了用户数据目录。先 `Get-Process ab-desktop \| Stop-Process -Force` |
| 应用启动后没有窗口 | 设计如此（`tauri.conf.json` 的 `"windows": []`，托盘常驻）。用 `ab-desktop.exe aibridge://panel/library` 唤起，或用托盘菜单 |
| `http://127.0.0.1:1420` 连不上 | dev server 只监听 IPv6。用 `http://localhost:1420` |
| 没有单实例机制 | 可以同时起多个 `ab-desktop.exe`，测试前先清干净 |
| `cargo` 不在 PATH | 需注入工具链路径与 `CARGO_HOME`，命令见上文验收标准第 5 条 |
| PowerShell 读写 UTF-8 中文会损坏 | 用 `[System.IO.File]::WriteAllText($p,$s,[System.Text.UTF8Encoding]::new($false))`，或改用 Node 的 `fs.writeFileSync(..., 'utf8')` |
| `desktop/src-tauri/Cargo.lock` 出现无关 diff | cargo 自动改写，提交前 `git checkout --` 还原 |

## 绝对禁止制造垃圾（上一任就是这么把仓库搞脏的）

这个仓库刚被清理过一次，删掉了 **3.6 GB** 临时产物和 6 个误导性文件。**不要再制造。**

### 你的产物必须只有这两类

1. **接线代码** —— `desktop/src/**`、必要的 `desktop/src-tauri/**`
2. **验收证据** —— 验收脚本的输出文本

其它一律不留。

### 禁止在仓库内落这些

| 禁止 | 为什么 | 正确做法 |
|---|---|---|
| 探针/诊断脚本（`probe*.mjs`、`*-test.ps1`、`diag.ps1`…） | 一次性用完即废，留在仓库里污染 `git status` | 要么用完就删，要么写到系统临时目录 |
| 浏览器 / WebView2 用户数据目录 | 单个 55–95 MB，且含大量二进制缓存 | 用 `os.tmpdir()`；**不要**放在 `desktop/` 或仓库任何位置 |
| 构建到仓库内的临时 `CARGO_TARGET_DIR` | 上一任留下 3.4 GB | 用系统临时目录，或用默认 `target/` 并接受 exe 被占用 |
| 日志文件（`*.log`、`*-err.txt`、`*-out.txt`） | 排障残留，无人再读 | 需要就重定向到临时目录 |
| **源文件的"改前备份"副本** | 最危险：上一任留下 `.ab-pre-broker.rs`（93 KB，重构前的旧版本），比当前源码还大。新模型极可能把它当源码读，拿到过时代码 | 用 `git diff` / `git stash`，**不要**手抄副本 |
| 上下文打包（`*-full-context.zip`、vendor 列表、git log 转储） | 一次性交接材料，之后纯属体积 | 交接用文档写清，不要塞进仓库 |
| 截图 PNG | 无视觉的接手方读不了，只是占位 | 用文本断言替代（见 `check-ui-acceptance.mjs`） |

### 每完成一步就自检

```powershell
cd C:\Users\HKX\Desktop\AI-Bridge
git status --short
```

**期望只看到你实际改动的源码文件。** 出现下列任何一种，立刻清理，不要留到"最后一并处理"：

- 未跟踪的 `.mjs` / `.ps1` / `.log` / `.txt` / `.png`
- 任何 `-profile` / `-user-data` / `wv2*` / `appdata*` 目录
- `.ab-*` 或任何带"备份"含义的文件名
- 体积异常的目录（用 `Get-ChildItem target -Directory | ...` 看大小，单项超过 100 MB 就要怀疑）

### 提交前

```powershell
# 仓库总体积自查：不该有肉眼可见的膨胀
Get-ChildItem C:\Users\HKX\Desktop\AI-Bridge\target -Directory -Force |
  ForEach-Object { "{0,-20} {1,10:N2} MB" -f $_.Name,
    ((Get-ChildItem $_.FullName -Recurse -Force -File -ErrorAction SilentlyContinue |
      Measure-Object Length -Sum).Sum / 1MB) }
```

`target/` 是 gitignored 的**正常构建缓存**（`target/debug` 约 7.6 GB 属正常，别删它）。
但**你自己新增的**子目录（`desktop-prod`、`wv2-*`、`test-data-dir` 之类）在验收完成后必须删掉。

## 遇到不确定就停下来问

以下情况**不要自行决定**，先报告再等指示：

- 需要给项目新增依赖（如 QR 库）
- 需要修改后端（补命令 / 改 core）
- 发现验收标准之间有冲突
- 某个界面按设计无法在当前后端支持下完成

## 完成后的报告格式

给出以下内容，缺一不可：

1. `node scripts/check-ui-acceptance.mjs` 的**完整输出**（含退出码）
2. `npm run typecheck`、`npm run check`、`cargo test` 的退出码
3. 逐个面板说明：接了哪些按钮、调用哪些 command
4. 按上文降级了哪些按钮，以及降级理由
5. 未验证项显式声明（跑了但没验的，说没验）
6. **`git status --short` 的完整输出**（证明没留下垃圾），以及 `target/` 各目录大小

---

*提示词版本：2026-10-16，基线 commit `dd370b7`。文件一旦被改动，其中的行号需重新核对。*
