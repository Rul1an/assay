#!/usr/bin/env python3
"""Build experiment-scoped agent observability evidence packs."""

from __future__ import annotations

import argparse
import hashlib
import json
import shlex
import shutil
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

EVIDENCE_PACK_SCHEMA = "assay.experiment.agent_observability_fidelity.evidence_pack.v0"
REDACTION_MANIFEST_SCHEMA = (
    "assay.experiment.agent_observability_fidelity.redaction_manifest.v0"
)
EXPERIMENT = "agent-observability-fidelity-2026-05"


def utc_now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace(
        "+00:00", "Z"
    )


def sha256_file(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(chunk)
    return f"sha256:{hasher.hexdigest()}"


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def copy_artifact(
    *,
    source: Path,
    destination: Path,
    role: str,
    required: bool,
) -> dict[str, Any]:
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    return {
        "role": role,
        "path": str(destination),
        "required": required,
        "bytes": destination.stat().st_size,
        "sha256": sha256_file(destination),
        "redaction_state": "unredacted",
    }


def artifact_row(
    *,
    pack_dir: Path,
    path: Path,
    role: str,
    required: bool,
    redaction_state: str,
) -> dict[str, Any]:
    return {
        "role": role,
        "path": str(path.relative_to(pack_dir)),
        "required": required,
        "bytes": path.stat().st_size,
        "sha256": sha256_file(path),
        "redaction_state": redaction_state,
    }


def relative_artifact_rows(
    pack_dir: Path, rows: list[dict[str, Any]]
) -> list[dict[str, Any]]:
    relative_rows = []
    for row in rows:
        rel = dict(row)
        rel["path"] = str(Path(row["path"]).relative_to(pack_dir))
        relative_rows.append(rel)
    return relative_rows


def validate_source(path: Path, label: str) -> None:
    if not path.exists():
        raise FileNotFoundError(f"{label} does not exist: {path}")
    if not path.is_file():
        raise ValueError(f"{label} is not a file: {path}")


def load_observation_health(path: Path) -> dict[str, Any]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    return {
        "kernel_layer": payload.get("kernel_layer", "unknown"),
        "ringbuf_drops": int(payload.get("ringbuf_drops", -1)),
        "cgroup_correlation": payload.get("cgroup_correlation", "unknown"),
    }


def health_status(health: dict[str, Any]) -> str:
    if (
        health.get("kernel_layer") == "complete"
        and health.get("ringbuf_drops") == 0
        and health.get("cgroup_correlation") == "clean"
    ):
        return "clean"
    return "inconclusive"


def pack_id_from_rows(rows: list[dict[str, Any]]) -> str:
    material = json.dumps(
        [
            {
                "role": row["role"],
                "path": row["path"],
                "sha256": row["sha256"],
                "bytes": row["bytes"],
            }
            for row in rows
        ],
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")
    return f"sha256:{hashlib.sha256(material).hexdigest()}"


def redaction_manifest(*, created_at: str, policy: str) -> dict[str, Any]:
    return {
        "schema": REDACTION_MANIFEST_SCHEMA,
        "created_at": created_at,
        "policy": policy,
        "redaction_applied": False,
        "omitted_content": [],
        "notes": [
            "Prototype pack records redaction explicitly; no redaction was applied."
        ],
    }


def reproduction_command(
    *,
    out_dir: Path,
    scenario_id: str,
    claim_summary: str,
    claim_class: str,
    runner_archive: Path,
    trace_json: Path | None,
    observation_health: Path,
    created_at: str,
    redaction_policy: str,
) -> str:
    args = [
        "python3",
        "docs/experiments/agent-observability-fidelity-2026-05/evidence_pack.py",
        "create",
        "--out-dir",
        str(out_dir),
        "--scenario-id",
        scenario_id,
        "--claim-class",
        claim_class,
        "--claim-summary",
        claim_summary,
        "--runner-archive",
        str(runner_archive),
        "--observation-health",
        str(observation_health),
        "--created-at",
        created_at,
        "--redaction-policy",
        redaction_policy,
    ]
    if trace_json is not None:
        args.extend(["--trace-json", str(trace_json)])
    return " ".join(shlex.quote(arg) for arg in args)


def summary_markdown(manifest: dict[str, Any]) -> str:
    health = manifest["observation_health"]
    lines = [
        "# Agent Observability Evidence Pack",
        "",
        "| Field | Value |",
        "|---|---|",
        f"| Pack ID | `{manifest['pack_id']}` |",
        f"| Scenario | `{manifest['scenario_id']}` |",
        f"| Claim class | `{manifest['claim_class']}` |",
        f"| Claim summary | {manifest['claim_summary']} |",
        f"| Observation health | `{manifest['observation_health_status']}` |",
        f"| Kernel layer | `{health['kernel_layer']}` |",
        f"| Ringbuf drops | `{health['ringbuf_drops']}` |",
        f"| Cgroup correlation | `{health['cgroup_correlation']}` |",
        f"| Redaction applied | `{manifest['redaction']['redaction_applied']}` |",
        "",
        "## Artifacts",
        "",
        "| Role | Path | Required | SHA-256 |",
        "|---|---|---:|---|",
    ]
    for artifact in manifest["artifacts"]:
        lines.append(
            f"| `{artifact['role']}` | `{artifact['path']}` | "
            f"`{artifact['required']}` | `{artifact['sha256']}` |"
        )
    lines.extend(
        [
            "",
            "## Non-Claims",
            "",
        ]
    )
    for non_claim in manifest["non_claims"]:
        lines.append(f"- {non_claim}")
    lines.append("")
    return "\n".join(lines)


def build_pack(
    *,
    out_dir: Path,
    scenario_id: str,
    claim_summary: str,
    claim_class: str,
    runner_archive: Path,
    trace_json: Path | None,
    observation_health: Path,
    created_at: str,
    redaction_policy: str,
) -> dict[str, Any]:
    if out_dir.exists() and any(out_dir.iterdir()):
        raise FileExistsError(f"output directory is not empty: {out_dir}")
    out_dir.mkdir(parents=True, exist_ok=True)
    artifacts_dir = out_dir / "artifacts"
    rows = [
        copy_artifact(
            source=runner_archive,
            destination=artifacts_dir / runner_archive.name,
            role="runner_archive",
            required=True,
        ),
        copy_artifact(
            source=observation_health,
            destination=artifacts_dir / "observation-health.json",
            role="observation_health",
            required=True,
        ),
    ]
    if trace_json is not None:
        rows.append(
            copy_artifact(
                source=trace_json,
                destination=artifacts_dir / "trace.json",
                role="trace_json",
                required=False,
            )
        )
    redaction = redaction_manifest(created_at=created_at, policy=redaction_policy)
    write_json(out_dir / "redaction-manifest.json", redaction)
    rows.append(
        {
            "role": "redaction_manifest",
            "path": str(out_dir / "redaction-manifest.json"),
            "required": True,
            "bytes": (out_dir / "redaction-manifest.json").stat().st_size,
            "sha256": sha256_file(out_dir / "redaction-manifest.json"),
            "redaction_state": "manifest",
        }
    )
    relative_rows = relative_artifact_rows(out_dir, rows)
    health = load_observation_health(observation_health)
    manifest = {
        "schema": EVIDENCE_PACK_SCHEMA,
        "experiment": EXPERIMENT,
        "pack_id": pack_id_from_rows(relative_rows),
        "scenario_id": scenario_id,
        "created_at": created_at,
        "claim_class": claim_class,
        "claim_summary": claim_summary,
        "observation_health_status": health_status(health),
        "observation_health": health,
        "artifacts": relative_rows,
        "redaction": redaction,
        "reproduction": {
            "command": reproduction_command(
                out_dir=out_dir,
                scenario_id=scenario_id,
                claim_summary=claim_summary,
                claim_class=claim_class,
                runner_archive=runner_archive,
                trace_json=trace_json,
                observation_health=observation_health,
                created_at=created_at,
                redaction_policy=redaction_policy,
            ),
            "inputs": {
                "runner_archive": str(runner_archive),
                "trace_json": str(trace_json) if trace_json is not None else None,
                "observation_health": str(observation_health),
            },
        },
        "non_claims": [
            "does_not_strengthen_underlying_claims",
            "does_not_verify_runner_archive_integrity",
            "does_not_promote_evidence_pack_to_product_api",
        ],
    }
    summary_path = out_dir / "summary.md"
    summary_path.write_text(summary_markdown(manifest), encoding="utf-8")
    manifest["artifacts"].append(
        artifact_row(
            pack_dir=out_dir,
            path=summary_path,
            role="summary_markdown",
            required=True,
            redaction_state="rendered",
        )
    )
    write_json(out_dir / "manifest.json", manifest)
    return manifest


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    create = subparsers.add_parser("create", help="Create an evidence pack directory")
    create.add_argument("--out-dir", type=Path, required=True)
    create.add_argument("--scenario-id", required=True)
    create.add_argument("--claim-summary", required=True)
    create.add_argument(
        "--claim-class",
        choices=("diagnostic", "fidelity_boundary", "semantic_gap", "positive_join"),
        default="diagnostic",
    )
    create.add_argument("--runner-archive", type=Path, required=True)
    create.add_argument("--trace-json", type=Path)
    create.add_argument("--observation-health", type=Path, required=True)
    create.add_argument("--created-at", default=utc_now())
    create.add_argument("--redaction-policy", default="none")
    args = parser.parse_args(argv)
    validate_source(args.runner_archive, "runner archive")
    validate_source(args.observation_health, "observation health")
    if args.trace_json is not None:
        validate_source(args.trace_json, "trace JSON")

    build_pack(
        out_dir=args.out_dir,
        scenario_id=args.scenario_id,
        claim_summary=args.claim_summary,
        claim_class=args.claim_class,
        runner_archive=args.runner_archive,
        trace_json=args.trace_json,
        observation_health=args.observation_health,
        created_at=args.created_at,
        redaction_policy=args.redaction_policy,
    )
    print(f"wrote {args.out_dir / 'manifest.json'}")
    print(f"wrote {args.out_dir / 'summary.md'}")
    print(f"wrote {args.out_dir / 'redaction-manifest.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
