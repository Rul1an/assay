#!/usr/bin/env python3
"""Fail-closed local smoke for one assay-it wheel cell. No PyPI network."""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

MATRIX_REL = "assay-python-sdk/python-artifact-matrix.v0.json"
NATIVE_RE = re.compile(r"assay/_native[^/]*\.(so|dylib|pyd)$")


def workspace_version(root: Path) -> str:
    text = (root / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'(?m)^version\s*=\s*"([^"]+)"', text)
    if not match:
        raise SystemExit("Cargo.toml: missing workspace version")
    return match.group(1)


def load_cell(root: Path, target: str) -> dict:
    matrix = json.loads((root / MATRIX_REL).read_text(encoding="utf-8"))
    matches = [row for row in matrix["wheels"] if row["target"] == target]
    if len(matches) != 1:
        raise SystemExit(f"matrix has {len(matches)} cells for target {target}")
    return matches[0]


def find_wheel(dist: Path, package: str, version: str, tag: str) -> Path:
    expected = f"{package.replace('-', '_')}-{version}-{tag}.whl"
    found = sorted(path for path in dist.glob("*.whl") if path.name == expected)
    if len(found) != 1:
        names = [path.name for path in sorted(dist.glob("*.whl"))]
        raise SystemExit(
            f"expected exactly one {expected} in {dist}, found {len(found)}: {names}"
        )
    return found[0]


def native_members(wheel: Path) -> list[str]:
    with zipfile.ZipFile(wheel) as archive:
        return [name for name in archive.namelist() if NATIVE_RE.search(name)]


def install_and_import(python: str, wheel: Path, version: str) -> None:
    if shutil.which(python) is None and not Path(python).exists():
        raise SystemExit(f"native import_smoke requires {python}")
    scratch = Path(tempfile.mkdtemp(prefix="assay-it-wheel-smoke-"))
    try:
        venv = scratch / "venv"
        subprocess.run([python, "-m", "venv", str(venv)], check=True)
        pip = venv / "bin" / "python"
        env = os.environ.copy()
        env["PIP_NO_INDEX"] = "1"
        env.pop("PIP_INDEX_URL", None)
        env.pop("PIP_EXTRA_INDEX_URL", None)
        subprocess.run(
            [
                str(pip),
                "-m",
                "pip",
                "install",
                "--disable-pip-version-check",
                "--no-index",
                "--only-binary",
                ":all:",
                "--find-links",
                str(wheel.parent),
                f"assay-it=={version}",
            ],
            check=True,
            env=env,
        )
        probe = (
            "import importlib.metadata as m, assay, assay._native\n"
            f"assert m.version('assay-it') == {version!r}, m.version('assay-it')\n"
            "print('import_smoke=native version=' + m.version('assay-it'))\n"
        )
        subprocess.run([str(pip), "-c", probe], check=True, env=env)
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".")
    parser.add_argument("--dist-dir", default="assay-python-sdk/dist")
    parser.add_argument("--target", default=os.environ.get("ASSAY_WHEEL_TARGET", ""))
    parser.add_argument("--python", default=os.environ.get("PYTHON_BIN", "python3.12"))
    args = parser.parse_args(argv)
    if not args.target:
        raise SystemExit("ASSAY_WHEEL_TARGET or --target is required")
    root = Path(args.root).resolve()
    dist = Path(args.dist_dir)
    if not dist.is_absolute():
        dist = root / dist
    cell = load_cell(root, args.target)
    version = workspace_version(root)
    wheel = find_wheel(dist, "assay-it", version, cell["tag"])
    natives = native_members(wheel)
    if len(natives) != 1:
        raise SystemExit(f"{wheel.name}: expected one assay._native extension, found {natives}")
    mode = cell.get("import_smoke")
    if mode == "unsupported":
        print(
            f"import_smoke=unsupported target={args.target} wheel={wheel.name} "
            f"native={natives[0]} no native load claimed"
        )
        return 0
    if mode != "native":
        raise SystemExit(f"{args.target}: import_smoke must be native or unsupported")
    install_and_import(args.python, wheel, version)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
