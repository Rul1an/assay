"""Failure-path tests; mocked Docker is not isolation or replay execution proof."""
import json
import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tarfile
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
    sidecar_path = fresh / "producer/receipts.ndjson"
    check.assertTrue(sidecar_path.is_file(), "replay never produced a request sidecar")
    sidecar = sidecar_path.read_bytes()
    rows = [json.loads(line) for line in sidecar.splitlines()]
    check.assertEqual(len(rows), 2)
    check.assertEqual({row["schema"] for row in rows}, {"assay.request_decision_receipt.v0"})
    sidecar_digest = hashlib.sha256(sidecar).hexdigest()
    context = json.loads((fresh / "producer/context.json").read_text())
    check.assertEqual([(e["input_id"], e["sha256"]) for e in context["expectations"]], [
        ("wrap-decision-log", hashlib.sha256(log).hexdigest()),
        ("wrap-request-receipts", sidecar_digest),
    ])
    with tarfile.open(fresh / "package.tar", "r:") as archive:
        with archive.extractfile("objects/" + sidecar_digest) as source:
            check.assertEqual(source.read(), sidecar, "handoff changed original sidecar bytes")
        with archive.extractfile("assessment.json") as source:
            assessment = json.load(source)
        check.assertTrue(all(s["observation"] == "unknown" for s in assessment["surfaces"]))
    package = (fresh / "package.tar").read_bytes()
    handoff = json.loads((fresh / "handoff.json").read_text())
    check.assertEqual(handoff["package_sha256"], hashlib.sha256(package).hexdigest())
    check.assertEqual(handoff["decision_log_sha256"], hashlib.sha256(log).hexdigest())
    check.assertEqual(handoff["request_sidecar_sha256"], sidecar_digest)
    for case, reason in [("valid", None), ("corrupt", "input_shape"),
                         ("missing", "digest"), ("wrong-expectation", "expectation_mismatch"),
                         ("small-budget", "resource_selected"), ("assessment-corrupt", "digest"),
                         ("sidecar-corrupt", "input_shape"), ("sidecar-missing", "digest"),
                         ("sidecar-wrong-expectation", "expectation_mismatch")]:
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
    # These negatives must target the sidecar, not repeat the decision-log cases.
    for case in ("sidecar-corrupt", "sidecar-missing"):
        with tarfile.open(fresh / case / "package.tar", "r:") as archive:
            with archive.extractfile("objects/" + handoff["decision_log_sha256"]) as source:
                check.assertEqual(source.read(), log)
            name = "objects/" + sidecar_digest
            if case == "sidecar-missing":
                check.assertNotIn(name, archive.getnames())
            else:
                changed = bytearray(sidecar)
                changed[0] ^= 1
                with archive.extractfile(name) as source:
                    check.assertEqual(source.read(), bytes(changed))
    wrong_context = json.loads((fresh / "sidecar-wrong-expectation/context.json").read_text())
    expected_context = json.loads((fresh / "producer/context.json").read_text())
    expected_context["expectations"][1]["sha256"] = "0" * 64
    check.assertEqual(wrong_context, expected_context)
    for action in ("accept", "decline"):
        mixed = output / ("mixed-" + action)
        sidecar_path = mixed / "producer/receipts.ndjson"
        check.assertTrue(sidecar_path.is_file(), "replay never produced a mixed ask/answer sidecar")
        sidecar = sidecar_path.read_bytes()
        records = [json.loads(line) for line in sidecar.splitlines()]
        check.assertEqual({r["schema"] for r in records}, {
            "assay.request_decision_receipt.v0", "assay.request_observation.v0"})
        wire = (mixed / "producer/wrap.stdout").read_bytes()
        sent = (mixed / "producer/requests.ndjson").read_bytes()
        ask = next(line for line in wire.splitlines() if b'elicitation/create' in line)
        answer = next(line for line in sent.splitlines() if b'"action"' in line)
        check.assertEqual(json.loads(answer)["result"]["action"], action)
        observations = [r for r in records if r["schema"] == "assay.request_observation.v0"]
        check.assertEqual([r["kind"] for r in observations], ["ask", "answer"])
        check.assertEqual([(r["call_link"], r["call_id"]) for r in observations],
                          [("unique", 2), ("unique", 2)])
        check.assertEqual(len(records), 3)
        # Compare exact forwarded frame strings, not reconstructed JSON.
        check.assertTrue(any(ask.decode() == r["request_line"].rstrip("\r\n") for r in observations))
        check.assertTrue(any(answer.decode() == r["request_line"].rstrip("\r\n") for r in observations))
        sidecar_digest = hashlib.sha256(sidecar).hexdigest()
        with tarfile.open(mixed / "package.tar", "r:") as archive:
            with archive.extractfile("objects/" + sidecar_digest) as source:
                check.assertEqual(source.read(), sidecar)
            with archive.extractfile("assessment.json") as source:
                assessment = json.load(source)
            check.assertTrue(all(s["observation"] == "unknown" for s in assessment["surfaces"]))
            check.assertEqual(assessment["joins"], [])
            check.assertEqual(assessment["claims"], [])
        valid = json.loads((mixed / "valid/report.json").read_text())
        check.assertEqual(valid["counts"]["units"], 1)
        check.assertEqual(valid["expectation"], "matched")
        for case, reason in [("valid", None), ("observation-corrupt", "input_shape")]:
            report = json.loads((mixed / case / "report.json").read_text())
            receipt = json.loads((mixed / case / "receipt.json").read_text())
            check.assertEqual(report["reason"], reason)
            check.assertEqual(receipt["exit"], 0 if reason is None else 2)
            check.assertEqual(receipt["cleanup_exit"], 0)
            check.assertEqual(sorted(receipt["payload_sha256"]), ["assay", "combined.tar", "context.json"])
            check.assertEqual([(m["Destination"], m["RW"]) for m in receipt["container_config"]["Mounts"]],
                              [("/payload", False)])
        with tarfile.open(mixed / "observation-corrupt/package.tar", "r:") as archive:
            with archive.extractfile("objects/" + sidecar_digest) as source:
                changed = source.read()
        # Only the retained answer is altered; a receipt-only corruption is insufficient.
        answer_start = 0
        for line in sidecar.splitlines(keepends=True):
            if json.loads(line).get("kind") == "answer":
                break
            answer_start += len(line)
        else:
            check.fail("missing answer row")
        marker = b'\\"action\\":\\"'
        at = answer_start + line.index(marker) + len(marker)
        check.assertEqual(sidecar[at:at + len(action)], action.encode())
        expected = bytearray(sidecar)
        expected[at] ^= 1
        check.assertEqual(changed, bytes(expected))
    print("fresh offline recipient: receipts and mixed ask/answer handoffs passed")


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "--live":
        live_receiver_test(Path(sys.argv[2]).resolve(), Path(sys.argv[3]).resolve())
    else:
        unittest.main()
