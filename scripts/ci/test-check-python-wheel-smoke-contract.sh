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


run_install_spy() {
  local smoke_path="$1"
  local spy_root="$TMP/spy-root"
  local spy_dist="$TMP/spy-dist"
  mkdir -p "$spy_root" "$spy_dist"
  python3 - "$smoke_path" "$spy_root" "$spy_dist" <<'PY'
import importlib.util
import sys
from pathlib import Path

smoke_path = Path(sys.argv[1])
root = sys.argv[2]
dist = sys.argv[3]

spec = importlib.util.spec_from_file_location("smoke_python_wheel_spy", smoke_path)
if spec is None or spec.loader is None:
    raise SystemExit("cannot load smoke script")
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)

sentinel_version = "9.9.9-sentinel"
sentinel_wheel = Path("/sentinel/wheel.whl")
spy_calls = []


def load_cell(root, target):
    return {"import_smoke": "native", "tag": "cp312-cp312-manylinux_2_17_x86_64"}


def workspace_version(root):
    return sentinel_version


def find_wheel(dist, package, version, tag):
    return sentinel_wheel


def native_members(wheel):
    return ["assay/_native.so"]


def install_and_import(python, wheel, version):
    spy_calls.append((python, wheel, version))


mod.load_cell = load_cell
mod.workspace_version = workspace_version
mod.find_wheel = find_wheel
mod.native_members = native_members
mod.install_and_import = install_and_import

try:
    rc = mod.main(
        [
            "--root",
            root,
            "--dist-dir",
            dist,
            "--target",
            "x86_64-unknown-linux-gnu",
            "--python",
            "/sentinel/python",
            "--package",
            "assay-it",
        ]
    )
except SystemExit as exc:
    print(f"main raised SystemExit: {exc}", file=sys.stderr)
    raise SystemExit(1)

expected = [("/sentinel/python", sentinel_wheel, sentinel_version)]
if rc != 0 or spy_calls != expected:
    print(f"rc={rc!r} spy_calls={spy_calls!r} expected {expected!r}", file=sys.stderr)
    raise SystemExit(1)
raise SystemExit(0)
PY
}

expect_pass "production install_and_import spy" run_install_spy "$SMOKE"

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
        run: python3 scripts/ci/smoke-python-wheel.py --dist-dir assay-python-sdk/dist --python "python${{ needs.plan-python-artifact.outputs.python }}"

"""
if old not in text:
    raise SystemExit("smoke step not found")
path.write_text(text.replace(old, "", 1))
PY
expect_fail "drop smoke step" python3 "$CONTRACT" --root "$CASE"
cp "$ROOT/.github/workflows/release.yml" "$CASE/.github/workflows/release.yml"

echo "=== mutation: empty dist / no produced wheel ==="
mkdir -p "$CASE/assay-python-sdk/dist"
expect_fail "empty dist native cell" python3 "$SMOKE" --root "$ROOT" --dist-dir "$CASE/assay-python-sdk/dist" --target x86_64-unknown-linux-gnu --python python3 --package assay-it

echo "=== mutation: extra wheel beside expected ==="
write_dummy_wheel "$CASE/assay-python-sdk/dist" "assay_it-5.4.0-cp312-cp312-macosx_10_12_x86_64.whl"
write_dummy_wheel "$CASE/assay-python-sdk/dist" "noise-not-the-cell.whl"
expect_fail "extra wheel beside expected" python3 "$SMOKE" --root "$ROOT" --dist-dir "$CASE/assay-python-sdk/dist" --target x86_64-apple-darwin --python python3 --package assay-it
rm -f "$CASE/assay-python-sdk/dist/noise-not-the-cell.whl"

echo "=== mutation: renamed produced wheel ==="
mv "$CASE/assay-python-sdk/dist/assay_it-5.4.0-cp312-cp312-macosx_10_12_x86_64.whl" \
  "$CASE/assay-python-sdk/dist/renamed-away.whl"
expect_fail "renamed produced wheel" python3 "$SMOKE" --root "$ROOT" --dist-dir "$CASE/assay-python-sdk/dist" --target x86_64-apple-darwin --python python3 --package assay-it

echo "=== mutation: restore old x86 macos-15 + unsupported ==="
python3 - "$CASE/assay-python-sdk/python-artifact-matrix.v0.json" <<'PY'
import json
from pathlib import Path
import sys
path = Path(sys.argv[1])
data = json.loads(path.read_text())
for wheel in data["wheels"]:
    if wheel["target"] == "x86_64-apple-darwin":
        wheel["os"] = "macos-15"
        wheel["import_smoke"] = "unsupported"
path.write_text(json.dumps(data, indent=2) + "\n")
PY
expect_fail "old x86 macos-15 unsupported row" python3 "$CONTRACT" --root "$CASE"
cp "$ROOT/assay-python-sdk/python-artifact-matrix.v0.json" "$CASE/assay-python-sdk/python-artifact-matrix.v0.json"

echo "=== mutation: declared pair import_smoke=unsupported ==="
python3 - "$CASE/assay-python-sdk/python-artifact-matrix.v0.json" <<'PY'
import json
from pathlib import Path
import sys
path = Path(sys.argv[1])
data = json.loads(path.read_text())
data["wheels"][0]["import_smoke"] = "unsupported"
path.write_text(json.dumps(data, indent=2) + "\n")
PY
expect_fail "declared pair import_smoke unsupported" python3 "$CONTRACT" --root "$CASE"
cp "$ROOT/assay-python-sdk/python-artifact-matrix.v0.json" "$CASE/assay-python-sdk/python-artifact-matrix.v0.json"

echo "=== mutation: x86 os macos-15 even if native ==="
python3 - "$CASE/assay-python-sdk/python-artifact-matrix.v0.json" <<'PY'
import json
from pathlib import Path
import sys
path = Path(sys.argv[1])
data = json.loads(path.read_text())
for wheel in data["wheels"]:
    if wheel["target"] == "x86_64-apple-darwin":
        wheel["os"] = "macos-15"
        wheel["import_smoke"] = "native"
path.write_text(json.dumps(data, indent=2) + "\n")
PY
expect_fail "x86 os macos-15 with native smoke" python3 "$CONTRACT" --root "$CASE"
cp "$ROOT/assay-python-sdk/python-artifact-matrix.v0.json" "$CASE/assay-python-sdk/python-artifact-matrix.v0.json"

echo "=== mutation: release.yml wheels x86 left on macos-15 ==="
python3 - "$CASE/.github/workflows/release.yml" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
start = text.find("\n  wheels:")
end = text.find("\n  publish-pypi:", start)
if start < 0 or end < 0:
    raise SystemExit("wheels job bounds missing")
job = text[start:end]
old = "include: ${{ fromJSON(needs.plan-python-artifact.outputs.wheels) }}"
new = """include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
          - os: macos-15
            target: x86_64-apple-darwin
          - os: macos-15
            target: aarch64-apple-darwin
"""
if old not in job:
    raise SystemExit("wheels plan include not found")
job = job.replace(old, new, 1)
path.write_text(text[:start] + job + text[end:])
PY
expect_fail "release.yml wheels x86 os macos-15 vs matrix intel" python3 "$CONTRACT" --root "$CASE"
cp "$ROOT/.github/workflows/release.yml" "$CASE/.github/workflows/release.yml"

echo "=== mutation: drop native import from smoke script ==="
python3 - "$CASE/scripts/ci/smoke-python-wheel.py" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text().replace("assay._native", "assay_native_omitted")
path.write_text(text)
PY
expect_fail "smoke script without assay._native" python3 "$CONTRACT" --root "$CASE"

cp "$ROOT/scripts/ci/smoke-python-wheel.py" "$CASE/scripts/ci/smoke-python-wheel.py"
echo "=== mutation: drop production install_and_import call ==="
python3 - "$CASE/scripts/ci/smoke-python-wheel.py" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
old = "    install_and_import(args.python, wheel, version)\n"
if old not in text:
    raise SystemExit("production install_and_import call not found")
path.write_text(text.replace(old, "", 1))
PY
expect_fail "drop production install_and_import call" python3 "$CONTRACT" --root "$CASE"
cp "$ROOT/scripts/ci/smoke-python-wheel.py" "$CASE/scripts/ci/smoke-python-wheel.py"


echo "=== mutation: drop production install_and_import call (spy) ==="
python3 - "$CASE/scripts/ci/smoke-python-wheel.py" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
old = "    install_and_import(args.python, wheel, version)\n"
if old not in text:
    raise SystemExit("production install_and_import call not found")
path.write_text(text.replace(old, "", 1))
PY
expect_fail "drop production install_and_import call (spy)" run_install_spy "$CASE/scripts/ci/smoke-python-wheel.py"
cp "$ROOT/scripts/ci/smoke-python-wheel.py" "$CASE/scripts/ci/smoke-python-wheel.py"

echo "=== mutation: wrap production install_and_import in if False ==="
python3 - "$CASE/scripts/ci/smoke-python-wheel.py" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
old = "    install_and_import(args.python, wheel, version)\n"
new = "    if False:\n        install_and_import(args.python, wheel, version)\n"
if old not in text:
    raise SystemExit("production install_and_import call not found")
path.write_text(text.replace(old, new, 1))
PY
expect_fail "wrap production install_and_import in if False" run_install_spy "$CASE/scripts/ci/smoke-python-wheel.py"
cp "$ROOT/scripts/ci/smoke-python-wheel.py" "$CASE/scripts/ci/smoke-python-wheel.py"

echo "=== mutation: wrong python/wheel/version args to install_and_import ==="
python3 - "$CASE/scripts/ci/smoke-python-wheel.py" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
old = "    install_and_import(args.python, wheel, version)\n"
new = '    install_and_import("/wrong/python", wheel, version)\n'
if old not in text:
    raise SystemExit("production install_and_import call not found")
path.write_text(text.replace(old, new, 1))
PY
expect_fail "wrong python/wheel/version args to install_and_import" run_install_spy "$CASE/scripts/ci/smoke-python-wheel.py"
cp "$ROOT/scripts/ci/smoke-python-wheel.py" "$CASE/scripts/ci/smoke-python-wheel.py"

echo "=== mutation: return 0 before production install_and_import ==="
python3 - "$CASE/scripts/ci/smoke-python-wheel.py" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
old = "    install_and_import(args.python, wheel, version)\n"
new = "    return 0\n    install_and_import(args.python, wheel, version)\n"
if old not in text:
    raise SystemExit("production install_and_import call not found")
path.write_text(text.replace(old, new, 1))
PY
expect_fail "return 0 before production install_and_import" run_install_spy "$CASE/scripts/ci/smoke-python-wheel.py"
cp "$ROOT/scripts/ci/smoke-python-wheel.py" "$CASE/scripts/ci/smoke-python-wheel.py"

echo "=== no-op restore ==="
cp "$ROOT/.github/workflows/release.yml" "$CASE/.github/workflows/release.yml"
cp "$ROOT/assay-python-sdk/python-artifact-matrix.v0.json" "$CASE/assay-python-sdk/python-artifact-matrix.v0.json"
cp "$ROOT/scripts/ci/smoke-python-wheel.py" "$CASE/scripts/ci/smoke-python-wheel.py"
expect_pass "restored smoke contract" python3 "$CONTRACT" --root "$CASE"

echo "ALL GREEN"
