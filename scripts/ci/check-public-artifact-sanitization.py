#!/usr/bin/env python3
"""Public-safe artifact sanitization guard.

This check is intentionally safe for public PR logs:

- it does not carry the private sensitive-vocabulary list;
- it reports only category names and locations;
- it never prints matched text.

The trusted/private-list comparison described in CI-CONTRACT.md can be layered on
top later. This script is the required fork-safe structural gate.
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Sequence


REPO_ROOT = Path(__file__).resolve().parents[2]

EXCLUDED_PARTS = {
    ".git",
    ".hg",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".tox",
    ".venv",
    "__pycache__",
    "node_modules",
    "target",
    "dist",
    "build",
    "vendor",
}

EXCLUDED_SUFFIXES = {
    ".7z",
    ".a",
    ".bin",
    ".bmp",
    ".bz2",
    ".class",
    ".dll",
    ".dylib",
    ".exe",
    ".gif",
    ".gz",
    ".ico",
    ".jar",
    ".jpeg",
    ".jpg",
    ".lockb",
    ".o",
    ".pdf",
    ".png",
    ".rlib",
    ".so",
    ".tar",
    ".tgz",
    ".wasm",
    ".webp",
    ".zip",
}

TEXT_BYTES_LIMIT = 2_000_000


@dataclass(frozen=True)
class PatternRule:
    category: str
    pattern: re.Pattern[str]


RULES: tuple[PatternRule, ...] = (
    PatternRule(
        "publication_blocker_marker",
        re.compile(
            r"\b("
            r"REDACT[\s_-]*BEFORE[\s_-]*PUBLIC|"
            r"PUBLICATION[\s_-]*BLOCKED|"
            r"PRIVATE[\s_-]*NOTES?[\s_-]*ONLY"
            r")\b",
            re.IGNORECASE,
        ),
    ),
    PatternRule(
        "secret_placeholder_marker",
        re.compile(
            r"\b("
            r"REPLACE[\s_-]*WITH[\s_-]*REAL[\s_-]*(TOKEN|SECRET|KEY)|"
            r"PASTE[\s_-]*(TOKEN|SECRET|KEY)[\s_-]*HERE"
            r")\b",
            re.IGNORECASE,
        ),
    ),
    PatternRule(
        "private_log_marker",
        re.compile(
            r"\b("
            r"RAW[\s_-]*PRIVATE[\s_-]*RUN|"
            r"PRIVATE[\s_-]*RUN[\s_-]*LOG|"
            r"UNREDACTED[\s_-]*(REQUEST|RESPONSE|TRACE)"
            r")\b",
            re.IGNORECASE,
        ),
    ),
)


@dataclass(frozen=True)
class Finding:
    path: Path
    line: int
    category: str


def git_files(root: Path) -> list[Path]:
    proc = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=root,
        check=True,
        stdout=subprocess.PIPE,
    )
    return [root / item.decode("utf-8") for item in proc.stdout.split(b"\0") if item]


def should_scan(path: Path, root: Path) -> bool:
    rel = path.relative_to(root)
    if any(part in EXCLUDED_PARTS for part in rel.parts):
        return False
    if path.suffix.lower() in EXCLUDED_SUFFIXES:
        return False
    try:
        if path.stat().st_size > TEXT_BYTES_LIMIT:
            return False
    except FileNotFoundError:
        return False
    return True


def read_text(path: Path) -> str | None:
    try:
        data = path.read_bytes()
    except OSError:
        return None
    if b"\0" in data:
        return None
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return data.decode("utf-8", errors="ignore")


def scan_files(files: Iterable[Path], root: Path) -> list[Finding]:
    findings: list[Finding] = []
    for path in files:
        if not should_scan(path, root):
            continue
        text = read_text(path)
        if text is None:
            continue
        rel = path.relative_to(root)
        for line_no, line in enumerate(text.splitlines(), start=1):
            for rule in RULES:
                if rule.pattern.search(line):
                    findings.append(Finding(rel, line_no, rule.category))
    return findings


def print_findings(findings: Sequence[Finding]) -> None:
    counts: dict[str, int] = {}
    for finding in findings:
        counts[finding.category] = counts.get(finding.category, 0) + 1

    print("public-artifact-sanitization=failed")
    print(f"finding_count={len(findings)}")
    for category in sorted(counts):
        print(f"category_count {category}={counts[category]}")

    print("locations:")
    for finding in findings:
        print(f"- {finding.path}:{finding.line} category={finding.category}")


def self_test() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        marker = "".join(("REDACT", "_BEFORE_PUBLIC"))
        (root / "README.md").write_text(
            f"hello\n{marker}: placeholder\n", encoding="utf-8"
        )
        (root / "target").mkdir()
        (root / "target" / "generated.txt").write_text(
            "DO NOT PUBLISH\n", encoding="utf-8"
        )
        findings = scan_files([root / "README.md", root / "target" / "generated.txt"], root)
        assert len(findings) == 1
        assert findings[0].path == Path("README.md")
        assert findings[0].category == "publication_blocker_marker"
    print("self-test=passed")


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)

    if args.self_test:
        self_test()
        return 0

    root = Path(os.environ.get("GITHUB_WORKSPACE", REPO_ROOT)).resolve()
    findings = scan_files(git_files(root), root)
    if findings:
        print_findings(findings)
        return 1
    print("public-artifact-sanitization=passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
