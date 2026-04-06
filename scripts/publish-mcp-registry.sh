#!/usr/bin/env bash
# Publish Assay to the official MCP Registry.
# See: https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/quickstart.mdx
#
# Prerequisites:
#   - mcp-publisher installed
#   - a generated registry metadata file, typically release/server.json from a
#     real release asset set
#
# Usage:
#   ./scripts/publish-mcp-registry.sh [path/to/server.json]
#
# This will:
#   1. Validate server.json against the official registry
#   2. Login to the official MCP Registry using GitHub auth
#   3. Publish the generated metadata
#
# Notes:
#   - The official MCP Registry is still in preview.
#   - This script assumes the namespace matches the authenticated GitHub owner.
#   - Use generated release/server.json, not a hand-maintained root file.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SERVER_JSON="${1:-${REPO_ROOT}/release/server.json}"
REGISTRY_URL="https://registry.modelcontextprotocol.io"

cd "$REPO_ROOT"

if ! command -v mcp-publisher >/dev/null 2>&1; then
  echo "mcp-publisher not found. Install it first."
  exit 1
fi

if [[ ! -f "$SERVER_JSON" ]]; then
  echo "server.json not found at $SERVER_JSON"
  echo "Generate it from a real release asset set first."
  exit 1
fi

echo "Validating server.json against the official registry..."
if ! mcp-publisher validate "$SERVER_JSON"; then
  echo "server.json validation failed. Aborting publish."
  exit 1
fi

echo ""
echo "Logging in to the official MCP Registry with GitHub..."
mcp-publisher login github

echo ""
echo "Publishing to ${REGISTRY_URL}..."
mcp-publisher publish "$SERVER_JSON"

echo ""
echo "Done. Check ${REGISTRY_URL} for the published entry."
