#!/usr/bin/env bash
set -euo pipefail

base_ref="${BASE_REF:-origin/codex/wave53-hotspot-top2-9-step1}"
if ! git rev-parse --verify "$base_ref" >/dev/null 2>&1; then
  if [[ -z "${BASE_REF:-}" ]] && git rev-parse --verify codex/wave53-hotspot-top2-9-step1 >/dev/null 2>&1; then
    base_ref="codex/wave53-hotspot-top2-9-step1"
  else
    echo "FAIL: cannot resolve Step2 base ref: $base_ref"
    echo "Set BASE_REF to the Step1 branch/ref used for this stacked review."
    exit 1
  fi
fi

base_changed="$(git diff --name-only "$base_ref"...HEAD)"
worktree_changed="$(
  {
    git diff --name-only
    git diff --cached --name-only
    git ls-files --others --exclude-standard
  } | sort -u
)"
changed="$(printf '%s\n%s\n' "$base_changed" "$worktree_changed" | sed '/^$/d' | sort -u)"

allowed_pattern='^(docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9\.md|docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step2\.md|docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step2\.md|docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step1\.md|docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step2\.md|scripts/ci/review-wave53-hotspot-top2-9-step1\.sh|scripts/ci/review-wave53-hotspot-top2-9-step2\.sh|crates/assay-core/src/report/summary\.rs|crates/assay-core/src/report/summary/types\.rs|crates/assay-core/src/report/summary/metrics\.rs|crates/assay-core/src/report/summary/writer\.rs|crates/assay-cli/src/cli/commands/bundle\.rs|crates/assay-cli/src/cli/commands/bundle/implementation\.rs|crates/assay-cli/src/cli/commands/bundle/verify\.rs|crates/assay-cli/src/cli/commands/bundle/paths\.rs|crates/assay-cli/src/cli/commands/bundle/coverage\.rs|crates/assay-registry/src/lockfile\.rs|crates/assay-registry/src/lockfile_next/types\.rs)$'
unexpected="$(printf '%s\n' "$changed" | rg -v "$allowed_pattern" || true)"
if [[ -n "$unexpected" ]]; then
  echo "FAIL: Wave53 Step2 changed files outside the allowlist:"
  printf '%s\n' "$unexpected"
  exit 1
fi

if printf '%s\n' "$changed" | rg '^\.github/workflows/' >/dev/null; then
  echo "FAIL: workflow edits are out of scope for Wave53 Step2"
  exit 1
fi

if printf '%s\n' "$changed" | rg '^crates/assay-ebpf/src/vmlinux\.rs$' >/dev/null; then
  echo "FAIL: generated vmlinux.rs is out of scope for Wave53 Step2"
  exit 1
fi

required=(
  "docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9.md"
  "docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step2.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step2.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step2.md"
  "scripts/ci/review-wave53-hotspot-top2-9-step2.sh"
  "crates/assay-core/src/report/summary.rs"
  "crates/assay-core/src/report/summary/types.rs"
  "crates/assay-core/src/report/summary/metrics.rs"
  "crates/assay-core/src/report/summary/writer.rs"
  "crates/assay-cli/src/cli/commands/bundle.rs"
  "crates/assay-cli/src/cli/commands/bundle/implementation.rs"
  "crates/assay-cli/src/cli/commands/bundle/verify.rs"
  "crates/assay-cli/src/cli/commands/bundle/paths.rs"
  "crates/assay-cli/src/cli/commands/bundle/coverage.rs"
  "crates/assay-registry/src/lockfile.rs"
  "crates/assay-registry/src/lockfile_next/types.rs"
)

for path in "${required[@]}"; do
  test -f "$path" || {
    echo "FAIL: missing required file: $path"
    exit 1
  }
done

require_marker() {
  local pattern="$1"
  local path="$2"
  local message="$3"
  if ! rg -q "$pattern" "$path"; then
    echo "FAIL: $message"
    exit 1
  fi
}

forbid_marker() {
  local pattern="$1"
  local path="$2"
  local message="$3"
  if rg -q "$pattern" "$path"; then
    echo "FAIL: $message"
    exit 1
  fi
}

require_marker '^mod metrics;$' "crates/assay-core/src/report/summary.rs" "summary facade must declare metrics module"
require_marker '^mod types;$' "crates/assay-core/src/report/summary.rs" "summary facade must declare types module"
require_marker '^mod writer;$' "crates/assay-core/src/report/summary.rs" "summary facade must declare writer module"
require_marker '^pub use metrics::judge_metrics_from_results;$' "crates/assay-core/src/report/summary.rs" "summary facade must re-export judge metrics"
require_marker '^pub use types::\*;$' "crates/assay-core/src/report/summary.rs" "summary facade must re-export public summary types"
require_marker '^pub use writer::write_summary;$' "crates/assay-core/src/report/summary.rs" "summary facade must re-export summary writer"
forbid_marker '^pub fn (judge_metrics_from_results|write_summary)\b' "crates/assay-core/src/report/summary.rs" "summary facade must not own moved function bodies"

require_marker '^\#\[path = "bundle/coverage\.rs"\]$' "crates/assay-cli/src/cli/commands/bundle.rs" "bundle facade must point to coverage module"
require_marker '^\#\[path = "bundle/implementation\.rs"\]$' "crates/assay-cli/src/cli/commands/bundle.rs" "bundle facade must point to implementation module"
require_marker '^\#\[path = "bundle/paths\.rs"\]$' "crates/assay-cli/src/cli/commands/bundle.rs" "bundle facade must point to paths module"
require_marker '^\#\[path = "bundle/verify\.rs"\]$' "crates/assay-cli/src/cli/commands/bundle.rs" "bundle facade must point to verify module"
require_marker 'implementation::run\(args, legacy_mode\)\.await' "crates/assay-cli/src/cli/commands/bundle.rs" "bundle facade must delegate to implementation::run"

require_marker '^pub use lockfile_next::types::' "crates/assay-registry/src/lockfile.rs" "lockfile facade must re-export moved public types"
forbid_marker '^pub (struct|enum) (Lockfile|LockedPack|LockSource|LockSignature|VerifyLockResult|LockMismatch)\b' "crates/assay-registry/src/lockfile.rs" "lockfile facade must not own moved public type definitions"

check_loc_max() {
  local path="$1"
  local max="$2"
  local loc
  loc="$(wc -l < "$path" | tr -d ' ')"
  if (( loc > max )); then
    echo "FAIL: $path has $loc LOC, expected <= $max"
    exit 1
  fi
}

check_loc_max "crates/assay-core/src/report/summary.rs" 80
check_loc_max "crates/assay-cli/src/cli/commands/bundle.rs" 80
check_loc_max "crates/assay-cli/src/cli/commands/bundle/implementation.rs" 320
check_loc_max "crates/assay-registry/src/lockfile.rs" 580

cargo fmt --check
cargo check -p assay-registry
cargo test -q -p assay-registry lockfile
cargo check -p assay-core
cargo test -q -p assay-core --lib report::summary
cargo check -p assay-cli
cargo test -q -p assay-cli -- bundle
git diff --check "$base_ref"...HEAD
git diff --check
git diff --cached --check

echo "PASS: Wave53 Step2 high-readiness split gate"
