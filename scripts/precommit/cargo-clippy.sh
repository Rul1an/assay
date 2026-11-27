#!/usr/bin/env bash
set -euo pipefail

# This script runs cargo clippy on the workspace.
# Pre-push: runs with timeout so it doesn't hang indefinitely (CI runs clippy too).
# Excludes assay-python-sdk (pyo3 needs Python at build time; can hang or fail locally)
# and assay-ebpf (Linux-only, heavy). CI runs full workspace on Ubuntu.
# Skip the heavy workspace lint only for a diff CI itself would call lightweight: the decision is
# CI's classifier (scripts/ci/classify-lightweight-changes.sh) over the --no-renames path list, so
# active docs and crate Markdown, which Rust reads through include_str!, still get the lint, and
# both sides of a rename count. test-cargo-clippy-lightweight-scope.sh pins that parity.
# For code pushes from clean worktrees, share a common target dir so clippy does not rebuild
# the whole workspace from scratch on every branch.

export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"

# Timeout in seconds (default 300 = 5 min). Set CLIPPY_TIMEOUT=0 to disable.
CLIPPY_TIMEOUT="${CLIPPY_TIMEOUT:-300}"

# Exclude crates that often hang or fail on dev machines (CI runs full workspace)
# assay-it = assay-python-sdk (pyo3 needs Python at build time); assay-ebpf = Linux-only
CLIPPY_EXCLUDE="--exclude assay-it --exclude assay-ebpf"
CLIPPY_CMD="cargo clippy --workspace --all-targets $CLIPPY_EXCLUDE -- -D warnings"

resolve_base_ref() {
    if [[ -n "${ASSAY_PREPUSH_BASE_REF:-}" ]]; then
        printf '%s\n' "${ASSAY_PREPUSH_BASE_REF}"
        return 0
    fi

    if git rev-parse --verify --quiet '@{upstream}' >/dev/null; then
        printf '%s\n' '@{upstream}'
        return 0
    fi

    if git rev-parse --verify --quiet origin/main >/dev/null; then
        git merge-base HEAD origin/main
        return 0
    fi

    return 1
}

changed_files_file="$(mktemp)"
trap 'rm -f "$changed_files_file"' EXIT

classifier="$(git rev-parse --show-toplevel)/scripts/ci/classify-lightweight-changes.sh"

if base_ref="$(resolve_base_ref)"; then
    # Both sides of a rename, as CI's detect step lists them. With git's default rename detection
    # a crate file moved into docs/archive/ would show only its lightweight destination.
    #
    # A diff that cannot be listed stops the hook here (set -e), as it always has.
    git diff --no-renames --name-only "${base_ref}...HEAD" > "$changed_files_file"
    # Only a classifier that exits 0 and prints exactly "true" skips the lint. Its exit status and
    # its answer are judged separately: a classifier that prints "true" and then fails, a missing
    # one and an unknown answer all run clippy.
    if verdict="$(bash "$classifier" "$changed_files_file")" && [[ "$verdict" == "true" ]]; then
        echo "cargo-clippy: lightweight-only diff detected; skipping workspace clippy."
        sed 's/^/  - /' "$changed_files_file"
        exit 0
    fi
fi

if [[ -z "${CARGO_TARGET_DIR:-}" ]]; then
    common_dir="$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null || git rev-parse --git-common-dir)"
    export CARGO_TARGET_DIR="${common_dir}/prepush-target"
fi

echo "cargo-clippy: checking workspace (timeout=${CLIPPY_TIMEOUT}s, exclude assay-it, assay-ebpf, target=${CARGO_TARGET_DIR})..."

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
        echo "cargo-clippy: timed out after ${CLIPPY_TIMEOUT}s."
        echo "cargo-clippy: this usually means a cold workspace build or an undersized local timeout."
        echo "cargo-clippy: rerun manually or increase CLIPPY_TIMEOUT; GitHub CI remains the final gate."
    fi
    exit "$code"
fi
