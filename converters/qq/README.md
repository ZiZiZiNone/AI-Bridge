# converters/qq —— QQ 首接（B4.3）

首个官方转换器，用途是**验证标准接入管线**（manifest → 验签 → 沙盒取数 → 归一化 → StandardAccess）。
规范见 `converters/sdk/README.md`（唯一事实源为 `contracts/` 三个 schema）。

## 结构

```
qq/
├─ manifest.json      清单（id=qq, level=c, readable_paths, 官方签名位）
├─ converter.mjs      ABI 翻译层（机械实现，与 _template 同构）
├─ extractor.mjs      取数：只读声明路径 → 中间格式（QQ 字段原样透传，如 uin）
├─ normalizer.mjs     归一化：uin → account_ref（`qq-<uin>`，唯一映射点）→ StandardAccess
└─ tests/fixtures/    脱敏夹具：raw/session-store.json + expected/standard-access.{c,b,a}.json
```

三个等级的取数/归一化分支都已实现（c 会话注入 / b 凭据填表 / a 仅启动），对应
「开工首验取数，失败自动降 b/a」的降级能力面（桌面端流程 W-F2）。

## 待确认清单（COM-007：占位 + 说明，不编造事实）

以下内容为**占位实现**，开工首验（对真实 QQ 安装取数）后校准；管线验收不受影响：

1. **数据产物路径**：`{APPDATA}\Tencent\QQ\session-store.json` 为占位模板路径，非事实记载；
   真实路径以首验为准（`manifest.readable_paths` 与 `extractor.mjs` 的 `STORE_PATH` 同步改）。
2. **数据产物格式**：夹具中的 `uin` / `session.cookies` / `session.token` 等结构为占位形状；
   真实 QQ 本地数据格式（SQLite / LevelDB / 其它）以首验为准，只改 `extractor.mjs` 读取与解析段。
3. **身份映射**：`uin → account_ref = qq-<uin>` 待首验校准（改动只落在 `normalizer.mjs` 的 `accountRefOf` 一处）。
4. **降级语义**：「失败自动降 b/a」与 SDK §3「产物 level 必须与清单一致」的衔接待裁决
   （当前实现：三个等级分支都可独立产出合规产物；生产运行按清单 level=c 出产物）。
5. **a 级路径拆分**：SDK §7 建议真实转换器把启动元数据与会话库拆成两个声明路径；
   当前占位为单一路径，首验时按真实目录结构决定是否拆分。

## 发布链（未签名状态是设计内状态）

`module_sha256` 与 `signature.value` 当前为全零占位：**占位拒载**（`tools/check-contracts.py`、
`core/contract`）会拒绝打包与启用，防未签名成品外流。发布前：

```powershell
python tools/sign-converter.py   # 计算 module_sha256、产出待签名字节、回填官方 ed25519 签名
python tools/package-converter.py --src converters/qq   # 契约校验通过后打包
```

## 自测（已实跑）

```powershell
node converters/sdk/tests/conformance.mjs converters/qq     # 26 项检查，0 失败
python converters/sdk/tests/schema-check.py converters/qq   # 4 个文件，0 失败
```
