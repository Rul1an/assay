#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

PROXY="crates/assay-core/src/mcp/proxy.rs"
CLIENT="crates/assay-core/src/mcp/proxy/client.rs"
BRANCHES="crates/assay-core/src/mcp/proxy/client/branches.rs"
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

non_test_lines() {
  awk 'BEGIN{n=0; in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} !in_tests{n++} END{print n}' "$1"
}

echo "[review] workflow and generated-file guard"
if ! git diff --quiet -- .github/workflows; then
  echo "FAIL: Wave 51 MCP Proxy Step7 must not touch workflows"
  exit 1
fi
if ! git diff --quiet -- crates/assay-ebpf/src/vmlinux.rs; then
  echo "FAIL: generated vmlinux.rs must stay out of scope"
  exit 1
fi

echo "[review] facade boundary"
proxy_code_lines="$(non_test_lines "$PROXY")"
echo "proxy non-test lines: $proxy_code_lines"
if [ "$proxy_code_lines" -gt 260 ]; then
  echo "FAIL: proxy facade is too thick after Step7"
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

echo "[review] client loop boundary"
client_code_lines="$(non_test_lines "$CLIENT")"
echo "client non-test lines: $client_code_lines"
if [ "$client_code_lines" -gt 130 ]; then
  echo "FAIL: client loop is too thick after Step7"
  exit 1
fi
assert_rg 'mod branches;' "$CLIENT" "client branches module declaration missing"
assert_rg 'fn run_client_to_server\(' "$CLIENT" "client loop entrypoint missing"
assert_rg 'stdin\.lock\(\)' "$CLIENT" "client module does not own stdin lock"
assert_rg 'read_line\(&mut line\)' "$CLIENT" "client module does not own read loop"
assert_rg 'serde_json::from_str::<JsonRpcRequest>' "$CLIENT" "client module does not parse JSON-RPC requests"
assert_rg 'PolicyState::default\(\)' "$CLIENT" "policy state ownership missing from client loop"
assert_rg 'AuditLog::new' "$CLIENT" "audit log ownership missing from client loop"
assert_rg 'handle_policy_decision\(' "$CLIENT" "client loop does not delegate policy branches"
assert_rg 'PolicyBranchContext' "$CLIENT" "client loop does not pass explicit branch context"
assert_rg 'BranchOutcome::Blocked' "$CLIENT" "client loop does not honor blocking branch outcomes"
assert_rg 'child_stdin\.write_all' "$CLIENT" "child stdin forwarding missing from client loop"
assert_rg 'Suspicious unparsable JSON' "$CLIENT" "suspicious JSON warning missing from client loop"
assert_not_rg 'PolicyDecision::Allow|PolicyDecision::AllowWithWarning|PolicyDecision::Deny|make_deny_response|map_policy_code|AuditEvent|reason_codes::P_POLICY_ALLOW|decision_str = ' "$CLIENT" "policy branch detail still lives in client loop"
assert_not_rg 'observe_tool_definition|BufReader::new\(child_stdout\)' "$CLIENT" "client module leaked server output responsibilities"

echo "[review] branch module ownership"
assert_rg 'fn handle_policy_decision' "$BRANCHES" "branch dispatcher missing"
assert_rg 'fn handle_allow_branch\(' "$BRANCHES" "allow branch handler missing"
assert_rg 'fn handle_allow_with_warning_branch\(' "$BRANCHES" "warning branch handler missing"
assert_rg 'fn handle_deny_branch' "$BRANCHES" "deny branch handler missing"
assert_rg 'PolicyDecision::Allow' "$BRANCHES" "allow branch match missing"
assert_rg 'PolicyDecision::AllowWithWarning' "$BRANCHES" "allow-with-warning branch match missing"
assert_rg 'PolicyDecision::Deny' "$BRANCHES" "deny branch match missing"
assert_rg 'make_deny_response' "$BRANCHES" "deny response construction missing from branch module"
assert_rg 'map_policy_code' "$BRANCHES" "deny reason-code mapping missing from branch module"
assert_rg 'emit_decision\(' "$BRANCHES" "decision emission missing from branch module"
assert_rg 'AuditEvent' "$BRANCHES" "audit event ownership missing from branch module"
assert_rg 'BranchOutcome::Blocked' "$BRANCHES" "blocking outcome missing from branch module"
assert_not_rg 'stdin\.lock|read_line\(&mut line\)|serde_json::from_str::<JsonRpcRequest>|child_stdin\.write_all|observe_tool_definition|BufReader::new\(child_stdout\)' "$BRANCHES" "branch module leaked loop/server responsibilities"
assert_not_rg 'PolicyDecision|make_deny_response|AuditLog|emit_decision' "$SERVER" "server module leaked client policy/audit responsibilities"

 echo "[review] branch contracts"
assert_rg 'proxy_contract_client_branch_allow_emits_allow_and_forwards' "$BRANCHES" "allow branch contract missing"
assert_rg 'proxy_contract_client_branch_warning_emits_allow_and_forwards' "$BRANCHES" "warning branch contract missing"
assert_rg 'proxy_contract_client_branch_deny_emits_deny_and_blocks' "$BRANCHES" "deny branch contract missing"
assert_rg 'proxy_contract_client_branch_dry_run_deny_emits_allow_and_forwards' "$BRANCHES" "dry-run deny branch contract missing"
assert_rg 'proxy_contract_client_loop_inputs_remain_private' "$CLIENT" "client loop contract test missing"
assert_rg 'proxy_contract_server_loop_enrichment_inputs_remain_private' "$SERVER" "server module contract test missing"
assert_rg 'proxy_contract_emit_decision_preserves_core_fields' "$DECISIONS" "decision projection contract test missing"
assert_rg 'proxy_contract_observe_tool_definition_rejects_empty_names' "$TOOLS" "tools/list observation contract test missing"

echo "[review] repo checks"
cargo fmt --check
cargo check -p assay-core
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core --lib proxy_contract_client_branch_
cargo test -p assay-core --lib proxy_contract_
cargo test -p assay-core --lib mcp::proxy::
git diff --check

echo "[review] PASS"
