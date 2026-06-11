#!/usr/bin/env python3
"""Claims-boundary guard for public prose.

This advisory check catches narrow, affirmative wording that equates weak
evidence signals with strong security, trust, or compliance claims. It is
intentionally not a private-vocabulary sanitizer: findings print the public
sentence so authors can repair the wording.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Sequence


REPO_ROOT = Path(__file__).resolve().parents[2]

ALLOW_RE = re.compile(
    r"(?:<!--\s*)?(?:#\s*)?claims-guard:\s*allow:\s*\S+",
    re.IGNORECASE,
)

NEGATION_RE = re.compile(
    r"\b(?:not|never|no|isn't|isnt|doesn't|doesnt|does\s+not|"
    r"is\s+not|are\s+not|rather\s+than)\b|!=",
    re.IGNORECASE,
)

BOUNDARY_CONTEXT_RE = re.compile(
    r"^\s*(?:[-*]\s*)?(?:implying|avoid(?:ing)?|must\s+not\s+imply|"
    r"should\s+not\s+imply)\b",
    re.IGNORECASE,
)

CLAUSE_BOUNDARY_RE = re.compile(r"[.;:!?()\[\]{}]|--")


@dataclass(frozen=True)
class ClaimRule:
    name: str
    pattern: re.Pattern[str]
    suggestion: str


@dataclass(frozen=True)
class Finding:
    path: Path
    line: int
    rule: str
    sentence: str
    suggestion: str


RULES: tuple[ClaimRule, ...] = (
    ClaimRule(
        "gate-as-truth",
        re.compile(
            r"\b(?:CI|gate|gates|check|checks|scan|scans|badge|green|passing)\b"
            r"[\w\s,/'\"`-]{0,90}"
            r"\b(?:means|proves|guarantees|ensures)\b"
            r"[\w\s,/'\"`-]{0,90}"
            r"\b(?:secure|safe|compliant|trusted|production-ready|"
            r"production\s+ready|true)\b",
            re.IGNORECASE,
        ),
        "Say what the gate observed, and name the boundary it does not prove.",
    ),
    ClaimRule(
        "proof-of-x",
        re.compile(
            r"\b(?:proof|guarantee)s?\s+of\s+"
            r"(?:compliance|security|safety|trust)\b",
            re.IGNORECASE,
        ),
        "Use evidence, signal, fixture, or check result unless a reviewed proof exists.",
    ),
    ClaimRule(
        "tool-guarantees",
        re.compile(
            r"\b(?:Assay|this\s+action|the\s+gate|the\s+check)\b"
            r"[\w\s,/'\"`-]{0,90}"
            r"\b(?:proves|guarantees|ensures)\b"
            r"[\w\s,/'\"`-]{0,90}"
            r"\b(?:agent|tool|call|tool\s+call)\b"
            r"[\w\s,/'\"`-]{0,60}"
            r"\b(?:safe|secure|compliant|trusted)\b",
            re.IGNORECASE,
        ),
        "State the artifact or decision checked, not that the agent/tool is guaranteed.",
    ),
)


def git_files(root: Path) -> list[Path]:
    proc = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=root,
        check=True,
        stdout=subprocess.PIPE,
    )
    return [root / item.decode("utf-8") for item in proc.stdout.split(b"\0") if item]


def is_scoped_public_prose(path: Path, root: Path) -> bool:
    rel = path.relative_to(root)
    rel_posix = rel.as_posix()
    if rel_posix in {"README.md", "CHANGELOG.md", "CI-CONTRACT.md"}:
        return True
    if rel.parts and rel.parts[0] == "docs":
        return path.suffix.lower() in {".md", ".mdx", ".rst", ".txt"}
    if rel_posix in {"assay-action/action.yml", "assay-action/action.yaml"}:
        return True
    return False


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


def clause_prefix(line: str, start: int) -> str:
    prefix = line[:start]
    parts = CLAUSE_BOUNDARY_RE.split(prefix)
    return parts[-1] if parts else prefix


def is_negated(line: str, start: int) -> bool:
    prefix = clause_prefix(line, start)
    return bool(NEGATION_RE.search(prefix))


def sentence_for_line(line: str) -> str:
    return " ".join(line.strip().split())


def scan_text(path: Path, text: str) -> list[Finding]:
    findings: list[Finding] = []
    for line_no, line in enumerate(text.splitlines(), start=1):
        if ALLOW_RE.search(line):
            continue
        if BOUNDARY_CONTEXT_RE.search(line):
            continue
        sentence = sentence_for_line(line)
        if not sentence:
            continue
        for rule in RULES:
            for match in rule.pattern.finditer(line):
                if is_negated(line, match.start()):
                    continue
                findings.append(
                    Finding(path, line_no, rule.name, sentence, rule.suggestion)
                )
                break
    return findings


def scan_files(files: Iterable[Path], root: Path) -> list[Finding]:
    findings: list[Finding] = []
    for path in files:
        if not is_scoped_public_prose(path, root):
            continue
        text = read_text(path)
        if text is None:
            continue
        rel = path.relative_to(root)
        findings.extend(scan_text(rel, text))
    return findings


def print_findings(findings: Sequence[Finding]) -> None:
    counts: dict[str, int] = {}
    for finding in findings:
        counts[finding.rule] = counts.get(finding.rule, 0) + 1

    print("claims-boundary=failed")
    print(f"finding_count={len(findings)}")
    for rule in sorted(counts):
        print(f"rule_count {rule}={counts[rule]}")

    print("locations:")
    for finding in findings:
        print(f"- {finding.path}:{finding.line} rule={finding.rule}")
        print(f"  sentence: {finding.sentence}")
        print(f"  suggestion: {finding.suggestion}")


def assert_flags(rule: str, line: str) -> None:
    findings = scan_text(Path("TEST.md"), line)
    assert any(finding.rule == rule for finding in findings), (rule, line, findings)


def assert_clean(line: str) -> None:
    findings = scan_text(Path("TEST.md"), line)
    assert findings == [], (line, findings)


def self_test() -> None:
    assert_flags("gate-as-truth", "CI green proves this agent is secure.")
    assert_flags("gate-as-truth", "A passing scan guarantees production ready tools.")
    assert_flags("proof-of-x", "This evidence bundle is a proof of compliance.")
    assert_flags("proof-of-x", "The report provides guarantees of trust.")
    assert_flags("tool-guarantees", "Assay guarantees every tool call is safe.")
    assert_flags("tool-guarantees", "The gate ensures the agent remains compliant.")

    assert_clean("This is not a proof of compliance.")
    assert_clean("Records the decision, not proof of the effect.")
    assert_clean("CI green does not prove the service is secure.")
    assert_clean("The binding is not authenticity.")
    assert_clean("The receipt is asserted, not verified.")
    assert_clean("This is a compliance evidence record, not a compliance guarantee.")
    assert_clean("A gate checks the fixture and reports a bounded signal.")
    assert_clean("- implying that a passing evaluation means the system is safe")
    assert_clean(
        "A reviewed exception may say proof of compliance. "
        "<!-- claims-guard: allow: reviewed legal wording -->"
    )
    print("self-test=passed")


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--repo-root", type=Path, default=REPO_ROOT)
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    if args.self_test:
        self_test()
        return 0

    root = args.repo_root.resolve()
    findings = scan_files(git_files(root), root)
    if findings:
        print_findings(findings)
        return 1

    print("claims-boundary=passed")
    print("finding_count=0")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
