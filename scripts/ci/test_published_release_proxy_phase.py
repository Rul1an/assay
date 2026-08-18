#!/usr/bin/env python3
"""Behavioral tests for the published-release proxy phase."""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "scripts/ci/published_release_proxy_phase.py"


class PublishedReleaseProxyPhaseTests(unittest.TestCase):
    def run_phase(
        self,
        fake_exit: int,
        request: bytes = b'{"jsonrpc":"2.0","id":1}\n',
        fake_sleep: float = 0,
        fake_output_bytes: int = 0,
        timeout_seconds: int = 60,
    ) -> tuple[subprocess.CompletedProcess[bytes], Path]:
        temporary = Path(self.enterContext(tempfile.TemporaryDirectory()))
        fake = temporary / "assay-mcp-server"
        fake.write_text(
            textwrap.dedent(
                """\
                #!/usr/bin/env python3
                import json, os, pathlib, sys, time

                def value(flag):
                    return pathlib.Path(sys.argv[sys.argv.index(flag) + 1])

                decisions = value("--enforcement-decision-out")
                observations = value("--denied-call-observation-out")
                invocation = decisions.parent / "fake-invocations.jsonl"
                with invocation.open("a", encoding="utf-8") as handle:
                    handle.write(json.dumps(sys.argv) + "\\n")
                (decisions.parent / "fake-environment.json").write_text(
                    json.dumps(dict(os.environ)), encoding="utf-8"
                )
                control = json.loads(
                    (decisions.parent / "fake-control.json").read_text(encoding="utf-8")
                )
                sys.stdin.buffer.read()
                time.sleep(control["sleep"])
                if control["output_bytes"]:
                    sys.stdout.write("x" * control["output_bytes"])
                    sys.stdout.flush()
                decisions.write_text('{"decision":"deny"}\\n', encoding="utf-8")
                observations.write_text('{"observed":true}\\n', encoding="utf-8")
                print("fake proxy stdout")
                print("fake proxy stderr", file=sys.stderr)
                raise SystemExit(control["exit"])
                """
            ),
            encoding="utf-8",
        )
        fake.chmod(0o755)
        results = temporary / "results"
        results.mkdir()
        (results / "fake-control.json").write_text(
            json.dumps(
                {"exit": fake_exit, "sleep": fake_sleep, "output_bytes": fake_output_bytes}
            ),
            encoding="utf-8",
        )
        command = [
            sys.executable,
            str(HELPER),
            "--timeout-seconds",
            str(timeout_seconds),
        ]
        environment = os.environ.copy()
        environment["GH_TOKEN"] = "must-not-reach-release-code"
        environment["PYTHONPATH"] = "/must/not/reach/release/code"
        environment["PATH"] = f"{temporary}{os.pathsep}{environment['PATH']}"
        completed = subprocess.run(
            command,
            input=request,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            env=environment,
            cwd=results,
        )
        return completed, results

    def assert_observed_equals_recorded(self, results: Path, expected_status: int) -> None:
        observed = [
            json.loads(line)
            for line in (results / "fake-invocations.jsonl").read_text(encoding="utf-8").splitlines()
        ]
        records = [
            json.loads(line)
            for line in (results / "commands.ndjson").read_text(encoding="utf-8").splitlines()
        ]
        self.assertEqual(len(observed), 1)
        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["name"], "proxy-enforce")
        self.assertEqual(records[0]["exit_code"], expected_status)
        self.assertEqual(records[0]["argv"], observed[0])
        self.assertEqual((results / "proxy.jsonl").read_text(encoding="utf-8"), "fake proxy stdout\n")
        self.assertEqual((results / "proxy.stderr").read_text(encoding="utf-8"), "fake proxy stderr\n")
        self.assertTrue((results / "decisions.ndjson").is_file())
        self.assertTrue((results / "denied-observations.ndjson").is_file())
        child_environment = json.loads(
            (results / "fake-environment.json").read_text(encoding="utf-8")
        )
        self.assertNotIn("GH_TOKEN", child_environment)
        self.assertNotIn("PYTHONPATH", child_environment)

    def test_success_records_the_executed_argv_once(self) -> None:
        completed, results = self.run_phase(0)
        self.assertEqual(completed.returncode, 0, completed.stderr.decode())
        self.assert_observed_equals_recorded(results, 0)

    def test_failure_preserves_the_real_status_and_argv(self) -> None:
        completed, results = self.run_phase(23)
        self.assertEqual(completed.returncode, 23, completed.stderr.decode())
        self.assert_observed_equals_recorded(results, 23)

    def test_request_ceiling_fails_before_execution(self) -> None:
        completed, results = self.run_phase(0, b"x" * (1_048_576 + 1))
        self.assertNotEqual(completed.returncode, 0)
        self.assertIn(b"proxy request exceeds 1 MiB ceiling", completed.stderr)
        self.assertFalse((results / "fake-invocations.jsonl").exists())
        self.assertFalse((results / "commands.ndjson").exists())

    def test_timeout_records_the_bounded_harness_status(self) -> None:
        completed, results = self.run_phase(0, fake_sleep=2, timeout_seconds=1)
        self.assertEqual(completed.returncode, 124, completed.stderr.decode())
        records = [
            json.loads(line)
            for line in (results / "commands.ndjson").read_text(encoding="utf-8").splitlines()
        ]
        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["exit_code"], 124)
        observed = [
            json.loads(line)
            for line in (results / "fake-invocations.jsonl").read_text(encoding="utf-8").splitlines()
        ]
        self.assertEqual(records[0]["argv"], observed[0])

    def test_output_file_ceiling_stops_unbounded_child_output(self) -> None:
        completed, results = self.run_phase(0, fake_output_bytes=16_777_216 + 1)
        self.assertNotEqual(completed.returncode, 0)
        self.assertLessEqual((results / "proxy.jsonl").stat().st_size, 16_777_216)
        records = [
            json.loads(line)
            for line in (results / "commands.ndjson").read_text(encoding="utf-8").splitlines()
        ]
        self.assertEqual(len(records), 1)
        self.assertNotEqual(records[0]["exit_code"], 0)


if __name__ == "__main__":
    unittest.main()
