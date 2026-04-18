from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from patterns.paused_approval import validate_pause_artifact


ROOT = Path(__file__).resolve().parents[3]
RAW_RESULT = ROOT / "patterns" / "paused_approval" / "fixtures" / "raw" / "openai_agents_js.paused_result.json"
RAW_STATE = ROOT / "patterns" / "paused_approval" / "fixtures" / "raw" / "openai_agents_js.serialized_state.txt"
SMOKE = ROOT / "patterns" / "paused_approval" / "smoke.py"
MAPPER = ROOT / "examples" / "openai-agents-js-approval-interruption-evidence" / "map_to_assay.py"


class PausedApprovalSmokeTests(unittest.TestCase):
    def test_smoke_script_stdout_emits_valid_artifact(self) -> None:
        result = subprocess.run(
            [
                sys.executable,
                str(SMOKE),
                "--paused-result",
                str(RAW_RESULT),
                "--serialized-state",
                str(RAW_STATE),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        artifact = json.loads(result.stdout)
        normalized = validate_pause_artifact(artifact)
        self.assertEqual(normalized["framework"], "openai_agents_js")

    def test_public_import_one_liner_emits_valid_artifact(self) -> None:
        command = """
import json
from pathlib import Path
from patterns.paused_approval import capture_paused_approval, derive_resume_state_ref, emit_pause_artifact
raw = json.loads(Path("patterns/paused_approval/fixtures/raw/openai_agents_js.paused_result.json").read_text())
state = Path("patterns/paused_approval/fixtures/raw/openai_agents_js.serialized_state.txt").read_text()
artifact = emit_pause_artifact(
    capture_paused_approval(raw),
    framework="openai_agents_js",
    schema="openai-agents-js.tool-approval-interruption.export.v1",
    surface="tool_approval_interruption_resumable_state",
    resume_state_ref=derive_resume_state_ref(state),
)
print(json.dumps(artifact))
"""
        result = subprocess.run(
            [sys.executable, "-c", command],
            capture_output=True,
            text=True,
            cwd=ROOT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        artifact = json.loads(result.stdout)
        normalized = validate_pause_artifact(artifact)
        self.assertEqual(normalized["pause_reason"], "tool_approval")

    def test_smoke_output_is_mapper_compatible(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            artifact_path = Path(temp_dir) / "artifact.json"
            ndjson_path = Path(temp_dir) / "artifact.ndjson"
            smoke = subprocess.run(
                [
                    sys.executable,
                    str(SMOKE),
                    "--paused-result",
                    str(RAW_RESULT),
                    "--serialized-state",
                    str(RAW_STATE),
                    "--output",
                    str(artifact_path),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(smoke.returncode, 0, smoke.stderr)
            mapper = subprocess.run(
                [
                    sys.executable,
                    str(MAPPER),
                    str(artifact_path),
                    "--output",
                    str(ndjson_path),
                    "--import-time",
                    "2026-04-18T10:45:00Z",
                    "--overwrite",
                ],
                capture_output=True,
                text=True,
                cwd=ROOT,
                check=False,
            )
            self.assertEqual(mapper.returncode, 0, mapper.stderr or mapper.stdout)
            self.assertTrue(ndjson_path.exists())


if __name__ == "__main__":
    unittest.main()
