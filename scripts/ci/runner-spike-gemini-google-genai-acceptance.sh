#!/usr/bin/env bash
# Gemini Python google-genai second-runtime acceptance script.
#
# Mirrors scripts/ci/runner-spike-openai-agents-kernel-policy-acceptance.sh
# structurally. The fixture-specific differences:
# - Python venv with google-genai instead of Node.js with @openai/agents
# - cassette-replay instead of DeterministicToolCallModel
# - tool_call_id comes from cassette (FunctionCall.id), not from a hardcoded
#   ASSAY_RUNNER_SDK_TOOL_CALL_ID
# - expected SDK source/name strings reflect the Gemini fixture
#
# Per #1307, this acceptance asserts the full kernel+policy+SDK shape with
# the same level-3 stable-identity rule used for S5: SDK and policy
# tool_call_id must equal each other, and that value must match the
# cassette's FunctionCall.id.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [ "$(uname -s)" != "Linux" ]; then
  echo "SKIP: runner-spike Gemini kernel+policy acceptance requires Linux." >&2
  exit 40
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "SKIP: runner-spike Gemini fixture requires python3 on PATH." >&2
  exit 40
fi

GEMINI_FIXTURE_DIR="$ROOT/tests/fixtures/runner-spike/gemini-google-genai"
GEMINI_PYTHON="${ASSAY_RUNNER_GEMINI_FIXTURE_PYTHON:-python3}"
if ! command -v "$GEMINI_PYTHON" >/dev/null 2>&1; then
  echo "SKIP: Gemini fixture Python interpreter not found: $GEMINI_PYTHON." >&2
  exit 40
fi
GEMINI_PYTHONPATH="${ASSAY_RUNNER_GEMINI_PYTHONPATH:-$GEMINI_FIXTURE_DIR/.python-deps}"
if [ ! -d "$GEMINI_PYTHONPATH" ]; then
  echo "SKIP: Gemini fixture deps not found at $GEMINI_PYTHONPATH." >&2
  echo "Hint: install them via:" >&2
  echo "  python3 -m pip install --require-hashes --target $GEMINI_FIXTURE_DIR/.python-deps -r $GEMINI_FIXTURE_DIR/requirements.txt" >&2
  exit 40
fi

GEMINI_CASSETTE="$GEMINI_FIXTURE_DIR/cassettes/fixture.yaml"
if [ ! -f "$GEMINI_CASSETTE" ]; then
  echo "SKIP: Gemini fixture cassette not found at $GEMINI_CASSETTE." >&2
  echo "Hint: see $GEMINI_FIXTURE_DIR/MAINTAINER-PROBE.md for the maintainer recording step." >&2
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
  TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/assay-runner-gemini-google-genai-kernel-policy.XXXXXX")"
  cleanup_tmp_root() {
    rm -rf -- "$TMP_ROOT"
  }
fi

if [ -d /dev/shm ]; then
  CONTROL_ROOT="$(mktemp -d /dev/shm/assay-runner-gemini-control.XXXXXX)"
else
  CONTROL_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/assay-runner-gemini-control.XXXXXX")"
fi
cleanup_all() {
  cleanup_tmp_root
  rm -rf -- "$CONTROL_ROOT"
}
trap cleanup_all EXIT

WORK_DIR="${ASSAY_RUNNER_ACCEPTANCE_WORK_DIR:-$TMP_ROOT/work}"
EXTRACT_DIR="$TMP_ROOT/extract"
BUNDLE="$TMP_ROOT/runner-gemini-google-genai-kernel-policy.tar.gz"
SDK_LOG="$CONTROL_ROOT/sdk-events.ndjson"
DECISION_LOG="$CONTROL_ROOT/policy-decisions.ndjson"
RUN_ID="${ASSAY_RUNNER_ACCEPTANCE_RUN_ID:-run_gemini_google_genai_kernel_policy_acceptance}"
AGENT_SCRIPT="$CONTROL_ROOT/gemini-google-genai-sdk-policy-agent.sh"
POLICY_WRAPPER="$CONTROL_ROOT/policy-wrapper.sh"
MCP_FILE_SERVER="$CONTROL_ROOT/mcp_file_server.py"
EXPECTED_SDK_SOURCE="${ASSAY_RUNNER_ACCEPTANCE_EXPECT_SDK_SOURCE:-gemini-google-genai-fixture}"
EXPECTED_SDK_NAME="${ASSAY_RUNNER_ACCEPTANCE_EXPECT_SDK_NAME:-google-genai}"
EXPECTED_SDK_VERSION="${ASSAY_RUNNER_ACCEPTANCE_EXPECT_SDK_VERSION:-2.6.0}"

export ASSAY_BIN
export ASSAY_FIXTURE_ROOT="$ROOT"
export ASSAY_RUNNER_GEMINI_FIXTURE_PYTHON="$GEMINI_PYTHON"
export ASSAY_RUNNER_GEMINI_PYTHONPATH="$GEMINI_PYTHONPATH"
export ASSAY_RUNNER_GEMINI_FIXTURE_SCRIPT="$GEMINI_FIXTURE_DIR/fixture.py"
export ASSAY_RUNNER_GEMINI_EXTRACT_SCRIPT="$GEMINI_FIXTURE_DIR/extract_cassette_tool_call_id.py"
export ASSAY_RUNNER_GEMINI_POLICY_WRAPPER="$POLICY_WRAPPER"
export ASSAY_RUNNER_MCP_FILE_SERVER="$MCP_FILE_SERVER"
export ASSAY_RUNNER_POLICY_DECISION_LOG="$DECISION_LOG"
export ASSAY_RUNNER_RUN_ID="$RUN_ID"

cp "$ROOT/tests/fixtures/runner-spike/gemini-google-genai-sdk-policy-agent.sh" "$AGENT_SCRIPT"
cp "$ROOT/tests/fixtures/runner-spike/gemini-google-genai/policy-wrapper.sh" "$POLICY_WRAPPER"
cp "$ROOT/tests/fixtures/runner-spike/mcp_file_server.py" "$MCP_FILE_SERVER"
chmod +x "$AGENT_SCRIPT" "$POLICY_WRAPPER" "$MCP_FILE_SERVER"

mkdir -p "$WORK_DIR"
# Fixture writes its own input files if missing; the wrapper only seeds the
# policy companion file to mirror the S5 acceptance two-path shape. Gemini
# fixture creates gemini-input.txt itself; policy wrapper creates
# policy-input.txt itself. We pre-seed both to make the kernel layer's
# openat() pattern deterministic.
printf '%s\n' "gemini google-genai fixture input" > "$WORK_DIR/gemini-input.txt"
printf '%s\n' "assay runner policy fixture input" > "$WORK_DIR/policy-input.txt"

"$ASSAY_BIN" runner-spike run \
  --agent-shim gemini-google-genai \
  --kernel-capture \
  --ebpf "$ASSAY_EBPF_PATH" \
  --sdk-event-log "$SDK_LOG" \
  --policy-decision-log "$DECISION_LOG" \
  --run-id "$RUN_ID" \
  --output "$BUNDLE" \
  -- "$AGENT_SCRIPT" "$WORK_DIR"

mkdir -p "$EXTRACT_DIR"
tar -xzf "$BUNDLE" -C "$EXTRACT_DIR"

python3 - "$EXTRACT_DIR" "$WORK_DIR" "$RUN_ID" "$EXPECTED_SDK_SOURCE" "$EXPECTED_SDK_NAME" "$EXPECTED_SDK_VERSION" "$DECISION_LOG" "$GEMINI_FIXTURE_DIR" <<'PY'
import hashlib
import json
import subprocess
import sys
from collections import Counter
from pathlib import Path

extract_dir = Path(sys.argv[1])
work_dir = Path(sys.argv[2]).resolve()
run_id = sys.argv[3]
expected_sdk_source = sys.argv[4]
expected_sdk_name = sys.argv[5]
expected_sdk_version = sys.argv[6]
decision_log = Path(sys.argv[7])
gemini_fixture_dir = Path(sys.argv[8])


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


# Resolve the cassette tool_call_id via the same helper the fixture
# wrapper uses, so the acceptance assertion matches what the runtime saw.
result = subprocess.run(
    [sys.executable, str(gemini_fixture_dir / "extract_cassette_tool_call_id.py")],
    capture_output=True,
    text=True,
    check=False,
)
if result.returncode != 0:
    fail(
        f"could not extract cassette tool_call_id: rc={result.returncode} "
        f"stderr={result.stderr.strip()!r}"
    )
cassette_tool_call_id = result.stdout.strip()
if not cassette_tool_call_id:
    fail("cassette tool_call_id extractor returned empty stdout")


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
expect(
    policy_event["tool_call_id"] == cassette_tool_call_id,
    f"policy event tool_call_id mismatch: expected {cassette_tool_call_id!r}, got {policy_event['tool_call_id']!r}",
)
expect(policy_event["tool"] == "read_file", "policy event tool mismatch")
expect(policy_event["decision"] == "allow", "policy event decision mismatch")

source_events = [json.loads(line) for line in decision_log.read_text(encoding="utf-8").splitlines() if line.strip()]
expect(len(source_events) == 1, f"expected one source decision event, got {len(source_events)}")
expect(source_events[0]["type"] == "assay.tool.decision", "source decision event type mismatch")
expect(
    source_events[0]["data"]["tool_call_id"] == cassette_tool_call_id,
    "source decision tool_call_id mismatch",
)

for expected_seq, event in enumerate(sdk_events):
    expect(event.get("schema") == "assay.runner.sdk_event.v0", f"sdk event {expected_seq} schema mismatch")
    expect(event.get("run_id") == run_id, f"sdk event {expected_seq} run_id mismatch")
    expect(event.get("seq") == expected_seq, f"sdk event {expected_seq} seq mismatch")
    expect(event.get("source") == expected_sdk_source, f"sdk event {expected_seq} source mismatch")
    expect(event.get("sdk_name") == expected_sdk_name, f"sdk event {expected_seq} sdk_name mismatch")
    expect(event.get("sdk_version") == expected_sdk_version, f"sdk event {expected_seq} sdk_version mismatch")

sdk_tool_calls = {event.get("tool_call_id") for event in sdk_events if event.get("tool_call_id")}
sdk_tools = {event.get("tool") for event in sdk_events if event.get("tool")}
expect(
    sdk_tool_calls == {cassette_tool_call_id},
    f"sdk tool_call_id mismatch: expected {{{cassette_tool_call_id!r}}}, got {sdk_tool_calls!r}",
)
expect(sdk_tools == {"read_file"}, f"sdk tool mismatch: {sdk_tools!r}")
expect(
    [event.get("event_type") for event in sdk_events]
    == ["tool_call_started", "tool_call_completed", "run_finished"],
    "sdk event sequence mismatch",
)

filesystem = set(surface.get("filesystem_paths", []))
expect(
    str(work_dir / "gemini-input.txt") in filesystem,
    "Gemini fixture input read was not recorded",
)
expect(
    str(work_dir / "policy-input.txt") in filesystem,
    "policy fixture input read was not recorded",
)
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
expect(binding["tool_call_id"] == cassette_tool_call_id, "binding tool_call_id mismatch")
expect(binding["policy_decision"] == "allow", "binding policy_decision mismatch")
expect(binding["kernel_event_count"] > 0, "binding must include kernel events")
expect(binding["window"] == {"start": "run_started", "end": "run_finished"}, "binding window mismatch")

print("runner-spike Gemini google-genai kernel+policy archive verified")
PY

echo "PASS: runner-spike Gemini google-genai kernel+policy acceptance"
