#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

PROXY="crates/assay-core/src/mcp/proxy.rs"

assert_rg() {
  local pattern="$1"
  local file="$2"
  local message="$3"
  if ! rg -n "$pattern" "$file" >/dev/null; then
    echo "FAIL: $message"
    exit 1
  fi
}

echo "[review] workflow and generated-file guard"
if ! git diff --quiet -- .github/workflows; then
  echo "FAIL: Wave 51 MCP Proxy Step3 must not touch workflows"
  exit 1
fi
if ! git diff --quiet -- crates/assay-ebpf/src/vmlinux.rs; then
  echo "FAIL: generated vmlinux.rs must stay out of scope"
  exit 1
fi

echo "[review] characterization-only boundary"
if [ -d crates/assay-core/src/mcp/proxy ]; then
  echo "FAIL: Step3 must not introduce proxy submodules yet"
  exit 1
fi
assert_rg 'pub struct McpProxy' "$PROXY" "McpProxy facade moved out of proxy.rs"
assert_rg 'pub fn run\(' "$PROXY" "McpProxy::run moved out of proxy.rs"
assert_rg 'thread::spawn' "$PROXY" "threaded proxy loops no longer visible in proxy.rs"
assert_rg 'make_deny_response' "$PROXY" "deny response path marker missing"
assert_rg 'fn emit_decision\(' "$PROXY" "decision event projection marker missing"
assert_rg 'fn observe_tool_definition\(' "$PROXY" "tool-definition observation marker missing"
assert_rg 'fn extract_tool_call_id\(' "$PROXY" "idempotency helper marker missing"

echo "[review] proxy contract tests"
assert_rg 'proxy_contract_tool_call_id_prefers_meta' "$PROXY" "explicit tool_call_id contract test missing"
assert_rg 'proxy_contract_tool_call_id_uses_request_id' "$PROXY" "request id fallback contract test missing"
assert_rg 'proxy_contract_unknown_tool_call_id_is_generated' "$PROXY" "generated id contract test missing"
assert_rg 'proxy_contract_policy_code_mapping_is_stable' "$PROXY" "policy code mapping contract test missing"
assert_rg 'proxy_contract_observe_tool_definition_rejects_empty_names' "$PROXY" "empty tool definition contract test missing"
assert_rg 'proxy_contract_emit_decision_preserves_core_fields' "$PROXY" "decision projection contract test missing"

echo "[review] repo checks"
cargo fmt --check
cargo check -p assay-core
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core --lib proxy_contract_
git diff --check

echo "[review] PASS"
