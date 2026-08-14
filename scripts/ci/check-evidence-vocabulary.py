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

# Non-normative implementation records. Not outward product claims.
SCAN_PREFIX_EXCLUDES = ("docs/superpowers/plans/",)

# The guard necessarily spells Merkle while defining the rule. Listing those
# self-hits in ALLOWED_MERKLE_USES is allowlist sprawl. Exclude only these two
# implementation paths — never scripts/ci/ as a directory.
SCAN_PATH_EXCLUDES = (
    "scripts/ci/check-evidence-vocabulary.py",
    "scripts/ci/test-evidence-vocabulary.sh",
)

# Complete-line patterns (fullmatch on the stripped line). The only `.*...*`
# permit is generated `merkle_tree_*` identifiers in vmlinux.rs. Hand-written
# import/call lines use exact `_E` text so a comment cannot ride the token.
# Do not restore substring or prose-capable wildcard permits.
_E = re.escape
ALLOWED_MERKLE_USES: dict[str, tuple[str, ...]] = {
    "docs/architecture/SPEC-Outward-Product-Truth-v1.md": (
        _E("2. evidence vocabulary and the false Merkle claim tracked by issue #2222;"),
        _E("of entry hashes. It is not a Merkle root and it does not provide a Merkle inclusion proof."),
        _E("tests, demos, and fixtures that teach the public contract. Genuine Merkle references remain valid"),
        _E("when they describe a real Merkle construction, including Rekor, RFC 6962 experiments, and the"),
        _E("A scoped recurrence guard must reject new false `run_root`-as-Merkle claims while allowing named,"),
        _E("reviewed genuine uses. The guard must not ban the word `Merkle` repository-wide."),
        _E("- issue #2222's false current `run_root` Merkle claims are removed;"),
        _E("- genuine Merkle constructions remain documented;"),
    ),
    # Generated kernel identifiers (four `merkle_tree_*` shapes). Not prose.
    "crates/assay-ebpf/src/vmlinux.rs": (r".*merkle_tree_.*",),
    "scripts/experiments/aee_spike_lib.py": (
        _E("fixture signature, run-binding, and RFC6962-style Merkle rules in one place so"),
        _E("def merkle_root(leaves: list[dict[str, Any]]) -> str:"),
        _E('"""RFC6962-style SHA-256 Merkle root over canonical observation records."""'),
    ),
    "scripts/experiments/aee_spike_check.py": (
        _E("merkle_root,"),
        _E('if records and predicate.get("batchRoot") != merkle_root(records):'),
    ),
    "scripts/experiments/aee_spike_emit.py": (
        _E("merkle_root,"),
        _E('"batchRoot": merkle_root(records),'),
        _E('predicate["batchRoot"] = merkle_root(predicate["observationRecords"])'),
    ),
    "crates/assay-registry/src/rekor.rs": (
        _E("// (5) Merkle inclusion: leaf = SHA256(0x00 || canonicalizedBody); recompute the root."),
        _E("use checkpoint::{b64, parse_checkpoint, rfc6962_root, sha256};"),
        _E("let Some(recomputed) = rfc6962_root(leaf_hash, ip_index, checkpoint.tree_size, &proof_hashes)"),
    ),
    "crates/assay-registry/src/rekor/checkpoint.rs": (
        _E("/// RFC 6962 section 2.1.1 inclusion-proof verification. Recomputes the tree root from the leaf hash, the"),
    ),
    "docs/architecture/ADR-012-Transparency-Log.md": (
        _E("│  │ 3. Verify inclusion proof (Merkle tree)                 │   │"),
        _E("// 2. Walk Merkle proof to root"),
        _E("- [Merkle Tree Proofs](https://transparency.dev/verifiable-data-structures/)"),
    ),
    "crates/assay-cli/tests/spec_reason_code_registry.rs": (
        _E('// leaving that line untouched. That gap re-admitted a withdrawn "Merkle root ... inclusion'),
        _E('// `compute_run_root` is a flat sha256 over the concatenated content hashes, so "Merkle" would'),
        _E('!boundary.contains("Merkle"),'),
        _E('"{BOUNDARY} names a Merkle structure; `run_root` is a hash chain"'),
    ),
    "docs/architecture/ADR-009-WORM-Storage.md": (
        _E("- Native Merkle tree verification"),
        _E("### 3. Custom Merkle Chain on PostgreSQL"),
    ),
}

# Exact filename-reference lines. These are path identifiers, not evidence
# claims, and not genuine Merkle constructions. Not a whole-file exemption.
LEGACY_IDENTIFIERS: dict[str, tuple[str, ...]] = {
    "demo/produce_video.sh": (
        _E("vhs demo/scenes/merkle-chain.tape"),
        _E('cp demo/scenes/merkle-chain.mp4 "$TEMP_DIR/shot05.mp4"'),
    ),
    "demo/scenes/merkle-chain.tape": (_E("Output demo/scenes/merkle-chain.mp4"),),
}

FALSE_CLAIM_RE = re.compile(
    r"run_root\s+is\s+a\s+merkle\s+root",  # affirmative run_root-as-Merkle
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
    if rel in SCAN_PATH_EXCLUDES:
        return True
    return any(rel == prefix.rstrip("/") or rel.startswith(prefix) for prefix in SCAN_PREFIX_EXCLUDES)


def compiled_pattern(pat: str) -> re.Pattern[str]:
    return re.compile(pat, re.IGNORECASE)


def compiled_patterns(patterns: Sequence[str]) -> list[re.Pattern[str]]:
    return [compiled_pattern(pat) for pat in patterns]


def line_matches(line: str, pat: re.Pattern[str]) -> bool:
    return pat.fullmatch(line.strip()) is not None


def line_is_allowed(line: str, patterns: Sequence[re.Pattern[str]]) -> bool:
    return any(line_matches(line, pat) for pat in patterns)


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
        compiled = compiled_patterns(patterns)
        for raw, cre in zip(patterns, compiled, strict=True):
            if not any(line_matches(line, cre) for line in text.splitlines()):
                messages.append(
                    f"vacuous allowlist entry: {rel} / {raw!r} matched 0 times"
                )
    return messages


def scan_findings(
    root: Path,
    files: Iterable[Path],
    allowlist: Mapping[str, Sequence[str]],
    identifiers: Mapping[str, Sequence[str]],
) -> list[str]:
    compiled = {rel: compiled_patterns(pats) for rel, pats in allowlist.items()}
    compiled_ids = {rel: compiled_patterns(pats) for rel, pats in identifiers.items()}
    findings: list[str] = []
    for path in files:
        rel = rel_posix(path, root)
        if is_excluded(rel):
            continue
        text = read_text(path)
        if text is None:
            continue
        allowed = compiled.get(rel, ())
        legacy = compiled_ids.get(rel, ())
        for line_no, line in enumerate(text.splitlines(), start=1):
            if not MERKLE_RE.search(line):
                continue
            if FALSE_CLAIM_RE.search(line):
                findings.append(
                    f"{rel}:{line_no}: false run_root-as-Merkle claim: {line.strip()}"
                )
                continue
            if line_is_allowed(line, allowed) or line_is_allowed(line, legacy):
                continue
            findings.append(
                f"{rel}:{line_no}: unapproved Merkle claim: {line.strip()}"
            )
    return findings


def check_tree(
    root: Path,
    allowlist: Mapping[str, Sequence[str]] | None = None,
    identifiers: Mapping[str, Sequence[str]] | None = None,
) -> int:
    rules = ALLOWED_MERKLE_USES if allowlist is None else allowlist
    idents = LEGACY_IDENTIFIERS if identifiers is None else identifiers
    stale = allowlist_staleness(root, rules) + allowlist_staleness(root, idents)
    findings = scan_findings(root, git_files(root), rules, idents)
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
