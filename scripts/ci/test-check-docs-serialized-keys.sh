#!/usr/bin/env bash
# Prove documented CoverageReport keys are checked against the serialised type (#3095).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

CHECK="$ROOT/scripts/ci/check-docs-serialized-keys.py"
PAGE=docs/getting-started/python-quickstart.md
SDK_PAGE=docs/python-sdk/index.md
SOURCE=crates/assay-core/src/coverage_next/types.rs

reset_tree() {
  rm -rf "$TMP/tree"
  mkdir -p "$TMP/tree/$(dirname "$PAGE")" "$TMP/tree/$(dirname "$SDK_PAGE")" \
    "$TMP/tree/$(dirname "$SOURCE")"
  cp "$ROOT/$PAGE" "$TMP/tree/$PAGE"
  cp "$ROOT/$SDK_PAGE" "$TMP/tree/$SDK_PAGE"
  cp "$ROOT/$SOURCE" "$TMP/tree/$SOURCE"
}

run_check() {
  python3 "$CHECK" --root "$TMP/tree"
}

replace_once() {
  python3 - "$TMP/tree/$1" "$2" "$3" <<'PY'
from pathlib import Path
import sys

path, old, new = Path(sys.argv[1]), sys.argv[2], sys.argv[3]
text = path.read_text(encoding="utf-8")
if text.count(old) != 1:
    raise SystemExit(f"mutation anchor must occur exactly once in {path}: {old!r}")
path.write_text(text.replace(old, new), encoding="utf-8")
PY
}

reset_tree
run_check >"$TMP/control.out" 2>&1 || {
  cat "$TMP/control.out" >&2
  echo "FAIL: the shipped pages and CoverageReport must agree" >&2
  exit 1
}
printf 'PASS: shipped pages keys agree with CoverageReport\n'

mutations=0
expect_red() {
  local name="$1" diagnostic="$2"
  if run_check >"$TMP/$name.out" 2>&1; then
    echo "FAIL: mutation $name was not observed" >&2
    exit 1
  fi
  grep -Fq "$diagnostic" "$TMP/$name.out" || {
    cat "$TMP/$name.out" >&2
    echo "FAIL: mutation $name missed diagnostic: $diagnostic" >&2
    exit 1
  }
  mutations=$((mutations + 1))
  printf 'PASS: %s\n' "$name"
  reset_tree
}

# The page as it shipped when #3095 was opened: the historical defect must stay red.
python3 - "$TMP/tree/$PAGE" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
text = text.replace(
    "assert report[\"meets_threshold\"]",
    "assert report[\"passed\"]",
    1,
)
if "report[\"passed\"]" not in text:
    raise SystemExit("historical passed subscript did not land")
path.write_text(text, encoding="utf-8")
PY
expect_red historical-passed "documents 'passed'"

replace_once "$PAGE" 'report["overall_coverage_pct"]' 'report["score"]'
expect_red historical-score "documents 'score'"

replace_once "$PAGE" 'report["policy_violations"]' "report['violations']"
expect_red historical-violations "documents 'violations'"

replace_once "$SOURCE" '    pub meets_threshold: bool,' '    pub threshold_met: bool,'
expect_red dropped-field "documents 'meets_threshold'"

replace_once "$SOURCE" '    pub meets_threshold: bool,' \
  '    #[serde(rename = "passed")]\n    pub meets_threshold: bool,'
expect_red serde-rename 'serde(rename)'

replace_once "$SOURCE" 'pub struct CoverageReport {' 'pub struct CoverageAnalysis {'
expect_red struct-missing 'exactly one `pub struct CoverageReport`'

# The SDK page must stay in the scanned set: a list that omitted it stayed green.
replace_once "$SDK_PAGE" 'report["meets_threshold"]' 'report["passed"]'
expect_red sdk-page-scanned "docs/python-sdk/index.md: documents 'passed'"

if [ "$mutations" -ne 7 ]; then
  echo "FAIL: expected 7 observed mutations, got $mutations" >&2
  exit 1
fi

# A reworded page that keeps the same keys stays green.
replace_once "$PAGE" 'Returns Coverage.analyze() unchanged' \
  'Returns the coverage analysis'
run_check >"$TMP/reworded.out" 2>&1 || {
  cat "$TMP/reworded.out" >&2
  echo "FAIL: a reworded page naming the same keys was refused" >&2
  exit 1
}
printf 'PASS: reworded page with the same keys stays green\n'
printf 'docs serialized-keys mutations: %s observed\n' "$mutations"
