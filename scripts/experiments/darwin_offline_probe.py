#!/usr/bin/env python3
"""Measured sandbox-exec feasibility for TCP loopback and one external TCP connect.

This file does not satisfy the launch definition. A first-time user still has to
install the pinned published release by the documented command, run the nine
golden-path steps with their documented stdout and exits, produce one evidence
bundle and verify it offline, and read docs that are true of that binary. The
support commitment remains the current published release. Nothing here is a
five-platform launch pass, universal network isolation, or a claim that the
profile language is a supported product API. Unknown evidence stays inconclusive.

Packet-filter isolation is outside this file. Self-test performs no network,
process, or host change. Hosted execution refuses before acquisition unless
--hosted is set and the process is a verified GitHub-hosted Darwin runner.
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path
from typing import Any, Callable

# One published release pin. Install and invocation both read this constant.
RELEASE_TAG = "v6.6.2"
# One cosign release pin. The workflow prints it; verify-blob checks it.
COSIGN_RELEASE = "v3.1.3"
OIDC_ISSUER = "https://token.actions.githubusercontent.com"
REPORT_SCHEMA = "assay.privileged_mcp_action.verify.report.v0"
PHASES = ("P1-before", "P0", "P1-after")
MEASURED_OPERATIONS = ("tcp_loopback", "tcp_external")
RUNNER_LABELS = {"macos-26": "ARM64", "macos-26-intel": "X64"}
ARCHIVE_MAX_BYTES = 64 * 1024 * 1024
MANIFEST_MAX_BYTES = 65536
BUNDLE_MAX_BYTES = 1024 * 1024
OUTPUT_MAX_BYTES = 65536
RECEIPT_MAX_BYTES = 262144
PLACEHOLDER_BUNDLE = b"not-a-valid-bundle"
DECODED_MAX_BYTES = 256 * 1024 * 1024
IMPORTED_BUNDLE_MAX_BYTES = 8 * 1024 * 1024
# The decisions-only import in published-release-historical-retention.sh. The golden
# path also passes --denied-observations, which the proxy writes and which needs
# assay-mcp-server. That binary has no Darwin archive, so it is not this route.
DECISION_NDJSON = (
    '{"schema":"assay.enforcement_decision.v0","caller":{"id":"ci-agent"},"tool":{"name":"github.add_deploy_key","action_class":"github_deploy_key"},"action":{"verb":"create","resource_type":"github_deploy_key","target":{"provider":"github","owner":"acme"},"target_digest":"sha256:df4be9dfaa840f625ba03f5d577e6276a732f565c9527521138cfee1874546cf"},"decision":"deny","reason":"classification_incomplete","fail_closed":true,"drift_state":"not_evaluated","credential_alias":"gh-deploy","non_claims":["policy decision only; does not assert or verify the upstream side effect (stays asserted, E9 ladder)","an allow is the decision to forward; it does not assert the call reached or was performed by the upstream (a transport failure surfaces as proxy_failed, not here)","credential referenced by alias only, never the token or declared scopes","deny is fail-closed caution and allow is a policy decision — neither is a maliciousness verdict","not the observation artifact (assay.mcp_manifest_observed.v0) and not the mechanism artifact (assay.enforcement_health.v0)"]}\n'
).encode()
CHILD_DEADLINE_S = 20
CLEANUP_DEADLINE_S = 2
NETWORK_DEADLINE_S = 5
HOST_EFFECTS = {"fetch": 0, "sandbox": 0, "network": 0, "unlink": 0}


class Refuse(Exception):
    """Fail closed before acquisition, execution, or a completed pass."""


class Verdict(tuple):
    def __new__(cls, verdict: str, reason: str) -> "Verdict":
        if verdict not in {"PASS", "MECHANISM_FAILS", "INCONCLUSIVE"}:
            raise ValueError("verdict")
        return tuple.__new__(cls, (verdict, reason))

    @property
    def verdict(self) -> str:
        return self[0]

    @property
    def reason(self) -> str:
        return self[1]


def _ops_by_name(observations: Any) -> dict[str, dict[str, Any]] | None:
    if not isinstance(observations, list):
        return None
    found: dict[str, dict[str, Any]] = {}
    for item in observations:
        if not isinstance(item, dict) or not isinstance(item.get("op"), str):
            return None
        found[item["op"]] = item
    return found


def _nonempty(value: Any) -> bool:
    return isinstance(value, str) and bool(value)


def _context_reason(context: Any) -> str | None:
    if not isinstance(context, dict):
        return "context"
    if context.get("proc_translated") == 1:
        return "translated"
    if context.get("proc_translated") != 0:
        return "architecture"
    arch = context.get("runner_arch")
    uname_m = context.get("uname_m")
    if arch == "ARM64" and (uname_m != "arm64" or context.get("translation_known") is not True):
        return "architecture"
    if arch == "X64" and uname_m != "x86_64":
        return "architecture"
    if RUNNER_LABELS.get(context.get("expect_label")) != arch:
        return "runner_label"
    if context.get("runner_environment") != "github-hosted" or context.get("runner_os") != "macOS":
        return "context"
    if context.get("cosign_release") != COSIGN_RELEASE:
        return "cosign_pin"
    required = (
        "os_product",
        "image_os",
        "image_version",
        "sandbox_exec_listing",
        "python_version",
        "sw_vers",
    )
    if any(not _nonempty(context.get(key)) for key in required):
        return "context"
    return None


def _paths_reason(paths: Any) -> str | None:
    if not isinstance(paths, dict):
        return "paths"
    try:
        sentinel = _sbpl_literal(paths.get("sentinel", ""))
        decoy = _sbpl_literal(paths.get("decoy", ""))
        real = _sbpl_literal(paths.get("real_bundle", ""))
    except Refuse:
        return "paths"
    if len({sentinel, decoy, real}) != 3:
        return "paths"
    return None


def _endpoint_reason(endpoints: Any) -> str | None:
    if not isinstance(endpoints, dict):
        return "endpoints"
    loopback = endpoints.get("tcp_loopback")
    external = endpoints.get("tcp_external")
    if not isinstance(loopback, str) or not isinstance(external, str):
        return "endpoints"
    if loopback.split(":")[0] != "127.0.0.1" or external.split(":")[0] in {"127.0.0.1", "localhost"}:
        return "endpoints"
    for value in (loopback, external):
        host, sep, port = value.rpartition(":")
        if not sep or not port.isdigit() or host.count(".") != 3:
            return "endpoints"
        if any(not octet.isdigit() or not 0 <= int(octet) <= 255 for octet in host.split(".")):
            return "endpoints"
    return None


def _report_success(record: Any) -> bool:
    if not isinstance(record, dict) or record.get("exit") != 0 or not isinstance(record.get("stdout"), str):
        return False
    try:
        data = json.loads(record["stdout"])
    except json.JSONDecodeError:
        return False
    return (
        isinstance(data, dict)
        and data.get("schema") == REPORT_SCHEMA
        and data.get("bundle_integrity") == "pass"
        and data.get("verdict") == "valid"
        and "reason_code" not in data
    )


def _scope_before_network(observations: Any) -> bool:
    found = _ops_by_name(observations)
    if found is None:
        return False
    seen: set[str] = set()
    for item in observations:
        op = item["op"]
        if op in MEASURED_OPERATIONS and {"sentinel_write", "decoy_read"} - seen:
            return False
        if op in {"sentinel_write", "decoy_read"}:
            seen.add(op)
    return {"sentinel_write", "decoy_read", *MEASURED_OPERATIONS} <= set(found)


def _scope_denied(observations: list[dict[str, Any]]) -> bool:
    found = _ops_by_name(observations) or {}
    for op in ("sentinel_write", "decoy_read"):
        item = found.get(op) or {}
        if item.get("result") != "deny" or item.get("errno") != "EPERM":
            return False
    return True


def _network_state(phase: dict[str, Any]) -> str:
    found = _ops_by_name(phase.get("observations")) or {}
    rows = [found.get(name) for name in MEASURED_OPERATIONS]
    if any(row is None for row in rows):
        return "incomplete"
    if all(row.get("result") == "connect" for row in rows):
        return "connects"
    if any(row.get("result") == "connect" for row in rows):
        return "partial"
    if all(row.get("result") == "deny" and _nonempty(row.get("errno")) for row in rows):
        return "denies"
    return "incomplete"


def _shape_reason(phases: dict[str, Any], endpoints: dict[str, str], paths: dict[str, str]) -> str | None:
    tails: list[tuple[str, ...]] = []
    expected_paths = {"sentinel": paths["sentinel"], "decoy": paths["decoy"]}
    for name in PHASES:
        phase = phases[name]
        argv = phase.get("argv")
        if not isinstance(argv, list) or argv[:2] != ["/usr/bin/sandbox-exec", "-p"]:
            return "profile_mismatch"
        if argv[2] != phase.get("profile"):
            return "profile_mismatch"
        tail = tuple(argv[3:])
        tails.append(tail)
        if phase.get("probe") != list(tail):
            return "probe_drift"
        if phase.get("endpoints") != endpoints:
            return "endpoint_drift"
        if phase.get("paths") != expected_paths:
            return "path_drift"
    if len(set(tails)) != 1:
        return "probe_drift"
    return None


def _verifier_reason(verifier: Any, paths: dict[str, str]) -> str | None:
    if not isinstance(verifier, dict):
        return "missing_verifier_scope"
    keys = ("decoy_host", "decoy_p0", "decoy_p1", "real_connected", "real_p0")
    if any(key not in verifier for key in keys):
        return "missing_verifier_scope"
    host = verifier["decoy_host"]
    if host.get("bundle") != paths["decoy"] or host.get("readable") is not True or host.get("valid_bundle") is not True:
        return "verifier_not_scope_proof"
    if not _report_success(host):
        return "verifier_not_scope_proof"
    success = verifier["real_connected"]
    isolated = verifier["real_p0"]
    if success.get("bundle") != paths["real_bundle"] or isolated.get("bundle") != paths["real_bundle"]:
        return "missing_verifier_scope"
    if not _report_success(success) or not _report_success(isolated):
        return "verifier_semantics"
    if success.get("stdout") != isolated.get("stdout"):
        return "verifier_bytes"
    for key in ("decoy_p0", "decoy_p1"):
        denial = verifier[key]
        if not isinstance(denial, dict) or denial.get("bundle") != paths["decoy"]:
            return "missing_verifier_scope"
        if denial.get("read_errno") != "EPERM" or denial.get("exit") == 0 or _report_success(denial):
            return "missing_verifier_scope"
        stdout = denial.get("stdout") if isinstance(denial.get("stdout"), str) else ""
        stderr = denial.get("stderr") if isinstance(denial.get("stderr"), str) else ""
        success_stdout = success.get("stdout")
        # The published verifier prints the stage-1 report on stdout. Empty output,
        # and output equal to the connected success report, are not a denial.
        if (not stdout and not stderr) or stdout == success_stdout or (stderr != "" and stderr == success_stdout):
            return "verifier_error_output"
    return None


def _grandchild_verdict(grandchild: Any) -> Verdict | None:
    if not isinstance(grandchild, dict) or grandchild.get("present") is not True or grandchild.get("phase") != "P0":
        return Verdict("INCONCLUSIVE", "missing_grandchild")
    if grandchild.get("sentinel_write") != "deny" or grandchild.get("errno") != "EPERM":
        return Verdict("INCONCLUSIVE", "missing_grandchild")
    if grandchild.get("tcp_external") == "connect":
        return Verdict("MECHANISM_FAILS", "grandchild_not_denied")
    if grandchild.get("tcp_external") != "deny":
        return Verdict("INCONCLUSIVE", "missing_grandchild")
    return None


def _fixtures_unchanged(fixtures: Any) -> bool:
    if not isinstance(fixtures, dict):
        return False
    pairs = (("decoy_sha256_before", "decoy_sha256_after"), ("real_sha256_before", "real_sha256_after"))
    for before_key, after_key in pairs:
        before = fixtures.get(before_key)
        after = fixtures.get(after_key)
        if not isinstance(before, str) or before != after or len(before) != 64:
            return False
        if any(char not in "0123456789abcdef" for char in before):
            return False
    return True


def evaluate(receipts: Any) -> Verdict:
    """Pure verdict over already collected receipts. No I/O."""
    if not isinstance(receipts, dict):
        return Verdict("INCONCLUSIVE", "malformed")
    if receipts.get("timed_out") is True:
        return Verdict("INCONCLUSIVE", "timeout")
    context_reason = _context_reason(receipts.get("context"))
    if context_reason:
        return Verdict("INCONCLUSIVE", context_reason)
    if receipts.get("release_tag") != RELEASE_TAG:
        return Verdict("INCONCLUSIVE", "release_pin")
    if receipts.get("measured_operations") != list(MEASURED_OPERATIONS):
        return Verdict("INCONCLUSIVE", "operations")
    paths = receipts.get("paths")
    paths_reason = _paths_reason(paths)
    if paths_reason:
        return Verdict("INCONCLUSIVE", paths_reason)
    endpoints = receipts.get("endpoints")
    endpoint_reason = _endpoint_reason(endpoints)
    if endpoint_reason:
        return Verdict("INCONCLUSIVE", endpoint_reason)
    phases = receipts.get("phases")
    if not isinstance(phases, dict) or any(name not in phases or not isinstance(phases[name], dict) for name in PHASES):
        return Verdict("INCONCLUSIVE", "phases")
    for name in PHASES:
        phase = phases[name]
        if phase.get("timed_out") is True:
            return Verdict("INCONCLUSIVE", "timeout")
        if phase.get("profile_executed") is not True:
            return Verdict("INCONCLUSIVE", "profile_setup")
    shape = _shape_reason(phases, endpoints, paths)
    if shape:
        return Verdict("INCONCLUSIVE", shape)
    canonical = build_profiles(paths["sentinel"], paths["decoy"])
    p0_profile = phases["P0"]["profile"]
    p1_profile = phases["P1-before"]["profile"]
    if phases["P1-after"]["profile"] != p1_profile or p1_profile != canonical["P1-before"]:
        return Verdict("INCONCLUSIVE", "profile_mismatch")
    stripped = canonical["P0"].replace("(deny network*)", "")
    allowed = canonical["P0"].replace("(deny network*)", "(allow network*)")
    rule_removed = p0_profile in {stripped, allowed, canonical["P1-before"]} and p0_profile != canonical["P0"]
    if p0_profile != canonical["P0"] and not rule_removed:
        return Verdict("INCONCLUSIVE", "profile_mismatch")
    for name in PHASES:
        if not _scope_before_network(phases[name].get("observations")):
            return Verdict("INCONCLUSIVE", "scope_order")
        if not _scope_denied(phases[name]["observations"]):
            return Verdict("INCONCLUSIVE", "missing_scope")
    host_scope = receipts.get("host_scope")
    host_rows = host_scope.get("observations") if isinstance(host_scope, dict) else None
    host_found = _ops_by_name(host_rows)
    if not isinstance(host_scope, dict) or host_scope.get("sandboxed") is not False or host_found is None:
        return Verdict("INCONCLUSIVE", "missing_host_scope")
    for op in ("sentinel_write", "decoy_read"):
        if host_found.get(op, {}).get("result") != "allow":
            return Verdict("INCONCLUSIVE", "missing_host_scope")
    host_network = _ops_by_name(receipts.get("host_positive"))
    if host_network is None or any(host_network.get(op, {}).get("result") != "connect" for op in MEASURED_OPERATIONS):
        return Verdict("INCONCLUSIVE", "missing_positive")
    if not all(_network_state(phases[name]) == "connects" for name in ("P1-before", "P1-after")):
        return Verdict("INCONCLUSIVE", "missing_p1_positive")
    p0_network = _network_state(phases["P0"])
    if p0_network in {"connects", "partial"}:
        return Verdict("MECHANISM_FAILS", "p0_network_not_denied")
    grandchild = receipts.get("grandchild")
    if isinstance(grandchild, dict) and grandchild.get("tcp_external") == "connect":
        return Verdict("MECHANISM_FAILS", "grandchild_not_denied")
    verifier_reason = _verifier_reason(receipts.get("verifier"), paths)
    if verifier_reason:
        return Verdict("INCONCLUSIVE", verifier_reason)
    if not _fixtures_unchanged(receipts.get("fixtures")):
        return Verdict("INCONCLUSIVE", "fixture_mutation")
    grandchild_verdict = _grandchild_verdict(grandchild)
    if grandchild_verdict is not None:
        return grandchild_verdict
    if rule_removed or p0_network != "denies":
        return Verdict("INCONCLUSIVE", "network_rule_unattributable" if rule_removed else "p0_network")
    if receipts.get("cleanup") != "clean":
        return Verdict("INCONCLUSIVE", "cleanup")
    return Verdict("PASS", "tcp_loopback_and_tcp_external")


def _sbpl_literal(path: str) -> str:
    if not isinstance(path, str) or not path.startswith("/") or path.endswith("/"):
        raise Refuse("path")
    parts = path.split("/")
    if parts[0] != "" or any(part in {"", ".", ".."} for part in parts[1:]):
        raise Refuse("path")
    if any(char in path for char in '"()\\\n\r\t '):
        raise Refuse("path")
    return path


def profile(network: str, sentinel: str, decoy: str) -> str:
    if network not in {"deny", "allow"}:
        raise Refuse("network rule")
    sentinel = _sbpl_literal(sentinel)
    decoy = _sbpl_literal(decoy)
    if sentinel == decoy:
        raise Refuse("path")
    return (
        "(version 1)(allow default)"
        f"({network} network*)"
        f'(deny file-write* (literal "{sentinel}"))'
        f'(deny file-read* (literal "{decoy}"))'
    )


def build_profiles(sentinel: str, decoy: str) -> dict[str, str]:
    allow = profile("allow", sentinel, decoy)
    deny = profile("deny", sentinel, decoy)
    if deny.replace("(deny network*)", "(allow network*)") != allow:
        raise Refuse("profile pair")
    return {"P1-before": allow, "P0": deny, "P1-after": allow}


def isolation_argv(profile_text: str, command: list[str]) -> list[str]:
    if not command or command[0] == "/usr/bin/sandbox-exec":
        raise Refuse("command")
    return ["/usr/bin/sandbox-exec", "-p", profile_text, *command]


def child_argv(executable: str, script: str) -> list[str]:
    return [executable, script, "--child"]


def bundle_import_argv(assay: str, decisions: str, bundle_out: str) -> list[str]:
    return [
        assay,
        "evidence",
        "import",
        "privileged-mcp-action",
        "--decisions",
        decisions,
        "--bundle-out",
        bundle_out,
    ]


def verifier_argv(assay: str, bundle: str) -> list[str]:
    return [
        assay,
        "evidence",
        "verify-privileged-mcp-action",
        bundle,
        "--format",
        "json",
        "--profile-version",
        "v1",
    ]


def certificate_identity(repository: str) -> str:
    if repository.count("/") != 1 or not all(part and "/" not in part for part in repository.split("/")):
        raise Refuse("repository")
    if any(char in repository for char in " @\n\r"):
        raise Refuse("repository")
    return (
        f"https://github.com/{repository}/.github/workflows/release.yml"
        f"@refs/tags/{RELEASE_TAG}"
    )


def native_triple(uname_m: str, runner_arch: str) -> str:
    if runner_arch == "ARM64" and uname_m == "arm64":
        return "aarch64-apple-darwin"
    if runner_arch == "X64" and uname_m == "x86_64":
        return "x86_64-apple-darwin"
    raise Refuse("architecture")


def archive_name(triple: str) -> str:
    if triple not in {"aarch64-apple-darwin", "x86_64-apple-darwin"}:
        raise Refuse("target")
    return f"assay-{RELEASE_TAG}-{triple}.tar.gz"


def cosign_verify_argv(cosign_bin: str, bundle_path: str, manifest_path: str, identity: str) -> list[str]:
    return [
        cosign_bin,
        "verify-blob",
        "--bundle",
        bundle_path,
        "--certificate-identity",
        identity,
        "--certificate-oidc-issuer",
        OIDC_ISSUER,
        manifest_path,
    ]


def manifest_sha256(text: str, selected: str) -> str:
    suffix = f"  {selected}"
    matches = [line[: -len(suffix)] for line in text.splitlines() if line.endswith(suffix)]
    if len(matches) != 1:
        raise Refuse("checksum manifest")
    digest = matches[0]
    if len(digest) != 64 or any(char not in "0123456789abcdef" for char in digest):
        raise Refuse("checksum manifest")
    return digest


def observed_read_errno(stdout: str, stderr: str) -> str | None:
    """EPERM only when the child text contains Darwin's `os error 1`."""
    import re

    if re.search(r"os error 1(?!\d)", stdout) or re.search(r"os error 1(?!\d)", stderr):
        return "EPERM"
    return None


def _read_for(stream: Any, size: int, wait: float) -> bytes | None:
    if hasattr(stream, "read_for"):
        return stream.read_for(size, wait)
    import select

    if wait <= 0:
        return None
    ready, _, _ = select.select([stream], [], [], wait)
    if not ready:
        return None
    return stream.read(size)


def _reap_owned(proc: Any, group: int, signal_group: Callable[[int, int], None]) -> None:
    """Kill and reap only the owned group, on a clock that is not the execution deadline."""
    import signal
    import subprocess
    import time

    cleanup_deadline = time.monotonic() + CLEANUP_DEADLINE_S
    try:
        signal_group(group, signal.SIGKILL)
    except ProcessLookupError:
        pass
    except OSError as exc:
        raise Refuse("cleanup") from exc
    remaining = cleanup_deadline - time.monotonic()
    if remaining <= 0:
        raise Refuse("cleanup")
    try:
        proc.wait(timeout=remaining)
    except subprocess.TimeoutExpired as exc:
        raise Refuse("cleanup") from exc


def collect_child(
    argv: list[str],
    *,
    timeout: float,
    env: dict[str, str] | None = None,
    spawn: Callable[..., Any] | None = None,
) -> Any:
    """Drain both pipes and wait for exit under one monotonic deadline.

    The owned group is the session this call creates, or the group the injected
    process declares. A nested child keeps its parent's group. Expiry, overflow,
    and interrupt reap that group on a separate cleanup clock and drop late bytes.
    """
    import os
    import subprocess
    import threading
    import time

    if spawn is None:
        nested = os.environ.get("ASSAY_DARWIN_OFFLINE_CHILD") == "1"
        proc = subprocess.Popen(
            argv,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
            start_new_session=not nested,
        )
        group = proc.pid

        def signal_group(pgid: int, sig: int) -> None:
            if pgid != group:
                raise Refuse("process group")
            if nested:
                os.kill(pgid, sig)
            else:
                os.killpg(pgid, sig)
    else:
        proc = spawn(argv, env)
        group = getattr(proc, "owned_group", None)
        signal_group = getattr(proc, "signal_group", None)
        if not isinstance(group, int) or group <= 0 or not callable(signal_group):
            raise Refuse("process group")
    deadline = time.monotonic() + timeout
    stop = threading.Event()
    overflow = threading.Event()
    stdout_eof = threading.Event()
    stderr_eof = threading.Event()
    errors: list[BaseException] = []
    stdout_buf = bytearray()
    stderr_buf = bytearray()

    def drain(stream: Any, buf: bytearray, eof: threading.Event) -> None:
        try:
            while not stop.is_set():
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    return
                chunk = _read_for(stream, min(65536, OUTPUT_MAX_BYTES - len(buf) + 1), min(remaining, 0.05))
                if stop.is_set() or time.monotonic() > deadline:
                    return
                if chunk is None:
                    continue
                if chunk == b"":
                    eof.set()
                    return
                if len(buf) + len(chunk) > OUTPUT_MAX_BYTES:
                    overflow.set()
                    stop.set()
                    return
                buf.extend(chunk)
        except Exception as exc:  # noqa: BLE001 - the supervisor reaps before this surfaces
            errors.append(exc)
            stop.set()

    threads = (
        threading.Thread(target=drain, args=(proc.stdout, stdout_buf, stdout_eof), daemon=True),
        threading.Thread(target=drain, args=(proc.stderr, stderr_buf, stderr_eof), daemon=True),
    )
    for thread in threads:
        thread.start()
    result = None
    seen_exit = False
    code = 1
    try:
        while time.monotonic() < deadline:
            if overflow.is_set() or errors:
                break
            if seen_exit and stdout_eof.is_set() and stderr_eof.is_set():
                break
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                break
            if not seen_exit:
                try:
                    code = proc.wait(timeout=min(0.05, remaining))
                    seen_exit = True
                except subprocess.TimeoutExpired:
                    pass
            else:
                time.sleep(min(0.05, remaining))
        if (
            seen_exit
            and stdout_eof.is_set()
            and stderr_eof.is_set()
            and not overflow.is_set()
            and not errors
            and time.monotonic() <= deadline
        ):
            result = type("Completed", (), {"returncode": code, "stdout": bytes(stdout_buf), "stderr": bytes(stderr_buf)})()
    finally:
        stop.set()
        for thread in threads:
            thread.join(timeout=0.1)
        if result is None:
            _reap_owned(proc, group, signal_group)
    if result is not None:
        return result
    if overflow.is_set():
        raise Refuse("output ceiling")
    if errors:
        raise errors[0]
    raise subprocess.TimeoutExpired(argv, timeout)


def bounded_read(chunks: list[bytes], max_bytes: int) -> bytes:
    if max_bytes <= 0:
        raise Refuse("ceiling")
    total = 0
    output = bytearray()
    for chunk in chunks:
        total += len(chunk)
        if total > max_bytes:
            raise Refuse("ceiling")
        output += chunk
    if total == 0:
        raise Refuse("empty")
    return bytes(output)


def translation_ok(sysctl_ok: bool, sysctl_value: str, uname_m: str, runner_arch: str) -> None:
    if sysctl_ok and sysctl_value == "1":
        raise Refuse("translated architecture")
    if sysctl_ok and sysctl_value == "0":
        pass
    elif not sysctl_ok and uname_m == "x86_64" and runner_arch == "X64":
        pass
    else:
        raise Refuse("architecture ambiguity")
    if runner_arch == "ARM64" and (not sysctl_ok or uname_m != "arm64"):
        raise Refuse("architecture ambiguity")
    if runner_arch == "X64" and uname_m != "x86_64":
        raise Refuse("architecture mismatch")
    native_triple(uname_m, runner_arch)


def require_hosted_darwin(env: dict[str, str], *, sysctl_ok: bool, sysctl_value: str, uname_m: str) -> dict[str, str]:
    """Refuse before any fetch. Callers must not acquire until this returns."""
    if env.get("GITHUB_ACTIONS") != "true":
        raise Refuse("not GitHub Actions")
    if env.get("RUNNER_ENVIRONMENT") != "github-hosted":
        raise Refuse("not GitHub-hosted")
    if env.get("RUNNER_OS") != "macOS":
        raise Refuse("not Darwin")
    image_os = env.get("ImageOS") or ""
    image_version = env.get("ImageVersion") or ""
    if not image_os or not image_version:
        raise Refuse("runner image")
    label = env.get("ASSAY_EXPECT_LABEL") or ""
    runner_arch = env.get("RUNNER_ARCH") or ""
    if RUNNER_LABELS.get(label) != runner_arch:
        raise Refuse("runner label")
    translation_ok(sysctl_ok, sysctl_value, uname_m, runner_arch)
    repository = env.get("GITHUB_REPOSITORY") or ""
    identity = certificate_identity(repository)
    triple = native_triple(uname_m, runner_arch)
    return {
        "label": label,
        "runner_arch": runner_arch,
        "uname_m": uname_m,
        "image_os": image_os,
        "image_version": image_version,
        "triple": triple,
        "archive": archive_name(triple),
        "identity": identity,
        "repository": repository,
    }


def release_urls(repository: str, selected: str) -> dict[str, str]:
    base = f"https://github.com/{repository}/releases/download/{RELEASE_TAG}"
    return {
        "archive": f"{base}/{selected}",
        "manifest": f"{base}/checksums.txt",
        "bundle": f"{base}/checksums.txt.sigstore.json",
    }


def authenticate_release(
    *,
    cosign_version: str,
    verify_exit: int,
    manifest_text: str,
    archive_bytes: bytes,
    selected: str,
) -> None:
    if cosign_version != COSIGN_RELEASE:
        raise Refuse("cosign pin")
    if verify_exit != 0:
        raise Refuse("signed checksum manifest")
    digest = manifest_sha256(manifest_text, selected)
    if hashlib.sha256(archive_bytes).hexdigest() != digest:
        raise Refuse("archive checksum")


def child_observations(ops: Any, endpoints: dict[str, str], paths: dict[str, str], *, spawn_grandchild: bool) -> tuple[list[dict[str, Any]], Any]:
    observations: list[dict[str, Any]] = []
    wrote = ops.write(paths["sentinel"])
    observations.append({"op": "sentinel_write", "result": wrote.result, "errno": wrote.errno})
    read = ops.read(paths["decoy"])
    observations.append({"op": "decoy_read", "result": read.result, "errno": read.errno})
    grandchild = ops.spawn() if spawn_grandchild else None
    for name in MEASURED_OPERATIONS:
        status = ops.connect(endpoints[name])
        observations.append({"op": name, "result": status.result, "errno": status.errno})
    return observations, grandchild


def _owned(root: str, path: str) -> bool:
    if not root.startswith("/") or not path.startswith(root + "/"):
        return False
    if ".." in path.split("/"):
        return False
    results = root + "/results"
    return path != results and not path.startswith(results + "/")


def perform_cleanup(root: str, owned: list[str], unlink: Callable[[str], None], *, results_ready: bool) -> str:
    if not results_ready:
        return "unknown"
    for path in owned:
        if not _owned(root, path):
            return "unknown"
        try:
            unlink(path)
        except OSError:
            return "unknown"
    return "clean"


def run_hosted(
    env: dict[str, str],
    *,
    sysctl_ok: bool,
    sysctl_value: str,
    uname_m: str,
    fetch: Callable[[dict[str, str]], Any],
    exec_cmd: Callable[[list[str]], int],
    cosign_version: Callable[[], str],
    measure: Callable[[dict[str, str]], Any] | None = None,
) -> dict[str, str]:
    context = require_hosted_darwin(env, sysctl_ok=sysctl_ok, sysctl_value=sysctl_value, uname_m=uname_m)
    version = cosign_version()
    if version != COSIGN_RELEASE:
        raise Refuse("cosign pin")
    assets = fetch(context)
    authenticate_release(
        cosign_version=version,
        verify_exit=exec_cmd(
            cosign_verify_argv("cosign", assets.bundle_path, assets.manifest_path, context["identity"])
        ),
        manifest_text=assets.manifest_text,
        archive_bytes=assets.archive_bytes,
        selected=context["archive"],
    )
    if measure is not None:
        measure(context)
    return context


def _status(result: str, errno: str | None = None) -> Any:
    return type("Status", (), {"result": result, "errno": errno})()


def _ops(network: str, scope: str = "deny", spawn: Any = None) -> Any:
    errno = "EPERM" if scope == "deny" else None
    net_errno = "EPERM" if network == "deny" else None

    def write(_path: str) -> Any:
        return _status(scope, errno)

    def read(_path: str) -> Any:
        return _status(scope, errno)

    def connect(_endpoint: str) -> Any:
        return _status(network, net_errno)

    def spawn_grandchild() -> Any:
        if spawn is None:
            external = "deny" if network == "deny" else "connect"
            return {
                "present": True,
                "phase": "P0",
                "sentinel_write": "deny",
                "errno": "EPERM",
                "tcp_external": external,
            }
        return spawn()

    return type("Ops", (), {"write": staticmethod(write), "read": staticmethod(read), "connect": staticmethod(connect), "spawn": staticmethod(spawn_grandchild)})()


def assemble_receipts(
    *,
    profiles: dict[str, str],
    paths: dict[str, str],
    endpoints: dict[str, str],
    context: dict[str, Any],
    runner: Callable[[str, list[str]], dict[str, Any]],
    verifier: Callable[[list[str]], dict[str, Any]],
    hasher: Callable[[str], str],
    effects: list[Any],
    host_scope_ops: Any,
    host_positive: Callable[[dict[str, str]], list[dict[str, Any]]],
    executable: str = "python3",
    script: str = "darwin_offline_probe.py",
    assay: str = "/opt/assay",
) -> dict[str, Any]:
    probe = child_argv(executable, script)
    phases: dict[str, Any] = {}
    grandchild = None
    before = {key: hasher(paths[key]) for key in ("decoy", "real_bundle")}
    for name in PHASES:
        argv = isolation_argv(profiles[name], probe)
        effects.append(("phase", argv))
        outcome = runner(name, argv)
        phase: dict[str, Any] = {
            "profile": profiles[name],
            "argv": argv,
            "endpoints": dict(endpoints),
            "paths": {"sentinel": paths["sentinel"], "decoy": paths["decoy"]},
            "probe": list(probe),
            "profile_executed": outcome.get("profile_executed", True),
            "timed_out": outcome.get("timed_out", False),
            "observations": [],
        }
        if phase["profile_executed"] and not phase["timed_out"]:
            if outcome.get("observations") is not None:
                phase["observations"] = outcome["observations"]
                if name == "P0":
                    grandchild = outcome.get("grandchild")
            else:
                observations, spawned = child_observations(
                    outcome["ops"], endpoints, paths, spawn_grandchild=(name == "P0")
                )
                phase["observations"] = observations
                if name == "P0":
                    grandchild = spawned
        phases[name] = phase
    host_scope, _unused = child_observations(host_scope_ops, endpoints, paths, spawn_grandchild=False)
    effects.append(("host-scope", "unsandboxed"))
    decoy_host = verifier(verifier_argv(assay, paths["decoy"]))
    real_connected = verifier(verifier_argv(assay, paths["real_bundle"]))
    decoy_p0 = verifier(isolation_argv(profiles["P0"], verifier_argv(assay, paths["decoy"])))
    decoy_p1 = verifier(isolation_argv(profiles["P1-before"], verifier_argv(assay, paths["decoy"])))
    real_p0 = verifier(isolation_argv(profiles["P0"], verifier_argv(assay, paths["real_bundle"])))
    for label, argv in (
        ("decoy_host", verifier_argv(assay, paths["decoy"])),
        ("real_connected", verifier_argv(assay, paths["real_bundle"])),
        ("decoy_p0", isolation_argv(profiles["P0"], verifier_argv(assay, paths["decoy"]))),
        ("decoy_p1", isolation_argv(profiles["P1-before"], verifier_argv(assay, paths["decoy"]))),
        ("real_p0", isolation_argv(profiles["P0"], verifier_argv(assay, paths["real_bundle"]))),
    ):
        effects.append(("verifier", label, argv))
    after = {key: hasher(paths[key]) for key in ("decoy", "real_bundle")}
    return {
        "release_tag": RELEASE_TAG,
        "measured_operations": list(MEASURED_OPERATIONS),
        "context": context,
        "paths": dict(paths),
        "endpoints": dict(endpoints),
        "phases": phases,
        "host_scope": {"sandboxed": False, "observations": host_scope},
        "host_positive": host_positive(endpoints),
        "grandchild": grandchild,
        "verifier": {
            "decoy_host": decoy_host,
            "decoy_p0": decoy_p0,
            "decoy_p1": decoy_p1,
            "real_connected": real_connected,
            "real_p0": real_p0,
        },
        "fixtures": {
            "decoy_sha256_before": before["decoy"],
            "decoy_sha256_after": after["decoy"],
            "real_sha256_before": before["real_bundle"],
            "real_sha256_after": after["real_bundle"],
        },
        "cleanup": "clean",
        "timed_out": False,
    }


def _canonical_report() -> str:
    return json.dumps(
        {
            "bundle_integrity": "pass",
            "schema": REPORT_SCHEMA,
            "verdict": "valid",
        },
        separators=(",", ":"),
        sort_keys=True,
    )


def _hosted_env(label: str = "macos-26") -> dict[str, str]:
    return {
        "GITHUB_ACTIONS": "true",
        "RUNNER_ENVIRONMENT": "github-hosted",
        "RUNNER_OS": "macOS",
        "RUNNER_ARCH": RUNNER_LABELS[label],
        "ImageOS": "macos26",
        "ImageVersion": "20260924.1",
        "GITHUB_REPOSITORY": "Rul1an/assay",
        "ASSAY_EXPECT_LABEL": label,
    }


def _context(label: str = "macos-26") -> dict[str, Any]:
    arch = RUNNER_LABELS[label]
    return {
        "os_product": "recorded-by-hosted-run",
        "uname_m": "arm64" if arch == "ARM64" else "x86_64",
        "runner_arch": arch,
        "proc_translated": 0,
        "translation_known": True,
        "runner_environment": "github-hosted",
        "runner_os": "macOS",
        "image_os": "macos26",
        "image_version": "recorded-by-hosted-run",
        "expect_label": label,
        "sandbox_exec_listing": "recorded-by-hosted-run",
        "python_version": "recorded-by-hosted-run",
        "cosign_release": COSIGN_RELEASE,
        "sw_vers": "recorded-by-hosted-run",
    }


def _paths() -> dict[str, str]:
    root = "/tmp/assay-darwin-feasibility"
    return {
        "sentinel": f"{root}/sentinel",
        "decoy": f"{root}/decoy.tar.gz",
        "real_bundle": f"{root}/real.tar.gz",
    }


def _endpoints() -> dict[str, str]:
    return {"tcp_loopback": "127.0.0.1:9", "tcp_external": "203.0.113.10:443"}


def _profile_runner(name: str, argv: list[str]) -> dict[str, Any]:
    del name
    prof = argv[2]
    network = "deny" if "(deny network*)" in prof else "connect"
    return {"profile_executed": True, "timed_out": False, "ops": _ops(network)}


def command_bundle(argv: list[str]) -> str:
    tail = argv[3:] if argv[:2] == ["/usr/bin/sandbox-exec", "-p"] else argv
    if len(tail) < 8 or tail[1] != "evidence" or tail[2] != "verify-privileged-mcp-action":
        raise Refuse("verifier argv")
    return tail[3]


def _verifier_double(argv: list[str]) -> dict[str, Any]:
    report = _canonical_report()
    bundle = command_bundle(argv)
    sandboxed = argv[0] == "/usr/bin/sandbox-exec"
    if sandboxed and bundle.endswith("/decoy.tar.gz"):
        return {
            "exit": 2,
            "stdout": "",
            "stderr": "read denied EPERM",
            "read_errno": "EPERM",
            "readable": True,
            "valid_bundle": True,
            "bundle": bundle,
        }
    return {
        "exit": 0,
        "stdout": report,
        "stderr": "",
        "readable": True,
        "valid_bundle": True,
        "bundle": bundle,
    }


def _hasher(path: str) -> str:
    return hashlib.sha256(path.encode()).hexdigest()


def passing_receipts(effects: list[Any] | None = None, **overrides: Any) -> dict[str, Any]:
    paths = _paths()
    profiles = build_profiles(paths["sentinel"], paths["decoy"])
    if "p0_profile" in overrides:
        profiles = dict(profiles)
        profiles["P0"] = overrides.pop("p0_profile")
    receipts = assemble_receipts(
        profiles=profiles,
        paths=paths,
        endpoints=_endpoints(),
        context=_context(),
        runner=overrides.pop("runner", _profile_runner),
        verifier=overrides.pop("verifier", _verifier_double),
        hasher=overrides.pop("hasher", _hasher),
        effects=effects if effects is not None else [],
        host_scope_ops=overrides.pop("host_scope_ops", _ops("connect", "allow", spawn=lambda: None)),
        host_positive=overrides.pop(
            "host_positive",
            lambda _endpoints: [{"op": name, "result": "connect", "errno": None} for name in MEASURED_OPERATIONS],
        ),
    )
    receipts.update(overrides)
    return receipts


def _pipe(data: bytes) -> Any:
    class Pipe:
        def __init__(self) -> None:
            self._pending = data

        def read(self, _size: int) -> bytes:
            pending = self._pending
            self._pending = b""
            return pending

        def read_for(self, size: int, wait: float) -> bytes | None:
            del wait
            return self.read(size)

    return Pipe()


def _proc(code: int, stdout: bytes, stderr: bytes = b"") -> Any:
    class Proc:
        returncode = code
        owned_group = 1

        def wait(self, timeout: float | None = None) -> int:
            del timeout
            return code

        def kill(self) -> None:
            return None

        def signal_group(self, pgid: int, sig: int) -> None:
            del sig
            if pgid != self.owned_group:
                raise OSError("unrelated process group")

    proc = Proc()
    proc.stdout = _pipe(stdout)
    proc.stderr = _pipe(stderr)
    return proc


def _tiny_archive() -> bytes:
    import io
    import tarfile

    payload = b"not-executed\n"
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w:gz") as archive:
        info = tarfile.TarInfo("assay")
        info.size = len(payload)
        info.mode = 0o644
        archive.addfile(info, io.BytesIO(payload))
    return raw.getvalue()


def _success_report(*, extra: dict[str, str] | None = None) -> bytes:
    body: dict[str, Any] = {
        "bundle_integrity": "pass",
        "schema": REPORT_SCHEMA,
        "verdict": "valid",
    }
    if extra:
        body.update(extra)
    return json.dumps(body, separators=(",", ":"), sort_keys=True).encode()


def _denial_report(*, errno: bool) -> bytes:
    detail = (
        "failed to open bundle: Permission denied (os error 1)"
        if errno
        else "failed to open bundle: E_EVIDENCE_UNREADABLE"
    )
    return json.dumps(
        {
            "bundle_integrity": "fail",
            "findings": [{"detail": detail}],
            "reason_code": "E_EVIDENCE_UNREADABLE",
            "schema": REPORT_SCHEMA,
        },
        separators=(",", ":"),
        sort_keys=True,
    ).encode()


def _child_payload(deny_network: bool) -> bytes:
    def row(op: str, result: str, errno: str | None) -> dict[str, Any]:
        return {"errno": errno, "op": op, "result": result}

    network = ("deny", "EPERM") if deny_network else ("connect", None)
    payload: dict[str, Any] = {
        "observations": [
            row("sentinel_write", "deny", "EPERM"),
            row("decoy_read", "deny", "EPERM"),
            row("tcp_loopback", network[0], network[1]),
            row("tcp_external", network[0], network[1]),
        ]
    }
    if deny_network:
        payload["grandchild"] = {
            "errno": "EPERM",
            "phase": "P0",
            "present": True,
            "sentinel_write": "deny",
            "tcp_external": "deny",
        }
    return json.dumps(payload, separators=(",", ":"), sort_keys=True).encode()


def hosted_collector_cases(failures: list[str]) -> None:
    """Drive _measure_hosted and _verifier_process. Injected processes only."""
    import os
    import tempfile

    archive = _tiny_archive()

    class Listener:
        def close(self) -> None:
            return None

    def fetch(context: dict[str, str]) -> dict[str, bytes]:
        digest = hashlib.sha256(archive).hexdigest()
        return {
            "archive": archive,
            "manifest": f"{digest}  {context['archive']}\n".encode(),
            "bundle": b"{}",
        }

    def bind() -> tuple[Any, str]:
        return Listener(), "127.0.0.1:9"

    def host_tcp(endpoints: dict[str, str]) -> list[dict[str, Any]]:
        del endpoints
        return [{"op": name, "result": "connect", "errno": None} for name in MEASURED_OPERATIONS]

    def run_case(mode: str) -> tuple[int | None, dict[str, Any] | None, list[list[str]], bytes | None]:
        calls: list[list[str]] = []
        decisions: list[bytes] = []

        def spawn(argv: list[str], env: dict[str, str] | None = None) -> Any:
            del env
            calls.append(list(argv))
            if argv == ["cosign", "version"]:
                return _proc(0, f"GitVersion: {COSIGN_RELEASE}\n".encode())
            if len(argv) > 1 and argv[1] == "verify-blob":
                return _proc(0, b"")
            if argv == ["/usr/bin/sw_vers"]:
                return _proc(0, b"ProductVersion:\t26.0\n")
            if argv[:3] == ["/bin/ls", "-l", "/usr/bin/sandbox-exec"]:
                return _proc(0, b"-r-xr-xr-x 1 root wheel 1 /usr/bin/sandbox-exec\n")
            if len(argv) > 1 and argv[1] == "version":
                return _proc(0, b"6.6.2\n")
            if len(argv) > 3 and argv[1:4] == ["evidence", "import", "privileged-mcp-action"]:
                bundle_out = argv[argv.index("--bundle-out") + 1]
                decisions.append(Path(argv[argv.index("--decisions") + 1]).read_bytes())
                blob = PLACEHOLDER_BUNDLE if mode == "placeholder" else b"imported-bundle\n"
                descriptor = os.open(bundle_out, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
                try:
                    os.write(descriptor, blob)
                finally:
                    os.close(descriptor)
                return _proc(0, b"")
            if "verify-privileged-mcp-action" in argv:
                if mode == "overflow":
                    return _proc(0, b"x" * (OUTPUT_MAX_BYTES + 1))
                sandboxed = argv[0] == "/usr/bin/sandbox-exec"
                bundle = command_bundle(argv)
                if sandboxed and bundle.endswith("/decoy.tar.gz"):
                    return _proc(2, _denial_report(errno=mode != "absent-errno"))
                extra = {"optional_udp": "not_measured"} if mode == "noop" else None
                return _proc(0, _success_report(extra=extra))
            if argv[:2] == ["/usr/bin/sandbox-exec", "-p"]:
                return _proc(0, _child_payload("(deny network*)" in argv[2]))
            return _proc(2, b"", b"unexpected argv")

        root = tempfile.mkdtemp(prefix="assay-darwin-collector-")
        verdict: dict[str, Any] | None = None
        code: int | None = None
        refused: BaseException | None = None
        try:
            code = _measure_hosted(
                _hosted_env(),
                require_hosted_darwin(_hosted_env(), sysctl_ok=True, sysctl_value="0", uname_m="arm64"),
                root,
                sysctl_ok=True,
                sysctl_value="0",
                spawn=spawn,
                fetch=fetch,
                bind=bind,
                host_ops=_ops("connect", "allow"),
                host_tcp=host_tcp,
            )
            verdict_path = Path(root) / "results" / "verdict.json"
            if verdict_path.is_file():
                verdict = json.loads(verdict_path.read_text(encoding="utf-8"))
        except (TypeError, Refuse) as exc:
            refused = exc
        finally:
            import shutil

            shutil.rmtree(root, ignore_errors=True)
        return code, verdict, calls, decisions[0] if decisions else None, refused

    code, verdict, calls, decision, refused = run_case("positive")
    if isinstance(refused, TypeError):
        for name in ("positive", "noop", "placeholder", "absent-errno", "output-overflow"):
            failures.append(f"{name}: {refused}")
        return
    if refused is not None or code != 0 or not verdict or verdict.get("verdict") != "PASS":
        failures.append(f"positive: {code} {verdict} {refused}")
    else:
        denial = verdict["receipt"]["verifier"]
        if any(denial[key].get("read_errno") != "EPERM" for key in ("decoy_p0", "decoy_p1")):
            failures.append("positive did not observe EPERM from the verifier output")
    imports = [argv for argv in calls if len(argv) > 3 and argv[1:4] == ["evidence", "import", "privileged-mcp-action"]]
    if len(imports) != 2:
        failures.append(f"positive did not import two bundles: {len(imports)}")
    if decision != DECISION_NDJSON:
        failures.append("positive decision is not the historical retention fixture")
    noop_code, noop_verdict, _noop_calls, _noop_decision, noop_refused = run_case("noop")
    if noop_refused is not None or noop_code != 0 or not noop_verdict or noop_verdict.get("verdict") != "PASS":
        failures.append(f"noop: {noop_code} {noop_verdict} {noop_refused}")
    placeholder_code, placeholder_verdict, placeholder_calls, _placeholder_decision, placeholder_refused = run_case(
        "placeholder"
    )
    if placeholder_verdict and placeholder_verdict.get("verdict") == "PASS":
        failures.append("placeholder bundle reached PASS")
    if not isinstance(placeholder_refused, Refuse) or "bundle import" not in str(placeholder_refused):
        failures.append(f"placeholder was not refused at import: {placeholder_refused}")
    if not any(
        len(argv) > 3 and argv[1:4] == ["evidence", "import", "privileged-mcp-action"] for argv in placeholder_calls
    ):
        failures.append("placeholder producer did not call the published import")
    if placeholder_code == 0:
        failures.append("placeholder returned success")
    absent_code, absent_verdict, _absent_calls, _absent_decision, absent_refused = run_case("absent-errno")
    if absent_refused is not None or absent_code == 0 or not absent_verdict or absent_verdict.get("verdict") == "PASS":
        failures.append(f"absent errno reached PASS: {absent_code} {absent_verdict} {absent_refused}")
    records = ((absent_verdict or {}).get("receipt") or {}).get("verifier") or {}
    for key in ("decoy_p0", "decoy_p1"):
        errno = (records.get(key) or {}).get("read_errno")
        if errno is not None:
            failures.append(f"absent errno recorded {errno} on {key}")
    if observed_read_errno("os error 10", "") is not None or observed_read_errno("os error 13", "") is not None:
        failures.append("errno parser treated a different os error as EPERM")
    if observed_read_errno("E_EVIDENCE_UNREADABLE", "") is not None:
        failures.append("errno parser fabricated EPERM from E_EVIDENCE_UNREADABLE")
    overflow_code, overflow_verdict, _overflow_calls, _overflow_decision, overflow_refused = run_case("overflow")
    if overflow_refused is None or "ceiling" not in str(overflow_refused):
        failures.append(f"output overflow was collected: {overflow_code} {overflow_verdict} {overflow_refused}")
    if overflow_code == 0 or (overflow_verdict and overflow_verdict.get("verdict") == "PASS"):
        failures.append("output overflow reached PASS")
    try:
        _verifier_process(
            verifier_argv("/opt/assay", "/tmp/assay-darwin-feasibility/decoy.tar.gz"),
            spawn=lambda argv, env=None: _proc(0, b"y" * (OUTPUT_MAX_BYTES + 1)),
        )
        failures.append("verifier collector retained an oversized child")
    except TypeError as exc:
        failures.append(f"verifier collector: {exc}")
    except Refuse:
        pass


def _holding_stream(data: bytes, hold: float, gate: Any = None) -> Any:
    import threading

    ready = gate if gate is not None else threading.Event()
    if gate is None:
        threading.Timer(hold, ready.set).start()

    class Stream:
        def __init__(self) -> None:
            self._data = data
            self._done = False

        def read(self, _size: int) -> bytes:
            ready.wait(hold)
            if self._done:
                return b""
            self._done = True
            return self._data

        def read_for(self, _size: int, wait: float) -> bytes | None:
            if not ready.wait(max(wait, 0)):
                return None
            if self._done:
                return b""
            self._done = True
            return self._data

    return Stream()


def _owned_proc(stdout: Any, stderr: Any, *, group: int, fail_reap: bool = False) -> Any:
    import subprocess

    class Proc:
        owned_group = group
        returncode = None

        def __init__(self) -> None:
            self.stdout = stdout
            self.stderr = stderr
            self.signaled: tuple[int, int] | None = None
            self.waits: list[float | None] = []
            self.killed_direct = False

        def signal_group(self, pgid: int, sig: int) -> None:
            if pgid != group:
                raise OSError("unrelated process group")
            self.signaled = (pgid, sig)

        def kill(self) -> None:
            self.killed_direct = True

        def wait(self, timeout: float | None = None) -> int:
            self.waits.append(timeout)
            if fail_reap and self.signaled is not None:
                raise subprocess.TimeoutExpired(["owned"], timeout or 0)
            return 0

    return Proc()


def deadline_cases(failures: list[str]) -> None:
    """The execution deadline covers both pipes and the exit wait."""
    import subprocess
    import time

    hold = 0.8
    limit = 0.2
    late = b'{"bundle_integrity":"pass","schema":"assay.privileged_mcp_action.verify.report.v0","verdict":"valid"}'

    def elapsed_call(fn: Callable[[], Any]) -> tuple[float, Any, BaseException | None]:
        started = time.monotonic()
        try:
            value = fn()
        except (subprocess.TimeoutExpired, Refuse, OSError) as exc:
            return time.monotonic() - started, None, exc
        return time.monotonic() - started, value, None

    def rejected(name: str, fn: Callable[[], Any]) -> BaseException | None:
        duration, value, exc = elapsed_call(fn)
        if value is not None:
            failures.append(f"{name} accepted a receipt after {duration:.2f}s")
            return None
        if duration > 0.55:
            failures.append(f"{name} blocked for {duration:.2f}s before the deadline")
        return exc

    open_pipe = _holding_stream(late, hold)
    rejected(
        "open pipe",
        lambda: collect_child(["probe"], timeout=limit, spawn=lambda _argv, env=None: _owned_proc(open_pipe, _pipe(b""), group=41)),
    )
    stderr_gate = __import__("threading").Event()

    def stderr_stream() -> Any:
        stream = _holding_stream(b"filled", hold, stderr_gate)
        original = stream.read_for

        def read_for(size: int, wait: float) -> bytes | None:
            stderr_gate.set()
            return original(size, wait)

        stream.read_for = read_for
        original_read = stream.read

        def read(size: int) -> bytes:
            stderr_gate.set()
            return original_read(size)

        stream.read = read
        return stream

    duration, value, exc = elapsed_call(
        lambda: collect_child(
            ["probe"],
            timeout=limit,
            spawn=lambda _argv, env=None: _owned_proc(_holding_stream(b"out", hold, stderr_gate), stderr_stream(), group=42),
        )
    )
    if exc is not None or value is None or value.stdout != b"out" or value.stderr != b"filled" or duration > 0.55:
        failures.append(f"stderr while stdout open: {duration:.2f}s {value} {exc}")
    late_pipe = _holding_stream(late, hold)
    late_value = rejected(
        "late receipt",
        lambda: collect_child(["probe"], timeout=limit, spawn=lambda _argv, env=None: _owned_proc(late_pipe, _pipe(b""), group=43)),
    )
    if late_value is None:
        failures.append("late receipt was returned")
    descendant = _owned_proc(_holding_stream(late, hold), _pipe(b""), group=77)
    rejected(
        "descendant pipe",
        lambda: collect_child(["probe"], timeout=limit, spawn=lambda _argv, env=None: descendant),
    )
    if descendant.signaled != (77, __import__("signal").SIGKILL):
        failures.append(f"descendant group was not reaped: {descendant.signaled} direct={descendant.killed_direct}")
    stuck = _owned_proc(_holding_stream(late, hold), _pipe(b""), group=88, fail_reap=True)
    duration, value, exc = elapsed_call(
        lambda: collect_child(["probe"], timeout=limit, spawn=lambda _argv, env=None: stuck)
    )
    if value is not None or not isinstance(exc, Refuse) or "cleanup" not in str(exc):
        failures.append(f"cleanup failure returned a receipt: {value} {exc}")
    elif any(wait is None or wait > CLEANUP_DEADLINE_S for wait in stuck.waits if stuck.signaled is not None):
        failures.append(f"cleanup wait reused the execution deadline: {stuck.waits}")
    duration, value, exc = elapsed_call(
        lambda: collect_child(["probe"], timeout=limit, spawn=lambda _argv, env=None: _owned_proc(_pipe(b"ok"), _pipe(b""), group=11))
    )
    if exc is not None or value is None or value.stdout != b"ok" or duration > 0.55:
        failures.append(f"normal child: {duration:.2f}s {value} {exc}")
    noop = collect_child(
        ["probe"],
        timeout=limit,
        spawn=lambda _argv, env=None: _owned_proc(_pipe(b"ok"), _pipe(b"idle"), group=12),
    )
    if noop.stdout != b"ok" or noop.stderr != b"idle":
        failures.append("noop child lost output")

    import os
    import tempfile

    previous = CHILD_DEADLINE_S
    root = tempfile.mkdtemp(prefix="assay-darwin-deadline-")
    try:
        globals()["CHILD_DEADLINE_S"] = limit

        def spawn(argv: list[str], env: dict[str, str] | None = None) -> Any:
            del env
            if len(argv) > 3 and argv[1:4] == ["evidence", "import", "privileged-mcp-action"]:
                bundle_out = argv[argv.index("--bundle-out") + 1]
                descriptor = os.open(bundle_out, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
                try:
                    os.write(descriptor, b"imported-bundle\n")
                finally:
                    os.close(descriptor)
                return _owned_proc(_holding_stream(b"", hold), _pipe(b""), group=90)
            return _proc(0, b"")

        try:
            stage_bundles("/opt/assay", root, {"decoy": root + "/decoy.tar.gz", "real_bundle": root + "/real.tar.gz"}, spawn)
            failures.append("producer accepted an import that kept its pipe open")
        except (subprocess.TimeoutExpired, Refuse):
            pass
    finally:
        globals()["CHILD_DEADLINE_S"] = previous
        import shutil

        shutil.rmtree(root, ignore_errors=True)

    ceiling_root = tempfile.mkdtemp(prefix="assay-darwin-ceiling-")
    try:
        try:
            _extract_archive(_tiny_archive(), ceiling_root + "/out", max_decoded_bytes=4)
            failures.append("decoded archive ceiling extracted a member")
        except Refuse as exc:
            if "ceiling" not in str(exc):
                failures.append(f"decoded archive ceiling: {exc}")
        if os.path.exists(ceiling_root + "/out"):
            failures.append("decoded archive ceiling materialized the destination")

        def oversized(argv: list[str], env: dict[str, str] | None = None) -> Any:
            del env
            bundle_out = argv[argv.index("--bundle-out") + 1]
            descriptor = os.open(bundle_out, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            try:
                os.write(descriptor, b"12345")
            finally:
                os.close(descriptor)
            return _proc(0, b"")

        try:
            stage_bundles(
                "/opt/assay",
                ceiling_root,
                {"decoy": ceiling_root + "/decoy.tar.gz", "real_bundle": ceiling_root + "/real.tar.gz"},
                oversized,
                max_bundle_bytes=4,
            )
            failures.append("imported bundle ceiling accepted the file")
        except Refuse as exc:
            if "ceiling" not in str(exc):
                failures.append(f"imported bundle ceiling: {exc}")
    finally:
        import shutil

        shutil.rmtree(ceiling_root, ignore_errors=True)


def self_test() -> int:
    failures: list[str] = []

    def expect(name: str, verdict: str, receipts: dict[str, Any], reason: str) -> None:
        try:
            got = evaluate(receipts)
        except Exception as exc:  # noqa: BLE001 - the RED run is the missing verdict function
            failures.append(f"{name}: {type(exc).__name__}: {exc}")
            return
        if got.verdict != verdict or got.reason != reason:
            failures.append(f"{name}: expected {verdict}/{reason} got {got.verdict}/{got.reason}")

    effects: list[Any] = []
    side: list[str] = []

    def recording_runner(name: str, argv: list[str]) -> dict[str, Any]:
        side.append("runner")
        return _profile_runner(name, argv)

    positive = passing_receipts(effects, runner=recording_runner)
    side.append("noop")
    expect("positive", "PASS", positive, "tcp_loopback_and_tcp_external")
    if not effects or effects[0][1][0] != "/usr/bin/sandbox-exec":
        failures.append("positive did not construct sandbox-exec")
    if "pfctl" in json.dumps(effects) or "sudo" in json.dumps(effects):
        failures.append("plan constructed a host firewall command")
    if side.count("noop") != 1 or "runner" not in side:
        failures.append("posix side effect was not injected")
    noop = dict(positive)
    noop["optional_udp"] = "not_measured"
    expect("noop", "PASS", noop, "tcp_loopback_and_tcp_external")

    stripped = positive["phases"]["P0"]["profile"].replace("(deny network*)", "")
    removed = passing_receipts(p0_profile=stripped)
    expect("remove_p0_network_rule", "MECHANISM_FAILS", removed, "p0_network_not_denied")

    missing_p1 = passing_receipts()
    for item in missing_p1["phases"]["P1-before"]["observations"]:
        if item["op"] == "tcp_external":
            item["result"] = "deny"
    expect("missing_p1_positive", "INCONCLUSIVE", missing_p1, "missing_p1_positive")

    setup = passing_receipts(runner=lambda name, argv: {"profile_executed": False, "timed_out": False})
    expect("profile_setup", "INCONCLUSIVE", setup, "profile_setup")

    missing_verifier = passing_receipts()
    del missing_verifier["verifier"]
    expect("missing_verifier_scope", "INCONCLUSIVE", missing_verifier, "missing_verifier_scope")

    def empty_decoy(argv: list[str]) -> dict[str, Any]:
        bundle = command_bundle(argv)
        if argv[0] == "/usr/bin/sandbox-exec" and bundle.endswith("/decoy.tar.gz"):
            return {
                "exit": 2,
                "stdout": "",
                "stderr": "",
                "read_errno": "EPERM",
                "readable": True,
                "valid_bundle": True,
                "bundle": bundle,
            }
        return _verifier_double(argv)

    empty_error = passing_receipts(verifier=empty_decoy)
    expect("equal_empty_verifier", "INCONCLUSIVE", empty_error, "verifier_error_output")

    missing_grandchild = passing_receipts()
    missing_grandchild["grandchild"] = None
    expect("missing_grandchild", "INCONCLUSIVE", missing_grandchild, "missing_grandchild")

    timed = passing_receipts()
    timed["timed_out"] = True
    expect("timeout", "INCONCLUSIVE", timed, "timeout")

    dirty = passing_receipts()
    dirty["cleanup"] = "unknown"
    expect("unknown_cleanup", "INCONCLUSIVE", dirty, "cleanup")

    drifted = passing_receipts()
    drifted["phases"]["P0"]["endpoints"] = dict(drifted["endpoints"], tcp_external="198.51.100.10:443")
    expect("endpoint_drift", "INCONCLUSIVE", drifted, "endpoint_drift")

    def invalid_decoy(argv: list[str]) -> dict[str, Any]:
        record = dict(_verifier_double(argv))
        if argv[0] != "/usr/bin/sandbox-exec" and str(record["bundle"]).endswith("/decoy.tar.gz"):
            record["valid_bundle"] = False
            record["exit"] = 2
            record["stdout"] = ""
        return record

    expect("invalid_decoy", "INCONCLUSIVE", passing_receipts(verifier=invalid_decoy), "verifier_not_scope_proof")

    def moved_bytes(argv: list[str]) -> dict[str, Any]:
        record = dict(_verifier_double(argv))
        if argv[0] == "/usr/bin/sandbox-exec" and str(record["bundle"]).endswith("/real.tar.gz"):
            record["stdout"] = str(record["stdout"]) + "\n"
        return record

    expect("verifier_bytes", "INCONCLUSIVE", passing_receipts(verifier=moved_bytes), "verifier_bytes")

    denied_child = passing_receipts()
    denied_child["grandchild"] = dict(denied_child["grandchild"], tcp_external="connect")
    expect("grandchild_connects", "MECHANISM_FAILS", denied_child, "grandchild_not_denied")

    acquired: list[str] = []

    def fetch(context: dict[str, str]) -> Any:
        acquired.append(context["archive"])
        HOST_EFFECTS["fetch"] += 1
        raise AssertionError("fetch must not run")

    try:
        run_hosted({}, sysctl_ok=True, sysctl_value="0", uname_m="arm64", fetch=fetch, exec_cmd=lambda _argv: 0, cosign_version=lambda: COSIGN_RELEASE)
        failures.append("local context was accepted")
    except Refuse:
        pass
    if acquired:
        failures.append("acquisition ran before hosted context")

    try:
        run_hosted(
            _hosted_env(),
            sysctl_ok=True,
            sysctl_value="1",
            uname_m="x86_64",
            fetch=fetch,
            exec_cmd=lambda _argv: 0,
            cosign_version=lambda: COSIGN_RELEASE,
        )
        failures.append("translated architecture was accepted")
    except Refuse:
        pass
    if acquired:
        failures.append("acquisition ran for translated architecture")

    class Assets:
        bundle_path = "checksums.txt.sigstore.json"
        manifest_path = "checksums.txt"
        manifest_text = "abc\n"
        archive_bytes = b"not-the-archive"

    executed: list[list[str]] = []

    def wrong_hash_fetch(context: dict[str, str]) -> Assets:
        Assets.manifest_text = f"{'cd' * 32}  {context['archive']}\n"
        Assets.archive_bytes = b"archive-bytes"
        return Assets()

    try:
        run_hosted(
            _hosted_env(),
            sysctl_ok=True,
            sysctl_value="0",
            uname_m="arm64",
            fetch=wrong_hash_fetch,
            exec_cmd=lambda argv: executed.append(argv) or 0,
            cosign_version=lambda: COSIGN_RELEASE,
        )
        failures.append("checksum mismatch continued")
    except Refuse as exc:
        if "checksum" not in str(exc):
            failures.append(f"checksum mismatch reason drifted: {exc}")
    if not executed or executed[0][1] != "verify-blob":
        failures.append("verify-blob was not the production argv")
    if OIDC_ISSUER not in executed[0] or "@refs/tags/" not in " ".join(executed[0]):
        failures.append("verify-blob identity or issuer drifted")
    if any(item[:1] == ["assay"] or (item and item[0].endswith("/assay")) for item in executed):
        failures.append("assay ran after a failed checksum")
    measured: list[str] = []

    def matching_fetch(context: dict[str, str]) -> Assets:
        payload = b"archive-bytes"
        digest = hashlib.sha256(payload).hexdigest()
        Assets.manifest_text = f"{digest}  {context['archive']}\n"
        Assets.archive_bytes = payload
        return Assets()

    try:
        run_hosted(
            _hosted_env("macos-26-intel"),
            sysctl_ok=False,
            sysctl_value="",
            uname_m="x86_64",
            fetch=matching_fetch,
            exec_cmd=lambda argv: executed.append(argv) or 0,
            cosign_version=lambda: "v0.0.0",
            measure=lambda context: measured.append(context["archive"]),
        )
        failures.append("wrong cosign version continued")
    except Refuse as exc:
        if "cosign" not in str(exc):
            failures.append(f"cosign pin reason drifted: {exc}")
    if measured:
        failures.append("measurement ran before a matching cosign pin")

    run_hosted(
        _hosted_env(),
        sysctl_ok=True,
        sysctl_value="0",
        uname_m="arm64",
        fetch=matching_fetch,
        exec_cmd=lambda _argv: 0,
        cosign_version=lambda: COSIGN_RELEASE,
        measure=lambda context: measured.append(context["archive"]),
    )
    if measured != [archive_name("aarch64-apple-darwin")]:
        failures.append(f"authenticated measurement did not see the native archive: {measured}")
    try:
        hosted_entry({})
        failures.append("hosted entry accepted an empty environment")
    except Refuse:
        pass
    import os

    previous_child = os.environ.pop("ASSAY_DARWIN_OFFLINE_CHILD", None)
    try:
        if main(["--child"]) != 2 or main(["--grandchild"]) != 2:
            failures.append("child probe started without the hosted latch")
    finally:
        if previous_child is not None:
            os.environ["ASSAY_DARWIN_OFFLINE_CHILD"] = previous_child
    if git_version(f"GitVersion: {COSIGN_RELEASE}\n") != COSIGN_RELEASE:
        failures.append("cosign version parser drifted from the pin")

    text = Path(__file__).read_text(encoding="utf-8")
    if text.count(f'RELEASE_TAG = "{RELEASE_TAG}"') != 1:
        failures.append("release pin is not a single assignment")
    if text.count(f'COSIGN_RELEASE = "{COSIGN_RELEASE}"') != 1:
        failures.append("cosign pin is not a single assignment")
    workflow = Path(__file__).resolve().parents[2] / ".github/workflows/experiment-darwin-offline.yml"
    workflow_text = workflow.read_text(encoding="utf-8") if workflow.is_file() else ""
    for needle in (
        "cursor/darwin-offline-feasibility",
        "experiment-darwin-offline.yml",
        "darwin_offline_probe.py",
        "contents: read",
        "timeout-minutes:",
        "if: always()",
        "macos-26-intel",
        "macos-26",
        "--print-cosign-release",
        "--self-test",
        "actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09",
        "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
        "sigstore/cosign-installer@6f9f17788090df1f26f669e9d70d6ae9567deba6",
    ):
        if needle not in workflow_text:
            failures.append(f"workflow missing {needle}")
    if "pull_request" in workflow_text or "workflow_dispatch" in workflow_text:
        failures.append("workflow is not branch-push only")
    if "macos-latest" in workflow_text or COSIGN_RELEASE in workflow_text or RELEASE_TAG in workflow_text:
        failures.append("workflow restates a moving label or a second pin")
    if any(HOST_EFFECTS.values()):
        failures.append(f"self-test touched a host effect: {HOST_EFFECTS}")
    unlinked: list[str] = []
    if perform_cleanup("/tmp/assay-darwin-feasibility", [], lambda path: unlinked.append(path), results_ready=True) != "clean":
        failures.append("empty owned cleanup was not clean")
    if perform_cleanup("/tmp/assay-darwin-feasibility", ["/etc/passwd"], lambda path: unlinked.append(path), results_ready=True) != "unknown":
        failures.append("escaped cleanup path was accepted")
    if unlinked:
        failures.append("cleanup invoked unlink for an unowned path")
    try:
        bounded_read([b"x" * 8], 4)
        failures.append("bounded read accepted an oversized chunk")
    except Refuse:
        pass
    profiles = build_profiles(_paths()["sentinel"], _paths()["decoy"])
    if profiles["P1-before"] != profiles["P1-after"]:
        failures.append("P1 profiles diverged")
    if profiles["P0"].replace("(deny network*)", "(allow network*)") != profiles["P1-before"]:
        failures.append("P0 and P1 differ by more than the network rule")
    hosted_collector_cases(failures)
    deadline_cases(failures)

    if failures:
        print(f"SELF-TEST RED {len(failures)}", file=sys.stderr)
        for item in failures:
            print(item, file=sys.stderr)
        return 1
    print("SELF-TEST GREEN")
    return 0


def git_version(text: str) -> str:
    for line in text.splitlines():
        if line.startswith("GitVersion:"):
            parts = line.split()
            if len(parts) == 2 and parts[1].startswith("v") and parts[1].count(".") == 2:
                return parts[1]
    raise Refuse("cosign pin")


def encode_result(payload: dict[str, Any]) -> bytes:
    data = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
    if len(data) > RECEIPT_MAX_BYTES:
        raise Refuse("receipt ceiling")
    return data


def hosted_entry(env: dict[str, str]) -> int:
    """Hosted measurement. Returns before any fetch when the runner context is absent."""
    if env.get("GITHUB_ACTIONS") != "true" or env.get("RUNNER_ENVIRONMENT") != "github-hosted" or env.get("RUNNER_OS") != "macOS":
        raise Refuse("hosted context")
    import os

    root = env.get("RUN_ROOT", "")
    if not root.startswith("/") or ".." in root.split("/"):
        raise Refuse("run root")
    if os.path.exists(root):
        raise Refuse("run root exists")
    sysctl = collect_child(["/usr/sbin/sysctl", "-n", "sysctl.proc_translated"], timeout=5)
    sysctl_text = sysctl.stdout.decode().strip()
    context = require_hosted_darwin(
        env,
        sysctl_ok=sysctl.returncode == 0,
        sysctl_value=sysctl_text,
        uname_m=os.uname().machine,
    )
    os.mkdir(root, 0o700)
    try:
        return _measure_hosted(env, context, root, sysctl_ok=sysctl.returncode == 0, sysctl_value=sysctl_text)
    except Refuse as exc:
        _write_result(root, {"verdict": "INCONCLUSIVE", "reason": str(exc), "cleanup": "unknown"})
        raise


def _write_result(root: str, payload: dict[str, Any]) -> None:
    import os

    results = root + "/results"
    os.makedirs(results, 0o700)
    path = results + "/verdict.json"
    blob = encode_result(payload)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        os.write(descriptor, blob)
    finally:
        os.close(descriptor)


def read_capped(path: str, max_bytes: int) -> bytes:
    """Read at most the ceiling. The extra byte is not retained."""
    if max_bytes <= 0:
        raise Refuse("ceiling")
    with open(path, "rb") as handle:
        blob = handle.read(max_bytes + 1)
    if len(blob) > max_bytes:
        raise Refuse("ceiling")
    return blob


def stage_bundles(
    assay: str,
    root: str,
    paths: dict[str, str],
    spawn: Callable[..., Any] | None,
    max_bundle_bytes: int = IMPORTED_BUNDLE_MAX_BYTES,
) -> str:
    decisions = root + "/decisions.ndjson"
    _write_exclusive(decisions, DECISION_NDJSON)
    for key in ("decoy", "real_bundle"):
        completed = collect_child(
            bundle_import_argv(assay, decisions, paths[key]), timeout=CHILD_DEADLINE_S, spawn=spawn
        )
        if completed.returncode != 0:
            raise Refuse("bundle import")
        blob = read_capped(paths[key], max_bundle_bytes)
        if not blob or blob == PLACEHOLDER_BUNDLE:
            raise Refuse("bundle import")
    return decisions


def _measure_hosted(
    env: dict[str, str],
    context: dict[str, str],
    root: str,
    *,
    sysctl_ok: bool,
    sysctl_value: str,
    spawn: Callable[..., Any] | None = None,
    fetch: Callable[[dict[str, str]], dict[str, bytes]] | None = None,
    bind: Callable[[], tuple[Any, str]] | None = None,
    host_ops: Any = None,
    host_tcp: Callable[[dict[str, str]], list[dict[str, Any]]] | None = None,
) -> int:
    """Acquire the pinned CLI and measure sandbox-exec. An invalid bundle cannot PASS."""
    import os

    del env
    version = collect_child(["cosign", "version"], timeout=20, spawn=spawn)
    pin = git_version(version.stdout.decode())
    sys.stdout.write(f"cosign_release={pin}\n")
    if pin != COSIGN_RELEASE:
        raise Refuse("cosign pin")
    assets = (fetch or _fetch_release)(context)
    downloads = root + "/downloads"
    os.mkdir(downloads, 0o700)
    archive_path = downloads + "/" + context["archive"]
    manifest_path = downloads + "/checksums.txt"
    sig_path = downloads + "/checksums.txt.sigstore.json"
    _write_exclusive(archive_path, assets["archive"])
    _write_exclusive(manifest_path, assets["manifest"])
    _write_exclusive(sig_path, assets["bundle"])
    verified = collect_child(
        cosign_verify_argv("cosign", sig_path, manifest_path, context["identity"]),
        timeout=120,
        spawn=spawn,
    )
    authenticate_release(
        cosign_version=pin,
        verify_exit=verified.returncode,
        manifest_text=assets["manifest"].decode("ascii"),
        archive_bytes=assets["archive"],
        selected=context["archive"],
    )
    extract = root + "/extract"
    _extract_archive(assets["archive"], extract)
    assay = _one_assay(extract)
    identity = collect_child([assay, "version"], timeout=20, spawn=spawn)
    if identity.stdout.decode().strip() != RELEASE_TAG[1:]:
        raise Refuse("assay version")
    sw = collect_child(["/usr/bin/sw_vers"], timeout=5, spawn=spawn)
    listing = collect_child(["/bin/ls", "-l", "/usr/bin/sandbox-exec"], timeout=5, spawn=spawn)
    sw_text = sw.stdout.decode()
    listing_text = listing.stdout.decode()
    if listing.returncode != 0 or not sw_text.strip():
        raise Refuse("tool record")
    product = next((line.split(":", 1)[1].strip() for line in sw_text.splitlines() if line.startswith("ProductVersion:")), "")
    if not product:
        raise Refuse("tool record")
    paths = {"sentinel": root + "/sentinel", "decoy": root + "/decoy.tar.gz", "real_bundle": root + "/real.tar.gz"}
    decisions = stage_bundles(assay, root, paths, spawn)
    listener, loopback = (bind or _loopback_endpoint)()
    endpoints = {"tcp_loopback": loopback, "tcp_external": "1.1.1.1:443"}
    try:
        receipts = assemble_receipts(
            profiles=build_profiles(paths["sentinel"], paths["decoy"]),
            paths=paths,
            endpoints=endpoints,
            context={
                "os_product": product,
                "uname_m": context["uname_m"],
                "runner_arch": context["runner_arch"],
                "proc_translated": 0 if not sysctl_ok or sysctl_value == "0" else 1,
                "translation_known": bool(sysctl_ok and sysctl_value == "0"),
                "runner_environment": "github-hosted",
                "runner_os": "macOS",
                "image_os": context["image_os"],
                "image_version": context["image_version"],
                "expect_label": context["label"],
                "sandbox_exec_listing": listing_text.strip(),
                "python_version": sys.version.split()[0],
                "cosign_release": pin,
                "sw_vers": sw_text.strip(),
            },
            runner=lambda _name, argv: _sandboxed_phase(argv, paths, endpoints, spawn=spawn),
            verifier=lambda argv: _verifier_process(argv, spawn=spawn),
            hasher=lambda path: hashlib.sha256(read_capped(path, IMPORTED_BUNDLE_MAX_BYTES)).hexdigest(),
            effects=[],
            host_scope_ops=host_ops or _filesystem_ops(),
            host_positive=host_tcp or _host_tcp,
            executable=sys.executable,
            script=__file__,
            assay=assay,
        )
        owned = [
            root + "/downloads",
            root + "/extract",
            decisions,
            paths["sentinel"],
            paths["decoy"],
            paths["real_bundle"],
        ]
        receipts["cleanup"] = perform_cleanup(
            root, [item for item in owned if os.path.lexists(item)], _remove_owned, results_ready=True
        )
        verdict = evaluate(receipts)
    finally:
        listener.close()
    _write_result(
        root,
        {
            "verdict": verdict.verdict,
            "reason": verdict.reason,
            "non_claims": [
                "tcp_loopback_and_one_external_tcp_only",
                "launch_definition_unchanged",
                "valid_bundle_is_not_synthesized",
            ],
            "receipt": receipts,
        },
    )
    return 0 if verdict.verdict == "PASS" else 1


def _write_exclusive(path: str, data: bytes) -> None:
    import os

    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        os.write(descriptor, data)
    finally:
        os.close(descriptor)


def _fetch_release(context: dict[str, str]) -> dict[str, bytes]:
    import urllib.request

    HOST_EFFECTS["fetch"] += 1

    urls = release_urls(context["repository"], context["archive"])

    def read_url(url: str, max_bytes: int) -> bytes:
        request = urllib.request.Request(url, headers={"User-Agent": "assay-darwin-feasibility"})
        with urllib.request.urlopen(request, timeout=60) as response:  # noqa: S310
            chunks: list[bytes] = []
            total = 0
            while True:
                chunk = response.read(65536)
                if not chunk:
                    break
                total += len(chunk)
                if total > max_bytes:
                    raise Refuse("ceiling")
                chunks.append(chunk)
        return bounded_read(chunks, max_bytes)

    return {
        "archive": read_url(urls["archive"], ARCHIVE_MAX_BYTES),
        "manifest": read_url(urls["manifest"], MANIFEST_MAX_BYTES),
        "bundle": read_url(urls["bundle"], BUNDLE_MAX_BYTES),
    }


def _extract_archive(blob: bytes, dest: str, max_decoded_bytes: int = DECODED_MAX_BYTES) -> None:
    import io
    import os
    import tarfile

    if max_decoded_bytes <= 0:
        raise Refuse("ceiling")
    with tarfile.open(fileobj=io.BytesIO(blob), mode="r:gz") as archive:
        members = archive.getmembers()
        total = 0
        for member in members:
            if member.name.startswith("/") or ".." in member.name.split("/") or member.issym() or member.islnk():
                raise Refuse("archive path")
            if member.isreg():
                if member.size < 0 or member.size > max_decoded_bytes:
                    raise Refuse("ceiling")
                total += member.size
                if total > max_decoded_bytes:
                    raise Refuse("ceiling")
        os.mkdir(dest, 0o700)
        if hasattr(tarfile, "data_filter"):
            archive.extractall(dest, members=members, filter="data")
        else:
            archive.extractall(dest, members=members)


def _one_assay(dest: str) -> str:
    import os

    found = [os.path.join(dirpath, "assay") for dirpath, _dirs, files in os.walk(dest) if "assay" in files]
    if len(found) != 1:
        raise Refuse("assay binary")
    return found[0]


def _loopback_endpoint() -> tuple[Any, str]:
    import socket
    import threading

    server = socket.socket()
    server.bind(("127.0.0.1", 0))
    server.listen(16)
    server.settimeout(0.2)

    def accept_loop() -> None:
        while True:
            try:
                conn, _addr = server.accept()
            except TimeoutError:
                continue
            except OSError:
                return
            conn.close()

    threading.Thread(target=accept_loop, daemon=True).start()
    return server, f"127.0.0.1:{server.getsockname()[1]}"


def _filesystem_ops() -> Any:
    import errno as errno_mod
    import os

    def status(action: Callable[[], None], allow: str) -> Any:
        try:
            action()
        except OSError as exc:
            return _status("deny", errno_mod.errorcode.get(exc.errno, "EUNKNOWN"))
        return _status(allow, None)

    def write(path: str) -> Any:
        def act() -> None:
            descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            os.close(descriptor)

        return status(act, "allow")

    def read(path: str) -> Any:
        def act() -> None:
            descriptor = os.open(path, os.O_RDONLY)
            os.close(descriptor)

        return status(act, "allow")

    def connect(endpoint: str) -> Any:
        return _tcp_status(endpoint)

    def spawn() -> None:
        return None

    return type(
        "Ops",
        (),
        {"write": staticmethod(write), "read": staticmethod(read), "connect": staticmethod(connect), "spawn": staticmethod(spawn)},
    )()


def _tcp_status(endpoint: str) -> Any:
    import errno as errno_mod
    import socket

    HOST_EFFECTS["network"] += 1
    host, port = endpoint.rsplit(":", 1)
    try:
        sock = socket.create_connection((host, int(port)), timeout=NETWORK_DEADLINE_S)
        sock.close()
    except OSError as exc:
        return _status("deny", errno_mod.errorcode.get(exc.errno, "EUNKNOWN"))
    return _status("connect", None)


def _host_tcp(endpoints: dict[str, str]) -> list[dict[str, Any]]:
    rows = []
    for name in MEASURED_OPERATIONS:
        status = _tcp_status(endpoints[name])
        rows.append({"op": name, "result": status.result, "errno": status.errno})
    return rows


def _remove_owned(path: str) -> None:
    import os
    import shutil

    if os.path.isdir(path):
        shutil.rmtree(path)
    else:
        os.unlink(path)


def _sandboxed_phase(
    argv: list[str],
    paths: dict[str, str],
    endpoints: dict[str, str],
    spawn: Callable[..., Any] | None = None,
) -> dict[str, Any]:
    import os
    import subprocess

    if spawn is None:
        HOST_EFFECTS["sandbox"] += 1
    child_env = os.environ.copy()
    child_env.update(
        {
            "ASSAY_DARWIN_OFFLINE_CHILD": "1",
            "ASSAY_SENTINEL": paths["sentinel"],
            "ASSAY_DECOY": paths["decoy"],
            "ASSAY_LOOPBACK": endpoints["tcp_loopback"],
            "ASSAY_EXTERNAL": endpoints["tcp_external"],
        }
    )
    try:
        completed = collect_child(argv, timeout=CHILD_DEADLINE_S, env=child_env, spawn=spawn)
    except subprocess.TimeoutExpired:
        return {"profile_executed": True, "timed_out": True}
    if completed.returncode != 0 and not completed.stdout:
        return {"profile_executed": False, "timed_out": False}
    try:
        payload = json.loads(completed.stdout.decode())
    except json.JSONDecodeError:
        return {"profile_executed": False, "timed_out": False}
    return {
        "profile_executed": True,
        "timed_out": False,
        "observations": payload.get("observations", []),
        "grandchild": payload.get("grandchild"),
    }


def _verifier_process(argv: list[str], spawn: Callable[..., Any] | None = None) -> dict[str, Any]:
    completed = collect_child(argv, timeout=CHILD_DEADLINE_S, spawn=spawn)
    stdout = completed.stdout.decode(errors="replace")
    stderr = completed.stderr.decode(errors="replace")
    record = {
        "exit": completed.returncode,
        "stdout": stdout,
        "stderr": stderr,
        "bundle": command_bundle(argv),
        "read_errno": observed_read_errno(stdout, stderr),
        "readable": False,
        "valid_bundle": False,
    }
    record["valid_bundle"] = argv[0] != "/usr/bin/sandbox-exec" and _report_success(record)
    record["readable"] = record["valid_bundle"]
    return record


def child_entry(role: str) -> int:
    import os

    paths = {"sentinel": os.environ["ASSAY_SENTINEL"], "decoy": os.environ["ASSAY_DECOY"]}
    endpoints = {"tcp_loopback": os.environ["ASSAY_LOOPBACK"], "tcp_external": os.environ["ASSAY_EXTERNAL"]}
    ops = _filesystem_ops()

    def spawn() -> Any:
        import subprocess

        try:
            completed = collect_child(
                [sys.executable, __file__, "--grandchild"],
                timeout=CHILD_DEADLINE_S,
                env=os.environ.copy(),
            )
        except (Refuse, subprocess.TimeoutExpired):
            return None
        try:
            return json.loads(completed.stdout.decode()).get("grandchild")
        except json.JSONDecodeError:
            return None

    ops.spawn = spawn
    observations, grandchild = child_observations(ops, endpoints, paths, spawn_grandchild=role == "--child")
    if role == "--grandchild":
        found = {item["op"]: item for item in observations}
        payload: dict[str, Any] = {
            "grandchild": {
                "present": True,
                "phase": "P0",
                "sentinel_write": found["sentinel_write"]["result"],
                "errno": found["sentinel_write"]["errno"],
                "tcp_external": found["tcp_external"]["result"],
            }
        }
    else:
        payload = {"observations": observations, "grandchild": grandchild}
    encoded = json.dumps(payload).encode()
    if len(encoded) > OUTPUT_MAX_BYTES:
        return 2
    sys.stdout.buffer.write(encoded)
    return 0


def main(argv: list[str]) -> int:
    if argv == ["--self-test"]:
        return self_test()
    if argv == ["--print-cosign-release"]:
        sys.stdout.write(f"release={COSIGN_RELEASE}\n")
        return 0
    if argv == ["--hosted"]:
        import os

        try:
            return hosted_entry(dict(os.environ))
        except Refuse as exc:
            sys.stderr.write(f"refusing: {exc}\n")
            return 2
    if argv in (["--child"], ["--grandchild"]):
        import os

        if os.environ.get("ASSAY_DARWIN_OFFLINE_CHILD") != "1":
            sys.stderr.write("refusing: child probe starts only after hosted context is verified\n")
            return 2
        return child_entry(argv[0])
    sys.stderr.write("refusing: hosted Darwin execution requires --hosted and a verified GitHub-hosted runner\n")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
