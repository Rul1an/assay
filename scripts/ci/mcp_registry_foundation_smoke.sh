#!/usr/bin/env bash
set -euo pipefail

if ! command -v npx >/dev/null 2>&1; then
  echo "npx is required for the MCP Registry smoke test" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required for the MCP Registry smoke test" >&2
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MCPB_CLI_VERSION="${MCPB_CLI_VERSION:-2.1.2}"
MCPB_CLI_PACKAGE="@anthropic-ai/mcpb@${MCPB_CLI_VERSION}"
TMP_DIR="$(mktemp -d)"

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

mkdir -p "$TMP_DIR/x86/pkg" "$TMP_DIR/arm/pkg"

cat > "$TMP_DIR/x86/pkg/assay-mcp-server" <<'EOF'
#!/usr/bin/env sh
echo x86
EOF

cat > "$TMP_DIR/arm/pkg/assay-mcp-server" <<'EOF'
#!/usr/bin/env sh
echo arm64
EOF

chmod +x "$TMP_DIR/x86/pkg/assay-mcp-server" "$TMP_DIR/arm/pkg/assay-mcp-server"

(
  cd "$TMP_DIR/x86"
  tar -czf "$TMP_DIR/assay-mcp-server-v0.0.0-x86_64-unknown-linux-gnu.tar.gz" pkg
)

(
  cd "$TMP_DIR/arm"
  tar -czf "$TMP_DIR/assay-mcp-server-v0.0.0-aarch64-unknown-linux-gnu.tar.gz" pkg
)

bash "$REPO_ROOT/scripts/ci/build_mcpb_bundle.sh" \
  --version-tag v0.0.0 \
  --linux-x86-archive "$TMP_DIR/assay-mcp-server-v0.0.0-x86_64-unknown-linux-gnu.tar.gz" \
  --linux-arm64-archive "$TMP_DIR/assay-mcp-server-v0.0.0-aarch64-unknown-linux-gnu.tar.gz" \
  --output "$TMP_DIR/assay-mcp-server-v0.0.0-linux.mcpb"

npx --yes "${MCPB_CLI_PACKAGE}" info "$TMP_DIR/assay-mcp-server-v0.0.0-linux.mcpb" >/dev/null

sha="$(cut -d' ' -f1 "$TMP_DIR/assay-mcp-server-v0.0.0-linux.mcpb.sha256")"

bash "$REPO_ROOT/scripts/ci/render_registry_server_json.sh" \
  --version-tag v0.0.0 \
  --mcpb-url "https://github.com/Rul1an/assay/releases/download/v0.0.0/assay-mcp-server-v0.0.0-linux.mcpb" \
  --file-sha256 "$sha" \
  --output "$TMP_DIR/server.json"

test ! -e "$REPO_ROOT/server.json"

jq -e '.name == "io.github.Rul1an/assay-mcp-server"' "$TMP_DIR/server.json" >/dev/null
jq -e '.version == "0.0.0"' "$TMP_DIR/server.json" >/dev/null
jq -e '.packages | length == 1' "$TMP_DIR/server.json" >/dev/null
jq -e '.packages[0].registryType == "mcpb"' "$TMP_DIR/server.json" >/dev/null
jq -e '.packages[0].identifier | endswith(".mcpb")' "$TMP_DIR/server.json" >/dev/null
jq -e '.packages[0].transport.type == "stdio"' "$TMP_DIR/server.json" >/dev/null
jq -e '.packages[0].fileSha256 | test("^[0-9a-f]{64}$")' "$TMP_DIR/server.json" >/dev/null

if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  {
    echo "## MCP Registry foundation smoke"
    echo "- built synthetic assay-mcp-server MCPB"
    echo "- rendered generated release/server.json shape"
    echo "- confirmed root server.json is gone"
  } >> "$GITHUB_STEP_SUMMARY"
fi
