# AI-Bridge 转换器 SDK 与开发规范

> 适用范围：`converters/` 下的全部转换器（官方 `qq/`、模板 `_template/`、后续新接入）。
> 唯一事实源：`contracts/` 三个 schema。本文件只写「schema 之外的约定」；与 schema 冲突时以 schema 为准。
> 本文件不改 `core/`：转换器是「放入即接入」的数据产物适配器，新增/删除转换器时核心零改动。

## 0. 目录与模块

```
converters/
├─ sdk/
│  ├─ README.md                  ← 本文：开发规范（唯一一份）
│  ├─ abi.mjs                    ABI 常量 + 指针/长度编解码（不实现任何结构校验）
│  ├─ host-shim.mjs              本地宿主垫片：白名单只读沙盒 + ABI 驱动（供自测）
│  └─ tests/
│     ├─ conformance.mjs         行为一致性检查：ABI / 沙盒 / 职责分界 / golden / 义务 / 脱敏 lint
│     └─ schema-check.py         契约 schema 校验：清单 + 期望产物（jsonschema 权威库）
└─ <id>/                         一个转换器 = 一个目录
   ├─ manifest.json              contracts/converter-manifest.schema.json
   ├─ converter.mjs              参考入口（对应 manifest.entry = converter.wasm 的导出）
   ├─ extractor.mjs              取数（唯一允许做 IO 的环节）
   ├─ normalizer.mjs             归一化（纯函数，→ StandardAccess）
   └─ tests/fixtures/            脱敏夹具（raw/ + expected/）
```

## 1. 边界与依赖（硬规则）

| 规则 | 说明 |
|---|---|
| 单向依赖 | `converters/ → sdk/ + contracts/`；**永不**依赖 `core/`、`desktop/`、`mobile/`、`extensions/` 的实现代码 |
| 无网络 | 宿主不提供任何网络导入；转换器不得假设有网 |
| 只读 | 宿主只提供只读能力，且只对 `manifest.readable_paths` 声明的路径开放；未声明路径一律拒绝 |
| 无自取时间 | 时间由宿主注入（请求里的 `now`）；转换器不得自行取当前时间，否则产物不可复现 |
| 一件规则一份 | 契约结构校验的唯一实现在 `ab-contract`；SDK 与转换器不得再写一套字段校验 |
| 不写纯转发层 | `converter.mjs` 只做 ABI 翻译；不做判断、不搬字段。若某转换器没有自己的取数/归一化逻辑，它就不该存在 |

`_template/converter.mjs` 是「ABI 翻译层」的示范：JSON 进出 + 调用 `extract` / `normalize`，全部业务语义在另外两个文件里。

## 2. 生命周期

```
load(manifest) → verify(signature) → 每次采集：新建沙盒实例 → ab_run(请求 JSON) → 校验 StandardAccess → 入库/扩散
```

- 加载：读 `manifest.json`，校验 schema；枚举 wasm import 段，出现白名单外的导入即拒绝加载。
- 验签：`signature` 校验通过方可启用（未通过不得执行任何取数）。
- 采集：宿主为**每一次采集**新建实例，杜绝跨次状态残留；转换器不得依赖进程级状态。
- 产物：`ab_run` 的输出经 `ab-contract.validate()` 校验后才被接受。
- 超时 / 进程级沙盒（Job Object、受限令牌）是宿主职责，见 §11。

## 3. 接入等级（`level`）

`c` = 会话注入，`b` = 凭据填表，`a` = 仅启动。等级写在 `manifest.json`，UI 按等级呈现能力边界。
等级的**输出义务**见 §7；产物里的 `level` 必须与清单一致（转换器自检 + 宿主复核，不一致即整体失败）。

## 4. `manifest.json` 字段语义

逐字段对应 `contracts/converter-manifest.schema.json`（`additionalProperties: false`，不得自造字段）。

| 字段 | 类型 | 必填 | 语义与约定 |
|---|---|---|---|
| `id` | string | ✔ | 转换器标识，`^[a-z0-9][a-z0-9-]{1,63}$`；通常等于目标 app，须与产物 `app_id`、`source.id` 一致 |
| `name` | string | ✔ | 展示名（可中文），仅 UI 显示 |
| `version` | string | ✔ | 转换器自身版本，`^[0-9]+\.[0-9]+\.[0-9]+$`；须等于产物 `meta.converter_version` |
| `level` | `c`\|`b`\|`a` | ✔ | 接入等级，同时决定取数范围与输出义务（§7） |
| `readable_paths` | string[] | ✔ | 可读路径声明，≥1 项；宿主只开放这些路径的**只读**访问，超出即拒绝 |
| `entry` | string | ✔ | 入口 WASM 文件名，形如 `converter.wasm`；必须与 manifest 同目录，不得含路径分隔符 |
| `signature` | object | ✔ | 官方签名，`{algorithm:"ed25519", signer, value}`；见 §5 |
| `min_app_version` | string | ✘ | 要求的最低主程序版本；不满足时宿主不加载该转换器 |

### `readable_paths` 约定

- 每项是绝对路径，或含 `{ENV_NAME}` 占位符的模板：`{APPDATA}\sample-app\session-store.json`。
  - 占位符在展开时取宿主进程环境变量；展开后必须是绝对路径。
  - 禁止 `*`（通配）与 `..`（上跳）；宿主展开后要 canonicalize 并拒绝符号链接逃逸。
- 声明越窄越好：**一个数据文件一项**，不要声明整个目录或用户目录。
- 声明与实际读取必须一致：申请了不读的路径属声明过度，审查时按缺陷处理。
- 本地自测的挂载约定：夹具文件名 = 声明路径的 basename（见 §9）。

### 官方签名

签名原文（canonical payload）定义为：把 `manifest.json` 去掉 `signature` 字段后，**按 JSON 键升序**重新序列化（无多余空白、UTF-8、不转义非 ASCII）得到的字节串；`value` 是 ed25519 签名的小写 hex，`signer` 是官方发布密钥 ID。

- 只有官方签发的清单可启用；本地调试可用未签名清单，但宿主必须显式开启调试开关且不得用于发布。
- 占位签名（`value` 全 0）表示「未签名」；`_template` 必须保持占位，由 conformance 检查强制，防止模板被误当成品发布。

## 5. WASM ABI：输入输出都是 JSON，产物只能是 StandardAccess

沙盒内的 guest 模块（`manifest.entry`）与宿主之间只有一条 JSON 通道。

### 导出（guest 必须提供）

| 导出 | 签名 | 语义 |
|---|---|---|
| `memory` | 线性内存 | 宿主读写请求/响应的唯一通道 |
| `ab_alloc` | `(len: i32) -> i32` | 申请 guest 内存；宿主用它写入请求，guest 用它写出响应 |
| `ab_free` | `(ptr: i32, len: i32)` | 释放；实现可空 |
| `ab_run` | `(req_ptr: i32, req_len: i32) -> i64` | 唯一的执行入口 |

### 导入（宿主提供，白名单只有两个）

| 导入 | 签名 | 语义 |
|---|---|---|
| `ab_read_file` | `(path_ptr: i32, path_len: i32) -> i64` | 只读读取**已声明**路径，返回 `(ptr<<32)\|len` 指向 UTF-8 字节；失败返回 `-1` |
| `ab_log` | `(level: i32, msg_ptr: i32, msg_len: i32)` | 日志；`level` 取 `0`=error `1`=warn `2`=info `3`=debug |

白名单之外的一切导入（含 `wasi_snapshot_preview1`、网络、进程、时钟）都不存在；宿主在加载期检查 import 段，出现白名单外导入即拒绝加载。
文件读由**宿主**代理执行，是「只读声明路径」能被真正强制的前提：guest 自己打不开任何路径。

### 请求 JSON（宿主 → guest）

```json
{
  "op": "convert",
  "manifest_id": "sample-app",
  "converter_version": "0.1.0",
  "level": "c",
  "readable_paths": ["{APPDATA}\\sample-app\\session-store.json"],
  "now": "2026-01-15T08:30:00Z"
}
```

- `op`：当前只定义 `convert`（= 取数 + 归一化，一步产出产物）。未知 `op` 必须返回 `-1`。
- `now`：宿主时钟（RFC 3339 UTC）。产物的 `generated_at` 与 `meta.collected_at` 取同一值，不得用被采集文件里的时间戳替代。

### 响应与失败码

- 成功：返回 `(ptr << 32) | len`，指向 UTF-8 的 **StandardAccess JSON**（`contracts/standard-access.schema.json`）；这是 WASM 唯一允许的对外产物。
- 失败：返回 `-1`（i64 全 1）。**失败时不返回任何 JSON**——不允许「带 error 字段的半成品」出现在通道上；失败原因经 `ab_log`。
- 尺寸上限（双向检查）：请求 ≤ 64 KiB，响应 ≤ 512 KiB，线性内存 ≥ 1 MiB。超限即失败。
- 序列化与 `ab-contract` 对齐：**空数组、空对象一律省略**（对应 Rust 侧 `skip_serializing_if`），避免同一份数据出现两种等价形态。
- 文件编码：清单与夹具一律 UTF-8 **无 BOM**；SDK 工具遇到 BOM 会明确报错，不做静默清洗。

## 6. extractor / normalizer 职责分界

| | `extractor.*` | `normalizer.*` |
|---|---|---|
| 职责 | 读声明路径，产出中间格式 | 中间格式 → StandardAccess |
| 允许 | 只经 `ab_read_file` 的只读读取；解析 SQLite/LevelDB/注册表导出 | 纯函数：分组、字段搬运、取最早过期时间 |
| 禁止 | 产出契约结构；做归一化决策；联网；写文件；自取时间 | 任何 IO；改动入参；自造字段校验 |
| 产物 | ExtractorOutput（**转换器自定义**，只在沙盒内存在，绝不跨边界） | StandardAccess JSON（唯一对外产物） |

中间格式的好处是让两边都能被单独测试：conformance 会以「注入的假能力」跑 extractor、以「冻结入参」跑 normalizer。

中间格式不得出现契约字段名（`schema_version` / `level` / `sessions` / `credentials` / `meta`）——出现即视为职责越界，检查会失败。

`normalizer` 中空值处理：来源缺失的字段**省略**（不要用 `null` 占位）；会话 cookie 无过期时间时省略 `expires_at`，而不是写 `null`。

## 7. 接入等级 c / b / a 的输出义务

产物顶层字段恒定四件：`schema_version`、`app_id`、`level`、`generated_at`（`level` 必须等于 `manifest.level`）。

### level = c（会话注入）

- 必须输出 ≥1 条 `sessions[]`，`kind ∈ {cookie, token}`。
- `kind = cookie` 时必须完整携带 cookie 属性：`name`、`value`、`domain`、`path`、`secure`、`http_only`、`same_site`；`expires_at` 在来源存在时必须携带（会话 cookie 可省略）。**会话保真度关键：任何环节不得丢弃属性。**
- 每条会话必须有 `target`（`kind=web` 时 `domain` 必填）与 `source`（`type="converter"`，`id=manifest.id`）。
- 会话级 `expires_at` 取该会话内最早已知过期时间；短时效会话缺失过期时间即不满足 c 级义务。
- `credentials[]` 必须为空/缺省：会话注入不需要明文凭据，能不碰就不碰。
- `source_files` 只列实际读取过的声明路径。

### level = b（凭据填表）

- 必须输出 ≥1 条 `credentials[]`，每项含 `account_ref`、`kind ∈ {password, passkey, totp}`、`username`。
- `kind=password` 必须带 `url`；`kind=password/totp` 用 `secret` / `totp_seed`。
- `kind=passkey` 不得输出任何密钥材料（`secret` 必须省略）。
- `sessions[]` 必须为空/缺省。
- `secret` / `totp_seed` 只允许在内存中传递；禁止写日志、写文件、写 `meta`（落库前由 core 加密）。

### level = a（仅启动）

- `credentials[]` 与 `sessions[]` 必须为空/缺省。
- 只允许启动所需信息：`meta.launch = { app_id, executable_hint? }`（`app_id` 必填）。
- `extractor` 在 a 级下不得把账号 / 会话 / 凭据带进中间格式：真实转换器应把启动元数据与会话库拆成不同的声明路径，a 级只读前者。

> `_template` 的清单声明 `level = "c"`，但 `tests/fixtures/expected/` 下同时提供 `c`/`b`/`a` 三份样例：它们是**义务样例**，用来验证三个分支的实现正确。真实转换器只需提供与其清单等级一致的那一份。

## 8. 脱敏夹具要求（强制）

位置与命名：

```
tests/fixtures/raw/<声明路径的 basename>.json            取数输入（虚构目标 app 的数据产物）
tests/fixtures/expected/standard-access.<level>.json     期望产物（golden）
```

必须遵守：

1. **全部虚构**：夹具中不得出现任何真实凭据、真实账号、真实设备取数结果。样例数据一律手工编造。
2. **域名只用保留域名**：`.example` / `.invalid` / `.test` / `.localhost`（RFC 2606 / 6761）；禁止真实品牌串。
3. **敏感值统一前缀**：`value` / `token` / `secret` / `totp_seed` / `password` 等键的字符串值必须以 `fixture-` 开头。
4. **禁止真实凭据形状**：JWT（`eyJ…`）、AWS（`AKIA…`）、`sk-…`、`ghp_…`、`xoxb-…`、PEM（`-----BEGIN`）、40 位以上不透明串一律禁止。
5. **时间固定**：夹具里的时间戳是固定字面量（RFC 3339 UTC，`Z` 结尾），不随时间变化；这样 golden 才能逐字段比对。
6. **一个文件一次采集**：一个 raw 夹具只描述一次采集的内容，不混多个来源、不掺杂真实环境路径（可写 `{APPDATA}` 这类占位符）。

违反上述任一条，`conformance.mjs` 的脱敏 lint 会指出具体文件 + JSON 路径并失败。

## 9. 本地自测（两条命令，必须实跑）

```powershell
node converters/sdk/tests/conformance.mjs converters/_template --expect-placeholder-signature
python converters/sdk/tests/schema-check.py converters/_template
```

- `conformance.mjs`（Node ≥ 22，零依赖）覆盖 9 组检查：清单约定、契约常量漂移、ABI 往返、只读沙盒白名单、extractor/normalizer 职责分界、normalizer 纯函数、逐等级 golden 对比、等级输出义务、脱敏 lint（日志 + 夹具）。
- `schema-check.py`（Python 3.11 + `jsonschema`）把清单与期望产物喂给权威 schema 校验器，报告 JSON 路径级的失败原因。`format: date-time` 也在强制范围内：本机缺 `rfc3339-validator` 时由脚本按同一 RFC 3339 语义用标准库复核（不静默跳过）。
- 两者分工不重叠：行为与 golden 由 Node 侧断言，**字段结构**只由 schema 侧断言。
- 本地宿主垫片按 `manifest.entry` 推导参考入口：`converter.wasm` → 同目录 `converter.mjs`。真实宿主加载的是 `.wasm`，ABI 完全一致。
- 想换目标转换器：把两条命令的目录参数换成 `converters/<id>` 即可；`--expect-placeholder-signature` 只对模板使用。

## 10. 提交清单

- [ ] `manifest.json` 通过 `converter-manifest.schema.json`，`id`/`version`/`level` 与产物一致
- [ ] `readable_paths` 已收窄到必需文件，且与 `extractor` 实际读取一致
- [ ] `ab_run` 只依赖白名单导入；未知 `op` / 非法 `level` / 超限输入都返回 `-1`
- [ ] `extractor` 不含契约字段，`normalizer` 无 IO、不改入参、可重复求值
- [ ] 等级对应的输出义务逐条满足（§7），空集合以缺省表示
- [ ] 夹具全部脱敏（§8），`raw` 至少一份，`expected` 至少覆盖清单等级
- [ ] 日志不含任何敏感值原值
- [ ] 清单已由官方密钥签名（模板保持占位）
- [ ] 两条自测命令实跑通过，输出贴进变更说明

## 11. 本 SDK 不承担的部分（宿主职责，均未实现/未验证）

- 签名验签的具体实现（ed25519 校验、密钥轮换）在 `core/converter-host`。
- 进程级沙盒形态（Windows Job Object / 受限令牌）、CPU/内存/墙钟限额、`ab_read_file` 的真实只读句柄与 canonicalize 防逃逸。
- WASM 运行时加载与 import 段枚举（wasmtime/wasmi 选型）在 `core/converter-host`。
- 当前仓库 `cargo`/`rustc` 未安装：Rust 侧宿主**未构建**，本 SDK 的运行证据全部来自 `.mjs` 参考实现 + Node/Python 检查器。
- 结论口径：ABI 与规范可执行部分已验证；上述宿主侧条目属「未验证」，实现期首验。
