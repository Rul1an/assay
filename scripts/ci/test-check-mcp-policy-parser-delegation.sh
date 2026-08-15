#!/usr/bin/env bash
set -euo pipefail

GUARD="scripts/ci/check-mcp-policy-parser-delegation.py"
FILES=(
  crates/assay-core/src/mcp/policy/mod.rs
  crates/assay-core/src/mcp/policy/legacy.rs
)

python3 "$GUARD"
echo "PASS: parser guard accepts the production hops"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
mkdir -p "$TMPDIR/base/crates/assay-core/src/mcp/policy"
cp "${FILES[0]}" "$TMPDIR/base/crates/assay-core/src/mcp/policy/"
cp "${FILES[1]}" "$TMPDIR/base/crates/assay-core/src/mcp/policy/"

mutant() {
  local name=$1
  local mutation=$2
  rm -rf "$TMPDIR/current"
  cp -R "$TMPDIR/base" "$TMPDIR/current"
  "$mutation" "$TMPDIR/current"
  local output status
  set +e
  output=$(python3 "$GUARD" "$TMPDIR/current" 2>&1)
  status=$?
  set -e
  if [ "$status" -eq 0 ]; then
    echo "FAIL: parser guard accepted mutation: $name" >&2
    exit 1
  fi
  if ! printf '%s\n' "$output" | grep -q '^FAIL: '; then
    echo "FAIL: parser guard exited $status without a FAIL reason: $name" >&2
    printf '%s\n' "$output" >&2
    exit 1
  fi
  echo "PASS: parser guard rejects $name"
}

mutate_duplicate_deserializer() {
  python3 - "$1/crates/assay-core/src/mcp/policy/mod.rs" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1]); s = p.read_text()
old = "pub fn from_slice(bytes: &[u8]) -> anyhow::Result<Self> {\n        legacy::from_slice(bytes)\n    }"
new = """pub fn from_slice(bytes: &[u8]) -> anyhow::Result<Self> {
        let _ = serde_yaml::from_slice::<serde_yaml::Value>(bytes);
        legacy::from_slice(bytes)
    }"""
assert old in s
p.write_text(s.replace(old, new, 1))
PY
}

mutate_from_file_reparse() {
  python3 - "$1/crates/assay-core/src/mcp/policy/legacy.rs" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1]); s = p.read_text()
old = "let bytes = std::fs::read(path)?;\n    McpPolicy::from_slice(&bytes)"
new = """let bytes = std::fs::read(path)?;
    let _ = serde_yaml::from_slice::<serde_yaml::Value>(&bytes);
    McpPolicy::from_slice(&bytes)"""
assert old in s
p.write_text(s.replace(old, new, 1))
PY
}

mutant "public from_slice duplicates deserializer" mutate_duplicate_deserializer
mutant "legacy from_file reparses" mutate_from_file_reparse

echo "All MCP policy parser delegation mutations caught."
