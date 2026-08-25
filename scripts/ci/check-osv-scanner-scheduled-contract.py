#!/usr/bin/env python3
"""Lockstep + count-bind contract for the scheduled OSV scanner/reporter pair.

The reporter fail-opens on unreadable JSON (warn, empty results, exit 0). A
version-skewed pair can therefore upload an empty SARIF while the scanner JSON
still lists vulnerabilities. This checker refuses that hole in the workflow
text: scanner and reporter must share one 40-hex SHA and semver comment, and a
non-comment bind must compare the JSON vulnerability walk to the SARIF result
count and exit non-zero on mismatch.

Does not run the actions. Does not claim live 2.5.1 JSON/SARIF compatibility.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

WORKFLOW = Path(".github/workflows/osv-scanner-scheduled.yml")

USES_RE = re.compile(
    r"^[ \t]*uses:[ \t]+google/osv-scanner-action/"
    r"(osv-scanner-action|osv-reporter-action)"
    r"@([0-9a-f]{40})[ \t]+#[ \t]+(v\d+\.\d+\.\d+)[ \t]*$",
    re.M,
)

# Distinctive identifiers the bind step must use so a comment or a no-op
# `if False` cannot satisfy the contract.
BIND_CONDITION = "osv_json_vuln_count != osv_sarif_result_count"
JSON_NAME = "osv-results.json"
SARIF_NAME = "osv-results.sarif"


def _active_lines(text: str) -> list[str]:
    """Lines that are not empty and not full-line comments."""
    out: list[str] = []
    for raw in text.splitlines():
        stripped = raw.strip()
        if not stripped or stripped.startswith("#"):
            continue
        out.append(raw)
    return out


def check(text: str) -> list[str]:
    errors: list[str] = []
    active = "\n".join(_active_lines(text))
    uses = USES_RE.findall(text)
    by_kind: dict[str, list[tuple[str, str]]] = {"osv-scanner-action": [], "osv-reporter-action": []}
    for kind, sha, tag in uses:
        # Ignore matches that live on a commented uses line.
        by_kind[kind].append((sha, tag))

    # Re-scan line by line so a commented uses: is not counted.
    by_kind = {"osv-scanner-action": [], "osv-reporter-action": []}
    for line in _active_lines(text):
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

    if JSON_NAME not in active or SARIF_NAME not in active:
        errors.append("workflow no longer names both osv-results.json and osv-results.sarif")

    if BIND_CONDITION not in active:
        errors.append(
            "missing fail-closed count bind "
            f"({BIND_CONDITION} on an active line); "
            "reporter parse-fail can upload empty SARIF at exit 0"
        )
    elif "sys.exit(1)" not in active and "exit 1" not in active:
        errors.append("count bind has no non-zero exit")

    # The bind must actually read both artifacts, not just mention them in uses/with.
    bind_reads_json = re.search(
        r"(?:open|Path|vulnerability_count)\(\s*['\"]osv-results\.json['\"]",
        active,
    )
    bind_reads_sarif = re.search(
        r"(?:open|Path|sarif_result_count)\(\s*['\"]osv-results\.sarif['\"]",
        active,
    )
    if BIND_CONDITION in active and not (bind_reads_json and bind_reads_sarif):
        errors.append("count bind does not read both osv-results.json and osv-results.sarif")

    return errors


def main() -> int:
    path = Path(sys.argv[1]) if len(sys.argv) > 1 else WORKFLOW
    if not path.is_file():
        print(f"FAIL: workflow missing: {path}", file=sys.stderr)
        return 2
    errors = check(path.read_text(encoding="utf-8"))
    if errors:
        print(f"FAIL: {path}", file=sys.stderr)
        for err in errors:
            print(f"  {err}", file=sys.stderr)
        return 1
    print(f"ok    {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
