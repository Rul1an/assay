#!/usr/bin/env python3
"""Scoped evidence-vocabulary guard for false run_root-as-Merkle claims.

Every scanned merkle occurrence must match a path-bound allowlist pattern.
Affirmative run_root-as-Merkle phrases fail even when the file is allowlisted.
A vacuous allowlist entry (path exists, pattern matches 0 times) is a hard
failure. Whole-directory exemptions are not supported.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path
from typing import Iterable, Mapping, Sequence

REPO_ROOT = Path(__file__).resolve().parents[2]

SCAN_PREFIX_EXCLUDES = ("docs/superpowers/plans/",)

# Measured on origin/main at b34bc2f8ef5d97d2ec3d4988852cba90ff9b396f.
# Each pattern has ≥1 current hit. Do not restore the vacuous draft pairs.
ALLOWED_MERKLE_USES: dict[str, tuple[str, ...]] = {
    "docs/architecture/SPEC-Outward-Product-Truth-v1.md": (
        r"not a Merkle root",
        r"Merkle inclusion proof",
        r"Genuine Merkle references",
        r"real Merkle construction",
        r"run_root`-as-Merkle",
        r"word `Merkle`",
        r"false Merkle claim",
        r"run_root` Merkle claims",
        r"genuine Merkle constructions",
    ),
    "crates/assay-ebpf/src/vmlinux.rs": (r"merkle_tree_",),
    "scripts/experiments/aee_spike_lib.py": (
        r"RFC6962-style Merkle",
        r"def merkle_root",
        r"SHA-256 Merkle root",
    ),
    "scripts/experiments/aee_spike_check.py": (r"merkle_root\(",),
    "scripts/experiments/aee_spike_emit.py": (r"merkle_root\(",),
    "crates/assay-registry/src/rekor.rs": (r"Merkle inclusion", r"rfc6962_root"),
    "crates/assay-registry/src/rekor/checkpoint.rs": (r"RFC 6962",),
    "docs/architecture/ADR-012-Transparency-Log.md": (r"Merkle tree", r"Merkle proof"),
    "crates/assay-cli/tests/spec_reason_code_registry.rs": (
        r"names a Merkle structure",
        r'"Merkle root',
        r'"Merkle"',
    ),
    "docs/architecture/ADR-009-WORM-Storage.md": (
        r"Native Merkle tree verification",
        r"Custom Merkle Chain on PostgreSQL",
    ),
    # Remaining debt: Claude-owned follow-up for merged #2352. Exact current phrase.
    "crates/assay-cli/src/cli/commands/evidence/verify_side_effects.rs": (
        r"manifest hashes \+ Merkle root",
    ),
    # Checker / mutation-test self-text. Patterns match this source, not product claims.
    "scripts/ci/check-evidence-vocabulary.py": (
        r"as-Merkle",
        r"scanned merkle occurrence",
        r"ALLOWED_MERKLE_USES",
        r"not a Merkle root",
        r"Merkle inclusion",
        r"Genuine Merkle references",
        r"real Merkle construction",
        r"word `Merkle`",
        r"false Merkle claim",
        r"Merkle claims",
        r"genuine Merkle constructions",
        r"merkle_tree_",
        r"RFC6962-style Merkle",
        r"merkle_root",
        r"Merkle root",
        r"Merkle tree",
        r"Merkle proof",
        r"Merkle structure",
        r'"Merkle"',
        r"Merkle Chain",
        r"unapproved Merkle",
        r"MERKLE_RE",
        r"a merkle root",
        r"false-run-root-merkle",
        r"genuine-rekor-merkle",
        r"printf 'Merkle",
    ),
    "scripts/ci/test-evidence-vocabulary.sh": (
        r"ALLOWED_MERKLE_USES",
        r"_FALSE_SUFFIX",
        r"false-run-root-merkle",
        r"genuine-rekor-merkle",
        r"Merkle root",
        r"Merkle inclusion",
        r"printf 'Merkle",
    ),
}

FALSE_CLAIM_RE = re.compile(
    r"run_root\s+is\s+a\s+merkle\s+root"  # affirmative run_root-as-Merkle
    r"|run_root[`\s,)]*,\s*a\s+merkle\s+root",  # comma-form run_root-as-Merkle
    re.IGNORECASE,
)
MERKLE_RE = re.compile(r"merkle", re.IGNORECASE)


def git_files(root: Path) -> list[Path]:
    proc = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=root,
        check=True,
        stdout=subprocess.PIPE,
    )
    return [root / item.decode("utf-8") for item in proc.stdout.split(b"\0") if item]


def read_text(path: Path) -> str | None:
    try:
        data = path.read_bytes()
    except OSError:
        return None
    if b"\0" in data:
        return None
    return data.decode("utf-8", errors="replace")


def rel_posix(path: Path, root: Path) -> str:
    return path.relative_to(root).as_posix()


def is_excluded(rel: str) -> bool:
    return any(rel == prefix.rstrip("/") or rel.startswith(prefix) for prefix in SCAN_PREFIX_EXCLUDES)


def compiled_patterns(patterns: Sequence[str]) -> list[re.Pattern[str]]:
    return [re.compile(pat, re.IGNORECASE) for pat in patterns]


def line_is_allowed(line: str, patterns: Sequence[re.Pattern[str]]) -> bool:
    return any(pat.search(line) for pat in patterns)


def allowlist_staleness(
    root: Path, allowlist: Mapping[str, Sequence[str]]
) -> list[str]:
    messages: list[str] = []
    for rel, patterns in allowlist.items():
        path = root / rel
        if not path.is_file():
            messages.append(f"stale allowlist entry: missing path {rel}")
            continue
        text = read_text(path)
        if text is None:
            messages.append(f"stale allowlist entry: {rel} is unreadable or binary")
            continue
        for pat in patterns:
            if re.search(pat, text, re.IGNORECASE) is None:
                messages.append(
                    f"vacuous allowlist entry: {rel} / {pat!r} matched 0 times"
                )
    return messages


def scan_findings(
    root: Path,
    files: Iterable[Path],
    allowlist: Mapping[str, Sequence[str]],
) -> list[str]:
    compiled = {rel: compiled_patterns(pats) for rel, pats in allowlist.items()}
    findings: list[str] = []
    for path in files:
        rel = rel_posix(path, root)
        if is_excluded(rel):
            continue
        text = read_text(path)
        if text is None:
            continue
        allowed = compiled.get(rel, ())
        for line_no, line in enumerate(text.splitlines(), start=1):
            if not MERKLE_RE.search(line):
                continue
            if FALSE_CLAIM_RE.search(line):
                findings.append(
                    f"{rel}:{line_no}: false run_root-as-Merkle claim: {line.strip()}"
                )
                continue
            if line_is_allowed(line, allowed):
                continue
            findings.append(
                f"{rel}:{line_no}: unapproved Merkle claim: {line.strip()}"
            )
    return findings


def check_tree(
    root: Path, allowlist: Mapping[str, Sequence[str]] | None = None
) -> int:
    rules = ALLOWED_MERKLE_USES if allowlist is None else allowlist
    stale = allowlist_staleness(root, rules)
    findings = scan_findings(root, git_files(root), rules)
    if stale or findings:
        print("evidence-vocabulary=failed")
        for message in stale + findings:
            print(message)
        return 1
    print("evidence-vocabulary=passed")
    return 0


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=REPO_ROOT)
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    return check_tree(args.repo_root.resolve(), ALLOWED_MERKLE_USES)


if __name__ == "__main__":
    raise SystemExit(main())
