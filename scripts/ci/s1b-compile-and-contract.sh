#!/usr/bin/env bash
# Compile the S1b harness, run its selftests, then the coverage-gate contract.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
out="${RUNNER_TEMP:-/tmp}/s1b-harness"
cc -Wall -Werror -o "$out" scripts/ci/s1b-send-syscall-matrix.c
"$out" --timeout-selftest
fifo="${RUNNER_TEMP:-/tmp}/s1b-fifo-selftest"
rm -f "$fifo"
mkfifo "$fifo"
"$out" --fifo-selftest "$fifo"
rm -f "$fifo"
bash scripts/ci/test-s1b-coverage-gate.sh
