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
            "published_support_bound": (
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
                    "os": "macos-15-intel",
                    "target": "x86_64-apple-darwin",
                    "tag": "cp312-cp312-macosx_10_12_x86_64",
                    "import_smoke": "native",
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
                "docs/migration-v1.2.md",
                "llms.txt",
            ],
        },
        indent=2,
    )
    + "\n"
)
PY
  cat > "$dest/assay-python-sdk/pyproject.toml" <<'TOML'
name = "assay-it"
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
pyo3 = { version = "0.29", features = ["extension-module"] }
TOML
  cat > "$dest/.github/workflows/release.yml" <<'YML'
jobs:
  plan-python-artifact:
    outputs:
      python: ${{ steps.plan.outputs.python }}
      abi: ${{ steps.plan.outputs.abi }}
      wheels: ${{ steps.plan.outputs.wheels }}
    steps:
      - id: plan
        run: python3 scripts/ci/plan-python-artifact-matrix.py >> "$GITHUB_OUTPUT"
  wheels:
    name: Build Wheels
    needs: plan-python-artifact
    strategy:
      matrix:
        include: ${{ fromJSON(needs.plan-python-artifact.outputs.wheels) }}
    steps:
      - uses: actions/setup-python@v6
        with:
          python-version: ${{ needs.plan-python-artifact.outputs.python }}
      - name: Build wheels
        uses: PyO3/maturin-action@v1
        with:
          args: --release --out dist --locked -i python${{ needs.plan-python-artifact.outputs.python }} --compatibility pypi
      - name: Smoke the produced wheel
        env:
          ASSAY_WHEEL_TARGET: ${{ matrix.target }}
        run: python3 scripts/ci/smoke-python-wheel.py --dist-dir assay-python-sdk/dist --python "python${{ needs.plan-python-artifact.outputs.python }}"
      - name: Upload wheels
        with:
          path: assay-python-sdk/dist/*.whl
  publish-pypi:
    name: Publish to PyPI
YML
  bound='CPython 3.12 on macOS x86_64/arm64 and Linux x86_64; other interpreters and platforms are not claimed.'
  printf 'The Python wheels cover %s\n' "$bound" > "$dest/README.md"
  for rel in \
    assay-python-sdk/README.md \
    docs/python-sdk/index.md \
    docs/getting-started/python-quickstart.md \
    docs/getting-started/installation.md \
    docs/getting-started/index.md \
    docs/guides/troubleshooting.md \
    docs/AIcontext/user-flows.md \
    docs/migration-v1.2.md \
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

echo "=== mutation: coordinated Requires-Python widen ==="
cp "$GREEN/assay-python-sdk/pyproject.toml" "$TMP/pyproject.bak"
cp "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json" "$TMP/matrix.bak"
python3 - "$GREEN/assay-python-sdk/pyproject.toml" "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json" <<'PY'
from pathlib import Path
import json
import sys

pyproject = Path(sys.argv[1])
pyproject.write_text(
    pyproject.read_text().replace(
        'requires-python = "==3.12.*"', 'requires-python = ">=3.9"'
    )
)
matrix = Path(sys.argv[2])
data = json.loads(matrix.read_text())
data["requires_python"] = ">=3.9"
matrix.write_text(json.dumps(data, indent=2) + "\n")
PY
expect_fail "coordinated Requires-Python widen" --root "$GREEN"
mv "$TMP/pyproject.bak" "$GREEN/assay-python-sdk/pyproject.toml"
mv "$TMP/matrix.bak" "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json"

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

echo "=== mutation: coordinated PyPy with empty forbidden_classifiers ==="
cp "$GREEN/assay-python-sdk/pyproject.toml" "$TMP/pyproject.bak"
cp "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json" "$TMP/matrix.bak"
python3 - "$GREEN/assay-python-sdk/pyproject.toml" "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json" <<'PY'
from pathlib import Path
import json
import sys

pyproject = Path(sys.argv[1])
text = pyproject.read_text()
needle = '"Programming Language :: Python :: Implementation :: CPython",'
insert = needle + '\n    "Programming Language :: Python :: Implementation :: PyPy",'
if needle not in text:
    raise SystemExit("classifier insert point missing")
pyproject.write_text(text.replace(needle, insert, 1))
matrix = Path(sys.argv[2])
data = json.loads(matrix.read_text())
data["forbidden_classifiers"] = []
matrix.write_text(json.dumps(data, indent=2) + "\n")
PY
expect_fail "coordinated PyPy with empty forbidden_classifiers" --root "$GREEN"
mv "$TMP/pyproject.bak" "$GREEN/assay-python-sdk/pyproject.toml"
mv "$TMP/matrix.bak" "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json"

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

echo "=== mutation: drop one kernel-matrix artifact-truth path ==="
cp "$ROOT/.github/workflows/kernel-matrix.yml" "$GREEN/.github/workflows/kernel-matrix.yml"
python3 - "$GREEN/.github/workflows/kernel-matrix.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
old = '      - "llms.txt"\n'
if old not in text:
    raise SystemExit("llms.txt path entry missing")
path.write_text(text.replace(old, "", 1))
PY
expect_fail "dropped kernel-matrix artifact-truth path" --root "$GREEN"
rm -f "$GREEN/.github/workflows/kernel-matrix.yml"

echo "=== mutation: drop migration-v1.2.md from install_docs ==="
cp "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json" "$TMP/matrix.bak"
python3 - "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json" <<'PY'
import json
from pathlib import Path
import sys

path = Path(sys.argv[1])
data = json.loads(path.read_text())
data["install_docs"] = [rel for rel in data["install_docs"] if rel != "docs/migration-v1.2.md"]
path.write_text(json.dumps(data, indent=2) + "\n")
PY
expect_fail "drop migration-v1.2.md from install_docs" --root "$GREEN"
mv "$TMP/matrix.bak" "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json"

echo "=== live install_docs includes migration-v1.2.md ==="
python3 - "$ROOT/assay-python-sdk/python-artifact-matrix.v0.json" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())
if "docs/migration-v1.2.md" not in data.get("install_docs", []):
    raise SystemExit("live install_docs missing docs/migration-v1.2.md")
PY
if [ $? -ne 0 ]; then
  fail_test "live install_docs missing docs/migration-v1.2.md"
fi
pass "live install_docs includes docs/migration-v1.2.md"

echo "=== mutation: coherent 3.13 matrix+pyproject+tags, release still 3.12 ==="
cp "$GREEN/assay-python-sdk/pyproject.toml" "$TMP/pyproject.bak"
cp "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json" "$TMP/matrix.bak"
cp "$GREEN/.github/workflows/release.yml" "$TMP/release.bak"
python3 - "$GREEN/assay-python-sdk/pyproject.toml" \
  "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json" \
  "$GREEN/.github/workflows/release.yml" <<'PY'
from pathlib import Path
import json
import sys

pyproject = Path(sys.argv[1])
pyproject.write_text(
    pyproject.read_text()
    .replace('requires-python = "==3.12.*"', 'requires-python = "==3.13.*"')
    .replace("Programming Language :: Python :: 3.12", "Programming Language :: Python :: 3.13")
)
matrix = Path(sys.argv[2])
data = json.loads(matrix.read_text())
data["requires_python"] = "==3.13.*"
data["required_classifiers"] = [
    "Programming Language :: Python :: 3",
    "Programming Language :: Python :: 3.13",
    "Programming Language :: Python :: Implementation :: CPython",
]
for wheel in data["wheels"]:
    wheel["tag"] = wheel["tag"].replace("cp312", "cp313")
data["support_bound"] = (
    "CPython 3.13 on macOS x86_64/arm64 and Linux x86_64; "
    "other interpreters and platforms are not claimed."
)
matrix.write_text(json.dumps(data, indent=2) + "\n")
release = Path(sys.argv[3])
text = release.read_text()
text = text.replace(
    "${{ needs.plan-python-artifact.outputs.python }}",
    "3.12",
)
text = text.replace(
    "${{ needs.plan-python-artifact.outputs.python_bin }}",
    "python3.12",
)
if "python-version: '3.12'" not in text and 'python-version: "3.12"' not in text:
    text = text.replace("python-version:", "python-version: '3.12' #", 1)
if "-i python3.12" not in text:
    text = text.replace("-i ", "-i python3.12 ", 1)
release.write_text(text)
PY
expect_fail "coherent 3.13 matrix+pyproject+tags, release still 3.12" --root "$GREEN"
mv "$TMP/pyproject.bak" "$GREEN/assay-python-sdk/pyproject.toml"
mv "$TMP/matrix.bak" "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json"
mv "$TMP/release.bak" "$GREEN/.github/workflows/release.yml"

echo "=== mutation: requires_python >=3.9 + a cp39 tag ==="
cp "$GREEN/assay-python-sdk/pyproject.toml" "$TMP/pyproject.bak"
cp "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json" "$TMP/matrix.bak"
python3 - "$GREEN/assay-python-sdk/pyproject.toml" \
  "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json" <<'PY'
from pathlib import Path
import json
import sys

pyproject = Path(sys.argv[1])
pyproject.write_text(
    pyproject.read_text().replace('requires-python = "==3.12.*"', 'requires-python = ">=3.9"')
)
matrix = Path(sys.argv[2])
data = json.loads(matrix.read_text())
data["requires_python"] = ">=3.9"
tag = data["wheels"][0]["tag"]
data["wheels"][0]["tag"] = tag.replace("cp312-cp312", "cp39-cp39", 1)
matrix.write_text(json.dumps(data, indent=2) + "\n")
PY
expect_fail "requires_python >=3.9 + a cp39 tag" --root "$GREEN"
mv "$TMP/pyproject.bak" "$GREEN/assay-python-sdk/pyproject.toml"
mv "$TMP/matrix.bak" "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json"

echo "=== mutation: matrix package assay-py, smoke/pyproject still assay-it ==="
cp "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json" "$TMP/matrix.bak"
python3 - "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json" <<'PY'
from pathlib import Path
import json
import sys

path = Path(sys.argv[1])
data = json.loads(path.read_text())
data["package"] = "assay-py"
path.write_text(json.dumps(data, indent=2) + "\n")
PY
expect_fail "matrix package assay-py while pyproject/smoke stay assay-it" --root "$GREEN"
mv "$TMP/matrix.bak" "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json"

echo "=== mutation: drop troubleshooting.md from kernel-matrix paths ==="
cp "$ROOT/.github/workflows/kernel-matrix.yml" "$GREEN/.github/workflows/kernel-matrix.yml"
python3 - "$GREEN/.github/workflows/kernel-matrix.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
old = '      - "docs/guides/troubleshooting.md"\n'
if old not in text:
    raise SystemExit("troubleshooting.md path entry missing")
path.write_text(text.replace(old, "", 1))
PY
expect_fail "dropped kernel-matrix path docs/guides/troubleshooting.md" --root "$GREEN"
rm -f "$GREEN/.github/workflows/kernel-matrix.yml"

echo "=== mutation: drop user-flows.md from kernel-matrix paths ==="
cp "$ROOT/.github/workflows/kernel-matrix.yml" "$GREEN/.github/workflows/kernel-matrix.yml"
python3 - "$GREEN/.github/workflows/kernel-matrix.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
old = '      - "docs/AIcontext/user-flows.md"\n'
if old not in text:
    raise SystemExit("user-flows.md path entry missing")
path.write_text(text.replace(old, "", 1))
PY
expect_fail "dropped kernel-matrix path docs/AIcontext/user-flows.md" --root "$GREEN"
rm -f "$GREEN/.github/workflows/kernel-matrix.yml"

echo "=== mutation: drop migration-v1.2.md from pre-commit files selector ==="
cp "$ROOT/.pre-commit-config.yaml" "$GREEN/.pre-commit-config.yaml"
python3 - "$GREEN/.pre-commit-config.yaml" <<'PY'
from pathlib import Path
import re
import sys

path = Path(sys.argv[1])
text = path.read_text()
pattern = re.compile(
    r"(^      - id: python-artifact-truth\n(?:.*\n)*?        files: )(\S+)(\s*$)",
    re.M,
)
match = pattern.search(text)
if not match:
    raise SystemExit("python-artifact-truth files: selector missing")
files = match.group(2)
if "migration-v1" not in files:
    raise SystemExit("pre-commit files selector missing migration-v1.2.md")
mutated = files.replace("|migration-v1\\.2\\.md", "")
mutated = mutated.replace("migration-v1\\.2\\.md|", "")
if mutated == files:
    raise SystemExit("could not drop migration-v1.2.md from " + files)
path.write_text(text[: match.start(2)] + mutated + text[match.end(2) :])
PY
expect_fail "dropped pre-commit selector path docs/migration-v1.2.md" --root "$GREEN"
rm -f "$GREEN/.pre-commit-config.yaml"

echo "=== mutation: drop kernel-matrix.yml from pre-commit files selector ==="
cp "$ROOT/.pre-commit-config.yaml" "$GREEN/.pre-commit-config.yaml"
python3 - "$GREEN/.pre-commit-config.yaml" <<'PY'
from pathlib import Path
import re
import sys

path = Path(sys.argv[1])
text = path.read_text()
pattern = re.compile(
    r"(^      - id: python-artifact-truth\n(?:.*\n)*?        files: )(\S+)(\s*$)",
    re.M,
)
match = pattern.search(text)
if not match:
    raise SystemExit("python-artifact-truth files: selector missing")
files = match.group(2)
if "kernel-matrix" not in files:
    raise SystemExit("pre-commit files selector missing kernel-matrix.yml")
mutated = files.replace("(release|kernel-matrix)", "release")
mutated = mutated.replace("|kernel-matrix\\.yml", "")
mutated = mutated.replace("kernel-matrix\\.yml|", "")
if mutated == files:
    raise SystemExit("could not drop kernel-matrix.yml from " + files)
path.write_text(text[: match.start(2)] + mutated + text[match.end(2) :])
PY
expect_fail "dropped pre-commit selector path kernel-matrix.yml" --root "$GREEN"
rm -f "$GREEN/.pre-commit-config.yaml"

echo "=== mutation: drop getting-started/index.md from pre-commit files selector ==="
cp "$ROOT/.pre-commit-config.yaml" "$GREEN/.pre-commit-config.yaml"
python3 - "$GREEN/.pre-commit-config.yaml" <<'PY'
from pathlib import Path
import re
import sys

path = Path(sys.argv[1])
text = path.read_text()
pattern = re.compile(
    r"^(\s+- id: python-artifact-truth[\s\S]*?^\s+files:\s+)(\S+)",
    re.MULTILINE,
)
match = pattern.search(text)
if not match:
    raise SystemExit("python-artifact-truth files: selector missing")
files = match.group(2)
if "getting-started/(index|" not in files and "getting-started/(index)" not in files:
    raise SystemExit("pre-commit files selector missing getting-started index.md")
mutated = files.replace("index|", "")
if mutated == files:
    raise SystemExit("could not drop index from " + files)
path.write_text(text[: match.start(2)] + mutated + text[match.end(2) :])
PY
expect_fail "dropped pre-commit selector path docs/getting-started/index.md" --root "$GREEN"
rm -f "$GREEN/.pre-commit-config.yaml"

echo "=== mutation: drop README.md from pre-commit files selector ==="
cp "$ROOT/.pre-commit-config.yaml" "$GREEN/.pre-commit-config.yaml"
python3 - "$GREEN/.pre-commit-config.yaml" <<'PY'
from pathlib import Path
import re
import sys

path = Path(sys.argv[1])
text = path.read_text()
pattern = re.compile(
    r"^(\s+- id: python-artifact-truth[\s\S]*?^\s+files:\s+)(\S+)",
    re.MULTILINE,
)
match = pattern.search(text)
if not match:
    raise SystemExit("python-artifact-truth files: selector missing")
files = match.group(2)
if "|README.md" not in files and "|README\\.md" not in files:
    raise SystemExit("pre-commit files selector missing README.md")
mutated = files.replace("|README\\.md", "")
if mutated == files:
    raise SystemExit("could not drop README.md from " + files)
path.write_text(text[: match.start(2)] + mutated + text[match.end(2) :])
PY
expect_fail "dropped pre-commit selector path README.md" --root "$GREEN"
rm -f "$GREEN/.pre-commit-config.yaml"

echo "=== mutation: listed install-doc claims Python 3.12+ ==="
cp "$GREEN/docs/python-sdk/index.md" "$TMP/docs.bak"
python3 - "$GREEN/docs/python-sdk/index.md" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
path.write_text(path.read_text() + "Requires Python 3.12+.\n")
PY
expect_fail "install-doc claims Python 3.12+" --root "$GREEN"
mv "$TMP/docs.bak" "$GREEN/docs/python-sdk/index.md"

echo "=== mutation: listed install-doc claims 3.13+ ==="
cp "$GREEN/docs/python-sdk/index.md" "$TMP/docs.bak"
python3 - "$GREEN/docs/python-sdk/index.md" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
path.write_text(path.read_text() + "Supports 3.13+.\n")
PY
expect_fail "install-doc claims 3.13+" --root "$GREEN"
mv "$TMP/docs.bak" "$GREEN/docs/python-sdk/index.md"

echo "=== mutation: listed install-doc claims 3.10 and later ==="
cp "$GREEN/docs/python-sdk/index.md" "$TMP/docs.bak"
python3 - "$GREEN/docs/python-sdk/index.md" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
path.write_text(path.read_text() + "Python 3.10 and later.\n")
PY
expect_fail "install-doc claims 3.10 and later" --root "$GREEN"
mv "$TMP/docs.bak" "$GREEN/docs/python-sdk/index.md"

echo "=== mutation: support_bound drifts from declared wheels ==="
cp "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json" "$TMP/matrix.bak"
python3 - "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json" <<'PY'
from pathlib import Path
import json
import sys

path = Path(sys.argv[1])
data = json.loads(path.read_text())
data["support_bound"] = (
    "CPython 3.13 on macOS x86_64/arm64 and Linux x86_64; "
    "other interpreters and platforms are not claimed."
)
path.write_text(json.dumps(data, indent=2) + "\n")
PY
expect_fail "support_bound drifts from declared wheels" --root "$GREEN"
mv "$TMP/matrix.bak" "$GREEN/assay-python-sdk/python-artifact-matrix.v0.json"

echo "=== #3063 abi3 design cases ==="
ABI3="$TMP/abi3"
cp -a "$GREEN" "$ABI3"
python3 - "$ABI3/assay-python-sdk/python-artifact-matrix.v0.json" \
  "$ABI3/assay-python-sdk/pyproject.toml" \
  "$ABI3/assay-python-sdk/Cargo.toml" \
  "$ABI3/.github/workflows/release.yml" <<'PY'
from pathlib import Path
import json
import sys

matrix_path = Path(sys.argv[1])
pyproject_path = Path(sys.argv[2])
cargo_path = Path(sys.argv[3])
release_path = Path(sys.argv[4])

matrix = json.loads(matrix_path.read_text())
matrix["requires_python"] = ">=3.12"
matrix["abi"] = "abi3"
matrix["smoke_pythons"] = ["3.12", "3.13", "3.14"]
matrix["required_classifiers"] = [
    "Programming Language :: Python :: 3",
    "Programming Language :: Python :: 3.12",
    "Programming Language :: Python :: 3.13",
    "Programming Language :: Python :: 3.14",
    "Programming Language :: Python :: Implementation :: CPython",
]
matrix["support_bound"] = (
    "CPython 3.12, 3.13, and 3.14 on macOS x86_64/arm64 and Linux x86_64; "
    "other interpreters and platforms are not claimed."
)
matrix["published_support_bound"] = matrix["support_bound"]
for wheel in matrix["wheels"]:
    wheel["tag"] = wheel["tag"].replace("cp312-cp312", "cp312-abi3", 1)
matrix_path.write_text(json.dumps(matrix, indent=2) + "\n")

pyproject = pyproject_path.read_text()
pyproject = pyproject.replace('requires-python = "==3.12.*"', 'requires-python = ">=3.12"', 1)
pyproject = pyproject.replace(
    '"Programming Language :: Python :: 3.12",',
    '"Programming Language :: Python :: 3.12",\n'
    '    "Programming Language :: Python :: 3.13",\n'
    '    "Programming Language :: Python :: 3.14",',
    1,
)
pyproject_path.write_text(pyproject)

cargo_path.write_text(
    "[dependencies]\n"
    'pyo3 = { version = "0.29", features = ["extension-module", "abi3-py312"] }\n'
)

release = release_path.read_text()
old_setup = """      - uses: actions/setup-python@v6
        with:
          python-version: ${{ needs.plan-python-artifact.outputs.python }}
"""
new_setup = """      - uses: actions/setup-python@v6
        with:
          python-version: ${{ join(matrix.smoke_pythons, '\\n') }}
"""
if old_setup in release:
    release = release.replace(old_setup, new_setup, 1)
old_smoke = """      - name: Smoke the produced wheel
        env:
          ASSAY_WHEEL_TARGET: ${{ matrix.target }}
        run: python3 scripts/ci/smoke-python-wheel.py --dist-dir assay-python-sdk/dist --python "python${{ needs.plan-python-artifact.outputs.python }}"
"""
new_smoke = """      - name: Smoke the produced wheel
        env:
          ASSAY_WHEEL_TARGET: ${{ matrix.target }}
        run: |
          set -euo pipefail
          for py in ${{ join(matrix.smoke_pythons, ' ') }}; do
            python3 scripts/ci/smoke-python-wheel.py --dist-dir assay-python-sdk/dist --python "python${py}"
          done
"""
if old_smoke in release:
    release = release.replace(old_smoke, new_smoke, 1)
release_path.write_text(release)

old_bound = "CPython 3.12 on macOS x86_64/arm64 and Linux x86_64; other interpreters and platforms are not claimed."
new_bound = matrix["support_bound"]
for rel in matrix["install_docs"]:
    path = matrix_path.parent.parent / rel
    path.write_text(path.read_text().replace(old_bound, new_bound))
readme_path = matrix_path.parent.parent / "README.md"
if readme_path.is_file():
    readme_path.write_text(readme_path.read_text().replace(old_bound, new_bound))
PY

expect_pass "abi3 positive control" --root "$ABI3"

echo "=== #3063 case: abi3-py311 with >=3.12 ==="
cp "$ABI3/assay-python-sdk/Cargo.toml" "$TMP/abi3-cargo.bak"
python3 - "$ABI3/assay-python-sdk/Cargo.toml" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
path.write_text(path.read_text().replace("abi3-py312", "abi3-py311", 1))
PY
expect_fail "abi3-py311 with >=3.12" --root "$ABI3"
mv "$TMP/abi3-cargo.bak" "$ABI3/assay-python-sdk/Cargo.toml"

echo "=== #3063 case: bare abi3 feature ==="
cp "$ABI3/assay-python-sdk/Cargo.toml" "$TMP/abi3-cargo.bak"
python3 - "$ABI3/assay-python-sdk/Cargo.toml" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
path.write_text(path.read_text().replace("abi3-py312", "abi3", 1))
PY
expect_fail "bare abi3 feature" --root "$ABI3"
mv "$TMP/abi3-cargo.bak" "$ABI3/assay-python-sdk/Cargo.toml"

echo "=== #3063 case: abi3t feature ==="
cp "$ABI3/assay-python-sdk/Cargo.toml" "$TMP/abi3-cargo.bak"
python3 - "$ABI3/assay-python-sdk/Cargo.toml" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
path.write_text(path.read_text().replace("abi3-py312", "abi3t", 1))
PY
expect_fail "abi3t feature" --root "$ABI3"
mv "$TMP/abi3-cargo.bak" "$ABI3/assay-python-sdk/Cargo.toml"

echo "=== #3063 case: abi3 tags with ==3.12.* ==="
cp "$ABI3/assay-python-sdk/pyproject.toml" "$TMP/abi3-pyproject.bak"
cp "$ABI3/assay-python-sdk/python-artifact-matrix.v0.json" "$TMP/abi3-matrix.bak"
python3 - "$ABI3/assay-python-sdk/pyproject.toml" "$ABI3/assay-python-sdk/python-artifact-matrix.v0.json" <<'PY'
from pathlib import Path
import json
import sys

pyproject = Path(sys.argv[1])
matrix = Path(sys.argv[2])
pyproject.write_text(pyproject.read_text().replace('requires-python = ">=3.12"', 'requires-python = "==3.12.*"', 1))
data = json.loads(matrix.read_text())
data["requires_python"] = "==3.12.*"
matrix.write_text(json.dumps(data, indent=2) + "\n")
PY
expect_fail "abi3 tags with ==3.12.*" --root "$ABI3"
mv "$TMP/abi3-pyproject.bak" "$ABI3/assay-python-sdk/pyproject.toml"
mv "$TMP/abi3-matrix.bak" "$ABI3/assay-python-sdk/python-artifact-matrix.v0.json"

echo "=== #3063 case: >=3.12 with cp312-cp312 tags ==="
cp "$ABI3/assay-python-sdk/python-artifact-matrix.v0.json" "$TMP/abi3-matrix.bak"
python3 - "$ABI3/assay-python-sdk/python-artifact-matrix.v0.json" <<'PY'
from pathlib import Path
import json
import sys

path = Path(sys.argv[1])
data = json.loads(path.read_text())
for wheel in data["wheels"]:
    wheel["tag"] = wheel["tag"].replace("-abi3-", "-cp312-", 1)
path.write_text(json.dumps(data, indent=2) + "\n")
PY
expect_fail ">=3.12 with cp312-cp312 tags" --root "$ABI3"
mv "$TMP/abi3-matrix.bak" "$ABI3/assay-python-sdk/python-artifact-matrix.v0.json"

echo "=== #3063 case: cp313-abi3 with min 3.12 ==="
cp "$ABI3/assay-python-sdk/python-artifact-matrix.v0.json" "$TMP/abi3-matrix.bak"
python3 - "$ABI3/assay-python-sdk/python-artifact-matrix.v0.json" <<'PY'
from pathlib import Path
import json
import sys

path = Path(sys.argv[1])
data = json.loads(path.read_text())
data["wheels"][0]["tag"] = data["wheels"][0]["tag"].replace("cp312-abi3", "cp313-abi3", 1)
path.write_text(json.dumps(data, indent=2) + "\n")
PY
expect_fail "cp313-abi3 with minimum 3.12" --root "$ABI3"
mv "$TMP/abi3-matrix.bak" "$ABI3/assay-python-sdk/python-artifact-matrix.v0.json"

echo "=== #3063 case: smoke Python without matching classifier ==="
cp "$ABI3/assay-python-sdk/pyproject.toml" "$TMP/abi3-pyproject.bak"
python3 - "$ABI3/assay-python-sdk/pyproject.toml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
path.write_text(path.read_text().replace('    "Programming Language :: Python :: 3.14",\n', "", 1))
PY
expect_fail "smoke Python 3.14 without classifier" --root "$ABI3"
mv "$TMP/abi3-pyproject.bak" "$ABI3/assay-python-sdk/pyproject.toml"

echo "=== #3065 published_support_bound split cases ==="
TREE_BOUND='CPython 3.12, 3.13, and 3.14 on macOS x86_64/arm64 and Linux x86_64; other interpreters and platforms are not claimed.'
PUBLISHED_BOUND='CPython 3.12 on macOS x86_64/arm64 and Linux x86_64; other interpreters and platforms are not claimed.'
ABI3_POST_RELEASE="$TMP/abi3-post-release"
cp -a "$ABI3" "$ABI3_POST_RELEASE"
expect_pass "published == support_bound (post-release state)" --root "$ABI3_POST_RELEASE"

cp "$ABI3_POST_RELEASE/assay-python-sdk/python-artifact-matrix.v0.json" "$TMP/published-bound-matrix.bak"
python3 - "$ABI3_POST_RELEASE/assay-python-sdk/python-artifact-matrix.v0.json" <<'PY'
from pathlib import Path
import json
import sys

path = Path(sys.argv[1])
data = json.loads(path.read_text())
data.pop("published_support_bound", None)
path.write_text(json.dumps(data, indent=2) + "\n")
PY
expect_fail "published_support_bound missing" --root "$ABI3_POST_RELEASE"
mv "$TMP/published-bound-matrix.bak" "$ABI3_POST_RELEASE/assay-python-sdk/python-artifact-matrix.v0.json"

cp "$ABI3_POST_RELEASE/assay-python-sdk/python-artifact-matrix.v0.json" "$TMP/published-bound-matrix.bak"
python3 - "$ABI3_POST_RELEASE/assay-python-sdk/python-artifact-matrix.v0.json" <<'PY'
from pathlib import Path
import json
import sys

path = Path(sys.argv[1])
data = json.loads(path.read_text())
data["published_support_bound"] = (
    "CPython 3.12 and 3.15 on macOS x86_64/arm64 and Linux x86_64; "
    "other interpreters and platforms are not claimed."
)
path.write_text(json.dumps(data, indent=2) + "\n")
PY
expect_fail "published_support_bound names 3.15" --root "$ABI3_POST_RELEASE"
mv "$TMP/published-bound-matrix.bak" "$ABI3_POST_RELEASE/assay-python-sdk/python-artifact-matrix.v0.json"

cp "$ABI3_POST_RELEASE/assay-python-sdk/python-artifact-matrix.v0.json" "$TMP/published-bound-matrix.bak"
python3 - "$ABI3_POST_RELEASE/assay-python-sdk/python-artifact-matrix.v0.json" <<'PY'
from pathlib import Path
import json
import sys

path = Path(sys.argv[1])
data = json.loads(path.read_text())
data["published_support_bound"] = (
    "CPython 3.12 on macOS x86_64/arm64 and Linux arm64; "
    "other interpreters and platforms are not claimed."
)
path.write_text(json.dumps(data, indent=2) + "\n")
PY
expect_fail "published_support_bound platform text drifts" --root "$ABI3_POST_RELEASE"
mv "$TMP/published-bound-matrix.bak" "$ABI3_POST_RELEASE/assay-python-sdk/python-artifact-matrix.v0.json"

ABI3_PRE_RELEASE="$TMP/abi3-pre-release"
cp -a "$ABI3" "$ABI3_PRE_RELEASE"
python3 - "$ABI3_PRE_RELEASE" "$TREE_BOUND" "$PUBLISHED_BOUND" <<'PY'
from pathlib import Path
import json
import sys

root = Path(sys.argv[1])
tree_bound = sys.argv[2]
published_bound = sys.argv[3]
matrix_path = root / "assay-python-sdk/python-artifact-matrix.v0.json"
data = json.loads(matrix_path.read_text())
data["published_support_bound"] = published_bound
matrix_path.write_text(json.dumps(data, indent=2) + "\n")
for rel in data["install_docs"]:
    doc_path = root / rel
    doc_path.write_text(doc_path.read_text().replace(tree_bound, published_bound))
readme_path = root / "README.md"
if readme_path.is_file():
    readme_path.write_text(readme_path.read_text().replace(tree_bound, published_bound))
PY

python3 - "$ABI3_PRE_RELEASE" "$PUBLISHED_BOUND" "$TREE_BOUND" <<'PY'
from pathlib import Path
import json
import sys

root = Path(sys.argv[1])
published_bound = sys.argv[2]
tree_bound = sys.argv[3]
matrix_path = root / "assay-python-sdk/python-artifact-matrix.v0.json"
data = json.loads(matrix_path.read_text())
for rel in data["install_docs"]:
    doc_path = root / rel
    doc_path.write_text(doc_path.read_text().replace(published_bound, tree_bound))
readme_path = root / "README.md"
if readme_path.is_file():
    readme_path.write_text(readme_path.read_text().replace(published_bound, tree_bound))
PY
expect_fail "doc carries tree sentence (3.12, 3.13, and 3.14) instead of published sentence" --root "$ABI3_PRE_RELEASE"

echo "=== mutation: README Python support bound drifts ==="
cp "$GREEN/README.md" "$TMP/readme.bak"
printf 'The Python wheels cover CPython 3.11 on macOS x86_64/arm64 and Linux x86_64; other interpreters and platforms are not claimed.\n' > "$GREEN/README.md"
expect_fail "README Python support bound drifts" --root "$GREEN"
mv "$TMP/readme.bak" "$GREEN/README.md"

echo "=== mutation: docs/getting-started/index.md interpreter bound drifts without pip install ==="
cp "$GREEN/docs/getting-started/index.md" "$TMP/index.bak"
cat > "$GREEN/docs/getting-started/index.md" <<'EOF'
# Getting Started
## Prerequisites
- CPython 3.11 on macOS x86_64/arm64 and Linux x86_64; other interpreters and platforms are not claimed.
EOF
expect_fail "docs/getting-started/index.md without pip install has drifted bound" --root "$GREEN"
mv "$TMP/index.bak" "$GREEN/docs/getting-started/index.md"

echo "=== mutation: docs/getting-started/index.md has stale SDK prerequisite claim ==="
cp "$GREEN/docs/getting-started/index.md" "$TMP/index.bak"
cat > "$GREEN/docs/getting-started/index.md" <<'EOF'
# Getting Started
## Prerequisites
- **Rust 1.96** for repository development, or CPython 3.12 for Python SDK use
- CPython 3.12 on macOS x86_64/arm64 and Linux x86_64; other interpreters and platforms are not claimed.
EOF
expect_fail "docs/getting-started/index.md has stale SDK prerequisite claim" --root "$GREEN"
mv "$TMP/index.bak" "$GREEN/docs/getting-started/index.md"

echo "=== no-op restore ==="
expect_pass "restored green fixture" --root "$GREEN"

if [ "$LIVE_GREEN" -eq 0 ]; then
  echo "LIVE_TREE_RED"
  exit 2
fi
echo "ALL GREEN"
