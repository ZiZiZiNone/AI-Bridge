#!/usr/bin/env python3
"""AI-Bridge 转换器打包工具（tools/package-converter.py）。

把 converters/<id>/ 目录打包为发布目录或 zip（--out 以 .zip 结尾即 zip，否则目录）。
打包前**先运行 tools/check-contracts.py --data <manifest.json>**（复用同一套契约规则，
含占位签名拒载：docs/安全模型.md §6），校验不过**拒绝打包**。

打包内容 = 源目录全部文件（递归，含 manifest.json + manifest.entry 指向的模块 +
tests/ 样例）。manifest.entry 指向的模块文件缺失直接 FAIL（发布包必须含模块）；
tests/ 缺失只 WARN（docs 未规定其为打包必填，标注待确认）。

待确认：docs/目录结构.md §4 只定义了转换器源目录构成，未规定发布包是否裁剪
extractor.* / normalizer.* 等源码；本脚本整目录收录（最小实现，不静默丢文件），
是否裁剪待需求明确。

用法：
    python tools/package-converter.py --src converters/qq --out dist/qq-1.2.0
    python tools/package-converter.py --src converters/qq --out dist/qq-1.2.0.zip

退出码：0 = 打包成功；1 = 契约校验不过 / 缺文件 / 输出已存在 / 打包错误。
输出卫生：只打印文件清单与校验结果，不打印任何文件内容。
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

TOOLS_DIR = Path(__file__).resolve().parent
CHECK_CONTRACTS = TOOLS_DIR / "check-contracts.py"


def run_check_contracts(manifest: Path) -> tuple[bool, str]:
    proc = subprocess.run(
        [sys.executable, str(CHECK_CONTRACTS), "--data", str(manifest)],
        capture_output=True, text=True, encoding="utf-8", errors="replace",
    )
    return proc.returncode == 0, (proc.stdout or "") + (proc.stderr or "")


def collect_files(src: Path) -> list[Path]:
    return sorted(p for p in src.rglob("*") if p.is_file())


def package(src: Path, out: Path) -> list[str]:
    """整目录收录；返回包内相对路径清单。"""
    files = collect_files(src)
    names = [p.relative_to(src).as_posix() for p in files]
    if out.suffix.lower() == ".zip":
        out.parent.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as zf:
            for p, name in zip(files, names):
                zf.write(p, name)
    else:
        out.mkdir(parents=True)
        for p, name in zip(files, names):
            target = out / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(p.read_bytes())
    return names


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="package-converter.py",
        description="打包 converters/<id>/ 为发布目录或 zip；check-contracts 不过则拒绝打包",
    )
    parser.add_argument("--src", help="转换器目录（含 manifest.json；必填，--self-test 除外）")
    parser.add_argument("--out", help="输出路径：.zip 结尾=zip，否则发布目录（必填，--self-test 除外）")
    parser.add_argument("--self-test", action="store_true", help="运行最小自检后退出")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()
    if args.src is None or args.out is None:
        parser.error("--src 与 --out 必填（--self-test 除外）")

    src = Path(args.src).resolve()
    out = Path(args.out).resolve()
    if not src.is_dir():
        print(f"[FAIL] 转换器目录不存在：{src}")
        return 1
    manifest = src / "manifest.json"
    if not manifest.is_file():
        print(f"[FAIL] 缺少 {manifest}")
        return 1
    if out == src or src in out.parents:
        print(f"[FAIL] 输出路径不能位于源目录内：{out}")
        return 1
    if out.exists():
        print(f"[FAIL] 输出已存在，拒绝覆盖：{out}")
        return 1

    # 打包门禁：先跑 check-contracts（同一套规则），不过拒绝打包。
    ok, output = run_check_contracts(manifest)
    print(output.rstrip())
    if not ok:
        print(f"[FAIL] 契约校验不过，拒绝打包：{manifest}")
        return 1

    try:
        doc = json.loads(manifest.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as err:
        print(f"[FAIL] manifest 读取失败：{err}")
        return 1
    entry = doc.get("entry") if isinstance(doc, dict) else None
    if not isinstance(entry, str) or not entry or not (src / entry).is_file():
        print(f"[FAIL] manifest.entry 指向的模块文件缺失：{entry!r}（发布包必须含模块）")
        return 1
    if not (src / "tests").is_dir():
        print("[WARN] 待确认：源目录无 tests/ 样例（docs/目录结构.md §4 列出 tests/，但未规定为打包必填）")

    names = package(src, out)
    for name in names:
        print(f"[PASS] 打包：{name}")
    print(f"[PASS] 输出：{out}（{len(names)} 个文件）")
    print(f"结果：PASS（PASS {len(names) + 1} / FAIL 0）")
    return 0


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
        src = root / "sample-app"
        (src / "tests").mkdir(parents=True)
        (src / "converter.wasm").write_bytes(b"\x00asm-fake")
        (src / "tests" / "sample.json").write_text("{}", encoding="utf-8")
        manifest = {
            "id": "sample-app", "name": "Sample", "version": "0.1.0", "level": "c",
            "readable_paths": ["{APPDATA}\\sample-app"], "entry": "converter.wasm",
            "module_sha256": "3f2a9c1d5b7e4088a1c2d3e4f5061728394a5b6c7d8e9f001122334455667788",
            "signature": {"algorithm": "ed25519", "signer": "official", "value": "ab" * 64},
        }
        manifest_path = src / "manifest.json"
        manifest_path.write_text(json.dumps(manifest, ensure_ascii=False), encoding="utf-8")

        # 目录打包
        out_dir = root / "dist" / "sample-app"
        expect(main(["--src", str(src), "--out", str(out_dir)]) == 0, "目录打包应成功")
        expect({p.relative_to(out_dir).as_posix() for p in out_dir.rglob("*") if p.is_file()}
               == {"manifest.json", "converter.wasm", "tests/sample.json"},
               "目录打包内容不正确")

        # zip 打包
        out_zip = root / "dist" / "sample-app.zip"
        expect(main(["--src", str(src), "--out", str(out_zip)]) == 0, "zip 打包应成功")
        with zipfile.ZipFile(out_zip) as zf:
            expect(set(zf.namelist()) == {"manifest.json", "converter.wasm", "tests/sample.json"},
                   "zip 打包内容不正确")

        # 输出已存在 → 拒绝
        expect(main(["--src", str(src), "--out", str(out_dir)]) == 1, "输出已存在应拒绝")

        # 占位签名 → 契约校验不过，拒绝打包
        bad = dict(manifest)
        bad["signature"] = {"algorithm": "ed25519", "signer": "x", "value": "0" * 128}
        manifest_path.write_text(json.dumps(bad, ensure_ascii=False), encoding="utf-8")
        refused = root / "dist" / "refused"
        expect(main(["--src", str(src), "--out", str(refused)]) == 1, "占位签名应拒绝打包")
        expect(not refused.exists(), "拒绝打包时不得产生输出")

        # 模块缺失 → 拒绝（先把签名改回合法）
        good = dict(manifest)
        manifest_path.write_text(json.dumps(good, ensure_ascii=False), encoding="utf-8")
        (src / "converter.wasm").unlink()
        expect(main(["--src", str(src), "--out", str(root / "dist" / "no-module")]) == 1,
               "模块缺失应拒绝")
    finally:
        shutil.rmtree(root, ignore_errors=True)

    if fails:
        for f in fails:
            print(f"[FAIL] self-test：{f}")
        print(f"结果：FAIL（自检 {len(fails)} 项不过）")
        return 1
    print(f"结果：PASS（self-test {count[0]} 项断言全过）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
