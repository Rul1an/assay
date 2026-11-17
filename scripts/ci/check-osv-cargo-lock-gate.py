#!/usr/bin/env python3
"""Pin the PR OSV Cargo.lock gate: full-scan reusable, two lockfiles, CI rollup.

The job must call google's full-scan reusable workflow, not the PR-diff variant.
The PR-diff workflow reports only newly introduced vulns and would not have
caught GHSA-3rjw-m598-pq24 on cmov 0.5.2, which sat unchanged on main.

Stdlib only: the required CI Python image is not guaranteed to have PyYAML.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
import tomllib
from datetime import date
from pathlib import Path

WORKFLOW = Path(".github/workflows/ci.yml")
OSV_TOML = Path("osv-scanner.toml")
# OSV applies an osv-scanner.toml only to files in its own directory (docs/configuration.md at
# v2.7.0), so the root file never reaches fuzz/Cargo.lock. The fuzz file is optional: no file
# means no exception. When it exists it obeys the same rules, because a quoted ignoreUntil
# there makes OSV drop that whole file just as it does for the root one.
FUZZ_OSV_TOML = Path("fuzz/osv-scanner.toml")
# cargo-deny reads the root workspace, so its ignores apply to Cargo.lock, the lockfile the
# root osv-scanner.toml covers. cargo-deny 0.20.2 takes only `id` and `reason` on an ignore
# and rejects an expiry key, so an advisory ignored there is dated by the root OSV file.
DENY_TOML = Path("deny.toml")
# Match cargo-deny 0.20.2's string classification in src/advisories/cfg.rs:
# a hyphen with only uppercase ASCII characters before it denotes an advisory.
# `*` also preserves its all() result for an empty prefix; this classifies ignores,
# it does not validate advisory IDs or crate specs on cargo-deny's behalf.
ADVISORY_ID_RE = re.compile(r"^[A-Z]*-")
IGNORED_VULN_KEYS = frozenset({"id", "reason", "ignoreUntil"})
JOB_ID = "osv-cargo-lock"
PIN_SHA = "b1d267aea5211fa648d9d442fa64b90dbdcbdd36"
PIN_TAG = "v2.7.0"
USES = (
    "google/osv-scanner-action/.github/workflows/"
    f"osv-scanner-reusable.yml@{PIN_SHA}"
)
FORBIDDEN_REUSABLE = "osv-scanner-reusable-pr.yml"
EXPECTED_WITH_RUNS_ON = "ubuntu-24.04"
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


def _parsed_with_runs_on(text: str) -> tuple[object, str | None]:
    """Read effective `with.runs-on` via Ruby Psych (existing parsed route).

    A line/text pin would accept an inert comment or string scalar holding
    "runs-on: ubuntu-24.04" while the effective input is missing, and would
    miss the actual consumed mapping. Parsing the mapping proves effective
    consumption. Same subprocess shape as
    scripts/ci/check-actions-attest-lockstep.py and
    scripts/ci/test-ci-job-timeouts-contract.sh
    (`YAML.safe_load(STDIN.read, aliases: false)` over stdout as JSON);
    ruby is already required by those CI gates, so no new dependency.
    Fail closed: missing ruby, malformed YAML, or a non-mapping shape is a
    named error, never a pass.
    """
    try:
        completed = subprocess.run(
            [
                "ruby",
                "-ryaml",
                "-rjson",
                "-e",
                "puts JSON.generate(YAML.safe_load(STDIN.read, aliases: false))",
            ],
            input=text,
            capture_output=True,
            text=True,
            timeout=60,
        )
    except OSError as exc:
        return None, f"`{JOB_ID}` with.runs-on: yaml parser unavailable: {exc}"
    except subprocess.TimeoutExpired:
        return None, f"`{JOB_ID}` with.runs-on: yaml parser timed out"
    if completed.returncode != 0:
        detail = (completed.stderr.strip() or completed.stdout.strip())[:500]
        return None, f"`{JOB_ID}` with.runs-on: malformed workflow YAML: {detail}"
    try:
        document = json.loads(completed.stdout)
    except json.JSONDecodeError as exc:
        return None, f"`{JOB_ID}` with.runs-on: yaml parser returned non-JSON: {exc}"
    if not isinstance(document, dict):
        return None, f"`{JOB_ID}` with.runs-on: workflow root is not a mapping"
    jobs = document.get("jobs")
    if not isinstance(jobs, dict):
        return None, f"`{JOB_ID}` with.runs-on: workflow jobs is not a mapping"
    job = jobs.get(JOB_ID)
    if not isinstance(job, dict):
        return None, f"missing job `{JOB_ID}`"
    with_block = job.get("with")
    if not isinstance(with_block, dict):
        return None, (
            f"`{JOB_ID}` must consume with.runs-on: {EXPECTED_WITH_RUNS_ON} "
            f"(effective parsed value), got no `with:` mapping"
        )
    return with_block.get("runs-on"), None


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

    effective_runs_on, parse_error = _parsed_with_runs_on(text)
    if parse_error is not None:
        errors.append(parse_error)
    elif effective_runs_on != EXPECTED_WITH_RUNS_ON:
        errors.append(
            f"`{JOB_ID}` must consume with.runs-on: {EXPECTED_WITH_RUNS_ON} "
            f"(effective parsed value), got {effective_runs_on!r}"
        )
    return errors


def check_ignored_vulns(path: Path) -> list[str]:
    """Fail closed on [[IgnoredVulns]] so a comment is not the expiry rule.

    OSV treats a quoted ignoreUntil as RFC3339 and then ignores the whole file
    (upstream manager.go). A typo like ignoreUtil would otherwise drop expiry.
    """
    if not path.is_file():
        return [f"missing `{path}`"]
    try:
        data = tomllib.loads(path.read_text(encoding="utf-8"))
    except tomllib.TOMLDecodeError as exc:
        return [f"{path}: TOML parse error: {exc}"]

    entries = data.get("IgnoredVulns", [])
    if not isinstance(entries, list):
        return [f"{path}: IgnoredVulns must be an array of tables"]

    errors: list[str] = []
    for index, entry in enumerate(entries):
        prefix = f"{path} IgnoredVulns[{index}]"
        if not isinstance(entry, dict):
            errors.append(f"{prefix}: must be a table")
            continue
        for key in sorted(set(entry) - IGNORED_VULN_KEYS):
            errors.append(f"{prefix}: unknown key `{key}`")
        ident = entry.get("id")
        if not isinstance(ident, str) or not ident.strip():
            errors.append(f"{prefix}: missing or empty `id`")
        if "reason" not in entry:
            errors.append(f"{prefix}: missing `reason`")
        elif not isinstance(entry["reason"], str) or not entry["reason"].strip():
            errors.append(f"{prefix}: empty `reason`")
        if "ignoreUntil" not in entry:
            errors.append(f"{prefix}: missing `ignoreUntil`")
        elif type(entry["ignoreUntil"]) is not date:
            errors.append(
                f"{prefix}: `ignoreUntil` must be an unquoted TOML date "
                "(2026-12-31); a quoted string or datetime with a time part "
                "is parsed differently and OSV ignores the whole file"
            )
    return errors


def dated_ids(path: Path) -> tuple[dict[str, date], list[str]]:
    """The ID -> ignoreUntil view of one osv-scanner.toml, for the cross-file rules.

    Entries that `check_ignored_vulns` already refuses (no id, no date, a quoted date) are
    left out without a second error: that function reports them, and a rule that compares
    dates has nothing to compare. An ID that appears twice has no single date, which is
    an error of its own.
    """
    if not path.is_file():
        return {}, []
    try:
        data = tomllib.loads(path.read_text(encoding="utf-8"))
    except tomllib.TOMLDecodeError:
        return {}, []
    entries = data.get("IgnoredVulns", [])
    if not isinstance(entries, list):
        return {}, []

    dates: dict[str, date] = {}
    errors: list[str] = []
    for entry in entries:
        if not isinstance(entry, dict):
            continue
        ident = entry.get("id")
        until = entry.get("ignoreUntil")
        if not isinstance(ident, str) or not ident.strip() or type(until) is not date:
            continue
        if ident in dates:
            errors.append(
                f"{path}: `{ident}` is listed more than once, so it has no single ignoreUntil"
            )
            continue
        dates[ident] = until
    return dates, errors


def check_osv_date_parity(
    root: Path, root_dates: dict[str, date], fuzz: Path, fuzz_dates: dict[str, date]
) -> list[str]:
    """An ID excepted for both lockfiles is one exception and ends on one day.

    An ID in one file only is legitimate: an advisory can match one lockfile and not the
    other. The rule is about IDs both files carry.
    """
    return [
        f"`{ident}` has two ignoreUntil dates ({root}: {root_dates[ident]}, "
        f"{fuzz}: {fuzz_dates[ident]}); it is one exception and needs one date"
        for ident in sorted(set(root_dates) & set(fuzz_dates))
        if root_dates[ident] != fuzz_dates[ident]
    ]


def deny_advisory_ignores(path: Path) -> tuple[list[str], list[str]]:
    """The advisory IDs that deny.toml's [advisories] ignore list names.

    cargo-deny takes an entry as a string or as a table. A table names an advisory with `id`
    or a crate with `crate`; a string is an advisory ID or a crate spec for a yanked version.
    Crate specs are not advisories and are skipped. An entry that is none of these is an
    error, not a skip, so an unreadable entry cannot pass as "not an advisory".
    """
    if not path.is_file():
        return [], [f"missing `{path}`"]
    try:
        data = tomllib.loads(path.read_text(encoding="utf-8"))
    except tomllib.TOMLDecodeError as exc:
        return [], [f"{path}: TOML parse error: {exc}"]

    advisories = data.get("advisories", {})
    entries = advisories.get("ignore", []) if isinstance(advisories, dict) else []
    if not isinstance(entries, list):
        return [], [f"{path}: [advisories] ignore must be an array"]

    idents: list[str] = []
    errors: list[str] = []
    for index, entry in enumerate(entries):
        prefix = f"{path} [advisories] ignore[{index}]"
        if isinstance(entry, str):
            if ADVISORY_ID_RE.match(entry):
                idents.append(entry)
            continue
        if isinstance(entry, dict):
            ident = entry.get("id")
            if isinstance(ident, str) and ident.strip():
                idents.append(ident)
                continue
            crate = entry.get("crate")
            if isinstance(crate, str) and crate.strip():
                continue
        errors.append(f"{prefix}: neither an advisory `id` nor a `crate` spec")
    return idents, errors


def check_deny_ignores_dated(
    deny: Path, idents: list[str], root: Path, root_dates: dict[str, date]
) -> list[str]:
    """Every advisory deny.toml ignores has a dated entry in the root OSV file."""
    return [
        f"{deny} ignores `{ident}` but {root} has no dated entry for it; an ignore in "
        f"{deny} has no expiry of its own, so the ignoreUntil in {root} is its date"
        for ident in sorted(set(idents))
        if ident not in root_dates
    ]


def main() -> int:
    if not WORKFLOW.is_file():
        print(f"FAIL: workflow missing: {WORKFLOW}", file=sys.stderr)
        return 2
    errors = check(WORKFLOW.read_text(encoding="utf-8"))
    errors.extend(check_ignored_vulns(OSV_TOML))
    fuzz_toml_present = FUZZ_OSV_TOML.is_file()
    if fuzz_toml_present:
        errors.extend(check_ignored_vulns(FUZZ_OSV_TOML))
    root_dates, root_dup_errors = dated_ids(OSV_TOML)
    fuzz_dates, fuzz_dup_errors = dated_ids(FUZZ_OSV_TOML)
    errors.extend(root_dup_errors)
    errors.extend(fuzz_dup_errors)
    errors.extend(check_osv_date_parity(OSV_TOML, root_dates, FUZZ_OSV_TOML, fuzz_dates))
    deny_idents, deny_errors = deny_advisory_ignores(DENY_TOML)
    errors.extend(deny_errors)
    errors.extend(check_deny_ignores_dated(DENY_TOML, deny_idents, OSV_TOML, root_dates))
    if errors:
        print("FAIL: OSV Cargo.lock gate", file=sys.stderr)
        for err in errors:
            print(f"  {err}", file=sys.stderr)
        return 1
    print(f"ok    {WORKFLOW} {JOB_ID}")
    print(f"ok    {OSV_TOML} IgnoredVulns")
    if fuzz_toml_present:
        print(f"ok    {FUZZ_OSV_TOML} IgnoredVulns")
        print(f"ok    {OSV_TOML} and {FUZZ_OSV_TOML} agree on every shared ignoreUntil")
    print(f"ok    {DENY_TOML} advisory ignores are dated in {OSV_TOML}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
