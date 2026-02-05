#!/usr/bin/env bash
set -euo pipefail

# This script runs cargo clippy on the workspace.
# Pre-push: runs with timeout so it doesn't hang indefinitely (CI runs clippy too).
# Excludes assay-python-sdk (pyo3 needs Python at build time; can hang or fail locally)
# and assay-ebpf (Linux-only, heavy). CI runs full workspace on Ubuntu.

export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"

# Timeout in seconds (default 300 = 5 min). Set CLIPPY_TIMEOUT=0 to disable.
CLIPPY_TIMEOUT="${CLIPPY_TIMEOUT:-300}"

# Exclude crates that often hang or fail on dev machines (CI runs full workspace)
# assay-it = assay-python-sdk (pyo3 needs Python at build time); assay-ebpf = Linux-only
CLIPPY_EXCLUDE="--exclude assay-it --exclude assay-ebpf"
CLIPPY_CMD="cargo clippy --workspace --all-targets $CLIPPY_EXCLUDE -- -D warnings"

echo "cargo-clippy: checking workspace (timeout=${CLIPPY_TIMEOUT}s, exclude assay-it, assay-ebpf)..."

run_clippy() {
    $CLIPPY_CMD
}

# timeout(1) expects a command name, not a shell function; use bash -c with the actual command.
run_with_timeout() {
    local t=$1
    if command -v timeout &>/dev/null; then
        timeout "$t" bash -c "$CLIPPY_CMD"
    elif command -v gtimeout &>/dev/null; then
        gtimeout "$t" bash -c "$CLIPPY_CMD"
    else
        run_clippy
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
