from __future__ import annotations

import unittest

from patterns.paused_approval import validate_pause_artifact


def _valid_artifact() -> dict[str, object]:
    return {
        "schema": "assay.harness.paused-approval.v1",
        "framework": "openai_agents_js",
        "surface": "approval_interruption",
        "timestamp": "2026-04-18T10:00:00Z",
        "pause_reason": "tool_approval",
        "interruptions": [{"tool_name": "send_email", "call_id_ref": "call_1"}],
        "resume_state_ref": "runstate:sha256:abc123",
    }


class ValidatePauseArtifactTests(unittest.TestCase):
    def test_valid_artifact_passes(self) -> None:
        normalized = validate_pause_artifact(_valid_artifact())
        self.assertEqual(normalized["pause_reason"], "tool_approval")

    def test_optional_fields_pass(self) -> None:
        artifact = _valid_artifact()
        artifact["active_agent_ref"] = "agent:active"
        artifact["last_agent_ref"] = "agent:last"
        artifact["metadata_ref"] = "meta:1"
        normalized = validate_pause_artifact(artifact)
        self.assertEqual(normalized["active_agent_ref"], "agent:active")

    def test_tolerated_extensions_pass(self) -> None:
        artifact = _valid_artifact()
        artifact["policy_snapshot_hash"] = "sha256:policy"
        artifact["policy_decisions"] = ["allow", "log_only"]
        artifact["interruptions"] = [
            {"tool_name": "send_email", "call_id_ref": "call_1", "arguments_hash": "sha256:args"}
        ]
        normalized = validate_pause_artifact(artifact)
        self.assertEqual(normalized["policy_decisions"], ["allow", "log_only"])
        self.assertEqual(normalized["interruptions"][0]["arguments_hash"], "sha256:args")

    def test_rejects_history(self) -> None:
        artifact = _valid_artifact()
        artifact["history"] = []
        with self.assertRaisesRegex(ValueError, "history is out of scope"):
            validate_pause_artifact(artifact)

    def test_rejects_session(self) -> None:
        artifact = _valid_artifact()
        artifact["session"] = "sess_1"
        with self.assertRaisesRegex(ValueError, "session is out of scope"):
            validate_pause_artifact(artifact)

    def test_rejects_new_items_spelling(self) -> None:
        artifact = _valid_artifact()
        artifact["new_items"] = []
        with self.assertRaisesRegex(ValueError, "newItems/new_items"):
            validate_pause_artifact(artifact)

    def test_rejects_raw_state(self) -> None:
        artifact = _valid_artifact()
        artifact["state"] = "{}"
        with self.assertRaisesRegex(ValueError, "raw serialized state"):
            validate_pause_artifact(artifact)

    def test_rejects_resolved_decision(self) -> None:
        artifact = _valid_artifact()
        artifact["approved"] = True
        with self.assertRaisesRegex(ValueError, "resolved approval decision"):
            validate_pause_artifact(artifact)

    def test_rejects_url_resume_state_ref(self) -> None:
        artifact = _valid_artifact()
        artifact["resume_state_ref"] = "https://example.com/state"
        with self.assertRaisesRegex(ValueError, "opaque id, not a URL"):
            validate_pause_artifact(artifact)

    def test_rejects_empty_interruptions(self) -> None:
        artifact = _valid_artifact()
        artifact["interruptions"] = []
        with self.assertRaisesRegex(ValueError, "non-empty array"):
            validate_pause_artifact(artifact)

    def test_rejects_bad_pause_reason(self) -> None:
        artifact = _valid_artifact()
        artifact["pause_reason"] = "needs_human"
        with self.assertRaisesRegex(ValueError, "pause_reason must be tool_approval"):
            validate_pause_artifact(artifact)

    def test_rejects_unknown_interruption_key(self) -> None:
        artifact = _valid_artifact()
        artifact["interruptions"] = [{"tool_name": "send_email", "call_id_ref": "call_1", "arguments": {}}]
        with self.assertRaisesRegex(ValueError, "unsupported keys: arguments"):
            validate_pause_artifact(artifact)

    def test_rejects_duplicate_call_id_ref(self) -> None:
        artifact = _valid_artifact()
        artifact["interruptions"] = [
            {"tool_name": "send_email", "call_id_ref": "same"},
            {"tool_name": "send_email", "call_id_ref": "same"},
        ]
        with self.assertRaisesRegex(ValueError, "duplicate call_id_ref"):
            validate_pause_artifact(artifact)


if __name__ == "__main__":
    unittest.main()
