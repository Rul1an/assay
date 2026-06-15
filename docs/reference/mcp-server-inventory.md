# MCP Server Inventory Artifact

`assay.mcp_server_inventory.v0` is a bounded inventory artifact for configured or observed MCP
servers. It supports MCP09 shadow-server review by separating three ideas:

- **observed**: a server was found in a scanned source;
- **approved**: a declared allowlist contains a matching server identity;
- **coverage**: the scanner did or did not inspect the relevant sources.

The artifact does **not** prove that no MCP servers exist outside scanned sources.

## Shape

```json
{
  "schema": "assay.mcp_server_inventory.v0",
  "scanner_coverage": {
    "config_sources": {
      "claude_desktop": "complete",
      "vscode": "complete",
      "cursor": "not_scanned"
    },
    "process_scan": "unavailable",
    "network_scan": "not_scanned"
  },
  "servers": [
    {
      "server_id": "github-tools",
      "source": "vscode_mcp_config",
      "transport": "stdio",
      "command_digest": "sha256:...",
      "args_digest": "sha256:...",
      "observed_state": "observed"
    }
  ],
  "non_claims": [
    "absence from inventory is not absence from environment unless scanner coverage is complete"
  ]
}
```

## Finding Vocabulary

| Condition | Finding |
| --- | --- |
| Observed server has no declared allowlist entry | `shadow_mcp_server_observed` |
| Declared server command digest differs | `mcp_server_command_drift` |
| Declared server args digest differs | `mcp_server_args_drift` |
| Same server id appears with multiple command digests | `duplicate_mcp_server_identity` |
| Scanner coverage is incomplete and no stronger finding exists | `mcp_inventory_coverage_incomplete` |
| Approved server unchanged and coverage is complete for scanned sources | clean |

## Non-Claims

- Not observed is not absent unless scanner coverage is complete for the relevant source class.
- Observed is not approved.
- Approved is not safe; it only means the observed server matched a declared allowlist entry.
- Command or args drift is a review condition, not a maliciousness finding.
- Process and network scans are coverage signals, not endpoint governance.
