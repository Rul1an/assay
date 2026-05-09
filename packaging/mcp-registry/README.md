This directory holds Assay's MCP Registry packaging inputs.

The repository root no longer carries a static `server.json`, because that file
must match a real published package artifact. Assay now generates
`release/server.json` from the actual MCPB asset and SHA-256 produced during the
release workflow.

Current v1 registry direction:
- canonical surface: `assay-mcp-server`
- canonical registry name: `io.github.Rul1an/assay-mcp-server`
- package type: `mcpb`
- transport: `stdio`
- compatibility: Linux only for the first honest release line

Current release-truth check:

```bash
gh release download v3.9.2 --repo Rul1an/assay --pattern server.json --dir release
mcp-publisher validate release/server.json
mcp-publisher login github
mcp-publisher publish release/server.json
curl -fsSL 'https://registry.modelcontextprotocol.io/v0/servers?search=assay'
```

Use this generated release asset, not a hand-maintained root `server.json`.
The stale legacy `io.github.Rul1an/assay` identity has been deprecated rather
than deleted, so old discovery links have a safe migration signal while the
canonical `assay-mcp-server` line stays current. The registry/discovery audit
lives at
[`../../docs/ops/MCP-REGISTRY-DISCOVERY-AUDIT.md`](../../docs/ops/MCP-REGISTRY-DISCOVERY-AUDIT.md).

Supporting files:
- `../mcpb/manifest.assay-mcp-server.template.json`
- `../mcpb/run-assay-mcp-server.sh`
- `../../scripts/ci/build_mcpb_bundle.sh`
- `../../scripts/ci/render_registry_server_json.sh`
