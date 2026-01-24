#!/bin/bash

set -e
# Cleanup any stale monitors
pkill -x assay || true
rm -f /tmp/assay-test/secret.txt || true

echo ">> [Diag] Kernel: $(uname -r)"
echo ">> [Diag] Active LSMs: $(cat /sys/kernel/security/lsm 2>/dev/null || echo "N/A")"
echo ">> [Diag] Tracefs: $(mount | grep tracefs || echo "Missing")"
echo ">> [Diag] BPFFS: $(mount | grep bpf || echo "Missing")"

if ! grep -q "bpf" /sys/kernel/security/lsm 2>/dev/null; then
  echo "⚠️  SKIP: bpf not found in Active LSMs. Kernel cmdline needs lsm=...,bpf."
  exit 0
fi

if ! mount | grep -q tracefs; then
   echo "⚠️  Mounting tracefs..."
   mount -t tracefs tracefs /sys/kernel/tracing
fi

# Create Secret
echo "TOP_SECRET_DATA" > /tmp/assay-test/secret.txt
chmod 600 /tmp/assay-test/secret.txt

# Start Monitor
RUST_LOG=info ./assay monitor --ebpf ./assay-ebpf.o --policy ./deny_modern.yaml --monitor-all --print > monitor.log 2>&1 &
MONITOR_PID=$!
sleep 5 # Wait for attachment

echo ">> [Test] Attempting Access (cat /tmp/assay-test/secret.txt)..."
set +e
cat /tmp/assay-test/secret.txt
EXIT_CODE=$?
set -e

# Kill monitor
kill $MONITOR_PID
wait $MONITOR_PID 2>/dev/null

tail -n 20 monitor.log

if [ $EXIT_CODE -eq 0 ]; then
  echo "❌ FAIL: cat command succeeded but should have been blocked."
  exit 1
else
  echo "✅ PASS: cat command failed as expected (Exit: $EXIT_CODE)."
  exit 0
fi
