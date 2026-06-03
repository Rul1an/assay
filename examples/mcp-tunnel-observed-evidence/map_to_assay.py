"""Map one MCP tunnel observed-facts artifact into an Assay-shaped placeholder envelope."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
import re
from typing import Any


PLACEHOLDER_EVENT_TYPE = "example.placeholder.mcp-tunnel-observed"
PLACEHOLDER_SOURCE = "urn:example:assay:external:mcp:tunnel-observed"
PLACEHOLDER_PRODUCER = "assay-example"
PLACEHOLDER_PRODUCER_VERSION = "0.1.0"
PLACEHOLDER_GIT = "sample"
EXTERNAL_SCHEMA = "assay.mcp.tunnel_observed.v0"
REQUIRED_KEYS = (
    "schema",
    "artifact_id",
    "observed_at",
    "provider_context",
    "tunnel",
    "request_instance",
    "route",
    "upstream",
    "visibility",
    "non_claims",
)
OPTIONAL_KEYS = {
    "mcp",
    "auth_context",
    "control_plane",
    "inspector_event_refs",
    "evidence_refs",
    "notes",
}
TOP_LEVEL_KEYS = set(REQUIRED_KEYS) | OPTIONAL_KEYS
REQUIRED_NON_CLAIMS = {
    "agent_identity_not_verified_by_tunnel_observation",
    "authorization_not_proven_by_tunnel_observation",
    "policy_outcome_not_inferred_from_transport",
    "tool_result_truth_not_proven",
    "application_outcome_not_proven",
    "upstream_server_trust_not_proven",
    "token_freshness_not_proven",
    "observed_facts_trust_depends_on_observation_point_integrity",
    "route_facts_may_be_asserted_not_mediation_proven",
}
ALLOWED_DIRECTIONS = {
    "outbound_client_poll",
    "outbound_websocket",
    "outbound_cloudflared",
    "unknown",
}
ALLOWED_TRANSPORTS = {
    "https_long_poll",
    "websocket",
    "cloudflare_tunnel",
    "other",
    "unknown",
}
ALLOWED_PAYLOAD_MODES = {
    "not_observed",
    "digest_only",
    "redacted_projection",
}
SHA256_RE = re.compile(r"^sha256:[0-9a-f]{64}$")


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Map one MCP tunnel observed-facts artifact into an Assay-shaped placeholder envelope."
    )
    parser.add_argument("input", type=Path, help="MCP tunnel observed-facts artifact to read.")
    parser.add_argument(
        "--output",
        type=Path,
        required=True,
        help="Where to write placeholder Assay NDJSON output.",
    )
    parser.add_argument(
        "--import-time",
        default=None,
        help="RFC3339 UTC timestamp for the Assay envelope time field.",
    )
    parser.add_argument(
        "--assay-run-id",
        default=None,
        help="Optional Assay run id override. Defaults to import-mcp-tunnel-<stem>.",
    )
    parser.add_argument(
        "--overwrite",
        action="store_true",
        help="Allow overwriting the output file if it already exists.",
    )
    return parser.parse_args()


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"artifact: duplicate JSON key: {key}")
        result[key] = value
    return result


def _normalize_for_hash(value: Any) -> Any:
    if value is None or isinstance(value, (str, int, bool)):
        return value
    if isinstance(value, float):
        if not math.isfinite(value):
            raise ValueError("non-finite floats are not valid in canonical JSON")
        if value.is_integer():
            return int(value)
        raise ValueError("non-integer floats are not valid in this sample's canonical JSON subset")
    if isinstance(value, dict):
        return {str(key): _normalize_for_hash(nested) for key, nested in value.items()}
    if isinstance(value, list):
        return [_normalize_for_hash(item) for item in value]
    if isinstance(value, tuple):
        return [_normalize_for_hash(item) for item in value]
    raise TypeError(f"unsupported canonical JSON value: {type(value).__name__}")


def _canonical_json(value: Any) -> str:
    # This sample uses deterministic sorted-key JSON for a small fixture
    # corpus. It is not a full RFC 8785/JCS implementation for arbitrary JSON.
    normalized = _normalize_for_hash(value)
    return json.dumps(
        normalized,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
        allow_nan=False,
    )


def _sha256(value: Any) -> str:
    return f"sha256:{hashlib.sha256(_canonical_json(value).encode('utf-8')).hexdigest()}"


def _compute_assay_content_hash(data: dict[str, Any]) -> str:
    content_hash_input = {
        "specversion": "1.0",
        "type": PLACEHOLDER_EVENT_TYPE,
        "datacontenttype": "application/json",
        "data": data,
    }
    return _sha256(content_hash_input)


def _parse_rfc3339_datetime(value: str) -> datetime:
    normalized = value.replace("Z", "+00:00")
    try:
        parsed = datetime.fromisoformat(normalized)
    except ValueError as exc:
        raise ValueError(f"invalid RFC3339 timestamp: {value}") from exc
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc)


def _parse_rfc3339_utc(value: str | None) -> str:
    if value is None:
        return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    return _parse_rfc3339_datetime(value).isoformat().replace("+00:00", "Z")


def _validate_non_empty_string(value: Any, line_label: str, field_name: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{line_label}: {field_name} must be a non-empty string")
    return value.strip()


def _validate_object(value: Any, line_label: str, field_name: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError(f"{line_label}: {field_name} must be an object")
    return value


def _validate_sha256(value: Any, line_label: str, field_name: str) -> str:
    digest = _validate_non_empty_string(value, line_label, field_name)
    if not SHA256_RE.match(digest):
        raise ValueError(f"{line_label}: {field_name} must be sha256:<64 lowercase hex chars>")
    return digest


def _validate_string_enum(value: Any, line_label: str, field_name: str, allowed: set[str]) -> str:
    normalized = _validate_non_empty_string(value, line_label, field_name)
    if normalized not in allowed:
        joined = ", ".join(sorted(allowed))
        raise ValueError(f"{line_label}: {field_name} must be one of: {joined}")
    return normalized


def _validate_provider_context(value: Any) -> dict[str, Any]:
    context = _validate_object(value, "artifact", "provider_context")
    allowed = {"provider", "surface", "component", "component_version"}
    unknown = set(context) - allowed
    if unknown:
        raise ValueError(f"provider_context: unsupported keys: {', '.join(sorted(unknown))}")
    return {
        "provider": _validate_non_empty_string(context.get("provider"), "provider_context", "provider"),
        "surface": _validate_non_empty_string(context.get("surface"), "provider_context", "surface"),
        "component": _validate_non_empty_string(context.get("component"), "provider_context", "component"),
        "component_version": _validate_non_empty_string(
            context.get("component_version"), "provider_context", "component_version"
        ),
    }


def _validate_tunnel(value: Any) -> dict[str, Any]:
    tunnel = _validate_object(value, "artifact", "tunnel")
    allowed = {"tunnel_ref", "tunnel_ref_kind", "direction", "transport"}
    unknown = set(tunnel) - allowed
    if unknown:
        raise ValueError(f"tunnel: unsupported keys: {', '.join(sorted(unknown))}")
    return {
        "tunnel_ref": _validate_non_empty_string(tunnel.get("tunnel_ref"), "tunnel", "tunnel_ref"),
        "tunnel_ref_kind": _validate_non_empty_string(
            tunnel.get("tunnel_ref_kind"), "tunnel", "tunnel_ref_kind"
        ),
        "direction": _validate_string_enum(
            tunnel.get("direction"), "tunnel", "direction", ALLOWED_DIRECTIONS
        ),
        "transport": _validate_string_enum(
            tunnel.get("transport"), "tunnel", "transport", ALLOWED_TRANSPORTS
        ),
    }


def _validate_request_instance(value: Any) -> dict[str, Any]:
    request = _validate_object(value, "artifact", "request_instance")
    allowed = {
        "request_id",
        "request_envelope_digest",
        "request_envelope_canonicalization",
        "nonce",
    }
    unknown = set(request) - allowed
    if unknown:
        raise ValueError(f"request_instance: unsupported keys: {', '.join(sorted(unknown))}")

    normalized = {
        "request_envelope_digest": _validate_sha256(
            request.get("request_envelope_digest"),
            "request_instance",
            "request_envelope_digest",
        ),
        "request_envelope_canonicalization": _validate_non_empty_string(
            request.get("request_envelope_canonicalization"),
            "request_instance",
            "request_envelope_canonicalization",
        ),
    }
    if "request_id" in request:
        normalized["request_id"] = _validate_non_empty_string(
            request["request_id"], "request_instance", "request_id"
        )
    if "nonce" in request:
        normalized["nonce"] = _validate_non_empty_string(request["nonce"], "request_instance", "nonce")
    return normalized


def _validate_route(value: Any) -> dict[str, Any]:
    route = _validate_object(value, "artifact", "route")
    allowed = {"channel", "method", "path"}
    unknown = set(route) - allowed
    if unknown:
        raise ValueError(f"route: unsupported keys: {', '.join(sorted(unknown))}")
    return {
        "channel": _validate_non_empty_string(route.get("channel"), "route", "channel"),
        "method": _validate_non_empty_string(route.get("method"), "route", "method"),
        "path": _validate_non_empty_string(route.get("path"), "route", "path"),
    }


def _validate_upstream(value: Any) -> dict[str, Any]:
    upstream = _validate_object(value, "artifact", "upstream")
    allowed = {"target_ref", "target_kind", "target_digest"}
    unknown = set(upstream) - allowed
    if unknown:
        raise ValueError(f"upstream: unsupported keys: {', '.join(sorted(unknown))}")
    return {
        "target_ref": _validate_non_empty_string(upstream.get("target_ref"), "upstream", "target_ref"),
        "target_kind": _validate_non_empty_string(upstream.get("target_kind"), "upstream", "target_kind"),
        "target_digest": _validate_sha256(upstream.get("target_digest"), "upstream", "target_digest"),
    }


def _validate_mcp(value: Any) -> dict[str, Any]:
    mcp = _validate_object(value, "artifact", "mcp")
    allowed = {"method", "tool_name", "resource_uri_digest", "prompt_name"}
    unknown = set(mcp) - allowed
    if unknown:
        raise ValueError(f"mcp: unsupported keys: {', '.join(sorted(unknown))}")
    normalized = {"method": _validate_non_empty_string(mcp.get("method"), "mcp", "method")}
    for optional in ("tool_name", "resource_uri_digest", "prompt_name"):
        if optional in mcp:
            if optional == "resource_uri_digest":
                normalized[optional] = _validate_sha256(mcp[optional], "mcp", optional)
            else:
                normalized[optional] = _validate_non_empty_string(mcp[optional], "mcp", optional)
    return normalized


def _validate_auth_context(value: Any) -> dict[str, Any]:
    auth = _validate_object(value, "artifact", "auth_context")
    allowed = {
        "authorization_header_visible",
        "authorization_header_stored",
        "authorization_header_digest",
        "mcp_oauth_metadata_visible",
        "client_mtls_configured",
    }
    unknown = set(auth) - allowed
    if unknown:
        rawish = ", ".join(sorted(unknown))
        raise ValueError(f"auth_context: raw authorization or unsupported auth keys are not allowed: {rawish}")

    if auth.get("authorization_header_stored") is True:
        raise ValueError("auth_context: raw authorization headers must not be stored in this sample")

    normalized = {
        "authorization_header_visible": bool(auth.get("authorization_header_visible", False)),
        "authorization_header_stored": False,
        "mcp_oauth_metadata_visible": bool(auth.get("mcp_oauth_metadata_visible", False)),
        "client_mtls_configured": bool(auth.get("client_mtls_configured", False)),
    }
    if "authorization_header_digest" in auth:
        normalized["authorization_header_digest"] = _validate_sha256(
            auth["authorization_header_digest"], "auth_context", "authorization_header_digest"
        )
    return normalized


def _validate_visibility(value: Any) -> dict[str, Any]:
    visibility = _validate_object(value, "artifact", "visibility")
    required = {
        "request_payload_mode",
        "response_payload_mode",
        "tool_result_visible",
        "policy_decision_visible",
        "raw_payload_retained",
    }
    missing = required - set(visibility)
    if missing:
        raise ValueError(f"visibility: missing required keys: {', '.join(sorted(missing))}")
    unknown = set(visibility) - required
    if unknown:
        raise ValueError(f"visibility: unsupported keys: {', '.join(sorted(unknown))}")
    normalized = {
        "request_payload_mode": _validate_string_enum(
            visibility["request_payload_mode"],
            "visibility",
            "request_payload_mode",
            ALLOWED_PAYLOAD_MODES,
        ),
        "response_payload_mode": _validate_string_enum(
            visibility["response_payload_mode"],
            "visibility",
            "response_payload_mode",
            ALLOWED_PAYLOAD_MODES,
        ),
        "tool_result_visible": bool(visibility["tool_result_visible"]),
        "policy_decision_visible": bool(visibility["policy_decision_visible"]),
        "raw_payload_retained": bool(visibility["raw_payload_retained"]),
    }
    if normalized["raw_payload_retained"]:
        raise ValueError("visibility: raw_payload_retained must be false in this sample")
    return normalized


def _validate_evidence_refs(value: Any, request_instance: dict[str, Any]) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        raise ValueError("evidence_refs: must be a list")
    normalized_refs: list[dict[str, Any]] = []
    for index, ref in enumerate(value):
        label = f"evidence_refs[{index}]"
        ref_obj = _validate_object(ref, "artifact", label)
        allowed = {
            "kind",
            "digest",
            "relationship",
            "join_strength",
            "request_envelope_digest",
            "request_envelope_canonicalization",
        }
        unknown = set(ref_obj) - allowed
        if unknown:
            raise ValueError(f"{label}: unsupported keys: {', '.join(sorted(unknown))}")
        normalized = {
            "kind": _validate_non_empty_string(ref_obj.get("kind"), label, "kind"),
            "digest": _validate_sha256(ref_obj.get("digest"), label, "digest"),
            "relationship": _validate_non_empty_string(ref_obj.get("relationship"), label, "relationship"),
            "join_strength": _validate_non_empty_string(ref_obj.get("join_strength"), label, "join_strength"),
        }
        if "request_envelope_digest" in ref_obj:
            normalized["request_envelope_digest"] = _validate_sha256(
                ref_obj["request_envelope_digest"], label, "request_envelope_digest"
            )
        if "request_envelope_canonicalization" in ref_obj:
            normalized["request_envelope_canonicalization"] = _validate_non_empty_string(
                ref_obj["request_envelope_canonicalization"],
                label,
                "request_envelope_canonicalization",
            )
        if normalized["relationship"] == "same_request_instance" and normalized["join_strength"] == "strong":
            if (
                normalized.get("request_envelope_digest")
                != request_instance["request_envelope_digest"]
                or normalized.get("request_envelope_canonicalization")
                != request_instance["request_envelope_canonicalization"]
            ):
                raise ValueError(
                    f"{label}: same_request_instance strong joins require matching "
                    "request_envelope_digest and request_envelope_canonicalization"
                )
        normalized_refs.append(normalized)
    return normalized_refs


def _validate_non_claims(value: Any) -> list[str]:
    if not isinstance(value, list) or not value:
        raise ValueError("non_claims: must be a non-empty list")
    normalized = [
        _validate_non_empty_string(item, "non_claims", f"non_claims[{index}]")
        for index, item in enumerate(value)
    ]
    missing = REQUIRED_NON_CLAIMS - set(normalized)
    if missing:
        raise ValueError(f"non_claims: missing required values: {', '.join(sorted(missing))}")
    return normalized


def _normalized_record(record: dict[str, Any]) -> dict[str, Any]:
    missing = [key for key in REQUIRED_KEYS if key not in record]
    if missing:
        raise ValueError(f"artifact: missing required keys: {', '.join(missing)}")

    unknown = set(record) - TOP_LEVEL_KEYS
    if unknown:
        raise ValueError(f"artifact: unsupported top-level keys: {', '.join(sorted(unknown))}")
    if record["schema"] != EXTERNAL_SCHEMA:
        raise ValueError(f"artifact: expected schema {EXTERNAL_SCHEMA}, got {record['schema']}")

    observed_at = _parse_rfc3339_datetime(str(record["observed_at"])).isoformat().replace("+00:00", "Z")
    request_instance = _validate_request_instance(record["request_instance"])
    normalized = {
        "schema": EXTERNAL_SCHEMA,
        "artifact_id": _validate_non_empty_string(record["artifact_id"], "artifact", "artifact_id"),
        "observed_at": observed_at,
        "provider_context": _validate_provider_context(record["provider_context"]),
        "tunnel": _validate_tunnel(record["tunnel"]),
        "request_instance": request_instance,
        "route": _validate_route(record["route"]),
        "upstream": _validate_upstream(record["upstream"]),
        "visibility": _validate_visibility(record["visibility"]),
        "non_claims": _validate_non_claims(record["non_claims"]),
    }
    if "mcp" in record:
        normalized["mcp"] = _validate_mcp(record["mcp"])
    if "auth_context" in record:
        normalized["auth_context"] = _validate_auth_context(record["auth_context"])
    if "control_plane" in record:
        normalized["control_plane"] = _validate_object(record["control_plane"], "artifact", "control_plane")
    if "inspector_event_refs" in record:
        if not isinstance(record["inspector_event_refs"], list):
            raise ValueError("inspector_event_refs: must be a list")
        normalized["inspector_event_refs"] = record["inspector_event_refs"]
    if "evidence_refs" in record:
        normalized["evidence_refs"] = _validate_evidence_refs(record["evidence_refs"], request_instance)
    if "notes" in record:
        normalized["notes"] = _validate_non_empty_string(record["notes"], "artifact", "notes")
    return normalized


def map_record(record: dict[str, Any], assay_run_id: str, import_time: str) -> dict[str, Any]:
    normalized = _normalized_record(record)
    request_instance = normalized["request_instance"]
    data = {
        "external_system": "mcp_tunnel",
        "external_surface": "tunnel-observed-facts-artifact",
        "external_schema": EXTERNAL_SCHEMA,
        "observed_upstream_time": normalized["observed_at"],
        "request_binding": {
            "request_envelope_digest": request_instance["request_envelope_digest"],
            "request_envelope_canonicalization": request_instance["request_envelope_canonicalization"],
        },
        "join_guidance": {
            "same_request_instance": (
                "strong only when both artifacts bind the same request_envelope_digest "
                "and request_envelope_canonicalization"
            ),
            "diagnostic_correlation_when": [
                "json_rpc_id_only",
                "timestamp_only",
                "route_label_only",
                "provider_request_id_only",
            ],
        },
        "observed": normalized,
    }
    event = {
        "specversion": "1.0",
        "type": PLACEHOLDER_EVENT_TYPE,
        "source": PLACEHOLDER_SOURCE,
        "id": f"{assay_run_id}:0",
        "time": _parse_rfc3339_utc(import_time),
        "datacontenttype": "application/json",
        "assayrunid": assay_run_id,
        "assayseq": 0,
        "assayproducer": PLACEHOLDER_PRODUCER,
        "assayproducerversion": PLACEHOLDER_PRODUCER_VERSION,
        "assaygit": PLACEHOLDER_GIT,
        "assaypii": False,
        "assaysecrets": False,
        "data": data,
    }
    event["assaycontenthash"] = _compute_assay_content_hash(data)
    return event


def main() -> int:
    args = _parse_args()
    if args.output.exists() and not args.overwrite:
        raise SystemExit(f"{args.output} already exists; pass --overwrite to replace it")

    try:
        with args.input.open("r", encoding="utf-8") as handle:
            record = json.load(handle, object_pairs_hook=_reject_duplicate_keys)
    except (OSError, json.JSONDecodeError, ValueError) as exc:
        raise SystemExit(str(exc)) from exc

    if not isinstance(record, dict):
        raise SystemExit("artifact: top-level JSON value must be an object")

    try:
        import_time = _parse_rfc3339_utc(args.import_time)
        assay_run_id = args.assay_run_id or f"import-mcp-tunnel-{args.input.stem}"
        event = map_record(record, assay_run_id=assay_run_id, import_time=import_time)
    except ValueError as exc:
        raise SystemExit(str(exc)) from exc

    try:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with args.output.open("w", encoding="utf-8") as handle:
            handle.write(_canonical_json(event))
            handle.write("\n")
    except OSError as exc:
        raise SystemExit(str(exc)) from exc

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
