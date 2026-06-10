# MCP tool-manifest drift (`assay.mcp_manifest_observed.v0`)

Status: coarse path shipped — spec + fixtures + digest guard (P60a), the `assay-mcp-server` producer
module (P60b), and the consumer coarse-drift gate (P60c, released in Plimsoll v0.8.0). Live upstream
observation now ships too, as the opt-in [manifest-observation proxy mode](mcp-upstream-proxy-mode.md)
(assay v3.23.0). Part of the [privileged-action evidence](privileged-action-evidence.md) set.

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

Built by the producer from observed tool definitions. The live source is the latest fully observed
`tools/list`, captured on the wire by the [manifest-observation proxy mode](mcp-upstream-proxy-mode.md)
(assay v3.23.0); the producer also serves supplied-artifact review. Per-tool digests are
diagnostic/supporting detail in v0; the v0 consumer gate uses only the overall `manifest_digest`
(per-tool drift reason codes are P60d/v1).

```json
{
  "schema": "assay.mcp_manifest_observed.v0",
  "status": "observed",
  "server": { "id": "github" },
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

### `status` (enum: `observed` | `not_observed` | `ambiguous`)

The top-level observation state, so the consumer never has to infer it:
- `observed` — a `tools/list` was observed; `manifest_digest` is computed;
- `not_observed` — no `tools/list` was observed; an artifact state, never a missing file. `manifest_digest`
  is null, `tools_list_observed` false, `tools_list_complete` unknown, `tool_digests` empty;
- `ambiguous` — the observed list has duplicate tool names. `manifest_digest` is null (an ambiguous
  identity is never claimed clean), but per-tool detail and counts are still carried.

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

## Granular per-tool drift (P60d, `assay.declared_mcp_manifest.v0`)

The coarse gate (above) compares one overall `manifest_digest`. P60d adds **per-tool** drift: which
tool was added, removed, or changed, with privileged tools flagged. **P60d explains which tool digest
drifted; it still does not explain which field changed or whether the change is malicious.** Field-level
attribution (description vs schema vs annotations) is P60d-v2.

P60d v1 is **Option A — presence + per-tool digest, consumer-only**: it diffs the observed
`tool_digests[]` (already in `assay.mcp_manifest_observed.v0`, P60b) against a declared per-tool
baseline. **No producer change** — the observed artifact already carries `{name, tool_digest,
privileged, privilege_classification, action_class}` per tool.

### Baseline: `assay.declared_mcp_manifest.v0`

Operator-pinned, declared-not-trusted (same discipline as the coarse `declared_mcp_manifests` map). It
is structurally the `observed` block of a known-good complete run:
```json
{
  "schema": "assay.declared_mcp_manifest.v0",
  "server": { "id": "github" },
  "canonicalization": "assay.mcp_manifest_projection.v0",
  "manifest_digest": "sha256:...",
  "tools": [
    { "name": "github.add_deploy_key", "tool_digest": "sha256:...",
      "privileged": true, "privilege_classification": "classified", "action_class": "github_deploy_key" }
  ]
}
```
`manifest_digest` recomputes from `tools[].{name, tool_digest}` via the same JCS canonicalization as the
observed manifest (so a clean baseline's digest equals the P60a-anchored value). v1 baselines are
hand-authored or copied from a clean observed run — no `promote` helper in v1.

### Validity checks (each is inconclusive — never a per-tool diff against a bad baseline)

```
recompute(declared.tools) != declared.manifest_digest   -> declared_manifest_digest_mismatch
duplicate names in declared.tools                        -> declared_mcp_manifest_ambiguous
observed.server.id != declared.server.id                 -> mcp_manifest_server_mismatch
observed.canonicalization != declared.canonicalization   -> mcp_manifest_canonicalization_mismatch
observed.status = not_observed                           -> inconclusive_manifest_not_observed
observed.status = ambiguous OR duplicate observed names  -> mcp_manifest_observation_ambiguous
```
And a consumer (P60d-b) check when BOTH baselines are supplied: the coarse
`declared_mcp_manifests[server].manifest_digest` must equal `declared_mcp_manifest.manifest_digest`,
else `declared_manifest_baseline_conflict` (two declared truths → inconclusive).

### Per-tool finding matrix

```
observed name not in declared        -> observed privileged ? mcp_new_privileged_tool : mcp_tool_added
declared name not in observed:
  observed complete                  -> declared privileged ? mcp_privileged_tool_removed : mcp_tool_removed
  observed partial/unknown           -> NO per-tool removal finding; one inconclusive_manifest_partial_observation
                                        ("removals are not evaluable: the manifest observation was incomplete")
name in both, tool_digest differs    -> (observed OR declared privileged) ? mcp_privileged_tool_changed : mcp_tool_changed
name in both, tool_digest equal      -> no finding
```
Every per-tool finding contributes to `pending_tool_manifest_review`. Severity: the privileged variants
(`mcp_new_privileged_tool`, `mcp_privileged_tool_changed`, `mcp_privileged_tool_removed`) are high; the
non-privileged variants (`mcp_tool_added`, `mcp_tool_changed`, `mcp_tool_removed`) are findings too, at
lower severity — a non-privileged tool's surface change can still affect prompt/tool behavior, so it is
reviewable, just not as loud. Additions and changes among observed tools are assertable even under
partial observation; **only removals are suppressed under partial/unknown** (a "missing" tool could be
on an unobserved page).

## What v0 is NOT

No per-field attribution (which field changed) — that is P60d-v2; no behavior-drift detection, no LLM
risk scoring, no automatic block on legitimate churn, no maliciousness classification, no pre-flight
scan, no producer change in P60d v1.

## Producer (P60b)

`assay-mcp-server` carries the producer module (`manifest_observed`) that builds this record from
observed tool definitions. It reuses exactly the P60a canonicalization, so it reproduces the committed
P60a digests byte-for-byte (a cross-layer guard test feeds the canonical-example raw tool definitions
through the producer and asserts the committed `tool_digest`/`manifest_digest`). `privileged` is taken
from the P57c classifier keyed on the tool name (the server's annotations ride into the digest but
never decide privilege); `tools_list_complete` is supplied by the observer and never guessed here;
duplicate names produce `status: ambiguous` with a null `manifest_digest`. The producer decides
nothing about whether drift matters — baseline comparison and verdicts are the consumer's job (P60c).

The live observation wiring — capturing a proxied `tools/list` (including the full pagination chain to
prove `complete`) and an output path/flag — is a separate slice; P60b lands the producer and its
exact-digest guarantee.

### Live observation (P60b2) — topology finding

A read-only investigation of `assay-mcp-server` settled where an observed `tools/list` could come from:

- upstream forwarding seam: **none** — the server terminates the JSON-RPC protocol and serves its own
  built-in tools locally;
- `tools/call` path: handled locally (policy evaluation read from disk and synthesized), not relayed
  to an upstream;
- `tools/list` path: always the static built-in list (`tools::list_tools()`), never an upstream
  response;
- server identity: a constant label, not a real upstream identity; no pagination/cursor state and no
  upstream-passthrough tests exist.

Conclusion: live manifest observation is **not** a small tap on an existing seam; it requires a new
MCP upstream passthrough/proxy mode (config naming an upstream, a connection/child manager, a
forwarding handler, and a `tools/list` that observes the upstream response and its pagination chain).
That was its own design, specified separately before any code, and **shipped in assay v3.23.0** as the
opt-in [manifest-observation proxy mode](mcp-upstream-proxy-mode.md) (it forwards a tiny method
allowlist only and never forwards privileged `tools/call`). The artifact/file-based path remains too: a
producer builds `assay.mcp_manifest_observed.v0` from observed tool definitions, and the consumer
reviews a supplied artifact against a declared baseline. The producer is deliberately not wired to
`tools::list_tools()` — the server's own served tools are not an observed upstream manifest, and
emitting them as one would misstate what was observed.

## PR sequence

```
P60a   spec + fixtures + canonicalization examples + a digest-recompute guard test
P60b   producer: assay-mcp-server manifest_observed module emits assay.mcp_manifest_observed.v0
P60c   Plimsoll: coarse drift gate -> pending_tool_manifest_review (opt-in, coverage-gated)
P60b2  live observation: topology finding (above) -> manifest-observation proxy mode (SHIPPED v3.23.0)
P60d-a granular drift spec + assay.declared_mcp_manifest.v0 fixtures + guard test (NO producer change)
P60d-b Plimsoll granular consumer (--declared-mcp-manifest) -> per-tool reason codes + coarse-consistency
P60d-v2 LATER: field-level attribution (per-field digests in producer + baseline + consumer)
```

## Reference fixtures

`crates/assay-mcp-server/tests/fixtures/mcp_manifest_drift/`: canonicalization examples (a per-tool
projection and a manifest projection with their committed digests, recomputed in the guard test); a
coarse verdict corpus (same digest, drift, schema/annotations changed, new privileged tool, tool
removed, not observed, partial pagination, duplicate names), each labelled with its expected v0
verdict; and for P60d, a P60a-anchored per-tool baseline (`declared_per_tool_baseline.json`, whose
`manifest_digest` the guard test recomputes and equals the committed P60a value) plus a granular-diff
corpus (`granular_diff_cases.json`) covering the per-tool matrix and every validity check, each
labelled with its expected findings and inconclusive codes.
