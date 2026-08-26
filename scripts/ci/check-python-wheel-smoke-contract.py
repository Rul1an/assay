#!/usr/bin/env python3
"""Bind the wheels job to a local produced-wheel smoke step. YAML is not generated."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

MATRIX_REL = "assay-python-sdk/python-artifact-matrix.v0.json"
RELEASE_REL = ".github/workflows/release.yml"
SMOKE_REL = "scripts/ci/smoke-python-wheel.py"
SMOKE_STEP = "Smoke the produced wheel"
BUILD_STEP = "Build wheels"
UPLOAD_STEP = "Upload wheels"
NATIVE = "native"
UNSUPPORTED = "unsupported"
EXPECTED_OS = {
    "x86_64-unknown-linux-gnu": "ubuntu-latest",
    "x86_64-apple-darwin": "macos-15-intel",
    "aarch64-apple-darwin": "macos-15",
}
INCLUDE_PAIR_RE = re.compile(r"(?m)^\s+-\s+os:\s+(\S+)\s*\n\s+target:\s+(\S+)\s*$")


def fail(errors: list[str], msg: str) -> None:
    errors.append(msg)


def wheels_job(text: str) -> str:
    start = text.find("\n  wheels:")
    if start < 0:
        raise ValueError("wheels job missing")
    end = text.find("\n  publish-pypi:", start)
    if end < 0:
        end = len(text)
    return text[start:end]


def step_index(job: str, name: str) -> int:
    marker = f"      - name: {name}"
    return job.find(marker)


def check_workflow(root: Path, errors: list[str]) -> str:
    text = (root / RELEASE_REL).read_text(encoding="utf-8")
    try:
        job = wheels_job(text)
    except ValueError as exc:
        fail(errors, f"{RELEASE_REL}: {exc}")
        return ""
    build = step_index(job, BUILD_STEP)
    smoke = step_index(job, SMOKE_STEP)
    upload = step_index(job, UPLOAD_STEP)
    if build < 0:
        fail(errors, f"{RELEASE_REL}: missing {BUILD_STEP!r} step")
    if smoke < 0:
        fail(errors, f"{RELEASE_REL}: missing {SMOKE_STEP!r} step")
    if upload < 0:
        fail(errors, f"{RELEASE_REL}: missing {UPLOAD_STEP!r} step")
    if smoke >= 0 and build >= 0 and smoke < build:
        fail(errors, f"{RELEASE_REL}: smoke must run after {BUILD_STEP!r}")
    if smoke >= 0 and upload >= 0 and not (build < smoke < upload):
        fail(errors, f"{RELEASE_REL}: smoke must sit between build and upload")
    if "scripts/ci/smoke-python-wheel.py" not in job:
        fail(errors, f"{RELEASE_REL}: wheels job must invoke {SMOKE_REL}")
    if "ASSAY_WHEEL_TARGET" not in job:
        fail(errors, f"{RELEASE_REL}: smoke must bind ASSAY_WHEEL_TARGET to matrix.target")
    if re.search(r"pypi\.org|pip index|extra-index-url", job, re.IGNORECASE):
        fail(errors, f"{RELEASE_REL}: wheels job must not use a PyPI network index")
    return job


def check_matrix(root: Path, errors: list[str]) -> dict | None:
    path = root / MATRIX_REL
    if not path.is_file():
        fail(errors, f"missing {MATRIX_REL}")
        return None
    matrix = json.loads(path.read_text(encoding="utf-8"))
    wheels = matrix.get("wheels")
    if not isinstance(wheels, list) or not wheels:
        fail(errors, f"{MATRIX_REL}: wheels must be a non-empty list")
        return matrix
    for wheel in wheels:
        target = wheel.get("target")
        mode = wheel.get("import_smoke")
        os_label = wheel.get("os")
        if mode not in {NATIVE, UNSUPPORTED}:
            fail(errors, f"{MATRIX_REL}: {target}: import_smoke must be {NATIVE} or {UNSUPPORTED}")
        elif mode != NATIVE:
            fail(errors, f"{MATRIX_REL}: {target}: declared pair cannot be unsupported")
        expected_os = EXPECTED_OS.get(target)
        if expected_os is None:
            fail(errors, f"{MATRIX_REL}: {target}: unexpected declared target")
        elif os_label != expected_os:
            fail(errors, f"{MATRIX_REL}: {target}: os must be {expected_os}, got {os_label}")
    return matrix


def check_wheels_pairs(job: str, matrix: dict, errors: list[str]) -> None:
    actual = INCLUDE_PAIR_RE.findall(job)
    expected = [(wheel["os"], wheel["target"]) for wheel in matrix.get("wheels") or []]
    if actual != expected:
        fail(errors, f"{RELEASE_REL}: wheel (os, target) pairs {actual} != matrix {expected}")


def check_smoke_script(root: Path, errors: list[str]) -> None:
    path = root / SMOKE_REL
    if not path.is_file():
        fail(errors, f"missing {SMOKE_REL}")
        return
    text = path.read_text(encoding="utf-8")
    for needle, label in (
        ("--no-index", "no-index install"),
        ("--only-binary", "only-binary install"),
        ("assay._native", "native import"),
        ("import_smoke", "import_smoke branch"),
    ):
        if needle not in text:
            fail(errors, f"{SMOKE_REL}: missing {label}")
    if re.search(r"pypi\.org|extra-index-url", text):
        fail(errors, f"{SMOKE_REL}: must not talk to PyPI")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".")
    args = parser.parse_args(argv)
    root = Path(args.root).resolve()
    errors: list[str] = []
    matrix = check_matrix(root, errors)
    job = check_workflow(root, errors)
    if matrix is not None and job:
        check_wheels_pairs(job, matrix, errors)
    check_smoke_script(root, errors)
    if errors:
        print("python-wheel-smoke-contract FAIL", file=sys.stderr)
        for item in errors:
            print(f"  {item}", file=sys.stderr)
        return 1
    print("python-wheel-smoke-contract PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
