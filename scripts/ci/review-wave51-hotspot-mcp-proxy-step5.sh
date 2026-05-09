#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

PROXY="crates/assay-core/src/mcp/proxy.rs"
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
  echo "FAIL: Wave 51 MCP Proxy Step5 must not touch workflows"
  exit 1
fi
if ! git diff --quiet -- crates/assay-ebpf/src/vmlinux.rs; then
  echo "FAIL: generated vmlinux.rs must stay out of scope"
  exit 1
fi

echo "[review] facade and server-loop boundary"
proxy_code_lines="$(
  awk 'BEGIN{n=0; in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} !in_tests{n++} END{print n}' "$PROXY"
)"
echo "proxy non-test lines: $proxy_code_lines"
if [ "$proxy_code_lines" -gt 420 ]; then
  echo "FAIL: proxy facade is too thick after Step5"
  exit 1
fi
assert_rg 'mod server;' "$PROXY" "proxy server module declaration missing"
assert_rg 'run_server_to_client\(' "$PROXY" "proxy facade does not delegate server loop"
assert_rg 'pub struct McpProxy' "$PROXY" "McpProxy facade moved out of proxy.rs"
assert_rg 'pub fn spawn\(' "$PROXY" "McpProxy::spawn moved out of proxy.rs"
assert_rg 'pub fn run\(' "$PROXY" "McpProxy::run moved out of proxy.rs"
assert_rg 'thread::spawn' "$PROXY" "thread spawning no longer visible in proxy.rs"
assert_rg 'stdin\.lock\(' "$PROXY" "client-to-server policy loop moved too early"
assert_rg 'make_deny_response' "$PROXY" "deny response path marker missing"
assert_not_rg 'BufReader' "$PROXY" "server output reader still lives in proxy facade"
assert_not_rg 'observe_tool_definition' "$PROXY" "tools/list observation still called directly by proxy facade"

echo "[review] server module ownership"
assert_rg 'fn run_server_to_client\(' "$SERVER" "server loop entrypoint missing"
assert_rg 'BufReader::new\(child_stdout\)' "$SERVER" "server module does not own child stdout reader"
assert_rg 'read_line\(&mut line\)' "$SERVER" "server module does not own read loop"
assert_rg 'observe_tool_definition\(' "$SERVER" "server module does not enrich tools/list responses"
assert_rg 'identity_cache\.lock\(\)' "$SERVER" "server module does not update identity cache"
assert_rg 'tool_definition_cache\.lock\(\)' "$SERVER" "server module does not update tool-definition cache"
assert_not_rg 'PolicyDecision|make_deny_response|AuditLog|emit_decision' "$SERVER" "server module leaked client policy/audit responsibilities"
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
