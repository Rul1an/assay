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
target_a="crates/assay-core/tests/parity.rs"
target_b="crates/assay-registry/src/verify_internal/tests.rs"
target_c="crates/assay-cli/tests/contract_exit_codes.rs"

check_no_increase() {
  local file="$1"
  local pattern="$2"
  local label="$3"
  local before after
  before="$(
    git show "${base_ref}:${file}" | { "${rg_bin}" -n "${pattern}" || true; } | wc -l | tr -d ' '
  )"
  after="$(
    { "${rg_bin}" -n "${pattern}" "${file}" || true; } | wc -l | tr -d ' '
  )"
  echo "${file} ${label}: before=${before} after=${after}"
  if [ "${after}" -gt "${before}" ]; then
    echo "drift gate failed: ${file} ${label} increased"
    exit 1
  fi
}

echo "== Wave T1 Step A quality checks =="
cargo fmt --check
cargo clippy -p assay-core -p assay-registry -p assay-cli --all-targets -- -D warnings
cargo test -p assay-core --test parity
cargo test -p assay-registry verify_internal
cargo test -p assay-cli --test contract_exit_codes

echo "== Wave T1 Step A freeze gate (no target file edits) =="
for file in "${target_a}" "${target_b}" "${target_c}"; do
  if ! git diff --quiet "${base_ref}...HEAD" -- "${file}"; then
    echo "${file} changed in Step A; only docs/reviewer artifacts are allowed"
    git diff -- "${file}" | sed -n '1,160p'
    exit 1
  fi
done

echo "== Wave T1 Step A drift gates =="
for file in "${target_a}" "${target_b}" "${target_c}"; do
  check_no_increase "${file}" 'unwrap\(|expect\(' 'unwrap/expect'
  check_no_increase "${file}" '\bunsafe\b' 'unsafe'
  check_no_increase "${file}" 'println!\(|eprintln!\(|print!\(|dbg!\(' 'print/debug'
  check_no_increase "${file}" 'panic!\(|todo!\(|unimplemented!\(' 'panic/todo/unimplemented'
done

echo "== Wave T1 Step A diff allowlist =="
leaks="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "${rg_bin}" -v '^docs/contributing/SPLIT-CHECKLIST-wave-t1-tests.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave-t1-tests.md$|^scripts/ci/review-wave-t1-tests-a.sh$' || true
)"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo "workflow changes are forbidden in Wave T1 Step A"
  exit 1
fi

echo "Wave T1 Step A reviewer script: PASS"
