#!/usr/bin/env python3
"""Pin Assay's three organization-CI upload-sarif callsites in lockstep."""

from __future__ import annotations

import re
import sys
from pathlib import Path

EXPECTED_SHA = "db488ddef3bf6cb639b32c2e9a7c0a7ea8271d28"
EXPECTED_TAG = "v4.37.8"
WORKFLOWS = (
    Path(".github/workflows/assay-security.yml"),
    Path(".github/workflows/openssf-scorecard.yml"),
    Path(".github/workflows/osv-scanner-scheduled.yml"),
)
WORKFLOW_DIR = Path(".github/workflows")
USES_RE = re.compile(
    r"^[ \t]*(?:-[ \t]+)?uses:[ \t]+"
    r"(?P<quote>['\"]?)github/codeql-action/upload-sarif@"
    r"(?P<sha>[0-9a-f]{40})(?P=quote)[ \t]+"
    r"#[ \t]+(?P<tag>v\d+\.\d+\.\d+)[ \t]*$"
)
ACTIVE_UPLOAD_RE = re.compile(
    r"^[ \t]*(?:-[ \t]+)?uses:[ \t]+['\"]?"
    r"github/codeql-action/upload-sarif@"
)


def active_upload_pins(text: str) -> list[tuple[str, str]]:
    pins: list[tuple[str, str]] = []
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        match = USES_RE.match(line)
        if match:
            pins.append((match.group("sha"), match.group("tag")))
    return pins


def has_active_upload_callsite(text: str) -> bool:
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if ACTIVE_UPLOAD_RE.match(line):
            return True
    return False


def check() -> list[str]:
    errors: list[str] = []
    expected = (EXPECTED_SHA, EXPECTED_TAG)
    expected_paths = set(WORKFLOWS)

    discovered: set[Path] = set()
    for pattern in ("*.yml", "*.yaml"):
        for workflow in WORKFLOW_DIR.glob(pattern):
            if has_active_upload_callsite(workflow.read_text(encoding="utf-8")):
                discovered.add(workflow)
    for workflow in sorted(discovered - expected_paths):
        errors.append(f"unexpected upload-sarif workflow callsite: {workflow}")

    for workflow in WORKFLOWS:
        if not workflow.is_file():
            errors.append(f"workflow missing: {workflow}")
            continue
        pins = active_upload_pins(workflow.read_text(encoding="utf-8"))
        if len(pins) != 1:
            errors.append(
                f"{workflow}: want exactly one active upload-sarif SHA pin, found {len(pins)}"
            )
        elif pins[0] != expected:
            errors.append(
                f"{workflow}: pin @{pins[0][0]} # {pins[0][1]}, "
                f"want @{EXPECTED_SHA} # {EXPECTED_TAG}"
            )
    return errors


def main() -> int:
    errors = check()
    if errors:
        print("FAIL: CodeQL upload-sarif workflow pins", file=sys.stderr)
        for error in errors:
            print(f"  {error}", file=sys.stderr)
        return 1
    print(
        f"ok    {len(WORKFLOWS)} CodeQL upload-sarif callsites "
        f"@{EXPECTED_SHA} # {EXPECTED_TAG}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
