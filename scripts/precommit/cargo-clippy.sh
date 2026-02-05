#!/usr/bin/env bash
set -euo pipefail

# This script runs cargo clippy on the workspace.
# Pre-push: runs with timeout so it doesn't hang indefinitely (CI runs clippy too).

export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"

# pyo3 (assay-python-sdk) needs a valid Python at build time; avoid stale miniconda path
if [ -z "${PYO3_PYTHON:-}" ]; then
  for p in python3.12 python3.11 python3 python; do
    if pypath=$(command -v "$p" 2>/dev/null); then
      export PYO3_PYTHON=$pypath
      break
    fi
  done
fi

# Timeout in seconds (default 300 = 5 min). Set CLIPPY_TIMEOUT=0 to disable.
CLIPPY_TIMEOUT="${CLIPPY_TIMEOUT:-300}"

echo "cargo-clippy: checking workspace (timeout=${CLIPPY_TIMEOUT}s)..."

run_clippy() {
    cargo clippy --workspace --all-targets -- -D warnings
}

# timeout(1) expects a command name, not a shell function; use bash -c with the actual command.
run_with_timeout() {
    local t=$1
    local cmd='cargo clippy --workspace --all-targets -- -D warnings'
    if command -v timeout &>/dev/null; then
        timeout "$t" bash -c "$cmd"
    else
        command -v gtimeout &>/dev/null && gtimeout "$t" bash -c "$cmd" || run_clippy
    fi
}

if [ "$CLIPPY_TIMEOUT" -eq 0 ] 2>/dev/null; then
    run_clippy
else
    code=0
    run_with_timeout "$CLIPPY_TIMEOUT" || code=$?
    if [ "$code" -eq 124 ]; then
        echo "cargo-clippy: timed out after ${CLIPPY_TIMEOUT}s. Run manually or: git push --no-verify (CI will run clippy)."
    fi
    exit "$code"
fi
