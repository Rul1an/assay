#!/usr/bin/env python3
"""Behavioral regression suite for the #2494 MCP proxy-enforce cost pilot."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import textwrap
import time
import unittest

MODULE_PATH = Path(__file__).resolve().with_name("proxy_cost_pilot.py")
SPEC = importlib.util.spec_from_file_location("proxy_cost_pilot", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
pilot = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = pilot
SPEC.loader.exec_module(pilot)

FAKE_BINARY_TEMPLATE = """#!__SHEBANG__
import hashlib, json, os, subprocess, sys, time
from pathlib import Path

MODE = __MODE_REPR__
MARKER_PATH = __MARKER_REPR__
DESCENDANT_PID_PATH = __DESCENDANT_PID_REPR__

if MARKER_PATH:
    Path(MARKER_PATH).write_text("executed\\n", encoding="utf-8")

if not sys.argv[1:] or sys.argv[1] != "proxy-enforce":
    raise SystemExit(2)

args = sys.argv[2:]
upstream_cmd, upstream_args = None, []
enforce_policy = decision_out = denied_out = None
for i in range(0, len(args), 2):
    flag, val = args[i], args[i + 1]
    if flag == "--upstream-command":
        upstream_cmd = val
    elif flag == "--upstream-arg":
        upstream_args.append(val)
    elif flag == "--enforce-policy":
        enforce_policy = Path(val)
    elif flag == "--enforcement-decision-out":
        decision_out = Path(val)
    elif flag == "--denied-call-observation-out":
        denied_out = Path(val)

if MODE == "nonzero_exit":
    sys.stdout.write('{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05"}}\\n')
    sys.stdout.flush()
    sys.stderr.write("synthetic proxy-enforce fatal error\\n")
    sys.stderr.flush()
    raise SystemExit(17)

if MODE == "nonzero_exit_with_stderr_descendant":
    holder = subprocess.Popen(
        [sys.executable, "-c", "import sys, time; sys.stderr.write('descendant-holding-stderr\\\\n'); sys.stderr.flush(); time.sleep(20)"],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=None,
    )
    if DESCENDANT_PID_PATH:
        Path(DESCENDANT_PID_PATH).write_text(f"{holder.pid}\\n", encoding="utf-8")
    sys.stderr.write("leader-exiting-nonzero-with-stderr-descendant\\n")
    sys.stderr.flush()
    raise SystemExit(19)

if MODE == "timeout_with_descendant":
    sleeper = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"])
    if DESCENDANT_PID_PATH:
        Path(DESCENDANT_PID_PATH).write_text(f"{sleeper.pid}\\n", encoding="utf-8")
    sys.stderr.write("synthetic proxy-enforce stalling\\n")
    sys.stderr.flush()
    time.sleep(60)

if MODE == "exceed_fsize" and decision_out is not None:
    with decision_out.open("wb") as handle:
        handle.write(b"x" * (2 * 1024 * 1024))
    raise SystemExit(0)

is_deny = enforce_policy is not None and "no-allowance" in enforce_policy.name
upstream = None
if not (is_deny and MODE == "deny_no_upstream_spawn"):
    upstream = subprocess.Popen(
        [upstream_cmd, *upstream_args],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

decisions, denied_records = [], []
if MODE == "normal_with_extra_evidence" and not is_deny:
    decisions.append({"schema": "assay.enforcement_health.v0", "status": "healthy"})

call_index = 0
for raw_line in sys.stdin.buffer:
    if not raw_line.strip():
        continue
    msg = json.loads(raw_line.decode("utf-8"))
    method, mid = msg.get("method"), msg.get("id")

    if method == "initialize":
        if upstream is not None:
            upstream.stdin.write(raw_line)
            upstream.stdin.flush()
            resp = upstream.stdout.readline()
            probe = b'{"jsonrpc":"2.0","id":"assay-establish-1","method":"tools/list","params":{}}\\n'
            upstream.stdin.write(probe)
            upstream.stdin.flush()
            _ = upstream.stdout.readline()
        else:
            resp = b'{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}}}}\\n'
        sys.stdout.buffer.write(resp)
        sys.stdout.buffer.flush()
    elif method == "notifications/initialized":
        if upstream is not None:
            upstream.stdin.write(raw_line)
            upstream.stdin.flush()
    elif method == "tools/list":
        if upstream is not None:
            upstream.stdin.write(raw_line)
            upstream.stdin.flush()
            resp = upstream.stdout.readline()
        else:
            resp = b'{"jsonrpc":"2.0","id":2,"result":{"tools":[]}}\\n'
        sys.stdout.buffer.write(resp)
        sys.stdout.buffer.flush()
    elif method == "tools/call":
        call_index += 1
        params = msg.get("params") or {}
        tool_name = params.get("name", "")
        arguments = params.get("arguments") or {}
        target = {
            "provider": "github",
            "owner": arguments.get("owner", "acme"),
            "repo": arguments.get("repo", "prod-app"),
            "read_only": arguments.get("read_only", False),
        }
        target_jcs = json.dumps(target, sort_keys=True, separators=(",", ":")).encode("utf-8")
        target_digest = "sha256:" + hashlib.sha256(b"assay.mcp.target.v0\\0" + target_jcs).hexdigest()

        if is_deny:
            decisions.append({
                "schema": "assay.enforcement_decision.v0",
                "caller": {"id": "ci-agent"},
                "tool": {"name": tool_name, "action_class": "github_deploy_key"},
                "action": {
                    "verb": "create",
                    "resource_type": "github_deploy_key",
                    "target": target,
                    "target_digest": target_digest,
                },
                "decision": "deny",
                "reason": "no_declared_allowance",
                "fail_closed": True,
                "drift_state": "not_evaluated",
                "credential_alias": "gh-deploy",
                "non_claims": ["policy decision only"],
            })
            if MODE == "deny_forwards_upstream" and upstream is not None:
                upstream.stdin.write(raw_line)
                upstream.stdin.flush()
                _ = upstream.stdout.readline()
            err_data = {"origin": "assay-proxy", "reason": "no_declared_allowance"}
            if MODE == "deny_with_extra_fail_closed_in_error_data":
                err_data["fail_closed"] = True
            err_line = json.dumps({
                "jsonrpc": "2.0",
                "id": mid,
                "error": {
                    "code": -31999,
                    "message": "proxy-enforce denied tools/call: no_declared_allowance",
                    "data": err_data,
                },
            }, separators=(",", ":"))
            resp_digest = "sha256:" + hashlib.sha256(err_line.encode("utf-8")).hexdigest()
            if MODE != "deny_missing_observation":
                denied_records.append({
                    "schema": "assay.denied_call_observation.v1",
                    "call": {"tool_name": tool_name, "target_digest": target_digest},
                    "caller_visible_error": {"code": -31999, "origin": "assay-proxy", "reason": "no_declared_allowance"},
                    "caller_visible_response_digest": resp_digest,
                    "non_claims": ["denied before upstream invoke"],
                })
            sys.stdout.write(err_line + "\\n")
            sys.stdout.flush()
            continue

        allow_target = dict(target)
        if MODE == "missing_read_only_in_target":
            allow_target.pop("read_only", None)
        allow_action = {
            "verb": "create",
            "resource_type": "github_deploy_key",
            "target": allow_target,
            "target_digest": target_digest,
        }
        if MODE == "malformed_decision_nested_list":
            allow_action = [1]
        decisions.append({
            "schema": "assay.enforcement_decision.v0",
            "caller": {"id": "ci-agent"},
            "tool": {"name": tool_name, "action_class": "github_deploy_key"},
            "action": allow_action,
            "decision": "allow",
            "reason": "allow",
            "fail_closed": False,
            "drift_state": "satisfied",
            "credential_alias": "gh-deploy",
            "non_claims": ["policy decision only"],
        })
        if MODE == "skip_upstream_forward" and call_index == 1:
            sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": mid, "result": {"content": [{"type": "text", "text": "forwarded-ok"}], "isError": False}}) + "\\n")
            sys.stdout.flush()
            continue

        forward_bytes = raw_line
        if MODE == "mutate_upstream_args" and call_index == 1:
            mutated = json.loads(raw_line.decode("utf-8"))
            mutated["params"]["arguments"]["repo"] = "mutated-repo"
            forward_bytes = (json.dumps(mutated) + "\\n").encode("utf-8")

        upstream.stdin.write(forward_bytes)
        upstream.stdin.flush()
        resp_bytes = upstream.stdout.readline()

        if MODE == "wrong_response_id" and call_index == 1:
            parsed = json.loads(resp_bytes.decode("utf-8"))
            parsed["id"] = 999999
            resp_bytes = (json.dumps(parsed) + "\\n").encode("utf-8")
        elif MODE == "malformed_response_id_list" and call_index == 1:
            parsed = json.loads(resp_bytes.decode("utf-8"))
            parsed["id"] = []
            resp_bytes = (json.dumps(parsed) + "\\n").encode("utf-8")
        elif MODE == "missing_jsonrpc_version" and call_index == 1:
            parsed = json.loads(resp_bytes.decode("utf-8"))
            parsed.pop("jsonrpc", None)
            resp_bytes = (json.dumps(parsed) + "\\n").encode("utf-8")
        elif MODE == "missing_response" and call_index == 1:
            break
        elif MODE == "duplicate_response_id" and call_index == 1:
            sys.stdout.buffer.write(resp_bytes)
        elif MODE == "wrong_response_content" and call_index == 1:
            parsed = json.loads(resp_bytes.decode("utf-8"))
            parsed["result"]["content"][0]["text"] = "tampered-response"
            resp_bytes = (json.dumps(parsed) + "\\n").encode("utf-8")
        elif MODE == "oversized_line" and call_index == 1:
            parsed = json.loads(resp_bytes.decode("utf-8"))
            parsed["result"]["padding"] = "x" * 90000
            resp_bytes = (json.dumps(parsed) + "\\n").encode("utf-8")
        elif MODE == "duplicate_json_key" and call_index == 1:
            resp_bytes = f'{{"jsonrpc":"2.0","id":{json.dumps(mid)},"id":999,"result":{{"content":[{{"type":"text","text":"forwarded-ok"}}],"isError":false}}}}\\n'.encode("utf-8")

        sys.stdout.buffer.write(resp_bytes)
        sys.stdout.buffer.flush()

if upstream is not None:
    upstream.stdin.close()
    upstream.wait(timeout=5)
    upstream.stdout.close()
    upstream.stderr.close()

if decision_out is not None:
    if MODE == "missing_evidence" and not is_deny:
        pass
    elif MODE == "truncated_evidence" and not is_deny:
        decision_out.write_text('{"schema":"assay.enforcement_decision.v0"', encoding="utf-8")
    else:
        with decision_out.open("w", encoding="utf-8") as handle:
            for record in decisions:
                handle.write(json.dumps(record, separators=(",", ":")) + "\\n")

if denied_out is not None and denied_records:
    with denied_out.open("w", encoding="utf-8") as handle:
        for record in denied_records:
            handle.write(json.dumps(record, separators=(",", ":")) + "\\n")
"""


def write_fake_binary(
    directory: Path,
    mode: str = "normal",
    marker_path: Path | None = None,
    descendant_pid_path: Path | None = None,
    shebang: str = "/usr/bin/env python3",
) -> tuple[Path, str]:
    binary_path = directory / f"fake-assay-mcp-server-{mode}"
    script = (
        FAKE_BINARY_TEMPLATE.replace("__SHEBANG__", shebang)
        .replace("__MODE_REPR__", repr(mode))
        .replace("__MARKER_REPR__", repr(str(marker_path) if marker_path else None))
        .replace(
            "__DESCENDANT_PID_REPR__",
            repr(str(descendant_pid_path) if descendant_pid_path else None),
        )
    )
    binary_path.write_text(textwrap.dedent(script), encoding="utf-8")
    binary_path.chmod(binary_path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    with binary_path.open("rb") as handle:
        return binary_path, hashlib.file_digest(handle, "sha256").hexdigest()


def inspect_process_state(pid: int) -> str:
    proc_status = Path(f"/proc/{pid}/status")
    if proc_status.is_file():
        try:
            for line in proc_status.read_text(encoding="utf-8", errors="replace").splitlines():
                if line.startswith("State:"):
                    return line.strip()
        except OSError:
            pass
    try:
        res = subprocess.run(
            ["ps", "-o", "stat=", "-p", str(pid)],
            capture_output=True,
            text=True,
            timeout=1.0,
            check=False,
        )
        state = res.stdout.strip()
        if state:
            return f"ps_stat={state}"
    except Exception:
        pass
    return "unknown_live_state"


class ProxyCostPilotBehavioralTests(unittest.TestCase):
    def assert_process_absent(self, pid: int, timeout_seconds: float = 1.5) -> None:
        deadline = time.monotonic() + timeout_seconds
        while time.monotonic() < deadline:
            try:
                os.kill(pid, 0)
            except ProcessLookupError:
                return
            except PermissionError:
                break
            time.sleep(0.01)
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return
        state_desc = inspect_process_state(pid)
        try:
            os.kill(pid, 9)
        except OSError:
            pass
        self.fail(f"descendant pid {pid} still existed after {timeout_seconds}s ({state_desc})")

    def make_config(
        self,
        temp_dir: Path,
        binary_path: Path,
        expected_sha256: str,
        out_name: str = "out",
        blocks: int = 2,
        calls_per_session: int = 3,
        timeout_seconds: float = 5.0,
        max_line_bytes: int = 65_536,
        max_total_bytes: int = 1_048_576,
        bootstrap_resamples: int = 200,
    ) -> pilot.PilotConfig:
        return pilot.PilotConfig(
            mcp_binary=binary_path.resolve(),
            expected_binary_sha256=expected_sha256,
            out_dir=(temp_dir / out_name).resolve(),
            blocks=blocks,
            calls_per_session=calls_per_session,
            timeout_seconds=timeout_seconds,
            max_line_bytes=max_line_bytes,
            max_total_bytes=max_total_bytes,
            bootstrap_resamples=bootstrap_resamples,
            allow_non_linux=True,
        )

    def test_wrong_binary_hash_refuses_before_spawning_child(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            temp_dir = Path(tmp)
            marker_path = temp_dir / "child-ran.marker"
            binary_path, _actual_sha = write_fake_binary(
                temp_dir, mode="normal", marker_path=marker_path
            )
            config = self.make_config(temp_dir, binary_path, "0" * 64, out_name="wrong-hash")
            with self.assertRaises(pilot.PilotFailure) as ctx:
                pilot.run_pilot(config)
            self.assertEqual(ctx.exception.reason, "binary_sha256_mismatch")
            self.assertFalse(marker_path.exists())
            receipt = json.loads((config.out_dir / "receipt.json").read_text(encoding="utf-8"))
            self.assertEqual(receipt["status"], "failed")
            self.assertFalse(receipt["execution_valid"])
            self.assertNotIn("publishable_conclusion", receipt)

    def test_exclusive_output_dir_and_parameter_bounds(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            temp_dir = Path(tmp)
            binary_path, actual_sha = write_fake_binary(temp_dir, mode="normal")
            existing_out = temp_dir / "existing-out"
            existing_out.mkdir()
            sentinel = existing_out / "keep.txt"
            sentinel.write_text("keep\n", encoding="utf-8")
            with self.assertRaises(pilot.PilotFailure) as ctx:
                pilot.run_pilot(self.make_config(temp_dir, binary_path, actual_sha, out_name="existing-out"))
            self.assertEqual(ctx.exception.reason, "output_dir_not_exclusive")
            self.assertEqual(sentinel.read_text(encoding="utf-8"), "keep\n")

            for bad_timeout in (float("nan"), float("inf"), 0.0, 301.0):
                with self.assertRaises(pilot.PilotFailure) as ctx:
                    pilot.run_pilot(
                        self.make_config(
                            temp_dir,
                            binary_path,
                            actual_sha,
                            out_name=f"out-timeout-{bad_timeout}",
                            timeout_seconds=bad_timeout,
                        )
                    )
                self.assertEqual(ctx.exception.reason, "invalid_bounds")

            for bad_resamples in (0, 10_001):
                with self.assertRaises(pilot.PilotFailure) as ctx:
                    pilot.run_pilot(
                        self.make_config(
                            temp_dir,
                            binary_path,
                            actual_sha,
                            out_name=f"out-boot-{bad_resamples}",
                            bootstrap_resamples=bad_resamples,
                        )
                    )
                self.assertEqual(ctx.exception.reason, "invalid_bounds")

    def test_exit_zero_protocol_upstream_and_evidence_mismatches_fail_closed(self) -> None:
        cases = [
            ("wrong_response_id", "response_id_mismatch"),
            ("malformed_response_id_list", "response_id_mismatch"),
            ("missing_jsonrpc_version", "malformed_jsonrpc_response"),
            ("missing_response", "response_count_mismatch"),
            ("duplicate_response_id", "duplicate_response_id"),
            ("wrong_response_content", "response_result_mismatch"),
            ("skip_upstream_forward", "upstream_call_count_mismatch"),
            ("mutate_upstream_args", "upstream_call_parity_mismatch"),
            ("missing_evidence", "missing_decision_evidence"),
            ("truncated_evidence", "malformed_decision_evidence"),
            ("missing_read_only_in_target", "invalid_decision_evidence_fields"),
            ("malformed_decision_nested_list", "invalid_decision_evidence_fields"),
            ("deny_with_extra_fail_closed_in_error_data", "deny_error_contract_mismatch"),
        ]
        for mode, expected_reason in cases:
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as tmp:
                temp_dir = Path(tmp)
                binary_path, actual_sha = write_fake_binary(temp_dir, mode=mode)
                config = self.make_config(temp_dir, binary_path, actual_sha, out_name=f"out-{mode}", blocks=1)
                with self.assertRaises(pilot.PilotFailure) as ctx:
                    pilot.run_pilot(config)
                self.assertEqual(ctx.exception.reason, expected_reason)
                receipt = json.loads((config.out_dir / "receipt.json").read_text(encoding="utf-8"))
                self.assertFalse(receipt["execution_valid"])
                self.assertEqual(receipt["failure_reason"], expected_reason)

    def test_timeout_nonzero_and_oserror_retain_receipt_and_reap_descendants(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            temp_dir = Path(tmp)
            binary_path, actual_sha = write_fake_binary(temp_dir, mode="nonzero_exit")
            config = self.make_config(temp_dir, binary_path, actual_sha, out_name="out-nonzero", blocks=1)
            with self.assertRaises(pilot.PilotFailure) as ctx:
                pilot.run_pilot(config)
            self.assertEqual(ctx.exception.reason, "child_nonzero_exit")
            receipt = json.loads((config.out_dir / "receipt.json").read_text(encoding="utf-8"))
            stderr_path = Path(receipt["failed_session_artifacts"]["stderr_path"])
            self.assertIn("synthetic proxy-enforce fatal error", stderr_path.read_text(encoding="utf-8"))
            requests_path = Path(receipt["failed_session_artifacts"]["requests_path"])
            self.assertEqual(len(requests_path.read_bytes().splitlines()), 3)

            descendant_pid_file = temp_dir / "descendant.pid"
            t_bin, t_sha = write_fake_binary(
                temp_dir, mode="timeout_with_descendant", descendant_pid_path=descendant_pid_file
            )
            t_cfg = self.make_config(temp_dir, t_bin, t_sha, out_name="out-timeout", blocks=1, timeout_seconds=0.4)
            with self.assertRaises(pilot.PilotFailure) as ctx:
                pilot.run_pilot(t_cfg)
            self.assertEqual(ctx.exception.reason, "child_timeout")
            t_receipt = json.loads((t_cfg.out_dir / "receipt.json").read_text(encoding="utf-8"))
            t_requests_path = Path(t_receipt["failed_session_artifacts"]["requests_path"])
            self.assertEqual(len(t_requests_path.read_bytes().splitlines()), 1)
            descendant_pid = int(descendant_pid_file.read_text(encoding="utf-8").strip())
            self.assert_process_absent(descendant_pid)

            bad_bin, bad_sha = write_fake_binary(
                temp_dir, mode="bad_exec", shebang="/nonexistent/assay-interpreter"
            )
            bad_cfg = self.make_config(temp_dir, bad_bin, bad_sha, out_name="out-oserror", blocks=1)
            with self.assertRaises(pilot.PilotFailure) as ctx:
                pilot.run_pilot(bad_cfg)
            self.assertEqual(ctx.exception.reason, "execution_io_error")
            bad_receipt = json.loads((bad_cfg.out_dir / "receipt.json").read_text(encoding="utf-8"))
            self.assertEqual(bad_receipt["failure_reason"], "execution_io_error")

    def test_nonzero_leader_with_stderr_holding_descendant_does_not_hang_cleanup(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            temp_dir = Path(tmp)
            pid_file = temp_dir / "stderr-descendant.pid"
            binary_path, actual_sha = write_fake_binary(
                temp_dir,
                mode="nonzero_exit_with_stderr_descendant",
                descendant_pid_path=pid_file,
            )
            out_dir = temp_dir / "out-stderr-descendant"
            proc = subprocess.Popen(
                [
                    sys.executable,
                    str(MODULE_PATH),
                    "--mcp-binary",
                    str(binary_path),
                    "--expected-binary-sha256",
                    actual_sha,
                    "--out-dir",
                    str(out_dir),
                    "--blocks",
                    "1",
                    "--calls-per-session",
                    "2",
                    "--timeout-seconds",
                    "0.4",
                    "--allow-non-linux",
                ],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                start_new_session=True,
            )
            try:
                stdout_bytes, stderr_bytes = proc.communicate(timeout=4.0)
            except subprocess.TimeoutExpired:
                pilot.stop_process_group(proc)
                if proc.stdout is not None:
                    proc.stdout.close()
                if proc.stderr is not None:
                    proc.stderr.close()
                if pid_file.is_file():
                    try:
                        os.kill(int(pid_file.read_text(encoding="utf-8").strip()), 9)
                    except OSError:
                        pass
                self.fail("pilot cleanup hung past 4.0s supervisor when descendant held stderr open")

            self.assertEqual(proc.returncode, 1, msg=f"stdout={stdout_bytes!r} stderr={stderr_bytes!r}")
            receipt = json.loads((out_dir / "receipt.json").read_text(encoding="utf-8"))
            self.assertFalse(receipt["execution_valid"])
            self.assertEqual(receipt["failure_reason"], "child_nonzero_exit")
            stderr_artifact = Path(receipt["failed_session_artifacts"]["stderr_path"]).read_text(encoding="utf-8")
            self.assertIn("leader-exiting-nonzero-with-stderr-descendant", stderr_artifact)
            descendant_pid = int(pid_file.read_text(encoding="utf-8").strip())
            self.assert_process_absent(descendant_pid)

    def test_deny_check_requires_upstream_init_observation_and_zero_calls(self) -> None:
        for mode, expected_reason in [
            ("deny_no_upstream_spawn", "missing_upstream_observation"),
            ("deny_forwards_upstream", "deny_forwarded_to_upstream"),
            ("deny_missing_observation", "missing_denied_call_observation"),
        ]:
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as tmp:
                temp_dir = Path(tmp)
                binary_path, actual_sha = write_fake_binary(temp_dir, mode=mode)
                config = self.make_config(temp_dir, binary_path, actual_sha, out_name=f"out-{mode}", blocks=1)
                with self.assertRaises(pilot.PilotFailure) as ctx:
                    pilot.run_pilot(config)
                self.assertEqual(ctx.exception.reason, expected_reason)

    def test_bounds_rlimit_fsize_and_failure_never_becomes_zero_sample(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            temp_dir = Path(tmp)
            for mode, expected_reasons in [
                ("oversized_line", ("line_byte_limit_exceeded",)),
                ("duplicate_json_key", ("duplicate_json_key",)),
                ("exceed_fsize", ("child_nonzero_exit", "total_byte_limit_exceeded", "missing_decision_evidence")),
            ]:
                with self.subTest(mode=mode):
                    binary_path, actual_sha = write_fake_binary(temp_dir, mode=mode)
                    config = self.make_config(
                        temp_dir,
                        binary_path,
                        actual_sha,
                        out_name=f"out-{mode}",
                        blocks=1,
                        max_line_bytes=4096,
                        max_total_bytes=65_536,
                    )
                    with self.assertRaises(pilot.PilotFailure) as ctx:
                        pilot.run_pilot(config)
                    self.assertIn(ctx.exception.reason, expected_reasons)
                    summary = json.loads((config.out_dir / "summary.json").read_text(encoding="utf-8"))
                    self.assertEqual(summary["status"], "failed")
                    self.assertFalse(summary["execution_valid"])
                    self.assertEqual(summary["valid_blocks"], 0)

    def test_positive_counterbalanced_pilot_and_provenance(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            temp_dir = Path(tmp)
            binary_path, actual_sha = write_fake_binary(temp_dir, mode="normal_with_extra_evidence")
            config = self.make_config(
                temp_dir, binary_path, actual_sha, out_name="out-positive", blocks=2, calls_per_session=3
            )
            summary = pilot.run_pilot(config)
            self.assertEqual(summary["status"], "completed")
            self.assertTrue(summary["execution_valid"])
            self.assertNotIn("publishable_conclusion", summary)
            self.assertEqual(summary["valid_blocks"], 2)
            self.assertEqual(summary["blocks_order"], ["AB", "BA"])

            prov = summary["provenance"]
            self.assertEqual(prov["mcp_binary_sha256"], actual_sha)
            self.assertTrue(prov["binary_continuity_verified"])
            self.assertIn("pilot_script_sha256", prov)
            self.assertIn("published_proxy_helper_sha256", prov)
            self.assertIn("process_group_helper_sha256", prov)
            self.assertFalse(prov["operator_declared_release_metadata"]["authenticated_by_harness"])

            rss = summary["rss_memory_scope"]
            self.assertEqual(rss["label"], "maximum_single_process_high_water")
            self.assertFalse(rss["comparable_across_arms"])
            self.assertFalse(rss["combined_process_tree_claimed"])

            first_block = json.loads((config.out_dir / "blocks.jsonl").read_text(encoding="utf-8").splitlines()[0])
            self.assertEqual(first_block["proxy_enforce_allow"]["extra_upstream_tools_list_count"], 1)
            self.assertEqual(first_block["proxy_enforce_allow"]["other_legitimate_evidence_records"], 1)
            self.assertEqual(
                first_block["proxy_enforce_allow"]["upstream_observation_mode"],
                "fixture_strip_normalized_jsonl",
            )


if __name__ == "__main__":
    unittest.main()
