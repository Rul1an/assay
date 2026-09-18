#!/usr/bin/env python3
"""Pin the PR OSV Cargo.lock gate: full-scan reusable, two lockfiles, CI rollup.

The job must call google's full-scan reusable workflow, not the PR-diff variant.
The PR-diff workflow reports only newly introduced vulns and would not have
caught GHSA-3rjw-m598-pq24 on cmov 0.5.2, which sat unchanged on main.

Stdlib only: the required CI Python image is not guaranteed to have PyYAML.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

WORKFLOW = Path(".github/workflows/ci.yml")
JOB_ID = "osv-cargo-lock"
PIN_SHA = "a345acffa64b0eaede81a3d9aae6141214d9c8fc"
PIN_TAG = "v2.6.0"
USES = (
    "google/osv-scanner-action/.github/workflows/"
    f"osv-scanner-reusable.yml@{PIN_SHA}"
)
FORBIDDEN_REUSABLE = "osv-scanner-reusable-pr.yml"
LOCKFILES = ("Cargo.lock", "fuzz/Cargo.lock")
REQUIRED_PERMISSIONS = (
    ("actions", "read"),
    ("contents", "read"),
    ("security-events", "write"),
)
COMMENT_TELLS = ("osv-scanner-reusable-pr.yml", "GHSA-3rjw-m598-pq24")
JOB_KEY_RE = re.compile(rf"^  {re.escape(JOB_ID)}:\s*(?:#.*)?$")
NEXT_JOB_RE = re.compile(r"^  [A-Za-z0-9_][A-Za-z0-9_-]*:\s*(?:#.*)?$")
KEY_RE = re.compile(r"^    (?P<key>[A-Za-z0-9_-]+):\s*(?P<value>.*)$")
PERM_RE = re.compile(r"^      (?P<key>[A-Za-z0-9_-]+):\s*(?P<value>\S+)\s*$")
USES_LINE_RE = re.compile(
    rf"^[ \t]*uses:[ \t]+{re.escape(USES)}[ \t]+#[ \t]+{re.escape(PIN_TAG)}[ \t]*$"
)


def _active(line: str) -> str:
    stripped = line.strip()
    if not stripped or stripped.startswith("#"):
        return ""
    return line


def _job_raw_lines(text: str) -> list[str] | None:
    lines = text.splitlines()
    start = next((i for i, line in enumerate(lines) if JOB_KEY_RE.match(line)), None)
    if start is None:
        return None
    end = next(
        (i for i in range(start + 1, len(lines)) if NEXT_JOB_RE.match(lines[i])),
        len(lines),
    )
    return lines[start:end]


def _job_keys(raw: list[str]) -> dict[str, str]:
    keys: dict[str, str] = {}
    for line in raw:
        if not _active(line):
            continue
        match = KEY_RE.match(line)
        if match:
            value = match["value"].strip()
            if " #" in value:
                value = value.split(" #", 1)[0].rstrip()
            keys[match["key"]] = value
    return keys


def _permissions(raw: list[str]) -> list[tuple[str, str]]:
    found: list[tuple[str, str]] = []
    in_perms = False
    for line in raw:
        if not _active(line):
            continue
        if KEY_RE.match(line):
            in_perms = KEY_RE.match(line)["key"] == "permissions"
            continue
        if in_perms:
            match = PERM_RE.match(line)
            if match:
                found.append((match["key"], match["value"]))
    return found


def _scan_arg_tokens(raw: list[str]) -> list[str] | None:
    tokens: list[str] = []
    in_args = False
    for line in raw:
        if line.startswith("      scan-args:"):
            in_args = True
            rest = line.split(":", 1)[1].strip()
            if rest not in {"|-", "|", ">"}:
                tokens.extend(rest.split())
            continue
        if in_args:
            if line.startswith("        "):
                tokens.extend(line.strip().split())
                continue
            if not _active(line):
                continue
            break
    return tokens if tokens else None


def check(text: str) -> list[str]:
    raw = _job_raw_lines(text)
    if raw is None:
        return [f"missing job `{JOB_ID}`"]

    errors: list[str] = []
    keys = _job_keys(raw)
    uses = keys.get("uses")
    if uses != USES:
        errors.append(f"`{JOB_ID}.uses` must be {USES}, got {uses!r}")
    if uses and FORBIDDEN_REUSABLE in uses:
        errors.append(
            f"`{JOB_ID}` must not call {FORBIDDEN_REUSABLE}; "
            "that variant reports only newly introduced vulns"
        )
    if "timeout-minutes" in keys:
        errors.append(
            f"`{JOB_ID}` is a reusable-workflow caller and must not set timeout-minutes"
        )
    if keys.get("continue-on-error") in {"true", "True", "TRUE"}:
        errors.append(f"`{JOB_ID}` must not set continue-on-error")
    if "if" in keys:
        errors.append(
            f"`{JOB_ID}` must run on every CI event; an `if:` can path-filter it out of the gate"
        )
    fail_on_vuln = next(
        (
            line.strip()
            for line in raw
            if _active(line) and line.startswith("      fail-on-vuln:")
        ),
        "",
    )
    if fail_on_vuln.split(":", 1)[-1].strip() in {"false", "False", "FALSE"}:
        errors.append(f"`{JOB_ID}` must not set fail-on-vuln: false")

    permissions = _permissions(raw)
    if tuple(permissions) != REQUIRED_PERMISSIONS:
        errors.append(
            f"`{JOB_ID}` permissions must be exactly {list(REQUIRED_PERMISSIONS)}, "
            f"got {permissions!r}"
        )

    tokens = _scan_arg_tokens(raw)
    expected = [f"--lockfile={path}" for path in LOCKFILES]
    if tokens is None:
        errors.append(f"`{JOB_ID}` must pass scan-args naming only the two lockfiles")
    else:
        extra = [tok for tok in tokens if tok not in expected]
        missing = [tok for tok in expected if tok not in tokens]
        if extra or missing:
            errors.append(
                f"`{JOB_ID}` scan-args must be exactly {expected}, "
                f"missing={missing!r} extra={extra!r}"
            )
        if any(tok in {"-r", "--recursive", "./"} for tok in tokens):
            errors.append(
                f"`{JOB_ID}` scan-args must not use --recursive or ./ "
                "(example lockfiles stay out of scope)"
            )

    uses_lines = [line for line in raw if USES_LINE_RE.match(line)]
    if len(uses_lines) != 1:
        errors.append(
            f"`{JOB_ID}` must have exactly one active uses line "
            f"`{USES} # {PIN_TAG}`, found {len(uses_lines)}"
        )

    block = "\n".join(raw)
    for tell in COMMENT_TELLS:
        if tell not in block:
            errors.append(
                f"`{JOB_ID}` must keep a comment naming {tell} so the PR-diff "
                "variant is not 'optimised' back in"
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
    print(f"ok    {WORKFLOW} {JOB_ID}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
