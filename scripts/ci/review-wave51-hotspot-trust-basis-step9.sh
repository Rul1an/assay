#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

TRUST_BASIS="crates/assay-evidence/src/trust_basis.rs"
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

echo "[review] workflow and generated-file guard"
if ! git diff --quiet -- .github/workflows; then
  echo "FAIL: Wave 51 Trust Basis Step9 must not touch workflows"
  exit 1
fi
if ! git diff --quiet -- crates/assay-ebpf/src/vmlinux.rs; then
  echo "FAIL: generated vmlinux.rs must stay out of scope"
  exit 1
fi

echo "[review] module boundary"
test -f "$TYPES" || { echo "FAIL: types module missing"; exit 1; }
test -f "$DIFF" || { echo "FAIL: diff module missing"; exit 1; }
assert_rg '^mod diff;' "$TRUST_BASIS" "facade must declare diff module"
assert_rg '^mod types;' "$TRUST_BASIS" "facade must declare types module"
assert_rg '^pub use diff::\{diff_trust_basis, duplicate_trust_basis_claim_ids\};' "$TRUST_BASIS" "diff functions must be re-exported through facade"
assert_rg '^pub use types::\{' "$TRUST_BASIS" "types must be re-exported through facade"
assert_rg 'pub use trust_basis::' "$LIB" "root trust_basis re-export missing"

assert_not_rg '^pub enum TrustClaimId|^pub struct TrustBasisClaim|^pub struct TrustBasisDiffReport|^pub struct TrustBasisOptions' "$TRUST_BASIS" "public types must not remain in facade"
assert_rg '^pub enum TrustClaimId' "$TYPES" "TrustClaimId must live in types module"
assert_rg '^pub struct TrustBasisClaim' "$TYPES" "TrustBasisClaim must live in types module"
assert_rg '^pub struct TrustBasisDiffReport' "$TYPES" "TrustBasisDiffReport must live in types module"
assert_rg '^pub struct TrustBasisOptions' "$TYPES" "TrustBasisOptions must live in types module"
assert_rg '^pub const TRUST_BASIS_DIFF_SCHEMA' "$TYPES" "diff schema constant must live in types module"

assert_not_rg '^pub fn diff_trust_basis|^pub fn duplicate_trust_basis_claim_ids|^fn trust_claim_level_rank' "$TRUST_BASIS" "diff logic must not remain in facade"
assert_rg '^pub fn diff_trust_basis' "$DIFF" "diff_trust_basis must live in diff module"
assert_rg '^pub fn duplicate_trust_basis_claim_ids' "$DIFF" "duplicate claim helper must live in diff module"
assert_rg '^fn trust_claim_level_rank' "$DIFF" "level rank helper must live in diff module"

echo "[review] facade still owns generation/classifiers"
assert_rg '^pub fn generate_trust_basis' "$TRUST_BASIS" "generate_trust_basis must stay in facade for Step9"
assert_rg '^pub fn to_canonical_json_bytes' "$TRUST_BASIS" "canonical JSON helper must stay in facade for Step9"
assert_rg '^fn classify_external_eval_receipt_boundary' "$TRUST_BASIS" "receipt classifiers must stay in facade for Step9"
assert_rg '^fn classify_pack_findings' "$TRUST_BASIS" "pack classifier must stay in facade for Step9"

facade_non_test_loc=$(awk '/#\[cfg\(test\)\]/{exit} {count++} END{print count}' "$TRUST_BASIS")
if [ "$facade_non_test_loc" -gt 620 ]; then
  echo "FAIL: trust_basis facade non-test LOC drifted above Step9 ceiling ($facade_non_test_loc > 620)"
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
