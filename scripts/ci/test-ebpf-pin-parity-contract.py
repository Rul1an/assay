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
    files_pat = block_m.group(1)
    for stem in (
        "install-ebpf-toolchain",
        "test-ebpf-pin-parity-contract",
        "assay-xtask/src/main",
        "setup\\.sh",
        "cloud-init\\.yaml",
        "Dockerfile\\.ebpf-builder",
        "pre-commit-config",
    ):
        if stem not in files_pat:
            fail(f"files: must watch {stem!r}; got {files_pat}")
    ok("pre-commit hook watches the five consumers and this test")


def main() -> None:
    try:
        extract_provisioning("no pins here\n", "synth")
        fail("missing-pin synth stayed green")
    except ContractError as exc:
        if "missing toolchain" not in str(exc):
            fail(f"missing synth wrong error: {exc}")
    ok("synthetic missing toolchain is rejected")

    ambiguous = (
        "rustup toolchain install nightly-2026-01-01 --profile minimal\n"
        "rustup toolchain install nightly-2099-01-01 --profile minimal\n"
        "cargo install bpf-linker --version 0.10.3 --locked\n"
    )
    try:
        extract_provisioning(ambiguous, "synth")
        fail("ambiguous-pin synth stayed green")
    except ContractError as exc:
        if "ambiguous toolchain" not in str(exc):
            fail(f"ambiguous synth wrong error: {exc}")
    ok("synthetic ambiguous toolchain is rejected")

    try:
        toolchain, bpf_linker = check_parity(default_paths(ROOT))
    except ContractError as exc:
        fail(f"current consumers are not aligned: {exc}")
    ok(f"five consumers agree on toolchain={toolchain} bpf-linker={bpf_linker}")

    scratch = Path(tempfile.mkdtemp(prefix="ebpf-pin-parity-"))
    try:
        for cid, rel, _kind in CONSUMERS:
            mutant_path = scratch / Path(rel).name
            mutant_path.write_text(
                mutate_first((ROOT / rel).read_text(encoding="utf-8"), toolchain, "nightly-2099-12-31", cid),
                encoding="utf-8",
            )
            mutant_paths = default_paths(ROOT)
            mutant_paths[cid] = mutant_path
            msg = expect_red(mutant_paths, cid)
            cls = next(k for k in ("divergent", "ambiguous", "missing") if k in msg)
            ok(f"mutating {cid} toolchain pin turns red ({cls})")
    finally:
        shutil.rmtree(scratch, ignore_errors=True)

    assert_precommit_ownership()
    print("PASS: eBPF pin parity contract")


if __name__ == "__main__":
    main()
