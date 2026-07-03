#!/usr/bin/env python3
"""Build a delegated Assay-Runner proof-pack upload directory.

The delegated workflow runs on a self-hosted Linux/eBPF host and writes runner
archives under temporary gate directories. This helper copies the durable
forensic subset into a separate upload directory before workflow cleanup removes
those temporary paths.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import sys
import tempfile
from datetime import UTC, datetime
from pathlib import Path
from typing import Any


SCHEMA = "assay.runner.delegated_proof_pack.v1"
KIND = "delegated_runner_proof_pack"
GATE_SELECTIONS = {
    "all": (
        "kernel-only",
        "kernel-policy",
        "openai-agents-kernel-policy",
        "openai-agents-hidden-write",
        "gemini-google-genai-kernel-policy",
    ),
    "kernel-only": ("kernel-only",),
    "kernel-policy": ("kernel-policy",),
    "openai-agents-kernel-policy": ("openai-agents-kernel-policy",),
}
SELECTED_JSON = {
    "manifest.json",
    "observation-health.json",
    "capability-surface.json",
    "correlation-report.json",
}
GATED_PATHS_DOC = "scripts/ci/assay_runner_gated_paths.json"
CLAIM_CEILING = "delegated_gate_execution_only_not_runtime_safety"
DEFAULT_OUTPUT_DIR = Path("assay-runner-proof-upload")
DEFAULT_EBPF_OBJECT = Path("target/assay-ebpf.o")
DEFAULT_EBPF_PROVENANCE = Path("target/assay-ebpf.provenance.json")


class ProofPackError(Exception):
    pass


def sha256_file_hex(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sha256_file(path: Path) -> str:
    return "sha256:" + sha256_file_hex(path)


def copy_payload_file(source: Path, output_root: Path, relative: Path) -> dict[str, Any]:
    target = output_root / "payload" / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, target)
    return {
        "path": str(Path("payload") / relative),
        "bytes": target.stat().st_size,
        "sha256": sha256_file(target),
    }


def copy_optional_payload_file(source: Path | None, output_root: Path, relative: Path) -> dict[str, Any] | None:
    if source is None or not source.exists():
        return None
    return copy_payload_file(source, output_root, relative)


def pass_lines(log_path: Path) -> list[str]:
    if not log_path.exists():
        return []
    lines = []
    for line in log_path.read_text(encoding="utf-8", errors="replace").splitlines():
        if line.startswith("PASS:"):
            lines.append(line)
    return lines


def gate_status(gate_dir: Path) -> str:
    status_path = gate_dir / "status.txt"
    if not gate_dir.exists():
        return "missing"
    if not status_path.exists():
        return "incomplete"
    value = status_path.read_text(encoding="utf-8").strip()
    if value in {"passed", "failed", "skipped", "incomplete"}:
        return value
    return "incomplete"


def collect_gate(gate: str, proof_root: Path, output_root: Path) -> dict[str, Any]:
    gate_dir = proof_root / "gates" / gate
    status = gate_status(gate_dir)
    entry: dict[str, Any] = {
        "gate": gate,
        "status": status,
        "archives": [],
        "selected_json": [],
        "pass_lines": [],
    }

    if not gate_dir.exists():
        return entry

    log_path = gate_dir / "gate.log"
    if log_path.exists():
        copied = copy_payload_file(log_path, output_root, Path("gates") / gate / "gate.log")
        entry["gate_log"] = copied
        entry["pass_lines"] = pass_lines(log_path)

    for archive in sorted(gate_dir.rglob("runner-*.tar.gz")):
        relative = Path("gates") / gate / archive.relative_to(gate_dir)
        entry["archives"].append(copy_payload_file(archive, output_root, relative))

    for json_path in sorted(gate_dir.rglob("*.json")):
        if json_path.name not in SELECTED_JSON:
            continue
        relative = Path("gates") / gate / json_path.relative_to(gate_dir)
        entry["selected_json"].append(copy_payload_file(json_path, output_root, relative))

    if status == "passed" and not entry["archives"]:
        entry["status"] = "incomplete"
        entry.setdefault("notes", []).append("passed gate did not leave runner archive tarballs")

    return entry


def payload_files(output_root: Path) -> list[dict[str, Any]]:
    payload_root = output_root / "payload"
    if not payload_root.exists():
        return []
    files = []
    for path in sorted(item for item in payload_root.rglob("*") if item.is_file()):
        relative = path.relative_to(output_root)
        files.append(
            {
                "path": str(relative),
                "bytes": path.stat().st_size,
                "sha256": sha256_file(path),
            }
        )
    return files


def validate_path_within_root(candidate: Path, root: Path, *, label: str) -> Path:
    resolved_root = root.resolve(strict=False)
    if not candidate.is_absolute():
        candidate = resolved_root / candidate
    resolved_candidate = candidate.resolve(strict=False)
    try:
        resolved_candidate.relative_to(resolved_root)
    except ValueError as exc:
        raise ProofPackError(f"{label} must be within workspace root {resolved_root}: {candidate}") from exc
    return resolved_candidate


def workspace_display_path(path: Path, workspace_root: Path) -> str:
    resolved = path.resolve()
    root = workspace_root.resolve()
    try:
        return str(resolved.relative_to(root))
    except ValueError as exc:
        raise ProofPackError(f"path escapes workspace root {root}: {path}") from exc


def role_for_payload_path(path: str) -> str:
    if path == "payload/build/assay-ebpf.provenance.json":
        return "ebpf_build_provenance"
    if path.endswith("/gate.log"):
        return "gate_log"
    if path.endswith(".tar.gz"):
        return "runner_archive"
    if path.endswith(".json"):
        return "gate_json"
    return "payload"


def subject_for_file(path: Path, *, name: str, role: str) -> dict[str, Any]:
    return {
        "path": name,
        "bytes": path.stat().st_size,
        "sha256": sha256_file(path),
        "role": role,
    }


def proof_subjects(output_root: Path, ebpf_object: Path | None, workspace_root: Path) -> list[dict[str, Any]]:
    subjects: list[dict[str, Any]] = []
    if ebpf_object is not None and ebpf_object.exists():
        subjects.append(
            subject_for_file(
                ebpf_object,
                name=workspace_display_path(ebpf_object, workspace_root),
                role="ebpf_object",
            )
        )
    for item in payload_files(output_root):
        physical = output_root / Path(item["path"])
        subjects.append(
            subject_for_file(
                physical,
                name=workspace_display_path(physical, workspace_root),
                role=role_for_payload_path(item["path"]),
            )
        )
    return subjects


def load_required_content_provenance_paths() -> tuple[str, ...]:
    root = Path(__file__).resolve().parents[2]
    with (root / GATED_PATHS_DOC).open(encoding="utf-8") as handle:
        manifest = json.load(handle)
    return tuple(str(path) for path in manifest["content_provenance_paths"])


def load_path_trees(ebpf_provenance: Path | None, *, require: bool) -> dict[str, Any]:
    if ebpf_provenance is None or not ebpf_provenance.exists():
        if require:
            raise ProofPackError("missing required eBPF provenance for content-addressed proof")
        return {}
    try:
        document = json.loads(ebpf_provenance.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise ProofPackError(f"invalid eBPF provenance JSON: {exc}") from exc

    source = document.get("source")
    if source is None:
        source = {}
    if not isinstance(source, dict):
        raise ProofPackError("eBPF provenance source must be an object")
    path_trees = source.get("path_trees") or {}
    if not isinstance(path_trees, dict):
        raise ProofPackError("eBPF provenance source.path_trees must be an object")

    required_paths = load_required_content_provenance_paths()
    errors: list[str] = []
    normalized: dict[str, Any] = {}
    for path in required_paths:
        entry = path_trees.get(path)
        if not isinstance(entry, dict):
            errors.append(f"{path}: missing tree entry")
            normalized[path] = {"oid": None, "error": "missing_tree_entry"}
            continue
        oid = entry.get("oid")
        error = entry.get("error")
        normalized[path] = {"oid": oid, "error": error}
        if not isinstance(oid, str) or not oid:
            errors.append(f"{path}: missing oid")
        if error not in (None, ""):
            errors.append(f"{path}: {error}")
    if errors:
        raise ProofPackError("invalid content provenance path tree(s): " + "; ".join(errors))
    return normalized


def write_subject_checksums(
    output_root: Path,
    checksum_path: Path,
    subjects: list[dict[str, Any]],
    workspace_root: Path,
) -> None:
    lines: list[str] = []
    manifest_path = output_root / "manifest.json"
    attested_subjects = [
        subject_for_file(
            manifest_path,
            name=workspace_display_path(manifest_path, workspace_root),
            role="proof_pack_manifest",
        ),
        *subjects,
    ]
    for subject in attested_subjects:
        digest = str(subject["sha256"])
        if not digest.startswith("sha256:"):
            raise ProofPackError(f"unsupported subject digest for {subject['path']}: {digest}")
        lines.append(f"{digest[len('sha256:') :]}  {subject['path']}")
    checksum_path.parent.mkdir(parents=True, exist_ok=True)
    checksum_path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def total_size(output_root: Path) -> int:
    return sum(path.stat().st_size for path in output_root.rglob("*") if path.is_file())


def write_manifest(output_root: Path, manifest: dict[str, Any]) -> None:
    manifest_path = output_root / "manifest.json"
    for _ in range(10):
        manifest["pack_size_bytes"] = total_size(output_root)
        text = json.dumps(manifest, indent=2, sort_keys=True) + "\n"
        manifest_path.write_text(text, encoding="utf-8")
        new_size = total_size(output_root)
        if new_size == manifest["pack_size_bytes"]:
            return
    raise ProofPackError("manifest pack_size_bytes did not stabilize")


def build_manifest(args: argparse.Namespace) -> dict[str, Any]:
    selected_gates = GATE_SELECTIONS.get(args.gates)
    if selected_gates is None:
        allowed = ", ".join(sorted(GATE_SELECTIONS))
        raise ProofPackError(f"unsupported gates value {args.gates!r}; expected one of {allowed}")

    workspace_root = Path.cwd().resolve(strict=False)
    proof_root = args.proof_root
    output_root = validate_path_within_root(args.output_dir, workspace_root, label="proof upload directory")
    ebpf_object = validate_path_within_root(args.ebpf_object, workspace_root, label="eBPF object")
    ebpf_provenance_path = validate_path_within_root(
        args.ebpf_provenance,
        workspace_root,
        label="eBPF provenance",
    )
    subject_checksums = validate_path_within_root(
        output_root / "subject-checksums.txt",
        workspace_root,
        label="subject checksums",
    )
    if output_root.exists():
        shutil.rmtree(output_root)
    output_root.mkdir(parents=True)

    gates = [collect_gate(gate, proof_root, output_root) for gate in selected_gates]
    ebpf_provenance = copy_optional_payload_file(
        ebpf_provenance_path,
        output_root,
        Path("build") / "assay-ebpf.provenance.json",
    )
    require_content_provenance = str(args.build_ebpf).lower() == "true"
    path_trees = load_path_trees(ebpf_provenance_path, require=require_content_provenance)
    subjects = proof_subjects(output_root, ebpf_object, workspace_root)
    manifest = {
        "schema": SCHEMA,
        "kind": KIND,
        "proof_pack": {
            "schema": SCHEMA,
            "subjects": subjects,
        },
        "created_at": datetime.now(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "source": {
            "repository": args.repository,
            "head_sha": args.head_sha,
            "ref": args.ref,
            "workflow_name": args.workflow_name,
            "workflow_path": args.workflow_path,
            "workflow_sha": args.workflow_sha,
            "run_id": args.run_id,
            "run_attempt": args.run_attempt,
            "run_url": args.run_url,
        },
        "inputs": {
            "gates": args.gates,
            "build_ebpf": args.build_ebpf,
        },
        "content_provenance": {
            "path_trees": path_trees,
            "source": ebpf_provenance["path"] if ebpf_provenance else None,
        },
        "claim_ceiling": CLAIM_CEILING,
        "workflow": {
            "name": args.workflow_name,
            "run_id": args.run_id,
            "run_attempt": args.run_attempt,
            "run_url": args.run_url,
            "ref": args.ref,
            "head_sha": args.head_sha,
            "workflow_sha": args.workflow_sha,
            "inputs": {
                "gates": args.gates,
                "build_ebpf": args.build_ebpf,
            },
        },
        "verification_modes": {
            "historical": "proof_pack_sufficient_for_recorded_run",
            "current_state": "fresh_delegated_dispatch_required",
        },
        "retention_days": args.retention_days,
        "expected_size_policy": {
            "soft_cap_bytes": args.soft_cap_bytes,
            "action": "revisit retention and quota impact before expanding delegated matrices",
        },
        "evidence_boundary": {
            "separate_from_normalized_runner_evidence": True,
            "not_a_runner_emitted_artifact": True,
        },
        "build_provenance": {
            "ebpf": ebpf_provenance,
        },
        "gates": gates,
        "payload_files": [],
        "pack_size_bytes": 0,
    }
    manifest["payload_files"] = payload_files(output_root)
    manifest["proof_pack"]["subjects"] = subjects
    write_manifest(output_root, manifest)
    if subject_checksums:
        write_subject_checksums(output_root, subject_checksums, manifest["proof_pack"]["subjects"], workspace_root)
    return manifest


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run a local collector self-test")
    parser.add_argument("--proof-root", type=Path, help="delegated proof work root")
    parser.add_argument("--gates", default="all", help="delegated gates input")
    parser.add_argument("--build-ebpf", default="true", help="delegated build_ebpf input")
    parser.add_argument("--run-id", default="", help="GitHub workflow run id")
    parser.add_argument("--run-attempt", default="", help="GitHub workflow run attempt")
    parser.add_argument("--run-url", default="", help="GitHub workflow run URL")
    parser.add_argument("--ref", default="", help="Git ref for the workflow run")
    parser.add_argument("--head-sha", default="", help="workflow head SHA")
    parser.add_argument("--workflow-sha", default="", help="workflow definition SHA")
    parser.add_argument("--workflow-name", default="Runner Spike Delegated")
    parser.add_argument("--workflow-path", default=".github/workflows/runner-spike-delegated.yml")
    parser.add_argument("--repository", default="")
    parser.add_argument("--retention-days", type=int, default=365)
    parser.add_argument("--soft-cap-bytes", type=int, default=50 * 1024 * 1024)
    args = parser.parse_args(argv)
    args.output_dir = DEFAULT_OUTPUT_DIR
    args.ebpf_object = DEFAULT_EBPF_OBJECT
    args.ebpf_provenance = DEFAULT_EBPF_PROVENANCE
    return args


def self_test() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        proof_root = root / "proof"
        ebpf_object = root / "target" / "assay-ebpf.o"
        ebpf_object.parent.mkdir(parents=True)
        ebpf_object.write_bytes(b"ebpf")
        if validate_path_within_root(Path("target/assay-ebpf.o"), root, label="self-test") != ebpf_object.resolve(
            strict=False
        ):
            raise ProofPackError("self-test relative path did not resolve under workspace root")
        ebpf_provenance = root / "target" / "assay-ebpf.provenance.json"
        ebpf_provenance.parent.mkdir(parents=True, exist_ok=True)
        path_trees = {
            path: {"oid": hashlib.sha1(path.encode("utf-8")).hexdigest(), "error": None}
            for path in load_required_content_provenance_paths()
        }
        ebpf_provenance.write_text(
            json.dumps(
                {
                    "schema": "assay.ci.ebpf_build_provenance.v0",
                    "source": {"path_trees": path_trees},
                },
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )
        gate_dir = proof_root / "gates" / "kernel-only" / "run-1" / "extract"
        gate_dir.mkdir(parents=True)
        (proof_root / "gates" / "kernel-only" / "status.txt").write_text("passed\n", encoding="utf-8")
        (proof_root / "gates" / "kernel-only" / "gate.log").write_text(
            "noise\nPASS: runner-spike kernel-only acceptance\n",
            encoding="utf-8",
        )
        (proof_root / "gates" / "kernel-only" / "run-1" / "runner-kernel-only.tar.gz").write_bytes(b"tar")
        (gate_dir / "observation-health.json").write_text('{"schema":"x"}\n', encoding="utf-8")

        args = argparse.Namespace(
            proof_root=proof_root,
            output_dir=Path("upload"),
            gates="all",
            build_ebpf="true",
            run_id="123",
            run_attempt="1",
            run_url="https://github.example/run/123",
            ref="refs/heads/test",
            head_sha="abc",
            workflow_sha="def",
            workflow_name="Runner Spike Delegated",
            workflow_path=".github/workflows/runner-spike-delegated.yml",
            repository="Rul1an/assay",
            ebpf_provenance=Path("target/assay-ebpf.provenance.json"),
            ebpf_object=Path("target/assay-ebpf.o"),
            retention_days=365,
            soft_cap_bytes=50 * 1024 * 1024,
        )
        old_cwd = Path.cwd()
        try:
            os.chdir(root)
            manifest = build_manifest(args)
        finally:
            os.chdir(old_cwd)
        if manifest["schema"] != SCHEMA:
            raise ProofPackError("self-test manifest schema mismatch")
        if manifest["proof_pack"]["schema"] != SCHEMA:
            raise ProofPackError("self-test proof_pack schema mismatch")
        subject_roles = {subject["role"] for subject in manifest["proof_pack"]["subjects"]}
        if not {"ebpf_object", "ebpf_build_provenance", "runner_archive", "gate_log", "gate_json"} <= subject_roles:
            raise ProofPackError(f"self-test subject roles incomplete: {subject_roles}")
        if manifest["source"]["repository"] != "Rul1an/assay":
            raise ProofPackError("self-test source repository mismatch")
        if manifest["inputs"] != {"gates": "all", "build_ebpf": "true"}:
            raise ProofPackError("self-test inputs mismatch")
        if set(manifest["content_provenance"]["path_trees"]) != set(load_required_content_provenance_paths()):
            raise ProofPackError("self-test path_trees missing required paths")
        if manifest["claim_ceiling"] != CLAIM_CEILING:
            raise ProofPackError("self-test claim ceiling mismatch")
        if manifest["verification_modes"]["current_state"] != "fresh_delegated_dispatch_required":
            raise ProofPackError("self-test verification mode mismatch")
        statuses = {gate["gate"]: gate["status"] for gate in manifest["gates"]}
        if statuses != {
            "kernel-only": "passed",
            "kernel-policy": "missing",
            "openai-agents-kernel-policy": "missing",
            "openai-agents-hidden-write": "missing",
            "gemini-google-genai-kernel-policy": "missing",
        }:
            raise ProofPackError(f"self-test gate status mismatch: {statuses}")
        if not manifest["gates"][0]["archives"]:
            raise ProofPackError("self-test did not collect archive digest")
        if not manifest["build_provenance"]["ebpf"]:
            raise ProofPackError("self-test did not collect eBPF build provenance")
        if not (root / "upload" / "manifest.json").exists():
            raise ProofPackError("self-test did not write manifest")
        checksums = (root / "upload" / "subject-checksums.txt").read_text(encoding="utf-8").splitlines()
        if not any(line.endswith("manifest.json") for line in checksums):
            raise ProofPackError("self-test checksum file did not include manifest")
        if len(checksums) != 1 + len(manifest["proof_pack"]["subjects"]):
            raise ProofPackError("self-test checksum file did not include every subject")

        broken_provenance = root / "broken-provenance.json"
        broken = json.loads(ebpf_provenance.read_text(encoding="utf-8"))
        first_path = load_required_content_provenance_paths()[0]
        broken["source"]["path_trees"][first_path]["oid"] = None
        broken_provenance.write_text(json.dumps(broken) + "\n", encoding="utf-8")
        broken_args = argparse.Namespace(
            **{**vars(args), "output_dir": Path("broken-upload"), "ebpf_provenance": broken_provenance}
        )
        try:
            old_cwd = Path.cwd()
            try:
                os.chdir(root)
                build_manifest(broken_args)
            finally:
                os.chdir(old_cwd)
        except ProofPackError as exc:
            if "invalid content provenance path tree" not in str(exc):
                raise
        else:
            raise ProofPackError("self-test accepted broken content provenance")

        malformed_source = root / "malformed-source-provenance.json"
        malformed_source.write_text('{"source": "not-an-object"}\n', encoding="utf-8")
        malformed_args = argparse.Namespace(
            **{**vars(args), "output_dir": Path("malformed-upload"), "ebpf_provenance": malformed_source}
        )
        try:
            old_cwd = Path.cwd()
            try:
                os.chdir(root)
                build_manifest(malformed_args)
            finally:
                os.chdir(old_cwd)
        except ProofPackError as exc:
            if "source must be an object" not in str(exc):
                raise
        else:
            raise ProofPackError("self-test accepted malformed provenance source")


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    try:
        if args.self_test:
            self_test()
            print("delegated proof-pack self-test ok")
            return 0
        if args.proof_root is None or args.output_dir is None:
            raise ProofPackError("--proof-root and --output-dir are required")
        manifest = build_manifest(args)
        print(
            "delegated proof-pack manifest written: "
            f"{args.output_dir / 'manifest.json'} ({manifest['pack_size_bytes']} bytes)"
        )
    except ProofPackError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
