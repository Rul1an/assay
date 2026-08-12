#!/usr/bin/env python3
"""Interruption and residue safety for the ADR-045 seal producer mutation hook.

Proves SIGINT/SIGTERM restore the producer from a scratch copy (never git), that a
read-only residue check detects declared live mutants without rewriting, and that
each signal restoration path is load-bearing.
"""

from __future__ import annotations

import hashlib
import importlib.util
import os
import re
import shutil
import stat
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


def file_mode(path: Path = TARGET) -> int:
    return stat.S_IMODE(path.stat().st_mode)


def snapshot() -> tuple[str, str, str, int]:
    return sha256(TARGET.read_bytes()), index_digest(), porcelain(), file_mode()


def load_mutation_module(script: Path | None = None):
    path = script or MUTATION_SCRIPT
    spec = importlib.util.spec_from_file_location("aee_seal_producer_mutations", path)
    if spec is None or spec.loader is None:
        raise SystemExit(f"cannot load {path}")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


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
    os.chmod(TARGET, 0o644)
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
                "SIGTERM interruption changed digest/index/porcelain/mode: "
                f"before={before!r} after={after!r}"
            )
    finally:
        if TARGET.read_bytes() != original:
            TARGET.write_bytes(original)
        os.chmod(TARGET, before[3])
    print("ok   SIGTERM interruption restores bytes/index/porcelain/mode")


def test_sigint_interruption_restores() -> None:
    original = TARGET.read_bytes()
    os.chmod(TARGET, 0o644)
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
                "SIGINT interruption changed digest/index/porcelain/mode: "
                f"before={before!r} after={after!r}"
            )
    finally:
        if TARGET.read_bytes() != original:
            TARGET.write_bytes(original)
        os.chmod(TARGET, before[3])
    print("ok   SIGINT interruption restores bytes/index/porcelain/mode")


def test_atomic_write_preserves_exact_mode() -> None:
    """Regression: mkstemp defaults to 0600; producer must stay at its original mode."""
    fixture_dir = Path(tempfile.mkdtemp(prefix="aee-mode-"))
    fixture = fixture_dir / "aee_seal.rs"
    try:
        fixture.write_bytes(b"mode-preservation-probe\n")
        os.chmod(fixture, 0o644)
        before = file_mode(fixture)
        if before != 0o644:
            fail(f"fixture setup expected 0644, got {oct(before)}")
        mod = load_mutation_module()
        mod.write_bytes_atomic(fixture, fixture.read_bytes())
        after = file_mode(fixture)
        if after != 0o644:
            fail(
                f"write_bytes_atomic changed mode from {oct(before)} to {oct(after)} "
                "(mkstemp residue; git cannot see this)"
            )
        # Non-default mode must also round-trip.
        os.chmod(fixture, 0o640)
        mod.write_bytes_atomic(fixture, fixture.read_bytes())
        if file_mode(fixture) != 0o640:
            fail(f"write_bytes_atomic did not preserve 0640; got {oct(file_mode(fixture))}")
    finally:
        shutil.rmtree(fixture_dir, ignore_errors=True)
    print("ok   write_bytes_atomic preserves exact mode")


def test_removing_fchmod_reopens_mode_gap() -> None:
    text = MUTATION_SCRIPT.read_text()
    pattern = r"(?m)^\s*os\.fchmod\(fd, mode\)\n"
    if len(re.findall(pattern, text)) != 1:
        fail("mutation script has no unique os.fchmod(fd, mode) path to bite")
    mutated = re.sub(pattern, "", text, count=1)
    probe_dir = Path(tempfile.mkdtemp(prefix="aee-mode-mut-"))
    probe = probe_dir / "mutations.py"
    fixture = probe_dir / "aee_seal.rs"
    try:
        probe.write_text(mutated)
        fixture.write_bytes(b"mode-gap-mutation\n")
        os.chmod(fixture, 0o644)
        mod = load_mutation_module(probe)
        mod.write_bytes_atomic(fixture, fixture.read_bytes())
        after = file_mode(fixture)
        if after == 0o644:
            fail("stripping os.fchmod still preserved 0644; mode path is not load-bearing")
        if after != 0o600:
            fail(f"expected mkstemp 0600 after stripping fchmod, got {oct(after)}")
    finally:
        shutil.rmtree(probe_dir, ignore_errors=True)
    print("ok   removing fchmod re-opens 0644->0600 mode gap")


def test_invalid_interrupt_signal_restores() -> None:
    original = TARGET.read_bytes()
    os.chmod(TARGET, 0o644)
    before = snapshot()
    try:
        result = run_mutation_script(
            env_extra={
                "ASSAY_AEE_SEAL_MUTATION_INTERRUPT_AFTER": RUN_BINDING,
                "ASSAY_AEE_SEAL_MUTATION_INTERRUPT_SIGNAL": "NOT_A_SIGNAL",
            }
        )
        if result.returncode == 0:
            fail("invalid interrupt signal exited 0")
        if "unknown ASSAY_AEE_SEAL_MUTATION_INTERRUPT_SIGNAL" not in result.stderr:
            fail(f"expected SystemExit message in stderr; got {result.stderr!r}")
        after = snapshot()
        if MUTANT_MARKER.encode() in TARGET.read_bytes():
            fail("invalid interrupt signal left run-binding preimage mutant")
        if after != before:
            fail(
                "invalid interrupt signal changed digest/index/porcelain/mode: "
                f"before={before!r} after={after!r}"
            )
    finally:
        if TARGET.read_bytes() != original:
            TARGET.write_bytes(original)
        os.chmod(TARGET, before[3])
    print("ok   invalid interrupt signal restores bytes/index/porcelain/mode")


def test_removing_baseexception_restore_reopens_systemexit_gap() -> None:
    text = MUTATION_SCRIPT.read_text()
    anchor = "except BaseException:\n"
    if text.count(anchor) != 1:
        fail(f"expected exactly one BaseException restore handler, found {text.count(anchor)}")
    mutated = text.replace(anchor, "except Exception:\n", 1)
    probe_dir = Path(tempfile.mkdtemp(prefix="aee-sysexit-mut-"))
    probe = probe_dir / "mutations.py"
    original = TARGET.read_bytes()
    os.chmod(TARGET, 0o644)
    try:
        probe.write_text(mutated)
        before = snapshot()
        result = run_mutation_script(
            script=probe,
            env_extra={
                "ASSAY_AEE_SEAL_MUTATION_INTERRUPT_AFTER": RUN_BINDING,
                "ASSAY_AEE_SEAL_MUTATION_INTERRUPT_SIGNAL": "NOT_A_SIGNAL",
            },
        )
        after_bytes = TARGET.read_bytes()
        dirty = after_bytes != original or MUTANT_MARKER.encode() in after_bytes
        if after_bytes != original:
            TARGET.write_bytes(original)
        os.chmod(TARGET, before[3])
        if not dirty:
            fail(
                "narrowing BaseException->Exception still cleaned SystemExit path "
                f"(exit={result.returncode}); restore path is not load-bearing"
            )
        if snapshot()[0] != before[0]:
            fail("failed to restore producer after SystemExit-path mutation probe")
    finally:
        if TARGET.read_bytes() != original:
            TARGET.write_bytes(original)
        os.chmod(TARGET, 0o644)
        shutil.rmtree(probe_dir, ignore_errors=True)
    print("ok   narrowing BaseException->Exception re-opens SystemExit residue")


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
    if signame == "SIGINT":
        # BaseException restore also covers KeyboardInterrupt. Pair handler deletion
        # with Exception narrowing so the SIGINT gap is observable again.
        if mutated.count("except BaseException:\n") != 1:
            fail("SIGINT bite needs a unique BaseException restore to narrow")
        mutated = mutated.replace("except BaseException:\n", "except Exception:\n", 1)
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
    os.chmod(TARGET, 0o644)
    baseline = snapshot()
    baseline_bytes = TARGET.read_bytes()
    baseline_mode = baseline[3]
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
            os.chmod(TARGET, baseline_mode)

    # Interruption residue first: the bug this issue exists to close.
    step(test_sigterm_interruption_restores)
    step(test_sigint_interruption_restores)
    step(test_atomic_write_preserves_exact_mode)
    step(test_removing_fchmod_reopens_mode_gap)
    step(test_invalid_interrupt_signal_restores)
    step(test_removing_baseexception_restore_reopens_systemexit_gap)
    step(test_clean_tree_residue_check_passes)
    step(test_residue_check_detects_planted_mutant_read_only)
    step(test_removing_signal_path_fails, "SIGTERM")
    step(test_removing_signal_path_fails, "SIGINT")
    step(test_uncommitted_producer_bytes_survive_sigterm)
    step(test_precommit_trigger_excludes_config_yaml)
    step(test_no_git_restore_in_mutation_script)
    if (
        TARGET.read_bytes() != baseline_bytes
        or snapshot() != baseline
        or file_mode() != baseline_mode
    ):
        TARGET.write_bytes(baseline_bytes)
        os.chmod(TARGET, baseline_mode)
        fail(f"safety suite altered {REL_TARGET} bytes/index/porcelain/mode")
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
