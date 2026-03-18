#!/usr/bin/env bash
# Publish Assay to MCPCentral registry.
# See: https://mcpcentral.io/submit-server
#
# Prerequisites:
#   - mcp-publisher installed (brew install mcp-publisher)
#   - server.json in repo root
#
# Usage:
#   ./scripts/publish-mcpcentral.sh
#
# This will:
#   1. Validate server.json
#   2. Check if registry.mcpcentral.io is reachable
#   3. Login to MCPCentral (opens browser for GitHub OAuth)
#   4. Publish to registry.mcpcentral.io
#
# Note: registry.mcpcentral.io may be unreachable (DNS NXDOMAIN). MCPCentral
# ETLs from the Official MCP Registry; Assay is already there.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SERVER_JSON="${REPO_ROOT}/server.json"
REGISTRY="https://registry.mcpcentral.io"

cd "$REPO_ROOT"

if ! command -v mcp-publisher &>/dev/null; then
  echo "mcp-publisher not found. Install with: brew install mcp-publisher"
  exit 1
fi

if [[ ! -f "$SERVER_JSON" ]]; then
  echo "server.json not found at $SERVER_JSON"
  exit 1
fi

echo "Validating server.json..."
if ! mcp-publisher validate "$SERVER_JSON"; then
  echo "server.json validation failed. Aborting publish."
  exit 1
fi

echo ""
if ! host registry.mcpcentral.io &>/dev/null; then
  echo "⚠️  registry.mcpcentral.io is not reachable (DNS NXDOMAIN)."
  echo "   MCPCentral ETLs from the Official MCP Registry; Assay is already there."
  echo "   Retry later when MCPCentral's registry is up."
  exit 1
fi

echo "Logging in to MCPCentral (opens browser)..."
mcp-publisher login github -registry "$REGISTRY"

echo ""
echo "Publishing to MCPCentral..."
mcp-publisher publish "$SERVER_JSON"

echo ""
echo "Done. Check https://mcpcentral.io/registry for your listing."
