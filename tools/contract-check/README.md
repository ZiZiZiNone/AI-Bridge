# 契约校验工具（tools/contract-check）

AI-Bridge 契约层（`contracts/*.schema.json`）的**离线校验**入口。两套检查各自独立、零第三方依赖、只读 `contracts/`：

| 脚本 | 作用 | 退出码 |
|---|---|---|
| [check.mjs](check.mjs) | 用夹具（fixtures）验契约：required / type / enum / pattern / const / date-time / additionalProperties / minItems / `$ref`，外加 `kind` ↔ `payload` 一致性 | 0 全过 / 1 有失败 |
| [check-compat.mjs](../../tests/contract-compat/check-compat.mjs) | schema 漂移锁：三份 schema 的 required 字段集合与 `schema_version` 常量 `"1.0"` 不得被悄悄改动 | 0 未漂移 / 1 漂移 |

环境：Node ≥ 22（实测 v22.22.1），仅用 `node:fs` / `node:path` / `node:url`。

## 用法

在仓库根执行：

```powershell
node tools/contract-check/check.mjs
node tests/contract-compat/check-compat.mjs            # 校验锁定值
node tests/contract-compat/check-compat.mjs --selftest # 另证"锁能咬住漂移"
```

两者都只读文件，可安全地串进任何本地 / CI 流程：

```powershell
node tools/contract-check/check.mjs; if ($LASTEXITCODE -ne 0) { exit 1 }
node tests/contract-compat/check-compat.mjs; if ($LASTEXITCODE -ne 0) { exit 1 }
```

## check.mjs：夹具与校验规则

### 夹具命名规范

`tools/contract-check/fixtures/<schema 前缀>.<valid|invalid>[.<规则代号>].json`

- 前缀 = `contracts/` 文件名去掉 `.schema.json`（如 `standard-access`）
- `valid` 用例必须 0 错误；`invalid` 用例必须**命中指定规则代号**（只"报错"不算过，报错原因不符也算失败）
- 规则代号 → 错误码：

| 规则代号 | 错误码 | 触发条件 |
|---|---|---|
| `missing-required` | `REQUIRED_MISSING` | 缺 required 字段 |
| `unknown-field` | `UNKNOWN_FIELD` | 出现未声明字段且 `additionalProperties: false` |
| `type` | `TYPE_MISMATCH` | 类型不符（object/array/string/boolean/integer/number） |
| `enum` | `ENUM_INVALID` | 取值不在 `enum` 内 |
| `pattern` | `PATTERN_MISMATCH` | 字符串不匹配 `pattern` |
| `const` | `CONST_MISMATCH` | 取值不等于 `const` |
| `format` | `FORMAT_INVALID` | 不是 RFC3339 `date-time` |
| `min-items` | `MIN_ITEMS` | 数组短于 `minItems` |
| `kind-payload-mismatch` | `PAYLOAD_KIND_MISMATCH` | `kind` 与 `payload` 不一致（见下） |

另有 `REF_UNRESOLVED`（`$ref` 解析不到）——属 schema 自身损坏，直接判失败。

### 跨字段规则：kind ↔ payload

对「`kind` 枚举恰为 `cookie|token|credential` 且声明了 `payload`」的 schema 生效（即 `standard-access.$defs.session` 与 `session-envelope` 顶层；`$defs.credential` 的 `kind` 枚举是 `password|passkey|totp`，不触发）：

| `kind` | 要求 |
|---|---|
| `cookie` | `payload.cookies` 为非空数组 |
| `token` | `payload.token` 为非空字符串 |
| `credential` | `payload.credential` 存在 |

该规则与 `core/contract/src/lib.rs` 的 `validate` 同义；**运行时权威仍是 `ab-contract`**，本工具只做仓库级离线校验，不承担业务规则。

### 夹具清单（17 份）

| schema | 合法 | 非法 |
|---|---|---|
| `standard-access` | `standard-access.valid.json` | `missing-required`、`kind-payload-mismatch`、`enum`、`pattern`、`const`、`unknown-field` |
| `converter-manifest` | `converter-manifest.valid.json` | `missing-required`、`type`、`pattern`、`min-items` |
| `session-envelope` | `session-envelope.valid.json` | `missing-required`、`kind-payload-mismatch`、`enum`、`format` |

九种规则代号全部有用例覆盖。

每份夹具都是脱敏样例，不含真实凭据。`check.mjs` 同时自检覆盖度：每份 schema 至少 1 合法 + 2 非法、且整体至少 1 份 `kind-payload-mismatch`，否则判失败。

### 新增夹具

1. 在 `fixtures/` 按命名规范放文件（合法样例只需 `<前缀>.valid.json`）
2. 跑 `node tools/contract-check/check.mjs`，确认新用例 PASS
3. 若规则代号不在上表，先在 `check.mjs` 的 `RULE_TO_CODE` 登记并在本 README 补一行

## check-compat.mjs：漂移锁

锁定的基线写在脚本顶部（`EXPECTED_REQUIRED` / `EXPECTED_SCHEMA_VERSION_DECLS`），覆盖**顶层 + 嵌套**全部 `required` 声明（含 `$defs`、`items` 内的对象）：

- `converter-manifest.schema.json`：2 处 required 声明；无 `schema_version`（它的 `version` 是转换器自身版本）
- `session-envelope.schema.json`：5 处；顶层 `schema_version` = `const "1.0"`、`type "string"`
- `standard-access.schema.json`：7 处；顶层 `schema_version` = `const "1.0"`、`type "string"`

`--selftest` 会在内存副本上注入 6 种漂移（删/改/增顶层与嵌套 required、改 `schema_version` 常量、删 `schema_version` 声明），要求锁逐一检出，用以证明锁不是空转。

变更流程：契约真需要改（新增字段一律可选、删除/改语义升主版本）时，**同一次提交内**同时更新 `contracts/` 与脚本基线，让漂移在评审里显式可见。

## 退出码与输出

- 输出 `[PASS] / [FAIL]` 逐条列出，并以 `结果：PASS|FAIL` 收尾
- 退出码 `0` = 全过；`1` = 任一用例失败或契约/夹具自检不通过
