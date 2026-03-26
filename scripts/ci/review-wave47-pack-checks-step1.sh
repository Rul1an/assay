#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

changed_files() {
  {
    git diff --name-only "$BASE_REF"...HEAD
    git diff --name-only
    git diff --cached --name-only
    git ls-files --others --exclude-standard
  } | awk 'NF' | sort -u
}

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave47-pack-checks.md"
  "docs/contributing/SPLIT-CHECKLIST-wave47-pack-checks-step1.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave47-pack-checks-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave47-pack-checks-step1.md"
  "scripts/ci/review-wave47-pack-checks-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF (workflow-ban)"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave47 Step1 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave47 Step1: $f"
    exit 1
  fi
done < <(changed_files)

for frozen in 'crates/assay-evidence/src/lint/packs/**' 'crates/assay-evidence/tests/**' 'packs/open/**'; do
  if git diff --name-only "$BASE_REF"...HEAD -- "$frozen" | rg -n '.' >/dev/null; then
    echo "FAIL: Wave47 Step1 must not change frozen path: $frozen"
    git diff --name-only "$BASE_REF"...HEAD -- "$frozen"
    exit 1
  fi
  if git ls-files --others --exclude-standard -- "$frozen" | rg -n '.' >/dev/null; then
    echo "FAIL: untracked files present under frozen path: $frozen"
    git ls-files --others --exclude-standard -- "$frozen" | sed 's/^/  - /'
    exit 1
  fi
done

echo "[review] marker checks"
PLAN="docs/contributing/SPLIT-PLAN-wave47-pack-checks.md"
MOVE_MAP="docs/contributing/SPLIT-MOVE-MAP-wave47-pack-checks-step1.md"

for marker in \
  'checks.rs` Kernel Split' \
  'schema.rs` and `schema_next/*` are already shipped from Wave46 and are explicitly out of scope' \
  'single-path invariant for `json_path_exists.value_equals`' \
  'finding emission semantics (canonical rule id, severity, message meaning, fingerprint/pack metadata coupling)' \
  'No new check types, engine bump, spec expansion, or dispatch redesign.'
do
  rg -F -n "$marker" "$PLAN" >/dev/null || {
    echo "FAIL: missing marker in plan: $marker"
    exit 1
  }
done

for marker in \
  'checks_next/event.rs' \
  'checks_next/json_path.rs' \
  'checks_next/conditional.rs' \
  'checks_next/finding.rs' \
  'built-in/open pack parity and pack-lint baseline semantics remain identical'
do
  rg -F -n "$marker" "$MOVE_MAP" >/dev/null || {
    echo "FAIL: missing marker in move-map: $marker"
    exit 1
  }
done

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-evidence --all-targets -- -D warnings

echo "[review] pinned checks invariants"
cargo test -q -p assay-evidence --lib 'lint::packs::checks::tests::g3_authorization_check_uses_scoped_events_not_full_bundle' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::checks::tests::test_value_pointer' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::checks::tests::test_glob_matching' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_json_path_exists_value_equals_requires_exactly_one_path' -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test conditional_rule_fails_when_matching_event_lacks_required_path -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test event_field_present_respects_event_types_filter -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test json_path_exists_respects_event_types_filter -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test unsupported_conditional_shape_still_skips_for_security_pack -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test unsupported_conditional_shape_fails_for_compliance_pack -- --exact
cargo test -q -p assay-evidence --test mcp_signal_followup_pack mcp001_aligns_trust_basis_verified_and_pack_passes -- --exact
cargo test -q -p assay-evidence --test mcp_signal_followup_pack mcp001_aligns_trust_basis_absent_and_pack_fails -- --exact
cargo test -q -p assay-evidence --test mcp_signal_followup_pack mcp_followup_builtin_and_open_pack_are_exactly_equivalent -- --exact
cargo test -q -p assay-evidence --test a2a_discovery_card_followup_pack a2a_discovery_builtin_and_open_pack_are_exactly_equivalent -- --exact
cargo test -q -p assay-evidence --test a2a_discovery_card_followup_pack a2a_dc_001_fails_when_agent_card_visible_is_string_not_bool -- --exact
cargo test -q -p assay-evidence --test owasp_agentic_c1_mapping a3_conditional_presence_rule_is_supported_in_engine_v1_1 -- --exact
cargo test -q -p assay-evidence --test owasp_agentic_c1_mapping a3_conditional_presence_rule_fails_without_mandate_context -- --exact

echo "[review] PASS"
