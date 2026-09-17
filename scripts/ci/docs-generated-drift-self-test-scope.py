#!/usr/bin/env python3
"""Scope helper for generated-docs drift self-tests.

Single source of truth: this reads the `docs-generated-drift-self-test` hook's
`files:` regex from `.pre-commit-config.yaml` and applies it to a changed-file
list the same way pre-commit does (Python `re.search`).
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

HOOK_ID = "docs-generated-drift-self-test"


def extract_files_pattern(config_text: str) -> str:
    hook = re.search(
        rf"(?ms)^      - id: {re.escape(HOOK_ID)}\s*$" r"(?P<body>.*?)(?=^      - id: |\Z)",
        config_text,
    )
    if hook is None:
        raise ValueError(f"hook `{HOOK_ID}` not found in .pre-commit-config.yaml")

    match = re.search(r"(?m)^        files:\s*(?P<pattern>\S.*)$", hook.group("body"))
    if match is None:
        raise ValueError(f"hook `{HOOK_ID}` has no inline `files:` regex")
    return match.group("pattern").strip()


def loads_changed_files() -> list[str]:
    return [line.strip() for line in sys.stdin.read().splitlines() if line.strip()]


def scope_flag(changed_files: list[str], pattern: str) -> bool:
    matcher = re.compile(pattern)
    return any(matcher.search(file_path) for file_path in changed_files)


def run_self_test(config_text: str) -> int:
    pattern = extract_files_pattern(config_text)
    if not scope_flag(["scripts/ci/check-docs-generated-drift.sh"], pattern):
        print("FAIL: scope pattern must include scripts/ci/check-docs-generated-drift.sh", file=sys.stderr)
        return 1
    if scope_flag(["crates/assay-core/src/lib.rs"], pattern):
        print("FAIL: scope pattern must not include crates/assay-core/src/lib.rs", file=sys.stderr)
        return 1
    print("docs-generated-drift-self-test-scope self-test=passed")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    config_text = Path(".pre-commit-config.yaml").read_text(encoding="utf-8")
    if args.self_test:
        return run_self_test(config_text)

    changed_files = loads_changed_files()
    if not changed_files:
        print("false")
        return 0

    pattern = extract_files_pattern(config_text)
    print("true" if scope_flag(changed_files, pattern) else "false")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
