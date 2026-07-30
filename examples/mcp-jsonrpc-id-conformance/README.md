# MCP 2026-07-28 and JSON-RPC error-response IDs

This pack records a narrow interoperability contradiction in the MCP
`2026-07-28` error-response shape.

MCP says that all messages between clients and servers must follow JSON-RPC
2.0. Its TypeScript source of truth defines `JSONRPCErrorResponse.id` as
optional and defines `RequestId` as `string | number`. The generated JSON
Schema carries the same required/null boundary: `id` is not required and, when
present, cannot be null.

JSON-RPC 2.0 requires `id` on every response. When the request ID could not be
detected, it requires that response ID to be null.

That gives two incompatible arms for an error response whose request ID could
not be read:

| Message shape | MCP 2026-07-28 | JSON-RPC 2.0 |
| --- | --- | --- |
| omit `id` | accepted by the MCP error-response shape | rejected because response `id` is required |
| set `"id": null` | rejected because MCP `RequestId` excludes null | required for an unknown request ID |

The third vector uses a string ID and is accepted by both bounded validators.
It prevents an always-rejecting checker from reproducing the finding.

## Reproduce the committed record

The default mode is offline and uses only Python's standard library:

```bash
python3 examples/mcp-jsonrpc-id-conformance/check.py reproduce
```

It verifies `SHA256SUMS`, the per-vector digests in `PROVENANCE.json`, every
declared outcome, and the presence of both incompatible arms plus the shared
positive control.

This mode reproduces the committed claim record. It does not re-read upstream
specifications. The immutable source pins cannot tell us whether a later
revision repaired the boundary.

## Reassess upstream source bytes

`reassess` extracts the four relevant constraints from caller-supplied source
bytes. It checks the MCP TypeScript source of truth and generated JSON Schema
for agreement before comparing them with the JSON-RPC response clauses.

```bash
tmp="$(mktemp -d)"
max_bytes=2097152

curl --proto '=https' --tlsv1.2 --fail --location --max-time 30 \
  --max-filesize "$max_bytes" \
  'https://raw.githubusercontent.com/modelcontextprotocol/modelcontextprotocol/5f5440bb26a62e2cf3440b92da5a667efa03b267/schema/2026-07-28/schema.ts' \
  --output "$tmp/mcp-schema.ts"
curl --proto '=https' --tlsv1.2 --fail --location --max-time 30 \
  --max-filesize "$max_bytes" \
  'https://raw.githubusercontent.com/modelcontextprotocol/modelcontextprotocol/5f5440bb26a62e2cf3440b92da5a667efa03b267/schema/2026-07-28/schema.json' \
  --output "$tmp/mcp-schema.json"
curl --proto '=https' --tlsv1.2 --fail --location --max-time 30 \
  --max-filesize "$max_bytes" \
  'https://modelcontextprotocol.io/specification/2026-07-28/basic/index' \
  --output "$tmp/mcp-overview.html"
curl --proto '=https' --tlsv1.2 --fail --location --max-time 30 \
  --max-filesize "$max_bytes" \
  'https://www.jsonrpc.org/specification' \
  --output "$tmp/jsonrpc-spec.html"

python3 examples/mcp-jsonrpc-id-conformance/check.py reassess \
  --mcp-typescript "$tmp/mcp-schema.ts" \
  --mcp-schema "$tmp/mcp-schema.json" \
  --mcp-overview "$tmp/mcp-overview.html" \
  --jsonrpc-spec "$tmp/jsonrpc-spec.html"
```

The committed workflow runs the same inputs through `verify-pinned`, which
reads the source digests from `PROVENANCE.json` before extracting any
constraint. The workflow carries no second copy of those digests. For a later
MCP revision, use `reassess` with that revision's TypeScript and generated JSON
schema together, then record a new subject set. Do not overwrite the historical
record.

Exit codes:

| Code | Meaning |
| --- | --- |
| `0` | `reassess` finds at least one arm, or `reproduce` verifies both committed arms and the control |
| `2` | recognized subjects no longer reproduce either arm |
| `3` | the pack or supplied subjects are malformed, unrecognized, or inconsistent |

## Sources

- [MCP base protocol overview](https://modelcontextprotocol.io/specification/2026-07-28/basic/index)
- [MCP TypeScript schema at `5f5440bb...`](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/5f5440bb26a62e2cf3440b92da5a667efa03b267/schema/2026-07-28/schema.ts)
- [Generated MCP JSON Schema at the same commit](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/5f5440bb26a62e2cf3440b92da5a667efa03b267/schema/2026-07-28/schema.json)
- [JSON-RPC 2.0 specification](https://www.jsonrpc.org/specification)

The MCP overview names the TypeScript schema as the source of truth and the
JSON Schema as generated tooling output. This pack checks both so a generator
drift cannot silently support the finding.

## Claim ceiling

This is a specification-conformance observation over three message shapes. It
does not establish exploitability, severity, affected implementations, or a
security impact. It does not test an MCP SDK or server. It does not choose an
upstream remedy.

One possible resolution would make the MCP error-response shape require `id`
and admit null when the request ID was unreadable. Upstream may choose a
different resolution, but omission alone does not satisfy the JSON-RPC
response requirement recorded here.

## Pack layout

| Path | Role |
| --- | --- |
| `PROVENANCE.json` | source pins, source digests, bounded finding, and vector digests |
| `SHA256SUMS` | complete public-pack file binding |
| `check.py` | stdlib-only reproduce and reassess commands |
| `vectors/` | two incompatibility arms and one shared positive control |
| `tests/test_check.py` | positive, negative, provenance, and source-drift tests |
