from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from patterns.paused_approval import validate_pause_artifact


FIXTURE_DIR = Path(__file__).resolve().parent.parent / "fixtures"
MAPPER = (
    Path(__file__).resolve().parents[3]
    / "examples"
    / "openai-agents-js-approval-interruption-evidence"
    / "map_to_assay.py"
)


def _load_fixture(name: str) -> dict[str, object]:
    return json.loads((FIXTURE_DIR / name).read_text())


class PausedApprovalCorpusTests(unittest.TestCase):
    def _run_mapper(self, fixture_name: str) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as temp_dir:
            output_path = Path(temp_dir) / "artifact.ndjson"
            return subprocess.run(
                [
                    sys.executable,
                    str(MAPPER),
                    str(FIXTURE_DIR / fixture_name),
                    "--output",
                    str(output_path),
                    "--import-time",
                    "2026-04-18T10:30:00Z",
                    "--overwrite",
                ],
                capture_output=True,
                text=True,
                check=False,
            )

    def test_valid_fixture_passes_validator_and_mapper(self) -> None:
        normalized = validate_pause_artifact(_load_fixture("valid.paused.json"))
        self.assertEqual(normalized["framework"], "openai_agents_js")
        result = self._run_mapper("valid.paused.json")
        self.assertEqual(result.returncode, 0, result.stderr or result.stdout)

    def test_failure_fixture_passes_validator_and_mapper(self) -> None:
        normalized = validate_pause_artifact(_load_fixture("failure.paused.json"))
        self.assertEqual(normalized["pause_reason"], "tool_approval")
        result = self._run_mapper("failure.paused.json")
        self.assertEqual(result.returncode, 0, result.stderr or result.stdout)

    def test_history_fixture_fails_validator_and_mapper(self) -> None:
        fixture = _load_fixture("malformed.history.paused.json")
        with self.assertRaisesRegex(ValueError, "history is out of scope"):
            validate_pause_artifact(fixture)
        result = self._run_mapper("malformed.history.paused.json")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("history is out of scope", result.stderr or result.stdout)

    def test_raw_state_fixture_fails_validator_and_mapper(self) -> None:
        fixture = _load_fixture("malformed.raw_state.paused.json")
        with self.assertRaisesRegex(ValueError, "raw serialized state"):
            validate_pause_artifact(fixture)
        result = self._run_mapper("malformed.raw_state.paused.json")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("raw serialized state", result.stderr or result.stdout)

    def test_resolved_decision_fixture_fails_validator_and_mapper(self) -> None:
        fixture = _load_fixture("malformed.resolved.paused.json")
        with self.assertRaisesRegex(ValueError, "resolved approval decision"):
            validate_pause_artifact(fixture)
        result = self._run_mapper("malformed.resolved.paused.json")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("resolved approval decision", result.stderr or result.stdout)


if __name__ == "__main__":
    unittest.main()
