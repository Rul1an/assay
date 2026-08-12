#!/usr/bin/env python3
"""Interruption and residue safety for the ADR-045 seal producer mutation hook.

Proves SIGINT/SIGTERM restore the producer from a scratch copy (never git), that a
read-only residue check detects declared live mutants without rewriting, and that
each signal restoration path is load-bearing.
"""

from __future__ import annotations

import hashlib
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(
    subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
)
TARGET = ROOT / "crates/assay-cli/src/aee_seal.rs"
MUTATION_SCRIPT = ROOT / "scripts/ci/test_aee_seal_producer_mutations.py"
REL_TARGET = "crates/assay-cli/src/aee_seal.rs"
RUN_BINDING = "run-binding preimage"
MUTANT_MARKER = '"subject": "x",'


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def index_digest() -> str:
    return subprocess.run(
        ["git", "rev-parse", f":{REL_TARGET}"],
        capture_output=True,
        text=True,
        check=True,
        cwd=ROOT,
    ).stdout.strip()


def porcelain() -> str:
    return subprocess.run(
        ["git", "status", "--porcelain", "--", REL_TARGET],
        capture_output=True,
        text=True,
        check=True,
        cwd=ROOT,
    ).stdout


def snapshot() -> tuple[str, str, str]:
    return sha256(TARGET.read_bytes()), index_digest(), porcelain()


def run_mutation_script(
    *,
    env_extra: dict[str, str] | None = None,
    script: Path | None = None,
    timeout: float = 30.0,
) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    if env_extra:
        env.update(env_extra)
    return subprocess.run(
        [sys.executable, str(script or MUTATION_SCRIPT)],
        cwd=ROOT,
        env=env,
        capture_output=True,
        text=True,
        timeout=timeout,
    )


def fail(msg: str) -> None:
    print(f"FAIL: {msg}", file=sys.stderr)
    raise SystemExit(1)


def test_sigterm_interruption_restores() -> None:
    original = TARGET.read_bytes()
    before = snapshot()
    try:
        result = run_mutation_script(
            env_extra={
                "ASSAY_AEE_SEAL_MUTATION_INTERRUPT_AFTER": RUN_BINDING,
                "ASSAY_AEE_SEAL_MUTATION_INTERRUPT_SIGNAL": "SIGTERM",
            }
        )
        body = TARGET.read_bytes()
        after_digest = sha256(body)
        if MUTANT_MARKER.encode() in body or after_digest != before[0]:
            fail(
                "SIGTERM interruption left producer residue "
                f"(exit={result.returncode}, digest_changed={after_digest != before[0]}, "
                f"mutant_present={MUTANT_MARKER.encode() in body})"
            )
        after = snapshot()
        if after != before:
            fail(
                "SIGTERM interruption changed digest/index/porcelain: "
                f"before={before!r} after={after!r}"
            )
    finally:
        if TARGET.read_bytes() != original:
            TARGET.write_bytes(original)
    print("ok   SIGTERM interruption restores bytes/index/porcelain")


def test_sigint_interruption_restores() -> None:
    original = TARGET.read_bytes()
    before = snapshot()
    try:
        result = run_mutation_script(
            env_extra={
                "ASSAY_AEE_SEAL_MUTATION_INTERRUPT_AFTER": RUN_BINDING,
                "ASSAY_AEE_SEAL_MUTATION_INTERRUPT_SIGNAL": "SIGINT",
            }
        )
        body = TARGET.read_bytes()
        after_digest = sha256(body)
        if MUTANT_MARKER.encode() in body or after_digest != before[0]:
            fail(
                "SIGINT interruption left producer residue "
                f"(exit={result.returncode}, digest_changed={after_digest != before[0]}, "
                f"mutant_present={MUTANT_MARKER.encode() in body})"
            )
        after = snapshot()
        if after != before:
            fail(
                "SIGINT interruption changed digest/index/porcelain: "
                f"before={before!r} after={after!r}"
            )
    finally:
        if TARGET.read_bytes() != original:
            TARGET.write_bytes(original)
    print("ok   SIGINT interruption restores bytes/index/porcelain")


def test_residue_check_detects_planted_mutant_read_only() -> None:
    original = TARGET.read_bytes()
    planted = original.replace(
        b'"subject": env.subject_digest,',
        b'"subject": "x",',
        1,
    )
    if planted == original:
        fail("could not plant run-binding preimage mutant for residue check")

    fixture = Path(tempfile.mkdtemp(prefix="aee-residue-")) / "aee_seal.rs"
    try:
        fixture.write_bytes(planted)
        before = fixture.read_bytes()
        probe = subprocess.run(
            [
                sys.executable,
                str(MUTATION_SCRIPT),
                "--check-residue",
                "--target",
                str(fixture),
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        after = fixture.read_bytes()
        if after != before:
            fail("residue check rewrote the fixture")
        if probe.returncode == 0:
            fail("residue check accepted a live run-binding preimage mutant")
        if REL_TARGET not in probe.stderr and str(fixture) not in probe.stderr:
            # Accept either the real relative path message or the fixture path.
            if "run-binding preimage" not in probe.stderr:
                fail(
                    "residue check must name the mutation; stderr was: "
                    + probe.stderr.strip()
                )
        if "run-binding preimage" not in probe.stderr:
            fail(
                "residue check must name run-binding preimage; stderr was: "
                + probe.stderr.strip()
            )
    finally:
        TARGET.write_bytes(original)
        shutil.rmtree(fixture.parent, ignore_errors=True)
    print("ok   residue check detects planted mutant without rewriting")


def test_clean_tree_residue_check_passes() -> None:
    before = snapshot()
    probe = subprocess.run(
        [sys.executable, str(MUTATION_SCRIPT), "--check-residue"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    after = snapshot()
    if probe.returncode != 0:
        fail(f"clean residue check failed: {probe.stderr.strip()}")
    if after != before:
        fail("clean residue check mutated producer state")
    print("ok   clean residue check is read-only and quiet")


def _strip_signal_handler(script_text: str, signame: str) -> str:
    """Remove one signal restoration registration; must be unique."""
    if signame == "SIGTERM":
        pattern = r"(?m)^\s*signal\.signal\(signal\.SIGTERM, .*\)\n"
    elif signame == "SIGINT":
        pattern = r"(?m)^\s*signal\.signal\(signal\.SIGINT, .*\)\n"
    else:
        raise SystemExit(f"unknown signame {signame}")
    matches = re.findall(pattern, script_text)
    if len(matches) != 1:
        raise SystemExit(
            f"expected exactly one {signame} handler registration, found {len(matches)}"
        )
    return re.sub(pattern, "", script_text, count=1)


def test_removing_signal_path_fails(signame: str) -> None:
    original = TARGET.read_bytes()
    text = MUTATION_SCRIPT.read_text()
    mutated = _strip_signal_handler(text, signame)
    probe_dir = Path(tempfile.mkdtemp(prefix="aee-signal-mut-"))
    probe = probe_dir / "mutations.py"
    try:
        probe.write_text(mutated)
        before = snapshot()
        result = run_mutation_script(
            script=probe,
            env_extra={
                "ASSAY_AEE_SEAL_MUTATION_INTERRUPT_AFTER": RUN_BINDING,
                "ASSAY_AEE_SEAL_MUTATION_INTERRUPT_SIGNAL": signame,
            },
        )
        after_bytes = TARGET.read_bytes()
        dirty = after_bytes != original or MUTANT_MARKER.encode() in after_bytes
        # Restore from scratch bytes if the mutated script leaked (expected).
        if after_bytes != original:
            TARGET.write_bytes(original)
        if not dirty:
            fail(
                f"removing {signame} restoration path still cleaned interruption "
                f"(exit={result.returncode}); path is not load-bearing"
            )
        if snapshot()[0] != before[0]:
            fail("failed to restore producer after signal-path mutation probe")
    finally:
        if TARGET.read_bytes() != original:
            TARGET.write_bytes(original)
        shutil.rmtree(probe_dir, ignore_errors=True)
    print(f"ok   removing {signame} restoration path re-opens residue")


def test_uncommitted_producer_bytes_survive_sigterm() -> None:
    original = TARGET.read_bytes()
    marker = b"\n// uncommitted-producer-byte-probe-2318\n"
    dirty = original + marker
    TARGET.write_bytes(dirty)
    try:
        result = run_mutation_script(
            env_extra={
                "ASSAY_AEE_SEAL_MUTATION_INTERRUPT_AFTER": RUN_BINDING,
                "ASSAY_AEE_SEAL_MUTATION_INTERRUPT_SIGNAL": "SIGTERM",
            }
        )
        if TARGET.read_bytes() != dirty:
            fail(
                "SIGTERM restore wiped uncommitted producer bytes "
                f"(exit={result.returncode})"
            )
    finally:
        TARGET.write_bytes(original)
    print("ok   uncommitted producer bytes survive SIGTERM restore")


def test_precommit_trigger_excludes_config_yaml() -> None:
    config = (ROOT / ".pre-commit-config.yaml").read_text()
    block = re.search(
        r"- id: aee-seal-producer-mutations\n(?:.*\n)*?        files: (.+)\n",
        config,
    )
    if not block:
        fail("aee-seal-producer-mutations files filter missing")
    files = block.group(1)
    if r"\.pre-commit-config\.yaml" in files:
        fail(
            "aee-seal-producer-mutations still triggers on .pre-commit-config.yaml: "
            + files.strip()
        )
    if "aee_seal\\.rs" not in files or "test_aee_seal_producer_mutations\\.py" not in files:
        fail("producer mutation trigger lost its content paths: " + files.strip())
    print("ok   pre-push trigger no longer includes .pre-commit-config.yaml")


def test_no_git_restore_in_mutation_script() -> None:
    text = MUTATION_SCRIPT.read_text()
    # Docstrings may name the historical hazard; forbid live recovery invocations.
    live = [
        pat
        for pat in (
            r'\[\s*["\']git["\']\s*,\s*["\']restore["\']',
            r'\[\s*["\']git["\']\s*,\s*["\']checkout["\']',
            r'os\.system\([^)]*git\s+(?:restore|checkout)',
        )
        if re.search(pat, text)
    ]
    if live:
        fail(f"mutation script must not recover via git invocations matching {live!r}")
    print("ok   mutation script avoids git restore/checkout recovery")


def main() -> int:
    # Dirty trees are allowed: probes must restore the exact preimage bytes, including
    # uncommitted producer work. Only refuse to conclude if we cannot restore.
    baseline = snapshot()
    baseline_bytes = TARGET.read_bytes()
    start = time.perf_counter()
    failures = 0

    def step(fn, *args) -> None:
        nonlocal failures
        try:
            fn(*args)
        except SystemExit as exc:
            if exc.code not in (0, None):
                failures += 1
            else:
                raise
        except Exception as exc:  # noqa: BLE001 - surface probe crashes as failures
            print(f"FAIL: {fn.__name__} raised {exc!r}", file=sys.stderr)
            failures += 1
            if TARGET.read_bytes() != baseline_bytes:
                TARGET.write_bytes(baseline_bytes)

    # Interruption residue first: the bug this issue exists to close.
    step(test_sigterm_interruption_restores)
    step(test_sigint_interruption_restores)
    step(test_clean_tree_residue_check_passes)
    step(test_residue_check_detects_planted_mutant_read_only)
    step(test_removing_signal_path_fails, "SIGTERM")
    step(test_removing_signal_path_fails, "SIGINT")
    step(test_uncommitted_producer_bytes_survive_sigterm)
    step(test_precommit_trigger_excludes_config_yaml)
    step(test_no_git_restore_in_mutation_script)
    if TARGET.read_bytes() != baseline_bytes or snapshot() != baseline:
        # Last resort restore so a failed probe never strands the producer.
        TARGET.write_bytes(baseline_bytes)
        fail(f"safety suite altered {REL_TARGET} bytes/index/porcelain")
    elapsed = time.perf_counter() - start
    if failures:
        print(
            f"\n{failures} aee producer mutation safety check(s) failed ({elapsed:.2f}s)",
            file=sys.stderr,
        )
        return 1
    print(f"\nall aee producer mutation safety checks passed ({elapsed:.2f}s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
