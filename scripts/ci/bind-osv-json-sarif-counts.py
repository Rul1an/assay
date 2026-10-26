#!/usr/bin/env python3
"""Bind pinned OSV JSON and SARIF by alias group, package and manifest.

Raw counts can differ because the reporter repeats groups for each alias ID.
Exact duplicate results are accepted; missing or extra findings fail closed.
This is report-transfer coverage, not advisory authenticity or exploitability.
OSV_OUTCOME is supplied by the scheduled workflow; fixed artifact filenames
and the scanner execution-failure rule are owned here.
"""

from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path
from typing import Any


def object_value(value: Any, label: str) -> dict:
    if not isinstance(value, dict):
        raise ValueError(f"{label} is not an object")
    return value


def list_value(value: Any, label: str) -> list:
    if not isinstance(value, list):
        raise ValueError(f"{label} is not a list")
    return value


def text_value(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value or any(ord(c) < 32 for c in value):
        raise ValueError(f"{label} is not a nonempty printable string")
    return value


def source_path(value: Any) -> str:
    path = text_value(value, "source path").removeprefix("/github/workspace/")
    if path.startswith("/") or any(p in ("", ".", "..") for p in path.split("/")):
        raise ValueError("unsupported source path")
    return path


def scanner_findings(data: Any) -> tuple[set, dict, int]:
    """Bind the pinned reporter's alias groups, not nested advisory metadata."""
    expected = set()
    aliases = {}
    count = 0
    identities = {}
    for src in list_value(object_value(data, "OSV JSON").get("results"), "OSV results"):
        src = object_value(src, "source result")
        path = source_path(object_value(src.get("source"), "source").get("path"))
        for entry in list_value(src.get("packages"), "packages"):
            entry = object_value(entry, "package result")
            pkg = object_value(entry.get("package"), "package")
            if pkg.get("commit"):
                raise ValueError("commit packages are outside the scheduled manifest contract")
            name = text_value(pkg.get("name"), "package name")
            version = text_value(pkg.get("version"), "package version")
            ecosystem = text_value(pkg.get("ecosystem"), "package ecosystem")
            display = f"{name}@{version}"
            identity = (name, version, ecosystem)
            key = (path, display)
            if key in identities and identities[key] != identity:
                raise ValueError("ambiguous reporter package identity")
            identities[key] = identity
            vulnerabilities = list_value(entry.get("vulnerabilities", []), "vulnerabilities")
            vuln_ids = {text_value(object_value(v, "vulnerability").get("id"), "vulnerability ID")
                        for v in vulnerabilities}
            count += len(vulnerabilities)
            grouped = set()
            for group in list_value(entry.get("groups", []), "groups"):
                group = object_value(group, "group")
                ids = [text_value(i, "group ID") for i in list_value(group.get("ids"), "group IDs")]
                if not ids or not set(ids) <= vuln_ids:
                    raise ValueError("empty group or group without matching vulnerabilities")
                grouped.update(ids)
                key = (tuple(sorted(ids)), path, display)
                expected.add(key)
                for identifier in ids + list_value(group.get("aliases", []), "group aliases"):
                    identifier = text_value(identifier, "advisory alias")
                    lookup = (path, display, identifier)
                    if lookup in aliases and aliases[lookup] != key:
                        raise ValueError("ambiguous alias within package source")
                    aliases[lookup] = key
            if grouped != vuln_ids:
                raise ValueError("vulnerability is missing its reporter group")
    return expected, aliases, count


def reporter_findings(data: Any, aliases: dict) -> tuple[set, int]:
    findings = set()
    copies = {}
    count = 0
    runs = list_value(object_value(data, "OSV SARIF").get("runs"), "SARIF runs")
    if len(runs) != 1:
        raise ValueError("expected one pinned OSV reporter run")
    for run in runs:
        for result in list_value(object_value(run, "SARIF run").get("results"), "SARIF results"):
            result = object_value(result, "SARIF result")
            identifier = text_value(result.get("ruleId"), "SARIF ruleId")
            message = text_value(object_value(result.get("message"), "message").get("text"), "message text")
            match = re.fullmatch(r"Package '([^'\n]+)' is vulnerable to '([^'\n]+)'(?: \(also known as '[^\n]*'\))?\.", message)
            if not match or match[2] != identifier:
                raise ValueError("unsupported or inconsistent OSV package message")
            locations = list_value(result.get("locations"), "SARIF locations")
            if len(locations) != 1:
                raise ValueError("expected one OSV result location")
            location = object_value(locations[0], "location")
            physical = object_value(location.get("physicalLocation"), "physical location")
            artifact = object_value(physical.get("artifactLocation"), "artifact location")
            path = source_path(artifact.get("uri"))
            # Match only aliases declared for this package and manifest, not globally.
            key = aliases.get((path, match[1], identifier), ((identifier,), path, match[1]))
            encoded = json.dumps(result, sort_keys=True, separators=(",", ":"))
            if key in copies and copies[key] != encoded:
                raise ValueError("non-identical duplicate SARIF finding")
            copies[key] = encoded
            findings.add(key)
            count += 1
    return findings, count


def refuse_if_findings_differ(expected: set, actual: set) -> None:
    if expected != actual:
        missing, extra = sorted(expected - actual), sorted(actual - expected)
        raise ValueError(
            f"OSV finding mismatch: missing={len(missing)} extra={len(extra)}; "
            f"missing sample={missing[:5]!r}; extra sample={extra[:5]!r}"
        )


def read_osv_outcome() -> str:
    outcome = os.environ.get("OSV_OUTCOME")
    if outcome is None or outcome == "":
        print(
            "::error::OSV_OUTCOME is unset or empty; refusing (wiring hole)",
            file=sys.stderr,
        )
        raise SystemExit(1)
    return outcome


def refuse_if_scanner_execution_failed(outcome: str, json_count: int) -> None:
    """Non-success with zero JSON vulns is a scanner crash, not a clean scan."""
    if outcome != "success" and json_count == 0:
        print(
            "::error::OSV scanner exited non-zero without vulnerability results; treating this as scanner execution failure",
            file=sys.stderr,
        )
        raise SystemExit(1)


def bind(json_path: Path, sarif_path: Path) -> None:
    outcome = read_osv_outcome()
    if not json_path.is_file() or json_path.stat().st_size == 0:
        print(f"::error::OSV JSON result missing: {json_path}", file=sys.stderr)
        raise SystemExit(1)
    if not sarif_path.is_file() or sarif_path.stat().st_size == 0:
        print(f"::error::OSV SARIF result missing: {sarif_path}", file=sys.stderr)
        raise SystemExit(1)
    try:
        json_data = json.loads(json_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        print(f"::error::OSV JSON is malformed: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
    try:
        sarif_data = json.loads(sarif_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        print(f"::error::OSV SARIF is malformed: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
    try:
        expected, aliases, json_count = scanner_findings(json_data)
        actual, sarif_count = reporter_findings(sarif_data, aliases)
        refuse_if_findings_differ(expected, actual)
    except ValueError as exc:
        print(f"::error::{exc}", file=sys.stderr)
        raise SystemExit(1) from exc
    print(f"osv-json-vulnerabilities={json_count}")
    print(f"osv-sarif-results={sarif_count}")
    refuse_if_scanner_execution_failed(outcome, json_count)
    print(f"osv-distinct-findings={len(expected)}")


def main() -> int:
    if len(sys.argv) != 1:
        print("usage: bind-osv-json-sarif-counts.py", file=sys.stderr)
        return 2
    try:
        bind(Path("osv-results.json"), Path("osv-results.sarif"))
    except SystemExit as exc:
        code = exc.code
        return int(code) if isinstance(code, int) else 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
