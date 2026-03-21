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
cargo test -q -p assay-core --test mcp_id_correlation
cargo test -q -p assay-core --test mcp_transport_compat
cargo test -q -p assay-core --test mcp_import_smoke
cargo test -q -p assay-cli --test mcp_id_correlation_errors

echo "== parser anchors =="
check_has_match 'normalize_jsonrpc_id' 'crates/assay-core/src/mcp/parser.rs'
check_has_match 'Value::Null' 'crates/assay-core/src/mcp/parser.rs'
check_has_match 'Ok\(None\)' 'crates/assay-core/src/mcp/parser.rs'
check_has_match 'must not be a boolean' 'crates/assay-core/src/mcp/parser.rs'
check_has_match 'duplicate tools/call request id' 'crates/assay-core/src/mcp/parser.rs'
if rg -n -- 'get\("id"\).*trim_matches' crates/assay-core/src/mcp/parser.rs >/dev/null; then
  echo 'raw JSON-RPC id stringification shortcut must not remain in parser.rs' >&2
  exit 1
fi

echo "== test anchors =="
check_has_match 'contract_bool_id_true_fails_hard' 'crates/assay-core/tests/mcp_id_correlation.rs'
check_has_match 'contract_bool_id_false_fails_hard' 'crates/assay-core/tests/mcp_id_correlation.rs'
check_has_match 'contract_first_response_wins_and_later_duplicate_responses_are_orphan' 'crates/assay-core/tests/mcp_id_correlation.rs'
check_has_match 'assert_no_jsonrpc_id_literal_null' 'crates/assay-core/tests/mcp_id_correlation.rs'
check_has_match 'must not be a boolean' 'crates/assay-cli/tests/mcp_id_correlation_errors.rs'
check_has_match 'duplicate tools/call request id' 'crates/assay-cli/tests/mcp_id_correlation_errors.rs'

echo "== docs anchors =="
check_has_match '### JSON-RPC `id` Normalization' 'docs/mcp/import-formats.md'
check_has_match 'literal string `"null"`' 'docs/mcp/import-formats.md'
check_has_match 'duplicate `tools/call` request ids in one transcript fail at parse time' 'docs/mcp/import-formats.md'

echo "== diff allowlist =="
leaks="$(rg -v \
  '^crates/assay-core/src/mcp/parser\.rs$|^crates/assay-core/tests/mcp_id_correlation\.rs$|^crates/assay-cli/tests/mcp_id_correlation_errors\.rs$|^docs/mcp/import-formats\.md$|^scripts/ci/review-wave-t1a-id-correlation-step1\.sh$|^docs/contributing/SPLIT-(INVENTORY|CHECKLIST|MOVE-MAP|REVIEW-PACK)-wave-t1a-id-correlation-step1\.md$' \
  < <(git diff --name-only "${BASE_REF}...HEAD") || true)"
if [ -n "$leaks" ]; then
  echo "non-allowlisted files detected:" >&2
  echo "$leaks" >&2
  exit 1
fi

echo "== whitespace =="
git diff --check "${BASE_REF}...HEAD"

echo "Wave T1a Step1 reviewer script: PASS"
