# MCP tool-manifest drift (`assay.mcp_manifest_observed.v0`)

Status: spec + reference fixtures (P60a). No producer/consumer wired yet; the `assay-mcp-server`
producer (P60b) and the Plimsoll coarse-drift gate (P60c) build on this shape. Part of the
[privileged-action evidence](privileged-action-evidence.md) set.

## Why this exists

An MCP server that looks safe when its tools are approved can, at runtime, expose different tool
descriptions, input/output schemas, annotations, or new tools. Most mitigations scan the server
ahead of time; this one is the opposite, deterministic signal: a proof during the run that the
observed tool surface drifted from a declared/baselined manifest, by canonical digests, with no
model judgement. (The threat-model term for the hostile case is tool poisoning; this record does not
classify maliciousness.)

**Manifest drift is canonical-digest evidence, not maliciousness evidence.** A legitimate manifest
change also surfaces as drift and resolves to `pending_tool_manifest_review`, never a "malicious"
verdict.

## Claim and non-claims

**Claim:** Assay/Plimsoll detect that the observed MCP tool surface drifted from a declared/baselined
manifest, by canonical digests.

**Non-claims:**
- does not detect behavior drift under identical metadata (digest-invisible, by design);
- does not classify maliciousness; a legitimate change also surfaces as drift;
- is not a pre-flight scanner and not an LLM risk score;
- `privileged` is classifier-derived, not the server's own annotations as source of truth;
- does not infer tools outside the observed `tools/list`;
- an unobserved or partially observed manifest is inconclusive, never read as "no drift".

## Canonicalization (`assay.mcp_manifest_projection.v0`)

Digests are reproducible from committed bytes via JCS (RFC 8785, deterministic JSON
canonicalization). A third implementation must reproduce the same digests from this projection alone.

Per-tool projection:
```json
{
  "name": "<string>",
  "description": "<string or null>",
  "input_schema": "<JSON value or null>",
  "output_schema": "<JSON value or null>",
  "annotations": "<JSON value or null>"
}
```
`tool_digest = sha256(JCS(per-tool projection))`.

Manifest projection — the projection id is INSIDE the hashed preimage, so a shape change cannot
happen without a digest bump:
```json
{
  "projection": "assay.mcp_manifest_projection.v0",
  "tools": [ { "name": "<name>", "tool_digest": "sha256:..." } ]
}
```
Tools are sorted by `name`, then by `tool_digest` if duplicate names ever occur.
`manifest_digest = sha256(JCS(manifest projection))`.

## Observed manifest record (`assay.mcp_manifest_observed.v0`)

Emitted by the proxy from the observed `tools/list`. The proxy observes **every** `tools/list`
response and carries the latest. Per-tool digests are diagnostic/supporting detail in v0; the v0
Plimsoll gate uses only the overall `manifest_digest` (per-tool drift reason codes are P60d/v1).

```json
{
  "schema": "assay.mcp_manifest_observed.v0",
  "server": { "id": "github", "declared_manifest_digest": "sha256:..." },
  "observed": {
    "manifest_digest": "sha256:...",
    "canonicalization": "assay.mcp_manifest_projection.v0",
    "tool_count": 12,
    "privileged_tool_count": 2,
    "tools_list_observed": true,
    "tools_list_complete": "complete",
    "tool_digests": [
      {
        "name": "github.add_deploy_key",
        "tool_digest": "sha256:...",
        "privileged": true,
        "privilege_classification": "classified",
        "action_class": "github_deploy_key"
      }
    ]
  },
  "non_claims": [
    "does not judge whether a manifest change is malicious",
    "does not infer tools outside the observed tools/list",
    "does not detect behavior drift under identical metadata",
    "privileged is classifier-derived, not the server's own annotations"
  ]
}
```

### `tools_list_complete` (enum: `complete` | `partial` | `unknown`)

MCP list operations are paginated: a single `tools/list` response is not automatically the full
manifest, and a client continues following `nextCursor` until there is none.

- `complete` — only after the proxy observed the full list operation, including all paginated pages
  until no `nextCursor` remains;
- `partial` — pagination started but the chain was not completed (an error interrupted it, or only a
  subset was observed);
- `unknown` — the proxy saw a `tools/list`-shaped response but cannot prove the server's list
  operation was complete.

### `privilege_classification` (never bare bool)

`privileged` is classifier-derived (the P57c classifier leaf-names), and carries its classification so
`false` is never read as "safe":
- classified: `{ "privileged": true, "privilege_classification": "classified", "action_class": "github_deploy_key" }`
- unclassified: `{ "privileged": false, "privilege_classification": "unclassified", "action_class": null }`

## Baseline (v0: policy-map/config)

The baseline is pinned per server in the consumer policy (consistent with `declared_credentials` and
`network_declared_endpoints`); it is declared/baselined, not intrinsically trusted:
```yaml
declared_mcp_manifests:
  github:
    manifest_digest: "sha256:..."
```
A richer `assay.declared_mcp_manifest.v0` with per-tool expected digests is a later (P60d) artifact.

## Coverage rules and verdicts (Plimsoll, P60c — v0 coarse)

```
tools_list_observed = false                                  -> inconclusive_manifest_not_observed
tools_list_observed = true, complete = partial               -> inconclusive_manifest_partial_observation
                                                                (unless policy allows warning-only)
duplicate tool names in one observed manifest                -> manifest_observation_ambiguous (inconclusive)
observed.manifest_digest != baseline                         -> mcp_tool_manifest_drift -> pending_tool_manifest_review
  (drift is observed even when complete = unknown)
digest match, complete = unknown                             -> no drift finding, but a coverage warning
digest match, complete = complete                            -> no finding
```
Absence rule: "no observed `tools/list`" is not "no drift"; only "observed complete `tools/list` plus
a matching digest" is "no drift". A digest mismatch is drift even under `unknown` completeness; a
digest match under `unknown` completeness may not be claimed fully clean.

## What v0 is NOT

No per-tool granular drift reason codes (P60d/v1), no behavior-drift detection, no LLM risk scoring,
no automatic block on legitimate description churn, no maliciousness classification, no pre-flight
scan.

## PR sequence

```
P60a  this spec + fixtures + canonicalization examples + a digest-recompute guard test
P60b  producer: assay-mcp-server emits assay.mcp_manifest_observed.v0 (digest-only gate input, coverage)
P60c  Plimsoll: coarse drift gate -> pending_tool_manifest_review (opt-in, coverage-gated)
P60d  granular per-tool drift v1 + assay.declared_mcp_manifest.v0 (per-tool expected digests)
```

## Reference fixtures

`crates/assay-mcp-server/tests/fixtures/mcp_manifest_drift/`: canonicalization examples (a per-tool
projection and a manifest projection with their committed digests, recomputed in the guard test) and
a verdict corpus (same digest, drift, description/schema/annotations changed, new privileged tool,
tool removed, behavior-only identical metadata, not observed, partial pagination, duplicate names),
each labelled with its expected v0 verdict.
