#!/usr/bin/env python3
"""Stdlib-only Linux benchmark pilot for MCP proxy-enforce cost (#2494)."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import random
import select
import statistics
import subprocess
import sys
import threading
import time
from typing import Any

try:
    import resource
except ModuleNotFoundError:
    resource = None

REPO_ROOT = Path(__file__).resolve().parents[3]
PUBLISHED_PROXY_PHASE_MODULE_PATH = REPO_ROOT / "scripts/ci/published_release_proxy_phase.py"
PROCESS_GROUP_HELPER_PATH = REPO_ROOT / "scripts/ci/lib/process_group.py"
DEFAULT_UPSTREAM_SCRIPT = REPO_ROOT / "crates/assay-mcp-server/tests/fixtures/proxy/mock_upstream.py"
DEFAULT_DECLARED_MANIFEST = REPO_ROOT / "examples/privileged-action-gate/baseline-approved.json"
DEFAULT_ALLOW_POLICY = REPO_ROOT / "examples/privileged-action-gate/policies/allow.yaml"
DEFAULT_DENY_POLICY = REPO_ROOT / "examples/privileged-action-gate/policies/no-allowance.yaml"

PINNED_RELEASE_TAG = "v6.14.1"
PINNED_RELEASE_SOURCE_SHA = "098ebbeb2efe19c3b8fdc6bc41250c80169cc06c"
PINNED_ARCHIVE_SHA256 = "013ec933ec036120044a4f2d0eba967ed2604ff8dab093d1a0e1866f01177ea2"
BOOTSTRAP_SEED = 24942026
EXPERIMENT_SLUG = "mcp-proxy-cost-2026-10"

FIXED_TOOL_NAME = "github.add_deploy_key"
FIXED_TOOL_ARGUMENTS = {
    "owner": "acme",
    "repo": "prod-app",
    "key": "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPilotFixedSyntheticKey",
    "read_only": False,
}
EXPECTED_TOOL_RESULT = {"content": [{"type": "text", "text": "forwarded-ok"}], "isError": False}
EXPECTED_ACTION_TARGET = {"provider": "github", "owner": "acme", "repo": "prod-app", "read_only": False}
EXPECTED_TARGET_DIGEST = "sha256:47827d295798df4eb0b4a6a7c49dbb999a8b0dd81cd12b132ae5e2f78895e35c"
FORBIDDEN_CREDENTIAL_SCOPE = "repo:deploy_key:write"


def _load_published_proxy_phase_module():
    spec = importlib.util.spec_from_file_location("assay_ci_published_release_proxy_phase", PUBLISHED_PROXY_PHASE_MODULE_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"unable to load {PUBLISHED_PROXY_PHASE_MODULE_PATH}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_PUBLISHED_PROXY_PHASE = _load_published_proxy_phase_module()
stop_process_group = _PUBLISHED_PROXY_PHASE.stop_process_group
minimal_environment = _PUBLISHED_PROXY_PHASE.minimal_environment


class PilotFailure(ValueError):
    """Raised when a pilot admission, execution, or verification guard fails."""

    def __init__(self, reason: str, message: str, receipt_path: Path | None = None, artifacts: dict[str, Any] | None = None):
        super().__init__(f"{reason}: {message}")
        self.reason = reason
        self.detail = message
        self.receipt_path = receipt_path
        self.artifacts = artifacts or {}


def _require(condition: bool, reason: str, detail: str, artifacts: dict[str, Any] | None = None) -> None:
    if not condition:
        raise PilotFailure(reason, detail, artifacts=artifacts)


@dataclass(frozen=True)
class PilotConfig:
    mcp_binary: Path
    expected_binary_sha256: str
    out_dir: Path
    blocks: int = 6
    calls_per_session: int = 5
    timeout_seconds: float = 30.0
    max_line_bytes: int = 65_536
    max_total_bytes: int = 1_048_576
    bootstrap_resamples: int = 2000
    bootstrap_seed: int = BOOTSTRAP_SEED
    upstream_script: Path = DEFAULT_UPSTREAM_SCRIPT
    declared_manifest: Path = DEFAULT_DECLARED_MANIFEST
    allow_policy: Path = DEFAULT_ALLOW_POLICY
    deny_policy: Path = DEFAULT_DENY_POLICY
    allow_non_linux: bool = False


def utc_now_iso() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def sha256_file(path: Path) -> str:
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    obj: dict[str, Any] = {}
    for key, val in pairs:
        _require(key not in obj, "duplicate_json_key", f"duplicate JSON key {key!r}")
        obj[key] = val
    return obj


def parse_json_line_strict(raw_line: bytes, context: str) -> dict[str, Any]:
    try:
        value = json.loads(raw_line.decode("utf-8"), object_pairs_hook=reject_duplicate_keys)
    except PilotFailure:
        raise
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise PilotFailure("malformed_json_line", f"{context}: {exc}") from exc
    _require(isinstance(value, dict), "malformed_json_line", f"{context}: not an object")
    return value


def read_bounded_lines(path: Path, max_line_bytes: int, max_total_bytes: int, context: str) -> list[bytes]:
    size = path.stat().st_size
    _require(size <= max_total_bytes, "total_byte_limit_exceeded", f"{context} size {size} exceeds ceiling {max_total_bytes}")
    lines: list[bytes] = []
    total = 0
    with path.open("rb") as handle:
        while True:
            raw = handle.readline(max_line_bytes + 1)
            if not raw:
                break
            total += len(raw)
            _require(total <= max_total_bytes, "total_byte_limit_exceeded", f"{context} exceeded total ceiling ({max_total_bytes} bytes)")
            _require(
                len(raw) <= max_line_bytes and (raw.endswith(b"\n") or len(raw) < max_line_bytes),
                "line_byte_limit_exceeded",
                f"{context} exceeded line ceiling ({max_line_bytes} bytes)",
            )
            lines.append(raw)
    return lines


class _BoundedStdoutReader:
    """Reads newline-delimited frames from a child stdout fd with byte and time ceilings."""

    def __init__(self, fd: int, max_line_bytes: int, max_total_bytes: int):
        self.fd = fd
        self.max_line_bytes = max_line_bytes
        self.max_total_bytes = max_total_bytes
        self.raw_bytes = bytearray()
        self._buffer = bytearray()
        self.eof = False

    def _fill_once(self, deadline_ns: int) -> None:
        remaining_s = (deadline_ns - time.monotonic_ns()) / 1_000_000_000.0
        if remaining_s <= 0:
            raise TimeoutError("session deadline exceeded while waiting for stdout")
        ready, _, _ = select.select([self.fd], [], [], min(remaining_s, 0.05))
        if not ready:
            if time.monotonic_ns() >= deadline_ns:
                raise TimeoutError("session deadline exceeded while waiting for stdout")
            return
        chunk = os.read(self.fd, 4096)
        if not chunk:
            self.eof = True
            return
        self.raw_bytes.extend(chunk)
        _require(len(self.raw_bytes) <= self.max_total_bytes, "total_byte_limit_exceeded", f"stdout exceeded ceiling ({self.max_total_bytes} bytes)")
        self._buffer.extend(chunk)
        nl = self._buffer.find(b"\n")
        check_len = nl + 1 if nl != -1 else len(self._buffer)
        _require(check_len <= self.max_line_bytes, "line_byte_limit_exceeded", f"stdout line exceeded ceiling ({self.max_line_bytes} bytes)")

    def read_line(self, deadline_ns: int) -> bytes | None:
        while True:
            nl = self._buffer.find(b"\n")
            if nl != -1:
                line = bytes(self._buffer[: nl + 1])
                del self._buffer[: nl + 1]
                _require(len(line) <= self.max_line_bytes, "line_byte_limit_exceeded", f"stdout line exceeded ceiling ({self.max_line_bytes} bytes)")
                return line
            if self.eof:
                if self._buffer:
                    line = bytes(self._buffer)
                    self._buffer.clear()
                    _require(len(line) <= self.max_line_bytes, "line_byte_limit_exceeded", f"stdout line exceeded ceiling ({self.max_line_bytes} bytes)")
                    return line
                return None
            self._fill_once(deadline_ns)

    def drain_until_eof(self, deadline_ns: int) -> list[bytes]:
        extra: list[bytes] = []
        while True:
            line = self.read_line(deadline_ns)
            if line is None:
                break
            if line.strip():
                extra.append(line)
        return extra


def _reap_with_wait4(proc: subprocess.Popen[bytes], deadline_ns: int) -> tuple[int, int | None]:
    while True:
        pid, status, rusage = os.wait4(proc.pid, os.WNOHANG)
        if pid == proc.pid:
            exit_code = os.waitstatus_to_exitcode(status)
            proc.returncode = exit_code
            rss_raw = getattr(rusage, "ru_maxrss", None)
            if rss_raw is None:
                return exit_code, None
            return exit_code, (int(rss_raw) if sys.platform == "linux" else int(rss_raw) // 1024)
        if time.monotonic_ns() >= deadline_ns:
            raise TimeoutError("session deadline exceeded while waiting for child exit")
        time.sleep(0.002)


def _median_and_bootstrap_ci(deltas: list[float], resamples: int, seed: int) -> dict[str, Any]:
    point_median = float(statistics.median(deltas))
    rng = random.Random(seed)
    n = len(deltas)
    boot = sorted(float(statistics.median(rng.choices(deltas, k=n))) for _ in range(resamples))
    lo_idx = max(0, min(resamples - 1, int(0.025 * resamples)))
    hi_idx = max(0, min(resamples - 1, int(0.975 * resamples) - 1))
    return {
        "block_count": n,
        "block_deltas": [round(val, 6) for val in deltas],
        "median_paired_delta": round(point_median, 6),
        "min_paired_delta": round(float(min(deltas)), 6),
        "max_paired_delta": round(float(max(deltas)), 6),
        "bootstrap_95_ci": [round(boot[lo_idx], 6), round(boot[hi_idx], 6)],
        "bootstrap_resamples": resamples,
        "bootstrap_seed": seed,
    }


def _build_requests(calls_per_session: int) -> list[tuple[int | None, str, bytes]]:
    frames: list[tuple[int | None, str, bytes]] = [
        (1, "initialize", b'{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"assay-proxy-cost-pilot","version":"0.1.0"}}}\n'),
        (None, "notifications/initialized", b'{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}\n'),
        (2, "tools/list", b'{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}\n'),
    ]
    for idx in range(1, calls_per_session + 1):
        req_id = 100 + idx
        payload = {"jsonrpc": "2.0", "id": req_id, "method": "tools/call", "params": {"name": FIXED_TOOL_NAME, "arguments": FIXED_TOOL_ARGUMENTS}}
        frames.append((req_id, "tools/call", (json.dumps(payload, separators=(",", ":")) + "\n").encode("utf-8")))
    return frames


def _run_interactive_session(
    config: PilotConfig,
    session_dir: Path,
    arm: str,
    block_index: int | None,
    order_in_block: int | None,
    calls_per_session: int,
    expect_deny: bool = False,
) -> dict[str, Any]:
    session_dir.mkdir(parents=True, exist_ok=False)
    requests_path = session_dir / "caller_requests.ndjson"
    stdout_path = session_dir / "caller_responses.ndjson"
    stderr_path = session_dir / "child_stderr.log"
    upstream_log_path = session_dir / "upstream_methods.log"
    upstream_raw_path = session_dir / "upstream_raw.jsonl"
    decisions_path = session_dir / "decisions.ndjson"
    denied_path = session_dir / "denied_observations.ndjson"

    frames = _build_requests(calls_per_session)
    sent_request_bytes = bytearray()

    env = minimal_environment()
    env.update({
        "PYTHONUNBUFFERED": "1",
        "MOCK_UPSTREAM_MODE": "p60a",
        "MOCK_UPSTREAM_LOG": str(upstream_log_path),
        "MOCK_UPSTREAM_RAW_LOG": str(upstream_raw_path),
    })

    if arm == "direct_stdio":
        cmd = [sys.executable, "-u", str(config.upstream_script)]
    else:
        policy_path = config.deny_policy if expect_deny else config.allow_policy
        cmd = [
            str(config.mcp_binary), "proxy-enforce",
            "--upstream-command", sys.executable,
            "--upstream-arg", "-u",
            "--upstream-arg", str(config.upstream_script),
            "--enforce-policy", str(policy_path),
            "--declared-mcp-manifest", str(config.declared_manifest),
            "--enforcement-decision-out", str(decisions_path),
            "--denied-call-observation-out", str(denied_path),
        ]

    artifacts_ref = {
        "session_dir": str(session_dir),
        "arm": arm,
        "block_index": block_index,
        "requests_path": str(requests_path),
        "stdout_path": str(stdout_path),
        "stderr_path": str(stderr_path),
        "upstream_methods_path": str(upstream_log_path),
        "upstream_raw_path": str(upstream_raw_path),
        "decisions_path": str(decisions_path) if arm != "direct_stdio" else None,
        "denied_path": str(denied_path) if expect_deny else None,
    }

    stderr_bytes = bytearray()
    stderr_overflow = False

    def _drain_stderr(fd: int) -> None:
        nonlocal stderr_overflow
        try:
            while chunk := os.read(fd, 4096):
                if len(stderr_bytes) + len(chunk) <= config.max_total_bytes:
                    stderr_bytes.extend(chunk)
                else:
                    stderr_bytes.extend(chunk[: max(0, config.max_total_bytes - len(stderr_bytes))])
                    stderr_overflow = True
        except (OSError, ValueError):
            pass

    def _child_preexec() -> None:
        if resource is not None:
            resource.setrlimit(resource.RLIMIT_FSIZE, (config.max_total_bytes, config.max_total_bytes))

    session_start_iso = utc_now_iso()
    t_start_ns = time.monotonic_ns()
    deadline_ns = t_start_ns + int(config.timeout_seconds * 1_000_000_000)

    proc = subprocess.Popen(
        cmd,
        cwd=REPO_ROOT,
        env=env,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=True,
        preexec_fn=_child_preexec if os.name == "posix" else None,
    )
    assert proc.stdin is not None and proc.stdout is not None and proc.stderr is not None
    stderr_thread = threading.Thread(target=_drain_stderr, args=(proc.stderr.fileno(),), daemon=True)
    stderr_thread.start()
    reader = _BoundedStdoutReader(proc.stdout.fileno(), config.max_line_bytes, config.max_total_bytes)

    response_lines: list[bytes] = []
    call_rtts_ms: list[float] = []
    startup_ms: float | None = None
    closure_flush_ms: float | None = None
    exit_code: int | None = None
    max_rss_kib: int | None = None

    try:
        early_eof = False
        for req_id, _method, raw_bytes in frames[:3]:
            proc.stdin.write(raw_bytes)
            proc.stdin.flush()
            sent_request_bytes.extend(raw_bytes)
            if req_id is not None:
                resp_line = reader.read_line(deadline_ns)
                if resp_line is None:
                    early_eof = True
                    break
                response_lines.append(resp_line)
        startup_ms = (time.monotonic_ns() - t_start_ns) / 1_000_000.0

        if not early_eof:
            for _req_id, _method, raw_bytes in frames[3:]:
                c0_ns = time.monotonic_ns()
                proc.stdin.write(raw_bytes)
                proc.stdin.flush()
                sent_request_bytes.extend(raw_bytes)
                resp_line = reader.read_line(deadline_ns)
                c1_ns = time.monotonic_ns()
                if resp_line is None:
                    break
                response_lines.append(resp_line)
                call_rtts_ms.append((c1_ns - c0_ns) / 1_000_000.0)

        f0_ns = time.monotonic_ns()
        proc.stdin.close()
        response_lines.extend(reader.drain_until_eof(deadline_ns))
        exit_code, max_rss_kib = _reap_with_wait4(proc, deadline_ns)
        closure_flush_ms = (time.monotonic_ns() - f0_ns) / 1_000_000.0
    except TimeoutError as exc:
        artifacts_ref["exit_code"] = 124
        raise PilotFailure("child_timeout", f"session {arm} timed out after {config.timeout_seconds}s: {exc}", artifacts=artifacts_ref) from exc
    except (BrokenPipeError, OSError):
        try:
            exit_code, max_rss_kib = _reap_with_wait4(proc, time.monotonic_ns() + 500_000_000)
        except TimeoutError:
            exit_code = proc.poll()
    except PilotFailure as exc:
        exc.artifacts = artifacts_ref
        raise
    finally:
        try:
            proc.stdin.close()
        except (BrokenPipeError, OSError, ValueError):
            pass
        stop_process_group(proc)
        pg_deadline = time.monotonic() + 0.5
        while time.monotonic() < pg_deadline:
            try:
                os.killpg(proc.pid, 0)
            except (ProcessLookupError, PermissionError):
                break
            time.sleep(0.005)
        stderr_thread.join(timeout=1.0)
        for stream in (proc.stdout, proc.stderr):
            try:
                stream.close()
            except (OSError, ValueError):
                pass
        requests_path.write_bytes(bytes(sent_request_bytes))
        stdout_path.write_bytes(bytes(reader.raw_bytes))
        stderr_path.write_bytes(bytes(stderr_bytes))

    _require(not stderr_overflow, "total_byte_limit_exceeded", f"session {arm} stderr exceeded total ceiling", artifacts_ref)
    if exit_code != 0:
        artifacts_ref["exit_code"] = exit_code
        raise PilotFailure("child_nonzero_exit", f"session {arm} exited with code {exit_code}", artifacts=artifacts_ref)

    parsed_responses = [parse_json_line_strict(line, f"{arm} stdout") for line in response_lines if line.strip()]
    seen_ids: set[int] = set()
    for resp in parsed_responses:
        _require(resp.get("jsonrpc") == "2.0", "malformed_jsonrpc_response", f"session {arm} response missing jsonrpc 2.0", artifacts_ref)
        rid = resp.get("id")
        _require(type(rid) is int, "response_id_mismatch", f"session {arm} response id is not an integer: {rid!r}", artifacts_ref)
        _require(rid not in seen_ids, "duplicate_response_id", f"session {arm} duplicate response id {rid!r}", artifacts_ref)
        seen_ids.add(rid)

    expected_ids = [1, 2, *[100 + idx for idx in range(1, calls_per_session + 1)]]
    _require(len(parsed_responses) == len(expected_ids), "response_count_mismatch", f"session {arm} response count {len(parsed_responses)} != {len(expected_ids)}", artifacts_ref)
    actual_ids = [resp.get("id") for resp in parsed_responses]
    _require(actual_ids == expected_ids, "response_id_mismatch", f"session {arm} response ids {actual_ids!r} != {expected_ids!r}", artifacts_ref)
    for hdr_resp in parsed_responses[:2]:
        _require(isinstance(hdr_resp.get("result"), dict) and "error" not in hdr_resp, "response_result_mismatch", f"session {arm} handshake response invalid", artifacts_ref)

    call_responses = parsed_responses[2:]
    if not expect_deny:
        for resp in call_responses:
            _require("error" not in resp and resp.get("result") == EXPECTED_TOOL_RESULT, "response_result_mismatch", f"session {arm} tools/call result mismatch", artifacts_ref)

    _require(
        upstream_log_path.is_file() and upstream_log_path.stat().st_size > 0 and upstream_raw_path.is_file() and upstream_raw_path.stat().st_size > 0,
        "missing_upstream_observation",
        f"session {arm} did not produce non-empty upstream observation logs",
        artifacts_ref,
    )
    method_lines = [
        line.decode("utf-8", errors="replace").strip()
        for line in read_bounded_lines(upstream_log_path, config.max_line_bytes, config.max_total_bytes, f"{arm} upstream_methods.log")
        if line.strip()
    ]
    upstream_raw_lines = read_bounded_lines(upstream_raw_path, config.max_line_bytes, config.max_total_bytes, f"{arm} upstream_raw.jsonl")
    upstream_msgs = [parse_json_line_strict(line, f"{arm} upstream_raw.jsonl") for line in upstream_raw_lines if line.strip()]
    for m in upstream_msgs:
        _require(m.get("jsonrpc") == "2.0" and isinstance(m.get("method"), str), "malformed_upstream_record", f"session {arm} upstream record invalid", artifacts_ref)
    raw_methods = [m.get("method") for m in upstream_msgs]
    _require(
        "initialize" in method_lines and "tools/list" in method_lines and "initialize" in raw_methods and "tools/list" in raw_methods,
        "missing_upstream_init_or_discovery",
        f"session {arm} upstream logs lacked positive initialize/tools/list records",
        artifacts_ref,
    )

    upstream_calls = [m for m in upstream_msgs if m.get("method") == "tools/call"]
    upstream_lists = [m for m in upstream_msgs if m.get("method") == "tools/list"]
    extra_upstream_lists = [m for m in upstream_lists if m.get("id") != 2]

    if expect_deny:
        _require(len(upstream_calls) == 0 and "tools/call" not in method_lines, "deny_forwarded_to_upstream", f"deny session forwarded {len(upstream_calls)} calls", artifacts_ref)
        return _validate_deny_artifacts(config, call_responses[0], response_lines[2], decisions_path, denied_path, len(extra_upstream_lists), artifacts_ref)

    _require(len(upstream_calls) == calls_per_session, "upstream_call_count_mismatch", f"session {arm} upstream calls {len(upstream_calls)} != {calls_per_session}", artifacts_ref)
    normalized_upstream_calls: list[dict[str, Any]] = []
    for idx, up_call in enumerate(upstream_calls, start=1):
        params = up_call.get("params")
        _require(
            type(up_call.get("id")) is int and up_call.get("id") == 100 + idx and isinstance(params, dict) and params.get("name") == FIXED_TOOL_NAME and params.get("arguments") == FIXED_TOOL_ARGUMENTS,
            "upstream_call_parity_mismatch",
            f"session {arm} upstream tools/call #{idx} semantics mismatch",
            artifacts_ref,
        )
        normalized_upstream_calls.append({"id": up_call.get("id"), "name": params.get("name"), "arguments": params.get("arguments")})

    evidence_bytes = decision_records_count = other_legitimate_count = 0
    if arm == "proxy_enforce_allow":
        evidence_bytes, decision_records_count, other_legitimate_count = _validate_allow_decision_evidence(config, decisions_path, calls_per_session, artifacts_ref)

    assert startup_ms is not None and closure_flush_ms is not None
    return {
        "arm": arm,
        "block_index": block_index,
        "order_in_block": order_in_block,
        "started_at": session_start_iso,
        "completed_at": utc_now_iso(),
        "exit_code": exit_code,
        "startup_ms": round(startup_ms, 6),
        "in_session_call_rtts_ms": [round(val, 6) for val in call_rtts_ms],
        "in_session_call_median_ms": round(float(statistics.median(call_rtts_ms)), 6),
        "in_session_call_total_ms": round(float(sum(call_rtts_ms)), 6),
        "closure_flush_ms": round(closure_flush_ms, 6),
        "wait4_max_rss_kib": max_rss_kib,
        "wait4_max_rss_scope": "maximum_single_process_high_water",
        "caller_request_bytes": len(sent_request_bytes),
        "caller_response_bytes": len(reader.raw_bytes),
        "child_stderr_bytes": len(stderr_bytes),
        "upstream_fixture_normalized_bytes": upstream_raw_path.stat().st_size,
        "upstream_observation_mode": "fixture_strip_normalized_jsonl",
        "evidence_bytes": evidence_bytes,
        "decision_evidence_records": decision_records_count,
        "other_legitimate_evidence_records": other_legitimate_count,
        "upstream_tools_call_count": len(upstream_calls),
        "extra_upstream_tools_list_count": len(extra_upstream_lists),
        "_parity_signature": {"upstream_calls": normalized_upstream_calls, "caller_results": [resp.get("result") for resp in call_responses]},
    }


def _validate_allow_decision_evidence(
    config: PilotConfig, decisions_path: Path, calls_per_session: int, artifacts_ref: dict[str, Any]
) -> tuple[int, int, int]:
    _require(decisions_path.is_file() and decisions_path.stat().st_size > 0, "missing_decision_evidence", "missing decisions.ndjson after flush", artifacts_ref)
    try:
        raw_lines = read_bounded_lines(decisions_path, config.max_line_bytes, config.max_total_bytes, "decisions.ndjson")
        records = [parse_json_line_strict(line, "decisions.ndjson") for line in raw_lines if line.strip()]
    except PilotFailure as exc:
        if exc.reason == "malformed_json_line":
            raise PilotFailure("malformed_decision_evidence", exc.detail, artifacts=artifacts_ref) from exc
        exc.artifacts = artifacts_ref
        raise

    decision_records: list[dict[str, Any]] = []
    other_legitimate = 0
    for rec in records:
        schema = rec.get("schema")
        if schema == "assay.enforcement_decision.v0":
            decision_records.append(rec)
        elif isinstance(schema, str) and schema.startswith("assay."):
            other_legitimate += 1
        else:
            raise PilotFailure("unexpected_evidence_schema", f"unexpected record schema {schema!r}", artifacts=artifacts_ref)

    _require(len(decision_records) == calls_per_session, "decision_evidence_count_mismatch", f"decisions.ndjson count {len(decision_records)} != {calls_per_session}", artifacts_ref)
    for rec in decision_records:
        raw_serialized = json.dumps(rec, sort_keys=True)
        action = rec.get("action")
        tool = rec.get("tool")
        caller = rec.get("caller")
        _require(
            FORBIDDEN_CREDENTIAL_SCOPE not in raw_serialized
            and "forwarded" not in rec
            and isinstance(action, dict)
            and isinstance(tool, dict)
            and isinstance(caller, dict)
            and isinstance(rec.get("non_claims"), list)
            and rec.get("decision") == "allow"
            and rec.get("reason") == "allow"
            and rec.get("fail_closed") is False
            and rec.get("drift_state") == "satisfied"
            and rec.get("credential_alias") == "gh-deploy"
            and caller == {"id": "ci-agent"}
            and tool == {"name": FIXED_TOOL_NAME, "action_class": "github_deploy_key"}
            and action.get("verb") == "create"
            and action.get("resource_type") == "github_deploy_key"
            and action.get("target") == EXPECTED_ACTION_TARGET
            and action.get("target_digest") == EXPECTED_TARGET_DIGEST,
            "invalid_decision_evidence_fields",
            f"decision record did not match allow contract: {rec!r}",
            artifacts_ref,
        )
    return decisions_path.stat().st_size, len(decision_records), other_legitimate


def _validate_deny_artifacts(
    config: PilotConfig,
    call_response: dict[str, Any],
    raw_call_response_line: bytes,
    decisions_path: Path,
    denied_path: Path,
    extra_upstream_lists: int,
    artifacts_ref: dict[str, Any],
) -> dict[str, Any]:
    error = call_response.get("error")
    _require(isinstance(error, dict), "deny_missing_error_response", "deny session did not return JSON-RPC error", artifacts_ref)
    data = error.get("data")
    _require(
        error.get("code") == -31999
        and isinstance(error.get("message"), str)
        and "no_declared_allowance" in error["message"]
        and isinstance(data, dict)
        and data == {"origin": "assay-proxy", "reason": "no_declared_allowance"},
        "deny_error_contract_mismatch",
        f"deny error payload mismatch: {error!r}",
        artifacts_ref,
    )
    _require(decisions_path.is_file() and decisions_path.stat().st_size > 0, "missing_decision_evidence", "deny session did not write decisions.ndjson", artifacts_ref)
    decision_recs = [
        parse_json_line_strict(line, "deny decisions.ndjson")
        for line in read_bounded_lines(decisions_path, config.max_line_bytes, config.max_total_bytes, "deny decisions.ndjson")
        if line.strip()
    ]
    deny_decisions = [r for r in decision_recs if r.get("schema") == "assay.enforcement_decision.v0"]
    _require(len(deny_decisions) == 1, "invalid_deny_decision_evidence", f"expected 1 deny decision, got {len(deny_decisions)}", artifacts_ref)
    d_rec = deny_decisions[0]
    d_action = d_rec.get("action")
    d_tool = d_rec.get("tool")
    d_caller = d_rec.get("caller")
    _require(
        isinstance(d_action, dict)
        and isinstance(d_tool, dict)
        and isinstance(d_caller, dict)
        and isinstance(d_rec.get("non_claims"), list)
        and d_rec.get("decision") == "deny"
        and d_rec.get("reason") == "no_declared_allowance"
        and d_rec.get("fail_closed") is True
        and d_rec.get("drift_state") == "not_evaluated"
        and d_rec.get("credential_alias") == "gh-deploy"
        and d_caller == {"id": "ci-agent"}
        and d_tool == {"name": FIXED_TOOL_NAME, "action_class": "github_deploy_key"}
        and d_action.get("verb") == "create"
        and d_action.get("resource_type") == "github_deploy_key"
        and d_action.get("target") == EXPECTED_ACTION_TARGET
        and d_action.get("target_digest") == EXPECTED_TARGET_DIGEST,
        "invalid_deny_decision_evidence",
        f"deny decisions.ndjson invalid: {d_rec!r}",
        artifacts_ref,
    )
    _require(denied_path.is_file() and denied_path.stat().st_size > 0, "missing_denied_call_observation", "deny session did not write denied_observations.ndjson", artifacts_ref)
    denied_recs = [
        parse_json_line_strict(line, "denied_observations.ndjson")
        for line in read_bounded_lines(denied_path, config.max_line_bytes, config.max_total_bytes, "denied_observations.ndjson")
        if line.strip()
    ]
    expected_resp_digest = "sha256:" + hashlib.sha256(raw_call_response_line.rstrip(b"\r\n")).hexdigest()
    _require(
        len(denied_recs) == 1
        and denied_recs[0].get("schema") == "assay.denied_call_observation.v1"
        and denied_recs[0].get("call") == {"tool_name": FIXED_TOOL_NAME, "target_digest": EXPECTED_TARGET_DIGEST}
        and denied_recs[0].get("caller_visible_error") == {"code": -31999, "origin": "assay-proxy", "reason": "no_declared_allowance"}
        and denied_recs[0].get("caller_visible_response_digest") == expected_resp_digest
        and isinstance(denied_recs[0].get("non_claims"), list),
        "invalid_denied_call_observation",
        f"denied_observations.ndjson mismatch: {denied_recs!r}",
        artifacts_ref,
    )
    return {
        "status": "passed",
        "upstream_tools_call_count": 0,
        "extra_upstream_tools_list_count": extra_upstream_lists,
        "error_code": -31999,
        "reason": "no_declared_allowance",
        "decision_evidence_bytes": decisions_path.stat().st_size,
        "denied_observation_bytes": denied_path.stat().st_size,
        "caller_visible_response_digest": expected_resp_digest,
    }


def _write_failure_artifacts(
    config: PilotConfig, started_at: str, reason: str, detail: str, valid_blocks: int, failed_artifacts: dict[str, Any] | None = None
) -> Path:
    receipt_path = config.out_dir / "receipt.json"
    summary_path = config.out_dir / "summary.json"
    payload = {
        "experiment": EXPERIMENT_SLUG,
        "status": "failed",
        "execution_valid": False,
        "started_at": started_at,
        "completed_at": utc_now_iso(),
        "valid_blocks": valid_blocks,
        "failure_count": 1,
        "failure_reason": reason,
        "failure_detail": detail,
        "failed_session_artifacts": failed_artifacts or {},
    }
    serialized = json.dumps(payload, indent=2, sort_keys=True) + "\n"
    receipt_path.write_text(serialized, encoding="utf-8")
    summary_path.write_text(serialized, encoding="utf-8")
    return receipt_path


def _validate_config_and_inputs(config: PilotConfig) -> dict[str, str]:
    _require(config.allow_non_linux or sys.platform == "linux", "unsupported_platform", f"requires sys.platform == 'linux' (got {sys.platform!r})")
    _require(config.mcp_binary.is_absolute(), "mcp_binary_not_absolute", f"--mcp-binary must be absolute: {config.mcp_binary}")
    expected_hex = config.expected_binary_sha256.lower()
    _require(len(expected_hex) == 64 and all(ch in "0123456789abcdef" for ch in expected_hex), "invalid_expected_binary_sha256", "expected 64-char hex sha256")
    _require(
        1 <= config.blocks <= 64
        and 1 <= config.calls_per_session <= 100
        and math.isfinite(config.timeout_seconds)
        and 0 < config.timeout_seconds <= 300
        and 512 <= config.max_line_bytes <= config.max_total_bytes <= 16_777_216
        and 1 <= config.bootstrap_resamples <= 10_000,
        "invalid_bounds",
        "numeric execution bounds outside permitted limits",
    )
    _require(config.mcp_binary.is_file() and os.access(config.mcp_binary, os.X_OK), "mcp_binary_not_executable", f"not executable: {config.mcp_binary}")
    actual_binary_sha = sha256_file(config.mcp_binary)
    _require(actual_binary_sha == expected_hex, "binary_sha256_mismatch", f"mcp_binary sha256 {actual_binary_sha} != expected {expected_hex}")

    digests = {
        "mcp_binary_sha256": actual_binary_sha,
        "pilot_script_sha256": sha256_file(Path(__file__).resolve()),
        "published_proxy_helper_sha256": sha256_file(PUBLISHED_PROXY_PHASE_MODULE_PATH),
        "process_group_helper_sha256": sha256_file(PROCESS_GROUP_HELPER_PATH),
    }
    for label, path in (
        ("upstream_script", config.upstream_script),
        ("declared_manifest", config.declared_manifest),
        ("allow_policy", config.allow_policy),
        ("deny_policy", config.deny_policy),
    ):
        _require(path.is_file(), "missing_input_fixture", f"required input {label} missing: {path}")
        _require(path.stat().st_size <= config.max_total_bytes, "total_byte_limit_exceeded", f"input {label} exceeds total byte ceiling")
        digests[f"{label}_sha256"] = sha256_file(path)
    return digests


def run_pilot(config: PilotConfig) -> dict[str, Any]:
    _require(not config.out_dir.exists(), "output_dir_not_exclusive", f"--out-dir must not already exist: {config.out_dir}")
    config.out_dir.mkdir(parents=True, exist_ok=False)
    started_at = utc_now_iso()
    completed_blocks: list[dict[str, Any]] = []

    try:
        digests = _validate_config_and_inputs(config)
        sessions_root = config.out_dir / "sessions"
        sessions_root.mkdir(parents=True, exist_ok=False)

        deny_summary = _run_interactive_session(config, sessions_root / "deny_check", "proxy_enforce_deny", None, None, 1, expect_deny=True)
        (config.out_dir / "deny_check.json").write_text(json.dumps(deny_summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")

        blocks_order: list[str] = []
        with (config.out_dir / "blocks.jsonl").open("w", encoding="utf-8") as blocks_handle:
            for block_idx in range(config.blocks):
                order_tag = "AB" if block_idx % 2 == 0 else "BA"
                blocks_order.append(order_tag)
                arm_sequence = ["direct_stdio", "proxy_enforce_allow"] if order_tag == "AB" else ["proxy_enforce_allow", "direct_stdio"]
                block_dir = sessions_root / f"block_{block_idx:02d}_{order_tag}"
                block_dir.mkdir(parents=True, exist_ok=False)

                arm_results = {
                    arm_name: _run_interactive_session(config, block_dir / f"{pos}_{arm_name}", arm_name, block_idx, pos, config.calls_per_session)
                    for pos, arm_name in enumerate(arm_sequence)
                }
                direct_sig = arm_results["direct_stdio"].pop("_parity_signature")
                proxy_sig = arm_results["proxy_enforce_allow"].pop("_parity_signature")
                _require(direct_sig == proxy_sig, "cross_arm_parity_mismatch", f"block {block_idx} work semantics diverged")

                direct_res, proxy_res = arm_results["direct_stdio"], arm_results["proxy_enforce_allow"]
                deltas = {
                    "startup_ms": round(proxy_res["startup_ms"] - direct_res["startup_ms"], 6),
                    "in_session_call_median_ms": round(proxy_res["in_session_call_median_ms"] - direct_res["in_session_call_median_ms"], 6),
                    "in_session_call_total_ms": round(proxy_res["in_session_call_total_ms"] - direct_res["in_session_call_total_ms"], 6),
                    "closure_flush_ms": round(proxy_res["closure_flush_ms"] - direct_res["closure_flush_ms"], 6),
                    "evidence_bytes_per_session": proxy_res["evidence_bytes"] - direct_res["evidence_bytes"],
                }
                block_record = {
                    "block_index": block_idx,
                    "order": order_tag,
                    "direct_stdio": direct_res,
                    "proxy_enforce_allow": proxy_res,
                    "paired_delta_proxy_minus_direct": deltas,
                }
                completed_blocks.append(block_record)
                blocks_handle.write(json.dumps(block_record, sort_keys=True) + "\n")

        _require(sha256_file(config.mcp_binary) == digests["mcp_binary_sha256"], "binary_continuity_mismatch", "mcp_binary SHA-256 changed during pilot")

        metric_keys = ("startup_ms", "in_session_call_median_ms", "in_session_call_total_ms", "closure_flush_ms", "evidence_bytes_per_session")
        paired_metrics = {
            key: _median_and_bootstrap_ci(
                [float(b["paired_delta_proxy_minus_direct"][key]) for b in completed_blocks],
                resamples=config.bootstrap_resamples,
                seed=config.bootstrap_seed + offset,
            )
            for offset, key in enumerate(metric_keys)
        }
        direct_rss = [b["direct_stdio"]["wait4_max_rss_kib"] for b in completed_blocks if b["direct_stdio"]["wait4_max_rss_kib"] is not None]
        proxy_rss = [b["proxy_enforce_allow"]["wait4_max_rss_kib"] for b in completed_blocks if b["proxy_enforce_allow"]["wait4_max_rss_kib"] is not None]

        completed_at = utc_now_iso()
        summary = {
            "experiment": EXPERIMENT_SLUG,
            "status": "completed",
            "execution_valid": True,
            "started_at": started_at,
            "completed_at": completed_at,
            "blocks": config.blocks,
            "valid_blocks": len(completed_blocks),
            "failure_count": 0,
            "calls_per_session": config.calls_per_session,
            "blocks_order": blocks_order,
            "provenance": {
                **digests,
                "mcp_binary_path": str(config.mcp_binary),
                "binary_continuity_verified": True,
                "operator_declared_release_metadata": {
                    "release_tag": PINNED_RELEASE_TAG,
                    "release_source_sha": PINNED_RELEASE_SOURCE_SHA,
                    "archive_sha256": PINNED_ARCHIVE_SHA256,
                    "authenticated_by_harness": False,
                    "verification_note": "Operator-declared pin; archive provenance and attestation are verified externally.",
                },
                "host": {
                    "platform": sys.platform,
                    "machine": platform.machine(),
                    "kernel_release": platform.release(),
                    "cpu_count_logical": os.cpu_count(),
                    "python_version": platform.python_version(),
                },
            },
            "paired_block_metrics": paired_metrics,
            "rss_memory_scope": {
                "label": "maximum_single_process_high_water",
                "comparable_across_arms": False,
                "combined_process_tree_claimed": False,
                "direct_stdio_median_wait4_max_rss_kib": int(statistics.median(direct_rss)) if direct_rss else None,
                "proxy_enforce_allow_median_wait4_max_rss_kib": int(statistics.median(proxy_rss)) if proxy_rss else None,
                "note": "Linux wait4 ru_maxrss reflects the maximum single-process resident high-water mark, never a combined process-tree sum.",
            },
            "deny_check_status": deny_summary["status"],
            "non_claims": [
                "execution_valid records harness and protocol check completion only, not an automatic publishable conclusion.",
                "Measures only single-host local stdio JSON-RPC framing and policy enforcement on a fixed synthetic upstream fixture.",
                "Upstream JSONL log is strip-normalized by the fixture (fixture_strip_normalized_jsonl); exact wire bytes are retained on the caller side.",
                "Does not claim tail-latency SLAs (p95/p99), multi-client concurrency, combined process-tree memory, or zero-overhead equivalence.",
            ],
        }
        receipt = {
            "experiment": EXPERIMENT_SLUG,
            "status": "completed",
            "execution_valid": True,
            "started_at": started_at,
            "completed_at": completed_at,
            "valid_blocks": len(completed_blocks),
            "failure_count": 0,
            "provenance": summary["provenance"],
        }
        (config.out_dir / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        (config.out_dir / "receipt.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        return summary
    except (OSError, subprocess.SubprocessError, RuntimeError) as exc:
        if isinstance(exc, PilotFailure):
            raise
        receipt_path = _write_failure_artifacts(config, started_at, "execution_io_error", str(exc), len(completed_blocks))
        raise PilotFailure("execution_io_error", str(exc), receipt_path=receipt_path) from exc
    except PilotFailure as exc:
        exc.receipt_path = _write_failure_artifacts(config, started_at, exc.reason, exc.detail, len(completed_blocks), exc.artifacts)
        raise


def parse_args(argv: list[str] | None = None) -> PilotConfig:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mcp-binary", type=Path, required=True)
    parser.add_argument("--expected-binary-sha256", required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--blocks", type=int, default=6)
    parser.add_argument("--calls-per-session", type=int, default=5)
    parser.add_argument("--timeout-seconds", type=float, default=30.0)
    parser.add_argument("--max-line-bytes", type=int, default=65_536)
    parser.add_argument("--max-total-bytes", type=int, default=1_048_576)
    parser.add_argument("--bootstrap-resamples", type=int, default=2000)
    parser.add_argument("--allow-non-linux", action="store_true")
    ns = parser.parse_args(argv)
    return PilotConfig(
        mcp_binary=ns.mcp_binary,
        expected_binary_sha256=ns.expected_binary_sha256,
        out_dir=ns.out_dir,
        blocks=ns.blocks,
        calls_per_session=ns.calls_per_session,
        timeout_seconds=ns.timeout_seconds,
        max_line_bytes=ns.max_line_bytes,
        max_total_bytes=ns.max_total_bytes,
        bootstrap_resamples=ns.bootstrap_resamples,
        allow_non_linux=ns.allow_non_linux,
    )


def main(argv: list[str] | None = None) -> int:
    try:
        run_pilot(parse_args(argv))
        return 0
    except (PilotFailure, ValueError) as exc:
        print(f"proxy_cost_pilot failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
