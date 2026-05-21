#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [ "$(uname -s)" != "Linux" ]; then
  echo "SKIP: runner-spike kernel-only acceptance requires Linux." >&2
  exit 40
fi

ASSAY_EBPF_PATH="${ASSAY_EBPF_PATH:-$ROOT/target/assay-ebpf.o}"
if [ ! -f "$ASSAY_EBPF_PATH" ]; then
  echo "SKIP: eBPF object not found: $ASSAY_EBPF_PATH" >&2
  echo "Hint: build it first, for example with: cargo xtask build-ebpf" >&2
  exit 40
fi

if [ -n "${ASSAY_RUNNER_ACCEPTANCE_ARTIFACT_DIR:-}" ]; then
  TMP_ROOT="$ASSAY_RUNNER_ACCEPTANCE_ARTIFACT_DIR"
  mkdir -p "$TMP_ROOT"
  cleanup() {
    :
  }
else
  TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/assay-runner-kernel-only.XXXXXX")"
  cleanup() {
    rm -rf -- "$TMP_ROOT"
  }
fi
trap cleanup EXIT

WORK_DIR="${ASSAY_RUNNER_ACCEPTANCE_WORK_DIR:-$TMP_ROOT/work}"
EXTRACT_DIR="$TMP_ROOT/extract"
BUNDLE="$TMP_ROOT/runner-kernel-only.tar.gz"
RUN_ID="${ASSAY_RUNNER_ACCEPTANCE_RUN_ID:-run_kernel_only_acceptance}"

cargo run -p assay-cli --no-default-features -- \
  runner-spike run \
  --agent-shim none \
  --kernel-capture \
  --ebpf "$ASSAY_EBPF_PATH" \
  --run-id "$RUN_ID" \
  --output "$BUNDLE" \
  -- "$ROOT/tests/fixtures/runner-spike/kernel-only-agent.sh" "$WORK_DIR"

mkdir -p "$EXTRACT_DIR"
tar -xzf "$BUNDLE" -C "$EXTRACT_DIR"

python3 - "$EXTRACT_DIR" "$WORK_DIR" "$RUN_ID" <<'PY'
import hashlib
import json
import sys
from collections import Counter
from pathlib import Path

extract_dir = Path(sys.argv[1])
work_dir = Path(sys.argv[2]).resolve()
run_id = sys.argv[3]


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
expect(
    health["kernel_layer"] == "complete",
    f"kernel_layer must be complete, got {health['kernel_layer']!r}",
)
expect(health["ringbuf_drops"] == 0, f"ringbuf_drops must be 0, got {health['ringbuf_drops']!r}")
expect(health["policy_layer"] == "absent", f"policy_layer must be absent, got {health['policy_layer']!r}")
expect(health["sdk_layer"] == "absent", f"sdk_layer must be absent, got {health['sdk_layer']!r}")
expect(
    health["cgroup_correlation"] == "clean",
    f"cgroup_correlation must be clean, got {health['cgroup_correlation']!r}",
)

expect(
    (extract_dir / "layers/policy.ndjson").read_text(encoding="utf-8") == "",
    "policy layer must be empty",
)
expect((extract_dir / "layers/sdk.ndjson").read_text(encoding="utf-8") == "", "sdk layer must be empty")

expect(kernel_events, "kernel layer must contain events")
for line_number, event in enumerate(kernel_events, start=1):
    expect(
        event.get("schema") == "assay.runner.kernel_event.v0",
        f"kernel event {line_number} has unexpected schema: {event.get('schema')!r}",
    )

filesystem = set(surface.get("filesystem_paths", []))
expect(str(work_dir / "input.txt") in filesystem, "fixture input read was not recorded")

print("runner-spike kernel-only archive verified")
PY

echo "PASS: runner-spike kernel-only acceptance"
