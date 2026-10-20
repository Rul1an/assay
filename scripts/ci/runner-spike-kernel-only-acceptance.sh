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

die() {
  echo "FAIL: $*" >&2
  exit 1
}

# Fixed rehearsal bounds for this finite fixture: external controls on the run, not a
# producer-side event or byte ceiling. The budgets are checked after the run.
LOOPBACK_PORT=47391
RUN_TIMEOUT_SECONDS=300
ADDRESS_SPACE_LIMIT_BYTES=4294967296
FILE_SIZE_LIMIT_BYTES=67108864

# Never reuse or overwrite earlier output: a supplied artifact directory must be new or empty.
if [ -n "${ASSAY_RUNNER_ACCEPTANCE_ARTIFACT_DIR:-}" ]; then
  TMP_ROOT="$ASSAY_RUNNER_ACCEPTANCE_ARTIFACT_DIR"
  if [ -e "$TMP_ROOT" ] && [ -n "$(ls -A "$TMP_ROOT")" ]; then
    die "artifact directory is not empty, refusing to overwrite: $TMP_ROOT"
  fi
  mkdir -p "$TMP_ROOT"
  remove_tmp_root=false
else
  TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/assay-runner-kernel-only.XXXXXX")"
  remove_tmp_root=true
fi

LISTENER_DIR="$TMP_ROOT/loopback-listener"
OUTSIDE_DIR="$TMP_ROOT/outside-cgroup"
mkdir "$LISTENER_DIR" "$OUTSIDE_DIR"
LISTENER_PID=""
OUTSIDE_PID=""

stop_controls() {
  touch "$LISTENER_DIR/stop" "$OUTSIDE_DIR/stop" 2>/dev/null || true
  for pid in $OUTSIDE_PID $LISTENER_PID; do
    wait "$pid" 2>/dev/null || true
  done
  OUTSIDE_PID=""
  LISTENER_PID=""
}

cleanup() {
  stop_controls
  if [ "$remove_tmp_root" = true ]; then
    rm -rf -- "$TMP_ROOT"
  fi
}
trap cleanup EXIT

WORK_DIR="${ASSAY_RUNNER_ACCEPTANCE_WORK_DIR:-$TMP_ROOT/work}"
EXTRACT_DIR="$TMP_ROOT/extract"
BUNDLE="$TMP_ROOT/runner-kernel-only.tar.gz"
RUN_ID="${ASSAY_RUNNER_ACCEPTANCE_RUN_ID:-run_kernel_only_acceptance}"

# Build outside the rehearsal bounds; only the capture run is limited.
cargo build -p assay-cli --no-default-features --features runner
ASSAY_RUNNER_BIN="$ROOT/target/debug/assay"

# Loopback endpoint the fixture connects to, owned by this script and outside the session
# cgroup, so the port is known to be listening rather than assumed closed.
python3 -I -S -B - "$LOOPBACK_PORT" "$LISTENER_DIR" <<'PY' &
import json
import socket
import sys
import time
from pathlib import Path

port = int(sys.argv[1])
out_dir = Path(sys.argv[2])
record = {"port": port, "accepted": 0, "status": "listening"}

server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
server.bind(("127.0.0.1", port))
server.listen(4)
(out_dir / "ready").touch()
server.settimeout(0.2)
deadline = time.monotonic() + 360
while time.monotonic() < deadline and not (out_dir / "stop").exists():
    try:
        server.accept()[0].close()
        record["accepted"] += 1
    except socket.timeout:
        pass
# Count connections still queued when the stop request arrived.
server.setblocking(False)
while True:
    try:
        server.accept()[0].close()
        record["accepted"] += 1
    except BlockingIOError:
        break
record["status"] = "stopped"
(out_dir / "listener.json").write_text(json.dumps(record) + "\n", encoding="utf-8")
PY
LISTENER_PID=$!

# Control activity outside the session cgroup: a uniquely named executable and file that must
# never appear in the capture. The counter shows the loop ran while the capture was open.
OUTSIDE_EXEC_MARKER="$OUTSIDE_DIR/outside-exec-marker"
OUTSIDE_OPEN_MARKER="$OUTSIDE_DIR/outside-open-marker.txt"
cp /bin/true "$OUTSIDE_EXEC_MARKER"
printf '%s\n' "assay runner kernel-only outside-cgroup marker" > "$OUTSIDE_OPEN_MARKER"
(
  cat /proc/self/cgroup > "$OUTSIDE_DIR/cgroup.txt"
  i=0
  while [ "$i" -lt 6000 ] && [ ! -e "$OUTSIDE_DIR/stop" ]; do
    "$OUTSIDE_EXEC_MARKER"
    cat "$OUTSIDE_OPEN_MARKER" >/dev/null
    i=$((i + 1))
    # Publish a complete counter: readers must never observe a truncated file.
    echo "$i" > "$OUTSIDE_DIR/count.next"
    mv "$OUTSIDE_DIR/count.next" "$OUTSIDE_DIR/count"
    sleep 0.05
  done
) &
OUTSIDE_PID=$!

for _ in $(seq 1 50); do
  [ -e "$LISTENER_DIR/ready" ] && [ -s "$OUTSIDE_DIR/count" ] && break
  sleep 0.2
done
[ -e "$LISTENER_DIR/ready" ] || die "loopback listener on 127.0.0.1:$LOOPBACK_PORT did not start"
[ -s "$OUTSIDE_DIR/count" ] || die "outside-cgroup control loop did not start"
OUTSIDE_COUNT_BEFORE="$(cat "$OUTSIDE_DIR/count")"

timeout --kill-after=5 "$RUN_TIMEOUT_SECONDS" \
  prlimit --as="$ADDRESS_SPACE_LIMIT_BYTES" --fsize="$FILE_SIZE_LIMIT_BYTES" -- \
  "$ASSAY_RUNNER_BIN" \
  runner-spike run \
  --agent-shim none \
  --kernel-capture \
  --ebpf "$ASSAY_EBPF_PATH" \
  --run-id "$RUN_ID" \
  --output "$BUNDLE" \
  -- "$ROOT/tests/fixtures/runner-spike/kernel-only-agent.sh" "$WORK_DIR" "$LOOPBACK_PORT"

OUTSIDE_COUNT_AFTER="$(cat "$OUTSIDE_DIR/count")"
stop_controls

mkdir "$EXTRACT_DIR"
tar -xzf "$BUNDLE" -C "$EXTRACT_DIR"

python3 - "$EXTRACT_DIR" "$WORK_DIR" "$RUN_ID" "$BUNDLE" "$LISTENER_DIR" "$OUTSIDE_DIR" \
  "$OUTSIDE_COUNT_BEFORE" "$OUTSIDE_COUNT_AFTER" <<'PY'
import hashlib
import json
import sys
from collections import Counter
from pathlib import Path

extract_dir = Path(sys.argv[1])
work_dir = Path(sys.argv[2]).resolve()
run_id = sys.argv[3]
bundle = Path(sys.argv[4])
listener_dir = Path(sys.argv[5])
outside_dir = Path(sys.argv[6])
outside_count_before = int(sys.argv[7])
outside_count_after = int(sys.argv[8])

LOOPBACK_ENDPOINT = "127.0.0.1:47391"
MAX_KERNEL_EVENTS = 10_000
MAX_KERNEL_LAYER_BYTES = 4 * 1024 * 1024
MAX_ARCHIVE_BYTES = 4 * 1024 * 1024


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

# Rehearsal budgets, checked after the run.
kernel_layer_bytes = (extract_dir / "layers/kernel.ndjson").stat().st_size
expect(len(kernel_events) <= MAX_KERNEL_EVENTS, f"kernel event budget exceeded: {len(kernel_events)}")
expect(kernel_layer_bytes <= MAX_KERNEL_LAYER_BYTES, f"kernel layer byte budget exceeded: {kernel_layer_bytes}")
expect(bundle.stat().st_size <= MAX_ARCHIVE_BYTES, f"archive byte budget exceeded: {bundle.stat().st_size}")

# The controls the capture checks depend on actually ran.
report = json.loads((work_dir / "fixture-report.json").read_text(encoding="utf-8"))
expect(report.get("loopback_connect") == "connected", f"fixture did not reach the listener: {report!r}")
expect(
    report.get("failed_exec_errno") in {"ENOEXEC", "EACCES"},
    f"failed-exec control did not fail inside execve: {report!r}",
)
listener = json.loads((listener_dir / "listener.json").read_text(encoding="utf-8"))
expect(listener["accepted"] == 1, f"listener must accept exactly the fixture connection: {listener!r}")
outside_cgroup = (outside_dir / "cgroup.txt").read_text(encoding="utf-8")
expect("assay-session-" not in outside_cgroup, f"control loop is inside the session cgroup: {outside_cgroup!r}")
expect(outside_count_after > outside_count_before, "outside-cgroup control did not run during capture")

input_path = str(work_dir / "input.txt")
output_path = str(work_dir / "output.txt")
not_executable = str(work_dir / "not-an-executable")
outside_exec = str(outside_dir / "outside-exec-marker")
outside_open = str(outside_dir / "outside-open-marker.txt")

filesystem = set(surface.get("filesystem_paths", []))
network = set(surface.get("network_endpoints", []))
process = set(surface.get("process_execs", []))
values = {event.get("value") for event in kernel_events}


def has_event(kind: str, value: str, **fields) -> bool:
    return any(
        event.get("kind") == kind
        and event.get("value") == value
        and all(event.get(key) == expected for key, expected in fields.items())
        for event in kernel_events
    )


# Every check is evaluated so a red run names exactly which observations are missing.
checks = {
    "file: input.txt written": has_event("openat", input_path, status="success", access_mode="write"),
    "file: input.txt read": has_event("openat", input_path, status="success", access_mode="read"),
    "file: output.txt written": has_event("openat", output_path, status="success", access_mode="write"),
    "file: both paths in surface": {input_path, output_path} <= filesystem,
    "network: loopback connect attempt": has_event("connect", LOOPBACK_ENDPOINT),
    "network: loopback endpoint in surface": LOOPBACK_ENDPOINT in network,
    "network: coverage connect_only": health.get("network_protocol_coverage") == "connect_only",
    "negative: failed exec absent": not has_event("exec", not_executable) and not_executable not in process,
    "negative: outside exec marker absent": outside_exec not in values and outside_exec not in process,
    "negative: outside open marker absent": outside_open not in values and outside_open not in filesystem,
    "process: /usr/bin/env exec event": has_event("exec", "/usr/bin/env"),
    "process: /usr/bin/env in surface": "/usr/bin/env" in process,
}
for name, passed in checks.items():
    print(f"{'PASS' if passed else 'FAIL'}: {name}")
missing = [name for name, passed in checks.items() if not passed]
if missing:
    print_kernel_event_summary(kernel_events)
    fail(f"missing capture observations: {'; '.join(missing)}")

print("runner-spike kernel-only archive verified")
PY

echo "PASS: runner-spike kernel-only acceptance"
