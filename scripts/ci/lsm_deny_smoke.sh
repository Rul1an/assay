#!/bin/bash
set -euo pipefail

ASSAY_BIN="${ASSAY_BIN:-./target/release/assay}"
ASSAY_EBPF_PATH="${ASSAY_EBPF_PATH:-./target/assay-ebpf.o}"
BPFTOOL="${BPFTOOL:-bpftool}"

echo "=== Assay LSM Smoke Test ==="
echo "Binary: $ASSAY_BIN"
echo "eBPF:   $ASSAY_EBPF_PATH"
echo "Kernel: $(uname -r)"
echo "TMPDIR: ${TMPDIR:-<unset>}"

# 0) Must run as root
if [ "$(id -u)" -ne 0 ]; then
  echo "Error: must run as root (use sudo)."
  exit 2
fi

if [ ! -x "$ASSAY_BIN" ]; then
  echo "Error: Binary not executable or not found: $ASSAY_BIN"
  exit 1
fi
if [ ! -f "$ASSAY_EBPF_PATH" ]; then
  echo "Error: eBPF object not found: $ASSAY_EBPF_PATH"
  exit 1
fi

# 1) Setup Victim (stable file, not auto-deleted by mktemp(1) anyway)
VICTIM="$(mktemp /tmp/assay-victim.XXXXXX)"
echo "SECRET_DATA_PAYLOAD" > "$VICTIM"
chmod 644 "$VICTIM"
echo "Victim File: $VICTIM"
stat "$VICTIM" || true

# 2) Setup Policy
POLICY="$(mktemp /tmp/assay-policy.XXXXXX.yaml)"
cat <<EOF > "$POLICY"
version: "2.0"
runtime_monitor:
  enabled: true
  rules:
    - id: deny_victim
      type: file_open
      match:
        path_globs: ["$VICTIM"]
      action: deny
EOF
echo "Generated Policy:"
cat "$POLICY"

# 3) Start Monitor (capture logs)
LOG="$(mktemp /tmp/assay-monitor.XXXXXX.log)"
echo "Starting Monitor... (log: $LOG)"
# Ensure no stale assay
pkill -x assay 2>/dev/null || true

# Start in background, capture stdout+stderr
"$ASSAY_BIN" monitor --policy "$POLICY" --ebpf "$ASSAY_EBPF_PATH" >"$LOG" 2>&1 &
MONITOR_PID=$!
echo "Monitor PID: $MONITOR_PID"

cleanup() {
  echo "Cleaning up..."
  kill "$MONITOR_PID" 2>/dev/null || true
  wait "$MONITOR_PID" 2>/dev/null || true
  rm -f "$VICTIM" "$POLICY" "$LOG"
}
trap cleanup EXIT

# 3b) Prove attachment succeeded
echo "Waiting for monitor attach confirmation..."
ATTACHED=0
for _ in {1..20}; do
  if grep -q "Assay Monitor running" "$LOG"; then
    ATTACHED=1
    break
  fi
  # if it died, fail early
  if ! kill -0 "$MONITOR_PID" 2>/dev/null; then
    echo "FAILURE: Monitor exited early."
    echo "--- monitor.log (last 200) ---"
    tail -n 200 "$LOG" || true
    exit 1
  fi
  sleep 0.5
done

if [ "$ATTACHED" -ne 1 ]; then
  echo "FAILURE: Monitor did not confirm attach ('Assay Monitor running' not seen)."
  echo "--- monitor.log (last 200) ---"
  tail -n 200 "$LOG" || true
  exit 1
fi
echo "✅ Monitor attached"

# 4) Verify Map State (Diagnostics)
echo "--- Map Status (Pre-Check) ---"
if command -v "$BPFTOOL" >/dev/null 2>&1; then
  $BPFTOOL map show name DENY_INO || echo "Failed to show DENY_INO"
  $BPFTOOL map dump name DENY_INO || echo "Failed to dump DENY_INO"
else
  echo "bpftool not available; skipping map diagnostics"
fi

# 5) Attempt Access (Expect EPERM)
echo "Attempting to cat victim file (expect EPERM)..."
set +e
OUTPUT="$(cat "$VICTIM" 2>&1)"
EXIT_CODE=$?
set -e
echo "Access Result: ExitCode=$EXIT_CODE Output='$OUTPUT'"

# 6) Validate Results
if [ "$EXIT_CODE" -ne 0 ]; then
  # Accept both common Linux messages
  if echo "$OUTPUT" | grep -Eq "Operation not permitted|Permission denied"; then
    echo "✅ SUCCESS: Access denied as expected."
    SUCCESS=true
  else
    echo "⚠️  Access failed, but message was unexpected."
    SUCCESS=false
  fi
else
  echo "❌ FAILURE: Successfully read victim file!"
  SUCCESS=false
fi

echo "--- LSM_HIT Counter ---"
if command -v "$BPFTOOL" >/dev/null 2>&1; then
  $BPFTOOL map dump name LSM_HIT || true
fi

if [ "$SUCCESS" = true ]; then
  echo "Test PASSED"
  exit 0
else
  echo "--- monitor.log (last 200) ---"
  tail -n 200 "$LOG" || true
  echo "--- dmesg bpf/verifier/lsm (last 200) ---"
  dmesg -T | grep -Ei "bpf|verifier|lsm|aya|assay" | tail -n 200 || true
  echo "Test FAILED"
  exit 1
fi
