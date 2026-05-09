#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

TRUST_BASIS="crates/assay-evidence/src/trust_basis.rs"
CANONICAL="crates/assay-evidence/src/trust_basis/canonical.rs"
GENERATION="crates/assay-evidence/src/trust_basis/generation.rs"
CLASSIFIERS="crates/assay-evidence/src/trust_basis/classifiers.rs"
TYPES="crates/assay-evidence/src/trust_basis/types.rs"
DIFF="crates/assay-evidence/src/trust_basis/diff.rs"
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
  echo "FAIL: Wave 51 Trust Basis Step11 must not touch workflows"
  exit 1
fi
if changed_in_review_scope crates/assay-ebpf/src/vmlinux.rs; then
  echo "FAIL: generated vmlinux.rs must stay out of scope"
  exit 1
fi

echo "[review] module boundary"
test -f "$CANONICAL" || { echo "FAIL: canonical module missing"; exit 1; }
test -f "$GENERATION" || { echo "FAIL: generation module missing"; exit 1; }
test -f "$CLASSIFIERS" || { echo "FAIL: classifiers module missing"; exit 1; }
test -f "$TYPES" || { echo "FAIL: types module missing"; exit 1; }
test -f "$DIFF" || { echo "FAIL: diff module missing"; exit 1; }
assert_rg '^mod canonical;' "$TRUST_BASIS" "facade must declare canonical module"
assert_rg '^pub fn to_canonical_json_bytes' "$TRUST_BASIS" "public canonical facade missing"
assert_rg 'canonical::to_canonical_json_bytes' "$TRUST_BASIS" "canonical facade must delegate to canonical module"
assert_rg '^pub fn generate_trust_basis' "$TRUST_BASIS" "public generate facade missing"
assert_rg 'pub use trust_basis::' "$LIB" "root trust_basis re-export missing"

assert_not_rg 'PrettyFormatter|Serializer::with_formatter|serde::Serialize|output\.push\(b' "$TRUST_BASIS" "canonical serializer body must not remain in facade"
assert_rg '^pub\(super\) fn to_canonical_json_bytes' "$CANONICAL" "canonical module must own internal canonical implementation"
assert_rg 'PrettyFormatter' "$CANONICAL" "canonical module must own pretty formatter"
assert_rg 'Serializer::with_formatter' "$CANONICAL" "canonical module must own serializer"
assert_rg 'serialize\(&mut serializer\)' "$CANONICAL" "canonical module must serialize trust basis"
assert_rg "output\.push\(b'\\\\n'\)" "$CANONICAL" "canonical module must preserve trailing newline"

facade_non_test_loc=$(awk '/#\[cfg\(test\)\]/{exit} {count++} END{print count}' "$TRUST_BASIS")
if [ "$facade_non_test_loc" -gt 35 ]; then
  echo "FAIL: trust_basis facade non-test LOC drifted above Step11 ceiling ($facade_non_test_loc > 35)"
  exit 1
fi

echo "[review] freeze contracts still present"
assert_rg 'trust_basis_contract_generated_claim_id_order_is_frozen' "$TRUST_BASIS" "claim order contract missing"
assert_rg 'trust_basis_contract_canonical_json_shape_is_frozen' "$TRUST_BASIS" "canonical JSON contract missing"
assert_rg 'trust_basis_contract_diff_report_ordering_is_frozen' "$TRUST_BASIS" "diff ordering contract missing"

echo "[review] repo checks"
cargo fmt --check
cargo check -p assay-evidence
cargo clippy -p assay-evidence --all-targets -- -D warnings
cargo test -p assay-evidence --lib trust_basis_contract_
cargo test -p assay-evidence --lib trust_basis
git diff --check

echo "[review] PASS"
