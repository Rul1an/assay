#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

TRUST_BASIS="crates/assay-evidence/src/trust_basis.rs"
TESTS="crates/assay-evidence/src/trust_basis/tests.rs"
LIB="crates/assay-evidence/src/lib.rs"

assert_rg() {
  local pattern="$1"
  local file="$2"
  local message="$3"
  if ! rg -n "$pattern" "$file" >/dev/null; then
    echo "FAIL: $message"
    exit 1
  fi
}

assert_not_rg() {
  local pattern="$1"
  local file="$2"
  local message="$3"
  if rg -n "$pattern" "$file" >/dev/null; then
    echo "FAIL: $message"
    exit 1
  fi
}

changed_in_review_scope() {
  local path="$1"
  local base_ref
  local base
  local candidates=()

  if ! git diff --quiet -- "$path"; then
    return 0
  fi
  if ! git diff --cached --quiet -- "$path"; then
    return 0
  fi

  if [ -n "${ASSAY_REVIEW_BASE_REF:-}" ]; then
    candidates+=("$ASSAY_REVIEW_BASE_REF")
  fi
  if [ -n "${GITHUB_BASE_REF:-}" ]; then
    candidates+=("origin/${GITHUB_BASE_REF}" "$GITHUB_BASE_REF")
  fi
  candidates+=("origin/main" "main" "HEAD^1")

  for base_ref in "${candidates[@]}"; do
    if git rev-parse --verify -q "${base_ref}^{commit}" >/dev/null; then
      base="$(git merge-base "$base_ref" HEAD 2>/dev/null || true)"
      if [ -n "$base" ]; then
        ! git diff --quiet "$base..HEAD" -- "$path"
        return $?
      fi
    fi
  done

  echo "FAIL: unable to resolve review base for scope guard"
  exit 1
}

echo "[review] workflow and generated-file guard"
if changed_in_review_scope .github/workflows; then
  echo "FAIL: Wave 51 Trust Basis Step12 must not touch workflows"
  exit 1
fi
if changed_in_review_scope crates/assay-ebpf/src/vmlinux.rs; then
  echo "FAIL: generated vmlinux.rs must stay out of scope"
  exit 1
fi

echo "[review] test layout boundary"
test -f "$TESTS" || { echo "FAIL: trust_basis tests module missing"; exit 1; }
assert_rg '^#\[cfg\(test\)\]' "$TRUST_BASIS" "facade must keep cfg test module marker"
assert_rg '^mod tests;' "$TRUST_BASIS" "facade must load tests from tests.rs"
assert_not_rg '^mod tests \{' "$TRUST_BASIS" "inline tests must not remain in facade"
assert_rg '^use super::\*;' "$TESTS" "tests module must retain parent imports"
assert_rg 'trust_basis_contract_generated_claim_id_order_is_frozen' "$TESTS" "claim order contract missing from tests module"
assert_rg 'trust_basis_contract_canonical_json_shape_is_frozen' "$TESTS" "canonical JSON contract missing from tests module"
assert_rg 'trust_basis_contract_diff_report_ordering_is_frozen' "$TESTS" "diff ordering contract missing from tests module"
assert_rg '^mod canonical;|^mod classifiers;|^mod diff;|^mod generation;|^mod types;' "$TRUST_BASIS" "production modules must remain declared"
assert_rg 'pub use trust_basis::' "$LIB" "root trust_basis re-export missing"

facade_loc=$(wc -l < "$TRUST_BASIS" | tr -d ' ')
if [ "$facade_loc" -gt 40 ]; then
  echo "FAIL: trust_basis facade LOC drifted above Step12 ceiling ($facade_loc > 40)"
  exit 1
fi

echo "[review] repo checks"
cargo fmt --check
cargo check -p assay-evidence
cargo clippy -p assay-evidence --all-targets -- -D warnings
cargo test -p assay-evidence --lib trust_basis_contract_
cargo test -p assay-evidence --lib trust_basis
git diff --check

echo "[review] PASS"
