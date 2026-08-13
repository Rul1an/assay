#!/usr/bin/env bash
# Live positive sendto/sendmsg syscall/effect matrix.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
test -f target/assay-ebpf.o
sudo -E env HARNESS_BIN="${RUNNER_TEMP:?}/s1b-harness" WORKDIR="${RUNNER_TEMP:?}/s1b-positive" \
  bash scripts/ci/run-send-syscall-matrix.sh positive
