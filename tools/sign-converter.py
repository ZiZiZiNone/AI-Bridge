#!/usr/bin/env python3
"""AI-Bridge 转换器签名工具（tools/sign-converter.py）—— 离线发布链一环。

⚠ 签名能力边界（务必先读）：
  Python 标准库没有 ed25519 签名/验签实现；密码学实现不得手写（任务约束）。
  因此本脚本**不签名、不接触任何私钥**，只实现到：
    ① 计算模块 module_sha256（entry 模块字节的 SHA-256，hex 小写）；
    ② 产出「manifest 规范化字节 + module_sha256」待签名字节。
  真正的签名步骤需**外部 ed25519 工具**（如 openssl / ssh-keygen；具体命令未验证，
  见 tools/README.md「未验证项」）。签名产出后，用本脚本 --signer + --signature-hex
  回填 manifest.json。回填只做格式/占位校验（复用 tools/check-contracts.py --data，
  占位签名拒载），**不验签**；验签由 core/converter-host 的 SignatureVerifier
  在加载时执行（docs/安全模型.md §6）。

签名覆盖范围（docs/安全模型.md §6）：
  「签名覆盖：manifest 规范化字节 + module_sha256（WASM 字节摘要，F3）」
  ——签名绑定模块本体摘要，防「换模块保清单」；占位/全零签名一律拒载。

⚠ 待确认（docs/ 与 core/ 均未定义字节构造，采用最小实现）：
  core/converter-host/src/traits.rs:10-11 的 SignatureVerifier 只声明覆盖义务
  （「签名覆盖清单全部字段（含 module_sha256）且非占位」），verify() 收到的是
  反序列化后的类型而非原始 JSON 字节，**「manifest 规范化字节」的确切定义在仓库内
  无事实源**。本脚本采用：
      canonical = UTF-8(json.dumps(manifest 去掉 signature 字段、
                                   module_sha256 置为本次计算值,
                                   sort_keys=True, ensure_ascii=False,
                                   separators=(",", ":")))
      待签名字节 = canonical + module_sha256_hex 的 ASCII 字节
  与宿主验签实现落地时必须对齐，否则签名互不认；对齐后只需改
  canonical_manifest_bytes() 一处。

用法：
    # ① 产出待签名字节（不改 manifest.json）
    python tools/sign-converter.py --manifest converters/qq/manifest.json \
        [--module converters/qq/qq.wasm] [--out-bytes qq.presign.bin]

    # ② 外部 ed25519 工具对 <out-bytes> 签名（私钥只在该工具内使用）

    # ③ 回填外部签名（算法固定 ed25519；同时写入计算出的 module_sha256）
    python tools/sign-converter.py --manifest converters/qq/manifest.json \
        --signer aibridge-official-2026 --signature-hex <128 位 hex>

退出码：0 = 成功；1 = 输入/校验失败。
输出卫生：不打印任何秘密材料（本脚本不读私钥；打印的摘要与签名值均为公开值）。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

TOOLS_DIR = Path(__file__).resolve().parent
CHECK_CONTRACTS = TOOLS_DIR / "check-contracts.py"


# ---------------- 核心变换 ----------------

def sha256_hex(data: bytes) -> str:
    """模块摘要：core/contract 的 module_sha256 = entry 字节 SHA-256（hex 小写）。"""
    return hashlib.sha256(data).hexdigest()


def canonical_manifest_bytes(manifest: dict, module_hash: str) -> bytes:
    """「manifest 规范化字节 + module_sha256」——见文件头「待确认」。"""
    body = {k: v for k, v in manifest.items() if k != "signature"}
    body["module_sha256"] = module_hash
    canonical = json.dumps(
        body, sort_keys=True, ensure_ascii=False, separators=(",", ":")
    ).encode("utf-8")
    return canonical + module_hash.encode("ascii")


def load_json(path: Path) -> dict:
    doc = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(doc, dict):
        raise ValueError(f"{path} 顶层必须是 JSON 对象")
    return doc


def resolve_module(manifest_path: Path, manifest: dict, override: str | None) -> Path:
    if override:
        return Path(override)
    entry = manifest.get("entry")
    if not isinstance(entry, str) or not entry:
        raise ValueError("manifest.entry 缺失，无法定位模块文件（可用 --module 显式指定）")
    return manifest_path.parent / entry


def run_check_contracts(data_path: Path) -> tuple[bool, str]:
    """复用 tools/check-contracts.py 的同一套规则（不另写第二份）。"""
    proc = subprocess.run(
        [sys.executable, str(CHECK_CONTRACTS), "--data", str(data_path)],
        capture_output=True, text=True, encoding="utf-8", errors="replace",
    )
    return proc.returncode == 0, (proc.stdout or "") + (proc.stderr or "")


# ---------------- 两种模式 ----------------

def emit_mode(manifest_path: Path, module_path: Path, out_bytes: Path) -> int:
    items: list[tuple[str, str]] = []
    if not manifest_path.is_file():
        print(f"[FAIL] manifest 不存在：{manifest_path}")
        return 1
    if not module_path.is_file():
        print(f"[FAIL] 模块文件不存在：{module_path}")
        return 1
    try:
        manifest = load_json(manifest_path)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError, ValueError) as err:
        print(f"[FAIL] manifest 读取失败：{err}")
        return 1

    module = module_path.read_bytes()
    module_hash = sha256_hex(module)
    items.append(("PASS", f"模块：{module_path}（{len(module)} 字节）"))
    items.append(("PASS", f"module_sha256：{module_hash}"))
    current = manifest.get("module_sha256")
    if isinstance(current, str) and current and current != module_hash:
        items.append(("WARN", "manifest.module_sha256 与本次计算值不一致（回填签名时将写入计算值）"))
    elif current == module_hash:
        items.append(("PASS", "manifest.module_sha256 与本次计算值一致"))

    pre_sign = canonical_manifest_bytes(manifest, module_hash)
    out_bytes.parent.mkdir(parents=True, exist_ok=True)
    out_bytes.write_bytes(pre_sign)
    items.append(("PASS", f"待签名字节：{out_bytes}（{len(pre_sign)} 字节）"))
    items.append(("PASS", f"待签名字节 SHA-256（供外部工具核对）：{sha256_hex(pre_sign)}"))

    for level, msg in items:
        print(f"[{level}] {msg}")
    print(f"结果：PASS（PASS {len(items)} / FAIL 0）")
    print("⚠ 本脚本不签名：签名步骤需外部 ed25519 工具对上述待签名字节文件签名；")
    print(f"  签好后回填：python tools/sign-converter.py --manifest {manifest_path} "
          "--signer <签名者ID> --signature-hex <128 位 hex>")
    return 0


def apply_mode(manifest_path: Path, module_path: Path,
               signer: str, signature_hex: str) -> int:
    if not manifest_path.is_file():
        print(f"[FAIL] manifest 不存在：{manifest_path}")
        return 1
    if not module_path.is_file():
        print(f"[FAIL] 模块文件不存在：{module_path}")
        return 1
    try:
        manifest = load_json(manifest_path)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError, ValueError) as err:
        print(f"[FAIL] manifest 读取失败：{err}")
        return 1

    signature_hex = signature_hex.strip().lower()
    updated = dict(manifest)
    updated["module_sha256"] = sha256_hex(module_path.read_bytes())
    updated["signature"] = {
        "algorithm": "ed25519",
        "signer": signer,
        "value": signature_hex,
    }

    # 先写临时文件，经 check-contracts（同一套规则，含占位签名拒载）通过后才替换。
    # 不用 tempfile.mkstemp：其 0o600 建文件在本机产生写入被拒的 ACL。
    tmp_path = manifest_path.parent / f".manifest.{os.getpid()}.tmp"
    try:
        tmp_path.write_text(
            json.dumps(updated, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
        ok, output = run_check_contracts(tmp_path)
        if not ok:
            print("[FAIL] 更新后的 manifest 未通过 check-contracts，拒绝写回：")
            print(output.rstrip())
            return 1
        os.replace(tmp_path, manifest_path)
    finally:
        if tmp_path.exists():
            tmp_path.unlink()

    print(f"[PASS] 已写回 manifest：{manifest_path}")
    print(f"[PASS] module_sha256：{updated['module_sha256']}")
    print("[PASS] signature：algorithm=ed25519（回填外部签名；本脚本未验签，"
          "验签由 core/converter-host 加载时执行）")
    print("结果：PASS")
    return 0


# ---------------- 自检 ----------------

def self_test_scratch() -> Path:
    """自检暂存目录：默认 tools/.self-test-tmp，可用环境变量 TOOLS_SCRATCH 覆盖。
    不用 tempfile.mkdtemp：其 0o700 建目录在本机产生后续写入被拒的 ACL。"""
    base = Path(os.environ.get("TOOLS_SCRATCH") or (TOOLS_DIR / ".self-test-tmp"))
    base.mkdir(parents=True, exist_ok=True)
    scratch = base / f"run-{os.getpid()}"
    if scratch.exists():
        shutil.rmtree(scratch, ignore_errors=True)
    scratch.mkdir()
    return scratch


def self_test() -> int:
    fails: list[str] = []
    count = [0]

    def expect(cond: bool, msg: str) -> None:
        count[0] += 1
        if not cond:
            fails.append(msg)

    root = self_test_scratch()
    try:
        module_path = root / "converter.wasm"
        module_bytes = b"\x00asm-fake-module-bytes-for-self-test"
        module_path.write_bytes(module_bytes)
        expect(sha256_hex(module_bytes) == hashlib.sha256(module_bytes).hexdigest(),
               "sha256_hex 与 hashlib 不一致")

        manifest = {
            "id": "sample-app", "name": "Sample", "version": "0.1.0", "level": "c",
            "readable_paths": ["{APPDATA}\\sample-app"], "entry": "converter.wasm",
            "module_sha256": "0" * 64,
            "signature": {"algorithm": "ed25519", "signer": "x", "value": "0" * 128},
        }
        manifest_path = root / "manifest.json"
        manifest_path.write_text(json.dumps(manifest, ensure_ascii=False), encoding="utf-8")

        module_hash = sha256_hex(module_bytes)
        canon = canonical_manifest_bytes(manifest, module_hash)
        # 签名字段不得进入待签名字节；module_sha256 必须在场（含尾部拼接）。
        expect(b'"signature"' not in canon, "signature 字段不应进入待签名字节")
        expect(canon.endswith(module_hash.encode("ascii")), "待签名字节尾部应为 module_sha256")
        # 键序无关：规范化必须确定。
        reordered = dict(reversed(list(manifest.items())))
        expect(canonical_manifest_bytes(reordered, module_hash) == canon,
               "键序不同应产出相同待签名字节")

        # ① 产出模式
        out_bytes = root / "m.presign.bin"
        expect(emit_mode(manifest_path, module_path, out_bytes) == 0, "emit_mode 应成功")
        expect(out_bytes.is_file() and out_bytes.read_bytes() == canon,
               "emit_mode 产出的待签名字节应与 canonical 一致")

        # ② 回填模式（check-contracts 复核通过 → 写回）
        sig_hex = "ab" * 64
        expect(apply_mode(manifest_path, module_path, "official", sig_hex) == 0,
               "apply_mode 应成功")
        written = load_json(manifest_path)
        expect(written["signature"] == {"algorithm": "ed25519", "signer": "official",
                                        "value": sig_hex},
               "写回的 signature 不正确")
        expect(written["module_sha256"] == module_hash, "写回的 module_sha256 不正确")

        # ③ 占位签名必须被拒（复用 check-contracts 的占位拒载规则）
        expect(apply_mode(manifest_path, module_path, "official", "0" * 128) == 1,
               "占位签名应回填失败")
    finally:
        shutil.rmtree(root, ignore_errors=True)

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
        prog="sign-converter.py",
        description="转换器签名链：产出待签名字节（不签名）；可回填外部 ed25519 签名",
    )
    parser.add_argument("--manifest", help="manifest.json 路径（必填，--self-test 除外）")
    parser.add_argument("--module", help="模块文件路径（默认取 manifest.entry 同目录）")
    parser.add_argument("--out-bytes", help="待签名字节输出路径（默认 <manifest>.presign.bin）")
    parser.add_argument("--signer", help="签名者标识（回填模式，与 --signature-hex 同用）")
    parser.add_argument("--signature-hex", help="外部 ed25519 签名值（128 位 hex，回填模式）")
    parser.add_argument("--self-test", action="store_true", help="运行最小自检后退出")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()
    if args.manifest is None:
        parser.error("--manifest 必填（--self-test 除外）")

    manifest_path = Path(args.manifest)
    try:
        if not manifest_path.is_file():
            raise FileNotFoundError(f"manifest 不存在：{manifest_path}")
        manifest = load_json(manifest_path)
        module_path = resolve_module(manifest_path, manifest, args.module)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError, ValueError) as err:
        print(f"[FAIL] {err}")
        return 1

    if (args.signer is None) != (args.signature_hex is None):
        print("[FAIL] --signer 与 --signature-hex 必须同时给出（回填模式），"
              "或都不给（产出待签名字节模式）")
        return 1
    if args.signer is not None:
        return apply_mode(manifest_path, module_path, args.signer, args.signature_hex)
    out_bytes = Path(args.out_bytes) if args.out_bytes else manifest_path.with_suffix(".presign.bin")
    return emit_mode(manifest_path, module_path, out_bytes)


if __name__ == "__main__":
    sys.exit(main())
