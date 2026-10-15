#!/usr/bin/env python3
"""cargo 依赖离线供应商工具（fetch-vendor）。

背景：本机沙箱的 schannel（cargo/curl 的 TLS 栈）无法获取凭证
（SEC_E_NO_CREDENTIALS），cargo 无法直连 crates.io；OpenSSL 栈（Python/Node）正常。
方案：用 Python 按 sparse index 解析依赖版本闭包 → 下载 .crate →
解包为 cargo directory source → 写 CARGO_HOME/config.toml 源替换。
之后 cargo 全程离线可用。

用法：
    python tools/fetch-vendor.py                 # 默认 vendor 到 %USERPROFILE%/.cargo/vendor
    python tools/fetch-vendor.py --cargo-home D:/cargo-home

覆盖范围：workspace 全部成员的 [dependencies] + [workspace.dependencies]，
闭包含 normal/build/optional 依赖（宁多勿缺，多余 crate 不影响解析）。
"""

import argparse
import hashlib
import io
import json
import re
import sys
import tarfile
import urllib.request
from pathlib import Path

INDEX_BASE = "https://index.crates.io/"
CRATE_BASE = "https://static.crates.io/crates/"
REPO_ROOT = Path(__file__).resolve().parent.parent


def http_get(url: str) -> bytes:
    last = None
    for _ in range(3):
        try:
            with urllib.request.urlopen(url, timeout=60) as r:
                return r.read()
        except Exception as e:  # noqa: BLE001 —— 重试后向上抛
            last = e
    raise RuntimeError(f"下载失败 {url}: {last}")


def index_path(name: str) -> str:
    n = name.lower()
    if len(n) == 1:
        return f"1/{n}"
    if len(n) == 2:
        return f"2/{n}"
    if len(n) == 3:
        return f"3/{n[0]}/{n}"
    return f"{n[:2]}/{n[2:4]}/{n}"


def parse_version(v: str):
    m = re.match(r"^(\d+)\.(\d+)\.(\d+)(?:[-+].*)?$", v)
    return tuple(int(x) for x in m.groups()) if m else None


def parse_req(req: str):
    """解析 cargo 版本需求（逗号分隔的比较器），返回 matcher(版本元组) -> bool。"""
    comps = []
    for raw in re.split(r"\s*,\s*", req.strip()):
        if not raw or raw == "*":
            continue
        m = re.match(r"^(>=|<=|~=|=|\^|~|<|>)?\s*(\d+(?:\.(?:\d+|\*)){0,2}(?:[-+][0-9A-Za-z.+-]+)?)$", raw)
        if not m:
            raise ValueError(f"无法解析版本需求: {req!r}")
        op = m.group(1) or "^"
        vraw = m.group(2)
        if "*" in vraw:
            # 通配符按区间展开：X.* → [X.0.0, X+1.0.0)；X.Y.* → [X.Y.0, X.Y+1.0)
            pieces = vraw.split(".")
            idx = pieces.index("*")
            if idx == 0:
                continue  # 无约束
            lower = [int(x) for x in pieces[:idx]] + [0] * (3 - idx)
            upper = lower[:]
            upper[idx - 1] += 1
            for j in range(idx, 3):
                upper[j] = 0
            comps.append((">=", tuple(lower)))
            comps.append(("<", tuple(upper)))
            continue
        num = vraw.split("+")[0].split("-")[0]
        parts = [int(x) for x in num.split(".")]
        while len(parts) < 3:
            parts.append(0)
        comps.append((op, tuple(parts)))

    def match(ver):
        for op, base in comps:
            if op == "=":
                ok = ver == base
            elif op == ">=":
                ok = ver >= base
            elif op == "<=":
                ok = ver <= base
            elif op == "<":
                ok = ver < base
            elif op == ">":
                ok = ver > base
            elif op == "~":
                ok = ver >= base and ver[:2] == base[:2]
            else:  # ^ 与 ^0.x 的次版本边界
                if base[0] > 0:
                    ok = ver >= base and ver[0] == base[0]
                elif base[1] > 0:
                    ok = ver >= base and ver[:2] == base[:2]
                else:
                    ok = ver >= base and ver == base
            if not ok:
                return False
        return True

    return match


def read_direct_deps(extra_manifests=None):
    """name -> list[req]（workspace 成员的依赖 + workspace.dependencies + 指定额外清单）。"""
    reqs = {}
    files = [REPO_ROOT / "Cargo.toml"]
    for p in sorted((REPO_ROOT / "core").glob("*/Cargo.toml")):
        files.append(p)
    for p in extra_manifests or []:
        files.append(Path(p))
    dep_line = re.compile(r'^\s*([A-Za-z0-9_-]+)\s*=\s*(?:"([^"]+)"|\{\s*version\s*=\s*"([^"]+)")')
    section = ""
    wanted = ("[dependencies]", "[build-dependencies]", "[dev-dependencies]", "[workspace.dependencies]")
    for f in files:
        for line in f.read_text(encoding="utf-8").splitlines():
            s = line.strip()
            if s.startswith("["):
                section = s
                continue
            if not any(w in section for w in wanted):
                continue
            m = dep_line.match(line)
            if m:
                name, r1, r2 = m.group(1), m.group(2), m.group(3)
                if name.startswith("ab-"):
                    continue  # 本地路径依赖跳过
                reqs.setdefault(name, []).append(r1 or r2 or "0")
    return reqs


INDEX_CACHE_DIR = REPO_ROOT / ".cargo-home" / "index-cache"


def load_index(name: str):
    """sparse index 元数据，落盘缓存（重跑秒级、断点续跑）。"""
    cache_file = INDEX_CACHE_DIR / f"{name}.json"
    if cache_file.exists():
        raw = cache_file.read_text(encoding="utf-8")
    else:
        raw = http_get(INDEX_BASE + index_path(name)).decode("utf-8")
        INDEX_CACHE_DIR.mkdir(parents=True, exist_ok=True)
        cache_file.write_text(raw, encoding="utf-8")
    return [json.loads(line) for line in raw.splitlines() if line.strip()]


def selftest():
    """解析器语法自测（本地、零网络）：真实遇到过的全部语法一次性锁定。"""
    corpus = [
        ("^0.5", (0, 5, 0), True), ("^0.5", (0, 6, 0), False),
        ("0.10", (0, 10, 7), True), ("0.10", (0, 11, 0), False),
        ("2", (2, 9, 1), True), ("2", (3, 0, 0), False),
        ("=1.2.3", (1, 2, 3), True), ("=1.2.3", (1, 2, 4), False),
        (">=2.2, <2.3", (2, 2, 9), True), (">=2.2, <2.3", (2, 3, 0), False),
        ("~1.2", (1, 2, 9), True), ("~1.2", (1, 3, 0), False),
        ("^0.3.0-alpha.15", (0, 3, 0), True), ("^0.3.0-alpha.15", (0, 2, 9), False),
        ("3.*.*", (3, 4, 5), True), ("3.*.*", (4, 0, 0), False),
        ("1.2.*", (1, 2, 9), True), ("1.2.*", (1, 3, 0), False),
        ("*", (9, 9, 9), True),
        (">= 1.0, < 3.0", (2, 5, 0), True), (">= 1.0, < 3.0", (3, 0, 0), False),
    ]
    fails = [(req, v, want) for req, v, want in corpus if parse_req(req)(v) != want]
    if fails:
        raise SystemExit(f"解析器自测失败: {fails}")
    print(f"解析器自测：{len(corpus)}/{len(corpus)} 通过")


def resolve(reqs):
    """收敛选版：同名 crate 允许多个主版本共存（cargo 语义）。返回 (name, ver, meta) 列表。
    解析失败不中途放弃——一次性汇总全部问题（含来源父 crate），便于一轮修净。"""
    index_cache = {}

    def versions_of(name):
        if name not in index_cache:
            index_cache[name] = load_index(name)
        return index_cache[name]

    selected = {}  # (name, ver) -> meta
    pending = [(n, r, False, "<直接依赖>") for n, rs in reqs.items() for r in rs]
    seen = {(n, r) for n, r, _, _ in pending}
    problems = []  # (name, req, optional, parent, note)
    while pending:
        name, req, optional, parent = pending.pop()
        if name == "clippy":
            continue  # 老 crate 的 lint 占位依赖，非真实包
        matcher = parse_req(req)
        try:
            versions = versions_of(name)
        except Exception as e:  # noqa: BLE001 —— 汇总进报告而非中断
            problems.append((name, req, optional, parent, f"索引不可达: {e}"))
            continue
        cands = [
            v for v in versions
            if not v.get("yanked") and parse_version(v["vers"]) and matcher(parse_version(v["vers"]))
        ]
        if not cands:
            n_match = sum(1 for v in versions if parse_version(v["vers"]) and matcher(parse_version(v["vers"])))
            note = f"{n_match} 个匹配版本但全部 yanked" if n_match else "无匹配版本"
            problems.append((name, req, optional, parent, note))
            continue
        # 版本排序键：数值版本优先，同版时正式版高于预发布版（近似 semver，足够 vendoring 用）
        best = max(cands, key=lambda v: (parse_version(v["vers"]), "-" not in v["vers"]))
        key = (name, best["vers"])
        if key in selected:
            continue
        selected[key] = best
        me = f"{name} {best['vers']}"
        for dep in best.get("deps", []):
            if dep.get("kind") in (None, "normal", "build"):
                # 改名依赖取真实包名；编译器内置包（rustc-std-workspace-*）不上 crates.io，跳过
                dname = dep.get("package") or dep["name"]
                if dname.startswith("ab-") or dname.startswith("rustc-std-workspace"):
                    continue
                dreq = dep.get("req", "*")
                if (dname, dreq) not in seen:
                    seen.add((dname, dreq))
                    # 可选性沿路径继承：经由任何可选边到达的子树整体视为可选（真实构建不会激活它）
                    pending.append((dname, dreq, optional or bool(dep.get("optional")), me))

    warns = [p for p in problems if p[2]]
    real = [p for p in problems if not p[2]]
    for name, req, _opt, parent, note in warns:
        print(f"      警告：跳过可选依赖 {name} {req}（{note}；来自 {parent}）")
    if real:
        print("      解析失败（一次性列出全部）：")
        for name, req, _opt, parent, note in real:
            print(f"      X {name} {req} —— {note}；来自 {parent}")
        raise RuntimeError(f"{len(real)} 个必需依赖无法解析（见上方完整清单）")
    return [(n, v, meta) for (n, v), meta in sorted(selected.items())]


def download_and_unpack(name, version, vendor_dir: Path):
    target = vendor_dir / f"{name}-{version}"
    if (target / ".cargo-checksum.json").exists():
        return
    url = f"{CRATE_BASE}{name}/{name}-{version}.crate"
    blob = http_get(url)
    digest = hashlib.sha256(blob).hexdigest()
    target.mkdir(parents=True, exist_ok=True)
    with tarfile.open(fileobj=io.BytesIO(blob), mode="r:gz") as tf:
        for member in tf.getmembers():
            parts = Path(member.name).parts
            if len(parts) <= 1:
                continue
            member.name = "/".join(parts[1:])
            tf.extract(member, target, filter="data")
    (target / ".cargo-checksum.json").write_text(
        json.dumps({"files": {}, "package": digest}), encoding="utf-8"
    )


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cargo-home", default=str(REPO_ROOT / ".cargo-home"))
    ap.add_argument("--selftest", action="store_true", help="只跑解析器本地自测（零网络）")
    ap.add_argument(
        "--extra-manifest",
        action="append",
        default=[],
        help="额外解析的 Cargo.toml（如独立 workspace 的 desktop/src-tauri/Cargo.toml），可重复",
    )
    ap.add_argument(
        "--all-versions",
        action="append",
        default=[],
        help="把指定 crate 的全部未撤回版本拉进 vendor（精确 pin 与贪心选版冲突时，交回 cargo 自行回溯选版），可重复",
    )
    args = ap.parse_args()
    selftest()
    if args.selftest:
        return 0
    cargo_home = Path(args.cargo_home)
    vendor_dir = cargo_home / "vendor"
    vendor_dir.mkdir(parents=True, exist_ok=True)

    print("[1/3] 解析依赖闭包 ...")
    reqs = read_direct_deps(args.extra_manifest)
    print(f"      直接依赖 {len(reqs)} 个: {sorted(reqs)}")
    chosen = resolve(reqs)
    print(f"      闭包共 {len(chosen)} 个 crate")

    print("[2/3] 下载并解包 ...")
    for i, (name, ver, _meta) in enumerate(chosen, 1):
        print(f"      [{i}/{len(chosen)}] {name} {ver}")
        download_and_unpack(name, ver, vendor_dir)

    for name in args.all_versions:
        versions = [m for m in load_index(name) if not m.get("yanked")]
        print(f"      [全版本] {name}：{len(versions)} 个版本")
        for meta in versions:
            download_and_unpack(name, meta["vers"], vendor_dir)

    cfg = cargo_home / "config.toml"
    cfg.write_text(
        '[source.crates-io]\nreplace-with = "vendored-sources"\n'
        f'[source.vendored-sources]\ndirectory = "{vendor_dir.as_posix()}"\n',
        encoding="utf-8",
    )
    print(f"[3/3] 已写 {cfg}")
    print("完成：cargo 现可离线构建（如需恢复官方源，删除该 config.toml）。")


if __name__ == "__main__":
    sys.exit(main())
