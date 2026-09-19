#!/usr/bin/env bash
# Prove documented CoverageReport keys are checked against the serialised type (#3095).
# Each PAGES entry must be load-bearing: dropping it from the list turns this harness red (#3105).
# Coverage.analyze Returns bullets are a distinct extract; adding that file to PAGES is not it.
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

load_methods() {
  python3 - "$1" <<'PY'
from pathlib import Path
import importlib.util
import sys

spec = importlib.util.spec_from_file_location("check", Path(sys.argv[1]))
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
for item in getattr(mod, "METHODS", ()):
    if not isinstance(item, tuple) or len(item) != 3:
        raise SystemExit(f"METHODS entry must be (path, class, method): {item!r}")
    print("\t".join(item))
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

drop_method_and_add_to_pages() {
  python3 - "$1" "$2" "$3" "$4" "$5" <<'PY'
from pathlib import Path
import sys

src, rel, cls, meth, dest = (
    Path(sys.argv[1]),
    sys.argv[2],
    sys.argv[3],
    sys.argv[4],
    Path(sys.argv[5]),
)
text = src.read_text(encoding="utf-8")
line = f'    ("{rel}", "{cls}", "{meth}"),\n'
if text.count(line) != 1:
    raise SystemExit(
        f"METHODS entry must occur exactly once to drop: {line!r} count={text.count(line)}"
    )
text = text.replace(line, "")
start = text.index("PAGES = (")
close = text.index("\n)\n", start)
entry = f'    "{rel}",'
if entry in text[start:close]:
    raise SystemExit(f"{rel} already listed in PAGES")
text = text[:close] + "\n" + entry + text[close:]
dest.write_text(text, encoding="utf-8")
PY
}

hardcode_fields_in_check() {
  python3 - "$1" "$2" "$3" <<'PY'
from pathlib import Path
import importlib.util
import sys

src, dest, source_path = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
text = src.read_text(encoding="utf-8")
old = "def coverage_report_fields(source: str) -> set[str]:\n"
if text.count(old) != 1:
    raise SystemExit("coverage_report_fields def must occur once")
spec = importlib.util.spec_from_file_location("check", src)
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
frozen = sorted(mod.coverage_report_fields(source_path.read_text(encoding="utf-8")))
dest.write_text(
    text.replace(old, old + f"    return set({frozen!r})\n", 1),
    encoding="utf-8",
)
PY
}

# Copy the snapshot pages and method files, not the check's (possibly reduced)
# inventories, so a dropped entry still has a file to plant on.
reset_tree() {
  rm -rf "$TMP/tree"
  python3 - "$TMP/tree" "$ROOT" "$SOURCE" "$TMP/pages.snapshot" "$TMP/methods.snapshot" <<'PY'
from pathlib import Path
import shutil
import sys

dest, root, source, pages_file, methods_file = (
    Path(sys.argv[1]),
    Path(sys.argv[2]),
    sys.argv[3],
    Path(sys.argv[4]),
    Path(sys.argv[5]),
)
pages = [line for line in pages_file.read_text(encoding="utf-8").splitlines() if line]
method_files = []
for line in methods_file.read_text(encoding="utf-8").splitlines():
    if line:
        method_files.append(line.split("\t", 1)[0])
seen: set[str] = set()
rels: list[str] = []
for rel in (*pages, source, *method_files):
    if rel in seen:
        continue
    seen.add(rel)
    rels.append(rel)
for rel in rels:
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
new = new.replace("\\n", "\n")
text = path.read_text(encoding="utf-8")
if text.count(old) != 1:
    raise SystemExit(f"mutation anchor must occur exactly once in {path}: {old!r}")
path.write_text(text.replace(old, new), encoding="utf-8")
PY
}

plant_analyze_bullet() {
  python3 - "$TMP/tree/$COVERAGE_PY" "$1" <<'PY'
from pathlib import Path
import sys

path, key = Path(sys.argv[1]), sys.argv[2]
text = path.read_text(encoding="utf-8")
anchor = "                  - `threshold` (float): The threshold that was checked."
plant = f"                  - `{key}` (bool): planted."
if text.count(anchor) != 1:
    raise SystemExit("analyze Returns threshold bullet must occur exactly once")
if plant in text:
    raise SystemExit(f"plant {plant!r} already present")
path.write_text(text.replace(anchor, anchor + "\n" + plant, 1), encoding="utf-8")
for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
    if f"- `{key}`" in line and "planted." in line:
        print(lineno)
        break
else:
    raise SystemExit("planted bullet line not found")
PY
}

if [[ -n "${SERIALIZED_KEYS_PAGES_FILE:-}" ]]; then
  cp "$SERIALIZED_KEYS_PAGES_FILE" "$TMP/pages.snapshot"
else
  load_pages "$CHECK" >"$TMP/pages.snapshot"
fi

if [[ -n "${SERIALIZED_KEYS_METHODS_FILE:-}" ]]; then
  cp "$SERIALIZED_KEYS_METHODS_FILE" "$TMP/methods.snapshot"
else
  load_methods "$CHECK" >"$TMP/methods.snapshot"
fi

COVERAGE_PY="$(
  python3 - "$TMP/methods.snapshot" <<'PY'
from pathlib import Path
import sys

lines = [line for line in Path(sys.argv[1]).read_text(encoding="utf-8").splitlines() if line]
if not lines:
    raise SystemExit("METHODS snapshot is empty")
print(lines[0].split("\t", 1)[0])
PY
)"
METHOD_CLASS="$(
  python3 - "$TMP/methods.snapshot" <<'PY'
from pathlib import Path
import sys

print(Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()[0].split("\t")[1])
PY
)"
METHOD_NAME="$(
  python3 - "$TMP/methods.snapshot" <<'PY'
from pathlib import Path
import sys

print(Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()[0].split("\t")[2])
PY
)"

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

expect_green() {
  local name="$1" out
  out="$TMP/$(printf '%s' "$name" | tr '/' '_').out"
  if ! run_check >"$out" 2>&1; then
    cat "$out" >&2
    echo "FAIL: $name should have stayed green" >&2
    exit 1
  fi
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

# Container rename_all is outside the struct body; body-only serde(rename) misses it.
replace_once "$SOURCE" 'pub struct CoverageReport {' \
  '#[serde(rename_all = "camelCase")]\npub struct CoverageReport {'
expect_red serde-rename-all 'serde(rename_all)'

replace_once "$SOURCE" '    pub meets_threshold: bool,' \
  '    #[serde(skip)]\n    pub meets_threshold: bool,'
expect_red serde-skip 'serde(skip)'

replace_once "$SOURCE" '    pub tool_coverage: ToolCoverage,' \
  '    #[serde(flatten)]\n    pub tool_coverage: ToolCoverage,'
expect_red serde-flatten 'serde(flatten)'

replace_once "$SOURCE" 'pub struct CoverageReport {' 'pub struct CoverageAnalysis {'
expect_red struct-missing 'exactly one `pub struct CoverageReport`'

# The SDK page must stay in the scanned set: a list that omitted it stayed green.
replace_once "$SDK_PAGE" 'report["meets_threshold"]' 'report["passed"]'
expect_red sdk-page-scanned "docs/python-sdk/index.md: documents 'passed'"

# Docstring extract: plant must name the actual file, line, and key.
planted_line="$(plant_analyze_bullet passed)"
expect_red planted-passed "${COVERAGE_PY}:${planted_line}: documents 'passed'"

replace_once "$COVERAGE_PY" \
  '    def analyze(self, traces: list, min_coverage: float = 80.0) -> dict:' \
  '    def analyze_removed(self, traces: list, min_coverage: float = 80.0) -> dict:'
expect_red missing-analyze "${METHOD_CLASS}.${METHOD_NAME}"

replace_once "$COVERAGE_PY" \
  'from ._native import CoverageAnalyzer, Policy' \
  'from ._native import CoverageAnalyzer, Policy ('
expect_red coverage-syntax 'syntax'

malformed="$TMP/check-malformed-methods.py"
python3 - "$CHECK" "$malformed" "$COVERAGE_PY" "$METHOD_CLASS" <<'PY'
from pathlib import Path
import sys

src, dest, rel, cls = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3], sys.argv[4]
text = src.read_text(encoding="utf-8")
line = f'    ("{rel}", "{cls}", "analyze"),\n'
if text.count(line) != 1:
    raise SystemExit(f"METHODS analyze entry must occur once to blank: {line!r}")
dest.write_text(text.replace(line, f'    ("{rel}", "{cls}", ""),\n', 1), encoding="utf-8")
PY
CHECK="$malformed" expect_red malformed-methods 'malformed'
CHECK="${SERIALIZED_KEYS_CHECK:-$ROOT/scripts/ci/check-docs-serialized-keys.py}"

expected_mutations=$((14 + expected_pages))
if [ "$mutations" -ne "$expected_mutations" ]; then
  echo "FAIL: expected ${expected_mutations} observed mutations, got $mutations" >&2
  exit 1
fi

# A reworded page that keeps the same keys stays green.
replace_once "$PAGE" 'Returns Coverage.analyze() unchanged' \
  'Returns the coverage analysis'
expect_green reworded-page

# Other-method Returns must not leak into Coverage.analyze.
replace_once "$COVERAGE_PY" \
  '            ValueError: If the policy file is invalid.
        """
        self.policy = Policy.from_file(policy_path)' \
  '            ValueError: If the policy file is invalid.
        Returns:
                  - `passed` (bool): other-method plant.
        """
        self.policy = Policy.from_file(policy_path)'
expect_green other-method-plant

# Unbackticked prose and a Returns section with no key bullets stay green.
python3 - "$TMP/tree/$COVERAGE_PY" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = """        Returns:
            dict: A detailed coverage report dictionary containing:
                  - `tool_coverage` (dict): Tool coverage metrics.
                  - `rule_coverage` (dict): Rule coverage metrics.
                  - `high_risk_gaps` (list): Blocklisted tools never seen in traces.
                  - `policy_violations` (list): Policy violations found during analysis.
                  - `policy_warnings` (list): Policy warnings (for example unconstrained tools).
                  - `overall_coverage_pct` (float): Overall coverage percentage.
                  - `meets_threshold` (bool): Whether coverage met the threshold.
                  - `threshold` (float): The threshold that was checked."""
new = """        Returns:
            dict: A detailed coverage report dictionary. The passed flag is prose."""
if text.count(old) != 1:
    raise SystemExit("analyze Returns block must occur exactly once")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
expect_green no-key-prose

python3 - "$TMP/tree/$COVERAGE_PY" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = """                  - `tool_coverage` (dict): Tool coverage metrics.
                  - `rule_coverage` (dict): Rule coverage metrics.
                  - `high_risk_gaps` (list): Blocklisted tools never seen in traces.
                  - `policy_violations` (list): Policy violations found during analysis.
                  - `policy_warnings` (list): Policy warnings (for example unconstrained tools).
                  - `overall_coverage_pct` (float): Overall coverage percentage.
                  - `meets_threshold` (bool): Whether coverage met the threshold.
                  - `threshold` (float): The threshold that was checked."""
if text.count(old) != 1:
    raise SystemExit("analyze Returns bullets must occur exactly once")
path.write_text(text.replace(old, "", 1), encoding="utf-8")
PY
expect_green list-omission

replace_once "$SOURCE" '    pub meets_threshold: bool,' \
  '    #[serde(default)]\n    pub meets_threshold: bool,'
expect_green extra-serde-default

replace_once "$COVERAGE_PY" '        Returns:' '        :returns:'
plant_analyze_bullet passed >/dev/null
expect_green sphinx-returns-nonclaim

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
    SERIALIZED_KEYS_METHODS_FILE="$TMP/methods.snapshot" \
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

# METHODS inventory is load-bearing. Drop analyze and put the Python file on
# PAGES: a subscript extract still cannot see Returns bullets.
methods_mutated="$TMP/check-without-analyze-method.py"
drop_method_and_add_to_pages "$CHECK" "$COVERAGE_PY" "$METHOD_CLASS" "$METHOD_NAME" \
  "$methods_mutated"
if SERIALIZED_KEYS_SKIP_INVERTED=1 \
  SERIALIZED_KEYS_CHECK="$methods_mutated" \
  SERIALIZED_KEYS_PAGES_FILE="$TMP/pages.snapshot" \
  SERIALIZED_KEYS_METHODS_FILE="$TMP/methods.snapshot" \
  bash "$0" >"$TMP/inverted-methods.out" 2>&1; then
  echo "FAIL: dropping Coverage.analyze from METHODS left the harness green" >&2
  cat "$TMP/inverted-methods.out" >&2
  exit 1
fi
grep -Fq "FAIL: mutation planted-passed was not observed" \
  "$TMP/inverted-methods.out" || {
  cat "$TMP/inverted-methods.out" >&2
  echo "FAIL: dropping METHODS did not fail on the docstring plant" >&2
  exit 1
}
printf 'PASS: inverted-drop METHODS (PAGES add is not extraction)\n'

# Source oracle is load-bearing: a frozen field set ignores a struct rename.
oracle_mutated="$TMP/check-hardcoded-fields.py"
hardcode_fields_in_check "$CHECK" "$oracle_mutated" "$ROOT/$SOURCE"
if SERIALIZED_KEYS_SKIP_INVERTED=1 \
  SERIALIZED_KEYS_CHECK="$oracle_mutated" \
  SERIALIZED_KEYS_PAGES_FILE="$TMP/pages.snapshot" \
  SERIALIZED_KEYS_METHODS_FILE="$TMP/methods.snapshot" \
  bash "$0" >"$TMP/inverted-source-oracle.out" 2>&1; then
  echo "FAIL: hardcoding coverage_report_fields left the harness green" >&2
  cat "$TMP/inverted-source-oracle.out" >&2
  exit 1
fi
grep -Fq "FAIL: mutation dropped-field was not observed" \
  "$TMP/inverted-source-oracle.out" || {
  cat "$TMP/inverted-source-oracle.out" >&2
  echo "FAIL: hardcoding fields did not fail on dropped-field" >&2
  exit 1
}
printf 'PASS: inverted-drop source oracle\n'
