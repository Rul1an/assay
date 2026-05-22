#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [ "$(uname -s)" != "Linux" ]; then
  echo "SKIP: runner-spike OpenAI Agents kernel+policy acceptance requires Linux." >&2
  exit 40
fi

if ! command -v node >/dev/null 2>&1; then
  echo "SKIP: runner-spike OpenAI Agents fixture requires Node.js 22 or newer; node was not found on PATH." >&2
  exit 40
fi

if ! node -e 'process.exit(Number(process.versions.node.split(".")[0]) >= 22 ? 0 : 1)'; then
  echo "SKIP: runner-spike OpenAI Agents fixture requires Node.js 22 or newer." >&2
  exit 40
fi

OPENAI_FIXTURE_DIR="$ROOT/runner-fixtures/openai-agents"
if [ ! -d "$OPENAI_FIXTURE_DIR/node_modules/@openai/agents" ]; then
  echo "SKIP: missing fixture dependency '@openai/agents' under $OPENAI_FIXTURE_DIR/node_modules." >&2
  echo "Hint: run 'npm ci --ignore-scripts --no-audit --no-fund' in $OPENAI_FIXTURE_DIR." >&2
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
  cleanup_tmp_root() {
    :
  }
else
  TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/assay-runner-openai-agents-kernel-policy.XXXXXX")"
  cleanup_tmp_root() {
    rm -rf -- "$TMP_ROOT"
  }
fi

if [ -d /dev/shm ]; then
  CONTROL_ROOT="$(mktemp -d /dev/shm/assay-runner-openai-agents-control.XXXXXX)"
else
  CONTROL_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/assay-runner-openai-agents-control.XXXXXX")"
fi
cleanup_all() {
  cleanup_tmp_root
  rm -rf -- "$CONTROL_ROOT"
}
trap cleanup_all EXIT

WORK_DIR="${ASSAY_RUNNER_ACCEPTANCE_WORK_DIR:-$TMP_ROOT/work}"
EXTRACT_DIR="$TMP_ROOT/extract"
BUNDLE="$TMP_ROOT/runner-openai-agents-kernel-policy.tar.gz"
SDK_LOG="$CONTROL_ROOT/sdk-events.ndjson"
DECISION_LOG="$CONTROL_ROOT/policy-decisions.ndjson"
RUN_ID="${ASSAY_RUNNER_ACCEPTANCE_RUN_ID:-run_openai_agents_kernel_policy_acceptance}"
AGENT_SCRIPT="$CONTROL_ROOT/openai-agents-sdk-policy-agent.sh"
OPENAI_FIXTURE_AGENT="$CONTROL_ROOT/fixture-agent.js"
POLICY_AGENT="$CONTROL_ROOT/mcp-policy-agent.sh"
MCP_FILE_SERVER="$CONTROL_ROOT/mcp_file_server.py"
# The full S5 gate requires the SDK and policy layers to share the policy
# fixture's stable tool_call_id. Mismatch behavior is covered by separate gates.
SDK_TOOL_CALL_ID="tc_runner_policy_001"
EXPECTED_SDK_SOURCE="${ASSAY_RUNNER_ACCEPTANCE_EXPECT_SDK_SOURCE:-openai-agents-fixture}"
EXPECTED_SDK_VERSION="${ASSAY_RUNNER_ACCEPTANCE_EXPECT_SDK_VERSION:-0.11.4}"

export ASSAY_BIN
export ASSAY_FIXTURE_ROOT="$ROOT"
# Export fixture directory explicitly so the wrapper's
# `${ASSAY_RUNNER_OPENAI_FIXTURE_DIR:-$(dirname BASH_SOURCE)}` default
# resolves to the runner-fixtures path even when the script is copied
# to $CONTROL_ROOT (BASH_SOURCE would otherwise point at the temp
# control root). Same defensive pattern as Slice 5A for Gemini.
export ASSAY_RUNNER_OPENAI_FIXTURE_DIR="$OPENAI_FIXTURE_DIR"
export ASSAY_RUNNER_OPENAI_FIXTURE_AGENT="$OPENAI_FIXTURE_AGENT"
export ASSAY_RUNNER_MCP_FILE_SERVER="$MCP_FILE_SERVER"
export ASSAY_RUNNER_POLICY_AGENT="$POLICY_AGENT"
export ASSAY_RUNNER_POLICY_DECISION_LOG="$DECISION_LOG"
export ASSAY_RUNNER_RUN_ID="$RUN_ID"
export ASSAY_RUNNER_SDK_TOOL_CALL_ID="$SDK_TOOL_CALL_ID"

cp "$ROOT/runner-fixtures/openai-agents/sdk-policy-agent.sh" "$AGENT_SCRIPT"
cp "$ROOT/runner-fixtures/openai-agents/fixture-agent.js" "$OPENAI_FIXTURE_AGENT"
cp "$ROOT/tests/fixtures/runner-spike/mcp-policy-agent.sh" "$POLICY_AGENT"
cp "$ROOT/tests/fixtures/runner-spike/mcp_file_server.py" "$MCP_FILE_SERVER"
chmod +x "$AGENT_SCRIPT" "$OPENAI_FIXTURE_AGENT" "$POLICY_AGENT" "$MCP_FILE_SERVER"

mkdir -p "$WORK_DIR"
printf '%s\n' "openai agents fixture input" > "$WORK_DIR/openai-agents-input.txt"
printf '%s\n' "assay runner policy fixture input" > "$WORK_DIR/policy-input.txt"

"$ASSAY_BIN" runner-spike run \
  --agent-shim openai-agents \
  --kernel-capture \
  --ebpf "$ASSAY_EBPF_PATH" \
  --sdk-event-log "$SDK_LOG" \
  --policy-decision-log "$DECISION_LOG" \
  --run-id "$RUN_ID" \
  --output "$BUNDLE" \
  -- "$AGENT_SCRIPT" "$WORK_DIR"

mkdir -p "$EXTRACT_DIR"
tar -xzf "$BUNDLE" -C "$EXTRACT_DIR"

python3 - "$EXTRACT_DIR" "$WORK_DIR" "$RUN_ID" "$SDK_TOOL_CALL_ID" "$EXPECTED_SDK_SOURCE" "$EXPECTED_SDK_VERSION" "$DECISION_LOG" <<'PY'
import hashlib
import json
import sys
from collections import Counter
from pathlib import Path

extract_dir = Path(sys.argv[1])
work_dir = Path(sys.argv[2]).resolve()
run_id = sys.argv[3]
sdk_tool_call_id = sys.argv[4]
expected_sdk_source = sys.argv[5]
expected_sdk_version = sys.argv[6]
decision_log = Path(sys.argv[7])
policy_tool_call_id = "tc_runner_policy_001"


def fail(message: str) -> None:
    raise SystemExit(f"FAIL: {message}")


def expect(condition: bool, message: str) -> None:
    if not condition:
        fail(message)


def read_json(path: str):
    return json.loads((extract_dir / path).read_text(encoding="utf-8"))


def read_kernel_events():
    events = []
    for line_number, line in enumerate(
        (extract_dir / "layers/kernel.ndjson").read_text(encoding="utf-8").splitlines(),
        start=1,
    ):
        try:
            events.append(json.loads(line))
        except json.JSONDecodeError as error:
            fail(f"kernel event {line_number} is not valid JSON: {error}")
    return events


def print_kernel_event_summary(events) -> None:
    kind_counts = Counter(event.get("kind") for event in events)
    value_counts = Counter(
        (event.get("kind"), event.get("value"))
        for event in events
        if event.get("value") is not None
    )
    print("kernel event kind counts:")
    for kind, count in kind_counts.most_common():
        print(f"  {kind}: {count}")
    print("top kernel event values:")
    for (kind, value), count in value_counts.most_common(20):
        print(f"  {kind}: {count} {value}")


manifest = read_json("manifest.json")
health = read_json("observation-health.json")
surface = read_json("capability-surface.json")
correlation = read_json("correlation-report.json")
kernel_events = read_kernel_events()

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

if health["kernel_layer"] != "complete" or health["ringbuf_drops"] != 0:
    print("observation-health:")
    print(json.dumps(health, indent=2, sort_keys=True))
    print_kernel_event_summary(kernel_events)
expect(health["kernel_layer"] == "complete", f"kernel_layer must be complete, got {health['kernel_layer']!r}")
expect(health["ringbuf_drops"] == 0, f"ringbuf_drops must be 0, got {health['ringbuf_drops']!r}")
expect(health["policy_layer"] == "present", f"policy_layer must be present, got {health['policy_layer']!r}")
expect(health["sdk_layer"] == "self_reported", f"sdk_layer must be self_reported, got {health['sdk_layer']!r}")
expect(
    health["cgroup_correlation"] == "clean",
    f"cgroup_correlation must be clean, got {health['cgroup_correlation']!r}",
)
expect(
    any(note == "s5_sdk_capture: sdk_events=3 sdk_tool_calls=1" for note in health["notes"]),
    "sdk capture note missing",
)

expect(kernel_events, "kernel layer must contain events")
for line_number, event in enumerate(kernel_events, start=1):
    expect(
        event.get("schema") == "assay.runner.kernel_event.v0",
        f"kernel event {line_number} has unexpected schema: {event.get('schema')!r}",
    )

policy_events = [
    json.loads(line)
    for line in (extract_dir / "layers/policy.ndjson").read_text(encoding="utf-8").splitlines()
]
sdk_events = [
    json.loads(line)
    for line in (extract_dir / "layers/sdk.ndjson").read_text(encoding="utf-8").splitlines()
]
expect(len(policy_events) == 1, f"expected one policy event, got {len(policy_events)}")
expect(len(sdk_events) == 3, f"expected three sdk events, got {len(sdk_events)}")

policy_event = policy_events[0]
expect(policy_event["schema"] == "assay.runner.policy_event.v0", "unexpected policy event schema")
expect(policy_event["run_id"] == run_id, "policy event run_id mismatch")
expect(policy_event["tool_call_id"] == policy_tool_call_id, "policy event tool_call_id mismatch")
expect(policy_event["tool"] == "read_file", "policy event tool mismatch")
expect(policy_event["decision"] == "allow", "policy event decision mismatch")

source_events = [json.loads(line) for line in decision_log.read_text(encoding="utf-8").splitlines() if line.strip()]
expect(len(source_events) == 1, f"expected one source decision event, got {len(source_events)}")
expect(source_events[0]["type"] == "assay.tool.decision", "source decision event type mismatch")
expect(
    source_events[0]["data"]["tool_call_id"] == policy_tool_call_id,
    "source decision tool_call_id mismatch",
)

for expected_seq, event in enumerate(sdk_events):
    expect(event.get("schema") == "assay.runner.sdk_event.v0", f"sdk event {expected_seq} schema mismatch")
    expect(event.get("run_id") == run_id, f"sdk event {expected_seq} run_id mismatch")
    expect(event.get("seq") == expected_seq, f"sdk event {expected_seq} seq mismatch")
    expect(event.get("source") == expected_sdk_source, f"sdk event {expected_seq} source mismatch")
    expect(event.get("sdk_name") == "@openai/agents", f"sdk event {expected_seq} sdk_name mismatch")
    expect(event.get("sdk_version") == expected_sdk_version, f"sdk event {expected_seq} sdk_version mismatch")

sdk_tool_calls = {event.get("tool_call_id") for event in sdk_events if event.get("tool_call_id")}
sdk_tools = {event.get("tool") for event in sdk_events if event.get("tool")}
expect(sdk_tool_calls == {sdk_tool_call_id}, f"sdk tool_call_id mismatch: {sdk_tool_calls!r}")
expect(sdk_tool_call_id == policy_tool_call_id, "full S5 acceptance requires SDK and policy tool_call_id match")
expect(sdk_tools == {"read_file"}, f"sdk tool mismatch: {sdk_tools!r}")
expect(
    [event.get("event_type") for event in sdk_events]
    == ["tool_call_started", "tool_call_completed", "run_finished"],
    "sdk event sequence mismatch",
)

filesystem = set(surface.get("filesystem_paths", []))
expect(str(work_dir / "openai-agents-input.txt") in filesystem, "OpenAI Agents fixture input read was not recorded")
expect(str(work_dir / "policy-input.txt") in filesystem, "policy fixture input read was not recorded")
expect("read_file" in set(surface.get("mcp_tools", [])), "read_file MCP tool was not recorded")
expect(
    "allow:read_file" in set(surface.get("policy_decisions", [])),
    "allow:read_file policy decision was not recorded",
)

bindings = correlation.get("bindings", [])
expect(correlation["status"] == "clean", f"correlation status must be clean, got {correlation['status']!r}")
expect(correlation.get("ambiguities", []) == [], f"correlation ambiguities must be empty: {correlation.get('ambiguities', [])!r}")
expect(len(bindings) == 1, f"expected one correlation binding, got {len(bindings)}")
binding = bindings[0]
expect(binding["tool_call_id"] == policy_tool_call_id, "binding tool_call_id mismatch")
expect(binding["policy_decision"] == "allow", "binding policy_decision mismatch")
expect(binding["kernel_event_count"] > 0, "binding must include kernel events")
expect(binding["window"] == {"start": "run_started", "end": "run_finished"}, "binding window mismatch")

print("runner-spike OpenAI Agents kernel+policy archive verified")
PY

echo "PASS: runner-spike OpenAI Agents kernel+policy acceptance"
