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
old_file="crates/assay-cli/src/env_filter.rs"
new_mod_file="crates/assay-cli/src/env_filter/mod.rs"
env_surface=(
  "${new_mod_file}"
  "crates/assay-cli/src/env_filter/engine.rs"
  "crates/assay-cli/src/env_filter/matcher.rs"
  "crates/assay-cli/src/env_filter/patterns.rs"
  "crates/assay-cli/src/env_filter/tests.rs"
)

check_no_increase() {
  local pattern="$1"
  local label="$2"
  local before after
  before="$({ git show "${base_ref}:${old_file}" | "${rg_bin}" -n "${pattern}" || true; } | wc -l | tr -d ' ')"
  after="$({ "${rg_bin}" -n "${pattern}" "${env_surface[@]}" || true; } | wc -l | tr -d ' ')"
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
  hits="$({ "${rg_bin}" -n "${pattern}" "${env_surface[@]}" || true; })"
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

echo "== Wave C1 B3 quality checks =="
cargo fmt --check
cargo clippy -p assay-cli --all-targets -- -D warnings
cargo test -p assay-cli

echo "== Wave C1 B3 facade-thinness checks =="
line_count="$(wc -l < "${new_mod_file}" | tr -d ' ')"
echo "${new_mod_file} line_count=${line_count}"
if [ "${line_count}" -gt 40 ]; then
  echo "facade gate failed: env_filter/mod.rs should remain thin"
  exit 1
fi
if "${rg_bin}" -n '^pub struct EnvFilter|^pub enum EnvMode|^pub struct EnvFilterResult' "${new_mod_file}"; then
  echo "facade gate failed: core type definitions must be in engine.rs"
  exit 1
fi

echo "== Wave C1 B3 single-source checks =="
assert_single_source '^pub enum EnvMode\b' 'crates/assay-cli/src/env_filter/engine.rs' 'EnvMode'
assert_single_source '^pub struct EnvFilterResult\b' 'crates/assay-cli/src/env_filter/engine.rs' 'EnvFilterResult'
assert_single_source '^pub struct EnvFilter\b' 'crates/assay-cli/src/env_filter/engine.rs' 'EnvFilter'
assert_single_source '^pub fn matches_any_pattern\(' 'crates/assay-cli/src/env_filter/matcher.rs' 'matches_any_pattern'
assert_single_source '^pub const SAFE_BASE_PATTERNS\b' 'crates/assay-cli/src/env_filter/patterns.rs' 'SAFE_BASE_PATTERNS'
assert_single_source '^pub const SECRET_SCRUB_PATTERNS\b' 'crates/assay-cli/src/env_filter/patterns.rs' 'SECRET_SCRUB_PATTERNS'
assert_single_source '^pub const EXEC_INFLUENCE_PATTERNS\b' 'crates/assay-cli/src/env_filter/patterns.rs' 'EXEC_INFLUENCE_PATTERNS'

echo "== Wave C1 B3 drift gates =="
check_no_increase 'unwrap\(|expect\(' 'unwrap/expect'
check_no_increase '\bunsafe\b' 'unsafe'
check_no_increase 'println!\(|eprintln!\(|print!\(|dbg!\(' 'print/debug'
check_no_increase 'panic!\(|todo!\(|unimplemented!\(' 'panic/todo/unimplemented'

echo "== Wave C1 B3 diff allowlist =="
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^crates/assay-cli/src/env_filter.rs$|^crates/assay-cli/src/env_filter/mod.rs$|^crates/assay-cli/src/env_filter/engine.rs$|^crates/assay-cli/src/env_filter/matcher.rs$|^crates/assay-cli/src/env_filter/patterns.rs$|^crates/assay-cli/src/env_filter/tests.rs$|^docs/contributing/SPLIT-CHECKLIST-wave-c1-b3-env-filter.md$|^docs/contributing/SPLIT-MOVE-MAP-wave-c1-b3-env-filter.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave-c1-b3-env-filter.md$|^scripts/ci/review-wave-c1-b3-env-filter.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo "workflow changes are forbidden in Wave C1 B3"
  exit 1
fi

echo "Wave C1 B3 reviewer script: PASS"
