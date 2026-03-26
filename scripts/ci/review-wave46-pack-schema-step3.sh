#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave46-pack-schema.md"
  "docs/contributing/SPLIT-CHECKLIST-wave46-pack-schema-step3.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave46-pack-schema-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave46-pack-schema-step3.md"
  "scripts/ci/review-wave46-pack-schema-step3.sh"
)

FROZEN_PATHS=(
  "crates/assay-evidence/src/lint/packs"
  "crates/assay-evidence/tests"
  "packs/open"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r file; do
  [[ -z "${file:-}" ]] && continue

  if [[ "$file" == .github/workflows/* ]]; then
    echo "FAIL: Wave46 Step3 must not touch workflows ($file)"
    exit 1
  fi

  ok="false"
  for allowed in "${ALLOWLIST[@]}"; do
    [[ "$file" == "$allowed" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave46 Step3: $file"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] frozen tracked paths must not change"
for path in "${FROZEN_PATHS[@]}"; do
  if git diff --name-only "$BASE_REF"...HEAD -- "$path" | rg -n '.' >/dev/null; then
    echo "FAIL: Wave46 Step3 must not change frozen path: $path"
    git diff --name-only "$BASE_REF"...HEAD -- "$path"
    exit 1
  fi
done

echo "[review] frozen paths must not contain untracked files"
for path in "${FROZEN_PATHS[@]}"; do
  if git ls-files --others --exclude-standard -- "$path" | rg -n '.' >/dev/null; then
    echo "FAIL: untracked files present under frozen path: $path"
    git ls-files --others --exclude-standard -- "$path" | sed 's/^/  - /'
    exit 1
  fi
done

echo "[review] marker checks"
PLAN="docs/contributing/SPLIT-PLAN-wave46-pack-schema.md"
MOVE_MAP="docs/contributing/SPLIT-MOVE-MAP-wave46-pack-schema-step3.md"

for marker in \
  'Wave46 Step2 shipped on `main` via `#964`.' \
  'keeps `schema.rs` as the stable facade entrypoint' \
  'Step3 constraints' \
  'no new module cuts' \
  'no behavior cleanup beyond internal follow-up notes'
do
  rg -F -n "$marker" "$PLAN" >/dev/null || {
    echo "FAIL: missing marker in plan: $marker"
    exit 1
  }
done

for marker in \
  'crates/assay-evidence/src/lint/packs/schema.rs' \
  'crates/assay-evidence/src/lint/packs/schema_next/conditional.rs' \
  'crates/assay-evidence/src/lint/packs/schema_next/errors.rs' \
  'internal visibility tightening only if it requires no code edits in this wave' \
  'validation error or reason-meaning changes'
do
  rg -F -n "$marker" "$MOVE_MAP" >/dev/null || {
    echo "FAIL: missing marker in move-map: $marker"
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
