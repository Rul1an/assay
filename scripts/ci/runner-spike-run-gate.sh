#!/usr/bin/env bash
set -euo pipefail

# Run one delegated gate and record its outcome.
#
# Extracted from a bash function inside a single workflow step so that each gate
# can be its own step. That split is the point: GitHub records step names and
# conclusions, and the gate does not write that record. It is the only
# observation of which gates ran that does not come from the delegated host,
# where gates execute as root and are handed their own proof directory (#2041).
#
# Usage: runner-spike-run-gate.sh <key> <label> <script>
#
# Contract, unchanged from the function this replaces:
#   - any non-zero status from the gate is a failure, exit 40 included; a skip in
#     this lane means the runner contract drifted (see the delegated runbook)
#   - `status.txt` records passed or failed
#   - the gate log is teed into the gate directory for the proof pack

key="$1"
label="$2"
script="$3"

root="$(git rev-parse --show-toplevel)"
cd "$root"

assay_bin="$PWD/target/debug/assay"
ebpf_path="$PWD/target/assay-ebpf.o"
gate_dir="$ASSAY_RUNNER_DELEGATED_PROOF_ROOT/gates/$key"

echo "=== runner-spike delegated gate: ${label} ==="
rm -rf "$gate_dir"
mkdir -p "$gate_dir"

set +e
sudo -E env \
  "PATH=$PATH" \
  "ASSAY_BIN=$assay_bin" \
  "ASSAY_EBPF_PATH=$ebpf_path" \
  "ASSAY_RUNNER_DELEGATED_PROOF_GATE_DIR=$gate_dir" \
  bash "$script" 2>&1 | tee "$gate_dir/gate.log"
status="${PIPESTATUS[0]}"
sudo chown -R "$(id -u):$(id -g)" "$gate_dir" 2>/dev/null || true
set -e

if [ "$status" -eq 0 ]; then
  echo "passed" > "$gate_dir/status.txt"
  echo "- ${label}: passed" >> "${GITHUB_STEP_SUMMARY:-/dev/null}"
  exit 0
fi

echo "failed" > "$gate_dir/status.txt"
exit "$status"
