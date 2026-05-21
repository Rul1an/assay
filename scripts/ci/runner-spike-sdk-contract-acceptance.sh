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

TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/assay-runner-sdk-contract.XXXXXX")"
cleanup() {
  rm -rf -- "$TMP_ROOT"
}
trap cleanup EXIT

WORK_DIR="$TMP_ROOT/work"
EXTRACT_DIR="$TMP_ROOT/extract"
BUNDLE="$TMP_ROOT/runner-sdk-contract.tar.gz"
SDK_LOG="$TMP_ROOT/sdk-events.ndjson"
RUN_ID="run_sdk_contract_acceptance"

"$ASSAY_BIN" runner-spike run \
  --agent-shim openai-agents \
  --sdk-event-log "$SDK_LOG" \
  --run-id "$RUN_ID" \
  --output "$BUNDLE" \
  -- "$ROOT/tests/fixtures/runner-spike/sdk-event-wrapper.sh" "$WORK_DIR"

mkdir -p "$EXTRACT_DIR"
tar -xzf "$BUNDLE" -C "$EXTRACT_DIR"

python3 - "$EXTRACT_DIR" "$WORK_DIR" "$RUN_ID" "$SDK_LOG" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

extract_dir = Path(sys.argv[1])
work_dir = Path(sys.argv[2]).resolve()
run_id = sys.argv[3]
sdk_log = Path(sys.argv[4])


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

expect(health["kernel_layer"] == "absent", f"kernel_layer must be absent, got {health['kernel_layer']!r}")
expect(health["ringbuf_drops"] == 0, f"ringbuf_drops must be 0, got {health['ringbuf_drops']!r}")
expect(health["policy_layer"] == "absent", f"policy_layer must be absent, got {health['policy_layer']!r}")
expect(health["sdk_layer"] == "self_reported", f"sdk_layer must be self_reported, got {health['sdk_layer']!r}")
expect(
    health["cgroup_correlation"] == "partial",
    f"cgroup_correlation must remain partial without kernel capture, got {health['cgroup_correlation']!r}",
)

expect((extract_dir / "layers/kernel.ndjson").read_text(encoding="utf-8") == "", "kernel layer must be empty")
expect((extract_dir / "layers/policy.ndjson").read_text(encoding="utf-8") == "", "policy layer must be empty")

sdk_events = (extract_dir / "layers/sdk.ndjson").read_text(encoding="utf-8").splitlines()
source_events = sdk_log.read_text(encoding="utf-8").splitlines()
expect(len(sdk_events) == 3, f"expected three sdk events, got {len(sdk_events)}")
expect(len(source_events) == 3, f"expected three source sdk events, got {len(source_events)}")

for seq, line in enumerate(sdk_events):
    event = json.loads(line)
    source_event = json.loads(source_events[seq])
    expect(event["schema"] == "assay.runner.sdk_event.v0", f"sdk event {seq} schema mismatch")
    expect(event["run_id"] == run_id, f"sdk event {seq} run_id mismatch")
    expect(event["seq"] == seq, f"sdk event {seq} seq mismatch")
    for field in ("event_type", "source", "sdk_name", "sdk_version"):
        expect(event.get(field) == source_event.get(field), f"sdk event {seq} {field} mismatch")
    if event["event_type"] in {"tool_call_started", "tool_call_completed"}:
        expect(event["tool_call_id"] == "tc_runner_sdk_001", f"sdk event {seq} tool_call_id mismatch")
        expect(event["tool"] == "read_file", f"sdk event {seq} tool mismatch")

expect((work_dir / "sdk-wrapper-ran.txt").read_text(encoding="utf-8").strip() == "sdk wrapper fixture ran", "sdk wrapper fixture did not run")
expect(surface.get("filesystem_paths", []) == [], "sdk-only fixture must not add filesystem capabilities")
expect(correlation.get("bindings", []) == [], "sdk-only fixture must not add correlation bindings")

print("runner-spike SDK contract archive verified")
PY

echo "PASS: runner-spike SDK contract acceptance"
