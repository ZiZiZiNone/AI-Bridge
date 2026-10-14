#!/usr/bin/env python3
"""转换器清单哈希填充、规范化签名字节生成与 Ed25519 签名/验签工具（`converters/sdk/README.md` §4）。

职责：
1. 计算转换器入口模块的 `module_sha256`（64 位小写 hex）；
2. 按唯一契约规则生成待签名规范化字节（`canonical_signing_bytes` 的逐字节 Python 镜像）：
   去掉 `signature` 字段后，按 JSON 键升序紧凑序列化（`separators=(",", ":")`，`ensure_ascii=False`），UTF-8 编码；
3. 支持直接用 Ed25519 私钥签名（`--private-key-hex`）或用公钥验签（`--verify`），默认对齐 `core/contract` 内置官方公钥；
4. `--self-test` 校验本脚本产出与 `core/contract/src/lib.rs` 基准向量及 Ed25519 往返验签逐字节一致。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import sys
from typing import Any

OFFICIAL_SIGNER_ID = "aibridge-official-2026"
OFFICIAL_CONVERTER_PUBKEY_HEX = (
    "2c6a2895c4b3fce893ca6626c15c9d08dcf4a9a1b3f7d42d9346e8dd4203afd1"
)

ALLOWED_MANIFEST_KEYS = {
    "id",
    "name",
    "version",
    "level",
    "readable_paths",
    "entry",
    "module_sha256",
    "signature",
    "min_app_version",
}

REQUIRED_SIGNING_KEYS = {
    "id",
    "name",
    "version",
    "level",
    "readable_paths",
    "entry",
    "module_sha256",
}


def canonical_signing_bytes(manifest: dict[str, Any]) -> bytes:
    """与 `ab_contract::canonical_signing_bytes` 逐字节一致的待签名字节生成。"""
    unknown = set(manifest.keys()) - ALLOWED_MANIFEST_KEYS
    if unknown:
        raise ValueError(f"manifest 存在未定义的额外字段：{sorted(unknown)}")
    missing = REQUIRED_SIGNING_KEYS - set(manifest.keys())
    if missing:
        raise ValueError(f"manifest 缺少必填字段：{sorted(missing)}")

    unsigned: dict[str, Any] = {
        "entry": str(manifest["entry"]),
        "id": str(manifest["id"]),
        "level": str(manifest["level"]),
        "module_sha256": str(manifest["module_sha256"]),
        "name": str(manifest["name"]),
        "readable_paths": [str(p) for p in manifest["readable_paths"]],
        "version": str(manifest["version"]),
    }
    if manifest.get("min_app_version") is not None:
        unsigned["min_app_version"] = str(manifest["min_app_version"])

    text = json.dumps(
        unsigned,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
    )
    return text.encode("utf-8")


def compute_module_sha256(entry_path: pathlib.Path) -> str:
    return hashlib.sha256(entry_path.read_bytes()).hexdigest().lower()


def sign_manifest_ed25519(manifest: dict[str, Any], private_key_hex: str) -> str:
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

    raw_sk = bytes.fromhex(private_key_hex.strip())
    if len(raw_sk) != 32:
        raise ValueError("Ed25519 私钥种子必须为 32 字节（64 位 hex）")
    sk = Ed25519PrivateKey.from_private_bytes(raw_sk)
    payload = canonical_signing_bytes(manifest)
    return sk.sign(payload).hex().lower()


def verify_manifest_ed25519(
    manifest: dict[str, Any],
    pubkey_hex: str = OFFICIAL_CONVERTER_PUBKEY_HEX,
    expected_signer: str = OFFICIAL_SIGNER_ID,
) -> None:
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey

    sig_obj = manifest.get("signature")
    if not isinstance(sig_obj, dict):
        raise ValueError("manifest.signature 必须为对象")
    if sig_obj.get("algorithm") != "ed25519":
        raise ValueError(f"不支持的签名算法：{sig_obj.get('algorithm')}")
    if sig_obj.get("signer") != expected_signer:
        raise ValueError(
            f"未知签名者：{sig_obj.get('signer')}（期望 {expected_signer}）"
        )
    sig_hex = str(sig_obj.get("value", ""))
    if len(sig_hex) != 128 or all(ch == "0" for ch in sig_hex):
        raise ValueError("manifest.signature.value 非法或为全零占位值")

    pk = Ed25519PublicKey.from_public_bytes(bytes.fromhex(pubkey_hex.strip()))
    payload = canonical_signing_bytes(manifest)
    pk.verify(bytes.fromhex(sig_hex), payload)


def run_self_test() -> int:
    sample_with_min = {
        "id": "qq",
        "name": "QQ 本地数据转换器",
        "version": "1.2.0",
        "level": "c",
        "readable_paths": ["C:\\Users\\me\\AppData\\Roaming\\QQ"],
        "entry": "qq.wasm",
        "module_sha256": "3f2a9c1d5b7e4088a1c2d3e4f5061728394a5b6c7d8e9f001122334455667788",
        "signature": {
            "algorithm": "ed25519",
            "signer": OFFICIAL_SIGNER_ID,
            "value": "0" * 128,
        },
        "min_app_version": "0.1.0",
    }
    expected_with_min = (
        '{"entry":"qq.wasm","id":"qq","level":"c","min_app_version":"0.1.0",'
        '"module_sha256":"3f2a9c1d5b7e4088a1c2d3e4f5061728394a5b6c7d8e9f001122334455667788",'
        '"name":"QQ 本地数据转换器","readable_paths":["C:\\\\Users\\\\me\\\\AppData\\\\Roaming\\\\QQ"],"version":"1.2.0"}'
    ).encode("utf-8")

    actual_with_min = canonical_signing_bytes(sample_with_min)
    if actual_with_min != expected_with_min:
        sys.stderr.write(
            f"[FAIL] 含 min_app_version 基准向量不匹配:\n  actual  ={actual_with_min!r}\n  expected={expected_with_min!r}\n"
        )
        return 1

    sample_without_min = dict(sample_with_min)
    sample_without_min.pop("min_app_version")
    expected_without_min = (
        '{"entry":"qq.wasm","id":"qq","level":"c",'
        '"module_sha256":"3f2a9c1d5b7e4088a1c2d3e4f5061728394a5b6c7d8e9f001122334455667788",'
        '"name":"QQ 本地数据转换器","readable_paths":["C:\\\\Users\\\\me\\\\AppData\\\\Roaming\\\\QQ"],"version":"1.2.0"}'
    ).encode("utf-8")

    actual_without_min = canonical_signing_bytes(sample_without_min)
    if actual_without_min != expected_without_min:
        sys.stderr.write(
            f"[FAIL] 不含 min_app_version 基准向量不匹配:\n  actual  ={actual_without_min!r}\n  expected={expected_without_min!r}\n"
        )
        return 1

    repo_root = pathlib.Path(__file__).resolve().parent.parent
    contract_rs = (repo_root / "core" / "contract" / "src" / "lib.rs").read_text(
        encoding="utf-8"
    )
    if "pub fn canonical_signing_bytes(manifest: &ConverterManifest) -> Vec<u8>" not in contract_rs:
        sys.stderr.write(
            "[FAIL] core/contract/src/lib.rs 未导出 canonical_signing_bytes\n"
        )
        return 1
    if OFFICIAL_CONVERTER_PUBKEY_HEX not in contract_rs:
        sys.stderr.write(
            "[FAIL] core/contract/src/lib.rs 的 OFFICIAL_CONVERTER_PUBKEY_HEX 与脚本不一致\n"
        )
        return 1

    # 验证 Ed25519 签名与官方公钥验签闭环
    test_seed = "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90"
    sig_hex = sign_manifest_ed25519(sample_with_min, test_seed)
    sample_with_min["signature"]["value"] = sig_hex
    verify_manifest_ed25519(
        sample_with_min,
        pubkey_hex=OFFICIAL_CONVERTER_PUBKEY_HEX,
        expected_signer=OFFICIAL_SIGNER_ID,
    )

    print(
        "[PASS] tools/sign-converter.py 自测通过（与 ab_contract::canonical_signing_bytes 基准向量及内置官方公钥互锁）"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="计算转换器 module_sha256、导出规范化待签名字节并支持 Ed25519 签名/验签"
    )
    parser.add_argument(
        "converter_dir",
        nargs="?",
        help="转换器目录路径（含 manifest.json 与入口模块）",
    )
    parser.add_argument(
        "--update-hash",
        action="store_true",
        help="将计算出的入口模块 SHA-256 写回 manifest.json",
    )
    parser.add_argument(
        "--out-signing-bytes",
        type=pathlib.Path,
        help="将待签名规范化字节写入指定文件",
    )
    parser.add_argument(
        "--private-key-hex",
        help="使用给定的 32 字节 Ed25519 私钥种子（64 位 hex）签名并写回 manifest.json",
    )
    parser.add_argument(
        "--signer",
        default=OFFICIAL_SIGNER_ID,
        help=f"签名者 ID（默认 {OFFICIAL_SIGNER_ID}）",
    )
    parser.add_argument(
        "--verify",
        action="store_true",
        help="使用官方公钥（或 --pubkey-hex）校验 manifest.json 的 Ed25519 签名",
    )
    parser.add_argument(
        "--pubkey-hex",
        default=OFFICIAL_CONVERTER_PUBKEY_HEX,
        help="验签公钥（64 位 hex，默认内置官方公钥）",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="运行与 core/contract 基准向量及官方公钥的互锁自测",
    )
    args = parser.parse_args()

    if args.self_test:
        return run_self_test()

    if not args.converter_dir:
        parser.error("必须提供 converter_dir 或使用 --self-test")

    conv_dir = pathlib.Path(args.converter_dir).resolve()
    manifest_path = conv_dir / "manifest.json"
    if not manifest_path.is_file():
        sys.stderr.write(f"错误：未找到清单文件 {manifest_path}\n")
        return 1

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    entry_rel = manifest.get("entry")
    if not isinstance(entry_rel, str) or not entry_rel:
        sys.stderr.write("错误：manifest.entry 必须为非空字符串\n")
        return 1

    entry_path = (conv_dir / entry_rel).resolve()
    if not str(entry_path).startswith(str(conv_dir)):
        sys.stderr.write("错误：manifest.entry 越出转换器目录边界\n")
        return 1
    if not entry_path.is_file():
        sys.stderr.write(f"错误：入口模块不存在 {entry_path}\n")
        return 1

    module_sha256 = compute_module_sha256(entry_path)
    if args.update_hash:
        manifest["module_sha256"] = module_sha256

    if args.private_key_hex:
        manifest["module_sha256"] = module_sha256
        sig_hex = sign_manifest_ed25519(manifest, args.private_key_hex)
        manifest["signature"] = {
            "algorithm": "ed25519",
            "signer": args.signer,
            "value": sig_hex,
        }
        manifest_path.write_text(
            json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )
    elif args.update_hash:
        manifest_path.write_text(
            json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )

    if args.verify:
        verify_manifest_ed25519(
            manifest,
            pubkey_hex=args.pubkey_hex,
            expected_signer=args.signer,
        )

    signing_bytes = canonical_signing_bytes(manifest)
    signing_digest = hashlib.sha256(signing_bytes).hexdigest().lower()

    if args.out_signing_bytes:
        args.out_signing_bytes.write_bytes(signing_bytes)

    print(f"converter_id         : {manifest['id']}")
    print(f"module_sha256        : {module_sha256}")
    print(f"signing_bytes_len    : {len(signing_bytes)}")
    print(f"signing_bytes_sha256 : {signing_digest}")
    print(f"signing_bytes_utf8   : {signing_bytes.decode('utf-8')}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
