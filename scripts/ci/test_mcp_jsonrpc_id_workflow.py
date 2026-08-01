#!/usr/bin/env python3
"""Contract tests for immutable replay and mutable-source drift lanes."""

from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]
IMMUTABLE = ROOT / ".github/workflows/mcp-2026-jsonrpc-id-conformance.yml"
DRIFT = ROOT / ".github/workflows/mcp-2026-jsonrpc-id-drift.yml"


class McpJsonRpcIdWorkflowContractTest(unittest.TestCase):
    def test_pull_request_lane_is_offline_with_respect_to_upstream_subjects(self):
        workflow = IMMUTABLE.read_text(encoding="utf-8")

        self.assertIn("pull_request:", workflow)
        self.assertIn("verify-committed", workflow)
        self.assertNotIn("curl ", workflow)
        self.assertNotIn("https://", workflow)
        self.assertNotIn("modelcontextprotocol.io", workflow)
        self.assertNotIn("jsonrpc.org", workflow)

    def test_live_drift_lane_is_scheduled_manual_and_bounded(self):
        workflow = DRIFT.read_text(encoding="utf-8")

        self.assertIn("schedule:", workflow)
        self.assertIn("workflow_dispatch:", workflow)
        self.assertNotIn("pull_request:", workflow)
        self.assertNotIn("push:", workflow)
        self.assertIn("--max-time", workflow)
        self.assertIn("--max-filesize", workflow)
        self.assertIn("live-drift", workflow)


if __name__ == "__main__":
    unittest.main()
