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
CHILD_DEADLINE_S = 20
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
        error = denial.get("stderr") if isinstance(denial.get("stderr"), str) else ""
        if not error or error == success.get("stdout") or denial.get("stdout") == success.get("stdout"):
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
    import subprocess

    root = env.get("RUN_ROOT", "")
    if not root.startswith("/") or ".." in root.split("/"):
        raise Refuse("run root")
    if os.path.exists(root):
        raise Refuse("run root exists")
    sysctl = subprocess.run(
        ["/usr/sbin/sysctl", "-n", "sysctl.proc_translated"],
        capture_output=True,
        text=True,
        timeout=5,
    )
    context = require_hosted_darwin(
        env,
        sysctl_ok=sysctl.returncode == 0,
        sysctl_value=sysctl.stdout.strip(),
        uname_m=os.uname().machine,
    )
    os.mkdir(root, 0o700)
    try:
        return _measure_hosted(env, context, root, sysctl_ok=sysctl.returncode == 0, sysctl_value=sysctl.stdout.strip())
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


def _measure_hosted(env: dict[str, str], context: dict[str, str], root: str, *, sysctl_ok: bool, sysctl_value: str) -> int:
    """Acquire the pinned CLI and measure sandbox-exec. An invalid bundle cannot PASS."""
    import os
    import subprocess

    del env
    version = subprocess.run(["cosign", "version"], capture_output=True, text=True, timeout=20)
    pin = git_version(version.stdout)
    sys.stdout.write(f"cosign_release={pin}\n")
    if pin != COSIGN_RELEASE:
        raise Refuse("cosign pin")
    HOST_EFFECTS["fetch"] += 1
    assets = _fetch_release(context)
    downloads = root + "/downloads"
    os.mkdir(downloads, 0o700)
    archive_path = downloads + "/" + context["archive"]
    manifest_path = downloads + "/checksums.txt"
    sig_path = downloads + "/checksums.txt.sigstore.json"
    _write_exclusive(archive_path, assets["archive"])
    _write_exclusive(manifest_path, assets["manifest"])
    _write_exclusive(sig_path, assets["bundle"])
    verified = subprocess.run(
        cosign_verify_argv("cosign", sig_path, manifest_path, context["identity"]),
        capture_output=True,
        timeout=120,
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
    identity = subprocess.run([assay, "version"], capture_output=True, timeout=20)
    if identity.stdout.decode().strip() != RELEASE_TAG[1:]:
        raise Refuse("assay version")
    sw = subprocess.run(["/usr/bin/sw_vers"], capture_output=True, text=True, timeout=5)
    listing = subprocess.run(["/bin/ls", "-l", "/usr/bin/sandbox-exec"], capture_output=True, text=True, timeout=5)
    if listing.returncode != 0 or not sw.stdout.strip():
        raise Refuse("tool record")
    product = next((line.split(":", 1)[1].strip() for line in sw.stdout.splitlines() if line.startswith("ProductVersion:")), "")
    if not product:
        raise Refuse("tool record")
    paths = {"sentinel": root + "/sentinel", "decoy": root + "/decoy.tar.gz", "real_bundle": root + "/real.tar.gz"}
    _write_exclusive(paths["decoy"], b"not-a-valid-bundle")
    _write_exclusive(paths["real_bundle"], b"not-a-valid-bundle")
    listener, loopback = _loopback_endpoint()
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
                "sandbox_exec_listing": listing.stdout.strip(),
                "python_version": sys.version.split()[0],
                "cosign_release": pin,
                "sw_vers": sw.stdout.strip(),
            },
            runner=lambda _name, argv: _sandboxed_phase(argv, paths, endpoints),
            verifier=_verifier_process,
            hasher=lambda path: hashlib.sha256(Path(path).read_bytes()).hexdigest(),
            effects=[],
            host_scope_ops=_filesystem_ops(),
            host_positive=_host_tcp,
            executable=sys.executable,
            script=__file__,
            assay=assay,
        )
        owned = [root + "/downloads", root + "/extract", paths["sentinel"], paths["decoy"], paths["real_bundle"]]
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


def _extract_archive(blob: bytes, dest: str) -> None:
    import io
    import os
    import tarfile

    os.mkdir(dest, 0o700)
    with tarfile.open(fileobj=io.BytesIO(blob), mode="r:gz") as archive:
        for member in archive.getmembers():
            if member.name.startswith("/") or ".." in member.name.split("/") or member.issym() or member.islnk():
                raise Refuse("archive path")
        if hasattr(tarfile, "data_filter"):
            archive.extractall(dest, filter="data")
        else:
            archive.extractall(dest)


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


def _sandboxed_phase(argv: list[str], paths: dict[str, str], endpoints: dict[str, str]) -> dict[str, Any]:
    import os
    import subprocess

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
        completed = subprocess.run(argv, env=child_env, capture_output=True, timeout=CHILD_DEADLINE_S)
    except subprocess.TimeoutExpired:
        return {"profile_executed": True, "timed_out": True}
    if len(completed.stdout) > OUTPUT_MAX_BYTES:
        raise Refuse("output ceiling")
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


def _verifier_process(argv: list[str]) -> dict[str, Any]:
    import subprocess

    completed = subprocess.run(argv, capture_output=True, timeout=CHILD_DEADLINE_S)
    record = {
        "exit": completed.returncode,
        "stdout": completed.stdout[:OUTPUT_MAX_BYTES].decode(errors="replace"),
        "stderr": completed.stderr[:OUTPUT_MAX_BYTES].decode(errors="replace"),
        "bundle": command_bundle(argv),
        "read_errno": None,
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

        completed = subprocess.run(
            [sys.executable, __file__, "--grandchild"],
            env=os.environ.copy(),
            capture_output=True,
            timeout=CHILD_DEADLINE_S,
        )
        if len(completed.stdout) > OUTPUT_MAX_BYTES:
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
