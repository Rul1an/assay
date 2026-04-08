#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  render_registry_server_json.sh \
    --version-tag vX.Y.Z \
    --mcpb-url <github-release-url> \
    --file-sha256 <sha256> \
    --output <path/to/server.json>

Render official MCP Registry metadata for the generated assay-mcp-server MCPB.
EOF
}

VERSION_TAG=""
MCPB_URL=""
FILE_SHA256=""
OUTPUT=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version-tag)
      VERSION_TAG="${2:-}"
      shift 2
      ;;
    --mcpb-url)
      MCPB_URL="${2:-}"
      shift 2
      ;;
    --file-sha256)
      FILE_SHA256="${2:-}"
      shift 2
      ;;
    --output)
      OUTPUT="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$VERSION_TAG" || -z "$MCPB_URL" || -z "$FILE_SHA256" || -z "$OUTPUT" ]]; then
  usage >&2
  exit 2
fi

SEMVER="${VERSION_TAG#v}"
mkdir -p "$(dirname "$OUTPUT")"

cat > "$OUTPUT" <<EOF
{
  "\$schema": "https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json",
  "name": "io.github.Rul1an/assay-mcp-server",
  "title": "Assay MCP Server",
  "description": "Policy-enforcing MCP proxy with portable evidence output.",
  "repository": {
    "url": "https://github.com/Rul1an/assay",
    "source": "github"
  },
  "version": "${SEMVER}",
  "packages": [
    {
      "registryType": "mcpb",
      "identifier": "${MCPB_URL}",
      "version": "${SEMVER}",
      "fileSha256": "${FILE_SHA256}",
      "transport": {
        "type": "stdio"
      }
    }
  ],
  "_meta": {
    "io.modelcontextprotocol.registry/publisher-provided": {
      "tool": "assay-release-workflow",
      "channel": "github-release"
    }
  }
}
EOF
