#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [ -n "${ASSAY_BIN:-}" ]; then
  if [ ! -x "$ASSAY_BIN" ]; then
    echo "ERROR: ASSAY_BIN is not executable: $ASSAY_BIN" >&2
    exit 2
  fi
else
  cargo build -p assay-cli --no-default-features
  ASSAY_BIN="$ROOT/target/debug/assay"
fi

if [ -n "${ASSAY_RUNNER_ACCEPTANCE_ARTIFACT_DIR:-}" ]; then
  TMP_ROOT="$ASSAY_RUNNER_ACCEPTANCE_ARTIFACT_DIR"
  mkdir -p "$TMP_ROOT"
  cleanup() {
    :
  }
else
  TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/assay-runner-sdk-policy.XXXXXX")"
  cleanup() {
    rm -rf -- "$TMP_ROOT"
  }
fi
trap cleanup EXIT

WORK_DIR="${ASSAY_RUNNER_ACCEPTANCE_WORK_DIR:-$TMP_ROOT/work}"
EXTRACT_DIR="$TMP_ROOT/extract"
BUNDLE="$TMP_ROOT/runner-sdk-policy.tar.gz"
SDK_LOG="$TMP_ROOT/sdk-events.ndjson"
DECISION_LOG="$TMP_ROOT/policy-decisions.ndjson"
RUN_ID="${ASSAY_RUNNER_ACCEPTANCE_RUN_ID:-run_sdk_policy_correlation}"
SDK_TOOL_CALL_ID="${ASSAY_RUNNER_ACCEPTANCE_SDK_TOOL_CALL_ID:-tc_runner_policy_001}"
EXPECT_SDK_POLICY_MISMATCH="${ASSAY_RUNNER_ACCEPTANCE_EXPECT_SDK_POLICY_MISMATCH:-0}"

export ASSAY_BIN
export ASSAY_RUNNER_POLICY_DECISION_LOG="$DECISION_LOG"
export ASSAY_RUNNER_RUN_ID="$RUN_ID"
export ASSAY_RUNNER_SDK_TOOL_CALL_ID="$SDK_TOOL_CALL_ID"

"$ASSAY_BIN" runner-spike run \
  --agent-shim openai-agents \
  --sdk-event-log "$SDK_LOG" \
  --policy-decision-log "$DECISION_LOG" \
  --run-id "$RUN_ID" \
  --output "$BUNDLE" \
  -- "$ROOT/tests/fixtures/runner-spike/sdk-policy-agent.sh" "$WORK_DIR"

mkdir -p "$EXTRACT_DIR"
tar -xzf "$BUNDLE" -C "$EXTRACT_DIR"

python3 - "$EXTRACT_DIR" "$RUN_ID" "$SDK_TOOL_CALL_ID" "$EXPECT_SDK_POLICY_MISMATCH" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

extract_dir = Path(sys.argv[1])
run_id = sys.argv[2]
sdk_tool_call_id = sys.argv[3]
expect_sdk_policy_mismatch = sys.argv[4] == "1"
policy_tool_call_id = "tc_runner_policy_001"


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

for relative_path, entry in manifest["files"].items():
    payload = (extract_dir / relative_path).read_bytes()
    digest = "sha256:" + hashlib.sha256(payload).hexdigest()
    expect(entry["path"] == relative_path, f"manifest path mismatch for {relative_path}")
    expect(entry["bytes"] == len(payload), f"manifest byte count mismatch for {relative_path}")
    expect(entry["sha256"] == digest, f"manifest sha256 mismatch for {relative_path}")

expect(health["run_id"] == run_id, "health run_id mismatch")
expect(health["sdk_layer"] == "self_reported", f"sdk_layer mismatch: {health['sdk_layer']!r}")
expect(health["policy_layer"] == "present", f"policy_layer mismatch: {health['policy_layer']!r}")
expect(health["kernel_layer"] == "absent", f"kernel_layer mismatch: {health['kernel_layer']!r}")
expect(
    any(note == "s5_sdk_capture: sdk_events=3 sdk_tool_calls=1" for note in health["notes"]),
    "sdk capture note missing",
)

sdk_events = [json.loads(line) for line in (extract_dir / "layers/sdk.ndjson").read_text(encoding="utf-8").splitlines()]
policy_events = [json.loads(line) for line in (extract_dir / "layers/policy.ndjson").read_text(encoding="utf-8").splitlines()]
expect(len(sdk_events) == 3, f"expected three sdk events, got {len(sdk_events)}")
expect(len(policy_events) == 1, f"expected one policy event, got {len(policy_events)}")

sdk_tool_calls = {event.get("tool_call_id") for event in sdk_events if event.get("tool_call_id")}
policy_tool_calls = {event.get("tool_call_id") for event in policy_events}
expect(sdk_tool_calls == {sdk_tool_call_id}, f"sdk tool_call_id mismatch: {sdk_tool_calls!r}")
expect(policy_tool_calls == {policy_tool_call_id}, f"policy tool_call_id mismatch: {policy_tool_calls!r}")

expect("read_file" in set(surface.get("mcp_tools", [])), "read_file MCP tool missing")
expect("allow:read_file" in set(surface.get("policy_decisions", [])), "allow:read_file policy decision missing")

bindings = correlation.get("bindings", [])
expect(len(bindings) == 1, f"expected one policy correlation binding, got {len(bindings)}")
expect(bindings[0]["tool_call_id"] == policy_tool_call_id, "binding tool_call_id mismatch")
ambiguities = correlation.get("ambiguities", [])
expected_ambiguity = f"sdk_tool_call_without_policy_binding:{sdk_tool_call_id}"
if expect_sdk_policy_mismatch:
    expect(sdk_tool_call_id != policy_tool_call_id, "mismatch mode must use a distinct SDK tool_call_id")
    expect(correlation["status"] == "partial", f"correlation status must be partial, got {correlation['status']!r}")
    expect(expected_ambiguity in ambiguities, f"expected mismatch ambiguity missing: {ambiguities!r}")
else:
    expect(sdk_tool_call_id == policy_tool_call_id, "match mode must use the policy tool_call_id")
    expect(
        not any(item.startswith("sdk_tool_call_without_policy_binding:") for item in ambiguities),
        f"sdk/policy mismatch ambiguity present: {ambiguities!r}",
    )

print("runner-spike SDK+policy correlation verified")
PY

echo "PASS: runner-spike SDK+policy correlation"
