#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

PROXY="crates/assay-core/src/mcp/proxy.rs"
DECISIONS="crates/assay-core/src/mcp/proxy/decisions.rs"
TOOLS="crates/assay-core/src/mcp/proxy/tools.rs"

assert_rg() {
  local pattern="$1"
  local file="$2"
  local message="$3"
  if ! rg -n "$pattern" "$file" >/dev/null; then
    echo "FAIL: $message"
    exit 1
  fi
}

assert_not_rg() {
  local pattern="$1"
  local file="$2"
  local message="$3"
  if rg -n "$pattern" "$file" >/dev/null; then
    echo "FAIL: $message"
    exit 1
  fi
}

echo "[review] workflow and generated-file guard"
if ! git diff --quiet -- .github/workflows; then
  echo "FAIL: Wave 51 MCP Proxy Step4 must not touch workflows"
  exit 1
fi
if ! git diff --quiet -- crates/assay-ebpf/src/vmlinux.rs; then
  echo "FAIL: generated vmlinux.rs must stay out of scope"
  exit 1
fi

echo "[review] facade thinness and split boundary"
proxy_code_lines="$(
  awk 'BEGIN{n=0; in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} !in_tests{n++} END{print n}' "$PROXY"
)"
echo "proxy non-test lines: $proxy_code_lines"
if [ "$proxy_code_lines" -gt 450 ]; then
  echo "FAIL: proxy facade is too thick after Step4"
  exit 1
fi
assert_rg 'mod decisions;' "$PROXY" "proxy decisions module declaration missing"
assert_rg 'mod tools;' "$PROXY" "proxy tools module declaration missing"
assert_rg 'pub struct McpProxy' "$PROXY" "McpProxy facade moved out of proxy.rs"
assert_rg 'pub fn spawn\(' "$PROXY" "McpProxy::spawn moved out of proxy.rs"
assert_rg 'pub fn run\(' "$PROXY" "McpProxy::run moved out of proxy.rs"
assert_rg 'thread::spawn' "$PROXY" "threaded proxy loops no longer visible in proxy.rs"
assert_rg 'make_deny_response' "$PROXY" "deny response path marker missing"
assert_not_rg '^    fn (handle_allow|extract_tool_call_id|map_policy_code|emit_decision|observe_tool_definition)\(' "$PROXY" "helper definition still lives in proxy facade"

echo "[review] moved helper ownership"
assert_rg 'fn handle_allow\(' "$DECISIONS" "handle_allow helper missing from decisions module"
assert_rg 'fn extract_tool_call_id\(' "$DECISIONS" "idempotency helper missing from decisions module"
assert_rg 'fn map_policy_code\(' "$DECISIONS" "policy code mapper missing from decisions module"
assert_rg 'fn emit_decision\(' "$DECISIONS" "decision event projector missing from decisions module"
assert_rg 'struct ToolDefinitionObservation' "$TOOLS" "tool observation type missing from tools module"
assert_rg 'fn observe_tool_definition\(' "$TOOLS" "tools/list observation helper missing from tools module"
assert_rg 'proxy_contract_tool_call_id_prefers_meta' "$DECISIONS" "explicit tool_call_id contract test missing"
assert_rg 'proxy_contract_policy_code_mapping_is_stable' "$DECISIONS" "policy code mapping contract test missing"
assert_rg 'proxy_contract_emit_decision_preserves_core_fields' "$DECISIONS" "decision projection contract test missing"
assert_rg 'proxy_contract_observe_tool_definition_rejects_empty_names' "$TOOLS" "empty tool definition contract test missing"

echo "[review] repo checks"
cargo fmt --check
cargo check -p assay-core
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core --lib proxy_contract_
cargo test -p assay-core --lib mcp::proxy::
git diff --check

echo "[review] PASS"
