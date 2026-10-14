# tools/ 工程脚本

AI-Bridge 工程脚本（docs/架构与契约.md §3 表格 `tools/` 行：**构建 / 签名 / 打包 / 契约校验**）。
本次交付三个 Python 脚本（3.10+，**仅标准库**）加本文；均为离线工具，不联网、不打印任何秘密材料。

| 脚本 | 职责 | 退出码 |
|---|---|---|
| [check-contracts.py](check-contracts.py) | 契约校验：schema 合法性 + 与 core/contract 规则一致性；`--data` 校验数据文件 | 0 通过 / 1 有 FAIL |
| [sign-converter.py](sign-converter.py) | 转换器签名链：算 `module_sha256`、产出待签名字节；回填外部 ed25519 签名 | 0 成功 / 1 失败 |
| [package-converter.py](package-converter.py) | 打包 `converters/<id>/` 为发布目录或 zip；check-contracts 不过拒绝打包 | 0 成功 / 1 拒绝 |

目录内既有产物（本次未改动）：`contract-check/`（Node 夹具级校验）、`fetch-vendor.py`（cargo 离线依赖）。

### fetch-vendor.py（2026-10-12 增补两个参数）

cargo 直连 crates.io 在本机失败（schannel 凭证错误），本工具经 Python/OpenSSL 栈把依赖下载到
`.cargo-home/vendor` 供 cargo 离线解析；增量幂等（已有 `.cargo-checksum.json` 的 crate 跳过）。

```powershell
python tools/fetch-vendor.py                                          # 根 workspace（core/*）依赖闭包
python tools/fetch-vendor.py --extra-manifest desktop/src-tauri/Cargo.toml   # 额外独立 workspace 清单，可重复
python tools/fetch-vendor.py --all-versions toml_datetime             # 拉取该 crate 全部未撤回版本，可重复
python tools/fetch-vendor.py --selftest                               # 解析器本地自测（零网络，21 项）
```

`--all-versions` 的用途：贪心选版与精确 pin（`=x.y.z`）冲突时，把候选版本集交给 cargo 自己回溯
选版（2026-10-12 tauri 2.12.3 的 `toml_datetime =0.6.3` 即此例）。

## check-contracts.py —— 契约校验

```powershell
python tools/check-contracts.py                 # 校验 contracts/ 三份 schema
python tools/check-contracts.py --data <file>   # 校验数据文件（自动识别三种类型）
python tools/check-contracts.py --self-test     # 最小自检
```

- **默认模式**：`contracts/*.schema.json` 是合法 JSON；三份 schema 的 required / 字段语义
  与 `core/contract/src/lib.rs` 一致（schema_version 必填且 const=1.0、id/app_id 格式
  `^[a-z0-9][a-z0-9-]{1,63}$`、module_sha256 为 64 位小写 hex、签名算法白名单仅 ed25519、
  level 枚举 c|b|a）；并对 lib.rs 关键字面量做漂移哨兵（缺一即 FAIL）。
- **`--data` 模式**：数据文件按**同一套规则**校验（规则只定义一份，schema 侧与数据侧共用；
  含 required 齐全、占位签名/占位 module_sha256 拒载、readable_paths 非空等）。
  类型按字段自动识别：converter-manifest / standard-access / session-envelope。
- 输出 `[PASS]/[FAIL]/[WARN]` 逐条列出，`结果：PASS|FAIL` 收尾；**WARN 不影响退出码**。
- 局限：不做完整 JSON Schema 级校验（type/format/additionalProperties/kind↔payload 等，
  见 `contract-check/check.mjs`）；运行时权威始终是 ab-contract。

## sign-converter.py —— 转换器签名链

> **⚠ 本脚本不签名。** Python 标准库没有 ed25519 实现，密码学实现不得手写，
> 故实现到「产出待签名字节与摘要」为止。**签名步骤需外部 ed25519 工具**
> （openssl / ssh-keygen / cryptography 等；具体命令与密钥格式**未验证**，属待确认项）。
> 本脚本不接受、不读取、不打印任何私钥。

```powershell
# ① 产出待签名字节（只读 manifest，写出 <out-bytes>）
python tools/sign-converter.py --manifest converters/qq/manifest.json `
    [--module converters/qq/qq.wasm] [--out-bytes qq.presign.bin]

# ② 外部 ed25519 工具对待签名字节文件签名（私钥只在该工具内使用）

# ③ 回填外部签名（算法固定 ed25519，同时写入计算出的 module_sha256）
python tools/sign-converter.py --manifest converters/qq/manifest.json `
    --signer aibridge-official-2026 --signature-hex <128 位 hex>

python tools/sign-converter.py --self-test
```

- 待签名字节 = 「manifest 规范化字节 + module_sha256」（docs/安全模型.md §6：
  签名覆盖 manifest 规范化字节 + module_sha256，防「换模块保清单」）。
- 回填模式先写临时文件、经 `check-contracts.py --data` 通过（含**占位签名拒载**）才替换
  manifest.json；回填**不验签**，验签由 core/converter-host 加载时执行。
- 顺序不可乱：产出 → 外部签名 → 回填；期间不得修改 manifest，否则签名失效。
- **待确认（仓库内无事实源）**：「manifest 规范化字节」的确切定义。本脚本采用
  `json.dumps(manifest 去 signature 字段、module_sha256 置为计算值, sort_keys=True,
  ensure_ascii=False, separators=(",",":"))` 的 UTF-8 字节，尾部直接拼接 module_sha256
  hex 的 ASCII 字节。core/converter-host/src/traits.rs 只声明覆盖义务、不定义字节构造；
  宿主验签实现落地时必须与此对齐（对齐只需改 `canonical_manifest_bytes()` 一处）。

## package-converter.py —— 打包

```powershell
python tools/package-converter.py --src converters/qq --out dist/qq-1.2.0       # 发布目录
python tools/package-converter.py --src converters/qq --out dist/qq-1.2.0.zip   # zip
python tools/package-converter.py --self-test
```

- **门禁**：打包前先运行 `check-contracts.py --data <src/manifest.json>`，校验不过
  **拒绝打包**（含占位签名/占位摘要拒载）；`manifest.entry` 指向的模块文件缺失也拒绝。
- 打包内容 = 源目录全部文件（递归；manifest.json + 模块 + tests/ 样例必然在内）。
  输出已存在的路径拒绝覆盖；输出不得位于源目录内。
- **待确认**：发布包是否裁剪 `extractor.*`/`normalizer.*` 等源码（docs 未规定）——
  当前整目录收录，不静默丢文件；`tests/` 缺失仅 WARN（是否打包必填 docs 未规定）。

## 未验证项与待确认项（时点：2026-10-12；规范/设计补全后本清单作废）

1. **ed25519 签名步骤未实现、未验证**：需外部工具；具体工具命令、密钥格式、
   与回填值的 hex 约定均**待确认**，本组脚本只保证待签名字节产出与格式校验。
2. **「manifest 规范化字节」定义待确认**（见 sign-converter 一节），宿主验签落地须对齐。
3. **schema 与 core/contract 的两处已知不对称**（check-contracts 以 WARN 提示，不判 FAIL）：
   - schema pattern 表达不了「非全零」拒载（lib.rs `is_all_zero_hex`），全零占位只能在
     数据校验/运行时拒绝；
   - `version.pattern`（x.y.z）严于 lib.rs 的非空检查，schema 比 Rust 收紧。
4. **发布包内容边界待确认**（是否含转换器源码、tests/ 是否必填）。
5. **本机环境注记**：系统 `%TEMP%` 在本机沙箱下不可写，且 `tempfile.mkdtemp/mkstemp`
   的 0o700/0o600 建目录/文件会产生后续写入被拒的 Windows 权限项——脚本因此不使用
   tempfile，自检暂存于 `tools/.self-test-tmp/`（`TOOLS_SCRATCH` 可覆盖），用后即删。
   **2026-10-12 诊断完成**：`tools/.self-test-tmp/` 下 4 个残留空目录（mode700/tmp*）的权限条目
   即 Python 0o700 mkdir 的标准最小形态（仅 CREATOR OWNER / SYSTEM / 管理员），受限令牌下列举
   必然被拒；专用诊断工具判为「非可修复类」（无缺失权限、无外来软件包条目），未做任何改动，
   证据存 `.acl-recovery/`。无流程依赖这些残渣；若要清掉需临时开「完全权限」手动删除。
6. 三个脚本的自检与示例运行已在本机实际执行（见交付报告）；`converters/_template`
   为未签名模板（全零占位），按设计会被拒载/拒绝打包，这是预期行为而非缺陷。
