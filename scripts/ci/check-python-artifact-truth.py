#!/usr/bin/env python3
"""Fail-closed gate: assay-it metadata, wheel targets, docs, and pip-download contract share one matrix."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

MATRIX_REL = "assay-python-sdk/python-artifact-matrix.v0.json"
PYPROJECT_REL = "assay-python-sdk/pyproject.toml"
CARGO_REL = "assay-python-sdk/Cargo.toml"
RELEASE_REL = ".github/workflows/release.yml"
WORKSPACE_REL = "Cargo.toml"
SCHEMA = "assay.python_artifact_matrix.v0"
PIP_INSTALL_RE = re.compile(
    r"""(?:python(?:3(?:\.\d+)?)?\s+-m\s+)?pip(?:3|x)?\s+install(?:\s+(?:-U|--upgrade|--user))*\s+["']?assay-it\b""",
    re.IGNORECASE,
)
BROADER_PYTHON_RE = re.compile(r"Python\s+3\.(?:[0-9]|1[01])\+|PyPy", re.IGNORECASE)
SDIST_CLAIM_RE = re.compile(r"\bsdist\b|source distribution", re.IGNORECASE)
ABI3_CLAIM_RE = re.compile(r"\babi3(?:-py\d+)?\b", re.IGNORECASE)


def fail(errors: list[str], msg: str) -> None:
    errors.append(msg)


def load_matrix(root: Path, errors: list[str]) -> dict | None:
    path = root / MATRIX_REL
    if not path.is_file():
        fail(errors, f"missing {MATRIX_REL}")
        return None
    data = json.loads(path.read_text(encoding="utf-8"))
    if data.get("schema") != SCHEMA:
        fail(errors, f"{MATRIX_REL}: schema must be {SCHEMA}")
    return data


def workspace_version(root: Path, errors: list[str]) -> str | None:
    text = (root / WORKSPACE_REL).read_text(encoding="utf-8")
    match = re.search(r'(?m)^version\s*=\s*"([^"]+)"', text)
    if not match:
        fail(errors, "Cargo.toml: missing workspace version")
        return None
    return match.group(1)


def check_pyproject(root: Path, matrix: dict, errors: list[str]) -> None:
    text = (root / PYPROJECT_REL).read_text(encoding="utf-8")
    req = re.search(r'(?m)^requires-python\s*=\s*"([^"]+)"', text)
    if not req or req.group(1) != matrix["requires_python"]:
        fail(errors, f"{PYPROJECT_REL}: requires-python must be {matrix['requires_python']!r}")
    for required in matrix["required_classifiers"]:
        if required not in text:
            fail(errors, f"{PYPROJECT_REL}: missing classifier {required!r}")
    for forbidden in matrix["forbidden_classifiers"]:
        if forbidden in text:
            fail(errors, f"{PYPROJECT_REL}: forbidden classifier {forbidden!r}")
    if "pyo3/abi3" in text or "abi3-py" in text:
        fail(errors, f"{PYPROJECT_REL}: abi3 is out of scope")


def check_cargo(root: Path, errors: list[str]) -> None:
    text = (root / CARGO_REL).read_text(encoding="utf-8")
    if "abi3" in text:
        fail(errors, f"{CARGO_REL}: abi3 is out of scope")
    if "extension-module" not in text:
        fail(errors, f"{CARGO_REL}: expected pyo3 extension-module only")


def wheels_job(text: str) -> str:
    start = text.find("\n  wheels:")
    if start < 0:
        raise ValueError("wheels job missing")
    end = text.find("\n  publish-pypi:", start)
    if end < 0:
        end = len(text)
    return text[start:end]


def check_release_workflow(root: Path, matrix: dict, errors: list[str]) -> None:
    text = (root / RELEASE_REL).read_text(encoding="utf-8")
    try:
        job = wheels_job(text)
    except ValueError as exc:
        fail(errors, f"{RELEASE_REL}: {exc}")
        return
    if "python-version: '3.12'" not in job and 'python-version: "3.12"' not in job:
        fail(errors, f"{RELEASE_REL}: wheels job must pin Python 3.12")
    if "-i python3.12" not in job:
        fail(errors, f"{RELEASE_REL}: maturin must use -i python3.12")
    if "*.tar.gz" in job or "sdist" in job.lower():
        fail(errors, f"{RELEASE_REL}: sdist publish is out of scope")
    if "dist/*.whl" not in job:
        fail(errors, f"{RELEASE_REL}: upload glob must stay wheel-only")
    targets = re.findall(r"(?m)^\s+target:\s+(\S+)$", job)
    expected = [wheel["target"] for wheel in matrix["wheels"]]
    if targets != expected:
        fail(errors, f"{RELEASE_REL}: wheel targets {targets} != matrix {expected}")
    os_labels = re.findall(r"(?m)^\s+-\s+os:\s+(\S+)$", job)
    expected_os = [wheel["os"] for wheel in matrix["wheels"]]
    if os_labels != expected_os:
        fail(errors, f"{RELEASE_REL}: wheel os labels {os_labels} != matrix {expected_os}")
    if "Smoke the produced wheel" not in job or "scripts/ci/smoke-python-wheel.py" not in job:
        fail(errors, f"{RELEASE_REL}: wheels job must bind the produced-wheel smoke")


def check_docs(root: Path, matrix: dict, errors: list[str]) -> None:
    bound = matrix["support_bound"]
    for rel in matrix["install_docs"]:
        path = root / rel
        if not path.is_file():
            fail(errors, f"{rel}: install-doc is missing")
            continue
        text = path.read_text(encoding="utf-8")
        if PIP_INSTALL_RE.search(text) and bound not in text:
            fail(errors, f"{rel}: pip install assay-it without support bound")
        if BROADER_PYTHON_RE.search(text):
            fail(errors, f"{rel}: broader Python/PyPy claim than the matrix")
        if SDIST_CLAIM_RE.search(text) and "assay-it" in text.lower():
            fail(errors, f"{rel}: sdist claim is out of scope")
        if ABI3_CLAIM_RE.search(text):
            fail(errors, f"{rel}: abi3 claim is out of scope")


def expected_wheel_names(matrix: dict, version: str) -> list[str]:
    dist = matrix["package"].replace("-", "_")
    return [f"{dist}-{version}-{wheel['tag']}.whl" for wheel in matrix["wheels"]]


def evaluate_published_files(matrix: dict, version: str, filenames: list[str]) -> list[str]:
    errors: list[str] = []
    expected = set(expected_wheel_names(matrix, version))
    actual = set(filenames)
    missing = sorted(expected - actual)
    extra_sdist = sorted(
        name for name in actual if name.endswith(".tar.gz") or ".tar.gz" in name
    )
    if missing:
        errors.append(f"declared wheel missing from published set: {missing}")
    if extra_sdist and not matrix.get("publish_sdist"):
        errors.append(f"sdist published while publish_sdist is false: {extra_sdist}")
    return errors


def check_pip_download_contract(
    matrix: dict, version: str, errors: list[str], published: list[str] | None
) -> None:
    if published is None:
        # Existence is the producer smoke, not a matrix-vs-itself compare.
        return
    errors.extend(evaluate_published_files(matrix, version, published))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".")
    parser.add_argument("--published-files", help="path to a JSON list of published filenames")
    args = parser.parse_args(argv)
    root = Path(args.root).resolve()
    errors: list[str] = []
    matrix = load_matrix(root, errors)
    if matrix is None:
        print("\n".join(errors), file=sys.stderr)
        return 1
    for wheel in matrix.get("wheels") or []:
        mode = wheel.get("import_smoke")
        target = wheel.get("target")
        if mode == "unsupported":
            fail(errors, f"{MATRIX_REL}: {target}: declared pair cannot be unsupported")
        elif mode != "native":
            fail(errors, f"{MATRIX_REL}: {target}: import_smoke must be native")
    version = workspace_version(root, errors)
    check_pyproject(root, matrix, errors)
    check_cargo(root, errors)
    check_release_workflow(root, matrix, errors)
    check_docs(root, matrix, errors)
    published = None
    if args.published_files:
        published = json.loads(Path(args.published_files).read_text(encoding="utf-8"))
    if version:
        check_pip_download_contract(matrix, version, errors, published)
    if errors:
        print("python-artifact-truth FAIL", file=sys.stderr)
        for item in errors:
            print(f"  {item}", file=sys.stderr)
        return 1
    print("python-artifact-truth PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
