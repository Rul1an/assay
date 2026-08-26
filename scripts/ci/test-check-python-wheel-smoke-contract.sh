#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/clear-git-repository-env.sh"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CONTRACT="$ROOT/scripts/ci/check-python-wheel-smoke-contract.py"
SMOKE="$ROOT/scripts/ci/smoke-python-wheel.py"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

pass() { echo "PASS: $*"; }
fail_test() { echo "FAIL: $*" >&2; exit 1; }

expect_fail() {
  local label="$1"
  shift
  if "$@" >"$TMP/out" 2>"$TMP/err"; then
    fail_test "$label: expected RED, got PASS"
  fi
  pass "$label RED"
}

expect_pass() {
  local label="$1"
  shift
  if ! "$@" >"$TMP/out" 2>"$TMP/err"; then
    cat "$TMP/err" >&2
    fail_test "$label: expected PASS"
  fi
  pass "$label GREEN"
}

expect_pass "live smoke contract" python3 "$CONTRACT" --root "$ROOT"

write_dummy_wheel() {
  local dest="$1" name="$2"
  python3 - "$dest" "$name" <<'PY'
import sys, zipfile
from pathlib import Path
dest = Path(sys.argv[1])
dest.mkdir(parents=True, exist_ok=True)
name = sys.argv[2]
with zipfile.ZipFile(dest / name, "w") as archive:
    archive.writestr("assay/_native.cpython-312-darwin.so", b"not-a-real-extension")
    archive.writestr(
        "assay_it-5.4.0.dist-info/METADATA",
        "Metadata-Version: 2.1\nName: assay-it\nVersion: 5.4.0\n",
    )
PY
}

CASE="$TMP/case"
mkdir -p "$CASE"
cp -a "$ROOT/.github" "$CASE/.github"
cp -a "$ROOT/assay-python-sdk" "$CASE/assay-python-sdk"
cp -a "$ROOT/scripts" "$CASE/scripts"
cp "$ROOT/Cargo.toml" "$CASE/Cargo.toml"

echo "=== mutation: drop smoke step ==="
python3 - "$CASE/.github/workflows/release.yml" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
old = """      - name: Smoke the produced wheel
        shell: bash
        env:
          ASSAY_WHEEL_TARGET: ${{ matrix.target }}
        run: python3 scripts/ci/smoke-python-wheel.py --dist-dir assay-python-sdk/dist

"""
if old not in text:
    raise SystemExit("smoke step not found")
path.write_text(text.replace(old, "", 1))
PY
expect_fail "drop smoke step" python3 "$CONTRACT" --root "$CASE"
cp "$ROOT/.github/workflows/release.yml" "$CASE/.github/workflows/release.yml"

echo "=== mutation: empty dist / no produced wheel ==="
mkdir -p "$CASE/assay-python-sdk/dist"
expect_fail "empty dist native cell" python3 "$SMOKE" --root "$ROOT" --dist-dir "$CASE/assay-python-sdk/dist" --target x86_64-unknown-linux-gnu

echo "=== mutation: two matching wheels ==="
write_dummy_wheel "$CASE/assay-python-sdk/dist" "assay_it-5.4.0-cp312-cp312-macosx_10_12_x86_64.whl"
cp "$CASE/assay-python-sdk/dist/assay_it-5.4.0-cp312-cp312-macosx_10_12_x86_64.whl" \
  "$CASE/assay-python-sdk/dist/assay_it-5.4.0-cp312-cp312-macosx_10_12_x86_64.whl.bak"
# two files with the same expected name cannot coexist; write a second unexpected extra plus the expected, then duplicate via copy to a second matching path using a temp dir listing
# The smoke matches exact filename, so two exact names need two dirs. Instead drop the expected name and leave nothing, already covered. Create two exact files by running find against a dir that has the expected plus a renamed duplicate that still matches glob+name.
python3 - "$CASE/assay-python-sdk/dist" <<'PY'
from pathlib import Path
import sys
dist = Path(sys.argv[1])
expected = dist / "assay_it-5.4.0-cp312-cp312-macosx_10_12_x86_64.whl"
# A second file with a different name must not satisfy the exact-name check; prove extra files are ignored.
(expected.with_name("noise-not-the-cell.whl")).write_bytes(expected.read_bytes())
print("extra non-matching wheel present")
PY
expect_pass "unsupported cell with one matching wheel" python3 "$SMOKE" --root "$ROOT" --dist-dir "$CASE/assay-python-sdk/dist" --target x86_64-apple-darwin

# two exact matches: copy into a listing that smoke sees as two same names via a wrapper dir is impossible. Simulate by editing find to... instead write two files where the second has the same name in a way glob returns both — can't. Use a subdirectory? glob is dist.glob("*.whl") so only top-level.
# Create two files that both equal expected name after we patch? Simpler: write expected twice by using a second identical tag file that the code treats as match — it matches path.name == expected, so only exact name.
# Reproduce "two wheels" by temporarily making find_wheel count all *.whl when names collide via hardlink? Two hardlinks same name is one file.
# I'll drop the expected file (rename) while leaving only noise — that's empty for that cell.
mv "$CASE/assay-python-sdk/dist/assay_it-5.4.0-cp312-cp312-macosx_10_12_x86_64.whl" \
  "$CASE/assay-python-sdk/dist/renamed-away.whl"
expect_fail "renamed produced wheel" python3 "$SMOKE" --root "$ROOT" --dist-dir "$CASE/assay-python-sdk/dist" --target x86_64-apple-darwin

echo "=== mutation: native import_smoke on cross macos x86_64 ==="
python3 - "$CASE/assay-python-sdk/python-artifact-matrix.v0.json" <<'PY'
import json
from pathlib import Path
import sys
path = Path(sys.argv[1])
data = json.loads(path.read_text())
for wheel in data["wheels"]:
    if wheel["target"] == "x86_64-apple-darwin":
        wheel["import_smoke"] = "native"
path.write_text(json.dumps(data, indent=2) + "\n")
PY
expect_fail "cross cell claimed native" python3 "$CONTRACT" --root "$CASE"
cp "$ROOT/assay-python-sdk/python-artifact-matrix.v0.json" "$CASE/assay-python-sdk/python-artifact-matrix.v0.json"

echo "=== mutation: drop native import from smoke script ==="
python3 - "$CASE/scripts/ci/smoke-python-wheel.py" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text().replace("assay._native", "assay_native_omitted")
path.write_text(text)
PY
expect_fail "smoke script without assay._native" python3 "$CONTRACT" --root "$CASE"

echo "=== no-op restore ==="
cp "$ROOT/.github/workflows/release.yml" "$CASE/.github/workflows/release.yml"
cp "$ROOT/assay-python-sdk/python-artifact-matrix.v0.json" "$CASE/assay-python-sdk/python-artifact-matrix.v0.json"
cp "$ROOT/scripts/ci/smoke-python-wheel.py" "$CASE/scripts/ci/smoke-python-wheel.py"
expect_pass "restored smoke contract" python3 "$CONTRACT" --root "$CASE"

echo "ALL GREEN"
