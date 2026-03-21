#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null 2>&1

while IFS= read -r file; do
  [[ -n "$file" ]] || continue
  case "$file" in
    docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md|\
    crates/assay-evidence/tests/fixtures/packs/owasp-agentic-a1-probe.yaml|\
    crates/assay-evidence/tests/fixtures/packs/owasp-agentic-a3-probe.yaml|\
    crates/assay-evidence/tests/fixtures/packs/owasp-agentic-a5-probe.yaml|\
    crates/assay-evidence/tests/owasp_agentic_c1_mapping.rs|\
    docs/contributing/SPLIT-INVENTORY-wave-c1-owasp-agentic-mapping-step1.md|\
    docs/contributing/SPLIT-CHECKLIST-wave-c1-owasp-agentic-mapping-step1.md|\
    docs/contributing/SPLIT-MOVE-MAP-wave-c1-owasp-agentic-mapping-step1.md|\
    docs/contributing/SPLIT-REVIEW-PACK-wave-c1-owasp-agentic-mapping-step1.md|\
    scripts/ci/review-wave-c1-owasp-agentic-mapping-step1.sh)
      ;;
    packs/open/*|crates/assay-evidence/packs/*|crates/assay-evidence/src/lint/packs/*|crates/assay-evidence/src/lint/engine.rs)
      echo "ERROR: forbidden file changed for C1 scope: $file" >&2
      exit 1
      ;;
    *)
      echo "ERROR: out-of-scope file changed: $file" >&2
      exit 1
      ;;
  esac
done < <(git diff --name-only "$BASE_REF"...HEAD)

rg -n '^## ASI01 Agent Goal Hijack$' docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md >/dev/null
rg -n '^## ASI03 Identity & Privilege Abuse$' docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md >/dev/null
rg -n '^## ASI05 Unexpected Code Execution$' docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md >/dev/null
rg -n 'Candidate Check' docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md >/dev/null
rg -n 'Evidence Signals' docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md >/dev/null
rg -n 'Max Provable Level' docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md >/dev/null
rg -n 'Outcome' docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md >/dev/null
rg -n 'No-Overclaim Rule For C2' docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md >/dev/null
rg -n 'Ship in C2\\?' docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md >/dev/null

rg -n -F 'fn a1_probe_executes_without_unsupported_checks(' crates/assay-evidence/tests/owasp_agentic_c1_mapping.rs >/dev/null
rg -n -F 'fn a3_signal_gap_requires_fixture_or_evidenceflow_proof(' crates/assay-evidence/tests/owasp_agentic_c1_mapping.rs >/dev/null
rg -n -F 'fn a5_sandbox_rule_is_signal_gap_in_current_baseline_fixture(' crates/assay-evidence/tests/owasp_agentic_c1_mapping.rs >/dev/null
rg -n -F 'fn security_pack_with_unsupported_check_skips_and_blocks_c2_claim(' crates/assay-evidence/tests/owasp_agentic_c1_mapping.rs >/dev/null

cargo fmt --check
cargo clippy -q -p assay-evidence --all-targets -- -D warnings
cargo test -q -p assay-evidence --test owasp_agentic_c1_mapping
cargo test -q -p assay-evidence --test pack_engine_manual_test
git diff --check
