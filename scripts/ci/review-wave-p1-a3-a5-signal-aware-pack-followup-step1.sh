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

open_pack="packs/open/owasp-agentic-a3-a5-signal-followup/pack.yaml"
builtin_pack="crates/assay-evidence/packs/owasp-agentic-a3-a5-signal-followup.yaml"
readme="packs/open/owasp-agentic-a3-a5-signal-followup/README.md"
test_file="crates/assay-evidence/tests/owasp_agentic_p1_signal_followup.rs"
a3_probe="crates/assay-evidence/tests/fixtures/packs/owasp-agentic-a3-probe.yaml"
c1_doc="docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md"

require_fixed_pattern() {
  local pattern="$1"
  local file="$2"
  "$rg_bin" -n -F "$pattern" "$file" >/dev/null
}

while IFS= read -r file; do
  [[ -n "$file" ]] || continue
  case "$file" in
    crates/assay-evidence/packs/owasp-agentic-a3-a5-signal-followup.yaml|\
    packs/open/owasp-agentic-a3-a5-signal-followup/pack.yaml|\
    packs/open/owasp-agentic-a3-a5-signal-followup/README.md|\
    packs/open/owasp-agentic-a3-a5-signal-followup/LICENSE|\
    crates/assay-evidence/src/lint/packs/mod.rs|\
    crates/assay-evidence/tests/owasp_agentic_p1_signal_followup.rs|\
    crates/assay-evidence/tests/owasp_agentic_c1_mapping.rs|\
    crates/assay-evidence/tests/fixtures/packs/owasp-agentic-a3-probe.yaml|\
    docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md|\
    docs/contributing/SPLIT-INVENTORY-wave-p1-a3-a5-signal-aware-pack-followup-step1.md|\
    docs/contributing/SPLIT-CHECKLIST-wave-p1-a3-a5-signal-aware-pack-followup-step1.md|\
    docs/contributing/SPLIT-MOVE-MAP-wave-p1-a3-a5-signal-aware-pack-followup-step1.md|\
    docs/contributing/SPLIT-REVIEW-PACK-wave-p1-a3-a5-signal-aware-pack-followup-step1.md|\
    scripts/ci/review-wave-p1-a3-a5-signal-aware-pack-followup-step1.sh)
      ;;
    crates/assay-evidence/packs/*|\
    packs/open/*|\
    crates/assay-evidence/src/*|\
    crates/assay-evidence/tests/*|\
    docs/security/*)
      echo "ERROR: forbidden file changed for P1 scope: $file" >&2
      exit 1
      ;;
    *)
      echo "ERROR: out-of-scope file changed: $file" >&2
      exit 1
      ;;
  esac
done < <(git diff --name-only "${base_ref}...HEAD")

if git diff --name-only "${base_ref}...HEAD" | grep -Eq \
  '(^crates/assay-evidence/packs/owasp-agentic-control-evidence-baseline.yaml$|^packs/open/owasp-agentic-control-evidence-baseline/)'; then
  echo "ERROR: baseline pack must remain unchanged in P1" >&2
  exit 1
fi

cmp -s "$open_pack" "$builtin_pack" || {
  echo "ERROR: open pack and built-in mirror differ" >&2
  exit 1
}

rule_count="$("$rg_bin" -c '^  - id:' "$open_pack")"
[[ "$rule_count" == "2" ]] || {
  echo "ERROR: expected exactly 2 shipped rules, found $rule_count" >&2
  exit 1
}

for rule_id in A3-003 A5-002; do
  "$rg_bin" -n "^  - id: ${rule_id}$" "$open_pack" >/dev/null
done

if "$rg_bin" -n '^  - id: (A1-002|A3-001|A3-002|A5-001)$' "$open_pack" >/dev/null; then
  echo "ERROR: companion pack must ship only A3-003 and A5-002" >&2
  exit 1
fi

require_fixed_pattern 'description: Decision evidence surfaces delegated authority context for supported delegated flows.' "$open_pack"
require_fixed_pattern 'description: Evidence records supported containment degradation fallback paths.' "$open_pack"
require_fixed_pattern 'pattern: assay.sandbox.degraded' "$open_pack"
require_fixed_pattern 'type: event_type_exists' "$open_pack"
require_fixed_pattern 'type: event_field_present' "$open_pack"

grep -Eq '^[[:space:]]*-[[:space:]]+/data/delegated_from$' "$open_pack" || {
  echo "ERROR: A3-003 must require delegated_from" >&2
  exit 1
}

for forbidden_path in /data/delegation_depth /data/actor_chain /data/inherited_scopes; do
  if grep -Eq "^[[:space:]]*-[[:space:]]+${forbidden_path}$" "$open_pack" "$a3_probe"; then
    echo "ERROR: forbidden A3 signal path required: ${forbidden_path}" >&2
    exit 1
  fi
done

grep -Fq 'supported delegated flows' "$readme" || {
  echo "ERROR: README must say supported delegated flows" >&2
  exit 1
}

"$rg_bin" -n '^## Non-Goals$' "$readme" >/dev/null
for required_phrase in \
  "delegation chain integrity" \
  "delegation validity" \
  "inherited-scope correctness" \
  "temporal delegation correctness" \
  "sandbox correctness" \
  "all containment failures detected"; do
  count="$(grep -Fic "$required_phrase" "$readme")"
  [[ "$count" == "1" ]] || {
    echo "ERROR: README must contain non-goal phrase exactly once: $required_phrase" >&2
    exit 1
  }
done

lower_text="$(
  cat "$open_pack" "$readme" "$c1_doc" \
    | tr '[:upper:]' '[:lower:]'
)"
normalized_c1_doc="$(tr '\n' ' ' < "$c1_doc" | tr -s '[:space:]' ' ')"

for forbidden_phrase in \
  "verifies delegation" \
  "guarantees chain integrity" \
  "validates inherited scopes" \
  "proves sandboxing" \
  "cryptographically verified delegation"; do
  if grep -Fq "$forbidden_phrase" <<<"$lower_text"; then
    echo "ERROR: overclaim phrase present in P1 text: $forbidden_phrase" >&2
    exit 1
  fi
done

require_fixed_pattern 'event_field_present(paths_any_of=/data/delegated_from)' "$c1_doc"
grep -Fq 'The companion pack `owasp-agentic-a3-a5-signal-followup` ships this rule only for supported delegated flows.' <<<"$normalized_c1_doc" || {
  echo "ERROR: C1 doc missing A3 companion-pack truth" >&2
  exit 1
}
grep -Fq 'The companion pack `owasp-agentic-a3-a5-signal-followup` ships this rule only in that presence-only form.' <<<"$normalized_c1_doc" || {
  echo "ERROR: C1 doc missing A5 companion-pack truth" >&2
  exit 1
}
require_fixed_pattern '| `A3-003` | `Field Presence` | `No` | Shipped in the signal-aware companion pack for supported delegated flows; it does not validate chain completeness or integrity. |' "$c1_doc"
require_fixed_pattern '| `A5-002` | `Presence` | `No` | Shipped in the signal-aware companion pack for supported fallback paths; it only proves degraded containment while execution continued. |' "$c1_doc"

for required_test in \
  'fn p1_builtin_and_open_pack_are_exactly_equivalent(' \
  'fn p1_baseline_pack_remains_unchanged(' \
  'fn p1_readme_explicitly_states_non_goals(' \
  'fn p1_a3_003_passes_when_supported_delegation_fields_are_present(' \
  'fn p1_a3_003_fails_when_supported_delegation_fields_are_absent(' \
  'fn p1_a5_002_passes_when_sandbox_degraded_event_is_present(' \
  'fn p1_a5_002_fails_when_supported_degradation_signal_is_absent('; do
  "$rg_bin" -n -F "$required_test" "$test_file" >/dev/null
done

cargo fmt --check
cargo clippy -q -p assay-evidence --all-targets -- -D warnings
cargo test -q -p assay-evidence --test owasp_agentic_p1_signal_followup
cargo test -q -p assay-evidence --test owasp_agentic_c1_mapping
cargo test -q -p assay-evidence --test owasp_agentic_c2_pack
cargo test -q -p assay-evidence --test pack_engine_manual_test
git diff --check

echo "Wave P1 Step1 reviewer script: PASS"
