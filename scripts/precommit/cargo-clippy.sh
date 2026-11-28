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
# Leave the target and intermediate build directories to Cargo: explicit environment choices,
# Cargo configuration, or the worktree's own target/. This hook sets none and removes none.

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

# Share the pushed-range admission rule with both consumer-battery selectors.
ROOT="$(git rev-parse --show-toplevel)"
# shellcheck source=scripts/ci/lib/pushed-range-changes.sh
source "$ROOT/scripts/ci/lib/pushed-range-changes.sh"
changed_files_file=""
pushed_paths=""
cleanup() {
    local file
    for file in "$changed_files_file" "$pushed_paths"; do
        [[ -z "$file" ]] || rm -f -- "$file"
    done
}
trap cleanup EXIT
changed_files_file="$(mktemp)"
pushed_paths="$(mktemp)"
classifier="$ROOT/scripts/ci/classify-lightweight-changes.sh"
range_known=false

# Presence matters: partial or explicitly empty push refs must not fall back to an upstream
# shortcut. A manual base override is available only outside an actual pre-commit push.
if [[ ${PRE_COMMIT_FROM_REF+x} || ${PRE_COMMIT_TO_REF+x} ]]; then
    if [[ ${ASSAY_PREPUSH_BASE_REF+x} ]]; then
        echo "cargo-clippy: ignoring ASSAY_PREPUSH_BASE_REF; checking the actual pushed range."
    fi
    if reason="$(pushed_range_ready)" && reason="$(pushed_range_paths "$pushed_paths")"; then
        range_known=true
    else
        echo "cargo-clippy: running in full: $reason"
    fi
elif base_ref="$(resolve_base_ref)"; then
    # The same clean-HEAD condition applies to a manual range: cargo reads the working tree.
    if reason="$(PRE_COMMIT_FROM_REF="$base_ref" PRE_COMMIT_TO_REF=HEAD pushed_range_ready)"; then
        # Preserve the manual path's nonzero stop when its explicit base cannot be listed.
        git diff --no-ext-diff --no-renames --name-only -z "${base_ref}...HEAD" -- > "$pushed_paths"
        range_known=true
    else
        echo "cargo-clippy: running in full: $reason"
    fi
fi

if [[ "$range_known" == true ]]; then
    # The classifier owns path policy and accepts one path per line. Do not split a Git path
    # into apparently harmless entries; uncertainty runs the lint instead.
    while IFS= read -r -d '' path; do
        if [[ "$path" == *$'\n'* || "$path" == *$'\r'* ]]; then
            echo "cargo-clippy: running in full: a changed path contains a line break." >&2
            range_known=false
            break
        fi
        printf '%s\n' "$path"
    done < "$pushed_paths" > "$changed_files_file"
    if [[ "$range_known" == true ]] && verdict="$(bash "$classifier" "$changed_files_file")" && [[ "$verdict" == "true" ]]; then
        echo "cargo-clippy: lightweight-only diff detected; skipping workspace clippy."
        sed 's/^/  - /' "$changed_files_file"
        exit 0
    fi
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

echo "cargo-clippy: checking workspace (timeout=${CLIPPY_TIMEOUT}s, exclude assay-it, assay-ebpf, target=${CARGO_TARGET_DIR:-${CARGO_BUILD_TARGET_DIR:-Cargo configuration/default}})..."

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
