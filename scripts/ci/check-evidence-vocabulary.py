#!/usr/bin/env python3
"""Scoped evidence-vocabulary guard for false run_root-as-Merkle claims.

Every scanned merkle occurrence must match a path-bound allowlist pattern.
Affirmative run_root-as-Merkle phrases fail even when the file is allowlisted.
A vacuous allowlist entry (path exists, pattern matches 0 times) is a hard
failure. Whole-directory exemptions are not supported.

Historical ADR/RFC/experiment prose is not a genuine-use allowlist. Those
lines stay on an exact-path corrected-history list and require an adjacent
dated correction, or a dated sidecar for frozen generated results.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Iterable, Mapping, Sequence

REPO_ROOT = Path(__file__).resolve().parents[2]

# The guard necessarily spells Merkle while defining the rule. Listing those
# self-hits in ALLOWED_MERKLE_USES is allowlist sprawl. Exclude only these two
# implementation paths — never scripts/ci/ as a directory.
SCAN_PATH_EXCLUDES = (
    "scripts/ci/check-evidence-vocabulary.py",
    "scripts/ci/test-evidence-vocabulary.sh",
)

# Same names as scripts/ci/lib/clear-git-repository-env.sh. A second list that
# drifted would re-admit hostile GIT_DIR selection.
HOSTILE_GIT_ENV_NAMES = (
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
)

TEXTUAL_SCAN_SUFFIXES = (
    ".md",
    ".rs",
    ".py",
    ".ts",
    ".js",
    ".yml",
    ".yaml",
    ".toml",
    ".sh",
    ".txt",
    ".json",
)

# Complete-line patterns (fullmatch on the stripped line). Hand-written
# import/call lines use exact `_E` text so a comment cannot ride the token.
# Do not restore substring or prose-capable wildcard permits.
_E = re.escape
ALLOWED_MERKLE_USES: dict[str, tuple[str, ...]] = {
    "docs/architecture/SPEC-Outward-Product-Truth-v1.md": (
        _E("2. evidence vocabulary and the false Merkle claim tracked by issue #2222;"),
        _E("It is not a Merkle root and it does not provide a Merkle inclusion proof."),
        _E("tests, demos, and fixtures that teach the public contract. Genuine Merkle references remain valid"),
        _E("when they describe a real Merkle construction, including Rekor, RFC 6962 experiments, and the"),
        _E("A scoped recurrence guard must reject new false `run_root`-as-Merkle claims while allowing named,"),
        _E("reviewed genuine uses. The guard must not ban the word `Merkle` repository-wide."),
        _E("- issue #2222's false current `run_root` Merkle claims are removed;"),
        _E("- genuine Merkle constructions remain documented;"),
    ),
    # Generated kernel identifiers. Exact lines only — not prose containing merkle_tree_.
    "crates/assay-ebpf/src/vmlinux.rs": (
        _E("pub read_merkle_tree_page: ::core::option::Option<"),
        _E("pub write_merkle_tree_block: ::core::option::Option<"),
        _E("pub struct merkle_tree_params {"),
        _E("pub tree_params: merkle_tree_params,"),
    ),
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
        _E('!boundary.contains("Merkle"),'),
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

# Exact original historical lines. Not ALLOWED_MERKLE_USES. Each match must
# have an adjacent dated correction, or a sidecar for frozen results.
CORRECTED_HISTORY: dict[str, tuple[str, ...]] = {
    "docs/architecture/ADR-007-Deterministic-Provenance.md": (
        _E("This creates a lightweight **Hash Chain** (Merkle sequence) that proves the integrity and order of the event stream."),
    ),
    "docs/architecture/ADR-034-Evidence-Redaction-At-Capture.md": (
        _E("- Determinism is non-negotiable here: assay evidence is replayable (VCR) and Merkle-hashed. Redaction"),
        _E("VCR replay and Merkle hashing stay stable."),
        _E("- Belt-and-suspenders: a final ASSERTION sweep over the assembled ndjson before the Merkle root and"),
    ),
    "docs/architecture/ADR-039-evidence-bundle-attestation.md": (
        _E("The evidence bundle has a manifest, Merkle root, and content-addressed events, but is"),
    ),
    "docs/architecture/RFC-001-dx-ux-governance.md": (
        _E("4. **Evidence integrity chain** separating metadata from payload integrity - `assay-evidence` manifest, SHA-256, Merkle root"),
    ),
    "docs/experiments/evidence-mutation-cost-2026-06/README.md": (
        _E("(`run_root`, a Merkle root over event content hashes) is bound by an external signature the"),
        _E("size and gzip ratio, bytes per event, the Merkle inclusion-proof size (ceil(log2(N)) hashes), and"),
    ),
    "docs/experiments/runner-vs-otel-2026-05/workload/src/manifest-binding.ts": (
        _E("* (see `crates/assay-evidence` Merkle infrastructure)."),
    ),
    "docs/experiments/evidence-mutation-cost-2026-06/results/cost.json": (
        _E('"inclusion_proof_hashes": 10'),
        _E('"inclusion_proof_hashes": 13'),
        _E('"inclusion_proof_hashes": 14'),
        _E('"inclusion_proof_hashes": 16'),
        _E('"inclusion_proof_hashes": 17'),
    ),
    "docs/experiments/evidence-mutation-cost-2026-06/results/cost.md": (
        _E("| events | verify ms (median) | reps | compressed bytes | gzip ratio | bytes/event | inclusion-proof hashes |"),
    ),
}

CORRECTED_HISTORY_SIDECARS: dict[str, str] = {
    "docs/experiments/evidence-mutation-cost-2026-06/results/cost.json": (
        "docs/experiments/evidence-mutation-cost-2026-06/results/CORRECTION-2026-08-14.md"
    ),
    "docs/experiments/evidence-mutation-cost-2026-06/results/cost.md": (
        "docs/experiments/evidence-mutation-cost-2026-06/results/CORRECTION-2026-08-14.md"
    ),
}

# Frozen 2026-06 measurement bytes from origin/main. Not regenerated.
CORRECTED_HISTORY_DIGESTS: dict[str, str] = {
    "docs/experiments/evidence-mutation-cost-2026-06/results/cost.json": (
        "4d3cb30137710324a844cd148f09ace0159679c849622bbcd8597bbd80b7d58e"
    ),
    "docs/experiments/evidence-mutation-cost-2026-06/results/cost.md": (
        "f14b07bd862624b68b86992773da17aea7cce04ff4a4016450f849bbfd863c01"
    ),
}

DATED_CORRECTION_BODY = (
    "Correction (2026-08-14): the shipped `run_root` is SHA-256 over newline-delimited",
    "event content-hash strings, with a trailing newline, in event sequence order —",
    "not a tree root, and not `event_id` bytes. References below to the historical",
    "tree proposal describe the model used at the time and are not claims about the",
    "shipped evidence format.",
)

FALSE_CLAIM_RE = re.compile(
    r"run_root\s+is\s+a\s+merkle\s+root",  # affirmative run_root-as-Merkle
    re.IGNORECASE,
)
MERKLE_RE = re.compile(r"merkle", re.IGNORECASE)
# Membership/inclusion of one event, even when the word Merkle is absent.
RUN_ROOT_MEMBERSHIP_RE = re.compile(
    r"run_root\b.{0,160}\b(?:included|inclusion|membership)\b",
    re.IGNORECASE,
)
RUN_ROOT_MEMBERSHIP_NEGATION_RE = re.compile(
    r"\b(?:does not|cannot|is not|not a|not an)\b.{0,40}\b(?:included|inclusion|membership)\b",
    re.IGNORECASE,
)

# Withdrawn experiment metric names. They do not contain "merkle" but still
# teach a production inclusion proof that run_root does not have. The Rust
# harness plus every docs/experiments/ path — not one dated directory.
# Genuine Rekor/ADR inclusion-proof text is out of scope.
WITHDRAWN_METRIC_LABELS: tuple[str, ...] = (
    "inclusion_proof_hashes",
    "inclusion-proof hashes",
)
WITHDRAWN_LABEL_RES: tuple[re.Pattern[str], ...] = tuple(
    re.compile(re.escape(label), re.IGNORECASE) for label in WITHDRAWN_METRIC_LABELS
)
WITHDRAWN_HARNESS = "crates/assay-evidence/tests/e3_verify_cost_curve.rs"
WITHDRAWN_EXPERIMENT_PREFIX = "docs/experiments/"


def scrub_hostile_git_env() -> None:
    for name in HOSTILE_GIT_ENV_NAMES:
        os.environ.pop(name, None)


def git_files(root: Path) -> list[Path]:
    scrub_hostile_git_env()
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


def sha256_hex(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def rel_posix(path: Path, root: Path) -> str:
    return path.relative_to(root).as_posix()


def is_textual_scan_surface(rel: str) -> bool:
    lower = rel.lower()
    return any(lower.endswith(suffix) for suffix in TEXTUAL_SCAN_SUFFIXES)


def is_excluded(rel: str) -> bool:
    return rel in SCAN_PATH_EXCLUDES


def is_withdrawn_surface(rel: str) -> bool:
    if rel == WITHDRAWN_HARNESS:
        return True
    prefix = WITHDRAWN_EXPERIMENT_PREFIX
    return rel == prefix.rstrip("/") or rel.startswith(prefix)


def compiled_pattern(pat: str) -> re.Pattern[str]:
    return re.compile(pat, re.IGNORECASE)


def compiled_patterns(patterns: Sequence[str]) -> list[re.Pattern[str]]:
    return [compiled_pattern(pat) for pat in patterns]


def line_matches(line: str, pat: re.Pattern[str]) -> bool:
    return pat.fullmatch(line.strip()) is not None


def line_is_allowed(line: str, patterns: Sequence[re.Pattern[str]]) -> bool:
    return any(line_matches(line, pat) for pat in patterns)


def is_run_root_membership_claim(line: str) -> bool:
    if not RUN_ROOT_MEMBERSHIP_RE.search(line):
        return False
    return RUN_ROOT_MEMBERSHIP_NEGATION_RE.search(line) is None


def normalize_correction_line(line: str) -> str:
    text = line.strip()
    if text.startswith(">"):
        text = text[1:].strip()
    if text.startswith("*"):
        text = text[1:].strip()
    return text


def is_dated_correction_block(lines: Sequence[str], start: int) -> bool:
    if start < 0 or start + len(DATED_CORRECTION_BODY) > len(lines):
        return False
    got = tuple(
        normalize_correction_line(lines[start + offset])
        for offset in range(len(DATED_CORRECTION_BODY))
    )
    return got == DATED_CORRECTION_BODY


def has_adjacent_dated_correction(lines: Sequence[str], idx: int) -> bool:
    n = len(DATED_CORRECTION_BODY)
    for gap in (0, 1):
        before = idx - n - gap
        if before >= 0 and is_dated_correction_block(lines, before):
            if gap == 0 or not lines[idx - 1].strip():
                return True
        after = idx + 1 + gap
        if is_dated_correction_block(lines, after):
            if gap == 0 or not lines[idx + 1].strip():
                return True
    return False


def sidecar_has_exact_correction(root: Path, sidecar_rel: str) -> bool:
    text = read_text(root / sidecar_rel)
    if text is None:
        return False
    lines = text.splitlines()
    return any(is_dated_correction_block(lines, idx) for idx in range(len(lines)))


def line_is_corrected_history(rel: str, line: str) -> bool:
    return line_is_allowed(line, compiled_patterns(CORRECTED_HISTORY.get(rel, ())))


def historical_line_admitted(
    root: Path, rel: str, line: str, lines: Sequence[str], idx: int
) -> bool:
    if not line_is_corrected_history(rel, line):
        return False
    sidecar = CORRECTED_HISTORY_SIDECARS.get(rel)
    if sidecar:
        return sidecar_has_exact_correction(root, sidecar)
    return has_adjacent_dated_correction(lines, idx)


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


def corrected_history_staleness(
    root: Path, history: Mapping[str, Sequence[str]] | None = None
) -> list[str]:
    entries = CORRECTED_HISTORY if history is None else history
    if not any((root / rel).is_file() for rel in entries):
        return []
    production = (root / "docs/architecture/SPEC-Outward-Product-Truth-v1.md").is_file()
    messages: list[str] = []
    for rel, patterns in entries.items():
        path = root / rel
        if not path.is_file():
            if production:
                messages.append(f"stale corrected-history entry: missing path {rel}")
            continue
        text = read_text(path)
        if text is None:
            messages.append(f"stale corrected-history entry: {rel} is unreadable or binary")
            continue
        lines = text.splitlines()
        compiled = compiled_patterns(patterns)
        sidecar = CORRECTED_HISTORY_SIDECARS.get(rel)
        for raw, cre in zip(patterns, compiled, strict=True):
            matches = [idx for idx, line in enumerate(lines) if line_matches(line, cre)]
            if not matches:
                messages.append(
                    f"vacuous corrected-history entry: {rel} / {raw!r} matched 0 times"
                )
                continue
            if sidecar:
                if not sidecar_has_exact_correction(root, sidecar):
                    messages.append(
                        f"corrected-history sidecar missing exact dated correction: {sidecar}"
                    )
            else:
                for idx in matches:
                    if not has_adjacent_dated_correction(lines, idx):
                        messages.append(
                            f"corrected-history line without adjacent dated correction: {rel}"
                        )
        expected = CORRECTED_HISTORY_DIGESTS.get(rel)
        if expected and sha256_hex(path) != expected:
            messages.append(f"corrected-history digest mismatch: {rel}")
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
            if is_textual_scan_surface(rel):
                findings.append(f"{rel}: unreadable or NUL textual surface")
            continue
        allowed = compiled.get(rel, ())
        legacy = compiled_ids.get(rel, ())
        lines = text.splitlines()
        for line_no, line in enumerate(lines, start=1):
            if is_run_root_membership_claim(line):
                findings.append(
                    f"{rel}:{line_no}: false run_root inclusion/membership claim: {line.strip()}"
                )
                continue
            if not MERKLE_RE.search(line):
                continue
            if FALSE_CLAIM_RE.search(line):
                findings.append(
                    f"{rel}:{line_no}: false run_root-as-Merkle claim: {line.strip()}"
                )
                continue
            if line_is_allowed(line, allowed) or line_is_allowed(line, legacy):
                continue
            if historical_line_admitted(root, rel, line, lines, line_no - 1):
                continue
            if line_is_corrected_history(rel, line):
                findings.append(
                    f"{rel}:{line_no}: historical line without adjacent dated correction: {line.strip()}"
                )
                continue
            findings.append(
                f"{rel}:{line_no}: unapproved Merkle claim: {line.strip()}"
            )
    return findings


def scan_withdrawn_labels(root: Path, files: Iterable[Path]) -> list[str]:
    findings: list[str] = []
    for path in files:
        rel = rel_posix(path, root)
        if is_excluded(rel) or not is_withdrawn_surface(rel):
            continue
        text = read_text(path)
        if text is None:
            if is_textual_scan_surface(rel):
                findings.append(f"{rel}: unreadable or NUL textual surface")
            continue
        lines = text.splitlines()
        for line_no, line in enumerate(lines, start=1):
            for label, cre in zip(WITHDRAWN_METRIC_LABELS, WITHDRAWN_LABEL_RES, strict=True):
                if not cre.search(line):
                    continue
                if historical_line_admitted(root, rel, line, lines, line_no - 1):
                    continue
                findings.append(
                    f"{rel}:{line_no}: withdrawn metric label {label!r}: {line.strip()}"
                )
    return findings


def check_tree(
    root: Path,
    allowlist: Mapping[str, Sequence[str]] | None = None,
    identifiers: Mapping[str, Sequence[str]] | None = None,
) -> int:
    rules = ALLOWED_MERKLE_USES if allowlist is None else allowlist
    idents = LEGACY_IDENTIFIERS if identifiers is None else identifiers
    stale = (
        allowlist_staleness(root, rules)
        + allowlist_staleness(root, idents)
        + corrected_history_staleness(root)
    )
    tracked = git_files(root)
    if not tracked:
        print("evidence-vocabulary=failed")
        print("tracked set is empty; refuse to pass")
        return 1
    findings = scan_findings(root, tracked, rules, idents)
    withdrawn = scan_withdrawn_labels(root, tracked)
    if stale or findings or withdrawn:
        print("evidence-vocabulary=failed")
        for message in stale + findings + withdrawn:
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
