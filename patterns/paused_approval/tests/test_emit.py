from __future__ import annotations

import unittest

from patterns.paused_approval import emit_pause_artifact


class EmitPauseArtifactTests(unittest.TestCase):
    def setUp(self) -> None:
        self.captured = {
            "timestamp": "2026-04-18T10:00:00Z",
            "pause_reason": "tool_approval",
            "interruptions": [{"tool_name": "send_email", "call_id_ref": "call_1"}],
        }

    def test_emit_minimal_artifact(self) -> None:
        artifact = emit_pause_artifact(
            self.captured,
            framework="openai_agents_js",
            resume_state_ref="runstate:sha256:abc",
        )
        self.assertEqual(artifact["schema"], "assay.harness.paused-approval.v1")
        self.assertEqual(artifact["surface"], "approval_interruption")

    def test_emit_carries_optional_fields(self) -> None:
        artifact = emit_pause_artifact(
            {
                **self.captured,
                "active_agent_ref": "agent:active",
                "metadata_ref": "meta:1",
                "policy_snapshot_hash": "sha256:policy",
                "policy_decisions": ["allow"],
            },
            framework="openai_agents_js",
            resume_state_ref="runstate:sha256:def",
        )
        self.assertEqual(artifact["active_agent_ref"], "agent:active")
        self.assertEqual(artifact["policy_decisions"], ["allow"])

    def test_emit_uses_custom_schema_and_surface(self) -> None:
        artifact = emit_pause_artifact(
            self.captured,
            framework="openai_agents_js",
            resume_state_ref="runstate:sha256:ghi",
            schema="custom.schema.v1",
            surface="tool_approval_interruption_resumable_state",
        )
        self.assertEqual(artifact["schema"], "custom.schema.v1")
        self.assertEqual(artifact["surface"], "tool_approval_interruption_resumable_state")

    def test_emit_rejects_bad_framework(self) -> None:
        with self.assertRaisesRegex(ValueError, "framework"):
            emit_pause_artifact(self.captured, framework="OpenAI", resume_state_ref="runstate:sha256:jkl")

    def test_emit_can_skip_validation(self) -> None:
        artifact = emit_pause_artifact(
            self.captured,
            framework="OpenAI",
            resume_state_ref="https://bad.example/ref",
            validate=False,
        )
        self.assertEqual(artifact["framework"], "OpenAI")

    def test_emit_requires_resume_state_ref(self) -> None:
        with self.assertRaises(TypeError):
            emit_pause_artifact(self.captured, framework="openai_agents_js")  # type: ignore[call-arg]


if __name__ == "__main__":
    unittest.main()
