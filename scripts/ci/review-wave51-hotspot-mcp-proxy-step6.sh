#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

PROXY="crates/assay-core/src/mcp/proxy.rs"
CLIENT="crates/assay-core/src/mcp/proxy/client.rs"
SERVER="crates/assay-core/src/mcp/proxy/server.rs"
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
  echo "FAIL: Wave 51 MCP Proxy Step6 must not touch workflows"
  exit 1
fi
if ! git diff --quiet -- crates/assay-ebpf/src/vmlinux.rs; then
  echo "FAIL: generated vmlinux.rs must stay out of scope"
  exit 1
fi

echo "[review] facade and client-loop boundary"
proxy_code_lines="$(
  awk 'BEGIN{n=0; in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} !in_tests{n++} END{print n}' "$PROXY"
)"
echo "proxy non-test lines: $proxy_code_lines"
if [ "$proxy_code_lines" -gt 260 ]; then
  echo "FAIL: proxy facade is too thick after Step6"
  exit 1
fi
assert_rg 'mod client;' "$PROXY" "proxy client module declaration missing"
assert_rg 'run_client_to_server\(' "$PROXY" "proxy facade does not delegate client loop"
assert_rg 'run_server_to_client\(' "$PROXY" "server loop delegation missing"
assert_rg 'pub struct McpProxy' "$PROXY" "McpProxy facade moved out of proxy.rs"
assert_rg 'pub fn spawn\(' "$PROXY" "McpProxy::spawn moved out of proxy.rs"
assert_rg 'pub fn run\(' "$PROXY" "McpProxy::run moved out of proxy.rs"
assert_rg 'thread::spawn' "$PROXY" "thread spawning no longer visible in proxy.rs"
assert_rg 'self\.child\.wait\(\)' "$PROXY" "child wait moved out of proxy facade"
assert_not_rg 'stdin\.lock|PolicyDecision|PolicyState|AuditLog|AuditEvent|JsonRpcRequest|make_deny_response|child_stdin\.write_all|emit_decision|handle_allow|map_policy_code|extract_tool_call_id' "$PROXY" "client policy/forwarding detail still lives in proxy facade"

echo "[review] client module ownership"
assert_rg 'fn run_client_to_server\(' "$CLIENT" "client loop entrypoint missing"
assert_rg 'stdin\.lock\(\)' "$CLIENT" "client module does not own stdin lock"
assert_rg 'read_line\(&mut line\)' "$CLIENT" "client module does not own read loop"
assert_rg 'serde_json::from_str::<JsonRpcRequest>' "$CLIENT" "client module does not parse JSON-RPC requests"
assert_rg 'PolicyDecision::Allow' "$CLIENT" "allow branch missing from client module"
assert_rg 'PolicyDecision::AllowWithWarning' "$CLIENT" "allow-with-warning branch missing from client module"
assert_rg 'PolicyDecision::Deny' "$CLIENT" "deny branch missing from client module"
assert_rg 'PolicyState::default\(\)' "$CLIENT" "policy state ownership missing from client module"
assert_rg 'AuditLog::new' "$CLIENT" "audit log ownership missing from client module"
assert_rg 'make_deny_response' "$CLIENT" "deny response path missing from client module"
assert_rg 'emit_decision\(' "$CLIENT" "decision emission missing from client module"
assert_rg 'child_stdin\.write_all' "$CLIENT" "child stdin forwarding missing from client module"
assert_rg 'Suspicious unparsable JSON' "$CLIENT" "suspicious JSON warning missing from client module"
assert_not_rg 'observe_tool_definition|BufReader::new\(child_stdout\)' "$CLIENT" "client module leaked server output responsibilities"
assert_not_rg 'PolicyDecision|make_deny_response|AuditLog|emit_decision' "$SERVER" "server module leaked client policy/audit responsibilities"
assert_rg 'proxy_contract_client_loop_inputs_remain_private' "$CLIENT" "client module contract test missing"
assert_rg 'proxy_contract_server_loop_enrichment_inputs_remain_private' "$SERVER" "server module contract test missing"
assert_rg 'proxy_contract_emit_decision_preserves_core_fields' "$DECISIONS" "decision projection contract test missing"
assert_rg 'proxy_contract_observe_tool_definition_rejects_empty_names' "$TOOLS" "tools/list observation contract test missing"

echo "[review] repo checks"
cargo fmt --check
cargo check -p assay-core
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core --lib proxy_contract_
cargo test -p assay-core --lib mcp::proxy::
git diff --check

echo "[review] PASS"
