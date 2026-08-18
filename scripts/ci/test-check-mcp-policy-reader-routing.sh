#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GUARD="$SCRIPT_DIR/check-mcp-policy-reader-routing.py"
FILES=(
  crates/assay-mcp-server/src/tools
  crates/assay-mcp-server/src/config.rs
  crates/assay-mcp-server/src/main.rs
  crates/assay-mcp-server/src/policy_byte_limit.rs
)

if ! python3 "$GUARD"; then
  echo "FAIL: reader guard must accept zero args from the allowlisted cwd" >&2
  exit 1
fi
echo "PASS: reader guard accepts zero args from the allowlisted cwd"

if python3 "$GUARD" extra 2>/dev/null; then
  echo "FAIL: reader guard must reject an extra argument" >&2
  exit 1
fi
echo "PASS: reader guard rejects an extra argument"

if python3 "$GUARD" --stdin </dev/null 2>/dev/null; then
  echo "FAIL: reader guard must reject --stdin" >&2
  exit 1
fi
echo "PASS: reader guard rejects --stdin"

if python3 "$GUARD" . 2>/dev/null; then
  echo "FAIL: reader guard must reject a positional path" >&2
  exit 1
fi
echo "PASS: reader guard rejects a positional path"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
mkdir -p "$TMPDIR/base/crates/assay-mcp-server/src"
for src in "${FILES[@]}"; do
  cp -R "$src" "$TMPDIR/base/$src"
done

if ! (
  cd "$TMPDIR/base"
  python3 "$GUARD"
); then
  echo "FAIL: unmutated reader scratch copy must pass before mutations are scored" >&2
  exit 1
fi
echo "PASS: unmutated reader scratch copy passes from its own cwd"

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

mutate_hardcode_policy_limit() {
  python3 - "$1/crates/assay-mcp-server/src/tools/policy_read.rs" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1]); s = p.read_text()
old = "crate::config::policy_byte_limit_from_env()"
new = "1_000_000"
assert old in s
p.write_text(s.replace(old, new, 1))
PY
}

mutate_public_parser() {
  python3 - "$1/crates/assay-mcp-server/src/policy_byte_limit.rs" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1]); s = p.read_text()
old = "pub(crate) fn policy_byte_limit_from_env"
new = "pub fn policy_byte_limit_from_env"
assert old in s
p.write_text(s.replace(old, new, 1))
PY
}

mutant "metadata-only accept then unbounded read" mutate_metadata_accept
mutant "direct tokio::fs::read bypass" mutate_direct_tokio
mutant "check_args from_file bypass" mutate_check_args_from_file
mutant "async entry skips read_bounded" mutate_skip_helper
mutant "production read hard-codes the policy ceiling" mutate_hardcode_policy_limit
mutant "pub fn policy_byte_limit_from_env" mutate_public_parser

echo "All MCP policy reader routing mutations caught."
