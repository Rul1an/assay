#!/usr/bin/env python3
"""RFC8785 adequacy harness: verify vendored serde_jcs, patched build, conformance test."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
from pathlib import Path

ARCHIVE_SHA256 = "d3a60f3fda61525e439ef6d67422118f11e986566997d9021c56867ad814a0aa"
VCS_SHA1 = "da9568831881506731d0da73e4afa46e1f8b0fdf"
RYU_JS_CHECKSUM = "6518fc26bced4d53678a22d6e423e9d8716377def84545fe328236e3af070e7f"
REGISTRY_SOURCE = "registry+https://github.com/rust-lang/crates.io-index"
PATCH_PATH = "third_party/serde_jcs-0.2.0"
MUTABLE_VENDORED_REL = "src/lib.rs"
PATCH_CONFIG = f'patch.crates-io.serde_jcs.path="{PATCH_PATH}"'
WIRING_ANCHOR = "self.tag.cmp(&other.tag)"
WIRING_REPLACEMENT = "self.key.cmp(&other.key)"
EXPECTED_WIRING_VECTORS = frozenset({"keyorder_utf16_vs_codepoint"})
VECTOR_FAIL_RE = re.compile(r"^\[([a-z0-9_]+)\]\s*$")
EXPECTED_FILES = frozenset(
    {
        ".cargo_vcs_info.json",
        ".gitignore",
        "Cargo.lock",
        "Cargo.toml",
        "Cargo.toml.orig",
        "LICENSE-APACHE",
        "LICENSE-MIT",
        "README.md",
        "src/lib.rs",
        "tests/basic.rs",
        "tests/fixtures/input/appendix_c.json",
        "tests/fixtures/input/appendix_e.json",
        "tests/fixtures/output/appendix_c.json",
        "tests/fixtures/output/appendix_e.json",
        "tests/testdata/README.md",
        "tests/testdata/input/arrays.json",
        "tests/testdata/input/french.json",
        "tests/testdata/input/structures.json",
        "tests/testdata/input/unicode.json",
        "tests/testdata/input/values.json",
        "tests/testdata/input/weird.json",
        "tests/testdata/output/arrays.json",
        "tests/testdata/output/french.json",
        "tests/testdata/output/structures.json",
        "tests/testdata/output/unicode.json",
        "tests/testdata/output/values.json",
        "tests/testdata/output/weird.json",
    }
)
PROVENANCE_PINNED: dict[str, object] = {
    "crate": "serde_jcs",
    "version": "0.2.0",
    "archive_path": "third_party/serde_jcs-0.2.0.crate",
    "archive_sha256": ARCHIVE_SHA256,
    "extract_dir": PATCH_PATH,
    "file_count": 27,
    "vcs_sha1": VCS_SHA1,
    "ryu_js_checksum": RYU_JS_CHECKSUM,
    "proof_composition": (
        "Compile-unproved classification composes pinned corpus-adequacy@13048989 "
        "build-gate semantics (mutant does not build => unproved) with this harness "
        "self-test bite (compile-break lib.rs reaches cargo, not verify_vendor)."
    ),
}
COMMAND_TIMEOUT_S = 1800


class HarnessError(Exception):
    pass


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def _sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def _safe_tar_members(tar: tarfile.TarFile) -> None:
    for member in tar.getmembers():
        name = member.name
        if name.startswith("/") or name.startswith("\\"):
            raise HarnessError(f"unsafe tar path: {name!r}")
        if ".." in Path(name).parts:
            raise HarnessError(f"unsafe tar path: {name!r}")


def _archive_file_bytes(archive: Path) -> dict[str, bytes]:
    out: dict[str, bytes] = {}
    with tarfile.open(archive, "r:gz") as tar:
        _safe_tar_members(tar)
        with tempfile.TemporaryDirectory() as tmp:
            tar.extractall(tmp, filter="data")
            top = Path(tmp)
            dirs = [p for p in top.iterdir() if p.is_dir()]
            if len(dirs) != 1:
                raise HarnessError("archive must contain one top-level directory")
            root = dirs[0]
            members = {p.relative_to(root).as_posix() for p in root.rglob("*") if p.is_file()}
            if members != EXPECTED_FILES:
                missing = sorted(EXPECTED_FILES - members)
                extra = sorted(members - EXPECTED_FILES)
                raise HarnessError(f"archive member set mismatch missing={missing} extra={extra}")
            for rel in EXPECTED_FILES:
                out[rel] = (root / rel).read_bytes()
    return out


def _extracted_file_set(extract: Path) -> frozenset[str]:
    if not extract.is_dir():
        return frozenset()
    return frozenset(p.relative_to(extract).as_posix() for p in extract.rglob("*") if p.is_file())


def _read_provenance(prov: Path) -> dict:
    if not prov.is_file():
        raise HarnessError(f"missing provenance: {prov}")
    try:
        doc = json.loads(prov.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise HarnessError(f"provenance is not valid JSON: {prov}") from exc
    if not isinstance(doc, dict):
        raise HarnessError("provenance must be a JSON object")
    return doc


def _assert_provenance(doc: dict) -> None:
    for key, want in PROVENANCE_PINNED.items():
        if doc.get(key) != want:
            raise HarnessError(f"provenance field {key!r} drift: got {doc.get(key)!r}, want {want!r}")
    if not isinstance(doc.get("non_claim"), str) or not doc["non_claim"].strip():
        raise HarnessError("provenance non_claim must be a non-empty string")


def _assert_immutable_vendor_bytes(extract: Path, archive_bytes: dict[str, bytes], *, allow_mutated_lib: bool) -> None:
    on_disk = _extracted_file_set(extract)
    if on_disk != EXPECTED_FILES:
        missing = sorted(EXPECTED_FILES - on_disk)
        extra = sorted(on_disk - EXPECTED_FILES)
        raise HarnessError(f"extracted file set mismatch missing={missing} extra={extra}")
    for rel in sorted(EXPECTED_FILES):
        path = extract / rel
        if allow_mutated_lib and rel == MUTABLE_VENDORED_REL:
            if not path.is_file():
                raise HarnessError(f"extracted file missing: {rel}")
            continue
        if path.read_bytes() != archive_bytes[rel]:
            raise HarnessError(f"extracted byte mismatch: {rel}")


def verify_vendor(root: Path) -> None:
    archive = root / "third_party/serde_jcs-0.2.0.crate"
    extract = root / PATCH_PATH
    prov = root / "third_party/serde_jcs-0.2.0.provenance.json"
    if not archive.is_file():
        raise HarnessError(f"missing archive: {archive}")
    if _sha256(archive) != ARCHIVE_SHA256:
        raise HarnessError("archive sha256 mismatch")
    _assert_provenance(_read_provenance(prov))
    archive_bytes = _archive_file_bytes(archive)
    _assert_immutable_vendor_bytes(extract, archive_bytes, allow_mutated_lib=False)
    vcs = json.loads((extract / ".cargo_vcs_info.json").read_text(encoding="utf-8"))
    if vcs.get("git", {}).get("sha1") != VCS_SHA1:
        raise HarnessError("VCS sha1 mismatch")


def _assert_build_inputs(root: Path) -> None:
    """Pin the committed vendor baseline without rejecting corpus mutants on lib.rs."""
    archive = root / "third_party/serde_jcs-0.2.0.crate"
    extract = root / PATCH_PATH
    prov = root / "third_party/serde_jcs-0.2.0.provenance.json"
    if not archive.is_file():
        raise HarnessError(f"missing archive: {archive}")
    if _sha256(archive) != ARCHIVE_SHA256:
        raise HarnessError("archive sha256 mismatch")
    _assert_provenance(_read_provenance(prov))
    archive_bytes = _archive_file_bytes(archive)
    _assert_immutable_vendor_bytes(extract, archive_bytes, allow_mutated_lib=True)
    if not (extract / "Cargo.lock").is_file():
        raise HarnessError("tracked Cargo.lock missing from extraction")


def _cargo_env(root: Path) -> dict[str, str]:
    env = os.environ.copy()
    if "adequacy-jcs-" in root.parent.name:
        env["CARGO_TARGET_DIR"] = str(root.parent / "cargo-target")
    elif not env.get("CARGO_TARGET_DIR"):
        env["CARGO_TARGET_DIR"] = str(root.parent / "assay-2593-target")
    return env


def _run(
    cmd: list[str],
    root: Path,
    *,
    check: bool = True,
    timeout: int = COMMAND_TIMEOUT_S,
) -> subprocess.CompletedProcess[str]:
    try:
        proc = subprocess.run(
            cmd,
            cwd=root,
            env=_cargo_env(root),
            text=True,
            capture_output=True,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired as exc:
        raise HarnessError(
            f"command timed out after {timeout}s: {' '.join(cmd)}"
        ) from exc
    if check and proc.returncode != 0:
        tail = (proc.stderr or proc.stdout or "")[-2000:]
        raise HarnessError(f"command failed ({proc.returncode}): {' '.join(cmd)}\n{tail}")
    return proc


def _validate_metadata_patch_doc(root: Path, doc: dict) -> None:
    want_manifest = (root / PATCH_PATH / "Cargo.toml").resolve()
    matches = [
        pkg
        for pkg in doc.get("packages", [])
        if pkg.get("name") == "serde_jcs" and pkg.get("version") == "0.2.0"
    ]
    if len(matches) != 1:
        raise HarnessError(
            f"expected exactly one serde_jcs 0.2.0 package in metadata, got {len(matches)}"
        )
    pkg = matches[0]
    if pkg.get("source") is not None:
        raise HarnessError(f"serde_jcs source must be null under patch, got {pkg.get('source')!r}")
    got_manifest = Path(pkg["manifest_path"]).resolve()
    if got_manifest != want_manifest:
        raise HarnessError(
            f"serde_jcs manifest_path must be {want_manifest}, got {got_manifest}"
        )


def _assert_metadata_patch(root: Path) -> None:
    proc = _run(
        ["cargo", "metadata", "--format-version", "1", "--config", PATCH_CONFIG],
        root,
    )
    _validate_metadata_patch_doc(root, json.loads(proc.stdout))


def _parse_lock_packages(lock_text: str) -> list[dict]:
    data = tomllib.loads(lock_text)
    packages = data.get("package")
    if not isinstance(packages, list):
        raise HarnessError("Cargo.lock has no [[package]] table")
    return packages


def _packages_named(packages: list[dict], name: str) -> list[dict]:
    return [pkg for pkg in packages if pkg.get("name") == name]


def _assert_lock_ryu_js(lock_text: str) -> None:
    matches = _packages_named(_parse_lock_packages(lock_text), "ryu-js")
    if len(matches) != 1:
        raise HarnessError("ryu-js must appear exactly once in Cargo.lock")
    checksum = matches[0].get("checksum")
    if checksum != RYU_JS_CHECKSUM:
        raise HarnessError("ryu-js checksum mismatch")


def _assert_lock_semantics(pre: str, post: str) -> None:
    pre_packages = _parse_lock_packages(pre)
    post_packages = _parse_lock_packages(post)
    pre_sj = _packages_named(pre_packages, "serde_jcs")
    post_sj = _packages_named(post_packages, "serde_jcs")
    if len(pre_sj) != 1 or len(post_sj) != 1:
        raise HarnessError("Cargo.lock must contain exactly one serde_jcs package record")
    pre_entry = pre_sj[0]
    post_entry = post_sj[0]
    if pre_entry.get("source") != REGISTRY_SOURCE:
        raise HarnessError("pre-lock serde_jcs must keep registry source")
    if pre_entry.get("checksum") != ARCHIVE_SHA256:
        raise HarnessError("pre-lock serde_jcs checksum must match the pinned crate")
    if "source" in post_entry or "checksum" in post_entry:
        raise HarnessError("post-lock serde_jcs must drop source and checksum")
    for key in ("name", "version", "dependencies"):
        if pre_entry.get(key) != post_entry.get(key):
            raise HarnessError(f"serde_jcs {key} changed during metadata materialization")
    pre_rest = sorted(
        (pkg for pkg in pre_packages if pkg.get("name") != "serde_jcs"),
        key=lambda pkg: (pkg.get("name", ""), pkg.get("version", "")),
    )
    post_rest = sorted(
        (pkg for pkg in post_packages if pkg.get("name") != "serde_jcs"),
        key=lambda pkg: (pkg.get("name", ""), pkg.get("version", "")),
    )
    if pre_rest != post_rest:
        raise HarnessError("Cargo.lock changed outside serde_jcs package entry")


def _require_materialized_repo(root: Path) -> None:
    """build/test rewrite Cargo.lock during the first patched metadata pass.

    That pass deliberately runs without ``--locked`` so serde_jcs source/checksum
    can drop out of the lock in the corpus tool's isolated copy only. Writer
    worktrees must keep the tracked root lock byte-unchanged.
    """
    if (root / ".git").exists():
        raise HarnessError(
            "build/test materialize Cargo.lock via patched metadata; "
            "run only in the corpus tool isolated repo copy, not a git worktree"
        )


def _assert_post_lock_serde_jcs(lock_text: str) -> None:
    entries = _packages_named(_parse_lock_packages(lock_text), "serde_jcs")
    if len(entries) != 1:
        raise HarnessError("Cargo.lock must contain exactly one serde_jcs package record")
    entry = entries[0]
    if "source" in entry or "checksum" in entry:
        raise HarnessError("materialized lock must not retain serde_jcs source/checksum")


def _materialize_patch_lock(root: Path) -> None:
    lock_path = root / "Cargo.lock"
    pre = lock_path.read_text(encoding="utf-8")
    _assert_lock_ryu_js(pre)
    pre_sj = _packages_named(_parse_lock_packages(pre), "serde_jcs")
    if len(pre_sj) != 1:
        raise HarnessError("Cargo.lock must contain exactly one serde_jcs package record")
    if "source" in pre_sj[0] and "checksum" in pre_sj[0]:
        _assert_metadata_patch(root)
        post = lock_path.read_text(encoding="utf-8")
        _assert_lock_semantics(pre, post)
        return
    _assert_post_lock_serde_jcs(pre)
    _assert_metadata_patch(root)


def build(root: Path) -> None:
    _require_materialized_repo(root)
    _assert_build_inputs(root)
    _materialize_patch_lock(root)
    _run(
        [
            "cargo",
            "build",
            "--locked",
            "-p",
            "assay-canonical",
            "--tests",
            "--config",
            PATCH_CONFIG,
        ],
        root,
    )


def test(root: Path) -> int:
    _require_materialized_repo(root)
    _assert_build_inputs(root)
    proc = _run(
        [
            "cargo",
            "test",
            "--locked",
            "-p",
            "assay-canonical",
            "--test",
            "rfc8785_conformance",
            "--config",
            PATCH_CONFIG,
            "--",
            "--nocapture",
        ],
        root,
        check=False,
    )
    if proc.stdout:
        sys.stdout.write(proc.stdout)
    if proc.stderr:
        sys.stderr.write(proc.stderr)
    return proc.returncode


def _copy_root(src: Path) -> Path:
    parent = tempfile.mkdtemp(prefix="adequacy-jcs-")
    dst = Path(parent) / "tree"
    shutil.copytree(src, dst, ignore=shutil.ignore_patterns("target", ".git"))
    return dst


def _drop_copy(tree: Path) -> None:
    shutil.rmtree(tree.parent, ignore_errors=True)


def _vendor_mismatch(exc: HarnessError) -> bool:
    msg = str(exc)
    return "byte mismatch" in msg or "extracted file set mismatch" in msg


def _mutate_compile_break(root: Path) -> None:
    path = root / PATCH_PATH / "src/lib.rs"
    text = path.read_text(encoding="utf-8")
    path.write_text(text.replace("pub fn to_vec", "pub fn to_vec BREAK"), encoding="utf-8")


def _mutate_wiring(root: Path) -> None:
    path = root / PATCH_PATH / "src/lib.rs"
    text = path.read_text(encoding="utf-8")
    count = text.count(WIRING_ANCHOR)
    if count != 1:
        raise HarnessError(f"wiring anchor must occur exactly once, got {count}")
    path.write_text(text.replace(WIRING_ANCHOR, WIRING_REPLACEMENT, 1), encoding="utf-8")


def _run_conformance(root: Path, *, patch: bool) -> subprocess.CompletedProcess[str]:
    cmd = [
        "cargo",
        "test",
        "--locked",
        "-p",
        "assay-canonical",
        "--test",
        "rfc8785_conformance",
    ]
    if patch:
        cmd.extend(["--config", PATCH_CONFIG])
    cmd.extend(["--", "--nocapture"])
    return _run(cmd, root, check=False)


def _parse_failed_vectors(proc: subprocess.CompletedProcess[str]) -> frozenset[str]:
    names: set[str] = set()
    in_failures = False
    combined = (proc.stdout or "").splitlines() + (proc.stderr or "").splitlines()
    for line in combined:
        if "vectors diverged" in line:
            in_failures = True
            continue
        if not in_failures:
            continue
        matched = VECTOR_FAIL_RE.match(line.strip())
        if matched:
            names.add(matched.group(1))
            continue
        if line.strip().startswith("A divergence"):
            break
    return frozenset(names)


def _metadata_validator_self_checks(root: Path) -> None:
    want_manifest = (root / PATCH_PATH / "Cargo.toml").resolve()
    sibling_manifest = want_manifest.parent.parent / "serde_jcs-0.2.0-evil" / "Cargo.toml"
    sibling_doc = {
        "packages": [
            {
                "name": "serde_jcs",
                "version": "0.2.0",
                "source": None,
                "manifest_path": str(sibling_manifest),
            }
        ]
    }
    try:
        _validate_metadata_patch_doc(root, sibling_doc)
    except HarnessError as exc:
        if "manifest_path must be" not in str(exc):
            raise
    else:
        raise HarnessError("sibling manifest_path must fail metadata validation")

    duplicate_doc = {
        "packages": [
            {
                "name": "serde_jcs",
                "version": "0.2.0",
                "source": None,
                "manifest_path": str(want_manifest),
            },
            {
                "name": "serde_jcs",
                "version": "0.2.0",
                "source": None,
                "manifest_path": str(want_manifest),
            },
        ]
    }
    try:
        _validate_metadata_patch_doc(root, duplicate_doc)
    except HarnessError as exc:
        if "exactly one serde_jcs 0.2.0" not in str(exc):
            raise
    else:
        raise HarnessError("duplicate serde_jcs packages must fail metadata validation")


def _timeout_wiring_self_check(root: Path) -> None:
    from unittest import mock

    captured: dict = {}

    def fake_run(*_args, **kwargs):
        captured.update(kwargs)
        return subprocess.CompletedProcess(["cargo", "metadata"], 0, "", "")

    with mock.patch("subprocess.run", side_effect=fake_run):
        _run(["cargo", "metadata"], root, check=False)
    if captured.get("timeout") != COMMAND_TIMEOUT_S:
        raise HarnessError(
            f"_run must pass timeout={COMMAND_TIMEOUT_S}, got {captured.get('timeout')!r}"
        )


def _timeout_self_check(root: Path) -> None:
    from unittest import mock

    with mock.patch(
        "subprocess.run",
        side_effect=subprocess.TimeoutExpired(cmd=["cargo", "metadata"], timeout=COMMAND_TIMEOUT_S),
    ):
        try:
            _run(["cargo", "metadata"], root, check=False)
        except HarnessError as exc:
            if "timed out" not in str(exc):
                raise
        else:
            raise HarnessError("TimeoutExpired must become HarnessError")


def _repo_root_self_checks() -> None:
    expected = Path(__file__).resolve().parents[2]
    if repo_root() != expected:
        raise HarnessError("repo_root() must derive from __file__ only")

    hostile = Path(tempfile.mkdtemp(prefix="adequacy-hostile-"))
    old_cwd = Path.cwd()
    old_env = os.environ.pop("ADEQUACY_REPO_ROOT", None)
    try:
        os.environ["ADEQUACY_REPO_ROOT"] = str(hostile / "nested")
        os.chdir(hostile)
        if repo_root() != expected:
            raise HarnessError("hostile ADEQUACY_REPO_ROOT and cwd must not change repo_root()")
    finally:
        os.chdir(old_cwd)
        if old_env is None:
            os.environ.pop("ADEQUACY_REPO_ROOT", None)
        else:
            os.environ["ADEQUACY_REPO_ROOT"] = old_env

    try:
        main(["verify-vendor", "--repo-root", str(hostile)])
    except SystemExit as exc:
        if exc.code in (None, 0):
            raise HarnessError("--repo-root must be rejected by argparse")
    else:
        raise HarnessError("--repo-root must be rejected by argparse")


def self_test(root: Path) -> None:
    _repo_root_self_checks()
    lock_before = (root / "Cargo.lock").read_bytes()
    verify_vendor(root)
    _metadata_validator_self_checks(root)
    _timeout_wiring_self_check(root)
    _timeout_self_check(root)
    try:
        build(root)
    except HarnessError as exc:
        if "git worktree" not in str(exc):
            raise
    else:
        raise HarnessError("build must refuse a git worktree (writer checkout)")

    scratch = scratch2 = scratch3 = scratch4 = None
    try:
        scratch = _copy_root(root)
        _mutate_compile_break(scratch)
        try:
            build(scratch)
        except HarnessError as exc:
            if _vendor_mismatch(exc):
                raise HarnessError("compile-breaking mutant must reach cargo, not vendor mismatch") from exc
            if "command failed" not in str(exc):
                raise
        else:
            raise HarnessError("compile-breaking mutation must fail build")

        scratch4 = _copy_root(root)
        _mutate_wiring(scratch4)
        try:
            build(scratch4)
        except HarnessError as exc:
            if _vendor_mismatch(exc):
                raise HarnessError("compilable lib.rs mutant must reach cargo, not vendor mismatch") from exc
            raise

        scratch2 = _copy_root(root)
        _mutate_wiring(scratch2)
        build(scratch2)
        failed = _parse_failed_vectors(_run_conformance(scratch2, patch=True))
        if failed != EXPECTED_WIRING_VECTORS:
            raise HarnessError(f"P-WIRING vector set mismatch: got {failed}, want {EXPECTED_WIRING_VECTORS}")

        scratch3 = _copy_root(root)
        _mutate_wiring(scratch3)
        proc = _run_conformance(scratch3, patch=False)
        if proc.returncode != 0:
            raise HarnessError("P-PATCH-OFF: registry build must stay green without patch")
    finally:
        for tree in (scratch, scratch2, scratch3, scratch4):
            if tree is not None:
                _drop_copy(tree)

    bad = _copy_root(root)
    try:
        (bad / PATCH_PATH / "src/lib.rs").write_text("not rust\n", encoding="utf-8")
        try:
            verify_vendor(bad)
        except HarnessError:
            pass
        else:
            raise HarnessError("corrupt extracted lib.rs must fail verify-vendor")
    finally:
        _drop_copy(bad)

    extra = _copy_root(root)
    try:
        (extra / PATCH_PATH / "extra.txt").write_text("unexpected\n", encoding="utf-8")
        try:
            verify_vendor(extra)
        except HarnessError as exc:
            if "extra=" not in str(exc):
                raise
        else:
            raise HarnessError("extra extracted file must fail verify-vendor")
    finally:
        _drop_copy(extra)

    prov_bad = _copy_root(root)
    try:
        prov_path = prov_bad / "third_party/serde_jcs-0.2.0.provenance.json"
        doc = json.loads(prov_path.read_text(encoding="utf-8"))
        doc["file_count"] = 28
        prov_path.write_text(json.dumps(doc), encoding="utf-8")
        try:
            verify_vendor(prov_bad)
        except HarnessError as exc:
            if "file_count" not in str(exc):
                raise
        else:
            raise HarnessError("provenance drift must fail verify-vendor")
    finally:
        _drop_copy(prov_bad)

    lock_missing = _copy_root(root)
    try:
        (lock_missing / PATCH_PATH / "Cargo.lock").unlink()
        try:
            verify_vendor(lock_missing)
        except HarnessError as exc:
            if "Cargo.lock" not in str(exc):
                raise
        else:
            raise HarnessError("missing tracked Cargo.lock must fail verify-vendor")
    finally:
        _drop_copy(lock_missing)

    lock_bad = _copy_root(root)
    try:
        text = (lock_bad / "Cargo.lock").read_text(encoding="utf-8")
        (lock_bad / "Cargo.lock").write_text(text.replace(RYU_JS_CHECKSUM, "0" * 64), encoding="utf-8")
        try:
            build(lock_bad)
        except HarnessError as exc:
            if "ryu-js checksum mismatch" not in str(exc):
                raise
        else:
            raise HarnessError("unrelated lock mutation must fail build/metadata guard")
    finally:
        _drop_copy(lock_bad)

    if (root / "Cargo.lock").read_bytes() != lock_before:
        raise HarnessError("writer worktree Cargo.lock must stay byte-unchanged after self-test")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("self-test", "verify-vendor", "build", "test"))
    args = parser.parse_args(argv)
    root = repo_root()
    if args.command == "self-test":
        self_test(root)
        print("OK: adequacy_serde_jcs self-test")
        return 0
    if args.command == "verify-vendor":
        verify_vendor(root)
        print("OK: vendored serde_jcs verified")
        return 0
    if args.command == "build":
        build(root)
        print("OK: patched assay-canonical tests build")
        return 0
    return test(root)


if __name__ == "__main__":
    raise SystemExit(main())
