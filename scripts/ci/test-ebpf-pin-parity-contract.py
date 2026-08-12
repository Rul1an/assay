#!/usr/bin/env python3
"""CI-4E / #2225: eBPF pin parity fallback (no shared manifest, no upstream claim).

Canonical defaults: `:-…` values in scripts/ci/install-ebpf-toolchain.sh.
One table-driven parser; rejects missing / ambiguous / divergent pins.
"""

from __future__ import annotations

import re
import shutil
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PRECOMMIT = ROOT / ".pre-commit-config.yaml"
TC = r"nightly-\d{4}-\d{2}-\d{2}"
LV = r"\d+\.\d+\.\d+"

# (id, repo-relative path, extractor name)
CONSUMERS = [
    ("install", "scripts/ci/install-ebpf-toolchain.sh", "install_script"),
    ("xtask", "crates/assay-xtask/src/main.rs", "xtask_rs"),
    ("setup", "infra/bpf-runner/setup.sh", "provisioning"),
    ("cloud-init", "infra/bpf-runner/cloud-init.yaml", "provisioning"),
    ("dockerfile", "docker/Dockerfile.ebpf-builder", "provisioning"),
]


class ContractError(Exception):
    pass


def fail(msg: str) -> None:
    print(f"FAIL: {msg}", file=sys.stderr)
    raise SystemExit(1)


def ok(msg: str) -> None:
    print(f"ok   {msg}")


def _unique(values: set[str], kind: str, consumer: str) -> str:
    if not values:
        raise ContractError(f"{consumer}: missing {kind} pin")
    if len(values) > 1:
        raise ContractError(
            f"{consumer}: ambiguous {kind} pins: {', '.join(sorted(values))}"
        )
    return next(iter(values))


def extract_install_script(text: str, consumer: str) -> tuple[str, str]:
    return (
        _unique(set(re.findall(rf"ASSAY_EBPF_RUST_TOOLCHAIN:-({TC})", text)), "toolchain", consumer),
        _unique(set(re.findall(rf"ASSAY_BPF_LINKER_VERSION:-({LV})", text)), "bpf-linker", consumer),
    )


def extract_xtask_rs(text: str, consumer: str) -> tuple[str, str]:
    return (
        _unique(
            set(re.findall(rf'const DEFAULT_EBPF_RUST_TOOLCHAIN:\s*&str\s*=\s*"({TC})"', text)),
            "toolchain",
            consumer,
        ),
        _unique(
            set(re.findall(rf'const DEFAULT_BPF_LINKER_VERSION:\s*&str\s*=\s*"({LV})"', text)),
            "bpf-linker",
            consumer,
        ),
    )


def extract_provisioning(text: str, consumer: str) -> tuple[str, str]:
    toolchains = set(re.findall(rf"toolchain install ({TC})", text))
    toolchains.update(re.findall(rf"--toolchain ({TC})", text))
    toolchains.update(re.findall(rf"rustup run ({TC})", text))
    toolchains.update(re.findall(rf"cargo \+({TC})", text))
    linkers = set(re.findall(rf"bpf-linker ({LV})", text))
    linkers.update(re.findall(rf"cargo install bpf-linker --version ({LV})", text))
    return (
        _unique(toolchains, "toolchain", consumer),
        _unique(linkers, "bpf-linker", consumer),
    )


EXTRACTORS = {
    "install_script": extract_install_script,
    "xtask_rs": extract_xtask_rs,
    "provisioning": extract_provisioning,
}


def check_parity(paths: dict[str, Path]) -> tuple[str, str]:
    """One rule: every consumer's semantic pins equal install-script defaults."""
    wanted = {c[0] for c in CONSUMERS}
    if set(paths) != wanted:
        raise ContractError(f"consumer set mismatch: got {sorted(paths)}, want {sorted(wanted)}")
    install_path = paths["install"]
    if not install_path.is_file():
        raise ContractError(f"install: missing file {install_path}")
    canonical = EXTRACTORS["install_script"](
        install_path.read_text(encoding="utf-8"), "install"
    )
    for cid, _rel, kind in CONSUMERS:
        path = paths[cid]
        if not path.is_file():
            raise ContractError(f"{cid}: missing file {path}")
        pins = EXTRACTORS[kind](path.read_text(encoding="utf-8"), cid)
        if pins[0] != canonical[0]:
            raise ContractError(
                f"{cid}: divergent toolchain {pins[0]!r} != canonical {canonical[0]!r}"
            )
        if pins[1] != canonical[1]:
            raise ContractError(
                f"{cid}: divergent bpf-linker {pins[1]!r} != canonical {canonical[1]!r}"
            )
    return canonical


def default_paths(root: Path) -> dict[str, Path]:
    return {cid: root / rel for cid, rel, _ in CONSUMERS}


def expect_red(paths: dict[str, Path], consumer: str) -> str:
    try:
        check_parity(paths)
    except ContractError as exc:
        msg = str(exc)
        if not any(k in msg for k in ("divergent", "ambiguous", "missing")):
            fail(f"{consumer}: red error lacked pin failure class: {msg!r}")
        return msg
    fail(f"{consumer}: pin mutation left the contract green")


def mutate_first(text: str, old: str, new: str, consumer: str) -> str:
    if old not in text:
        fail(f"{consumer}: mutation search {old!r} not found")
    out = text.replace(old, new, 1)
    if out == text:
        fail(f"{consumer}: mutation did not change text")
    return out


def _assert_files_regex_owns(rx: re.Pattern[str], label: str) -> None:
    """Compile-time ownership: exact positives + one nearby non-owner negative."""
    must_own = [rel for _cid, rel, _kind in CONSUMERS]
    must_own.append("scripts/ci/test-ebpf-pin-parity-contract.py")
    must_own.append(".pre-commit-config.yaml")
    for path in must_own:
        if rx.fullmatch(path) is None:
            raise ContractError(f"{label}: must own {path!r}")
    # Nearby non-owner: over-broad `.*` would match this and stay green otherwise.
    non_owner = "scripts/ci/test-cargo-plugin-versions-contract.sh"
    if rx.fullmatch(non_owner) is not None:
        raise ContractError(f"{label}: must not own nearby non-owner {non_owner!r}")


def assert_precommit_ownership() -> None:
    pc = PRECOMMIT.read_text(encoding="utf-8")
    if "id: ebpf-pin-parity-contract-self-test" not in pc:
        fail("pre-commit missing ebpf-pin-parity-contract-self-test hook")
    block_m = re.search(
        r"(?m)^      - id:\s*ebpf-pin-parity-contract-self-test\s*\n(?:.*\n){0,12}?"
        r"^        files:\s*(.+)\s*$",
        pc,
    )
    if not block_m:
        fail("ebpf-pin-parity-contract-self-test files: line missing")
    files_pat = block_m.group(1).strip()
    try:
        rx = re.compile(files_pat)
    except re.error as exc:
        fail(f"files: regex does not compile: {exc}: {files_pat}")
    try:
        _assert_files_regex_owns(rx, "files")
    except ContractError as exc:
        fail(str(exc))
    # Controls: root nested under scripts/ci/(...) fails; overbroad .* fails non-owner.
    broken = (
        r"^(scripts/ci/(install-ebpf-toolchain\.sh|test-ebpf-pin-parity-contract\.py|"
        r"\.pre-commit-config\.yaml)|crates/assay-xtask/src/main\.rs|"
        r"infra/bpf-runner/(setup\.sh|cloud-init\.yaml)|docker/Dockerfile\.ebpf-builder)$"
    )
    for label, pat, needle in (
        ("broken-grouping", broken, ".pre-commit-config.yaml"),
        ("overbroad", r".*", "must not own"),
    ):
        try:
            _assert_files_regex_owns(re.compile(pat), label)
            fail(f"{label} files: control left ownership green")
        except ContractError as exc:
            if needle not in str(exc):
                fail(f"{label} control wrong error: {exc}")
    ok("pre-commit files: owns owners; broken grouping and .* go red")

def main() -> None:
    for label, text, needle in (
        ("missing toolchain", "no pins here\n", "missing toolchain"),
        (
            "missing bpf-linker",
            "rustup toolchain install nightly-2026-01-01 --profile minimal\n",
            "missing bpf-linker",
        ),
        (
            "ambiguous toolchain",
            "rustup toolchain install nightly-2026-01-01 --profile minimal\n"
            "rustup toolchain install nightly-2099-01-01 --profile minimal\n"
            "cargo install bpf-linker --version 0.10.3 --locked\n",
            "ambiguous toolchain",
        ),
        (
            "ambiguous bpf-linker",
            "rustup toolchain install nightly-2026-01-01 --profile minimal\n"
            "cargo install bpf-linker --version 0.10.3 --locked\n"
            "cargo install bpf-linker --version 9.9.9 --locked\n",
            "ambiguous bpf-linker",
        ),
    ):
        try:
            extract_provisioning(text, "synth")
            fail(f"{label} synth stayed green")
        except ContractError as exc:
            if needle not in str(exc):
                fail(f"{label} synth wrong error: {exc}")
        ok(f"synthetic {label} is rejected")

    try:
        toolchain, bpf_linker = check_parity(default_paths(ROOT))
    except ContractError as exc:
        fail(f"current consumers are not aligned: {exc}")
    ok(f"five consumers agree on toolchain={toolchain} bpf-linker={bpf_linker}")

    scratch = Path(tempfile.mkdtemp(prefix="ebpf-pin-parity-"))
    try:
        fields = (
            ("toolchain", toolchain, "nightly-2099-12-31"),
            ("bpf-linker", bpf_linker, "9.9.9"),
        )
        for cid, rel, _kind in CONSUMERS:
            src = (ROOT / rel).read_text(encoding="utf-8")
            for field, old, new in fields:
                mutant_path = scratch / f"{cid}-{field}-{Path(rel).name}"
                mutant_path.write_text(
                    mutate_first(src, old, new, cid), encoding="utf-8"
                )
                mutant_paths = default_paths(ROOT)
                mutant_paths[cid] = mutant_path
                msg = expect_red(mutant_paths, f"{cid}/{field}")
                cls = next(k for k in ("divergent", "ambiguous", "missing") if k in msg)
                ok(f"mutating {cid} {field} pin turns red ({cls})")
    finally:
        shutil.rmtree(scratch, ignore_errors=True)

    assert_precommit_ownership()
    print("PASS: eBPF pin parity contract")


if __name__ == "__main__":
    main()
