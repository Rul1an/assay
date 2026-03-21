#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if ! git rev-parse --verify --quiet "${BASE_REF}^{commit}" >/dev/null; then
  echo "BASE_REF not found: ${BASE_REF}" >&2
  exit 1
fi

check_has_match() {
  local pattern="$1"
  local file="$2"
  if ! rg -n -- "$pattern" "$file" >/dev/null; then
    echo "missing expected pattern in ${file}: ${pattern}" >&2
    exit 1
  fi
}

echo "== formatting =="
cargo fmt --check

echo "== lint =="
cargo clippy -q -p assay-core -p assay-cli --all-targets -- -D warnings

echo "== contract tests =="
cargo test -q -p assay-core --test mcp_transport_compat
cargo test -q -p assay-core --test mcp_import_smoke
cargo test -q -p assay-cli --test mcp_transport_import

echo "== parser anchors =="
check_has_match 'StreamableHttp' 'crates/assay-core/src/mcp/types.rs'
check_has_match 'HttpSse' 'crates/assay-core/src/mcp/types.rs'
check_has_match '"http-sse" \| "sse-legacy"' 'crates/assay-core/src/mcp/types.rs'
check_has_match 'parse_streamable_http_transcript' 'crates/assay-core/src/mcp/parser.rs'
check_has_match 'parse_http_sse_transcript' 'crates/assay-core/src/mcp/parser.rs'
check_has_match 'must contain exactly one of request, response, or sse' 'crates/assay-core/src/mcp/parser.rs'
check_has_match 'event_name == "endpoint"' 'crates/assay-core/src/mcp/parser.rs'

echo "== docs anchors =="
check_has_match '`streamable-http`' 'docs/mcp/import-formats.md'
check_has_match '`http-sse`' 'docs/mcp/import-formats.md'
check_has_match 'deprecated MCP HTTP\+SSE transport family' 'docs/mcp/import-formats.md'
check_has_match 'Out Of Scope In T1' 'docs/mcp/import-formats.md'
check_has_match 'do not change semantic equivalence assertions' 'docs/mcp/import-formats.md'

echo "== diff allowlist =="
leaks="$(rg -v \
  '^crates/assay-core/src/mcp/types\.rs$|^crates/assay-core/src/mcp/parser\.rs$|^crates/assay-core/tests/mcp_transport_compat\.rs$|^crates/assay-cli/src/cli/args/import\.rs$|^crates/assay-cli/src/cli/args/replay\.rs$|^crates/assay-cli/src/cli/commands/import\.rs$|^crates/assay-cli/src/cli/commands/trace\.rs$|^crates/assay-cli/tests/mcp_transport_import\.rs$|^docs/mcp/import-formats\.md$|^scripts/ci/review-wave-t1-transport-compat-step1\.sh$|^docs/contributing/SPLIT-(INVENTORY|CHECKLIST|MOVE-MAP|REVIEW-PACK)-wave-t1-transport-compat-step1\.md$' \
  < <(git diff --name-only "${BASE_REF}...HEAD") || true)"
if [ -n "$leaks" ]; then
  echo "non-allowlisted files detected:" >&2
  echo "$leaks" >&2
  exit 1
fi

echo "== whitespace =="
git diff --check "${BASE_REF}...HEAD"

echo "Wave T1 Step1 reviewer script: PASS"
