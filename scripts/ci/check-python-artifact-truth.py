#!/usr/bin/env python3
"""Fail-closed gate: assay-it metadata, wheel targets, docs, and pip-download contract share one matrix."""

from __future__ import annotations

import argparse
import importlib.util
import json
import re
import sys
from pathlib import Path

MATRIX_REL = "assay-python-sdk/python-artifact-matrix.v0.json"
PYPROJECT_REL = "assay-python-sdk/pyproject.toml"
CARGO_REL = "assay-python-sdk/Cargo.toml"
RELEASE_REL = ".github/workflows/release.yml"
WORKSPACE_REL = "Cargo.toml"
KERNEL_MATRIX_REL = ".github/workflows/kernel-matrix.yml"
PRECOMMIT_REL = ".pre-commit-config.yaml"
PLANNER_REL = "scripts/ci/plan-python-artifact-matrix.py"
SCHEMA = "assay.python_artifact_matrix.v0"
PYPY_CLASSIFIER = "Programming Language :: Python :: Implementation :: PyPy"
PP_ABI_RE = re.compile(r"^pp\d+$")
# python-artifact-truth runs under kernel-matrix Lint. scripts/** already
# fires; these remaining surfaces must stay listed or a metadata/docs-only
# PR never starts the workflow.
ARTIFACT_TRUTH_PR_PATHS = (
    "assay-python-sdk/**",
    "llms.txt",
    "docs/python-sdk/**",
    "docs/getting-started/**",
    "docs/migration-v1.2.md",
    "docs/guides/troubleshooting.md",
    "docs/AIcontext/user-flows.md",
)
PRECOMMIT_REQUIRED_PATHS = (
    "docs/migration-v1.2.md",
    ".github/workflows/kernel-matrix.yml",
)
PIP_INSTALL_RE = re.compile(
    r"""(?:python(?:3(?:\.\d+)?)?\s+-m\s+)?pip(?:3|x)?\s+install(?:\s+(?:-U|--upgrade|--user))*\s+["']?assay-it\b""",
    re.IGNORECASE,
)
BROADER_PYTHON_RE = re.compile(
    r"(?:(?:CPython|Python)\s+)?3\.\d+\+|3\.\d+\s+(?:and|or)\s+later|PyPy",
    re.IGNORECASE,
)
SDIST_CLAIM_RE = re.compile(r"\bsdist\b|source distribution", re.IGNORECASE)
ABI3_CLAIM_RE = re.compile(r"\babi3(?:-py\d+)?\b", re.IGNORECASE)
STRAY_PYTHON_VERSION_RE = re.compile(
    r"""python-version:\s*['\"]?\d+\.\d+['\"]?"""
)
STRAY_MATURIN_INTERPRETER_RE = re.compile(r"-i\s+python\d+\.\d+")
FAMILY_ORDER = ("macOS", "Linux")
TARGET_FAMILY_ARCH = {
    "x86_64-apple-darwin": ("macOS", "x86_64"),
    "aarch64-apple-darwin": ("macOS", "arm64"),
    "x86_64-unknown-linux-gnu": ("Linux", "x86_64"),
}


def fail(errors: list[str], msg: str) -> None:
    errors.append(msg)


def load_planner():
    path = Path(__file__).resolve().parent / "plan-python-artifact-matrix.py"
    spec = importlib.util.spec_from_file_location("plan_python_artifact_matrix", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {PLANNER_REL}")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


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
    name = re.search(r'(?m)^name\s*=\s*"([^"]+)"', text)
    if not name or name.group(1) != matrix["package"]:
        fail(errors, f"{PYPROJECT_REL}: name must be {matrix['package']!r}")
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


def plan_job(text: str) -> str:
    start = text.find("\n  plan-python-artifact:")
    if start < 0:
        raise ValueError("plan-python-artifact job missing")
    end = text.find("\n  wheels:", start)
    if end < 0:
        end = len(text)
    return text[start:end]


def check_release_workflow(root: Path, matrix: dict, plan: dict, errors: list[str]) -> None:
    text = (root / RELEASE_REL).read_text(encoding="utf-8")
    try:
        planned = plan_job(text)
        job = wheels_job(text)
    except ValueError as exc:
        fail(errors, f"{RELEASE_REL}: {exc}")
        return
    if PLANNER_REL not in planned:
        fail(errors, f"{RELEASE_REL}: plan job must invoke {PLANNER_REL}")
    if "fromJSON(needs.plan-python-artifact.outputs.wheels)" not in job:
        fail(
            errors,
            f"{RELEASE_REL}: wheels include must consume needs.plan-python-artifact.outputs.wheels",
        )
    if "needs.plan-python-artifact.outputs.python" not in job:
        fail(errors, f"{RELEASE_REL}: setup-python and maturin -i must consume needs.plan-python-artifact.outputs.python")
    if STRAY_PYTHON_VERSION_RE.search(job):
        fail(errors, f"{RELEASE_REL}: wheels job must not pin a literal python-version")
    if STRAY_MATURIN_INTERPRETER_RE.search(job):
        fail(errors, f"{RELEASE_REL}: maturin must not use a literal -i pythonX.Y")
    if "*.tar.gz" in job or "sdist" in job.lower():
        fail(errors, f"{RELEASE_REL}: sdist publish is out of scope")
    if "dist/*.whl" not in job:
        fail(errors, f"{RELEASE_REL}: upload glob must stay wheel-only")
    if "Smoke the produced wheel" not in job or "scripts/ci/smoke-python-wheel.py" not in job:
        fail(errors, f"{RELEASE_REL}: wheels job must bind the produced-wheel smoke")
    if not re.search(r"--python\b|PYTHON_BIN", job):
        fail(errors, f"{RELEASE_REL}: smoke must pass --python or PYTHON_BIN explicitly")
    if "plan-python-artifact" not in job:
        fail(errors, f"{RELEASE_REL}: wheels job must need the plan job")
    del matrix, plan


def expected_support_bound(python: str, wheels: list) -> str:
    """Bind support_bound to parsed X.Y and the declared os/target/tag set."""
    families: dict[str, list[str]] = {}
    for wheel in wheels:
        target = wheel.get("target")
        mapped = TARGET_FAMILY_ARCH.get(target)
        if mapped is None:
            raise ValueError(f"cannot derive support_bound from target {target!r}")
        family, arch = mapped
        families.setdefault(family, [])
        if arch not in families[family]:
            families[family].append(arch)
    parts: list[str] = []
    for family in FAMILY_ORDER:
        if family in families:
            parts.append(f"{family} {'/'.join(families[family])}")
    for family, archs in families.items():
        if family not in FAMILY_ORDER:
            parts.append(f"{family} {'/'.join(archs)}")
    if not parts:
        raise ValueError("cannot derive support_bound from an empty wheels list")
    return (
        f"CPython {python} on {' and '.join(parts)}; "
        "other interpreters and platforms are not claimed."
    )


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


DOC_SUFFIXES = {".md", ".txt"}
# Live install-doc surfaces (current inventory dirs + top-level docs/*.md).
# Do not crawl tests/, CHANGELOG, archive, plans, or architecture specs.
INSTALL_DOC_SCAN_DIRS = (
    "docs/python-sdk",
    "docs/getting-started",
    "docs/guides",
    "docs/AIcontext",
    "assay-python-sdk",
)


def iter_install_doc_candidates(root: Path) -> list[str]:
    rels: list[str] = []
    docs = root / "docs"
    if docs.is_dir():
        for child in sorted(docs.iterdir()):
            if child.is_file() and child.suffix in DOC_SUFFIXES:
                rels.append(child.relative_to(root).as_posix())
    for rel_dir in INSTALL_DOC_SCAN_DIRS:
        base = root / rel_dir
        if not base.is_dir():
            continue
        for child in sorted(base.rglob("*")):
            if child.is_file() and child.suffix in DOC_SUFFIXES:
                rels.append(child.relative_to(root).as_posix())
    llms = root / "llms.txt"
    if llms.is_file():
        rels.append("llms.txt")
    seen: set[str] = set()
    out: list[str] = []
    for rel in rels:
        if rel not in seen:
            seen.add(rel)
            out.append(rel)
    return out


def check_install_docs_inventory(root: Path, matrix: dict, errors: list[str]) -> None:
    listed = set(matrix.get("install_docs") or [])
    for rel in iter_install_doc_candidates(root):
        path = root / rel
        try:
            page = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        if PIP_INSTALL_RE.search(page) and rel not in listed:
            fail(
                errors,
                f"{rel}: active pip install assay-it is omitted from install_docs",
            )


def check_tag_anchor(root: Path, matrix: dict, planner, errors: list[str]) -> dict | None:
    """Anchor Requires-Python / classifiers / tag ABI to the planner."""
    try:
        plan = planner.build_plan(matrix)
    except ValueError as exc:
        fail(errors, f"{MATRIX_REL}: {exc}")
        return None

    classifier = f"Programming Language :: Python :: {plan['python']}"
    required = matrix.get("required_classifiers") or []
    if classifier not in required:
        fail(
            errors,
            f"{MATRIX_REL}: required_classifiers must include {classifier!r}",
        )
    pyproject = (root / PYPROJECT_REL).read_text(encoding="utf-8")
    if classifier not in pyproject:
        fail(errors, f"{PYPROJECT_REL}: missing classifier {classifier!r}")
    try:
        expected_bound = expected_support_bound(plan["python"], matrix.get("wheels") or [])
    except ValueError as exc:
        fail(errors, f"{MATRIX_REL}: {exc}")
    else:
        if matrix.get("support_bound") != expected_bound:
            fail(
                errors,
                f"{MATRIX_REL}: support_bound must match declared wheels and "
                f"CPython {plan['python']}, expected {expected_bound!r}",
            )

    abis = [planner.tag_abi(str(wheel.get("tag") or "")) for wheel in matrix.get("wheels") or []]
    has_pypy = PYPY_CLASSIFIER in pyproject or PYPY_CLASSIFIER in required
    has_pp_tag = any(PP_ABI_RE.fullmatch(abi) for abi in abis)
    if has_pypy and not has_pp_tag:
        fail(
            errors,
            f"{MATRIX_REL}: {PYPY_CLASSIFIER!r} is forbidden unless a "
            f"declared tag ABI matches pp\\d+",
        )
    return plan


def pull_request_paths(text: str) -> list[str]:
    in_pr = in_paths = False
    paths: list[str] = []
    for line in text.splitlines():
        if re.match(r"^  pull_request:\s*$", line):
            in_pr, in_paths = True, False
            continue
        if in_pr and re.match(r"^  \S", line):
            in_pr = in_paths = False
        if in_pr and re.match(r"^    paths:\s*$", line):
            in_paths = True
            continue
        if in_pr and in_paths and re.match(r"^    \S", line):
            in_paths = False
        if in_paths:
            match = re.match(r'^      - "([^"]+)"\s*(?:#.*)?$', line)
            if match:
                paths.append(match.group(1))
    return paths


def check_kernel_matrix_paths(root: Path, errors: list[str]) -> None:
    path = root / KERNEL_MATRIX_REL
    if not path.is_file():
        return
    paths = pull_request_paths(path.read_text(encoding="utf-8"))
    for required in ARTIFACT_TRUTH_PR_PATHS:
        if required not in paths:
            fail(
                errors,
                f"{KERNEL_MATRIX_REL}: pull_request.paths must include {required!r} "
                f"(python-artifact-truth hook runs here)",
            )


def hook_files_regex(text: str, hook_id: str) -> str | None:
    in_hook = False
    for line in text.splitlines():
        if re.match(rf"^\s+- id: {re.escape(hook_id)}\s*$", line):
            in_hook = True
            continue
        if in_hook and re.match(r"^\s+- id:", line):
            break
        if in_hook:
            match = re.match(r"^\s+files:\s+(\S+)\s*$", line)
            if match:
                return match.group(1)
    return None


def check_precommit_selector(root: Path, errors: list[str]) -> None:
    path = root / PRECOMMIT_REL
    if not path.is_file():
        return
    regex = hook_files_regex(path.read_text(encoding="utf-8"), "python-artifact-truth")
    if not regex:
        fail(errors, f"{PRECOMMIT_REL}: python-artifact-truth files: selector missing")
        return
    try:
        compiled = re.compile(regex)
    except re.error as exc:
        fail(errors, f"{PRECOMMIT_REL}: python-artifact-truth files: invalid regex: {exc}")
        return
    for required in PRECOMMIT_REQUIRED_PATHS:
        if compiled.search(required) is None:
            fail(
                errors,
                f"{PRECOMMIT_REL}: python-artifact-truth files: must match {required!r}",
            )


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
    try:
        planner = load_planner()
    except RuntimeError as exc:
        print(f"python-artifact-truth FAIL\n  {exc}", file=sys.stderr)
        return 1
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
    plan = check_tag_anchor(root, matrix, planner, errors)
    check_cargo(root, errors)
    if plan is not None:
        check_release_workflow(root, matrix, plan, errors)
    check_docs(root, matrix, errors)
    check_install_docs_inventory(root, matrix, errors)
    check_kernel_matrix_paths(root, errors)
    check_precommit_selector(root, errors)
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
