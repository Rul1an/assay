# MCP Registry Discovery Audit

Date: 2026-05-06

Status: Operational discovery note.

This note tracks where the released Assay MCP server package can be found, where
registry metadata is stale, and where third-party discovery surfaces still need
follow-up. It is not a launch page, partnership claim, hosted-service claim, or
new product surface.

## Source Of Truth

Assay registry metadata is release-attached, not hand-maintained from `main`.
The canonical file for the current line is:

- GitHub release: [`v3.9.2`](https://github.com/Rul1an/assay/releases/tag/v3.9.2)
- Registry metadata:
  [`server.json`](https://github.com/Rul1an/assay/releases/download/v3.9.2/server.json)
- Canonical registry name: `io.github.Rul1an/assay-mcp-server`
- Package type: `mcpb`
- Transport: `stdio`
- Compatibility: Linux MCPB package for this line

The generated `server.json` for `v3.9.2` validates with:

```bash
mcp-publisher validate release/server.json
```

## Official Registry State

Checked on 2026-05-06.

| Surface | Observed state | Action |
| --- | --- | --- |
| Official MCP Registry search for `assay` | Canonical `io.github.Rul1an/assay-mcp-server` is active at `3.9.2` and marked latest. The older canonical `3.5.1` row remains active but not latest. | Keep publishing the canonical entry from release-attached `server.json` after each release. |
| Legacy official registry identity | Stale `io.github.Rul1an/assay` at `3.1.0` is deprecated with a pointer to the canonical `io.github.Rul1an/assay-mcp-server` identity. | Leave deprecated, not deleted, so older discovery links have a safe migration signal. |
| `v3.9.2` release metadata | `server.json` exists as a release asset, validates against the registry schema, and has been published to the canonical official registry identity. | Use this file for publish. Do not create a root-level static `server.json`. |
| GitHub repo homepage | Updated to `https://getassay.dev` because the previous `http://www.getassay.dev` value did not resolve reliably. | Keep repo metadata aligned with the canonical site URL. |

Publication command:

```bash
gh release download v3.9.2 --repo Rul1an/assay --pattern server.json --dir release
mcp-publisher validate release/server.json
mcp-publisher login github
mcp-publisher publish release/server.json
curl -fsSL 'https://registry.modelcontextprotocol.io/v0/servers?search=assay'
```

If `mcp-publisher publish` reports an expired registry token, rerun
`mcp-publisher login github` and repeat the publish step. Boring on purpose,
but it keeps release truth honest.

If a stale legacy identity needs to remain discoverable but should no longer be
presented as current, use the non-destructive status flow:

```bash
mcp-publisher status --status deprecated \
  --message 'Canonical registry identity is io.github.Rul1an/assay-mcp-server; use the latest version there.' \
  io.github.Rul1an/assay 3.1.0
```

## Third-Party Discovery State

Checked on 2026-05-06. These directories are external indexes. Presence there
does not imply endorsement, official integration, hosted availability, or
registry truth.

| Surface | Observed state | Action |
| --- | --- | --- |
| [MCPBench](https://mcpbench.ai/servers/io.github.Rul1an/assay) | Page returns `200`. | Recheck after the official registry refresh. If the page still tracks the legacy `io.github.Rul1an/assay` identity, prefer correcting the official registry first. |
| [mcpservers.org](https://mcpservers.org/th/servers/rul1an/assay) | Page returns `200`. | Recheck after the official registry refresh. Do not claim freshness unless the listing shows the current release line. |
| [mcp.so](https://mcp.so/server/assay) | Page returns `200`. | Recheck after the official registry refresh. Treat as third-party discovery only. |
| [PulseMCP](https://www.pulsemcp.com/servers/gh-rul1an-assay) | Search previously found a listing, but direct `curl` is blocked by Cloudflare with `403`. | Verify manually in a browser before making any freshness claim. |
| [Glama MCP directory](https://glama.ai/api/mcp/v1/servers?query=assay) | API search returns zero Assay results. | Candidate follow-up after official registry is current. Keep the submission factual: local MCPB package, no hosted server claim. |
| [Smithery](https://api.smithery.ai/servers?q=assay) | API search returns unrelated results for `assay`, `Rul1an/assay`, and `assay-mcp-server`. | Candidate follow-up only if Smithery can represent a local stdio MCPB package honestly. Do not imply a remote deployment. |
| [mcp.directory](https://mcp.directory/server/assay) | Exact page returns `404`. | Candidate follow-up after official registry is current. |

## Guardrails

This audit must not be used to claim:

- official partnership or upstream endorsement;
- hosted Assay MCP service availability;
- cross-platform MCPB availability beyond the release asset set;
- Trust Basis or evidence-receipt semantics changes;
- freshness in third-party indexes that have not been rechecked;
- that a third-party directory listing is the source of truth.

## Release Cadence

After each Assay release that includes an MCPB package:

1. Confirm the release includes `assay-mcp-server-<version>-linux.mcpb`.
2. Confirm the release includes generated `server.json`.
3. Run `mcp-publisher validate` against that exact `server.json`.
4. Publish the canonical `io.github.Rul1an/assay-mcp-server` entry.
5. Recheck the official registry search result.
6. Recheck third-party directories and update this audit only with observed
   facts.
