#!/usr/bin/env bash
set -euo pipefail

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"

base_ref="${BASE_REF:-origin/main}"
if ! git rev-parse --verify "$base_ref" >/dev/null 2>&1; then
  echo "FAIL: cannot resolve Wave55 Step1 base ref: $base_ref"
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

allowed_pattern='^(docs/contributing/SPLIT-PLAN-wave55-evidence-contract-schema\.md|docs/contributing/SPLIT-CHECKLIST-wave55-evidence-contract-schema-step1\.md|docs/contributing/SPLIT-MOVE-MAP-wave55-evidence-contract-schema-step1\.md|docs/contributing/SPLIT-REVIEW-PACK-wave55-evidence-contract-schema-step1\.md|scripts/ci/review-wave55-evidence-contract-schema-step1\.sh|crates/assay-cli/src/cli/commands/evidence/schema\.rs|crates/assay-cli/src/cli/commands/evidence/schema/(registry|reports|validate|write)\.rs)$'
unexpected="$(printf '%s\n' "$changed" | rg -v "$allowed_pattern" || true)"
if [[ -n "$unexpected" ]]; then
  echo "FAIL: Wave55 Step1 changed files outside the allowlist:"
  printf '%s\n' "$unexpected"
  exit 1
fi

for forbidden in \
  '^\.github/workflows/' \
  '^crates/assay-cli/receipt-schemas/' \
  '^docs/reference/receipt-schemas/' \
  '^crates/assay-cli/src/cli/commands/evidence/(pydantic_case_result|cyclonedx_mlbom_model|mastra_score_event)\.rs$'
do
  if printf '%s\n' "$changed" | rg "$forbidden" >/dev/null; then
    echo "FAIL: forbidden Wave55 Step1 path matched: $forbidden"
    exit 1
  fi
done

required=(
  "docs/contributing/SPLIT-PLAN-wave55-evidence-contract-schema.md"
  "docs/contributing/SPLIT-CHECKLIST-wave55-evidence-contract-schema-step1.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave55-evidence-contract-schema-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave55-evidence-contract-schema-step1.md"
  "scripts/ci/review-wave55-evidence-contract-schema-step1.sh"
  "crates/assay-cli/src/cli/commands/evidence/schema.rs"
  "crates/assay-cli/src/cli/commands/evidence/schema/registry.rs"
  "crates/assay-cli/src/cli/commands/evidence/schema/reports.rs"
  "crates/assay-cli/src/cli/commands/evidence/schema/validate.rs"
  "crates/assay-cli/src/cli/commands/evidence/schema/write.rs"
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

for module in registry reports validate write; do
  require_marker "^mod ${module};$" "crates/assay-cli/src/cli/commands/evidence/schema.rs" "schema facade must declare ${module} module"
done
require_marker '^pub fn cmd_schema\b' "crates/assay-cli/src/cli/commands/evidence/schema.rs" "schema facade must keep cmd_schema entrypoint"
require_marker '^pub struct SchemaArgs\b' "crates/assay-cli/src/cli/commands/evidence/schema.rs" "schema facade must keep CLI args"
require_marker '^pub enum SchemaCmd\b' "crates/assay-cli/src/cli/commands/evidence/schema.rs" "schema facade must keep CLI subcommands"
forbid_marker '^const SCHEMAS\b|^struct SchemaDescriptor\b|^fn validate_input\b|^fn collect_validation_errors\b|^fn write_list_text\b' "crates/assay-cli/src/cli/commands/evidence/schema.rs" "schema facade must not own moved registry/validation/rendering internals"

require_marker '^pub\(super\) const SCHEMAS\b' "crates/assay-cli/src/cli/commands/evidence/schema/registry.rs" "registry module must own SCHEMAS"
require_marker '^pub\(super\) fn find_schema\b' "crates/assay-cli/src/cli/commands/evidence/schema/registry.rs" "registry module must own lookup"
require_marker '^pub\(super\) struct SchemaMetadata\b' "crates/assay-cli/src/cli/commands/evidence/schema/reports.rs" "reports module must own metadata DTO"
require_marker '^pub\(super\) fn validate_input\b' "crates/assay-cli/src/cli/commands/evidence/schema/validate.rs" "validate module must own validation"
require_marker '^pub\(super\) fn write_list_text\b' "crates/assay-cli/src/cli/commands/evidence/schema/write.rs" "write module must own text rendering"
require_marker '^pub\(super\) fn write_json\b' "crates/assay-cli/src/cli/commands/evidence/schema/write.rs" "write module must own JSON rendering"

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

check_loc_max "crates/assay-cli/src/cli/commands/evidence/schema.rs" 160
check_loc_max "crates/assay-cli/src/cli/commands/evidence/schema/registry.rs" 300
check_loc_max "crates/assay-cli/src/cli/commands/evidence/schema/reports.rs" 120
check_loc_max "crates/assay-cli/src/cli/commands/evidence/schema/validate.rs" 120
check_loc_max "crates/assay-cli/src/cli/commands/evidence/schema/write.rs" 120

cargo fmt --check
cargo check -p assay-cli
cargo test -q -p assay-cli --test receipt_schema_registry_test
cargo clippy -p assay-cli --all-targets -- -D warnings
git diff --check "$base_ref"...HEAD
git diff --check
git diff --cached --check

echo "PASS: Wave55 Step1 evidence schema facade split gate"
