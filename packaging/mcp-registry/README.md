This directory holds Assay's MCP Registry packaging inputs.

The repository root no longer carries a static `server.json`, because that file
must match a real published package artifact. Assay now generates
`release/server.json` from the actual MCPB asset and SHA-256 produced during the
release workflow.

Current v1 registry direction:
- canonical surface: `assay-mcp-server`
- package type: `mcpb`
- transport: `stdio`
- compatibility: Linux only for the first honest release line

Supporting files:
- `../mcpb/manifest.assay-mcp-server.template.json`
- `../mcpb/run-assay-mcp-server.sh`
- `../../scripts/ci/build_mcpb_bundle.sh`
- `../../scripts/ci/render_registry_server_json.sh`
