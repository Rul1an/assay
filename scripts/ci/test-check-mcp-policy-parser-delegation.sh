#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GUARD="$SCRIPT_DIR/check-mcp-policy-parser-delegation.py"
FILES=(
  crates/assay-core/src/mcp/policy/mod.rs
  crates/assay-core/src/mcp/policy/legacy.rs
)

if ! python3 "$GUARD"; then
  echo "FAIL: parser guard must accept zero args from the allowlisted cwd" >&2
  exit 1
fi
echo "PASS: parser guard accepts zero args from the allowlisted cwd"

if python3 "$GUARD" extra 2>/dev/null; then
  echo "FAIL: parser guard must reject an extra argument" >&2
  exit 1
fi
echo "PASS: parser guard rejects an extra argument"

if python3 "$GUARD" --stdin </dev/null 2>/dev/null; then
  echo "FAIL: parser guard must reject --stdin" >&2
  exit 1
fi
echo "PASS: parser guard rejects --stdin"

if python3 "$GUARD" . 2>/dev/null; then
  echo "FAIL: parser guard must reject a positional path" >&2
  exit 1
fi
echo "PASS: parser guard rejects a positional path"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
mkdir -p "$TMPDIR/base/crates/assay-core/src/mcp/policy"
cp "${FILES[0]}" "$TMPDIR/base/crates/assay-core/src/mcp/policy/"
cp "${FILES[1]}" "$TMPDIR/base/crates/assay-core/src/mcp/policy/"

if ! (
  cd "$TMPDIR/base"
  python3 "$GUARD"
); then
  echo "FAIL: unmutated parser scratch copy must pass before mutations are scored" >&2
  exit 1
fi
echo "PASS: unmutated parser scratch copy passes from its own cwd"

mutant() {
  local name=$1
  local mutation=$2
  rm -rf "$TMPDIR/current"
  cp -R "$TMPDIR/base" "$TMPDIR/current"
  "$mutation" "$TMPDIR/current"
  local output status
  set +e
  output=$(
    cd "$TMPDIR/current"
    python3 "$GUARD" 2>&1
  )
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
old = "    McpPolicy::from_slice(&bytes)"
new = """    let _ = serde_yaml::from_slice::<serde_yaml::Value>(&bytes);
    McpPolicy::from_slice(&bytes)"""
assert s.count(old) == 1
p.write_text(s.replace(old, new, 1))
PY
}

mutant "public from_slice duplicates deserializer" mutate_duplicate_deserializer
mutant "legacy from_file reparses" mutate_from_file_reparse

echo "All MCP policy parser delegation mutations caught."
