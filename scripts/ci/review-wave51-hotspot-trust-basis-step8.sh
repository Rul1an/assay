#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

TRUST_BASIS="crates/assay-evidence/src/trust_basis.rs"
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
  echo "FAIL: Wave 51 Trust Basis Step8 must not touch workflows"
  exit 1
fi
if ! git diff --quiet -- crates/assay-ebpf/src/vmlinux.rs; then
  echo "FAIL: generated vmlinux.rs must stay out of scope"
  exit 1
fi

echo "[review] freeze-only boundary"
if [ -d crates/assay-evidence/src/trust_basis ]; then
  echo "FAIL: Step8 must not introduce trust_basis implementation modules yet"
  exit 1
fi
assert_rg 'pub enum TrustClaimId' "$TRUST_BASIS" "TrustClaimId moved before freeze split"
assert_rg 'pub struct TrustBasisClaim' "$TRUST_BASIS" "TrustBasisClaim moved before freeze split"
assert_rg 'pub struct TrustBasisDiffReport' "$TRUST_BASIS" "TrustBasisDiffReport moved before freeze split"
assert_rg 'pub fn diff_trust_basis\(' "$TRUST_BASIS" "diff_trust_basis moved before freeze split"
assert_rg 'pub fn generate_trust_basis' "$TRUST_BASIS" "generate_trust_basis moved before freeze split"
assert_rg 'pub fn to_canonical_json_bytes' "$TRUST_BASIS" "to_canonical_json_bytes moved before freeze split"
assert_rg 'pub fn duplicate_trust_basis_claim_ids' "$TRUST_BASIS" "duplicate claim helper moved before freeze split"
assert_rg 'pub use trust_basis::' "$LIB" "root trust_basis re-export missing"
assert_not_rg 'mod types;|mod diff;|mod generate;|mod classifiers;|mod canonical;' "$TRUST_BASIS" "Step8 must not start implementation split"

echo "[review] freeze contracts"
assert_rg 'trust_basis_contract_generated_claim_id_order_is_frozen' "$TRUST_BASIS" "claim order contract missing"
assert_rg 'trust_basis_contract_canonical_json_shape_is_frozen' "$TRUST_BASIS" "canonical JSON contract missing"
assert_rg 'trust_basis_contract_diff_report_ordering_is_frozen' "$TRUST_BASIS" "diff ordering contract missing"
assert_rg 'json!\("bundle_verified"\)' "$TRUST_BASIS" "serialized claim id spelling not locked"
assert_rg '"boundary": "supported-delegated-flows-only"' "$TRUST_BASIS" "canonical boundary spelling not locked"
assert_rg 'report\.level_order' "$TRUST_BASIS" "diff level order not locked"
assert_rg 'report\.summary\.regressed_claims' "$TRUST_BASIS" "diff summary counters not locked"

echo "[review] repo checks"
cargo fmt --check
cargo check -p assay-evidence
cargo clippy -p assay-evidence --all-targets -- -D warnings
cargo test -p assay-evidence --lib trust_basis_contract_
cargo test -p assay-evidence --lib trust_basis
git diff --check

echo "[review] PASS"
