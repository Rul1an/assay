#!/usr/bin/env bash
set -euo pipefail

base_ref="${BASE_REF:-${1:-}}"
if [ -z "${base_ref}" ] && [ -n "${GITHUB_BASE_REF:-}" ]; then
  base_ref="origin/${GITHUB_BASE_REF}"
fi
if [ -z "${base_ref}" ]; then
  base_ref="origin/main"
fi
if ! git rev-parse --verify --quiet "${base_ref}^{commit}" >/dev/null; then
  echo "BASE_REF not found: ${base_ref}"
  exit 1
fi

echo "BASE_REF=${base_ref} sha=$(git rev-parse "${base_ref}")"
echo "HEAD sha=$(git rev-parse HEAD)"

rg_bin="$(command -v rg)"
old_file="crates/assay-cli/src/cli/commands/replay.rs"
new_mod_file="crates/assay-cli/src/cli/commands/replay/mod.rs"
replay_surface=(
  "${new_mod_file}"
  "crates/assay-cli/src/cli/commands/replay/flow.rs"
  "crates/assay-cli/src/cli/commands/replay/run_args.rs"
  "crates/assay-cli/src/cli/commands/replay/failure.rs"
  "crates/assay-cli/src/cli/commands/replay/manifest.rs"
  "crates/assay-cli/src/cli/commands/replay/fs_ops.rs"
  "crates/assay-cli/src/cli/commands/replay/provenance.rs"
  "crates/assay-cli/src/cli/commands/replay/tests.rs"
)

check_no_increase() {
  local pattern="$1"
  local label="$2"
  local before after
  before="$({ git show "${base_ref}:${old_file}" | "${rg_bin}" -n "${pattern}" || true; } | wc -l | tr -d ' ')"
  after="$({ "${rg_bin}" -n "${pattern}" "${replay_surface[@]}" || true; } | wc -l | tr -d ' ')"
  echo "${label}: before=${before} after=${after}"
  if [ "${after}" -gt "${before}" ]; then
    echo "drift gate failed: ${label} increased"
    exit 1
  fi
}

assert_single_source() {
  local pattern="$1"
  local expected_file="$2"
  local label="$3"
  local hits count
  hits="$({ "${rg_bin}" -n "${pattern}" "${replay_surface[@]}" || true; })"
  count="$(printf '%s\n' "${hits}" | sed '/^$/d' | wc -l | tr -d ' ')"
  if [ "${count}" -ne 1 ]; then
    echo "single-source gate failed (${label}): expected 1 hit, got ${count}"
    printf '%s\n' "${hits}"
    exit 1
  fi
  if ! printf '%s\n' "${hits}" | "${rg_bin}" -q "^${expected_file}:"; then
    echo "single-source gate failed (${label}): expected in ${expected_file}"
    printf '%s\n' "${hits}"
    exit 1
  fi
}

echo "== Wave C1 B2 quality checks =="
cargo fmt --check
cargo clippy -p assay-cli --all-targets -- -D warnings
cargo test -p assay-cli

echo "== Wave C1 B2 facade-thinness checks =="
line_count="$(wc -l < "${new_mod_file}" | tr -d ' ')"
echo "${new_mod_file} line_count=${line_count}"
if [ "${line_count}" -gt 40 ]; then
  echo "facade gate failed: replay/mod.rs should remain thin"
  exit 1
fi
if ! "${rg_bin}" -n '^pub use flow::run;' "${new_mod_file}" >/dev/null; then
  echo "facade gate failed: replay/mod.rs must re-export run"
  exit 1
fi
if "${rg_bin}" -n '^pub async fn run\(' "${new_mod_file}" >/dev/null; then
  echo "facade gate failed: run implementation must stay in flow.rs"
  exit 1
fi

echo "== Wave C1 B2 single-source checks =="
assert_single_source '^pub async fn run\(' 'crates/assay-cli/src/cli/commands/replay/flow.rs' 'run'
assert_single_source '^pub\(super\) fn replay_run_args\(' 'crates/assay-cli/src/cli/commands/replay/run_args.rs' 'replay_run_args'
assert_single_source '^pub\(super\) fn write_replay_failure\(' 'crates/assay-cli/src/cli/commands/replay/failure.rs' 'write_replay_failure'
assert_single_source '^pub\(super\) fn write_missing_dependency\(' 'crates/assay-cli/src/cli/commands/replay/failure.rs' 'write_missing_dependency'
assert_single_source '^pub\(super\) fn offline_dependency_message\(' 'crates/assay-cli/src/cli/commands/replay/manifest.rs' 'offline_dependency_message'
assert_single_source '^pub\(super\) fn apply_seed_override\(' 'crates/assay-cli/src/cli/commands/replay/fs_ops.rs' 'apply_seed_override'
assert_single_source '^pub\(super\) fn annotate_run_json_provenance\(' 'crates/assay-cli/src/cli/commands/replay/provenance.rs' 'annotate_run_json_provenance'
assert_single_source '^pub\(super\) struct ReplayWorkspace\b' 'crates/assay-cli/src/cli/commands/replay/fs_ops.rs' 'ReplayWorkspace'

echo "== Wave C1 B2 drift gates =="
check_no_increase 'unwrap\(|expect\(' 'unwrap/expect'
check_no_increase '\bunsafe\b' 'unsafe'
check_no_increase 'println!\(|eprintln!\(|print!\(|dbg!\(' 'print/debug'
check_no_increase 'panic!\(|todo!\(|unimplemented!\(' 'panic/todo/unimplemented'

echo "== Wave C1 B2 diff allowlist =="
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^crates/assay-cli/src/cli/commands/replay.rs$|^crates/assay-cli/src/cli/commands/replay/mod.rs$|^crates/assay-cli/src/cli/commands/replay/flow.rs$|^crates/assay-cli/src/cli/commands/replay/run_args.rs$|^crates/assay-cli/src/cli/commands/replay/failure.rs$|^crates/assay-cli/src/cli/commands/replay/manifest.rs$|^crates/assay-cli/src/cli/commands/replay/fs_ops.rs$|^crates/assay-cli/src/cli/commands/replay/provenance.rs$|^crates/assay-cli/src/cli/commands/replay/tests.rs$|^docs/contributing/SPLIT-CHECKLIST-wave-c1-b2-replay.md$|^docs/contributing/SPLIT-MOVE-MAP-wave-c1-b2-replay.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave-c1-b2-replay.md$|^scripts/ci/review-wave-c1-b2-replay.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo "workflow changes are forbidden in Wave C1 B2"
  exit 1
fi

echo "Wave C1 B2 reviewer script: PASS"
