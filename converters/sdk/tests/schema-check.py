#!/usr/bin/env python3
"""转换器清单与产物的契约一致性校验（Python 3.11 + jsonschema）。

规则唯一来源是 contracts/*.schema.json：本脚本不重复实现任何字段校验，
只把文件喂给权威校验器，并报告 JSON 路径与失败原因。

用法：
    python converters/sdk/tests/schema-check.py [转换器目录]
默认目标：converters/_template
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys
from datetime import datetime

from jsonschema import Draft202012Validator, FormatChecker

REPO_ROOT = pathlib.Path(__file__).resolve().parents[3]
SCHEMAS = {
    "manifest": REPO_ROOT / "contracts" / "converter-manifest.schema.json",
    "standard_access": REPO_ROOT / "contracts" / "standard-access.schema.json",
}

# schema 里 format: date-time 的语义是 RFC 3339；本机缺 rfc3339-validator 时该 format 不生效，
# 这里按同一语义用标准库复核，避免"校验通过"名不副实。
RFC3339 = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$")


def check_rfc3339(value) -> bool:
    if not isinstance(value, str):
        return True  # 类型错误由 schema 的 type 断言负责
    if not RFC3339.match(value):
        return False
    try:
        datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return False
    return True


def build_format_checker() -> tuple[FormatChecker, bool]:
    # FormatChecker() 复制一份全局已注册的 format 检查器，实例上的注册不污染全局
    checker = FormatChecker()
    native = "date-time" in checker.checkers
    if not native:
        checker.checks("date-time")(check_rfc3339)
    return checker, native


def load_json(path: pathlib.Path):
    raw = path.read_bytes()
    if raw.startswith(b"\xef\xbb\xbf"):
        raise ValueError(f"{path.name} 带 UTF-8 BOM；SDK 约定 JSON 必须 UTF-8 无 BOM")
    return json.loads(raw.decode("utf-8"))


def json_path(error) -> str:
    parts = ["$"]
    for step in error.absolute_path:
        parts.append(f"[{step}]" if isinstance(step, int) else f".{step}")
    return "".join(parts)


def validate(schema_key: str, instance_path: pathlib.Path, format_checker: FormatChecker) -> list[str]:
    schema = load_json(SCHEMAS[schema_key])
    validator = Draft202012Validator(schema, format_checker=format_checker)
    instance = load_json(instance_path)
    problems = []
    for error in sorted(validator.iter_errors(instance), key=lambda e: list(e.absolute_path)):
        problems.append(f"{error.message}（{json_path(error)}）")
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description="转换器夹具契约校验")
    parser.add_argument("converter", nargs="?", default="converters/_template", help="转换器目录")
    args = parser.parse_args()

    converter_dir = pathlib.Path(args.converter)
    if not converter_dir.is_absolute():
        converter_dir = REPO_ROOT / converter_dir

    targets: list[tuple[str, pathlib.Path]] = [
        ("manifest", converter_dir / "manifest.json"),
    ]
    expected_dir = converter_dir / "tests" / "fixtures" / "expected"
    if expected_dir.is_dir():
        targets += [("standard_access", path) for path in sorted(expected_dir.glob("*.json"))]

    format_checker, native_format = build_format_checker()

    print("转换器契约一致性校验（jsonschema draft 2020-12）")
    print(f"  仓库根    : {REPO_ROOT}")
    print(f"  目标目录  : {converter_dir.relative_to(REPO_ROOT).as_posix()}")
    print(f"  date-time : {'库校验器（rfc3339-validator）' if native_format else 'SDK 内建 RFC 3339 复核（本机缺 rfc3339-validator）'}")

    failed = 0
    for schema_key, path in targets:
        if not path.is_file():
            print(f"  FAIL  {path.name}：文件不存在")
            failed += 1
            continue
        label = path.relative_to(REPO_ROOT).as_posix()
        try:
            problems = validate(schema_key, path, format_checker)
        except ValueError as error:
            print(f"  FAIL  {label}：{error}")
            failed += 1
            continue
        if problems:
            failed += 1
            print(f"  FAIL  {label}（{SCHEMAS[schema_key].name}）")
            for problem in problems:
                print(f"        - {problem}")
        else:
            print(f"  PASS  {label}（{SCHEMAS[schema_key].name}）")

    print(f"\n结果：{len(targets)} 个文件，{failed} 个失败")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
