"""Failure-path tests; mocked Docker is not isolation or replay execution proof."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import verify_incident_replay as replay


class CleanupTests(unittest.TestCase):
    def exercise(self, *, removal_exit=0, receipt_failure=False, timeout=False):
        root = Path(__file__).resolve().parents[2]
        expected = (root / "crates/assay-evidence/tests/fixtures/incident_package"
                    / "combined_session_3345.report.json").read_bytes()
        commands = []

        def run(command, **kwargs):
            commands.append(command)
            if command[:2] == ["docker", "start"]:
                if timeout:
                    raise subprocess.TimeoutExpired(command, 60, b"partial", b"diagnostic")
                return subprocess.CompletedProcess(command, 0, expected, b"")
            code = removal_exit if command[:2] == ["docker", "rm"] else 0
            return subprocess.CompletedProcess(command, code, b"", b"cleanup failed" if code else b"")

        def output(command, **kwargs):
            if command[0] == "git":
                return "a" * 40 + "\n"
            if command[:3] == ["docker", "image", "inspect"]:
                return json.dumps([{"Id": "test-image"}]).encode()
            return json.dumps([{
                "Id": "test-container", "Image": "test-image", "Mounts": [],
                "Config": {"User": "65532:65532"},
                "HostConfig": {"NetworkMode": "none", "ReadonlyRootfs": True,
                               "Privileged": False},
                "State": {"ExitCode": 0, "OOMKilled": False, "Running": False},
            }]).encode()

        original_write = Path.write_text

        def write(path, *args, **kwargs):
            if receipt_failure and path.name == "receipt.json":
                raise OSError("injected receipt write failure")
            return original_write(path, *args, **kwargs)

        with tempfile.TemporaryDirectory() as tmp:
            dest = Path(tmp) / "result"
            with patch.object(sys, "argv", ["replay", sys.executable, str(dest)]), \
                    patch.object(subprocess, "run", side_effect=run), \
                    patch.object(subprocess, "check_output", side_effect=output), \
                    patch.object(replay.platform, "platform", return_value="test-platform"), \
                    patch.object(Path, "write_text", write):
                with self.assertRaises((RuntimeError, OSError, subprocess.TimeoutExpired)):
                    replay.main()
            self.assertTrue(any(c[:2] == ["docker", "rm"] for c in commands),
                            "owned container cleanup was never attempted")
            if not receipt_failure:
                receipt = json.loads((dest / "receipt.json").read_text())
                self.assertEqual(receipt.get("cleanup_exit"), removal_exit)
            if timeout:
                self.assertEqual((dest / "report.json").read_bytes(), b"partial")
                self.assertEqual((dest / "runtime.stderr").read_bytes(), b"diagnostic")

    def test_failed_cleanup_cannot_return_success(self):
        self.exercise(removal_exit=1)

    def test_receipt_write_failure_still_attempts_cleanup(self):
        self.exercise(receipt_failure=True)

    def test_timeout_retains_output_and_removes_owned_container(self):
        self.exercise(timeout=True)


if __name__ == "__main__":
    unittest.main()
