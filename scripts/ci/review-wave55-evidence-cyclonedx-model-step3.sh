#!/usr/bin/env bash
set -euo pipefail

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"

base_ref="${BASE_REF:-origin/main}"
if ! git rev-parse --verify "$base_ref" >/dev/null 2>&1; then
  echo "FAIL: cannot resolve Wave55 Step3 base ref: $base_ref"
  echo "Set BASE_REF to the main ref used for this review."
  exit 1
fi

base_changed="$(git diff --name-only "$base_ref"...HEAD)"
worktree_changed="$(
  {
    git diff --name-only
    git diff --cached --name-only
    git ls-files --others --exclude-standard
  } | sort -u
)"
changed="$(printf '%s\n%s\n' "$base_changed" "$worktree_changed" | sed '/^$/d' | sort -u)"

allowed_pattern='^(docs/contributing/SPLIT-PLAN-wave55-evidence-contract-schema\.md|docs/contributing/SPLIT-CHECKLIST-wave55-evidence-cyclonedx-model-step3\.md|docs/contributing/SPLIT-MOVE-MAP-wave55-evidence-cyclonedx-model-step3\.md|docs/contributing/SPLIT-REVIEW-PACK-wave55-evidence-cyclonedx-model-step3\.md|scripts/ci/review-wave55-evidence-cyclonedx-model-step3\.sh|crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model\.rs|crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/(constants|events|reduce|source|validate|tests)\.rs)$'
unexpected="$(printf '%s\n' "$changed" | rg -v "$allowed_pattern" || true)"
if [[ -n "$unexpected" ]]; then
  echo "FAIL: Wave55 Step3 changed files outside the allowlist:"
  printf '%s\n' "$unexpected"
  exit 1
fi

for forbidden in \
  '^\.github/workflows/' \
  '^crates/assay-cli/receipt-schemas/' \
  '^docs/reference/receipt-schemas/' \
  '^crates/assay-cli/src/cli/commands/evidence/(pydantic_case_result|mastra_score_event)\.rs$' \
  '^crates/assay-cli/src/cli/commands/evidence/pydantic_case_result/' \
  '^crates/assay-cli/src/cli/commands/evidence/schema/'
do
  if printf '%s\n' "$changed" | rg "$forbidden" >/dev/null; then
    echo "FAIL: forbidden Wave55 Step3 path matched: $forbidden"
    exit 1
  fi
done

required=(
  "docs/contributing/SPLIT-PLAN-wave55-evidence-contract-schema.md"
  "docs/contributing/SPLIT-CHECKLIST-wave55-evidence-cyclonedx-model-step3.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave55-evidence-cyclonedx-model-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave55-evidence-cyclonedx-model-step3.md"
  "scripts/ci/review-wave55-evidence-cyclonedx-model-step3.sh"
  "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model.rs"
  "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/constants.rs"
  "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/events.rs"
  "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/reduce.rs"
  "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/source.rs"
  "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/validate.rs"
  "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/tests.rs"
)

for path in "${required[@]}"; do
  test -f "$path" || {
    echo "FAIL: missing required file: $path"
    exit 1
  }
done

require_marker() {
  local pattern="$1"
  local path="$2"
  local message="$3"
  if ! rg -q "$pattern" "$path"; then
    echo "FAIL: $message"
    exit 1
  fi
}

forbid_marker() {
  local pattern="$1"
  local path="$2"
  local message="$3"
  if rg -q "$pattern" "$path"; then
    echo "FAIL: $message"
    exit 1
  fi
}

facade="crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model.rs"
for module in constants events reduce source validate; do
  require_marker "^mod ${module};$" "$facade" "CycloneDX facade must declare ${module} module"
done
require_marker '^pub struct CycloneDxMlBomModelArgs\b' "$facade" "CycloneDX facade must keep CLI args"
require_marker '^pub fn cmd_cyclonedx_mlbom_model\b' "$facade" "CycloneDX facade must keep command entrypoint"
forbid_marker '^const EVENT_TYPE\b|^fn read_cyclonedx_model_event\b|^fn reduce_model_component\b|^fn select_component\b|^fn parse_import_time\b|^fn sha256_file\b|^fn bounded_string\b' "$facade" "CycloneDX facade must not own moved constants/helpers"

require_marker '^pub\(super\) const EVENT_TYPE\b' "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/constants.rs" "constants module must own event constants"
require_marker '^pub\(super\) const DEFAULT_RUN_ID\b' "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/constants.rs" "constants module must own default run id"
require_marker '^pub\(super\) fn read_cyclonedx_model_event\b' "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/events.rs" "events module must own event construction"
require_marker '^pub\(super\) fn reduce_model_component\b' "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/reduce.rs" "reduce module must own payload reduction"
require_marker '^fn select_component\b' "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/reduce.rs" "reduce module must own component selection"
require_marker '^pub\(super\) fn parse_import_time\b' "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/source.rs" "source module must own import time parsing"
require_marker '^pub\(super\) fn read_json_file\b' "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/source.rs" "source module must own JSON reading"
require_marker '^pub\(super\) fn sha256_file\b' "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/source.rs" "source module must own artifact digesting"
require_marker '^pub\(super\) fn bounded_string\b' "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/validate.rs" "validate module must own bounded string validation"
require_marker '^fn import_writes_verifiable_model_component_bundle_without_bodies\b' "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/tests.rs" "tests module must retain importer behavior tests"

check_loc_max() {
  local path="$1"
  local max="$2"
  local loc
  loc="$(wc -l < "$path" | tr -d ' ')"
  if (( loc > max )); then
    echo "FAIL: $path has $loc LOC, expected <= $max"
    exit 1
  fi
}

check_loc_max "$facade" 100
check_loc_max "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/constants.rs" 40
check_loc_max "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/events.rs" 60
check_loc_max "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/reduce.rs" 220
check_loc_max "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/source.rs" 60
check_loc_max "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/validate.rs" 80
check_loc_max "crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model/tests.rs" 240

cargo fmt --check
cargo check -p assay-cli
cargo test -q -p assay-cli cyclonedx_mlbom_model
cargo test -q -p assay-cli --test evidence_test importer_receipts::test_cyclonedx_mlbom_model_receipts_verify_and_feed_trust_basis_generation
cargo test -q -p assay-cli --test evidence_test importer_receipts::test_cyclonedx_mlbom_formulation_fixture_stays_inventory_only
cargo test -q -p assay-cli --test receipt_schema_registry_test schema_family_paths::cyclonedx_input_and_receipt_schemas_validate_supported_path
cargo clippy -p assay-cli --all-targets -- -D warnings
git diff --check "$base_ref"...HEAD
git diff --check
git diff --cached --check

echo "PASS: Wave55 Step3 CycloneDX model importer split gate"
