#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

TRUST_BASIS="crates/assay-evidence/src/trust_basis.rs"
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

echo "[review] workflow and generated-file guard"
if ! git diff --quiet -- .github/workflows; then
  echo "FAIL: Wave 51 Trust Basis Step10 must not touch workflows"
  exit 1
fi
if ! git diff --quiet -- crates/assay-ebpf/src/vmlinux.rs; then
  echo "FAIL: generated vmlinux.rs must stay out of scope"
  exit 1
fi

echo "[review] module boundary"
test -f "$GENERATION" || { echo "FAIL: generation module missing"; exit 1; }
test -f "$CLASSIFIERS" || { echo "FAIL: classifiers module missing"; exit 1; }
test -f "$TYPES" || { echo "FAIL: types module missing"; exit 1; }
test -f "$DIFF" || { echo "FAIL: diff module missing"; exit 1; }
assert_rg '^mod classifiers;' "$TRUST_BASIS" "facade must declare classifiers module"
assert_rg '^mod generation;' "$TRUST_BASIS" "facade must declare generation module"
assert_rg '^pub fn generate_trust_basis' "$TRUST_BASIS" "public generate facade missing"
assert_rg 'generation::generate_trust_basis' "$TRUST_BASIS" "generate facade must delegate to generation module"
assert_rg '^pub fn to_canonical_json_bytes' "$TRUST_BASIS" "canonical JSON facade missing"
assert_rg 'pub use trust_basis::' "$LIB" "root trust_basis re-export missing"

assert_not_rg '^fn classify_|^const PROMPTFOO_|^const OPENFEATURE_|^const CYCLONEDX_|^fn is_supported_' "$TRUST_BASIS" "classifier internals must not remain in facade"
if awk '/#\[cfg\(test\)\]/{exit} {print}' "$TRUST_BASIS" \
  | rg -n 'BundleReader::open_with_limits|lint_bundle_with_options|claims: vec!' >/dev/null; then
  echo "FAIL: generation body must not remain in facade production code"
  exit 1
fi

assert_rg '^pub\(super\) fn generate_trust_basis' "$GENERATION" "generation module must own internal generate implementation"
assert_rg 'BundleReader::open_with_limits' "$GENERATION" "generation module must own bundle reader opening"
assert_rg 'lint_bundle_with_options' "$GENERATION" "generation module must own lint integration"
assert_rg 'claims: vec!' "$GENERATION" "generation module must own claim vector construction"

assert_rg '^pub\(super\) fn classify_signing_evidence' "$CLASSIFIERS" "signing classifier missing"
assert_rg '^pub\(super\) fn classify_external_eval_receipt_boundary' "$CLASSIFIERS" "external eval classifier missing"
assert_rg '^pub\(super\) fn classify_external_decision_receipt_boundary' "$CLASSIFIERS" "external decision classifier missing"
assert_rg '^pub\(super\) fn classify_external_inventory_receipt_boundary' "$CLASSIFIERS" "external inventory classifier missing"
assert_rg '^pub\(super\) fn classify_pack_findings' "$CLASSIFIERS" "pack classifier missing"
assert_rg 'PROMPTFOO_RECEIPT_EVENT_TYPE' "$CLASSIFIERS" "promptfoo receipt constants missing"
assert_rg 'OPENFEATURE_DECISION_RECEIPT_EVENT_TYPE' "$CLASSIFIERS" "openfeature receipt constants missing"
assert_rg 'CYCLONEDX_MLBOM_MODEL_RECEIPT_EVENT_TYPE' "$CLASSIFIERS" "cyclonedx receipt constants missing"

facade_non_test_loc=$(awk '/#\[cfg\(test\)\]/{exit} {count++} END{print count}' "$TRUST_BASIS")
if [ "$facade_non_test_loc" -gt 45 ]; then
  echo "FAIL: trust_basis facade non-test LOC drifted above Step10 ceiling ($facade_non_test_loc > 45)"
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
