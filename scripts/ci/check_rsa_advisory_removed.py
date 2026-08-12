#!/usr/bin/env python3
"""Fail closed when the rsa crate or RUSTSEC-2023-0071 exceptions return.

Pins two properties that must stay true together after CI-4C (#2231):

1. No resolved package named ``rsa`` in workspace ``cargo metadata``.
2. No ``RUSTSEC-2023-0071`` exception text in the four policy/invocation sites
   that previously carried the ignore in lockstep.

The check reads metadata from stdin (JSON) or runs ``cargo metadata --locked``
itself. Policy files are read from the repository root.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ADVISORY_ID = "RUSTSEC-2023-0071"

POLICY_SITES = (
    "deny.toml",
    ".cargo/audit.toml",
    ".github/workflows/ci.yml",
    ".pre-commit-config.yaml",
)


def repo_root() -> Path:
    out = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True,
        text=True,
        check=True,
    )
    return Path(out.stdout.strip())


def load_metadata(raw: str | None) -> dict:
    if raw is not None:
        return json.loads(raw)
    proc = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise RuntimeError(
            "cargo metadata --locked failed:\n"
            + (proc.stderr.strip() or proc.stdout.strip() or "(no output)")
        )
    return json.loads(proc.stdout)


def packages_named_rsa(metadata: dict) -> list[str]:
    names: list[str] = []
    for package in metadata.get("packages", []):
        name = package.get("name")
        if name == "rsa":
            version = package.get("version", "?")
            names.append(f"{name} {version}")
    return names


def advisory_hits(root: Path, sites: tuple[str, ...] = POLICY_SITES) -> list[str]:
    hits: list[str] = []
    for rel in sites:
        path = root / rel
        if not path.is_file():
            hits.append(f"{rel}: missing (cannot prove absence of {ADVISORY_ID})")
            continue
        text = path.read_text(encoding="utf-8")
        if ADVISORY_ID in text:
            for lineno, line in enumerate(text.splitlines(), start=1):
                if ADVISORY_ID in line:
                    hits.append(f"{rel}:{lineno}: {line.strip()}")
    return hits


def evaluate(root: Path, metadata: dict) -> list[str]:
    failures: list[str] = []
    rsa_pkgs = packages_named_rsa(metadata)
    if rsa_pkgs:
        failures.append(
            "resolved workspace metadata still contains package named rsa: "
            + ", ".join(rsa_pkgs)
        )
    ignore_hits = advisory_hits(root)
    if ignore_hits:
        failures.append(
            f"{ADVISORY_ID} still referenced in policy/invocation sites:\n  - "
            + "\n  - ".join(ignore_hits)
        )
    return failures


def main(argv: list[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    if args:
        # No argv path surface: metadata arrives on stdin or from cargo metadata.
        print(
            "FAIL: unexpected arguments "
            f"{args!r}; pass metadata JSON on stdin or run with no args",
            file=sys.stderr,
        )
        return 2
    root = repo_root()
    raw: str | None = None
    if not sys.stdin.isatty():
        stdin = sys.stdin.read()
        if stdin.strip():
            raw = stdin
    try:
        metadata = load_metadata(raw)
    except (RuntimeError, json.JSONDecodeError) as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 2
    failures = evaluate(root, metadata)
    if failures:
        for item in failures:
            print(f"FAIL: {item}", file=sys.stderr)
        return 1
    print(
        "PASS: no package named rsa in resolved metadata; "
        f"no {ADVISORY_ID} exception in the four policy sites"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
