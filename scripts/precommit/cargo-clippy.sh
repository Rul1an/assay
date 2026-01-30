#!/usr/bin/env bash
set -euo pipefail

# This script runs cargo clippy on the workspace.
# Pre-push: runs with timeout so it doesn't hang indefinitely (CI runs clippy too).

export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"

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
