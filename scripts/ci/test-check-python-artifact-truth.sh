#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/clear-git-repository-env.sh"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECK="$ROOT/scripts/ci/check-python-artifact-truth.py"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

pass() { echo "PASS: $*"; }
fail_test() { echo "FAIL: $*" >&2; exit 1; }

expect_fail() {
  local label="$1"
  shift
  if python3 "$CHECK" "$@" >"$TMP/out" 2>"$TMP/err"; then
    fail_test "$label: expected RED, got PASS"
  fi
  pass "$label RED"
}

expect_pass() {
  local label="$1"
  shift
  if ! python3 "$CHECK" "$@" >"$TMP/out" 2>"$TMP/err"; then
    cat "$TMP/err" >&2
    fail_test "$label: expected PASS"
  fi
  pass "$label GREEN"
}

write_green_fixture() {
  local dest="$1"
  mkdir -p "$dest/assay-python-sdk" "$dest/.github/workflows" \
    "$dest/docs/python-sdk" "$dest/docs/getting-started" \
    "$dest/docs/guides" "$dest/docs/AIcontext"
  cat > "$dest/Cargo.toml" <<'TOML'
[workspace.package]
version = "5.4.0"
TOML
  python3 - "$dest/assay-python-sdk/python-artifact-matrix.v0.json" <<'PY'
from pathlib import Path
import json
import sys

Path(sys.argv[1]).write_text(
    json.dumps(
        {
            "schema": "assay.python_artifact_matrix.v0",
            "package": "assay-it",
            "requires_python": "==3.12.*",
            "required_classifiers": [
                "Programming Language :: Python :: 3",
                "Programming Language :: Python :: 3.12",
                "Programming Language :: Python :: Implementation :: CPython",
            ],
            "forbidden_classifiers": [
                "Programming Language :: Python :: Implementation :: PyPy",
            ],
            "publish_sdist": False,
            "support_bound": (
                "CPython 3.12 on macOS x86_64/arm64 and Linux x86_64; "
                "other interpreters and platforms are not claimed."
            ),
            "wheels": [
                {
                    "os": "ubuntu-latest",
                    "target": "x86_64-unknown-linux-gnu",
                    "tag": "cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64",
                    "import_smoke": "native",
                },
                {
                    "os": "macos-15",
                    "target": "x86_64-apple-darwin",
                    "tag": "cp312-cp312-macosx_10_12_x86_64",
                    "import_smoke": "unsupported",
                },
                {
                    "os": "macos-15",
                    "target": "aarch64-apple-darwin",
                    "tag": "cp312-cp312-macosx_11_0_arm64",
                    "import_smoke": "native",
                },
            ],
            "install_docs": [
                "assay-python-sdk/README.md",
                "docs/python-sdk/index.md",
                "docs/getting-started/python-quickstart.md",
                "docs/getting-started/installation.md",
                "docs/getting-started/index.md",
                "docs/guides/troubleshooting.md",
                "docs/AIcontext/user-flows.md",
                "llms.txt",
            ],
        },
        indent=2,
    )
    + "\n"
)
PY
  cat > "$dest/assay-python-sdk/pyproject.toml" <<'TOML'
requires-python = "==3.12.*"
classifiers = [
    "Programming Language :: Python :: 3",
    "Programming Language :: Python :: 3.12",
    "Programming Language :: Python :: Implementation :: CPython",
]
[tool.maturin]
features = ["pyo3/extension-module"]
TOML
  cat > "$dest/assay-python-sdk/Cargo.toml" <<'TOML'
[dependencies]
pyo3 = { version = "0.29", features = ["pyo3/extension-module"] }
TOML
  cat > "$dest/.github/workflows/release.yml" <<'YML'
jobs:
  wheels:
    name: Build Wheels
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
          - os: macos-15
            target: x86_64-apple-darwin
          - os: macos-15
            target: aarch64-apple-darwin
    steps:
      - uses: actions/setup-python@v6
        with:
          python-version: '3.12'
      - name: Build wheels
        uses: PyO3/maturin-action@v1
        with:
          args: --release --out dist --locked -i python3.12 --compatibility pypi
      - name: Smoke the produced wheel
        env:
          ASSAY_WHEEL_TARGET: ${{ matrix.target }}
        run: python3 scripts/ci/smoke-python-wheel.py --dist-dir assay-python-sdk/dist
      - name: Upload wheels
        with:
          path: assay-python-sdk/dist/*.whl
  publish-pypi:
    name: Publish to PyPI
YML
  bound='CPython 3.12 on macOS x86_64/arm64 and Linux x86_64; other interpreters and platforms are not claimed.'
  for rel in \
    assay-python-sdk/README.md \
    docs/python-sdk/index.md \
    docs/getting-started/python-quickstart.md \
    docs/getting-started/installation.md \
    docs/getting-started/index.md \
    docs/guides/troubleshooting.md \
    docs/AIcontext/user-flows.md \
    llms.txt
  do
    printf 'pip install assay-it\n%s\n' "$bound" > "$dest/$rel"
  done
}

GREEN="$TMP/green"
write_green_fixture "$GREEN"

echo "=== live tree ==="
if python3 "$CHECK" --root "$ROOT"; then
  pass "live tree GREEN"
  LIVE_GREEN=1
else
  echo "live tree RED (expected before the source fix)" >&2
  LIVE_GREEN=0
fi

echo "=== fixture GREEN ==="
expect_pass "green fixture" --root "$GREEN"

echo "=== mutation: missing required wheel ==="
python3 - "$TMP/missing.json" <<'PY'
import json
import sys
from pathlib import Path

Path(sys.argv[1]).write_text(
    json.dumps(
        [
            "assay_it-5.4.0-cp312-cp312-macosx_10_12_x86_64.whl",
            "assay_it-5.4.0-cp312-cp312-macosx_11_0_arm64.whl",
        ]
    )
    + "\n"
)
PY
expect_fail "missing required wheel" --root "$GREEN" --published-files "$TMP/missing.json"

echo "=== mutation: widened Requires-Python ==="
cp "$GREEN/assay-python-sdk/pyproject.toml" "$TMP/pyproject.bak"
python3 - "$GREEN/assay-python-sdk/pyproject.toml" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
path.write_text(path.read_text().replace('requires-python = "==3.12.*"', 'requires-python = ">=3.9"'))
PY
expect_fail "widened Requires-Python" --root "$GREEN"
mv "$TMP/pyproject.bak" "$GREEN/assay-python-sdk/pyproject.toml"

echo "=== mutation: PyPy without wheel ==="
cp "$GREEN/assay-python-sdk/pyproject.toml" "$TMP/pyproject.bak"
python3 - "$GREEN/assay-python-sdk/pyproject.toml" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
needle = '"Programming Language :: Python :: Implementation :: CPython",'
insert = needle + '\n    "Programming Language :: Python :: Implementation :: PyPy",'
if needle not in text:
    raise SystemExit('classifier insert point missing')
path.write_text(text.replace(needle, insert, 1))
PY
expect_fail "PyPy classifier without wheel" --root "$GREEN"
mv "$TMP/pyproject.bak" "$GREEN/assay-python-sdk/pyproject.toml"

echo "=== mutation: sdist claim ==="
python3 - "$TMP/sdist.json" <<'PY'
import json
import sys
from pathlib import Path

Path(sys.argv[1]).write_text(
    json.dumps(
        [
            "assay_it-5.4.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
            "assay_it-5.4.0-cp312-cp312-macosx_10_12_x86_64.whl",
            "assay_it-5.4.0-cp312-cp312-macosx_11_0_arm64.whl",
            "assay_it-5.4.0.tar.gz",
        ]
    )
    + "\n"
)
PY
expect_fail "sdist published" --root "$GREEN" --published-files "$TMP/sdist.json"

echo "=== mutation: bare pip-install docs ==="
cp "$GREEN/docs/python-sdk/index.md" "$TMP/docs.bak"
printf 'pip install assay-it\n' > "$GREEN/docs/python-sdk/index.md"
expect_fail "bare pip install docs" --root "$GREEN"
mv "$TMP/docs.bak" "$GREEN/docs/python-sdk/index.md"

echo "=== no-op restore ==="
expect_pass "restored green fixture" --root "$GREEN"

if [ "$LIVE_GREEN" -eq 0 ]; then
  echo "LIVE_TREE_RED"
  exit 2
fi
echo "ALL GREEN"
