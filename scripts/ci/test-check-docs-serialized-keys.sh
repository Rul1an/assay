#!/usr/bin/env bash
# Prove documented CoverageReport keys are checked against the serialised type (#3095).
# Each PAGES entry must be load-bearing: dropping it from the list turns this harness red (#3105).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

CHECK="${SERIALIZED_KEYS_CHECK:-$ROOT/scripts/ci/check-docs-serialized-keys.py}"
PAGE=docs/getting-started/python-quickstart.md
SDK_PAGE=docs/python-sdk/index.md
SOURCE=crates/assay-core/src/coverage_next/types.rs

load_pages() {
  python3 - "$1" <<'PY'
from pathlib import Path
import importlib.util
import sys

spec = importlib.util.spec_from_file_location("check", Path(sys.argv[1]))
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
for rel in mod.PAGES:
    print(rel)
PY
}

drop_page_from_check() {
  python3 - "$1" "$2" "$3" <<'PY'
from pathlib import Path
import sys

src, page, dest = Path(sys.argv[1]), sys.argv[2], Path(sys.argv[3])
text = src.read_text(encoding="utf-8")
line = f'    "{page}",\n'
if text.count(line) != 1:
    raise SystemExit(
        f"PAGES entry must occur exactly once to drop: {page!r} count={text.count(line)}"
    )
dest.write_text(text.replace(line, ""), encoding="utf-8")
PY
}

# Copy the snapshot pages, not the check's (possibly reduced) PAGES, so a dropped
# entry still has a file to plant on. A missing-file crash is not a red.
reset_tree() {
  rm -rf "$TMP/tree"
  python3 - "$TMP/tree" "$ROOT" "$SOURCE" "$TMP/pages.snapshot" <<'PY'
from pathlib import Path
import shutil
import sys

dest, root, source, pages_file = (
    Path(sys.argv[1]),
    Path(sys.argv[2]),
    sys.argv[3],
    Path(sys.argv[4]),
)
pages = [line for line in pages_file.read_text(encoding="utf-8").splitlines() if line]
for rel in (*pages, source):
    out = dest / rel
    out.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(root / rel, out)
PY
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

if [[ -n "${SERIALIZED_KEYS_PAGES_FILE:-}" ]]; then
  cp "$SERIALIZED_KEYS_PAGES_FILE" "$TMP/pages.snapshot"
else
  load_pages "$CHECK" >"$TMP/pages.snapshot"
fi

reset_tree
run_check >"$TMP/control.out" 2>&1 || {
  cat "$TMP/control.out" >&2
  echo "FAIL: the shipped pages and CoverageReport must agree" >&2
  exit 1
}
printf 'PASS: shipped pages keys agree with CoverageReport\n'

mutations=0
expect_red() {
  local name="$1" diagnostic="$2" out
  out="$TMP/$(printf '%s' "$name" | tr '/' '_').out"
  if run_check >"$out" 2>&1; then
    echo "FAIL: mutation $name was not observed" >&2
    exit 1
  fi
  grep -Fq "$diagnostic" "$out" || {
    cat "$out" >&2
    echo "FAIL: mutation $name missed diagnostic: $diagnostic" >&2
    exit 1
  }
  mutations=$((mutations + 1))
  printf 'PASS: %s\n' "$name"
  reset_tree
}

# Every snapshotted page is scanned. Derived from the check's PAGES, not from
# a second pinned list: adding a page to PAGES adds the plant; dropping one
# leaves that plant unobserved and this suite goes red. These run first so an
# inverted drop fails on the page-scanned plant, not on a later missing-file
# crash of a historical mutant.
page_scanned=0
while IFS= read -r page; do
  [[ -n "$page" ]] || continue
  python3 - "$TMP/tree/$page" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
plant = 'report["passed"]'
if plant in text:
    raise SystemExit(f"{path}: plant {plant!r} already present")
path.write_text(text + "\n" + plant + "\n", encoding="utf-8")
PY
  expect_red "page-scanned-${page}" "${page}: documents 'passed'"
  page_scanned=$((page_scanned + 1))
done <"$TMP/pages.snapshot"
expected_pages="$(grep -c . "$TMP/pages.snapshot")"
if [ "$page_scanned" -ne "$expected_pages" ]; then
  echo "FAIL: expected ${expected_pages} page-scanned mutants, got ${page_scanned}" >&2
  exit 1
fi

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

expected_mutations=$((7 + expected_pages))
if [ "$mutations" -ne "$expected_mutations" ]; then
  echo "FAIL: expected ${expected_mutations} observed mutations, got $mutations" >&2
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

# Inverted mutant (#3105): drop each snapshotted PAGES entry from a copy of
# the check and re-run this suite against that copy. The suite must go red.
# Pages are taken from the check under test, not from a second pinned list.
if [[ "${SERIALIZED_KEYS_SKIP_INVERTED:-}" == "1" ]]; then
  exit 0
fi

while IFS= read -r page; do
  [[ -n "$page" ]] || continue
  safe="$(printf '%s' "$page" | tr '/.' '__')"
  mutated="$TMP/check-without-${safe}.py"
  drop_page_from_check "$CHECK" "$page" "$mutated"
  if SERIALIZED_KEYS_SKIP_INVERTED=1 \
    SERIALIZED_KEYS_CHECK="$mutated" \
    SERIALIZED_KEYS_PAGES_FILE="$TMP/pages.snapshot" \
    bash "$0" >"$TMP/inverted-${safe}.out" 2>&1; then
    echo "FAIL: dropping $page from PAGES left the harness green" >&2
    cat "$TMP/inverted-${safe}.out" >&2
    exit 1
  fi
  grep -Fq "FAIL: mutation page-scanned-${page} was not observed" \
    "$TMP/inverted-${safe}.out" || {
    cat "$TMP/inverted-${safe}.out" >&2
    echo "FAIL: dropping $page did not fail on the page-scanned plant" >&2
    exit 1
  }
  printf 'PASS: inverted-drop %s\n' "$page"
done <"$TMP/pages.snapshot"
