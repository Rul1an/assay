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
import shutil
import sys
import tempfile
from datetime import UTC, datetime
from pathlib import Path
from typing import Any


SCHEMA = "assay.runner.delegated_proof_pack.v0"
KIND = "delegated_runner_proof_pack"
GATE_SELECTIONS = {
    "all": (
        "kernel-only",
        "kernel-policy",
        "openai-agents-kernel-policy",
        "openai-agents-hidden-write",
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


class ProofPackError(Exception):
    pass


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def copy_payload_file(source: Path, output_root: Path, relative: Path) -> dict[str, Any]:
    target = output_root / "payload" / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, target)
    return {
        "path": str(Path("payload") / relative),
        "bytes": target.stat().st_size,
        "sha256": sha256_file(target),
    }


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

    proof_root = args.proof_root
    output_root = args.output_dir
    if output_root.exists():
        shutil.rmtree(output_root)
    output_root.mkdir(parents=True)

    gates = [collect_gate(gate, proof_root, output_root) for gate in selected_gates]
    manifest = {
        "schema": SCHEMA,
        "kind": KIND,
        "created_at": datetime.now(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
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
        "gates": gates,
        "payload_files": [],
        "pack_size_bytes": 0,
    }
    manifest["payload_files"] = payload_files(output_root)
    write_manifest(output_root, manifest)
    return manifest


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run a local collector self-test")
    parser.add_argument("--proof-root", type=Path, help="delegated proof work root")
    parser.add_argument("--output-dir", type=Path, help="directory to upload as the proof-pack artifact")
    parser.add_argument("--gates", default="all", help="delegated gates input")
    parser.add_argument("--build-ebpf", default="true", help="delegated build_ebpf input")
    parser.add_argument("--run-id", default="", help="GitHub workflow run id")
    parser.add_argument("--run-attempt", default="", help="GitHub workflow run attempt")
    parser.add_argument("--run-url", default="", help="GitHub workflow run URL")
    parser.add_argument("--ref", default="", help="Git ref for the workflow run")
    parser.add_argument("--head-sha", default="", help="workflow head SHA")
    parser.add_argument("--workflow-sha", default="", help="workflow definition SHA")
    parser.add_argument("--workflow-name", default="Runner Spike Delegated")
    parser.add_argument("--retention-days", type=int, default=365)
    parser.add_argument("--soft-cap-bytes", type=int, default=50 * 1024 * 1024)
    return parser.parse_args(argv)


def self_test() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        proof_root = root / "proof"
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
            output_dir=root / "upload",
            gates="all",
            build_ebpf="true",
            run_id="123",
            run_attempt="1",
            run_url="https://github.example/run/123",
            ref="refs/heads/test",
            head_sha="abc",
            workflow_sha="def",
            workflow_name="Runner Spike Delegated",
            retention_days=365,
            soft_cap_bytes=50 * 1024 * 1024,
        )
        manifest = build_manifest(args)
        if manifest["schema"] != SCHEMA:
            raise ProofPackError("self-test manifest schema mismatch")
        if manifest["verification_modes"]["current_state"] != "fresh_delegated_dispatch_required":
            raise ProofPackError("self-test verification mode mismatch")
        statuses = {gate["gate"]: gate["status"] for gate in manifest["gates"]}
        if statuses != {
            "kernel-only": "passed",
            "kernel-policy": "missing",
            "openai-agents-kernel-policy": "missing",
            "openai-agents-hidden-write": "missing",
        }:
            raise ProofPackError(f"self-test gate status mismatch: {statuses}")
        if not manifest["gates"][0]["archives"]:
            raise ProofPackError("self-test did not collect archive digest")
        if not (args.output_dir / "manifest.json").exists():
            raise ProofPackError("self-test did not write manifest")


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
