#!/usr/bin/env python3
"""Read the artifact matrix; emit wheels JSON plus ==X.Y.* -> X.Y / cpXY."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

MATRIX_REL = "assay-python-sdk/python-artifact-matrix.v0.json"
REQUIRES_EXACT_RE = re.compile(r"^==(\d+)\.(\d+)\.\*$")


def parse_requires_python(spec: str) -> tuple[int, int]:
    """Parse exact ==X.Y.* only. Reject >=, ~=, bare 3.12, ==3.12, ==3.12.0, 3.12+."""
    if not isinstance(spec, str):
        raise ValueError(f"requires_python must be exact ==X.Y.*, got {spec!r}")
    match = REQUIRES_EXACT_RE.fullmatch(spec.strip())
    if not match:
        raise ValueError(f"requires_python must be exact ==X.Y.*, got {spec!r}")
    return int(match.group(1)), int(match.group(2))


def cpython_abi(major: int, minor: int) -> str:
    """3.12 -> cp312, 3.13 -> cp313. One function, not a 3.12 special case."""
    return f"cp{major}{minor}"


def tag_abi(tag: str) -> str:
    """Wheel ABI/impl is the first '-' separated tag component."""
    return str(tag).split("-", 1)[0]


def build_plan(matrix: dict) -> dict:
    major, minor = parse_requires_python(matrix.get("requires_python"))
    abi = cpython_abi(major, minor)
    python = f"{major}.{minor}"
    wheels_out: list[dict] = []
    for wheel in matrix.get("wheels") or []:
        tag = str(wheel.get("tag") or "")
        got = tag_abi(tag)
        if got != abi:
            raise ValueError(
                f"tag {tag!r}: ABI must be {abi} for Requires-Python =={python}.*"
            )
        wheels_out.append(
            {
                "os": wheel["os"],
                "target": wheel["target"],
                "tag": tag,
            }
        )
    if not wheels_out:
        raise ValueError("matrix.wheels must be a non-empty list")
    return {"python": python, "abi": abi, "wheels": wheels_out}


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
    print(f"wheels={json.dumps(plan['wheels'], separators=(',', ':'))}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
