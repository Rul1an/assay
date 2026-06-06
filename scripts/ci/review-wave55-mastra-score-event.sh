#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BASE_REF="${BASE_REF:-origin/main}"
FACADE="crates/assay-cli/src/cli/commands/evidence/mastra_score_event.rs"
MODULE_DIR="crates/assay-cli/src/cli/commands/evidence/mastra_score_event"

allowed_regex='^(crates/assay-cli/src/cli/commands/evidence/mastra_score_event\.rs|crates/assay-cli/src/cli/commands/evidence/mastra_score_event/(constants|events|reduce|source|validate|tests)\.rs|docs/contributing/SPLIT-(CHECKLIST|MOVE-MAP|REVIEW-PACK)-wave55-mastra-score-event\.md|scripts/ci/review-wave55-mastra-score-event\.sh)$'

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

echo "[review] scope allowlist"
changed_files="$(
  {
    git diff --name-only "$BASE_REF" --
    git ls-files --others --exclude-standard -- \
      "$FACADE" \
      "$MODULE_DIR" \
      docs/contributing/SPLIT-CHECKLIST-wave55-mastra-score-event.md \
      docs/contributing/SPLIT-MOVE-MAP-wave55-mastra-score-event.md \
      docs/contributing/SPLIT-REVIEW-PACK-wave55-mastra-score-event.md \
      scripts/ci/review-wave55-mastra-score-event.sh
  } | sort -u
)"
while IFS= read -r file; do
  [ -n "$file" ] || continue
  if [[ ! "$file" =~ $allowed_regex ]]; then
    echo "FAIL: out-of-scope path changed: $file"
    exit 1
  fi
done <<EOF
$changed_files
EOF

echo "[review] forbidden surfaces"
if ! git diff --quiet "$BASE_REF" -- Cargo.toml Cargo.lock .github/workflows; then
  echo "FAIL: Wave55 Mastra split must not touch Cargo files or workflows"
  exit 1
fi
if ! git diff --quiet "$BASE_REF" -- crates/assay-ebpf crates/assay-runner-core crates/assay-runner-linux crates/assay-runner-schema; then
  echo "FAIL: Wave55 Mastra split must not touch runner/eBPF crates"
  exit 1
fi
if ! git diff --quiet "$BASE_REF" -- crates/assay-cli/receipt-schemas docs/reference/receipt-family-matrix.json; then
  echo "FAIL: Wave55 Mastra split must not change receipt schemas or claim-family matrix"
  exit 1
fi

echo "[review] facade thinness"
facade_lines="$(wc -l < "$FACADE" | tr -d ' ')"
echo "facade lines: $facade_lines"
if [ "$facade_lines" -gt 120 ]; then
  echo "FAIL: Mastra facade is too thick after split"
  exit 1
fi
assert_rg '^mod constants;' "$FACADE" "constants module declaration missing"
assert_rg '^mod events;' "$FACADE" "events module declaration missing"
assert_rg '^mod reduce;' "$FACADE" "reduce module declaration missing"
assert_rg '^mod source;' "$FACADE" "source module declaration missing"
assert_rg '^mod validate;' "$FACADE" "validate module declaration missing"
assert_rg '^mod tests;' "$FACADE" "tests module declaration missing"
assert_rg 'pub struct MastraScoreEventArgs' "$FACADE" "MastraScoreEventArgs moved out of facade"
assert_rg 'pub fn cmd_mastra_score_event' "$FACADE" "cmd_mastra_score_event moved out of facade"
assert_not_rg '^fn (read_mastra_score_events|reduce_score_event|validate_top_level|string_equals|bounded_string|optional_bounded_string|validate_bounded_string|normalized_timestamp|parse_import_time|default_source_artifact_ref|sha256_file)\(' "$FACADE" "helper definition still lives in facade"

echo "[review] moved helper ownership"
assert_rg 'pub\(super\) const EVENT_TYPE' "$MODULE_DIR/constants.rs" "event constants missing"
assert_rg 'pub\(super\) fn read_mastra_score_events' "$MODULE_DIR/events.rs" "JSONL event reader missing"
assert_rg 'pub\(super\) fn reduce_score_event' "$MODULE_DIR/reduce.rs" "payload reducer missing"
assert_rg 'pub\(super\) fn parse_import_time' "$MODULE_DIR/source.rs" "import time helper missing"
assert_rg 'pub\(super\) fn sha256_file' "$MODULE_DIR/source.rs" "source digest helper missing"
assert_rg 'pub\(super\) fn validate_top_level' "$MODULE_DIR/validate.rs" "top-level validator missing"
assert_rg 'fn import_writes_verifiable_score_event_bundle' "$MODULE_DIR/tests.rs" "primary importer test missing"
assert_rg 'fn import_rejects_raw_metadata_and_correlation_context' "$MODULE_DIR/tests.rs" "raw metadata rejection test missing"
assert_rg 'fn import_rejects_legacy_underscore_surface' "$MODULE_DIR/tests.rs" "legacy surface rejection test missing"

echo "[review] repo checks"
cargo fmt --check
cargo check -p assay-cli
cargo test -p assay-cli mastra_score_event
cargo clippy -p assay-cli --all-targets -- -D warnings
git diff --check "$BASE_REF" --

echo "[review] PASS"
