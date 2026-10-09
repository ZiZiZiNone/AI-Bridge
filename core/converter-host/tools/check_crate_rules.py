#!/usr/bin/env python3
"""ab-converter 开发期核查（非运行时组件）。

用法：python core/converter-host/tools/check_crate_rules.py

核查项（全部可执行、可证伪）：
  1. 测试夹具 <-> contracts/*.schema.json 一致性（合法夹具必须通过，非法夹具必须被 schema 拒绝）
  2. Cargo.toml 包名 / workspace 继承 / 依赖白名单（只允许 ab-contract 路径依赖 + serde_json）
  3. 仓库根 workspace 是否纳入 core/*（本 crate 能否被 cargo 看见）
  4. 工程硬规则静态扫描：无 unsafe、无全局 static / 静态门面、无 std::fs / std::net / 宿主与密码学 crate
  5. 单测覆盖：四个必需场景的测试函数是否都在 src/tests.rs
  6. Rust 结构：`mod X;` 文件存在、include_str! 夹具存在、括号配平
  7. 跨 crate API 一致性：本 crate 用到的 ab-contract 公开项（类型 / 字段 / 方法 / 枚举变体）确实存在

说明：本机无 cargo / rustc，第 6、7 项是不能编译时的可执行替代核查，不能证明可编译。

退出码：0 全部通过；1 有失败项。
"""

from __future__ import annotations

import json
import re
import sys
import tomllib
from pathlib import Path

import jsonschema

CRATE = Path(__file__).resolve().parent.parent
REPO = CRATE.parent.parent
FIXTURES = CRATE / "tests" / "fixtures"
SCHEMAS = REPO / "contracts"
CONTRACT_LIB = REPO / "core" / "contract" / "src" / "lib.rs"

ALLOWED_DEPENDENCIES = {"ab-contract", "serde_json"}
FORBIDDEN_DEPENDENCY_HINTS = (
    "crypto",
    "ed25519",
    "dalek",
    "ring",
    "argon2",
    "aes",
    "openssl",
    "wasmtime",
    "wasm",
    "tauri",
    "reqwest",
    "hyper",
    "tokio",
)
FORBIDDEN_USE = re.compile(
    r"(?m)^\s*use\s+[^;]*\b(tauri|wasmtime|reqwest|ring|ed25519_dalek|argon2|aes|openssl|std::fs|std::net)\b"
)
UNSAFE = re.compile(r"\bunsafe\s*(\{|fn\b|impl\b|trait\b)")
STATIC_ITEM = re.compile(r"(?m)^\s*(?:pub(?:\s*\([^)]*\))?\s+)?static\s")
STATIC_FACADE = re.compile(r"\b(?:lazy_static|once_cell|OnceLock|OnceCell|thread_local)\b")
REQUIRED_TESTS = {
    # 场景一：验签失败拒载
    "failed_signature_rejects_load",
    # 场景二：等级裁剪正确（a / b / c 三档）
    "level_a_keeps_only_meta_and_launch_info",
    "level_b_keeps_only_credentials",
    "level_c_keeps_sessions",
    # 场景三：路径越界拒绝
    "path_outside_scope_rejected",
    # 场景四：合法加载全链
    "legal_load_and_extract_full_chain",
}
PUBLIC_SURFACE = (
    "ConverterHost",
    "VerifiedConverter",
    "SignatureVerifier",
    "SandboxRunner",
    "SandboxRequest",
    "PathGuard",
    "effective_level",
    "prune",
)

# 本 crate 实际用到的 ab-contract 公开面：名字写错在真编译时必然失败，这里在不编译的前提下核对存在性。
USED_ENUM_VARIANTS = {"AccessLevel": ("A", "B", "C")}
USED_STRUCT_FIELDS = {
    "ConverterManifest": ("id", "name", "level", "readable_paths", "entry"),
    "StandardAccess": ("level", "app_id", "generated_at", "credentials", "sessions", "meta"),
    "CredentialRecord": ("account_ref",),
}
USED_METHODS = (("StandardAccess", "validate"), ("ConverterManifest", "validate"))

failures: list[str] = []


def report(ok: bool, message: str) -> None:
    print(("  PASS " if ok else "  FAIL ") + message)
    if not ok:
        failures.append(message)


def check_fixtures_against_schemas() -> None:
    print("[1/7] 夹具 <-> 契约 schema")
    cases = (
        ("manifest_ok.json", "converter-manifest.schema.json", True),
        ("manifest_invalid.json", "converter-manifest.schema.json", False),
        ("standard_access_full.json", "standard-access.schema.json", True),
    )
    for fixture, schema_file, must_be_valid in cases:
        schema = json.loads((SCHEMAS / schema_file).read_text(encoding="utf-8"))
        instance = json.loads((FIXTURES / fixture).read_text(encoding="utf-8"))
        errors = sorted(
            jsonschema.Draft202012Validator(schema).iter_errors(instance),
            key=lambda err: [str(part) for part in err.path],
        )
        expectation = "符合" if must_be_valid else "被拒绝（负向夹具）"
        detail = "; ".join(err.message for err in errors[:3])
        report(
            (not errors) == must_be_valid,
            f"{fixture} 期望{expectation} {schema_file}"
            + (f" —— {detail}" if errors and must_be_valid else ""),
        )


def check_manifest() -> None:
    print("[2/7] Cargo.toml 依赖白名单与包名")
    manifest = tomllib.loads((CRATE / "Cargo.toml").read_text(encoding="utf-8"))
    package = manifest.get("package", {})
    report(
        package.get("name") == "ab-converter",
        f'package.name == "ab-converter"（实际 {package.get("name")!r}）',
    )
    report(
        package.get("edition") == {"workspace": True}
        and package.get("version") == {"workspace": True},
        "edition / version 继承 workspace",
    )

    dependencies = manifest.get("dependencies", {})
    names = set(dependencies)
    report(
        names == ALLOWED_DEPENDENCIES,
        f"依赖集合 == {sorted(ALLOWED_DEPENDENCIES)}（实际 {sorted(names)}）",
    )
    report(
        dependencies.get("ab-contract", {}).get("path") == "../contract",
        "ab-contract 为路径依赖 ../contract",
    )
    hits = sorted(
        name for name in names if any(hint in name for hint in FORBIDDEN_DEPENDENCY_HINTS)
    )
    report(not hits, f"无密码学 / 沙盒运行时 / 宿主依赖（命中 {hits}）")


def check_workspace_membership() -> None:
    print("[3/7] 仓库根 workspace 纳入 core/*")
    root = tomllib.loads((REPO / "Cargo.toml").read_text(encoding="utf-8"))
    members = root.get("workspace", {}).get("members", [])
    report("core/*" in members, f'workspace.members 含 "core/*"（实际 {members}）')


def check_hard_rules() -> None:
    print("[4/7] 工程硬规则静态扫描（src/*.rs）")
    sources = sorted((CRATE / "src").glob("*.rs"))
    report(bool(sources), f"src/*.rs 存在（{len(sources)} 个文件）")

    unsafe_hits = [
        path.name for path in sources if UNSAFE.search(path.read_text(encoding="utf-8"))
    ]
    report(not unsafe_hits, f"无 unsafe 代码块（命中 {unsafe_hits}）")

    static_hits = [
        path.name
        for path in sources
        if STATIC_ITEM.search(path.read_text(encoding="utf-8"))
        or STATIC_FACADE.search(path.read_text(encoding="utf-8"))
    ]
    report(not static_hits, f"无全局 static / 静态门面 / 服务定位器（命中 {static_hits}）")

    use_hits = [
        path.name for path in sources if FORBIDDEN_USE.search(path.read_text(encoding="utf-8"))
    ]
    report(not use_hits, f"无宿主 / 网络 / 文件系统 / 密码学 use（命中 {use_hits}）")

    lib = (CRATE / "src" / "lib.rs").read_text(encoding="utf-8")
    missing = [name for name in PUBLIC_SURFACE if name not in lib]
    report(not missing, f"lib.rs 公开发布面完整（缺 {missing}）")


def check_test_coverage() -> None:
    print("[5/7] 单测覆盖（src/tests.rs）")
    tests = (CRATE / "src" / "tests.rs").read_text(encoding="utf-8")
    for name in sorted(REQUIRED_TESTS):
        report(bool(re.search(rf"fn\s+{re.escape(name)}\s*\(", tests)), f"存在测试 {name}()")


def strip_rust_code(text: str) -> str:
    """剥离注释、字符串与字符字面量，只留下参与定界的代码字符（用于括号配平核查）。"""
    out: list[str] = []
    i = 0
    n = len(text)
    while i < n:
        ch = text[i]
        nxt = text[i + 1] if i + 1 < n else ""
        if ch == "/" and nxt == "/":
            newline = text.find("\n", i)
            i = n if newline < 0 else newline
            continue
        if ch == "/" and nxt == "*":
            end = text.find("*/", i + 2)
            i = n if end < 0 else end + 2
            continue
        if ch == "r" and nxt in ('"', "#"):
            hashes = 0
            j = i + 1
            while j < n and text[j] == "#":
                hashes += 1
                j += 1
            if j < n and text[j] == '"':
                terminator = '"' + "#" * hashes
                end = text.find(terminator, j + 1)
                i = n if end < 0 else end + len(terminator)
                continue
        if ch == '"':
            i += 1
            while i < n:
                if text[i] == "\\":
                    i += 2
                    continue
                if text[i] == '"':
                    i += 1
                    break
                i += 1
            continue
        if ch == "'":
            lifetime = re.match(r"'[A-Za-z_][A-Za-z0-9_]*", text[i:])
            if lifetime is not None:
                end = i + len(lifetime.group(0))
                if text[end : end + 1] != "'":
                    i = end
                    continue
            i += 1
            i += 2 if i < n and text[i] == "\\" else 1
            if i < n and text[i] == "'":
                i += 1
            continue
        out.append(ch)
        i += 1
    return "".join(out)


def check_rust_structure() -> None:
    print("[6/7] Rust 结构（mod 文件 / include_str! 夹具 / 括号配平）")
    lib = (CRATE / "src" / "lib.rs").read_text(encoding="utf-8")

    declared_mods = re.findall(r"(?m)^\s*(?:pub\s+)?mod\s+([a-z_][a-z0-9_]*)\s*;", lib)
    missing_mods = [name for name in declared_mods if not (CRATE / "src" / f"{name}.rs").exists()]
    report(
        not missing_mods,
        f"mod 声明均有对应文件（{len(declared_mods)} 个：{declared_mods}；缺 {missing_mods}）",
    )

    referenced: list[str] = []
    for path in sorted((CRATE / "src").glob("*.rs")):
        referenced += re.findall(
            r'include_str!\(\s*concat!\(\s*env!\("CARGO_MANIFEST_DIR"\)\s*,\s*"([^"]+)"',
            path.read_text(encoding="utf-8"),
        )
    missing_fixtures = [rel for rel in referenced if not (CRATE / rel.lstrip("/")).exists()]
    report(
        not missing_fixtures and bool(referenced),
        f"include_str! 夹具均存在（{sorted(set(referenced))}；缺 {missing_fixtures}）",
    )

    unbalanced: list[str] = []
    for path in sorted((CRATE / "src").glob("*.rs")):
        code = strip_rust_code(path.read_text(encoding="utf-8"))
        counts = {pair: code.count(pair[0]) - code.count(pair[1]) for pair in ("()", "{}", "[]")}
        if any(value != 0 for value in counts.values()):
            unbalanced.append(f"{path.name}{counts}")
    report(not unbalanced, f"括号 / 花括号 / 方括号配平（不平衡 {unbalanced}）")


def contract_block(text: str, header: str, prefix: str = "pub ") -> str:
    match = re.search(rf"{prefix}{header} \{{(.*?)\n\}}", text, re.S)
    return match.group(1) if match else ""


def check_contract_api() -> None:
    print("[7/7] 跨 crate API 一致性（ab-contract 公开面）")
    contract = CONTRACT_LIB.read_text(encoding="utf-8")
    declared = set(
        re.findall(r"(?m)^pub (?:struct|enum|const|fn|trait)\s+([A-Za-z_][A-Za-z0-9_]*)", contract)
    )

    imported: set[str] = set()
    for path in sorted((CRATE / "src").glob("*.rs")):
        source = path.read_text(encoding="utf-8")
        for group in re.findall(r"use\s+ab_contract::\{([^}]*)\}", source):
            imported.update(part.strip() for part in group.split(",") if part.strip())
        imported.update(re.findall(r"use\s+ab_contract::([A-Za-z_][A-Za-z0-9_]*)\s*;", source))
    missing = sorted(imported - declared)
    report(not missing, f"import 的 ab-contract 类型均存在（{sorted(imported)}；缺 {missing}）")

    for type_name, variants in USED_ENUM_VARIANTS.items():
        body = contract_block(contract, f"enum {type_name}")
        existing = set(re.findall(r"(?m)^\s+([A-Z][A-Za-z0-9_]*)\s*,", body))
        absent = [variant for variant in variants if variant not in existing]
        report(not absent, f"{type_name} 枚举变体存在（缺 {absent}）")

    for type_name, fields in USED_STRUCT_FIELDS.items():
        body = contract_block(contract, f"struct {type_name}")
        existing = set(re.findall(r"(?m)^\s*pub\s+([a-z_][a-z0-9_]*)\s*:", body))
        absent = [field for field in fields if field not in existing]
        report(not absent, f"{type_name} 字段存在（缺 {absent}）")

    for type_name, method in USED_METHODS:
        body = contract_block(contract, f"impl {type_name}", prefix="")
        report(f"pub fn {method}" in body, f"{type_name}::{method}() 存在")


def main() -> int:
    print(f"crate = {CRATE}")
    check_fixtures_against_schemas()
    check_manifest()
    check_workspace_membership()
    check_hard_rules()
    check_test_coverage()
    check_rust_structure()
    check_contract_api()
    if failures:
        print(f"\n结果：{len(failures)} 项失败")
        for item in failures:
            print(f"  - {item}")
        return 1
    print("\n结果：全部通过")
    return 0


if __name__ == "__main__":
    sys.exit(main())
