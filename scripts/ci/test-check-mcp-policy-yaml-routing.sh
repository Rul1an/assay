#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GUARD="$SCRIPT_DIR/check-mcp-policy-yaml-routing.py"
FILES=(
  crates/assay-mcp-server/src/tools
  crates/assay-core/src/mcp/policy
)

if ! python3 "$GUARD"; then
  echo "FAIL: YAML guard must accept zero args from the allowlisted cwd" >&2
  exit 1
fi
echo "PASS: YAML guard accepts zero args from the allowlisted cwd"

for rejected_arg in extra --stdin .; do
  if python3 "$GUARD" "$rejected_arg" 2>/dev/null; then
    echo "FAIL: YAML guard must reject argv value: $rejected_arg" >&2
    exit 1
  fi
done
echo "PASS: YAML guard rejects every tested argv shape"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
mkdir -p "$TMPDIR/base/crates/assay-mcp-server/src" "$TMPDIR/base/crates/assay-core/src/mcp"
cp -R "${FILES[0]}" "$TMPDIR/base/crates/assay-mcp-server/src/"
cp -R "${FILES[1]}" "$TMPDIR/base/crates/assay-core/src/mcp/"

if ! (
  cd "$TMPDIR/base"
  python3 "$GUARD"
); then
  echo "FAIL: unmutated scratch copy must pass before mutations are scored" >&2
  exit 1
fi
echo "PASS: unmutated scratch copy passes from its own cwd"

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
    echo "FAIL: routing guard accepted mutation: $name" >&2
    exit 1
  fi
  if ! printf '%s\n' "$output" | grep -q '^FAIL: '; then
    echo "FAIL: routing guard exited $status without a FAIL reason: $name" >&2
    printf '%s\n' "$output" >&2
    exit 1
  fi
  echo "PASS: routing guard rejects $name"
}

mutate_consumer_parser() {
  python3 - "$1/crates/assay-mcp-server/src/tools/check_coverage.rs" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1]); s = p.read_text()
old = "super::parse_tool_policy(&policy_bytes)"
new = "serde_yaml::from_slice::<assay_core::model::Policy>(&policy_bytes)"
assert old in s
p.write_text(s.replace(old, new, 1))
PY
}

mutate_duplicate_root() {
  python3 - "$1/crates/assay-mcp-server/src/tools/policy_decide.rs" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1]); s = p.read_text()
old = "let super::MappingStage(mapping) = yaml_mapping_stage(bytes)?;"
new = old + "\n    let _duplicate = serde_yaml::Value::Mapping(mapping.clone()).is_mapping();"
assert old in s
p.write_text(s.replace(old, new, 1))
PY
}

mutate_core_parallel_parser() {
  cat >> "$1/crates/assay-core/src/mcp/policy/legacy.rs" <<'RS'
fn _parallel_parser(bytes: &[u8]) {
    let _ = serde_yaml::from_slice::<serde_yaml::Value>(bytes);
}
RS
}

mutate_missing_route() {
  python3 - "$1/crates/assay-mcp-server/src/tools/explain_trace.rs" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1]); s = p.read_text()
old = "super::parse_tool_policy(&policy_bytes)"
assert old in s
p.write_text(s.replace(old, "super::alternate_policy_parser(&policy_bytes)", 1))
PY
}

mutate_full_parser_bypass() {
  python3 - "$1/crates/assay-mcp-server/src/tools/check_args.rs" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1]); s = p.read_text()
old = "McpPolicy::from_slice(&policy_bytes)"
assert old in s
p.write_text(s.replace(old, "load_policy_alternate(&policy_bytes)", 1))
PY
}

mutate_alias_parser() {
  cat >> "$1/crates/assay-mcp-server/src/tools/check_coverage.rs" <<'RS'
#[allow(dead_code)]
fn _alias_parser(bytes: &[u8]) {
    use serde_yaml as sy;
    let _ = sy::from_slice::<sy::Value>(bytes);
}
RS
}

mutate_absolute_alias_parser() {
  cat >> "$1/crates/assay-mcp-server/src/tools/check_coverage.rs" <<'RS'
#[allow(dead_code)]
fn _absolute_alias_parser(bytes: &[u8]) {
    use ::serde_yaml as sy;
    let _ = sy::from_slice::<sy::Value>(bytes);
}
RS
}

mutant "direct consumer parser" mutate_consumer_parser
mutant "duplicate consumer root classifier" mutate_duplicate_root
mutant "parallel core parser" mutate_core_parallel_parser
mutant "missing direct helper route" mutate_missing_route
mutant "full-policy parser bypass" mutate_full_parser_bypass
mutant "aliased parser constructor" mutate_alias_parser
mutant "absolute aliased parser constructor" mutate_absolute_alias_parser

echo "All MCP policy YAML routing mutations caught."
