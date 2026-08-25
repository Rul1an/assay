#!/usr/bin/env python3
"""Lockstep + bind-invocation contract for the scheduled OSV pair.

The reporter fail-opens on unreadable JSON (warn, empty results, exit 0). The
runtime rule lives in scripts/ci/bind-osv-json-sarif-counts.py. This checker
only pins that the workflow calls that script with both artifacts, and that
scanner/reporter share one 40-hex SHA and semver comment.

It does not reimplement the count. Does not run the actions.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

WORKFLOW = Path(".github/workflows/osv-scanner-scheduled.yml")
BIND_SCRIPT = "scripts/ci/bind-osv-json-sarif-counts.py"

USES_RE = re.compile(
    r"^[ \t]*uses:[ \t]+google/osv-scanner-action/"
    r"(osv-scanner-action|osv-reporter-action)"
    r"@([0-9a-f]{40})[ \t]+#[ \t]+(v\d+\.\d+\.\d+)[ \t]*$",
)

# Active invocation: the workflow must call the one runtime script with no
# path args. Filenames are fixed inside the script (cwd osv-results.json/.sarif).
INVOKE_RE = re.compile(
    r"^[ \t]+(?:run:[ \t]+)?python3[ \t]+scripts/ci/bind-osv-json-sarif-counts\.py[ \t]*$"
)


def _active_lines(text: str) -> list[str]:
    out: list[str] = []
    for raw in text.splitlines():
        stripped = raw.strip()
        if not stripped or stripped.startswith("#"):
            continue
        out.append(raw)
    return out


def check(text: str) -> list[str]:
    errors: list[str] = []
    active_lines = _active_lines(text)
    active = "\n".join(active_lines)

    by_kind: dict[str, list[tuple[str, str]]] = {
        "osv-scanner-action": [],
        "osv-reporter-action": [],
    }
    for line in active_lines:
        m = USES_RE.match(line)
        if not m:
            continue
        by_kind[m.group(1)].append((m.group(2), m.group(3)))

    for kind in ("osv-scanner-action", "osv-reporter-action"):
        found = by_kind[kind]
        if len(found) != 1:
            errors.append(
                f"{kind}: want exactly one active SHA-pin uses line, found {len(found)}"
            )
    if len(by_kind["osv-scanner-action"]) == 1 and len(by_kind["osv-reporter-action"]) == 1:
        scan_sha, scan_tag = by_kind["osv-scanner-action"][0]
        rep_sha, rep_tag = by_kind["osv-reporter-action"][0]
        if scan_sha != rep_sha or scan_tag != rep_tag:
            errors.append(
                "scanner/reporter pin drift: "
                f"scanner @{scan_sha} #{scan_tag} vs reporter @{rep_sha} #{rep_tag}"
            )

    if "continue-on-error: true" not in active:
        errors.append("scanner continue-on-error: true is missing")
    if "--fail-on-vuln=false" not in active:
        errors.append("reporter --fail-on-vuln=false is missing")
    if "category: osv-scanner-non-rust" not in active:
        errors.append("upload category osv-scanner-non-rust is missing")

    invokes = [line for line in active_lines if INVOKE_RE.match(line)]
    if len(invokes) != 1:
        errors.append(
            "want exactly one active invocation "
            f"`python3 {BIND_SCRIPT}`, "
            f"found {len(invokes)}"
        )

    return errors


def main() -> int:
    if not WORKFLOW.is_file():
        print(f"FAIL: workflow missing: {WORKFLOW}", file=sys.stderr)
        return 2
    errors = check(WORKFLOW.read_text(encoding="utf-8"))
    if errors:
        print(f"FAIL: {WORKFLOW}", file=sys.stderr)
        for err in errors:
            print(f"  {err}", file=sys.stderr)
        return 1
    print(f"ok    {WORKFLOW}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
