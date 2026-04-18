from __future__ import annotations

import json
import unittest
from pathlib import Path

from patterns.paused_approval import capture_paused_approval


FIXTURE_DIR = Path(__file__).resolve().parent.parent / "fixtures" / "raw"


class CapturePausedApprovalTests(unittest.TestCase):
    def test_capture_from_raw_fixture(self) -> None:
        payload = json.loads((FIXTURE_DIR / "openai_agents_js.paused_result.json").read_text())
        captured = capture_paused_approval(payload)
        self.assertEqual(captured["pause_reason"], "tool_approval")
        self.assertEqual(captured["interruptions"][0]["tool_name"], "send_email")
        self.assertEqual(captured["interruptions"][0]["call_id_ref"], "call_p23a_raw_1")
        self.assertEqual(captured["interruptions"][0]["agent_ref"], "agent:P22ApprovalProbe")
        self.assertEqual(captured["active_agent_ref"], "agent:P22ApprovalProbe")
        self.assertEqual(captured["metadata_ref"], "meta:p23a-raw-openai-agents-js")

    def test_accepts_tool_call_id_alias(self) -> None:
        captured = capture_paused_approval(
            {"timestamp": "2026-04-18T10:00:00Z", "interruptions": [{"tool_name": "send_email", "tool_call_id": "call_1"}]}
        )
        self.assertEqual(captured["interruptions"][0]["call_id_ref"], "call_1")

    def test_accepts_tool_use_id_alias(self) -> None:
        captured = capture_paused_approval(
            {"timestamp": "2026-04-18T10:00:00Z", "interruptions": [{"tool_name": "send_email", "tool_use_id": "call_2"}]}
        )
        self.assertEqual(captured["interruptions"][0]["call_id_ref"], "call_2")

    def test_accepts_call_id_alias(self) -> None:
        captured = capture_paused_approval(
            {"timestamp": "2026-04-18T10:00:00Z", "interruptions": [{"tool_name": "send_email", "call_id": "call_3"}]}
        )
        self.assertEqual(captured["interruptions"][0]["call_id_ref"], "call_3")

    def test_accepts_plain_id_alias(self) -> None:
        captured = capture_paused_approval(
            {"timestamp": "2026-04-18T10:00:00Z", "interruptions": [{"tool_name": "send_email", "id": "call_4"}]}
        )
        self.assertEqual(captured["interruptions"][0]["call_id_ref"], "call_4")

    def test_accepts_raw_item_alias(self) -> None:
        captured = capture_paused_approval(
            {
                "timestamp": "2026-04-18T10:00:00Z",
                "interruptions": [{"tool_name": "send_email", "rawItem": {"callId": "call_5"}}],
            }
        )
        self.assertEqual(captured["interruptions"][0]["call_id_ref"], "call_5")

    def test_accepts_pending_approvals_alias(self) -> None:
        captured = capture_paused_approval(
            {
                "timestamp": "2026-04-18T10:00:00Z",
                "pending_approvals": [{"toolName": "send_email", "callId": "call_6"}],
            }
        )
        self.assertEqual(captured["interruptions"][0]["call_id_ref"], "call_6")

    def test_accepts_optional_arguments_hash(self) -> None:
        captured = capture_paused_approval(
            {
                "timestamp": "2026-04-18T10:00:00Z",
                "interruptions": [
                    {
                        "tool_name": "send_email",
                        "call_id": "call_7",
                        "arguments_hash": "sha256:abc123",
                    }
                ],
            }
        )
        self.assertEqual(captured["interruptions"][0]["arguments_hash"], "sha256:abc123")

    def test_accepts_policy_extensions(self) -> None:
        captured = capture_paused_approval(
            {
                "timestamp": "2026-04-18T10:00:00Z",
                "interruptions": [{"tool_name": "send_email", "call_id": "call_8"}],
                "policy_snapshot_hash": "sha256:policy123",
                "policy_decisions": ["allow", "log_only"],
            }
        )
        self.assertEqual(captured["policy_snapshot_hash"], "sha256:policy123")
        self.assertEqual(captured["policy_decisions"], ["allow", "log_only"])

    def test_rejects_missing_interruptions(self) -> None:
        with self.assertRaisesRegex(ValueError, "interruptions must be a non-empty list"):
            capture_paused_approval({"timestamp": "2026-04-18T10:00:00Z"})

    def test_rejects_empty_interruptions(self) -> None:
        with self.assertRaisesRegex(ValueError, "interruptions must be a non-empty list"):
            capture_paused_approval({"timestamp": "2026-04-18T10:00:00Z", "interruptions": []})

    def test_rejects_missing_tool_name(self) -> None:
        with self.assertRaisesRegex(ValueError, "tool_name could not be derived"):
            capture_paused_approval({"timestamp": "2026-04-18T10:00:00Z", "interruptions": [{"call_id": "call_9"}]})

    def test_rejects_missing_call_id(self) -> None:
        with self.assertRaisesRegex(ValueError, "call_id_ref could not be derived"):
            capture_paused_approval(
                {"timestamp": "2026-04-18T10:00:00Z", "interruptions": [{"tool_name": "send_email"}]}
            )

    def test_rejects_duplicate_call_ids(self) -> None:
        with self.assertRaisesRegex(ValueError, "duplicate call_id_ref"):
            capture_paused_approval(
                {
                    "timestamp": "2026-04-18T10:00:00Z",
                    "interruptions": [
                        {"tool_name": "send_email", "call_id": "same"},
                        {"tool_name": "send_email", "call_id": "same"},
                    ],
                }
            )


if __name__ == "__main__":
    unittest.main()
