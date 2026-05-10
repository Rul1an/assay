#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."

echo "[review] OWASP MCP fixture docs mention concrete PR2 tests"
rg -q "owasp_mcp01_token_args_do_not_leak_to_proxy_logs" docs/security/OWASP-MCP-TOP10-TEST-MAP.md
rg -q "owasp_mcp03_metadata_poisoning_description_drift_denies_pinned_tool" docs/security/OWASP-MCP-TOP10-TEST-MAP.md
rg -q "owasp_mcp05_sandbox_keeps_shell_metacharacters_as_argv" docs/security/OWASP-MCP-TOP10-TEST-MAP.md
if rg -q "Add an end-to-end token-in-log fixture|Add a proxy metadata poisoning fixture|Add a sandbox command-injection fixture" docs/security/OWASP-MCP-TOP10-TEST-MAP.md; then
  echo "FAIL: PR2 fixture gaps should not remain listed as missing" >&2
  exit 1
fi

echo "[review] core proxy metadata-poisoning fixture"
cargo test -p assay-core --lib owasp_mcp03_metadata_poisoning_description_drift_denies_pinned_tool

echo "[review] build MCP wrap fixture binaries"
cargo build -p assay-cli -p assay-mcp-server --bins

echo "[review] MCP01 token-in-log E2E fixture"
cargo test -p assay-cli --test e2e_mcp_wrap_assert_cmd owasp_mcp01_token_args_do_not_leak_to_proxy_logs

if [[ "$(uname -s)" != MINGW* && "$(uname -s)" != MSYS* && "$(uname -s)" != CYGWIN* ]]; then
  echo "[review] MCP05 sandbox command-injection fixture"
  cargo test -p assay-cli --test profile_integration_test owasp_mcp05_sandbox_keeps_shell_metacharacters_as_argv
else
  echo "[review] skipping Unix shell argv fixture on Windows"
fi

echo "[review] diff hygiene"
git diff --check
