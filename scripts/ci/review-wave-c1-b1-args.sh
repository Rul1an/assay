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
mod_file="crates/assay-cli/src/cli/args/mod.rs"
split_files=(
  "crates/assay-cli/src/cli/args/coverage.rs"
  "crates/assay-cli/src/cli/args/evidence.rs"
  "crates/assay-cli/src/cli/args/import.rs"
  "crates/assay-cli/src/cli/args/mcp.rs"
  "crates/assay-cli/src/cli/args/replay.rs"
  "crates/assay-cli/src/cli/args/runtime.rs"
  "crates/assay-cli/src/cli/args/sim.rs"
)
all_split_surface=("${mod_file}" "${split_files[@]}")

check_no_increase() {
  local pattern="$1"
  local label="$2"
  local before after
  before="$({ git show "${base_ref}:${mod_file}" | "${rg_bin}" -n "${pattern}" || true; } | wc -l | tr -d ' ')"
  after="$({ "${rg_bin}" -n "${pattern}" "${all_split_surface[@]}" || true; } | wc -l | tr -d ' ')"
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
  hits="$({ "${rg_bin}" -n "${pattern}" "${all_split_surface[@]}" || true; })"
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

echo "== Wave C1 B1 quality checks =="
cargo fmt --check
cargo clippy -p assay-cli --all-targets -- -D warnings
cargo test -p assay-cli

echo "== Wave C1 B1 facade-thinness checks =="
line_count="$(wc -l < "${mod_file}" | tr -d ' ')"
echo "${mod_file} line_count=${line_count}"
if [ "${line_count}" -gt 180 ]; then
  echo "facade gate failed: ${mod_file} should remain thin"
  exit 1
fi

if "${rg_bin}" -n 'pub struct (ValidateArgs|ImportArgs|CoverageArgs|McpArgs|CalibrateArgs|DoctorArgs|WatchArgs|FixArgs|SandboxArgs|SimArgs|EvidenceArgs)' "${mod_file}"; then
  echo "facade gate failed: moved args structs must not be redefined in mod.rs"
  exit 1
fi

for required_mod in coverage evidence import mcp replay runtime sim; do
  if ! "${rg_bin}" -n "^pub mod ${required_mod};" "${mod_file}" >/dev/null; then
    echo "missing module declaration in facade: ${required_mod}"
    exit 1
  fi
done

echo "== Wave C1 B1 single-source checks =="
assert_single_source '^pub struct ValidateArgs\b' 'crates/assay-cli/src/cli/args/replay.rs' 'ValidateArgs'
assert_single_source '^pub struct ImportArgs\b' 'crates/assay-cli/src/cli/args/import.rs' 'ImportArgs'
assert_single_source '^pub struct CoverageArgs\b' 'crates/assay-cli/src/cli/args/coverage.rs' 'CoverageArgs'
assert_single_source '^pub struct McpArgs\b' 'crates/assay-cli/src/cli/args/mcp.rs' 'McpArgs'
assert_single_source '^pub struct CalibrateArgs\b' 'crates/assay-cli/src/cli/args/runtime.rs' 'CalibrateArgs'
assert_single_source '^pub enum MaxRisk\b' 'crates/assay-cli/src/cli/args/runtime.rs' 'MaxRisk'
assert_single_source '^pub struct SimArgs\b' 'crates/assay-cli/src/cli/args/sim.rs' 'SimArgs'
assert_single_source '^pub struct EvidenceArgs\b' 'crates/assay-cli/src/cli/args/evidence.rs' 'EvidenceArgs'

echo "== Wave C1 B1 drift gates =="
check_no_increase 'unwrap\(|expect\(' 'unwrap/expect'
check_no_increase '\bunsafe\b' 'unsafe'
check_no_increase 'println!\(|eprintln!\(|print!\(|dbg!\(' 'print/debug'
check_no_increase 'panic!\(|todo!\(|unimplemented!\(' 'panic/todo/unimplemented'

echo "== Wave C1 B1 diff allowlist =="
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^crates/assay-cli/src/cli/args/mod.rs$|^crates/assay-cli/src/cli/args/coverage.rs$|^crates/assay-cli/src/cli/args/evidence.rs$|^crates/assay-cli/src/cli/args/import.rs$|^crates/assay-cli/src/cli/args/mcp.rs$|^crates/assay-cli/src/cli/args/replay.rs$|^crates/assay-cli/src/cli/args/runtime.rs$|^crates/assay-cli/src/cli/args/sim.rs$|^docs/contributing/SPLIT-CHECKLIST-wave-c1-b1-args.md$|^docs/contributing/SPLIT-MOVE-MAP-wave-c1-b1-args.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave-c1-b1-args.md$|^scripts/ci/review-wave-c1-b1-args.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo "workflow changes are forbidden in Wave C1 B1"
  exit 1
fi

echo "Wave C1 B1 reviewer script: PASS"
