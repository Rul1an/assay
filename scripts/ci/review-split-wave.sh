#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: scripts/ci/review-split-wave.sh <crate-name> <allowed-path-regex> [base-ref]

Examples:
  scripts/ci/review-split-wave.sh assay-sim '^crates/assay-sim/src/attacks/consumer_downgrade'
  scripts/ci/review-split-wave.sh assay-registry '^crates/assay-registry/src/(trust|trust_next)'
USAGE
}

if [[ $# -lt 2 || $# -gt 3 ]]; then
  usage
  exit 2
fi

crate_name="$1"
allowed_path_regex="$2"
base_ref="${3:-origin/main}"

if ! git rev-parse --verify "${base_ref}" >/dev/null 2>&1; then
  echo "FAIL: cannot resolve base ref: ${base_ref}" >&2
  exit 1
fi

changed_files="$(
  {
    git diff --name-only "${base_ref}"...HEAD
    git diff --cached --name-only
  } | sort -u
)"

if [[ -z "${changed_files}" ]]; then
  echo "FAIL: no tracked changes found against ${base_ref}" >&2
  exit 1
fi

allowed_regex="^$|^docs/contributing/REFACTOR-WAVE-STATUS\.md$|^scripts/ci/review-split-wave\.sh$|${allowed_path_regex}"
unexpected="$(printf '%s\n' "${changed_files}" | grep -Ev "${allowed_regex}" || true)"

if [[ -n "${unexpected}" ]]; then
  echo "FAIL: split wave changed files outside allowed scope:" >&2
  printf '%s\n' "${unexpected}" >&2
  exit 1
fi

git diff --check
cargo fmt --check
cargo check -p "${crate_name}"
cargo test -p "${crate_name}"
cargo clippy -p "${crate_name}" --all-targets -- -D warnings

echo "PASS: split wave review gate for ${crate_name}"
