# `assay.mcp_server_inventory.v0`

The carrier `assay mcp inventory` emits: a coverage-honest projection of discovered MCP servers, for the
OWASP MCP09 (shadow-server) line. It is a **producer** artifact only — classifying servers against an
approved allowlist (shadow / drift / duplicate findings) is a separate consumer concern.

Discipline: command and args are **hashed, never emitted raw** (they routinely carry secrets and
paths); credential-bearing fields are flagged by **name only**, never by value; scanner coverage is
declared with an explicit per-source state, so an incomplete scan can never be read as an absence
claim.

## Shape

| Field | Notes |
| --- | --- |
| `schema` | Always `assay.mcp_server_inventory.v0`. |
| `scanner_coverage.config_sources` | Map of client → coverage state. |
| `scanner_coverage.process_scan` | Coverage state for process discovery. |
| `scanner_coverage.network_scan` | Coverage state for network discovery. |
| `servers[]` | Discovered server rows, sorted by `(server_id, source)`. |
| `non_claims` | Carries the absence non-claim. |

### Coverage states

`complete | partial | not_scanned | unavailable | unsupported`. **Anything other than `complete`
cannot support an absence claim** (not observed is not absent unless coverage is complete).

### `servers[]`

| Field | Notes |
| --- | --- |
| `server_id` | The configured/observed server name. |
| `source` | e.g. `claude_desktop_mcp_config`, `cursor_mcp_config`, `process_scan`, `network_scan`. |
| `transport` | `stdio` \| `http` \| `sse` \| `websocket` \| `unknown`. |
| `command_digest` / `args_digest` | `sha256:<hex>` over the canonical command/args. Raw values are never stored. |
| `credential_indicators` | Credential-bearing field **names** (`env:<KEY>`, `arg:<flag>`), never values. |
| `observed_state` | `observed`. Absence is not a state; it is governed by coverage. |

## Usage

```bash
assay mcp inventory                 # carrier to stdout
assay mcp inventory --out inv.json  # carrier to a file
```

## Non-claims

- Not observed is not absent unless scanner coverage is complete.
- A flagged credential field is an exposure-risk condition, not a secret-validity claim.
- This carrier is producer-only; it does not classify servers as approved/unapproved.
