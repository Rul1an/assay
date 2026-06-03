from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest


MODULE_PATH = Path(__file__).with_name("map_to_assay.py")


def _load_mapper():
    spec = importlib.util.spec_from_file_location("mcp_tunnel_map_to_assay", MODULE_PATH)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _valid_artifact() -> dict:
    return {
        "schema": "assay.mcp.tunnel_observed.v0",
        "artifact_id": "mcp-tunnel-observed-001",
        "observed_at": "2026-06-03T16:00:00Z",
        "provider_context": {
            "provider": "example",
            "surface": "mcp_tunnel",
            "component": "sample-tunnel-client",
            "component_version": "0.0.0",
        },
        "tunnel": {
            "tunnel_ref": "tunnel_redacted",
            "tunnel_ref_kind": "provider_id",
            "direction": "outbound_client_poll",
            "transport": "https_long_poll",
        },
        "request_instance": {
            "request_id": "req-001",
            "request_envelope_digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "request_envelope_canonicalization": "jcs:mcp_request_envelope.v1",
            "nonce": "n-001",
        },
        "route": {
            "channel": "main",
            "method": "tools/call",
            "path": "/mcp",
        },
        "upstream": {
            "target_ref": "local-stdio",
            "target_kind": "stdio",
            "target_digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
        },
        "mcp": {
            "method": "tools/call",
            "tool_name": "deploy_service",
        },
        "auth_context": {
            "authorization_header_visible": True,
            "authorization_header_stored": False,
            "authorization_header_digest": "sha256:3333333333333333333333333333333333333333333333333333333333333333",
            "mcp_oauth_metadata_visible": True,
            "client_mtls_configured": False,
        },
        "visibility": {
            "request_payload_mode": "digest_only",
            "response_payload_mode": "not_observed",
            "tool_result_visible": False,
            "policy_decision_visible": False,
            "raw_payload_retained": False,
        },
        "evidence_refs": [
            {
                "kind": "mcp.execution_record",
                "digest": "sha256:4444444444444444444444444444444444444444444444444444444444444444",
                "relationship": "same_request_instance",
                "join_strength": "strong",
                "request_envelope_digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
                "request_envelope_canonicalization": "jcs:mcp_request_envelope.v1",
            }
        ],
        "non_claims": [
            "agent_identity_not_verified_by_tunnel_observation",
            "authorization_not_proven_by_tunnel_observation",
            "policy_outcome_not_inferred_from_transport",
            "tool_result_truth_not_proven",
            "application_outcome_not_proven",
            "upstream_server_trust_not_proven",
            "token_freshness_not_proven",
            "observed_facts_trust_depends_on_observation_point_integrity",
            "route_facts_may_be_asserted_not_mediation_proven",
        ],
    }


class McpTunnelObservedMapperTest(unittest.TestCase):
    def test_maps_valid_artifact_without_folding_route_into_instance_binding(self):
        mapper = _load_mapper()

        event = mapper.map_record(
            _valid_artifact(),
            assay_run_id="import-mcp-tunnel-valid",
            import_time="2026-06-03T18:00:00Z",
        )

        observed = event["data"]["observed"]
        request_binding = event["data"]["request_binding"]

        self.assertEqual(event["type"], "example.placeholder.mcp-tunnel-observed")
        self.assertEqual(observed["route"]["channel"], "main")
        self.assertEqual(observed["upstream"]["target_kind"], "stdio")
        self.assertEqual(
            request_binding,
            {
                "request_envelope_digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
                "request_envelope_canonicalization": "jcs:mcp_request_envelope.v1",
            },
        )
        self.assertNotIn("route", request_binding)
        self.assertNotIn("upstream", request_binding)
        self.assertIn("route_facts_may_be_asserted_not_mediation_proven", observed["non_claims"])

    def test_rejects_raw_authorization_leak_even_when_digest_is_present(self):
        mapper = _load_mapper()
        artifact = _valid_artifact()
        artifact["auth_context"]["authorization_header_stored"] = True
        artifact["auth_context"]["authorization_header_raw"] = "Bearer should-not-ship"

        with self.assertRaisesRegex(ValueError, "raw authorization"):
            mapper.map_record(
                artifact,
                assay_run_id="import-mcp-tunnel-leak",
                import_time="2026-06-03T18:05:00Z",
            )

    def test_rejects_strong_same_request_join_with_mismatched_canonicalization(self):
        mapper = _load_mapper()
        artifact = _valid_artifact()
        artifact["evidence_refs"][0]["request_envelope_canonicalization"] = (
            "json:mcp_request_envelope.unstable"
        )

        with self.assertRaisesRegex(ValueError, "same_request_instance strong joins require matching"):
            mapper.map_record(
                artifact,
                assay_run_id="import-mcp-tunnel-bad-join",
                import_time="2026-06-03T18:10:00Z",
            )

    def test_rejects_string_boolean_values(self):
        mapper = _load_mapper()
        artifact = _valid_artifact()
        artifact["visibility"]["tool_result_visible"] = "false"

        with self.assertRaisesRegex(ValueError, "must be a JSON boolean"):
            mapper.map_record(
                artifact,
                assay_run_id="import-mcp-tunnel-string-bool",
                import_time="2026-06-03T18:15:00Z",
            )

    def test_rejects_payload_like_inspector_event_refs(self):
        mapper = _load_mapper()
        artifact = _valid_artifact()
        artifact["inspector_event_refs"] = [
            {
                "kind": "mcp.inspector_event",
                "digest": "sha256:5555555555555555555555555555555555555555555555555555555555555555",
                "raw_payload": {"method": "tools/call", "params": {"secret": "not here"}},
            }
        ]

        with self.assertRaisesRegex(ValueError, "unsupported keys"):
            mapper.map_record(
                artifact,
                assay_run_id="import-mcp-tunnel-bad-inspector-ref",
                import_time="2026-06-03T18:20:00Z",
            )

    def test_accepts_bounded_inspector_event_refs(self):
        mapper = _load_mapper()
        artifact = _valid_artifact()
        artifact["inspector_event_refs"] = [
            {
                "kind": "mcp.inspector_event",
                "digest": "sha256:5555555555555555555555555555555555555555555555555555555555555555",
                "ref": "inspector-event-001",
            }
        ]

        event = mapper.map_record(
            artifact,
            assay_run_id="import-mcp-tunnel-inspector-ref",
            import_time="2026-06-03T18:25:00Z",
        )

        self.assertEqual(
            event["data"]["observed"]["inspector_event_refs"],
            [
                {
                    "kind": "mcp.inspector_event",
                    "digest": "sha256:5555555555555555555555555555555555555555555555555555555555555555",
                    "ref": "inspector-event-001",
                }
            ],
        )


if __name__ == "__main__":
    unittest.main()
