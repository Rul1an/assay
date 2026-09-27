#!/usr/bin/env python3
"""Execute and record the published-release proxy phase from one argv value."""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time

try:
    import resource
except ModuleNotFoundError:  # Windows has no resource module.
    resource = None


MAX_REQUEST_BYTES = 1_048_576
MAX_OUTPUT_BYTES = 16_777_216


def bounded_seconds(value: str) -> int:
    seconds = int(value)
    if not 1 <= seconds <= 300:
        raise argparse.ArgumentTypeError("timeout must be between 1 and 300 seconds")
    return seconds


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--timeout-seconds", type=bounded_seconds, default=60)
    parser.add_argument("--policy", choices=("deny", "allow"), default="deny")
    parser.add_argument("--expect", choices=("deny", "allow", "unsupported"))
    return parser.parse_args()


def append_command_record(path: Path, exit_code: int, argv: list[str]) -> None:
    record = {"name": "proxy-enforce", "exit_code": exit_code, "argv": argv}
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n")
        handle.flush()
        os.fsync(handle.fileno())


def child_environment() -> dict[str, str]:
    allowed = ("HOME", "PATH", "LANG", "LC_ALL", "TZ")
    return {key: os.environ[key] for key in allowed if key in os.environ}


def limit_child_output() -> None:
    if resource is None:
        raise RuntimeError("RLIMIT_FSIZE is unavailable")
    resource.setrlimit(resource.RLIMIT_FSIZE, (MAX_OUTPUT_BYTES, MAX_OUTPUT_BYTES))


def stop_process_group(process: subprocess.Popen[bytes]) -> None:
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=1)
    except subprocess.TimeoutExpired:
        pass
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait()


def read_records(path: Path) -> list[dict]:
    def unique_object(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"duplicate JSON member: {key}")
            result[key] = value
        return result

    with path.open("rb") as handle:
        data = handle.read(MAX_OUTPUT_BYTES + 1)
    if len(data) > MAX_OUTPUT_BYTES:
        raise ValueError(f"{path.name} exceeds output ceiling")
    records = [json.loads(line, object_pairs_hook=unique_object) for line in data.splitlines() if line.strip()]
    if not all(isinstance(record, dict) for record in records):
        raise ValueError(f"{path.name} contains a non-object record")
    return records


def case_request_ids(expected: str) -> tuple[int, ...]:
    return (1, 9) if expected == "allow" else (9,)


def exchange_case(process: subprocess.Popen[bytes], request: bytes, output: Path,
                  expected_lines: int, timeout: int) -> int:
    # communicate() closes stdin immediately. Keep it open until the proxy has drained replies.
    # stdout stays file-backed with RLIMIT_FSIZE; nonblocking writes share the same deadline.
    deadline = time.monotonic() + timeout
    pending = memoryview(request)
    os.set_blocking(process.stdin.fileno(), False)
    lines = 0
    with output.open("rb") as reader:
        while True:
            if pending:
                try:
                    pending = pending[os.write(process.stdin.fileno(), pending):]
                except BlockingIOError:
                    pass
                except BrokenPipeError:
                    break
            lines += reader.read(65536).count(b"\n")
            if (not pending and lines >= expected_lines) or process.poll() is not None:
                break
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise subprocess.TimeoutExpired(process.args, timeout)
            time.sleep(min(0.01, remaining))
    process.stdin.close()
    return process.wait(timeout=max(0.001, deadline - time.monotonic()))


def load_windows_launcher():
    path = Path(__file__).with_name("published_release_offline_windows.py")
    spec = importlib.util.spec_from_file_location("published_release_offline_windows", path)
    if spec is None or spec.loader is None:
        raise FileNotFoundError(path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def run_proxy_child(
    argv: list[str],
    request: bytes,
    stdout_path: Path,
    stderr_path: Path,
    *,
    expected_lines: int | None,
    timeout: int,
) -> int:
    if sys.platform == "win32":
        result = load_windows_launcher().launch_interactive_job(
            argv,
            child_environment(),
            request,
            timeout,
            expected_lines,
            MAX_OUTPUT_BYTES,
        )
        stdout = result.get("stdout") if isinstance(result, dict) else b""
        stderr = result.get("stderr") if isinstance(result, dict) else b""
        if not isinstance(stdout, bytes) or not isinstance(stderr, bytes):
            stdout, stderr = b"", b"invalid Windows job output types"
        stdout_path.write_bytes(stdout[:MAX_OUTPUT_BYTES])
        stderr_path.write_bytes(stderr[:MAX_OUTPUT_BYTES])
        total = result.get("job_total_processes") if isinstance(result, dict) else None
        valid = (
            isinstance(result, dict)
            and result.get("create_process") is True
            and result.get("job_closed") is True
            and result.get("wait_result") == "exited"
            and result.get("truncated") is False
            and isinstance(total, int)
            and not isinstance(total, bool)
            and total >= 1
            and len(stdout) <= MAX_OUTPUT_BYTES
            and len(stderr) <= MAX_OUTPUT_BYTES
        )
        if not valid:
            return 124 if isinstance(result, dict) and result.get("wait_result") == "timeout" else 125
        status = result.get("exit")
        return status if isinstance(status, int) and not isinstance(status, bool) and 0 <= status <= 255 else 125

    with stdout_path.open("wb") as stdout_handle, stderr_path.open("wb") as stderr_handle:
        try:
            process = subprocess.Popen(
                argv,
                stdin=subprocess.PIPE,
                stdout=stdout_handle,
                stderr=stderr_handle,
                env=child_environment(),
                preexec_fn=limit_child_output,
                start_new_session=True,
            )
            try:
                if expected_lines is not None:
                    status = exchange_case(process, request, stdout_path, expected_lines, timeout)
                else:
                    process.communicate(input=request, timeout=timeout)
                    status = process.returncode
            except subprocess.TimeoutExpired:
                status = 124
            finally:
                stop_process_group(process)
        except OSError as error:
            stderr_handle.write(f"proxy process failed to start: {error}\n".encode())
            status = 127
    return status


def validate_case(results: Path, expected: str) -> None:
    def require(condition, message):
        if not condition:
            raise ValueError(message)

    wire = read_records(results / "proxy.jsonl")
    request_ids = case_request_ids(expected)
    require(len(wire) == len(request_ids), "unexpected response cardinality for request case")
    for record, request_id in zip(wire, request_ids):
        require(record.get("jsonrpc") == "2.0" and type(record.get("id")) is int
                and record["id"] == request_id, "wire request identity drifted")
    if expected == "allow":
        require(isinstance(wire[0].get("result"), dict) and "error" not in wire[0], "initialize failed")
    reply = wire[-1]
    if expected == "allow":
        require("error" not in reply and isinstance(reply.get("result"), dict), "allow request failed")
        require(reply["result"].get("isError") is False and reply["result"].get("content") == [
            {"type": "text", "text": "forwarded-ok (mock; no real GitHub call)"}
        ], "allow reply is not the credential-free mock result")
    else:
        code, reason = ((-31997, "method_not_allowlisted") if expected == "unsupported"
                        else (-31999, "no_declared_allowance"))
        error = reply.get("error")
        require("result" not in reply and isinstance(error, dict), "expected a typed proxy error")
        require(type(error.get("code")) is int and error["code"] == code
                and isinstance(error.get("data"), dict)
                and error["data"].get("origin") == "assay-proxy"
                and error["data"].get("reason") == reason, "proxy error contract drifted")
    decisions = results / "decisions.ndjson"
    observations = results / "denied-observations.ndjson"
    if expected == "unsupported":
        require(not decisions.exists() and not observations.exists(), "unsupported request retained evidence")
        return
    rows = read_records(decisions)
    require(len(rows) == 1, "expected exactly one policy decision")
    row = rows[0]
    require(row.get("schema") == "assay.enforcement_decision.v0"
            and row.get("decision") == expected
            and row.get("reason") == ("allow" if expected == "allow" else "no_declared_allowance"),
            "policy decision does not match the request case")
    require(isinstance(row.get("tool"), dict) and row["tool"].get("name") == "github.add_deploy_key"
            and isinstance(row.get("action"), dict)
            and row["action"].get("target") == {"provider": "github", "owner": "acme", "repo": "prod-app"},
            "policy decision does not describe the single fixture call")
    if expected == "allow":
        require(not observations.exists(), "allow request retained denied observations")
    else:
        require(len(read_records(observations)) == 1, "expected exactly one denied observation")


def main() -> int:
    args = parse_args()
    results = Path.cwd().resolve()
    harness_root = Path(__file__).resolve().parents[2]
    fixture_root = harness_root / "examples/privileged-action-gate"
    decisions = results / "decisions.ndjson"
    observations = results / "denied-observations.ndjson"
    if args.expect and any(path.exists() for path in (
        decisions, observations, results / "proxy.jsonl", results / "commands.ndjson"
    )):
        raise SystemExit("request case requires fresh output paths")
    request = sys.stdin.buffer.read(MAX_REQUEST_BYTES + 1)
    if len(request) > MAX_REQUEST_BYTES:
        raise SystemExit("proxy request exceeds 1 MiB ceiling")

    mcp_bin = shutil.which("assay-mcp-server", path=child_environment().get("PATH"))
    if mcp_bin is None:
        raise SystemExit("assay-mcp-server is absent from the restricted PATH")

    argv = [
        mcp_bin,
        "proxy-enforce",
        "--upstream-command",
        sys.executable,
        "--upstream-arg",
        "-u",
        "--upstream-arg",
        str(fixture_root / "mock_github_mcp.py"),
        "--enforce-policy",
        str(fixture_root / "policies" / ("allow.yaml" if args.policy == "allow" else "no-allowance.yaml")),
        "--declared-mcp-manifest",
        str(fixture_root / "baseline-approved.json"),
        "--enforcement-decision-out",
        str(decisions),
        "--denied-call-observation-out",
        str(observations),
    ]
    expected_lines = len(case_request_ids(args.expect)) if args.expect else None
    status = run_proxy_child(
        argv,
        request,
        results / "proxy.jsonl",
        results / "proxy.stderr",
        expected_lines=expected_lines,
        timeout=args.timeout_seconds,
    )
    append_command_record(results / "commands.ndjson", status, argv)
    if status == 0 and args.expect:
        try:
            validate_case(results, args.expect)
        except (OSError, ValueError, RecursionError) as error:
            print(f"request case failed: {error}", file=sys.stderr)
            return 1
    return status if 0 <= status <= 255 else 125


if __name__ == "__main__":
    raise SystemExit(main())
