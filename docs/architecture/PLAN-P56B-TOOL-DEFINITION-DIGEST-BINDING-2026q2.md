# PLAN — P56b Tool Definition Digest Binding (Q2 2026)

- **Date:** 2026-04-29
- **Owner:** Evidence / MCP Security
- **Status:** Implemented
- **Scope:** Bind supported `assay.tool.decision` evidence to a bounded MCP tool
  definition digest when a reviewed tool definition surface is available,
  without claiming tool safety, signature validity, or registry truth.

## 1. Goal

Make supported MCP decision evidence show which bounded tool definition surface
was reviewed for the tool that was allowed, denied, or sent to approval.

P56a made the policy snapshot boundary self-describing. P56b adds the companion
tool-definition boundary:

```text
decision evidence
  + policy_snapshot_digest
  + tool_definition_digest
```

The point is reviewability. P56b does not make the tool safe, trusted, signed,
approved, or globally registered.

P56b makes the reviewed MCP tool-definition boundary self-describing on
supported `assay.tool.decision` events. It does not add tool truth, signature
trust, registry trust, or implementation truth.

## 2. Current Seams

Assay already has three related surfaces:

- `ToolIdentity` in `crates/assay-core/src/mcp/identity.rs`, currently made
  from `server_id`, `tool_name`, `schema_hash`, and `meta_hash` for pinning and
  drift detection.
- MCP proxy discovery, which computes tool identity from `tools/list` responses
  and caches it for later `tools/call` decisions.
- `assay tool sign` / `assay tool verify`, which use `x-assay-sig`, JCS, and
  DSSE-style PAE over a tool definition with the signature field removed.

Those seams are useful but not yet the same as a single, self-describing
decision-evidence digest. P56b should create that reviewer surface without
collapsing identity pinning, signing, and runtime decision evidence into one
overbroad concept.

## 3. Supported v1 Surface

P56b v1 should be limited to supported MCP tool definitions observed from
`tools/list`.

The bounded canonical input is a JSON object containing only:

- `name`;
- optional `description`;
- optional `input_schema`;
- optional `server_id` only when the observed `tools/list` definition is
  already emitted in a server-scoped context and omitting the server label would
  collapse distinct reviewed definitions.

Before canonicalization:

- top-level fields are closed for v1: only `name`, `description`,
  `input_schema`, and conditionally `server_id` are allowed in the projection;
- `inputSchema` and `input_schema` MUST normalize to `input_schema`;
- `description` is part of the review boundary because it is agent- and
  reviewer-facing tool text;
- `description` values MUST be trimmed at the boundary; if the trimmed value is
  empty, the field MUST be treated as absent; internal whitespace is preserved;
- `server_id` MUST be absent unless the observed definition is server-scoped and
  omission would collapse distinct reviewed definitions;
- the full normalized `input_schema` object MUST be included, not a reduced
  schema subset;
- JSON object key ordering MUST NOT affect the digest;
- no `x-assay-sig` field;
- no provider metadata or vendor extension fields outside `input_schema`;
- no top-level vendor extensions, annotations, display hints, or extra metadata
  blobs;
- no runtime result payload;
- no full registry object;
- no inferred fields from a later `tools/call`.

The digest input is the JCS canonicalization of that bounded projection.
Unsupported top-level fields are excluded before JCS canonicalization. Schema
keywords inside `input_schema` are preserved as part of the reviewed schema
surface rather than normalized into a smaller Assay-specific schema language.
This means vendor-specific schema keywords inside `input_schema` remain part of
the reviewed schema surface, while vendor/provider fields beside the tool
definition are excluded.

The proposed decision-evidence projection is:

```json
{
  "tool_definition_digest": "sha256:...",
  "tool_definition_digest_alg": "sha256",
  "tool_definition_canonicalization": "jcs:mcp_tool_definition.v1",
  "tool_definition_schema": "assay.mcp.tool-definition.snapshot.v1",
  "tool_definition_source": "mcp.tools/list"
}
```

If `tool_definition_digest` is present, the field cluster is atomic:
`tool_definition_digest_alg`, `tool_definition_canonicalization`,
`tool_definition_schema`, and `tool_definition_source` MUST also be present.

The v1 cluster values are closed:

- `tool_definition_digest_alg` MUST be exactly `"sha256"`;
- `tool_definition_canonicalization` MUST be exactly
  `"jcs:mcp_tool_definition.v1"`;
- `tool_definition_schema` MUST be exactly
  `"assay.mcp.tool-definition.snapshot.v1"`;
- `tool_definition_source` MUST be exactly `"mcp.tools/list"`.

Any later source, canonicalization, or schema widening requires a deliberate
new version.

## 4. Relation to Existing ToolIdentity

`ToolIdentity` remains the runtime pin/drift surface. It should not be silently
renamed into P56b.

`tool_definition_digest` is not a replacement for `ToolIdentity`.

For v1:

- `ToolIdentity` may continue carrying `schema_hash` and `meta_hash`;
- `tool_definition_digest` should be a separate canonical digest over the
  bounded tool definition projection;
- the implementation may derive both from the same `tools/list` observation;
- neither surface should invent identity when `tools/list` was not observed.
- `ToolIdentity` may still be used for runtime drift/pinning even when no
  reviewable `tool_definition_digest` is present.

This keeps old policy-pin behavior compatible while giving reviewers a single
digest for the reviewed tool definition boundary. Absence of
`tool_definition_digest` must not be reinterpreted as absence of tool identity.

## 5. Relation to x-assay-sig, DSSE, and Transparency Logs

P56b does not require a signed tool.

When a signed tool definition is present, P56b should align with the existing
tool-signing domain where possible:

- `x-assay-sig` remains the signature carrier;
- DSSE PAE remains part of the signing/verification path;
- `tool_definition_digest` is the decision-evidence review digest;
- future signature fields, if added, should be separate from digest visibility.

When a signed tool definition and a decision-visible `tool_definition_digest`
are derived from the same observed `tools/list` surface, they SHOULD use the
same bounded canonical projection, minus `x-assay-sig`, so review and
verification do not diverge unnecessarily.

If signing uses that same bounded projection, any divergence between the signing
input and the `tool_definition_digest` input must be treated as a deliberate
versioned contract change, not an implementation detail.

P56b must not claim:

- the signature was verified;
- the signer is trusted;
- a transparency-log entry exists;
- the tool definition came from a trusted registry.

Those are later signing/provenance slices. P56b only makes the bounded tool
definition digest visible beside the decision.

## 6. Implementation Shape

Recommended implementation slices:

1. Add a small canonical tool-definition projection helper for the supported MCP
   `tools/list` shape.
2. Add deterministic tests for key-order stability, `inputSchema` normalization,
   and absence of raw provider metadata.
3. Cache a `ToolDefinitionBinding` beside the existing `ToolIdentity` in the MCP
   proxy discovery path.
4. Thread the binding into supported `assay.tool.decision` emission paths.
5. Add optional `tool_definition_*` fields to stable payload parsing as additive
   Evidence Contract v1 data.
6. Update ADR-006 and Evidence Contract v1 with the v1 shape and non-goals.

Do not reconstruct a tool definition digest from a later tool call. Project only
from an observed bounded definition surface.

## 7. Boundary

P56b is digest visibility, not tool truth.

It does not mean:

- the tool is safe;
- the tool is signed;
- the signature is valid;
- the signer is trusted;
- the tool registry is authoritative;
- the tool implementation matches the definition;
- the tool result is safe;
- the policy snapshot approved the tool definition.
- the tool definition is retrievable, embeddable, or viewer-ready.

Missing `tool_definition_digest` means the tool-definition boundary is not
visible, not that the tool is safe.

## 8. Acceptance

- Supported `tools/list` to `tools/call` flows can emit `assay.tool.decision`
  with an atomic `tool_definition_*` field cluster.
- Decision paths without an observed bounded tool definition omit the cluster
  rather than inventing a digest.
- Tests prove digest determinism for equivalent bounded definitions.
- Tests prove allowed top-level field subsets produce the same digest regardless
  of JSON key ordering.
- Tests prove unknown top-level fields are excluded before digesting.
- Tests prove `inputSchema` and `input_schema` normalize to the same digest.
- Tests prove whitespace-only `description` handling is deterministic.
- Tests prove `server_id` inclusion and omission follow the v1 server-scoping
  rule.
- Tests prove `x-assay-sig` does not affect the digest.
- Tests prove equivalent signed and unsigned observed definitions can carry the
  same digest when only `x-assay-sig` differs.
- Tests prove provider metadata, top-level vendor fields, and raw tool bodies
  are not imported into the decision evidence path.
- Tests prove half-present `tool_definition_*` clusters are not emitted.
- Tests prove missing tool-definition digest visibility does not classify as
  safe or verified trust.
- Docs clearly distinguish `ToolIdentity`, `tool_definition_digest`,
  `x-assay-sig`, DSSE, and future transparency-log verification.
- No new Trust Basis claim, Trust Card schema bump, registry import, or
  signature-verification claim is introduced in P56b v1.
