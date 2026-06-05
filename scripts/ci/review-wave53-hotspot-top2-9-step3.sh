#!/usr/bin/env bash
set -euo pipefail

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"

base_ref="${BASE_REF:-origin/codex/wave53-hotspot-top2-9-step2}"
if ! git rev-parse --verify "$base_ref" >/dev/null 2>&1; then
  if [[ -z "${BASE_REF:-}" ]] && git rev-parse --verify codex/wave53-hotspot-top2-9-step2 >/dev/null 2>&1; then
    base_ref="codex/wave53-hotspot-top2-9-step2"
  else
    echo "FAIL: cannot resolve Step3 base ref: $base_ref"
    echo "Set BASE_REF to the Step2 branch/ref used for this stacked review."
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

allowed_pattern='^(docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9\.md|docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step3\.md|docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step3\.md|docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step3\.md|scripts/ci/review-wave53-hotspot-top2-9-step3\.sh|crates/assay-cli/src/cli/commands/runner_spike\.rs|crates/assay-cli/src/cli/commands/runner_spike/args\.rs|crates/assay-cli/src/cli/commands/runner_spike/implementation\.rs|crates/assay-cli/src/cli/commands/runner_spike/spec\.rs|crates/assay-cli/src/cli/commands/runner_spike/phases\.rs|crates/assay-cli/src/cli/commands/runner_spike/cgroup\.rs|crates/assay-cli/src/cli/commands/runner_spike/logs\.rs|crates/assay-cli/src/cli/commands/runner_spike/exit_status\.rs|crates/assay-cli/src/cli/commands/doctor\.rs|crates/assay-cli/src/cli/commands/doctor/implementation\.rs|crates/assay-cli/src/cli/commands/doctor/fixes\.rs|crates/assay-cli/src/cli/commands/doctor/patching\.rs|crates/assay-cli/src/cli/commands/doctor/parse_error\.rs)$'
unexpected="$(printf '%s\n' "$changed" | rg -v "$allowed_pattern" || true)"
if [[ -n "$unexpected" ]]; then
  echo "FAIL: Wave53 Step3 changed files outside the allowlist:"
  printf '%s\n' "$unexpected"
  exit 1
fi

if printf '%s\n' "$changed" | rg '^\.github/workflows/' >/dev/null; then
  echo "FAIL: workflow edits are out of scope for Wave53 Step3"
  exit 1
fi

if printf '%s\n' "$changed" | rg '^crates/assay-ebpf/src/vmlinux\.rs$' >/dev/null; then
  echo "FAIL: generated vmlinux.rs is out of scope for Wave53 Step3"
  exit 1
fi

required=(
  "docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9.md"
  "docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step3.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step3.md"
  "scripts/ci/review-wave53-hotspot-top2-9-step3.sh"
  "crates/assay-cli/src/cli/commands/runner_spike.rs"
  "crates/assay-cli/src/cli/commands/runner_spike/args.rs"
  "crates/assay-cli/src/cli/commands/runner_spike/implementation.rs"
  "crates/assay-cli/src/cli/commands/runner_spike/spec.rs"
  "crates/assay-cli/src/cli/commands/runner_spike/phases.rs"
  "crates/assay-cli/src/cli/commands/runner_spike/cgroup.rs"
  "crates/assay-cli/src/cli/commands/runner_spike/logs.rs"
  "crates/assay-cli/src/cli/commands/runner_spike/exit_status.rs"
  "crates/assay-cli/src/cli/commands/doctor.rs"
  "crates/assay-cli/src/cli/commands/doctor/implementation.rs"
  "crates/assay-cli/src/cli/commands/doctor/fixes.rs"
  "crates/assay-cli/src/cli/commands/doctor/patching.rs"
  "crates/assay-cli/src/cli/commands/doctor/parse_error.rs"
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

for module in args cgroup exit_status implementation logs phases spec; do
  require_marker "^mod ${module};$" "crates/assay-cli/src/cli/commands/runner_spike.rs" "runner_spike facade must declare ${module} module"
done
require_marker '^pub use args::\{RunnerSpikeArgs, RunnerSpikeCommand, RunnerSpikeRunArgs\};$' "crates/assay-cli/src/cli/commands/runner_spike.rs" "runner_spike facade must re-export Clap types"
require_marker 'implementation::run\(args\)\.await' "crates/assay-cli/src/cli/commands/runner_spike.rs" "runner_spike facade must delegate to implementation::run"
forbid_marker '^async fn cmd_run\b|^fn cmd_run_contract_only\b|^fn build_spec\b|^fn write_u32_decimal\b' "crates/assay-cli/src/cli/commands/runner_spike.rs" "runner_spike facade must not own moved function bodies"

for module in fixes implementation parse_error patching; do
  require_marker "^mod ${module};$" "crates/assay-cli/src/cli/commands/doctor.rs" "doctor facade must declare ${module} module"
done
require_marker 'implementation::run\(args, legacy_mode\)\.await' "crates/assay-cli/src/cli/commands/doctor.rs" "doctor facade must delegate to implementation::run"
forbid_marker '^async fn run_doctor_fix\b|^fn try_fix_parse_error\b|^fn parse_unknown_field_error\b|^fn write_text_file\b' "crates/assay-cli/src/cli/commands/doctor.rs" "doctor facade must not own moved function bodies"

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

check_loc_max "crates/assay-cli/src/cli/commands/runner_spike.rs" 80
check_loc_max "crates/assay-cli/src/cli/commands/doctor.rs" 80
check_loc_max "crates/assay-cli/src/cli/commands/runner_spike/cgroup.rs" 480
check_loc_max "crates/assay-cli/src/cli/commands/doctor/fixes.rs" 260

cargo fmt --check
cargo check -p assay-cli
cargo test -q -p assay-cli -- runner_spike
cargo test -q -p assay-cli -- doctor
cargo clippy -p assay-cli --all-targets -- -D warnings
git diff --check

echo "PASS: Wave53 Step3 CLI command split gate"
