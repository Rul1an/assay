# Tool-decision truth layer (`assay.tool_decision_truth.v0`)

Status: experimental v0 primitives and conformance vectors. Field names, helper names, and digest
surfaces may change before this is promoted out of experimental. This page documents the current
claim ceiling for the public primitives; it does not mark the broader design as complete.

## Why this exists

Assay policies say what tool calls and argument shapes are declared. An MCP proxy can observe a
specific tool call. The tool-decision truth layer binds those two sides together so a consumer can
ask one narrow question:

> Did this observed tool decision match the declared policy constraints that Assay can evaluate?

The answer is a deterministic lattice verdict over declared and observed data:

```text
invalid > mismatch > incomplete > match
```

The layer is intentionally narrower than enforcement. It records and recomputes a decision
comparison. It does not forward, block, or authorize the tool call.

## Claim

Given a declared policy digest, an observed-input digest, and optional evidence for declared
constraints such as approval, scope, class, and redaction, Assay can deterministically recompute:

- a per-decision verdict: `match`, `incomplete`, `mismatch`, or `invalid`;
- a run verdict as the lattice maximum over ordered decisions;
- a content digest for the carrier record; and
- a recipe row that cites the carrier by content digest and fails closed on tampering.

A `match` means every declared constraint relevant to that decision was evaluated or proven not
applicable inside this experimental surface. It does not mean the tool call was safe, compliant, or
semantically correct.

## Non-claims

- does not prove runtime truth outside the observed MCP/tool-call boundary;
- does not infer intent, maliciousness, or user authorization;
- does not prove provider-side side effects happened or persisted;
- does not certify safety, compliance, or policy quality;
- does not replace enforcement and does not make an allow/deny decision in the action path;
- does not claim complete coverage when required evidence is absent;
- does not expose raw tool arguments, secrets, tokens, or key material;
- does not provide an OpenTelemetry projection or pinned OTel snapshot;
- does not integrate with the pack writer yet; the recipe-row primitive exists separately;
- does not provide a stable external schema until the experimental marker is removed.

## Three zones

The carrier separates identity, provenance, and classification so replay identity stays stable:

| Zone | Fields | In decision identity? |
|---|---|---|
| observed-input identity | `tool_name`, `args_digest`, `order`, `observed_input_digest` | yes, through `observed_input_digest` |
| observation provenance | `source_class`, `call_id`, `result_status`, `identity_state`, `key_id` | no |
| classification output | `declared_policy_digest`, `decision_identity`, `decision_verdict`, `declared_ref` | declared digest is part of identity; verdict is not |

`decision_identity` is the pair `(observed_input_digest, declared_policy_digest)`. It is the stable
join key for the logical decision. It is not the full carrier content digest.

## Declared side

`McpPolicy::declared_constraint_digest_experimental()` projects the declared constraint surface:

- `version`;
- tool allow/deny lists;
- class, approval, scope, and redaction lists;
- per-tool `schemas`; and
- `enforcement`.

Operational knobs such as runtime monitors, limits, discovery, signatures, and tool pins are outside
this digest. Set-like fields are sorted by canonical bytes so reordering does not move the digest.
Legacy constraints are normalized into the same view used by the verdict gate, and an explicit
schema takes precedence over a migrated legacy constraint for the same tool.

Known limitation: schema normalization is flat v0. Top-level `required` and direct
`properties.*.enum` arrays are normalized; nested schema structures are not recursively normalized
yet.

## Observed side

`args_digest(args, key, key_id)` drops secret-like argument keys recursively, then computes a
domain-separated keyed digest. The output includes the key id:

```text
hmac-sha256:<key_id>:<hex>
```

The raw arguments are never stored in the carrier. Secret-like keys are dropped rather than hashed,
because hashing low-entropy or known-format secrets can still invite recovery or correlation.

`observed_input_digest(tool_name, args_digest, order)` hashes only:

```json
{
  "tool_name": "deploy",
  "args_digest": "hmac-sha256:fixture-kid-v0:...",
  "order": 0
}
```

Changing the tool name, non-secret argument content, or order changes the observed-input digest.
Changing provenance fields does not.

## Verdict gate

`decision_verdict(...)` evaluates the declared policy against one observed decision. It uses the same
normalized declared view as the declared digest.

The gate evaluates these axes:

- tool name allow/deny, using the policy engine's `*` pattern semantics;
- per-tool argument schema;
- identity state: `present`, `absent`, `required_missing`, or `invalid`;
- tool class allow/deny;
- declared approval;
- declared scope; and
- declared redaction.

The load-bearing rule is conservative: if a declared constraint applies but the caller did not
provide enough evidence to evaluate it, the axis is `incomplete`, never `match`. Malformed declared
schemas are `invalid`, not panics and not missing evidence.

`run_verdict(decision_verdicts, orders)` computes the lattice maximum across a run. Duplicate orders
or mismatched verdict/order lengths are `invalid`.

## Carrier shape

Example carrier, shortened:

```json
{
  "schema": "assay.tool_decision_truth.v0",
  "tool_name": "deploy",
  "args_digest": "hmac-sha256:fixture-kid-v0:...",
  "order": 0,
  "source_class": "authoritative_boundary",
  "call_id": "deploy_match",
  "result_status": "ok",
  "identity_state": "present",
  "key_id": "fixture-kid-v0",
  "declared_ref": null,
  "decision_verdict": "match",
  "observed_input_digest": "sha256:...",
  "declared_policy_digest": "sha256:...",
  "decision_identity": {
    "observed_input_digest": "sha256:...",
    "declared_policy_digest": "sha256:..."
  }
}
```

Allowed carrier vocabularies are append-only:

| Field | Current values |
|---|---|
| `source_class` | `authoritative_boundary`, `reported_trace`, `inferred` |
| `result_status` | `ok`, `error`, `n/a` |
| `identity_state` | `present`, `absent`, `required_missing`, `invalid` |

The carrier builder rejects unknown values rather than minting a label a consumer might trust.

## Pack recipe-row primitive

The recipe-row helper binds a carrier to an Evidence Pack row without defining a new pack version.
It uses two different digests:

| Digest | Meaning |
|---|---|
| `decision_identity_digest` | digest of `(observed_input_digest, declared_policy_digest)`; stable logical join key |
| `carrier_content_digest` | digest of the full canonical carrier; citation target for the row |

The row cites the carrier by `carrier_content_digest` and carries `decision_identity_digest` as a
join key. Verification recomputes both from the supplied carrier, checks the row envelope
(`type`, `schema`, `canonicalization`, `digest_subject`), checks the run verdict vocabulary, and
rejects a row that claims a less severe verdict than the carrier it cites.

Current scope: the primitive and verifier exist. A production pack-emission path that writes this
row into real packs is a separate follow-up.

## Conformance vectors

The committed vectors live at:

```text
crates/assay-core/tests/fixtures/tool_decision_truth/vectors.json
```

The test generator and reproducer live at:

```text
crates/assay-core/tests/tool_decision_truth_vectors.rs
```

The vectors cover:

- verdict cases for `match`, `incomplete`, `mismatch`, and `invalid`;
- pattern deny matching;
- missing argument capture;
- absent, missing, and invalid identity states;
- approval evidence present, absent, and denied;
- malformed schema handling;
- duplicate-order and arity run-lattice failures;
- real keyed carriers with no raw arguments;
- positive pack rows over real carriers; and
- negative pack rows for tampered verdicts, foreign recipe/envelope values, and malformed digests.

Regenerate intentionally with:

```bash
UPDATE_TDT_VECTORS=1 cargo test -p assay-core --test tool_decision_truth_vectors
```

The normal test path recomputes the fixture from committed bytes and fails if the fixture drifts from
the current implementation.

## Current public status

Green:

- contract primitives;
- runtime helper behavior;
- declared digest and verdict-gate coherence;
- carrier-content pack-row primitive;
- fail-closed verifier; and
- conformance vectors with recompute-from-bytes guard.

Open:

- OpenTelemetry export against a pinned snapshot;
- integrated pack emission; and
- promotion from experimental names to a stable public schema.
