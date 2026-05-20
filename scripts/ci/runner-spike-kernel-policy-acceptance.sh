#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [ "$(uname -s)" != "Linux" ]; then
  echo "SKIP: runner-spike kernel+policy acceptance requires Linux." >&2
  exit 40
fi

ASSAY_EBPF_PATH="${ASSAY_EBPF_PATH:-$ROOT/target/assay-ebpf.o}"
if [ ! -f "$ASSAY_EBPF_PATH" ]; then
  echo "SKIP: eBPF object not found: $ASSAY_EBPF_PATH" >&2
  echo "Hint: build it first, for example with: cargo xtask build-ebpf" >&2
  exit 40
fi

ASSAY_BIN="${ASSAY_BIN:-$ROOT/target/debug/assay}"
if [ ! -x "$ASSAY_BIN" ]; then
  cargo build -p assay-cli --no-default-features
fi

if [ -n "${ASSAY_RUNNER_ACCEPTANCE_ARTIFACT_DIR:-}" ]; then
  TMP_ROOT="$ASSAY_RUNNER_ACCEPTANCE_ARTIFACT_DIR"
  mkdir -p "$TMP_ROOT"
  cleanup() {
    :
  }
else
  TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/assay-runner-kernel-policy.XXXXXX")"
  cleanup() {
    rm -rf -- "$TMP_ROOT"
  }
fi
trap cleanup EXIT

WORK_DIR="${ASSAY_RUNNER_ACCEPTANCE_WORK_DIR:-$TMP_ROOT/work}"
EXTRACT_DIR="$TMP_ROOT/extract"
BUNDLE="$TMP_ROOT/runner-kernel-policy.tar.gz"
DECISION_LOG="$TMP_ROOT/policy-decisions.ndjson"
RUN_ID="${ASSAY_RUNNER_ACCEPTANCE_RUN_ID:-run_kernel_policy_acceptance}"

export ASSAY_BIN
export ASSAY_RUNNER_POLICY_DECISION_LOG="$DECISION_LOG"
export ASSAY_RUNNER_RUN_ID="$RUN_ID"

"$ASSAY_BIN" runner-spike run \
  --agent-shim none \
  --kernel-capture \
  --ebpf "$ASSAY_EBPF_PATH" \
  --policy-decision-log "$DECISION_LOG" \
  --run-id "$RUN_ID" \
  --output "$BUNDLE" \
  -- "$ROOT/tests/fixtures/runner-spike/mcp-policy-agent.sh" "$WORK_DIR"

mkdir -p "$EXTRACT_DIR"
tar -xzf "$BUNDLE" -C "$EXTRACT_DIR"

python3 - "$EXTRACT_DIR" "$WORK_DIR" "$RUN_ID" "$DECISION_LOG" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

extract_dir = Path(sys.argv[1])
work_dir = Path(sys.argv[2]).resolve()
run_id = sys.argv[3]
decision_log = Path(sys.argv[4])


def fail(message: str) -> None:
    raise SystemExit(f"FAIL: {message}")


def expect(condition: bool, message: str) -> None:
    if not condition:
        fail(message)


def read_json(path: str):
    return json.loads((extract_dir / path).read_text(encoding="utf-8"))


manifest = read_json("manifest.json")
health = read_json("observation-health.json")
surface = read_json("capability-surface.json")
correlation = read_json("correlation-report.json")

expect(manifest["schema"] == "assay.runner.archive_manifest.v0", "unexpected manifest schema")
expect(health["schema"] == "assay.runner.observation_health.v0", "unexpected health schema")
expect(surface["schema"] == "assay.runner.capability_surface.v0", "unexpected surface schema")
expect(correlation["schema"] == "assay.runner.correlation_report.v0", "unexpected correlation schema")

for name, document in {
    "manifest": manifest,
    "observation-health": health,
    "capability-surface": surface,
    "correlation-report": correlation,
}.items():
    expect(document["run_id"] == run_id, f"{name} run_id mismatch: {document['run_id']!r}")

for relative_path, entry in manifest["files"].items():
    payload = (extract_dir / relative_path).read_bytes()
    digest = "sha256:" + hashlib.sha256(payload).hexdigest()
    expect(entry["path"] == relative_path, f"manifest path mismatch for {relative_path}")
    expect(entry["bytes"] == len(payload), f"manifest byte count mismatch for {relative_path}")
    expect(entry["sha256"] == digest, f"manifest sha256 mismatch for {relative_path}")

expect(health["kernel_layer"] == "complete", f"kernel_layer must be complete, got {health['kernel_layer']!r}")
expect(health["ringbuf_drops"] == 0, f"ringbuf_drops must be 0, got {health['ringbuf_drops']!r}")
expect(health["policy_layer"] == "present", f"policy_layer must be present, got {health['policy_layer']!r}")
expect(health["sdk_layer"] == "absent", f"sdk_layer must be absent, got {health['sdk_layer']!r}")
expect(
    health["cgroup_correlation"] == "clean",
    f"cgroup_correlation must be clean, got {health['cgroup_correlation']!r}",
)

expect((extract_dir / "layers/sdk.ndjson").read_text(encoding="utf-8") == "", "sdk layer must be empty")
kernel_events = (extract_dir / "layers/kernel.ndjson").read_text(encoding="utf-8").splitlines()
expect(kernel_events, "kernel layer must contain events")
for line_number, line in enumerate(kernel_events, start=1):
    event = json.loads(line)
    expect(
        event.get("schema") == "assay.runner.kernel_event.v0",
        f"kernel event {line_number} has unexpected schema: {event.get('schema')!r}",
    )

policy_events = (extract_dir / "layers/policy.ndjson").read_text(encoding="utf-8").splitlines()
expect(len(policy_events) == 1, f"expected one policy event, got {len(policy_events)}")
policy_event = json.loads(policy_events[0])
expect(policy_event["schema"] == "assay.runner.policy_event.v0", "unexpected policy event schema")
expect(policy_event["run_id"] == run_id, "policy event run_id mismatch")
expect(policy_event["tool_call_id"] == "tc_runner_policy_001", "policy event tool_call_id mismatch")
expect(policy_event["tool"] == "read_file", "policy event tool mismatch")
expect(policy_event["decision"] == "allow", "policy event decision mismatch")

source_events = [json.loads(line) for line in decision_log.read_text(encoding="utf-8").splitlines() if line.strip()]
expect(len(source_events) == 1, f"expected one source decision event, got {len(source_events)}")
expect(source_events[0]["type"] == "assay.tool.decision", "source decision event type mismatch")
expect(
    source_events[0]["data"]["tool_call_id"] == "tc_runner_policy_001",
    "source decision tool_call_id mismatch",
)

filesystem = set(surface.get("filesystem_prefixes", []))
expect(str(work_dir / "policy-input.txt") in filesystem, "fixture policy input read was not recorded")
expect("read_file" in set(surface.get("mcp_tools", [])), "read_file MCP tool was not recorded")
expect(
    "allow:read_file" in set(surface.get("policy_decisions", [])),
    "allow:read_file policy decision was not recorded",
)

bindings = correlation.get("bindings", [])
expect(len(bindings) == 1, f"expected one correlation binding, got {len(bindings)}")
binding = bindings[0]
expect(binding["tool_call_id"] == "tc_runner_policy_001", "binding tool_call_id mismatch")
expect(binding["policy_decision"] == "allow", "binding policy_decision mismatch")
expect(binding["kernel_event_count"] > 0, "binding must include kernel events")
expect(binding["window"] == {"start": "run_started", "end": "run_finished"}, "binding window mismatch")

print("runner-spike kernel+policy archive verified")
PY

echo "PASS: runner-spike kernel+policy acceptance"
