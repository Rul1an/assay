# An MCP error response cannot satisfy both MCP and JSON-RPC 2.0

MCP 2026-07-28 opens its Overview with a plain requirement:

> All messages between MCP clients and servers **MUST** follow the
> [JSON-RPC 2.0](https://www.jsonrpc.org/specification) specification.

JSON-RPC 2.0 says this about a Response object's `id`:

> This member is REQUIRED. It MUST be the same as the value of the id member in the Request Object.
> If there was an error in detecting the id in the Request object (e.g. Parse error/Invalid
> Request), it MUST be Null.

MCP's own published schema, at the commit pinned below, makes `id` optional on an error response
and gives it a type that excludes null:

```
JSONRPCErrorResponse.required : ["error", "jsonrpc"]
RequestId.type                : ["string", "integer"]
```

Both documents describe the same situation, a request whose id could not be read. JSON-RPC 2.0
prescribes `"id": null` for it. The MCP schema rejects that value and permits the member to be
absent instead, which JSON-RPC 2.0 does not allow. No single message satisfies both.

Worth noting because it shows the divergence is not an oversight in kind: MCP flags the *request*
side of this explicitly, "Unlike base JSON-RPC, the ID **MUST NOT** be `null`". The response side
carries no such note.

## Run it

```bash
curl -sLO https://raw.githubusercontent.com/modelcontextprotocol/modelcontextprotocol/271ecc9accafdd9b83a3c869fa67c22953b2af80/schema/2026-07-28/schema.json
python3 check.py --schema schema.json
```

Standard library only, no network at check time, no dependencies. The script refuses any schema
whose bytes are not the pinned ones, because a different revision is a different subject and
checking it silently would report a result about bytes nobody pinned.

The exit code is the verdict:

| Exit | Meaning |
|-----:|---------|
| 0 | the divergence is present, as documented here |
| 1 | a vector did not classify as its filename claims, so this example is wrong |
| 2 | the divergence is gone upstream, so this example is stale |
| 3 | the schema is unreadable or is not the pinned revision |

Exit 2 is the useful one over time. If MCP either requires `id` or admits null, the check flips by
itself and says so, rather than sitting here asserting something that stopped being true.

## Vectors

Two rejecting twins and one accepting twin. The accepting twin is what makes this a bounded finding
rather than a claim that the two specifications disagree generally.

| Vector | JSON-RPC 2.0 | MCP 2026-07-28 |
|--------|--------------|----------------|
| `n1-jsonrpc-conforming-error-response.json` (`"id": null`) | conforming | rejected |
| `n2-mcp-conforming-error-response.json` (`id` absent) | rejected | conforming |
| `p1-both-conforming-error-response.json` (`"id": 1`) | conforming | conforming |

## Pinned input

- repository: `modelcontextprotocol/modelcontextprotocol`
- commit: `271ecc9accafdd9b83a3c869fa67c22953b2af80` (2026-07-28T16:42:34Z)
- path: `schema/2026-07-28/schema.json`
- sha256: `ef70b61f99b6d2e5e3b46863822eab08dff6a45bedc7a08914e0e5b133f40203`

The digest was recomputed from the bytes served for that commit, not from a branch tip that can
move underneath the claim.

## A second, weaker observation

The specification calls the TypeScript schema "the source of truth for all protocol messages and
structures". That source declares:

```ts
export type RequestId = string | number;
```

while the generated JSON Schema says `["string", "integer"]` and the prose says "a string or integer
ID". The generated artifact is narrower than the source it is generated from.

This one resolves rather than contradicting. `typescript-json-schema` maps a bare `number` to
`integer` by default, and this schema uses the `@TJS-type number` escape hatch seven times, on
`temperature` and `priority`, exactly where floats belong. Integer is intended for `RequestId`.

It is still a documentation defect: a reader who takes the source of truth at its word, as the
specification invites, builds a type that accepts `1.5`. The convention that makes it correct is
nowhere in the file.

## Non-claims

This says nothing about any MCP implementation. It is a finding about specification text and a
published schema, checked against each other.

It does not claim the divergence is harmful in practice. Most implementations will never emit a
response for an unreadable id, and those that do will pick one form and be readable by peers that
expect it. What it does claim is narrower: an implementer who reads both documents and tries to obey
both cannot, and nothing in the MCP text warns them.

It does not propose a fix. Either direction closes the gap and the choice belongs to the
specification's maintainers.

## Why this lives here

Assay parses MCP transcripts, so this is upstream of code in this repository rather than an
observation from the sidelines. `crates/assay-core/src/mcp/parser.rs` currently folds an absent id
and a null id onto the same value, which under this specification are different things. That is
tracked separately.
