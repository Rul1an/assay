#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

base_ref="${BASE_REF:-${1:-}}"
if [[ -z "$base_ref" ]]; then
  if [[ -n "${GITHUB_BASE_REF:-}" ]]; then
    base_ref="origin/${GITHUB_BASE_REF}"
  else
    base_ref="origin/main"
  fi
fi

if ! git rev-parse --verify --quiet "${base_ref}^{commit}" >/dev/null; then
  echo "BASE_REF not found: ${base_ref}" >&2
  exit 1
fi

rg_bin="$(command -v rg || true)"
if [[ -z "$rg_bin" ]]; then
  echo "rg is required for reviewer anchors" >&2
  exit 1
fi

while IFS= read -r file; do
  [[ -n "$file" ]] || continue
  case "$file" in
    crates/assay-evidence/src/lib.rs|\
    crates/assay-evidence/src/trust_basis.rs|\
    crates/assay-cli/src/cli/args/mod.rs|\
    crates/assay-cli/src/cli/args/trust_basis.rs|\
    crates/assay-cli/src/cli/commands/mod.rs|\
    crates/assay-cli/src/cli/commands/dispatch.rs|\
    crates/assay-cli/src/cli/commands/trust_basis.rs|\
    crates/assay-cli/tests/trust_basis_test.rs|\
    docs/contributing/SPLIT-CHECKLIST-wave-t1a-trust-basis-step1.md|\
    docs/contributing/SPLIT-MOVE-MAP-wave-t1a-trust-basis-step1.md|\
    docs/contributing/SPLIT-REVIEW-PACK-wave-t1a-trust-basis-step1.md|\
    scripts/ci/review-wave-t1a-trust-basis-step1.sh)
      ;;
    *)
      echo "ERROR: out-of-scope file changed for T1a step1: $file" >&2
      exit 1
      ;;
  esac
done < <(git diff --name-only "${base_ref}...HEAD")

trust_basis_file="crates/assay-evidence/src/trust_basis.rs"
cli_file="crates/assay-cli/src/cli/commands/trust_basis.rs"
test_file="crates/assay-cli/tests/trust_basis_test.rs"

for required_pattern in \
  'BundleVerified' \
  'SigningEvidencePresent' \
  'ProvenanceBackedClaimsPresent' \
  'DelegationContextVisible' \
  'ContainmentDegradationObserved' \
  'AppliedPackFindingsPresent'; do
  "$rg_bin" -n -F "$required_pattern" "$trust_basis_file" >/dev/null
done

for required_pattern in \
  'BundleVerification' \
  'BundleProofSurface' \
  'CanonicalDecisionEvidence' \
  'CanonicalEventPresence' \
  'PackExecutionResults'; do
  "$rg_bin" -n -F "$required_pattern" "$trust_basis_file" >/dev/null
done

for required_pattern in \
  'BundleWide' \
  'SupportedDelegatedFlowsOnly' \
  'SupportedContainmentFallbackPathsOnly' \
  'ProofSurfacesOnly' \
  'PackExecutionOnly'; do
  "$rg_bin" -n -F "$required_pattern" "$trust_basis_file" >/dev/null
done

for forbidden_pattern in \
  'trustcard.json' \
  'trustcard.md' \
  'trusted/untrusted' \
  'safe/unsafe' \
  'trust score' \
  'maturity badge'; do
  if grep -R -n -i -F "$forbidden_pattern" "$trust_basis_file" "$cli_file" "$test_file" >/dev/null; then
    echo "ERROR: forbidden Trust Card or score-first text present: $forbidden_pattern" >&2
    exit 1
  fi
done

"$rg_bin" -n -F 'fn trust_basis_regeneration_is_byte_stable(' "$trust_basis_file" >/dev/null
"$rg_bin" -n -F 'fn trust_basis_keeps_signing_and_provenance_absent_despite_tempting_metadata(' "$trust_basis_file" >/dev/null
"$rg_bin" -n -F 'fn trust_basis_generate_is_byte_stable_and_pack_aware(' "$test_file" >/dev/null

cargo fmt --check
cargo clippy -q -p assay-evidence -p assay-cli --all-targets -- -D warnings
cargo test -q -p assay-evidence trust_basis_
cargo test -q -p assay-cli --test trust_basis_test
git diff --check

echo "Wave T1a Step1 reviewer script: PASS"
