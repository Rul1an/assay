#!/usr/bin/env bash
set -euo pipefail

GUARD="scripts/ci/check-mcp-policy-reader-routing.py"
FILES=(
  crates/assay-mcp-server/src/tools
)

python3 "$GUARD"
echo "PASS: reader guard accepts the production routes"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
mkdir -p "$TMPDIR/base/crates/assay-mcp-server/src"
cp -R "${FILES[0]}" "$TMPDIR/base/crates/assay-mcp-server/src/"

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
    echo "FAIL: reader guard accepted mutation: $name" >&2
    exit 1
  fi
  if ! printf '%s\n' "$output" | grep -q '^FAIL: '; then
    echo "FAIL: reader guard exited $status without a FAIL reason: $name" >&2
    printf '%s\n' "$output" >&2
    exit 1
  fi
  echo "PASS: reader guard rejects $name"
}

mutate_metadata_accept() {
  python3 - "$1/crates/assay-mcp-server/src/tools/policy_read.rs" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1]); s = p.read_text()
old = "let file = File::open(&path)?;"
new = """let file = File::open(&path)?;
            if file.metadata()?.len() as usize <= limit {
                let mut bytes = Vec::new();
                std::io::Read::read_to_end(&mut file, &mut bytes)?;
                return Ok(bytes);
            }"""
assert old in s
p.write_text(s.replace(old, new, 1))
PY
}

mutate_direct_tokio() {
  python3 - "$1/crates/assay-mcp-server/src/tools/policy_decide.rs" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1]); s = p.read_text()
old = "ctx.read_policy_bounded(policy_rel_path).await"
new = "tokio::fs::read(&policy_path).await"
assert old in s
p.write_text(s.replace(old, new, 1))
PY
}

mutate_check_args_from_file() {
  python3 - "$1/crates/assay-mcp-server/src/tools/check_args.rs" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1]); s = p.read_text()
old = "McpPolicy::from_slice(&policy_bytes)"
new = "McpPolicy::from_file(&policy_path)"
assert old in s
p.write_text(s.replace(old, new, 1))
PY
}

mutate_skip_helper() {
  python3 - "$1/crates/assay-mcp-server/src/tools/policy_read.rs" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1]); s = p.read_text()
old = "read_bounded(file, limit)"
new = "std::io::Read::read_to_end(&mut file, &mut Vec::new()).map(|_| Vec::new())"
assert old in s
p.write_text(s.replace(old, new, 1))
PY
}

mutant "metadata-only accept then unbounded read" mutate_metadata_accept
mutant "direct tokio::fs::read bypass" mutate_direct_tokio
mutant "check_args from_file bypass" mutate_check_args_from_file
mutant "async entry skips read_bounded" mutate_skip_helper

echo "All MCP policy reader routing mutations caught."
