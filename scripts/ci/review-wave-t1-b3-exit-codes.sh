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
legacy_file="crates/assay-cli/tests/contract_exit_codes.rs"

count_delta_no_increase() {
  local pattern="$1"
  local label="$2"
  local before after
  before="$({ git show "${base_ref}:${legacy_file}" | "${rg_bin}" -n "${pattern}" || true; } | wc -l | tr -d ' ')"
  after="$({ "${rg_bin}" -n "${pattern}" crates/assay-cli/tests/contract_exit_codes.rs crates/assay-cli/tests/exit_codes -g '*.rs' || true; } | wc -l | tr -d ' ')"
  echo "${label}: before=${before} after=${after}"
  if [ "${after}" -gt "${before}" ]; then
    echo "drift gate failed: ${label} increased"
    exit 1
  fi
}

echo "== Wave T1 B3 quality checks =="
cargo fmt --check
cargo clippy -p assay-cli --test contract_exit_codes -- -D warnings
cargo test -p assay-cli --test contract_exit_codes

echo "== Wave T1 B3 anchor checks =="
for name in \
  contract_ci_report_io_failure \
  contract_run_json_always_written_arg_conflict \
  contract_reason_code_trace_not_found_v2 \
  contract_legacy_v1_trace_not_found \
  contract_e72_seeds_happy_path \
  contract_exit_codes_missing_config \
  contract_replay_missing_dependency_offline \
  contract_replay_verify_failure_writes_outputs_with_provenance \
  contract_bundle_create_marks_missing_trace_as_incomplete_for_offline_replay \
  contract_replay_roundtrip_from_created_bundle \
  contract_replay_offline_is_hermetic_under_network_deny \
  contract_run_deny_deprecations_fails_on_legacy_policy_usage \
  contract_ci_deny_deprecations_fails_on_legacy_policy_usage

do
  if ! "${rg_bin}" -n "^fn ${name}\(" crates/assay-cli/tests/exit_codes -g '*.rs' >/dev/null; then
    echo "missing expected test function: ${name}"
    exit 1
  fi
done

if [ "$("${rg_bin}" -n '^fn read_run_json\(' crates/assay-cli/tests/contract_exit_codes.rs | wc -l | tr -d ' ')" -ne 1 ]; then
  echo "read_run_json helper must remain single-source in contract_exit_codes.rs"
  exit 1
fi

echo "== Wave T1 B3 drift gates =="
count_delta_no_increase 'unwrap\(|expect\(' 'unwrap/expect'
count_delta_no_increase '\bunsafe\b' 'unsafe'
count_delta_no_increase 'println!\(|eprintln!\(|print!\(|dbg!\(' 'print/debug'
count_delta_no_increase 'panic!\(|todo!\(|unimplemented!\(' 'panic/todo/unimplemented'

echo "== Wave T1 B3 diff allowlist =="
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^crates/assay-cli/tests/contract_exit_codes.rs$|^crates/assay-cli/tests/exit_codes/|^docs/contributing/SPLIT-CHECKLIST-wave-t1-b3-exit-codes.md$|^docs/contributing/SPLIT-MOVE-MAP-wave-t1-b3-exit-codes.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave-t1-b3-exit-codes.md$|^scripts/ci/review-wave-t1-b3-exit-codes.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo "workflow changes are forbidden in Wave T1 B3"
  exit 1
fi

echo "Wave T1 B3 reviewer script: PASS"
