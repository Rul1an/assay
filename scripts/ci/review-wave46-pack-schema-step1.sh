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
  "docs/contributing/SPLIT-PLAN-wave46-pack-schema.md"
  "docs/contributing/SPLIT-CHECKLIST-wave46-pack-schema-step1.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave46-pack-schema-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave46-pack-schema-step1.md"
  "scripts/ci/review-wave46-pack-schema-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF (workflow-ban)"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave46 Step1 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave46 Step1: $f"
    exit 1
  fi
done < <(changed_files)

for frozen in 'crates/assay-evidence/src/lint/packs/**' 'crates/assay-evidence/tests/**' 'packs/open/**'; do
  if git diff --name-only "$BASE_REF"...HEAD -- "$frozen" | rg -n '.' >/dev/null; then
    echo "FAIL: Wave46 Step1 must not change frozen path: $frozen"
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
PLAN="docs/contributing/SPLIT-PLAN-wave46-pack-schema.md"
MOVE_MAP="docs/contributing/SPLIT-MOVE-MAP-wave46-pack-schema-step1.md"

for marker in \
  'schema.rs` Kernel Split' \
  'checks.rs` is explicitly **out of scope** for this wave' \
  'json_path_exists` and `value_equals` validation rules' \
  'conditional-shape acceptance vs unsupported classification' \
  'built-in/open pack loadability and parity assumptions'
do
  rg -F -n "$marker" "$PLAN" "$MOVE_MAP" >/dev/null || {
    echo "FAIL: missing marker: $marker"
    exit 1
  }
done

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-evidence --all-targets -- -D warnings

echo "[review] pinned schema invariants"
cargo test -q -p assay-evidence --lib 'lint::packs::loader::loader_internal::tests::test_is_valid_pack_name' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::loader::loader_internal::tests::test_builtin_wins_over_local' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::loader::loader_internal::tests::test_local_invalid_yaml_fails' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::loader::loader_internal::tests::test_path_wins_over_builtin' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_supported_conditional_shape_parses' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_conditional_with_multiple_then_paths_is_unsupported' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_conditional_validation_requires_condition_object' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_conditional_validation_requires_then_object' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_json_path_exists_value_equals_requires_exactly_one_path' -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test conditional_rule_fails_when_matching_event_lacks_required_path -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test json_path_exists_respects_event_types_filter -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test unsupported_conditional_shape_still_skips_for_security_pack -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test unsupported_conditional_shape_fails_for_compliance_pack -- --exact
cargo test -q -p assay-evidence --test a2a_discovery_card_followup_pack a2a_discovery_builtin_and_open_pack_are_exactly_equivalent -- --exact
cargo test -q -p assay-evidence --test mcp_signal_followup_pack mcp_followup_builtin_and_open_pack_are_exactly_equivalent -- --exact

echo "[review] PASS"
