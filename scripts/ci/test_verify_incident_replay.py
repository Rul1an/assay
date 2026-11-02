"""Failure-path tests; mocked Docker is not isolation or replay execution proof."""
import json
import hashlib
import os
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


class ProducerTests(unittest.TestCase):
    def test_timeout_retains_diagnostic_and_reaps_owned_process(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp)
            command = [sys.executable, "-c", "import os,time; print(os.getpid(), flush=True); time.sleep(30)"]
            with self.assertRaises(subprocess.TimeoutExpired):
                replay.run_producer(command, output, output, "timeout", timeout=1)
            pid = int((output / "timeout.stdout").read_text())
            with self.assertRaises(ProcessLookupError):
                os.kill(pid, 0)
            receipt = json.loads((output / "timeout.command.json").read_text())
            self.assertLess(receipt["exit"], 0)

    def test_nonzero_producer_cannot_continue(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp)
            with self.assertRaises(subprocess.CalledProcessError):
                replay.run_producer([sys.executable, "-c", "raise SystemExit(7)"], output, output, "failure")
            self.assertEqual(json.loads((output / "failure.command.json").read_text())["exit"], 7)

    def test_live_recipe_resource_warning_is_fatal(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            script = root / "scripts/ci/verify_incident_replay.py"
            script.parent.mkdir(parents=True)
            script.write_text("import warnings\nwarnings.warn('replay warning probe', ResourceWarning)\n")
            with patch.object(sys.modules[__name__], "__file__", str(script.with_name("test_verify_incident_replay.py"))):
                with self.assertRaises(subprocess.CalledProcessError):
                    live_receiver_test(Path(sys.executable), root / "result")

    def test_noop_recipe_cannot_pass_live_assertions(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(subprocess, "run"):
            with self.assertRaisesRegex(AssertionError, "never produced a fresh wrap decision log"):
                live_receiver_test(Path(sys.executable), Path(tmp))


def live_receiver_test(binary, output):
    """Exercise the real recipe/binary/Docker, not the mocked cleanup witnesses."""
    root = Path(__file__).resolve().parents[2]
    subprocess.run([sys.executable, "-W", "error::ResourceWarning",
                    str(root / "scripts/ci/verify_incident_replay.py"),
                    str(binary), str(output)], check=True)
    check = unittest.TestCase()
    fresh = output / "fresh"
    check.assertTrue((fresh / "producer/decisions.ndjson").is_file(),
                     "replay never produced a fresh wrap decision log")
    log = (fresh / "producer/decisions.ndjson").read_bytes()
    decisions = [json.loads(line)["data"] for line in log.splitlines()]
    check.assertEqual([row["decision"] for row in decisions], ["allow", "deny"])
    package = (fresh / "package.tar").read_bytes()
    handoff = json.loads((fresh / "handoff.json").read_text())
    check.assertEqual(handoff["package_sha256"], hashlib.sha256(package).hexdigest())
    check.assertEqual(handoff["decision_log_sha256"], hashlib.sha256(log).hexdigest())
    for case, reason in [("valid", None), ("corrupt", "input_shape"),
                         ("missing", "digest"), ("wrong-expectation", "expectation_mismatch"),
                         ("small-budget", "resource_selected"), ("assessment-corrupt", "digest")]:
        with check.subTest(case=case):
            report = json.loads((fresh / case / "report.json").read_text())
            receipt = json.loads((fresh / case / "receipt.json").read_text())
            check.assertEqual(report["schema"], "assay.incident.verify.v1")
            check.assertEqual(report["outcome"], "package_verified" if reason is None else "package_refused")
            check.assertEqual(report["reason"], reason)
            check.assertEqual(receipt["exit"], 0 if reason is None else 2)
            check.assertEqual(receipt["cleanup_exit"], 0)
            check.assertEqual(sorted(receipt["payload_sha256"]), ["assay", "combined.tar", "context.json"])
            mounts = receipt["container_config"]["Mounts"]
            check.assertEqual([(m["Destination"], m["RW"]) for m in mounts], [("/payload", False)])
            if reason is None:
                check.assertEqual(report["artifact_sha256"], handoff["package_sha256"])
                check.assertEqual(report["counts"]["units"], 2)
                check.assertEqual(report["counts"]["examined"], 2)
                check.assertEqual(report["expectation"], "matched")
                check.assertIn("no_activity_completeness", report["non_claims"])
                check.assertIn("no_provider_outcome", report["non_claims"])
    print("fresh offline recipient: positive and five named refusals passed")


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "--live":
        live_receiver_test(Path(sys.argv[2]).resolve(), Path(sys.argv[3]).resolve())
    else:
        unittest.main()
