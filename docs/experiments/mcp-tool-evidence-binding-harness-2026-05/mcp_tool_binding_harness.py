#!/usr/bin/env python3
"""Generate synthetic MCP tool evidence-binding rows."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any

SCHEMA = "assay.experiment.mcp_tool_evidence_binding.binding_cell.v0"

STARTER_SCENARIOS = (
    "benign_tool_call_bound",
    "description_changed_before_call",
    "effect_outside_declared_tool_boundary",
    "description_visible_no_call",
    "call_made_no_measurable_effect",
    "call_made_with_other_descriptions_visible",
)


def canonical_json(payload: Any) -> str:
    return json.dumps(payload, sort_keys=True, separators=(",", ":"))


def digest(payload: Any) -> str:
    return "sha256:" + hashlib.sha256(canonical_json(payload).encode()).hexdigest()


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def git_commit() -> str:
    try:
        return subprocess.check_output(
            ["git", "rev-parse", "HEAD"],
            cwd=Path(__file__).resolve().parents[3],
            text=True,
        ).strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


@dataclass(frozen=True)
class ToolDefinition:
    name: str
    manifest_description: str
    visible_description: str
    declared_boundary: list[str]
    source_surface: str = "synthetic_mcp_tools_list"

    def manifest(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "description": self.manifest_description,
            "declared_boundary": self.declared_boundary,
        }

    def visible_record(self, index: int) -> dict[str, Any]:
        return {
            "tool_name": self.name,
            "ref": f"context-descriptor-set.json#/tools/{index}",
            "source_surface": self.source_surface,
            "digest": digest(
                {
                    "tool_name": self.name,
                    "visible_description": self.visible_description,
                    "declared_boundary": self.declared_boundary,
                }
            ),
            "declared_boundary": self.declared_boundary,
        }


READ_FILE = ToolDefinition(
    name="read_file",
    manifest_description="Read a file under /workspace/allowed.",
    visible_description="Read a file under /workspace/allowed.",
    declared_boundary=["filesystem_read:/workspace/allowed/*"],
)

READ_FILE_DRIFTED = ToolDefinition(
    name="read_file",
    manifest_description="Read a file under /workspace/allowed.",
    visible_description="Read any file path provided by the user.",
    declared_boundary=["filesystem_read:/workspace/allowed/*"],
)

WRITE_FILE = ToolDefinition(
    name="write_file",
    manifest_description="Write generated output under /workspace/out.",
    visible_description="Write generated output under /workspace/out.",
    declared_boundary=["filesystem_write:/workspace/out/*"],
)


def context_descriptor_set(tools: list[ToolDefinition]) -> dict[str, Any]:
    return {
        "kind": "synthetic_mcp_context_descriptor_set",
        "tools": [
            {
                "name": tool.name,
                "visible_description": tool.visible_description,
                "declared_boundary": tool.declared_boundary,
                "source_surface": tool.source_surface,
            }
            for tool in tools
        ],
    }


def tool_call(tool: ToolDefinition, *, call_id: str = "mcp_call_001") -> dict[str, Any]:
    return {
        "kind": "synthetic_mcp_tool_call",
        "tool_call_id": call_id,
        "tool_name": tool.name,
        "arguments_digest": digest({"path": "/workspace/allowed/safe.txt"}),
    }


def measured_effect(kind: str, path: str, *, status: str = "success") -> dict[str, Any]:
    return {
        "kind": "synthetic_measured_runtime_effect",
        "effect_kind": kind,
        "path": path,
        "status": status,
    }


def tunnel_context() -> dict[str, Any]:
    return {
        "kind": "synthetic_mcp_tunnel_transport_context",
        "transport_profile": "mcp_tunnel_synthetic",
        "connection_direction": "outbound_only",
        "proxy_role": "routes_to_private_mcp_server",
        "payload_tls_boundary": "inner_tls_to_proxy",
        "upstream_authentication": "independent_oauth_or_bearer_token",
        "metadata_visible_to_transport_provider": [
            "egress_ip",
            "host_fingerprint",
            "connection_timing",
            "byte_volume",
            "tunnel_subdomain",
        ],
        "transport_claim": "transport_context_only",
    }


def base_non_claims(*extra: str) -> list[str]:
    return [
        "does_not_detect_tool_poisoning",
        "does_not_classify_malicious_intent",
        "does_not_rank_mcp_clients_servers_or_providers",
        "does_not_define_mcp_spec_changes",
        "does_not_promote_experiment_schema_to_product_api",
        *extra,
    ]


def scenario_inputs(scenario_id: str) -> dict[str, Any]:
    if scenario_id == "benign_tool_call_bound":
        return {
            "role": "baseline",
            "tools": [READ_FILE],
            "called_tool": READ_FILE,
            "effect": measured_effect("filesystem_read", "/workspace/allowed/safe.txt"),
            "claim_outcome": "bound_tool_evidence",
            "transport_profile": "mcp_tunnel_synthetic",
            "notes": [
                "Visible description, called tool, and measured effect align inside the declared boundary.",
                "Tunnel transport metadata is retained as context only and does not authenticate or explain the tool call.",
            ],
            "non_claims": base_non_claims(
                "does_not_treat_tunnel_routing_as_tool_intent",
                "does_not_claim_tunnel_authenticates_upstream_mcp_server",
            ),
        }
    if scenario_id == "description_changed_before_call":
        return {
            "role": "drift",
            "tools": [READ_FILE_DRIFTED],
            "called_tool": READ_FILE_DRIFTED,
            "effect": measured_effect("filesystem_read", "/workspace/allowed/safe.txt"),
            "claim_outcome": "description_drift",
            "transport_profile": "local_synthetic",
            "notes": [
                "The called tool manifest digest differs from the model-visible description digest before the call.",
            ],
            "non_claims": base_non_claims("does_not_claim_the_drift_was_malicious"),
        }
    if scenario_id == "effect_outside_declared_tool_boundary":
        return {
            "role": "gap",
            "tools": [READ_FILE],
            "called_tool": READ_FILE,
            "effect": measured_effect("filesystem_write", "/workspace/outside/hidden.txt"),
            "claim_outcome": "effect_outside_declared_tool_boundary",
            "transport_profile": "local_synthetic",
            "notes": [
                "The measured write exceeds the visible read-only boundary without proving malicious intent.",
            ],
            "non_claims": base_non_claims(
                "does_not_claim_policy_failure",
                "does_not_claim_root_cause",
            ),
        }
    if scenario_id == "description_visible_no_call":
        return {
            "role": "absence_boundary",
            "tools": [READ_FILE],
            "called_tool": None,
            "effect": None,
            "claim_outcome": "diagnostic_only",
            "transport_profile": "local_synthetic",
            "notes": [
                "A tool definition is visible, but no call to that tool is observed inside the bounded call surface.",
            ],
            "non_claims": base_non_claims("does_not_claim_no_runtime_activity_occurred"),
        }
    if scenario_id == "call_made_no_measurable_effect":
        return {
            "role": "effect_boundary",
            "tools": [READ_FILE],
            "called_tool": READ_FILE,
            "effect": None,
            "claim_outcome": "inconclusive",
            "transport_profile": "local_synthetic",
            "effect_capture_status": "unavailable",
            "notes": [
                "The call exists, but the measured-effect layer is unavailable, so the chain cannot support an effect claim.",
            ],
            "non_claims": base_non_claims("does_not_claim_absence_of_effect"),
        }
    if scenario_id == "call_made_with_other_descriptions_visible":
        return {
            "role": "context_boundary",
            "tools": [READ_FILE, WRITE_FILE],
            "called_tool": READ_FILE,
            "effect": measured_effect("filesystem_read", "/workspace/allowed/safe.txt"),
            "claim_outcome": "call_isolated_in_visible_context",
            "transport_profile": "local_synthetic",
            "notes": [
                "The called tool is bound while preserving the complete co-visible tool description set.",
                "The row records co-visibility without claiming causation between other visible descriptions and the call.",
            ],
            "non_claims": base_non_claims("does_not_claim_co_visible_description_caused_call"),
        }
    raise KeyError(f"unknown scenario: {scenario_id}")


def binding_cell(scenario_id: str, *, assay_commit: str, created_at: str) -> tuple[dict[str, Any], dict[str, Any]]:
    inputs = scenario_inputs(scenario_id)
    tools: list[ToolDefinition] = inputs["tools"]
    context = context_descriptor_set(tools)
    called_tool: ToolDefinition | None = inputs["called_tool"]
    call = tool_call(called_tool) if called_tool is not None else None
    effect = inputs["effect"]
    transport_profile = inputs["transport_profile"]
    transport = tunnel_context() if transport_profile == "mcp_tunnel_synthetic" else None
    visible_records = [tool.visible_record(index) for index, tool in enumerate(tools)]
    called_visible = (
        next(record for record in visible_records if record["tool_name"] == called_tool.name)
        if called_tool is not None
        else None
    )
    manifest_digest = digest(called_tool.manifest()) if called_tool is not None else None
    description_digest = called_visible["digest"] if called_visible is not None else None
    description_matches_manifest = (
        called_tool.manifest_description == called_tool.visible_description
        if called_tool is not None
        else None
    )
    call_observed = call is not None
    effect_capture_status = inputs.get(
        "effect_capture_status",
        "observed" if effect is not None else "unobserved",
    )
    effect_within_boundary = None
    if effect is not None and called_tool is not None:
        effect_within_boundary = effect["path"].startswith("/workspace/allowed/")
    required_links_complete = (
        call_observed
        and called_visible is not None
        and effect_capture_status == "observed"
        and effect is not None
    )
    if inputs["claim_outcome"] in {"diagnostic_only", "call_isolated_in_visible_context"}:
        required_links_complete = True
    if inputs["claim_outcome"] == "description_drift":
        required_links_complete = True

    cell = {
        "schema": SCHEMA,
        "scenario_id": scenario_id,
        "role": inputs["role"],
        "claim_outcome": inputs["claim_outcome"],
        "created_at": created_at,
        "assay_commit": assay_commit,
        "context_descriptor_set_ref": "context-descriptor-set.json",
        "context_descriptor_set_digest": digest(context),
        "model_visible_tool_description_refs": visible_records,
        "co_visible_tool_names": [tool.name for tool in tools],
        "called_tool_name": called_tool.name if called_tool is not None else None,
        "called_tool_manifest_digest": manifest_digest,
        "called_tool_description_digest": description_digest,
        "tool_call_ref": "tool-call.json" if call is not None else None,
        "tool_call_id": call["tool_call_id"] if call is not None else None,
        "call_observed": call_observed,
        "measured_effect_ref": "measured-effect.json" if effect is not None else None,
        "measured_effect_kind": effect["effect_kind"] if effect is not None else "none",
        "effect_capture_status": effect_capture_status,
        "effect_within_declared_boundary": effect_within_boundary,
        "description_matches_manifest": description_matches_manifest,
        "required_links_complete": required_links_complete,
        "join_key": "tool_call_id" if call is not None and effect is not None else "none",
        "join_grade": "strong" if call is not None and effect is not None else "diagnostic",
        "transport_profile": transport_profile,
        "transport_context": transport,
        "mapping_notes": inputs["notes"],
        "non_claims": inputs["non_claims"],
    }
    artifacts = {
        "context": context,
        "call": call,
        "effect": effect,
        "transport": transport,
    }
    return cell, artifacts


def summary_for(cell: dict[str, Any]) -> str:
    return "\n".join(
        [
            f"# {cell['scenario_id']}",
            "",
            f"- Claim outcome: `{cell['claim_outcome']}`",
            f"- Visible tools: {', '.join(cell['co_visible_tool_names'])}",
            f"- Called tool: `{cell['called_tool_name']}`",
            f"- Effect capture: `{cell['effect_capture_status']}`",
            f"- Transport profile: `{cell['transport_profile']}`",
            "",
            "## Non-Claims",
            "",
            *[f"- `{claim}`" for claim in cell["non_claims"]],
            "",
        ]
    )


def generate_harness(
    *,
    out_dir: Path,
    scenarios: list[str],
    assay_commit: str | None = None,
    created_at: str = "2026-05-29T00:00:00Z",
) -> None:
    if out_dir.exists() and any(out_dir.iterdir()):
        raise SystemExit(f"refusing to write into non-empty directory: {out_dir}")
    out_dir.mkdir(parents=True, exist_ok=True)
    commit = assay_commit or git_commit()
    for scenario_id in scenarios:
        cell, artifacts = binding_cell(
            scenario_id,
            assay_commit=commit,
            created_at=created_at,
        )
        scenario_dir = out_dir / scenario_id
        write_json(scenario_dir / "binding-cell.json", cell)
        write_json(scenario_dir / "context-descriptor-set.json", artifacts["context"])
        if artifacts["call"] is not None:
            write_json(scenario_dir / "tool-call.json", artifacts["call"])
        if artifacts["effect"] is not None:
            write_json(scenario_dir / "measured-effect.json", artifacts["effect"])
        if artifacts["transport"] is not None:
            write_json(scenario_dir / "transport-context.json", artifacts["transport"])
        (scenario_dir / "summary.md").write_text(summary_for(cell), encoding="utf-8")
        print(f"wrote {scenario_dir}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out-dir", required=True, type=Path)
    parser.add_argument("--scenario", action="append", choices=STARTER_SCENARIOS)
    parser.add_argument("--assay-commit")
    parser.add_argument("--created-at", default="2026-05-29T00:00:00Z")
    args = parser.parse_args(argv)
    generate_harness(
        out_dir=args.out_dir,
        scenarios=args.scenario or list(STARTER_SCENARIOS),
        assay_commit=args.assay_commit,
        created_at=args.created_at,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
