#!/usr/bin/env python3
"""AI-Bridge 契约校验工具（tools/check-contracts.py）。

职责（docs/架构与契约.md §3 表格 `tools/` 行：构建 / 签名 / 打包 / 契约校验）：
  1. 默认模式：校验 contracts/*.schema.json 是合法 JSON，且三份 schema 的
     required / 字段语义与 core/contract 的 Rust 校验规则一致
     （含对 core/contract/src/lib.rs 关键字面量的漂移哨兵）。
  2. --data <file>：把给定 JSON 数据文件按**同一套规则**做语义校验。
     schema 一致性检查与数据校验共同消费下方「规则唯一定义」区，
     两处规则不得各写一份（规则改动只改那一处）。

规则事实源（可验伪定位，core/contract/src/lib.rs）：
  - :9    `pub const SCHEMA_VERSION: &str = "1.0";`
  - :309  `fn valid_id` → ^[a-z0-9][a-z0-9-]{1,63}$（总长 2..=64）
  - :322  `fn is_lower_hex` → 定长小写 hex
  - :326  `fn is_all_zero_hex` → 全零 = 模板占位，拒载
  - :444  `ConverterManifest::validate` → id 格式、module_sha256 格式且非全零、
          签名算法仅 ed25519、占位签名拒载
  - :420  `StandardAccess::validate` / :390 `SessionEnvelope::validate` →
          schema_version 必填且必须 == 1.0

用法：
    python tools/check-contracts.py                 # 校验 contracts/ 三份 schema
    python tools/check-contracts.py --data <file>   # 校验数据文件（自动识别三种类型）
    python tools/check-contracts.py --self-test     # 最小自检

退出码：0 = 全部通过（WARN 不计）；1 = 有 FAIL 或输入错误。

输出卫生：只打印字段名与规则，不回显 cookies / token / secret 等敏感值。

局限：只做上述规则的语义校验。完整 JSON Schema 级校验
（type / format / additionalProperties / kind↔payload 一致性等）见既有
tools/contract-check/check.mjs 与 converters/sdk/tests/schema-check.py；
运行时权威始终是 ab-contract（core/contract）。
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
CONTRACTS_DIR = REPO_ROOT / "contracts"
RUST_LIB = REPO_ROOT / "core" / "contract" / "src" / "lib.rs"

# ==================== 规则唯一定义（不得写第二份） ====================
# 事实源 core/contract/src/lib.rs；改动 Rust 校验规则时同步本区与 RUST_SENTINELS。
SCHEMA_VERSION = "1.0"
ID_PATTERN = r"^[a-z0-9][a-z0-9-]{1,63}$"  # lib.rs valid_id：首字符小写字母/数字，其余含 '-'，长 2..=64
MODULE_SHA256_PATTERN = r"^[0-9a-f]{64}$"  # lib.rs is_lower_hex(_, 64)；非全零另由语义层拒绝
SIGNATURE_VALUE_PATTERN = r"^[0-9a-f]{128}$"  # 签名值 64 字节的 hex 小写（schema 与 Rust 测试一致）
ALGORITHMS = ("ed25519",)  # lib.rs：签名算法仅 ed25519
LEVELS = ("c", "b", "a")  # lib.rs AccessLevel

REQUIRED = {
    "converter-manifest": [
        "id", "name", "version", "level", "readable_paths",
        "entry", "module_sha256", "signature",
    ],
    "standard-access": ["schema_version", "app_id", "level", "generated_at"],
    "session-envelope": [
        "schema_version", "account_ref", "target", "kind",
        "payload", "issued_at", "source",
    ],
}
SIGNATURE_REQUIRED = ["algorithm", "signer", "value"]

SCHEMA_FILE = {
    "converter-manifest": "converter-manifest.schema.json",
    "standard-access": "standard-access.schema.json",
    "session-envelope": "session-envelope.schema.json",
}

# lib.rs 关键字面量（每条可用 grep -F 验伪）；lib.rs 改动而未同步本脚本时此处 FAIL。
RUST_SENTINELS = [
    ('pub const SCHEMA_VERSION: &str = "1.0";', "SCHEMA_VERSION 常量"),
    ("if !(2..=64).contains(&len)", "valid_id 长度 2..=64"),
    ("matches!(b, b'0'..=b'9' | b'a'..=b'f')", "is_lower_hex 字符集"),
    ("is_lower_hex(&self.module_sha256, 64)", "module_sha256 64 位小写 hex"),
    ("is_all_zero_hex(&self.module_sha256)", "module_sha256 拒全零占位"),
    ('self.signature.algorithm != "ed25519"', "签名算法仅 ed25519"),
    ("is_all_zero_hex(&self.signature.value)", "占位签名拒载"),
]
# ==================== 规则唯一定义 结束 ====================


# ---------------- 基础谓词（schema 侧与数据侧共用上面的常量） ----------------

def is_all_zero_hex(value) -> bool:
    """lib.rs:326 is_all_zero_hex：全零是模板占位约定，永不是真签名/真摘要。"""
    return isinstance(value, str) and bool(value) and all(c == "0" for c in value)


def valid_id(value) -> bool:
    """lib.rs:309 valid_id 的正律化等价形式。"""
    return isinstance(value, str) and re.fullmatch(ID_PATTERN, value) is not None


def check_schema_version(value) -> list[str]:
    if value != SCHEMA_VERSION:
        return [f"schema_version 必须为 {SCHEMA_VERSION}（当前 {value!r}），core/contract 不支持其它版本"]
    return []


def check_id(field: str, value) -> list[str]:
    if not isinstance(value, str) or not value:
        return [f"{field} 不能为空"]
    if not valid_id(value):
        return [f"{field} 非法：要求 {ID_PATTERN}"]
    return []


def check_module_sha256(value) -> list[str]:
    if not isinstance(value, str) or not value:
        return ["module_sha256 不能为空"]
    if re.fullmatch(MODULE_SHA256_PATTERN, value) is None:
        return [f"module_sha256 格式非法：要求 {MODULE_SHA256_PATTERN}"]
    if is_all_zero_hex(value):
        return ["module_sha256 是全零占位值，拒载（docs/安全模型.md §6）"]
    return []


def check_signature(sig) -> list[str]:
    errs: list[str] = []
    if not isinstance(sig, dict):
        return ["signature 必须是对象且含 algorithm/signer/value"]
    for key in SIGNATURE_REQUIRED:
        if key not in sig:
            errs.append(f"signature 缺少必填字段 {key}")
    if errs:
        return errs
    algo = sig.get("algorithm")
    if algo not in ALGORITHMS:
        errs.append(f"不支持的签名算法 {algo!r}（白名单：{'/'.join(ALGORITHMS)}）")
    if not isinstance(sig.get("signer"), str) or not sig.get("signer"):
        errs.append("signature.signer 不能为空")
    value = sig.get("value")
    if not isinstance(value, str) or not value:
        errs.append("signature.value 不能为空")
    elif re.fullmatch(SIGNATURE_VALUE_PATTERN, value) is None:
        errs.append(f"signature.value 格式非法：要求 {SIGNATURE_VALUE_PATTERN}")
    elif is_all_zero_hex(value):
        errs.append("signature.value 是全零占位签名，拒载（docs/安全模型.md §6）")
    return errs


def check_level(value) -> list[str]:
    if value not in LEVELS:
        return [f"level 必须是 {'/'.join(LEVELS)} 之一（当前 {value!r}）"]
    return []


# ---------------- 通用输出 ----------------

Item = tuple[str, str]  # (PASS|FAIL|WARN, message)


def emit(items: list[Item], errs: list[str], ok_msg: str, fail_prefix: str) -> None:
    if errs:
        for e in errs:
            items.append(("FAIL", f"{fail_prefix}{e}"))
    else:
        items.append(("PASS", ok_msg))


def print_items(items: list[Item]) -> bool:
    for level, msg in items:
        print(f"[{level}] {msg}")
    n_pass = sum(1 for l, _ in items if l == "PASS")
    n_fail = sum(1 for l, _ in items if l == "FAIL")
    n_warn = sum(1 for l, _ in items if l == "WARN")
    verdict = "FAIL" if n_fail else "PASS"
    print(f"结果：{verdict}（PASS {n_pass} / FAIL {n_fail} / WARN {n_warn}）")
    return n_fail == 0


# ---------------- schema 一致性检查（规则消费处 1/2） ----------------

def check_schema_doc(name: str, doc: dict) -> list[Item]:
    """校验一份 schema 的 required / 字段语义与规则区一致。"""
    items: list[Item] = []
    fname = SCHEMA_FILE[name]

    required = doc.get("required")
    if not isinstance(required, list):
        items.append(("FAIL", f"{fname} 缺少 required 声明"))
    elif sorted(required) == sorted(REQUIRED[name]):
        items.append(("PASS", f"{fname} required 与 core/contract 一致（{len(required)} 项）"))
    else:
        items.append((
            "FAIL",
            f"{fname} required 不一致：schema={required} 期望={REQUIRED[name]}"
            "（期望值来自 core/contract 各结构体必填字段）",
        ))

    props = doc.get("properties")
    if not isinstance(props, dict):
        items.append(("FAIL", f"{fname} 缺少 properties 声明"))
        return items

    # schema_version 必填且常量 == 1.0（converter-manifest 无 schema_version，
    # 其 version 是转换器自身版本，见 tools/contract-check/README.md）。
    if name in ("standard-access", "session-envelope"):
        sv = props.get("schema_version", {})
        if sv.get("type") == "string" and sv.get("const") == SCHEMA_VERSION:
            items.append(("PASS", f"{fname} schema_version：type=string, const={SCHEMA_VERSION}"))
        else:
            items.append((
                "FAIL",
                f"{fname} schema_version 声明不一致：期望 type=string 且 const={SCHEMA_VERSION}"
                f"（当前 {sv!r}）",
            ))

    # id / app_id 格式（session-envelope 顶层两者皆无：account_ref 不是 id；
    # 嵌套 target.app_id 在 core/contract 仅是 Option<String>，不做格式校验）。
    if name in ("converter-manifest", "standard-access"):
        id_field = "id" if name == "converter-manifest" else "app_id"
        pat = props.get(id_field, {}).get("pattern")
        if pat == ID_PATTERN:
            items.append(("PASS", f"{fname} {id_field}.pattern == {ID_PATTERN}"))
        else:
            items.append(("FAIL", f"{fname} {id_field}.pattern 不一致：schema={pat!r} 期望={ID_PATTERN!r}"))

    # level 枚举（core/contract AccessLevel）。
    if name in ("converter-manifest", "standard-access"):
        enum = props.get("level", {}).get("enum")
        if enum == list(LEVELS):
            items.append(("PASS", f"{fname} level.enum == {list(LEVELS)}"))
        else:
            items.append(("FAIL", f"{fname} level.enum 不一致：schema={enum!r} 期望={list(LEVELS)!r}"))

    if name == "converter-manifest":
        # module_sha256 格式。
        pat = props.get("module_sha256", {}).get("pattern")
        if pat == MODULE_SHA256_PATTERN:
            items.append(("PASS", f"{fname} module_sha256.pattern == {MODULE_SHA256_PATTERN}"))
        else:
            items.append((
                "FAIL",
                f"{fname} module_sha256.pattern 不一致：schema={pat!r} 期望={MODULE_SHA256_PATTERN!r}",
            ))

        # 签名：required / 算法白名单 / 签名值格式。
        sig = props.get("signature", {})
        sig_required = sig.get("required")
        if isinstance(sig_required, list) and sorted(sig_required) == sorted(SIGNATURE_REQUIRED):
            items.append(("PASS", f"{fname} signature.required 与 core/contract 一致"))
        else:
            items.append((
                "FAIL",
                f"{fname} signature.required 不一致：schema={sig_required!r} 期望={SIGNATURE_REQUIRED}",
            ))
        sig_props = sig.get("properties", {})
        algo_decl = sig_props.get("algorithm", {})
        if algo_decl.get("const") == ALGORITHMS[0] or algo_decl.get("enum") == list(ALGORITHMS):
            items.append(("PASS", f"{fname} signature.algorithm 白名单 == {list(ALGORITHMS)}"))
        else:
            items.append((
                "FAIL",
                f"{fname} signature.algorithm 白名单不一致：schema={algo_decl!r}"
                f" 期望 const/enum == {list(ALGORITHMS)}（core/contract 仅 ed25519）",
            ))
        pat = sig_props.get("value", {}).get("pattern")
        if pat == SIGNATURE_VALUE_PATTERN:
            items.append(("PASS", f"{fname} signature.value.pattern == {SIGNATURE_VALUE_PATTERN}"))
        else:
            items.append((
                "FAIL",
                f"{fname} signature.value.pattern 不一致：schema={pat!r} 期望={SIGNATURE_VALUE_PATTERN!r}",
            ))

        # 已知的 schema/Rust 不对称（不构成拒载方向的不一致，标注待确认）。
        items.append((
            "WARN",
            "待确认：schema 的 pattern 无法表达「非全零」拒载语义"
            "（core/contract/src/lib.rs 的 is_all_zero_hex），全零占位值只能在数据校验/运行时拒绝",
        ))
        version_pattern = props.get("version", {}).get("pattern")
        if version_pattern:
            items.append((
                "WARN",
                f"待确认：version.pattern {version_pattern!r} 严于 core/contract 的非空检查"
                "（lib.rs:455），schema 会拒绝 Rust 接受的非语义化版本号；方向为收紧，不放行占位",
            ))

    return items


def run_schema_mode() -> list[Item]:
    items: list[Item] = []

    # 1) core/contract 漂移哨兵。
    if not RUST_LIB.is_file():
        items.append(("FAIL", f"找不到 {RUST_LIB}，无法锚定 core/contract 校验规则"))
        return items
    rust_text = RUST_LIB.read_text(encoding="utf-8")
    for literal, label in RUST_SENTINELS:
        if literal in rust_text:
            items.append(("PASS", f"core/contract 锚定命中：{label}"))
        else:
            items.append((
                "FAIL",
                f"core/contract/src/lib.rs 缺少字面量 {literal!r}（{label}）——"
                "Rust 校验规则已漂移，先核对 lib.rs 再同步本脚本",
            ))

    # 2) 三份 schema：合法 JSON + 语义一致。
    expected_files = set(SCHEMA_FILE.values())
    found = sorted(p.name for p in CONTRACTS_DIR.glob("*.schema.json"))
    extra = [n for n in found if n not in expected_files]
    if extra:
        items.append(("WARN", f"待确认：contracts/ 出现三份之外的 schema {extra}（docs/架构与契约.md §6 定义为三份）"))
    missing_files = [n for n in expected_files if n not in found]
    for n in missing_files:
        items.append(("FAIL", f"缺少 schema 文件：{CONTRACTS_DIR / n}"))

    for name, fname in SCHEMA_FILE.items():
        path = CONTRACTS_DIR / fname
        if not path.is_file():
            continue
        try:
            doc = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, UnicodeDecodeError, json.JSONDecodeError) as err:
            items.append(("FAIL", f"{fname} 不是合法 JSON：{err}"))
            continue
        if not isinstance(doc, dict):
            items.append(("FAIL", f"{fname} 顶层必须是 JSON 对象"))
            continue
        items.append(("PASS", f"{fname} 是合法 JSON"))
        items.extend(check_schema_doc(name, doc))

    return items


# ---------------- 数据校验（规则消费处 2/2，与上面共用同一套规则） ----------------

def detect_type(doc: dict) -> str | None:
    if any(k in doc for k in ("entry", "module_sha256", "signature", "readable_paths")):
        return "converter-manifest"
    if any(k in doc for k in ("app_id", "generated_at", "credentials", "sessions")):
        return "standard-access"
    if any(k in doc for k in ("issued_at", "scope", "account_ref")):
        return "session-envelope"
    return None


def check_data_doc(dtype: str, doc: dict) -> list[Item]:
    items: list[Item] = []

    missing = [k for k in REQUIRED[dtype] if k not in doc]
    if missing:
        items.append(("FAIL", f"缺少必填字段：{missing}"))
    else:
        items.append(("PASS", f"必填字段齐全（{len(REQUIRED[dtype])} 项）"))

    if dtype in ("standard-access", "session-envelope"):
        emit(items, check_schema_version(doc.get("schema_version")),
             f"schema_version == {SCHEMA_VERSION}", "")

    if dtype == "converter-manifest":
        emit(items, check_id("id", doc.get("id")), "id 格式合法", "")
        for field in ("name", "version", "entry"):
            value = doc.get(field)
            if not isinstance(value, str) or not value:
                items.append(("FAIL", f"{field} 不能为空"))
            else:
                items.append(("PASS", f"{field} 非空"))
        paths = doc.get("readable_paths")
        if not isinstance(paths, list) or not paths or not all(isinstance(p, str) for p in paths):
            items.append(("FAIL", "readable_paths 必须是非空字符串数组"))
        else:
            items.append(("PASS", f"readable_paths 非空（{len(paths)} 项）"))
        emit(items, check_level(doc.get("level")), "level 合法", "")
        emit(items, check_module_sha256(doc.get("module_sha256")), "module_sha256 合法且非占位", "")
        emit(items, check_signature(doc.get("signature")), "signature 合法且非占位（算法白名单 + 128 位 hex）", "")

    if dtype == "standard-access":
        emit(items, check_id("app_id", doc.get("app_id")), "app_id 格式合法", "")
        if not isinstance(doc.get("generated_at"), str) or not doc.get("generated_at"):
            items.append(("FAIL", "generated_at 不能为空"))
        else:
            items.append(("PASS", "generated_at 非空"))
        emit(items, check_level(doc.get("level")), "level 合法", "")

    return items


def run_data_mode(path: Path) -> list[Item]:
    if not path.is_file():
        return [("FAIL", f"数据文件不存在：{path}")]
    try:
        doc = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as err:
        return [("FAIL", f"{path.name} 不是合法 JSON：{err}")]
    if not isinstance(doc, dict):
        return [("FAIL", "顶层必须是 JSON 对象")]
    dtype = detect_type(doc)
    if dtype is None:
        return [("FAIL", "无法识别数据类型（期望 converter-manifest / standard-access / session-envelope 之一）")]
    items: list[Item] = [("PASS", f"数据文件 {path.name} 是合法 JSON，类型识别：{dtype}")]
    items.extend(check_data_doc(dtype, doc))
    return items


# ---------------- 自检 ----------------

def self_test() -> int:
    fails: list[str] = []
    count = [0]

    def expect(cond: bool, msg: str) -> None:
        count[0] += 1
        if not cond:
            fails.append(msg)

    # 谓词层
    expect(valid_id("qq"), "valid_id(qq) 应通过")
    expect(valid_id("sample-app"), "valid_id(sample-app) 应通过")
    expect(not valid_id("Bad!"), "valid_id(Bad!) 应拒绝")
    expect(not valid_id("a"), "valid_id(a) 短于 2 字符应拒绝")
    expect(not valid_id("a" * 65), "valid_id(65 字符) 超长应拒绝")
    good_hash = "3f2a9c1d5b7e4088a1c2d3e4f5061728394a5b6c7d8e9f001122334455667788"
    expect(check_module_sha256(good_hash) == [], "合法 module_sha256 应通过")
    expect(check_module_sha256("0" * 64) != [], "全零 module_sha256 占位应拒绝")
    expect(check_module_sha256("zz" * 32) != [], "非 hex module_sha256 应拒绝")
    expect(check_module_sha256("abcd") != [], "过短 module_sha256 应拒绝")
    good_sig = "ab" * 64
    ok_sig = {"algorithm": "ed25519", "signer": "official", "value": good_sig}
    expect(check_signature(ok_sig) == [], "合法 signature 应通过")
    expect(check_signature({**ok_sig, "value": "0" * 128}) != [], "全零占位签名应拒绝")
    expect(check_signature({**ok_sig, "algorithm": "rsa"}) != [], "非 ed25519 算法应拒绝")
    expect(check_signature({"algorithm": "ed25519", "signer": "x"}) != [], "缺 signature.value 应拒绝")
    expect(check_schema_version("1.0") == [], "schema_version=1.0 应通过")
    expect(check_schema_version("0.9") != [], "schema_version=0.9 应拒绝")

    # schema 侧：合法 schema 0 FAIL；篡改 pattern / required 必须 FAIL
    mini = {
        "type": "object",
        "required": REQUIRED["converter-manifest"],
        "properties": {
            "id": {"type": "string", "pattern": ID_PATTERN},
            "name": {"type": "string"},
            "version": {"type": "string", "pattern": r"^[0-9]+\.[0-9]+\.[0-9]+$"},
            "level": {"enum": list(LEVELS)},
            "readable_paths": {"type": "array", "items": {"type": "string"}, "minItems": 1},
            "entry": {"type": "string"},
            "module_sha256": {"type": "string", "pattern": MODULE_SHA256_PATTERN},
            "signature": {
                "type": "object",
                "required": SIGNATURE_REQUIRED,
                "properties": {
                    "algorithm": {"const": ALGORITHMS[0]},
                    "signer": {"type": "string"},
                    "value": {"type": "string", "pattern": SIGNATURE_VALUE_PATTERN},
                },
            },
        },
    }
    expect(not any(l == "FAIL" for l, _ in check_schema_doc("converter-manifest", mini)),
           "合法 schema 应 0 FAIL")
    bad_pattern = json.loads(json.dumps(mini))
    bad_pattern["properties"]["id"]["pattern"] = ".*"
    expect(any(l == "FAIL" for l, _ in check_schema_doc("converter-manifest", bad_pattern)),
           "id.pattern 漂移应 FAIL")
    bad_required = json.loads(json.dumps(mini))
    bad_required["required"] = ["id"]
    expect(any(l == "FAIL" for l, _ in check_schema_doc("converter-manifest", bad_required)),
           "required 漂移应 FAIL")

    # 数据侧：合法 manifest 0 FAIL；占位签名 / 缺必填必须 FAIL
    manifest = {
        "id": "sample-app", "name": "Sample", "version": "0.1.0", "level": "c",
        "readable_paths": ["{APPDATA}\\sample-app"], "entry": "converter.wasm",
        "module_sha256": good_hash,
        "signature": {"algorithm": "ed25519", "signer": "official", "value": good_sig},
    }
    expect(detect_type(manifest) == "converter-manifest", "类型识别 manifest 应正确")
    expect(not any(l == "FAIL" for l, _ in check_data_doc("converter-manifest", manifest)),
           "合法 manifest 应 0 FAIL")
    placeholder = json.loads(json.dumps(manifest))
    placeholder["signature"]["value"] = "0" * 128
    expect(any(l == "FAIL" for l, _ in check_data_doc("converter-manifest", placeholder)),
           "占位签名 manifest 应 FAIL")
    expect(any(l == "FAIL" for l, _ in check_data_doc("converter-manifest", {"id": "qq"})),
           "缺必填字段应 FAIL")
    envelope = {"schema_version": "1.0", "account_ref": "acc-1", "target": {"kind": "web"},
                "kind": "cookie", "payload": {}, "issued_at": "2026-10-09T00:00:00Z",
                "source": {"type": "extension", "id": "chromium"}}
    expect(detect_type(envelope) == "session-envelope", "类型识别 envelope 应正确")
    expect(not any(l == "FAIL" for l, _ in check_data_doc("session-envelope", envelope)),
           "合法 envelope 应 0 FAIL")
    bad_version = dict(envelope, schema_version="0.9")
    expect(any(l == "FAIL" for l, _ in check_data_doc("session-envelope", bad_version)),
           "schema_version=0.9 应 FAIL")

    if fails:
        for f in fails:
            print(f"[FAIL] self-test：{f}")
        print(f"结果：FAIL（自检 {len(fails)} 项不过）")
        return 1
    print(f"结果：PASS（self-test {count[0]} 项断言全过）")
    return 0


# ---------------- 入口 ----------------

def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="check-contracts.py",
        description="AI-Bridge 契约校验：schema 与 core/contract 规则一致性 + 数据文件语义校验",
    )
    parser.add_argument("--data", metavar="FILE", help="按同一套规则校验给定 JSON 数据文件")
    parser.add_argument("--self-test", action="store_true", help="运行最小自检后退出")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()
    if args.data:
        items = run_data_mode(Path(args.data))
    else:
        items = run_schema_mode()
    return 0 if print_items(items) else 1


if __name__ == "__main__":
    sys.exit(main())
