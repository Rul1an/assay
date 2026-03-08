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

echo '== Coverage command Step2 quality checks =='
cargo fmt --check
cargo clippy -p assay-cli --all-targets -- -D warnings
cargo test -p assay-cli coverage_contract_generates_valid_report_from_basic_jsonl -- --exact
cargo test -p assay-cli coverage_out_md_writes_json_and_markdown_artifacts -- --exact
cargo test -p assay-cli coverage_declared_tools_file_union_with_flags -- --exact

echo '== Coverage command Step2 scope checks =='
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^crates/assay-cli/src/cli/commands/coverage\.rs$|^crates/assay-cli/src/cli/commands/coverage/|^docs/contributing/SPLIT-CHECKLIST-coverage-command-step2\.md$|^docs/contributing/SPLIT-MOVE-MAP-coverage-command-step2\.md$|^docs/contributing/SPLIT-REVIEW-PACK-coverage-command-step2\.md$|^scripts/ci/review-coverage-command-step2\.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo 'non-allowlisted files detected:'
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo 'workflow changes are forbidden in coverage-command Step2'
  exit 1
fi

if git status --porcelain -- crates/assay-cli/src/cli/commands/coverage | "${rg_bin}" '^\?\?'; then
  echo 'untracked files under crates/assay-cli/src/cli/commands/coverage/** are forbidden in Step2'
  exit 1
fi

echo '== Coverage command Step2 facade invariants =='
facade='crates/assay-cli/src/cli/commands/coverage/mod.rs'
if [ ! -f "${facade}" ]; then
  echo "missing facade file: ${facade}"
  exit 1
fi

if [ -f 'crates/assay-cli/src/cli/commands/coverage.rs' ]; then
  echo 'coverage.rs should not remain after module-directory split'
  exit 1
fi

facade_loc="$(awk 'NF{c++} END{print c+0}' "${facade}")"
if [ "${facade_loc}" -gt 200 ]; then
  echo "facade LOC budget exceeded (${facade_loc} > 200): ${facade}"
  exit 1
fi

"${rg_bin}" -n '^mod format_md;\s*$' "${facade}" >/dev/null || { echo "missing 'mod format_md;'"; exit 1; }
"${rg_bin}" -n '^mod generate;\s*$' "${facade}" >/dev/null || { echo "missing 'mod generate;'"; exit 1; }
"${rg_bin}" -n '^mod io;\s*$' "${facade}" >/dev/null || { echo "missing 'mod io;'"; exit 1; }
"${rg_bin}" -n '^mod legacy;\s*$' "${facade}" >/dev/null || { echo "missing 'mod legacy;'"; exit 1; }
"${rg_bin}" -n '^mod report;\s*$' "${facade}" >/dev/null || { echo "missing 'mod report;'"; exit 1; }
"${rg_bin}" -n '^mod schema;\s*$' "${facade}" >/dev/null || { echo "missing 'mod schema;'"; exit 1; }

"${rg_bin}" -n '^pub async fn cmd_coverage\(' "${facade}" >/dev/null || { echo 'missing cmd_coverage facade entry'; exit 1; }
"${rg_bin}" -n '^pub\(crate\) async fn write_generated_coverage_report\(' "${facade}" >/dev/null || {
  echo 'missing write_generated_coverage_report facade wrapper';
  exit 1;
}
"${rg_bin}" -n '^pub\(crate\) async fn write_generated_coverage_report_with_format\(' "${facade}" >/dev/null || {
  echo 'missing write_generated_coverage_report_with_format facade wrapper';
  exit 1;
}

async_fn_count="$("${rg_bin}" -n '^\s*(pub(\(crate\))?\s+)?async\s+fn\s+' "${facade}" | wc -l | tr -d ' ')"
if [ "${async_fn_count}" -ne 3 ]; then
  echo "expected exactly 3 async fn definitions in facade, got ${async_fn_count}"
  exit 1
fi

if "${rg_bin}" -n 'create_dir_all|tokio::fs::write|tokio::fs::read_to_string|CoverageAnalyzer::from_policy|Baseline::from_coverage_report|capture_git_info' "${facade}" >/dev/null; then
  echo 'bulk IO/legacy logic markers leaked into facade'
  "${rg_bin}" -n 'create_dir_all|tokio::fs::write|tokio::fs::read_to_string|CoverageAnalyzer::from_policy|Baseline::from_coverage_report|capture_git_info' "${facade}"
  exit 1
fi

for call in '^\s*write_generated_coverage_report_with_format\(' \
            'generate::write_generated_coverage_report_with_format\(' \
            'generate::cmd_coverage_generate\(' \
            'legacy::cmd_coverage_legacy\('; do
  count="$("${rg_bin}" -n "${call}" "${facade}" | wc -l | tr -d ' ')"
  if [ "${count}" -ne 1 ]; then
    echo "expected exactly one facade call-site for ${call}, got ${count}"
    exit 1
  fi
done

echo '== Coverage command Step2 boundary invariants =='
"${rg_bin}" -n 'out_md|routes_top|declared_tools_file|validate_coverage_report_v1' crates/assay-cli/src/cli/commands/coverage/generate.rs >/dev/null || {
  echo 'missing generator markers in generate.rs'
  exit 1
}
if "${rg_bin}" -n 'capture_git_info|Baseline::from_coverage_report|CoverageAnalyzer::from_policy' crates/assay-cli/src/cli/commands/coverage/generate.rs >/dev/null; then
  echo 'legacy analyzer/baseline markers leaked into generate.rs'
  exit 1
fi

"${rg_bin}" -n 'CoverageAnalyzer::from_policy|Baseline::from_coverage_report|capture_git_info' crates/assay-cli/src/cli/commands/coverage/legacy.rs >/dev/null || {
  echo 'missing legacy analyzer/baseline markers in legacy.rs'
  exit 1
}

"${rg_bin}" -n 'tokio::fs::create_dir_all|tokio::fs::write' crates/assay-cli/src/cli/commands/coverage/io.rs >/dev/null || {
  echo 'missing write/path prep markers in io.rs'
  exit 1
}
if "${rg_bin}" -n 'validate_coverage_report_v1|CoverageAnalyzer::from_policy|Baseline::from_coverage_report|capture_git_info' crates/assay-cli/src/cli/commands/coverage/io.rs >/dev/null; then
  echo 'unexpected report-schema/legacy markers in io.rs'
  exit 1
fi

echo 'Coverage command Step2 reviewer script: PASS'
