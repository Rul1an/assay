#!/usr/bin/env python3
"""Read the artifact matrix and emit release-plan outputs for wheels and smoke runtimes."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

MATRIX_REL = "assay-python-sdk/python-artifact-matrix.v0.json"
REQUIRES_EXACT_RE = re.compile(r"^==(\d+)\.(\d+)\.\*$")
REQUIRES_MIN_RE = re.compile(r"^>=(\d+)\.(\d+)$")
PYTHON_DOTTED_RE = re.compile(r"^\d+\.\d+$")


def parse_requires_python(spec: str) -> tuple[str, int, int]:
    """Parse either ==X.Y.* (exact wheel ABI) or >=X.Y (abi3 minimum)."""
    if not isinstance(spec, str):
        raise ValueError(f"requires_python must be '==X.Y.*' or '>=X.Y', got {spec!r}")
    stripped = spec.strip()
    exact = REQUIRES_EXACT_RE.fullmatch(stripped)
    if exact:
        return "exact", int(exact.group(1)), int(exact.group(2))
    minimum = REQUIRES_MIN_RE.fullmatch(stripped)
    if minimum:
        return "minimum", int(minimum.group(1)), int(minimum.group(2))
    raise ValueError(f"requires_python must be '==X.Y.*' or '>=X.Y', got {spec!r}")


def cpython_abi(major: int, minor: int) -> str:
    """3.12 -> cp312, 3.13 -> cp313. One function, not a 3.12 special case."""
    return f"cp{major}{minor}"


def wheel_tag_parts(tag: str) -> tuple[str, str]:
    parts = str(tag).split("-")
    if len(parts) < 2:
        raise ValueError(f"wheel tag must start with <python>-<abi>-..., got {tag!r}")
    return parts[0], parts[1]


def tag_python(tag: str) -> str:
    return wheel_tag_parts(tag)[0]


def tag_abi(tag: str) -> str:
    """Wheel ABI is the second '-' separated tag component."""
    return wheel_tag_parts(tag)[1]


def parse_smoke_pythons(
    matrix: dict, *, mode: str, minimum_python: str
) -> list[str]:
    raw = matrix.get("smoke_pythons")
    if raw is None:
        return [minimum_python]
    if not isinstance(raw, list) or not raw:
        raise ValueError("smoke_pythons must be a non-empty list of X.Y strings")
    out: list[str] = []
    seen: set[str] = set()
    for item in raw:
        if not isinstance(item, str) or not PYTHON_DOTTED_RE.fullmatch(item):
            raise ValueError(f"smoke_pythons entry must be X.Y, got {item!r}")
        if item in seen:
            raise ValueError(f"smoke_pythons must not repeat {item!r}")
        seen.add(item)
        out.append(item)
    if mode == "minimum" and minimum_python not in seen:
        raise ValueError(
            f"smoke_pythons must include minimum interpreter {minimum_python!r}"
        )
    if mode == "minimum" and out[0] != minimum_python:
        raise ValueError(
            "smoke_pythons must start with the requires_python minimum"
        )
    return out


def build_plan(matrix: dict) -> dict:
    mode, major, minor = parse_requires_python(matrix.get("requires_python"))
    python = f"{major}.{minor}"
    cpython = cpython_abi(major, minor)
    smoke_pythons = parse_smoke_pythons(
        matrix, mode=mode, minimum_python=python
    )
    if mode == "exact":
        declared_abi = cpython
        expected_abi_tag = cpython
    else:
        if matrix.get("abi") != "abi3":
            raise ValueError(
                "requires_python >=X.Y requires matrix.abi = 'abi3'"
            )
        declared_abi = "abi3"
        expected_abi_tag = "abi3"
    wheels_out: list[dict] = []
    smoke_python_lines = "\n".join(smoke_pythons)
    for wheel in matrix.get("wheels") or []:
        tag = str(wheel.get("tag") or "")
        got_py = tag_python(tag)
        got_abi = tag_abi(tag)
        if got_py != cpython:
            raise ValueError(
                f"tag {tag!r}: python tag must be {cpython} for requires_python {matrix.get('requires_python')!r}"
            )
        if got_abi != expected_abi_tag:
            raise ValueError(
                f"tag {tag!r}: ABI tag must be {expected_abi_tag} for requires_python {matrix.get('requires_python')!r}"
            )
        wheels_out.append(
            {
                "os": wheel["os"],
                "target": wheel["target"],
                "tag": tag,
                "smoke_pythons": smoke_pythons,
                "smoke_python_lines": smoke_python_lines,
            }
        )
    if not wheels_out:
        raise ValueError("matrix.wheels must be a non-empty list")
    return {
        "python": python,
        "abi": declared_abi,
        "smoke_pythons": smoke_pythons,
        "wheels": wheels_out,
    }


def load_matrix(root: Path) -> dict:
    path = root / MATRIX_REL
    if not path.is_file():
        raise ValueError(f"missing {MATRIX_REL}")
    return json.loads(path.read_text(encoding="utf-8"))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".")
    parser.add_argument("--format", choices=("gha", "json"), default="gha")
    args = parser.parse_args(argv)
    root = Path(args.root).resolve()
    try:
        plan = build_plan(load_matrix(root))
    except ValueError as exc:
        print(str(exc), file=sys.stderr)
        return 1
    if args.format == "json":
        json.dump(plan, sys.stdout, indent=2)
        sys.stdout.write("\n")
        return 0
    print(f"python={plan['python']}")
    print(f"abi={plan['abi']}")
    print(
        f"smoke_pythons={json.dumps(plan['smoke_pythons'], separators=(',', ':'))}"
    )
    print(f"wheels={json.dumps(plan['wheels'], separators=(',', ':'))}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
