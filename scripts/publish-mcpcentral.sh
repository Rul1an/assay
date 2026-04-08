#!/usr/bin/env bash
# Publish Assay to MCPCentral registry.
# See: https://mcpcentral.io/submit-server
#
# Prerequisites:
#   - mcp-publisher installed (brew install mcp-publisher)
#   - a generated registry metadata file, typically release/server.json from a
#     real release asset set
#
# Usage:
#   ./scripts/publish-mcpcentral.sh [path/to/server.json]
#
# This will:
#   1. Validate server.json
#   2. Check if registry.mcpcentral.io is reachable
#   3. Login to MCPCentral (opens browser for GitHub OAuth)
#   4. Publish to registry.mcpcentral.io
#
# Note: registry.mcpcentral.io may be unreachable (DNS NXDOMAIN). MCPCentral may
# ETL from the official MCP Registry, but this script does not assume Assay is
# already published there.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SERVER_JSON="${1:-${REPO_ROOT}/release/server.json}"
REGISTRY="https://registry.mcpcentral.io"

cd "$REPO_ROOT"

if ! command -v mcp-publisher &>/dev/null; then
  echo "mcp-publisher not found. Install with: brew install mcp-publisher"
  exit 1
fi

if [[ ! -f "$SERVER_JSON" ]]; then
  echo "server.json not found at $SERVER_JSON"
  echo "Generate it from a real release asset set first."
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
  echo "   Retry later when MCPCentral's registry is up."
  echo "   If MCPCentral later syncs from the official registry, treat that as"
  echo "   additive, not as proof that this Assay line is already published."
  exit 1
fi

echo "Logging in to MCPCentral (opens browser)..."
mcp-publisher login github -registry "$REGISTRY"

echo ""
echo "Publishing to MCPCentral..."
mcp-publisher publish "$SERVER_JSON"

echo ""
echo "Done. Check https://mcpcentral.io/registry for your listing."
