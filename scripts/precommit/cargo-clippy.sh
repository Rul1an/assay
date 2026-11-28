#!/usr/bin/env bash
set -euo pipefail

# This script runs cargo clippy on the workspace.
# Pre-push: runs under a deadline so it doesn't hang indefinitely (CI runs clippy too). The
# deadline is run-with-timeout.py, next to this script (POSIX, with or without timeout(1)): it
# stops clippy's own process group; a process that leaves that group is not covered.
# CLIPPY_TIMEOUT=0 runs clippy directly, without a deadline.
# Excludes assay-python-sdk (pyo3 needs Python at build time; can hang or fail locally)
# and assay-ebpf (Linux-only, heavy). CI runs full workspace on Ubuntu.
# Skip the heavy workspace lint only for a diff CI itself would call lightweight: the decision is
# CI's classifier (scripts/ci/classify-lightweight-changes.sh) over the --no-renames path list, so
# active docs and crate Markdown, which Rust reads through include_str!, still get the lint, and
# both sides of a rename count. test-cargo-clippy-lightweight-scope.sh pins that parity.
# For code pushes from clean worktrees, share a common target dir so clippy does not rebuild
# the whole workspace from scratch on every branch.

export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"

# Deadline in whole seconds for the whole clippy run (default 300 = 5 min). 0 = no deadline.
CLIPPY_TIMEOUT="${CLIPPY_TIMEOUT:-300}"
DEADLINE_WRAPPER="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/run-with-timeout.py"
DEADLINE_HELPER="${DEADLINE_WRAPPER%.py}.sh"

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

# Only a literal 0 turns the deadline off. Nine digits at most, so shell arithmetic cannot
# overflow a value into 0; no leading zeros, so no octal reading either.
if ! [[ "$CLIPPY_TIMEOUT" =~ ^(0|[1-9][0-9]{0,8})$ ]]; then
    echo "cargo-clippy: CLIPPY_TIMEOUT must be 0 (no deadline) or a whole number of seconds up to 999999999, not '${CLIPPY_TIMEOUT}'." >&2
    exit 2
fi
# A deadline that cannot be enforced is refused, never silently dropped.
if [ "$CLIPPY_TIMEOUT" -gt 0 ] && { ! command -v python3 >/dev/null 2>&1 || [ ! -f "$DEADLINE_WRAPPER" ] || [ ! -f "$DEADLINE_HELPER" ]; }; then
    echo "cargo-clippy: cannot enforce CLIPPY_TIMEOUT=${CLIPPY_TIMEOUT}s: python3, ${DEADLINE_WRAPPER} or ${DEADLINE_HELPER} is missing." >&2
    echo "cargo-clippy: install python3, or set CLIPPY_TIMEOUT=0 to run clippy without a deadline." >&2
    exit 2
fi

echo "cargo-clippy: checking workspace (timeout=${CLIPPY_TIMEOUT}s, exclude assay-it, assay-ebpf, target=${CARGO_TARGET_DIR})..."

run_clippy() {
    $CLIPPY_CMD
}

if [ "$CLIPPY_TIMEOUT" -eq 0 ]; then
    run_clippy
else
    # run_with_deadline passes a terminal's Ctrl+C on to the wrapper, which has left this group.
    # shellcheck source=scripts/precommit/run-with-timeout.sh
    source "$DEADLINE_HELPER"
    code=0
    run_with_deadline "$CLIPPY_TIMEOUT" bash -c "$CLIPPY_CMD" || code=$?
    if [ "$code" -eq 124 ]; then
        echo "cargo-clippy: timed out after ${CLIPPY_TIMEOUT}s."
        echo "cargo-clippy: this usually means a cold workspace build or an undersized local timeout."
        echo "cargo-clippy: rerun manually or increase CLIPPY_TIMEOUT; GitHub CI remains the final gate."
    elif [ "$code" -eq 125 ]; then
        echo "cargo-clippy: the deadline could not stop every clippy process (see the run-with-timeout lines above); some may still be running."
    fi
    exit "$code"
fi
