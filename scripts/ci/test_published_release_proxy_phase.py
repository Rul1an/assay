#!/usr/bin/env python3
"""Behavioral tests for the published-release proxy phase."""

from __future__ import annotations

import json
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import time
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "scripts/ci/published_release_proxy_phase.py"
WINDOWS_LAUNCHER = ROOT / "scripts/ci/published_release_offline_windows.py"


def load_helper():
    spec = importlib.util.spec_from_file_location("published_release_proxy_phase", HELPER)
    if spec is None or spec.loader is None:
        raise FileNotFoundError(HELPER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_windows_launcher():
    spec = importlib.util.spec_from_file_location("published_release_offline_windows", WINDOWS_LAUNCHER)
    if spec is None or spec.loader is None:
        raise FileNotFoundError(WINDOWS_LAUNCHER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class PublishedReleaseProxyPhaseTests(unittest.TestCase):
    def test_explicit_packaged_inputs_reach_child(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = Path(directory).resolve()
            (fixture / 'policies').mkdir()
            for name in ('mock_github_mcp.py', 'baseline-approved.json', 'policies/no-allowance.yaml'):
                (fixture / name).write_text('trusted test input')
            completed, results = self.run_phase(0, fixture_dir=fixture)
            self.assertEqual(completed.returncode, 0, completed.stderr)
            invocation = json.loads((results / 'fake-invocations.jsonl').read_text().splitlines()[0])
            self.assertEqual(invocation[invocation.index('--enforce-policy') + 1], str(fixture / 'policies/no-allowance.yaml'))
            self.assertIn(str(fixture / 'mock_github_mcp.py'), invocation)

    def test_missing_explicit_fixture_never_falls_back(self):
        completed, results = self.run_phase(0, fixture_dir=Path('/nonexistent-assay-packaged-fixture'))
        self.assertNotEqual(completed.returncode, 0)
        self.assertIn(b'explicit fixture directory', completed.stderr)
        self.assertFalse((results / 'fake-invocations.jsonl').exists())

    def run_phase(
        self,
        fake_exit: int,
        request: bytes = b'{"jsonrpc":"2.0","id":1}\n',
        fake_sleep: float = 0,
        fake_output_bytes: int = 0,
        spawn_grandchild: bool = False,
        timeout_seconds: int = 60,
        fixture_dir: Path | None = None,
    ) -> tuple[subprocess.CompletedProcess[bytes], Path]:
        temporary = Path(self.enterContext(tempfile.TemporaryDirectory(prefix="proxy phase ")))
        fake = temporary / "assay-mcp-server"
        fake.write_text(
            textwrap.dedent(
                """\
                #!/usr/bin/env python3
                import json, os, pathlib, subprocess, sys, time

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
                if control["spawn_grandchild"]:
                    sentinel = decisions.parent / "grandchild-sentinel"
                    subprocess.Popen(
                        [
                            sys.executable,
                            "-c",
                            'import pathlib,sys,time; time.sleep(1.5); pathlib.Path(sys.argv[1]).write_text("survived"); pathlib.Path(sys.argv[2]).write_text("mutated")',
                            str(sentinel),
                            str(decisions),
                        ]
                    )
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
        poison = temporary / "pythonpath-poison"
        poison.mkdir()
        poison_sentinel = results / "pythonpath-imported"
        (poison / "json.py").write_text(
            f"open({str(poison_sentinel)!r}, 'w').write('loaded')\nraise RuntimeError('PYTHONPATH loaded')\n",
            encoding="utf-8",
        )
        (results / "fake-control.json").write_text(
            json.dumps(
                {
                    "exit": fake_exit,
                    "sleep": fake_sleep,
                    "output_bytes": fake_output_bytes,
                    "spawn_grandchild": spawn_grandchild,
                }
            ),
            encoding="utf-8",
        )
        command = [
            sys.executable,
            "-I",
            str(HELPER),
            "--timeout-seconds",
            str(timeout_seconds),
        ]
        if fixture_dir is not None:
            command.extend(["--fixture-dir", str(fixture_dir)])
        environment = os.environ.copy()
        environment["GH_TOKEN"] = "must-not-reach-release-code"
        environment["GITHUB_TOKEN"] = "must-not-reach-release-code"
        environment["PYTHONPATH"] = str(poison)
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
        self.assertFalse(poison_sentinel.exists(), "helper interpreter imported from PYTHONPATH")
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
        self.assertNotIn("GITHUB_TOKEN", child_environment)
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
        self.assertEqual(completed.returncode, 124, completed.stderr.decode() + (results / "proxy.stderr").read_text())
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

    def test_timeout_reaps_proxy_descendants_before_recording(self) -> None:
        completed, results = self.run_phase(
            0, fake_sleep=3, spawn_grandchild=True, timeout_seconds=1
        )
        self.assertEqual(completed.returncode, 124, completed.stderr.decode() + (results / "proxy.stderr").read_text())
        time.sleep(1)
        self.assertFalse((results / "grandchild-sentinel").exists())

    def test_success_reaps_proxy_descendants_before_recording(self) -> None:
        completed, results = self.run_phase(0, spawn_grandchild=True)
        self.assertEqual(
            completed.returncode,
            0,
            (results / "proxy.stderr").read_text(encoding="utf-8"),
        )
        time.sleep(2)
        self.assertFalse((results / "grandchild-sentinel").exists())
        self.assertEqual(
            (results / "decisions.ndjson").read_text(encoding="utf-8"),
            '{"decision":"deny"}\n',
        )

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


class WindowsJobExecutionTests(unittest.TestCase):
    def test_shared_windows_launcher_forwards_the_interactive_contract(self) -> None:
        windows = load_windows_launcher()
        result = {"exit": 0}
        with (
            mock.patch.object(windows, "launch_environment", return_value={"PATH": "clean"}),
            mock.patch.object(windows, "launch_in_profile", return_value=result) as launch,
            mock.patch.object(windows, "read_process_token", return_value={}),
        ):
            observed = windows.launch_interactive_job(
                ["assay-mcp-server.exe"],
                {"PATH": "dirty", "GH_TOKEN": "secret"},
                b"request\n",
                19,
                2,
                4096,
            )
        self.assertIs(observed, result)
        self.assertEqual(launch.call_args.args[:5], (None, None, ["assay-mcp-server.exe"], {"PATH": "clean"}, 19))
        self.assertEqual(launch.call_args.kwargs["input_bytes"], b"request\n")
        self.assertEqual(launch.call_args.kwargs["expected_lines"], 2)
        self.assertEqual(launch.call_args.kwargs["output_limit"], 4096)

    def test_windows_proxy_uses_the_shared_suspended_job_session(self) -> None:
        helper = load_helper()
        temporary = Path(self.enterContext(tempfile.TemporaryDirectory(prefix="windows proxy ")))
        stdout_path = temporary / "proxy.jsonl"
        stderr_path = temporary / "proxy.stderr"
        calls = []

        class Launcher:
            @staticmethod
            def launch_interactive_job(argv, env, request, timeout, expected_lines, output_limit):
                calls.append((argv, env, request, timeout, expected_lines, output_limit))
                return {
                    "create_process": True,
                    "exit": 0,
                    "job_closed": True,
                    "job_total_processes": 2,
                    "stderr": b"bounded stderr",
                    "stdout": b'{"jsonrpc":"2.0","id":9,"error":{}}\n',
                    "truncated": False,
                    "wait_result": "exited",
                }

        with (
            mock.patch.object(helper.sys, "platform", "win32"),
            mock.patch.object(helper, "load_windows_launcher", return_value=Launcher),
        ):
            status = helper.run_proxy_child(
                [r"C:\\bin\\assay-mcp-server.exe", "proxy-enforce"],
                b'{"jsonrpc":"2.0","id":9}\n',
                stdout_path,
                stderr_path,
                expected_lines=1,
                timeout=17,
            )

        self.assertEqual(status, 0)
        self.assertEqual(stdout_path.read_bytes(), b'{"jsonrpc":"2.0","id":9,"error":{}}\n')
        self.assertEqual(stderr_path.read_bytes(), b"bounded stderr")
        self.assertEqual(len(calls), 1)
        argv, env, request, timeout, expected_lines, output_limit = calls[0]
        self.assertEqual(argv[0], r"C:\\bin\\assay-mcp-server.exe")
        self.assertNotIn("GH_TOKEN", env)
        self.assertEqual(request, b'{"jsonrpc":"2.0","id":9}\n')
        self.assertEqual((timeout, expected_lines, output_limit), (17, 1, helper.MAX_OUTPUT_BYTES))

    def test_windows_proxy_refuses_truncated_or_unreaped_job_results(self) -> None:
        helper = load_helper()
        temporary = Path(self.enterContext(tempfile.TemporaryDirectory(prefix="windows proxy ")))
        valid = {
            "create_process": True,
            "exit": 0,
            "job_closed": True,
            "job_total_processes": 2,
            "stderr": b"",
            "stdout": b"response\n",
            "truncated": False,
            "wait_result": "exited",
        }
        variants = {
            "job-not-closed": {"job_closed": False},
            "output-truncated": {"truncated": True},
            "process-still-running": {"wait_result": "still-running"},
        }
        for name, changed in variants.items():
            result = {**valid, **changed}

            class Launcher:
                @staticmethod
                def launch_interactive_job(*_args, **_kwargs):
                    return result

            with (
                self.subTest(name=name),
                mock.patch.object(helper.sys, "platform", "win32"),
                mock.patch.object(helper, "load_windows_launcher", return_value=Launcher),
            ):
                status = helper.run_proxy_child(
                    ["assay-mcp-server.exe"],
                    b"request",
                    temporary / "stdout",
                    temporary / "stderr",
                    expected_lines=None,
                    timeout=5,
                )
                self.assertNotEqual(status, 0)


if __name__ == "__main__":
    unittest.main()
