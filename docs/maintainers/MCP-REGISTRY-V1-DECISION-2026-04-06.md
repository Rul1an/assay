# MCP Registry v1 Decision

Date: 2026-04-06

## Decision

Assay's first honest MCP Registry path is:

- canonical surface: `assay-mcp-server`
- package type: `mcpb`
- transport: `stdio`
- generated metadata: `release/server.json`
- compatibility claim: Linux only for the first line

The repository root should not carry a hand-maintained `server.json`.
Registry metadata must be rendered from a real release asset and its real SHA-256.

## Why this is the right v1

The official MCP Registry accepts supported package types such as `mcpb` and `oci`. A Rust crate or renamed release tarball is not enough on its own.

Assay already has a real standalone MCP server binary in `assay-mcp-server`, so the cleanest v1 is to package that binary honestly instead of inventing a second product identity around `assay mcp wrap`.

`mcpb` is the best current fit because:

- it matches the existing local stdio server shape
- it works with GitHub release assets
- it avoids pretending Assay already has a mature container delivery story
- it keeps the first publishable line small and understandable

## Why not OCI yet

OCI stays open as a later route, but not for this first pass.

Reasons:

- the repo does not currently ship a mature container packaging line for `assay-mcp-server`
- adding container delivery now would expand scope from registry truth into runtime packaging strategy
- the current need is package-truth, not more surfaces

If Assay later develops a strong container-native install story, OCI can be revisited as a second supported package path.

## Why not remote yet

Remote is also out of scope for this first line.

Reasons:

- there is no hosted Assay MCP service being published here
- remote would change the trust and operational boundary
- the current goal is to publish a local/server package truthfully

## Non-goals

This decision does not mean:

- Assay is already published in the official MCP Registry
- Assay is already present in MCPCentral
- `assay mcp wrap` gets a separate registry identity
- cross-platform MCPB support is promised in v1

## Guardrails

The v1 line should keep these boundaries:

- one registry identity only: `assay-mcp-server`
- generated `release/server.json`, not hand-edited root metadata
- no claim that a `.tar.gz` is an `mcpb`
- no assumption that ETL from another registry proves publication

## Revisit triggers

Revisit this decision only if one of these changes materially:

- Assay gains a real OCI/container delivery story for `assay-mcp-server`
- Assay decides to offer a hosted remote MCP service
- the official MCP Registry changes supported package types or publication rules
- the Linux-only MCPB line proves too restrictive for actual adoption

## Next release-line step

On the next real release line:

1. confirm the release includes `assay-mcp-server-<version>-linux.mcpb`
2. confirm the release includes generated `server.json`
3. confirm `mcp-publisher validate release/server.json` still passes
4. decide separately whether to publish that release to the official registry

Until that point, this remains a truthful packaging foundation, not a publication claim.
