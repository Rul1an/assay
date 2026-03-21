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

rg_bin="$(command -v rg)"
if [[ -z "$rg_bin" ]]; then
  echo "rg is required for reviewer anchors" >&2
  exit 1
fi

open_pack="packs/open/owasp-agentic-control-evidence-baseline/pack.yaml"
builtin_pack="crates/assay-evidence/packs/owasp-agentic-control-evidence-baseline.yaml"
readme="packs/open/owasp-agentic-control-evidence-baseline/README.md"
test_file="crates/assay-evidence/tests/owasp_agentic_c2_pack.rs"

while IFS= read -r file; do
  [[ -n "$file" ]] || continue
  case "$file" in
    packs/open/owasp-agentic-control-evidence-baseline/pack.yaml|\
    packs/open/owasp-agentic-control-evidence-baseline/README.md|\
    packs/open/owasp-agentic-control-evidence-baseline/LICENSE|\
    crates/assay-evidence/packs/owasp-agentic-control-evidence-baseline.yaml|\
    crates/assay-evidence/src/lint/packs/mod.rs|\
    crates/assay-evidence/tests/owasp_agentic_c2_pack.rs|\
    docs/contributing/SPLIT-INVENTORY-wave-c2-owasp-agentic-subset-step1.md|\
    docs/contributing/SPLIT-CHECKLIST-wave-c2-owasp-agentic-subset-step1.md|\
    docs/contributing/SPLIT-MOVE-MAP-wave-c2-owasp-agentic-subset-step1.md|\
    docs/contributing/SPLIT-REVIEW-PACK-wave-c2-owasp-agentic-subset-step1.md|\
    scripts/ci/review-wave-c2-owasp-agentic-subset-step1.sh)
      ;;
    crates/assay-evidence/src/lint/packs/checks.rs|\
    crates/assay-evidence/src/lint/packs/schema.rs|\
    crates/assay-evidence/src/lint/packs/executor.rs|\
    crates/assay-evidence/src/lint/engine.rs|\
    packs/open/*|crates/assay-evidence/packs/*)
      echo "ERROR: forbidden file changed for C2 scope: $file" >&2
      exit 1
      ;;
    *)
      echo "ERROR: out-of-scope file changed: $file" >&2
      exit 1
      ;;
  esac
done < <(git diff --name-only "${base_ref}...HEAD")

cmp -s "$open_pack" "$builtin_pack" || {
  echo "ERROR: open pack and built-in mirror differ" >&2
  exit 1
}

rule_count="$("$rg_bin" -c '^  - id:' "$open_pack")"
[[ "$rule_count" == "3" ]] || {
  echo "ERROR: expected exactly 3 shipped rules, found $rule_count" >&2
  exit 1
}

for rule_id in A1-002 A3-001 A5-001; do
  "$rg_bin" -n "^  - id: ${rule_id}$" "$open_pack" >/dev/null
done

for forbidden_rule in A1-001 A3-002 A3-003 A5-002; do
  if "$rg_bin" -n "^  - id: ${forbidden_rule}$" "$open_pack" >/dev/null; then
    echo "ERROR: forbidden rule shipped in C2: ${forbidden_rule}" >&2
    exit 1
  fi
done

for forbidden_token in conditional engine_min_version mandate_id delegated_from actor_chain delegation_depth inherited_scopes; do
  if "$rg_bin" -n -F "$forbidden_token" "$open_pack" >/dev/null; then
    echo "ERROR: forbidden token present in shipped pack: ${forbidden_token}" >&2
    exit 1
  fi
done

lower_yaml="$(tr '[:upper:]' '[:lower:]' < "$open_pack")"
for forbidden_phrase in \
  "detects goal hijack" \
  "verifies privilege abuse" \
  "privilege abuse prevention" \
  "proves sandboxing" \
  "sandbox degradation protection" \
  "mandate linkage enforcement" \
  "temporal validity enforcement"; do
  if grep -Fq "$forbidden_phrase" <<<"$lower_yaml"; then
    echo "ERROR: overclaim phrase present in pack yaml: $forbidden_phrase" >&2
    exit 1
  fi
done

"$rg_bin" -n '^## Non-Goals$' "$readme" >/dev/null
for required_phrase in \
  "goal hijack detection" \
  "privilege abuse prevention" \
  "mandate linkage enforcement" \
  "temporal validity of approvals or mandates" \
  "delegation-chain visibility" \
  "sandbox degradation detection"; do
  count="$(grep -Fic "$required_phrase" "$readme")"
  [[ "$count" == "1" ]] || {
    echo "ERROR: README must contain non-goal phrase exactly once: $required_phrase" >&2
    exit 1
  }
done

normalized_readme="$(tr '\n' ' ' < "$readme" | tr -s '[:space:]' ' ' | tr '[:upper:]' '[:lower:]')"
grep -Fq "this pack proves only that process-execution evidence is present in the baseline flow; it does not prove execution authorization, containment, or sandboxing." <<<"$normalized_readme" || {
  echo "ERROR: README missing process execution guardrail" >&2
  exit 1
}

"$rg_bin" -n '^\| `A1-002` \| `Field Presence` \| `Yes` \| Can ship only as control evidence for goal governance fields\. \|$' docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md >/dev/null
"$rg_bin" -n '^\| `A3-001` \| `Field Presence` \| `Yes` \| Can ship only as authorization-context capture evidence\. \|$' docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md >/dev/null
"$rg_bin" -n '^\| `A5-001` \| `Presence` \| `Yes` \| Can ship only as process-execution evidence presence\. \|$' docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md >/dev/null

"$rg_bin" -n -F 'fn c2_builtin_and_open_pack_are_exactly_equivalent(' "$test_file" >/dev/null
"$rg_bin" -n -F 'fn c2_pack_contains_no_skip_prone_checks(' "$test_file" >/dev/null
"$rg_bin" -n -F 'fn c2_readme_explicitly_states_non_goals(' "$test_file" >/dev/null
"$rg_bin" -n -F 'fn c2_pack_wording_stays_control_evidence_only(' "$test_file" >/dev/null

cargo fmt --check
cargo clippy -q -p assay-evidence --all-targets -- -D warnings
cargo test -q -p assay-evidence --test owasp_agentic_c2_pack
cargo test -q -p assay-evidence --test owasp_agentic_c1_mapping
cargo test -q -p assay-evidence --test pack_engine_manual_test
git diff --check
