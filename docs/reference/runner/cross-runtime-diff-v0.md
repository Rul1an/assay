# Assay-Runner Cross-Runtime Diff v0 Contract

> Internal Phase 2C contract slice. This page defines the first
> Assay-Runner cross-runtime capability-diff projection over normalized
> runner evidence sets recorded from *different* runtime fixtures. It is
> not a runner-emitted archive artifact, not a CLI surface, and not a
> product release contract.

The v0 cross-runtime diff answers one narrow question:

```text
Given two clean normalized runner evidence sets recorded from different
runtimes, what observed capability surface differs after explicit
fixture plumbing normalization?
```

The diff is descriptive. It reports added, removed, and unchanged
normalized capability values across runtimes, after exactly one
declared canonicalization rule has stripped run/work-dir prefixes. It
must not decide whether a difference is acceptable, must not infer
semantic equivalence beyond the declared rule, and must not synthesize
cross-runtime binding identity. Acceptability remains policy, reviewer,
or Harness responsibility.

## Decision Lineage

This contract freezes the combination resolved by the Phase 2C decision
gate at <https://github.com/Rul1an/assay/issues/1310> and recorded in
[`cross-runtime-diff-decisions.md`](cross-runtime-diff-decisions.md):

| Dimension | Choice | Meaning |
|---|---|---|
| Paths | **A1** | Work-dir prefix canonicalization only (replace with `<work>/`); fixture-local filenames remain observed surface values |
| Binding ids | **B3** | Binding ids are out of scope for cross-runtime comparability in v0; required only for within-runtime correlation |
| SDK metadata | **C1** | SDK metadata reported as side-band runtime provenance, not capability-surface |

The kernzin:

> v0 cross-runtime diff removes obvious fixture plumbing, preserves
> observed capability-surface differences, and avoids derived
> cross-runtime identity semantics. Binding ids remain required for
> within-runtime correlation, but are not themselves cross-runtime
> comparable in v0.

## Inputs

A v0 cross-runtime diff compares two evidence sets, named `base` and
`head`, **each recorded from a different runtime fixture**. Each side
provides two kinds of input plus one declared projector configuration
value.

### Primary capability-surface inputs

Each side must provide the same normalized artifacts required by
[`capability-diff-v0`](capability-diff-v0.md). These artifacts are the
exclusive source for `surface.*` set comparisons:

| Artifact | Role |
|---|---|
| `observation-health.json` | Determines whether the evidence set is clean enough for a clean cross-runtime diff |
| `capability-surface.json` | Provides the observed capability sets to compare across runtimes |
| `correlation-report.json` | Provides stable within-runtime binding identity through `tool_call_id` |

### Required side-band provenance input

`sdk_metadata` is reported as side-band runtime provenance per C1, so
v0 cross-runtime diff extends the input set with one required side-band
provenance artifact per side:

| Artifact | Role |
|---|---|
| `layers/sdk.ndjson` | Source of `sdk_name` and `sdk_version` for the [side-band `sdk_metadata` block](#sdk-metadata-side-band-provenance) |

`layers/sdk.ndjson` is **not** a capability-surface input. The
projector MUST consult it exclusively to populate the side-band
`sdk_metadata` block. Set comparisons in `surface.*` MUST NOT consume
`layers/*` streams. This preserves the
[`capability-diff-v0` boundary](capability-diff-v0.md#inputs) that
layer streams are diagnostic for capability projection, while making
the SDK-metadata sourcing for cross-runtime explicit and reproducible.

### Declared per-side projector configuration

Per A1, exactly one canonicalization rule applies to `filesystem_paths`:
the work-dir prefix is replaced with `<work>/`. The prefix MUST be
declared per side; the projector MUST NOT infer it from input path
values, mktemp templates, or runtime-name heuristics. See
[Per-Side Prefix Declaration](#per-side-prefix-declaration).

### Excluded inputs

Raw kernel telemetry, workflow logs, proof-pack metadata, and
`layers/policy.ndjson` are diagnostic context only. They are not v0
cross-runtime diff inputs.

The v0 cross-runtime diff is a pure projection over normalized evidence
plus one declared per-side configuration value. Workflow run URLs,
commit SHAs, generation timestamps, and cassette provenance are
intentionally not part of this schema. Consumers that need forensic
anchoring should pair the diff with proof-pack manifests from both
sides.

## Contract Principles

This contract inherits all six `capability-diff-v0`
[contract principles](capability-diff-v0.md#contract-principles), and
adds three cross-runtime-specific principles:

7. **Idempotent and meaning-preserving canonicalization.**
   Cross-runtime canonicalization MUST be idempotent and
   meaning-preserving: applying it more than once produces the same
   output as applying it once, and it may remove only explicitly
   declared fixture plumbing. It MUST NOT infer semantic equivalence,
   synthesize binding identity, rewrite tool names, or rewrite policy
   decision summaries.
8. **Within-runtime identity remains required; cross-runtime identity
   is intentionally not modeled.** Each side must independently meet
   the `capability-diff-v0` within-runtime binding-identity rule.
   Cross-runtime binding-id comparability is explicitly out of scope.
9. **No adapter knowledge in the projector.** The canonicalization rule
   set MUST be syntactic and runtime-agnostic. The projector MUST NOT
   carry per-runtime case logic.

## Schema

Schema string:

```text
assay.runner.cross_runtime_diff.v0
```

Fields:

| Field | Type | Required | Semantics |
|---|---|---:|---|
| `schema` | string | yes | Must equal `assay.runner.cross_runtime_diff.v0` |
| `base_run_id` | string | yes | `run_id` from the base evidence set |
| `head_run_id` | string | yes | `run_id` from the head evidence set |
| `base_runtime` | string | yes | Runtime identifier for the base side; v0 accepts only the values in the [runtime identifier table](#runtime-identifiers) |
| `head_runtime` | string | yes | Runtime identifier for the head side; v0 accepts only the values in the [runtime identifier table](#runtime-identifiers) and MUST differ from `base_runtime` |
| `status` | enum | yes | `clean`, `partial:health`, `partial:correlation`, `partial:unbound`, or `failed` |
| `preconditions` | object | yes | Machine-readable checks that determine whether the diff can be clean |
| `scope` | object | yes | Declares what evidence domain this diff used; includes the `cross_runtime` flag |
| `canonicalization` | object | yes | Declares which canonicalization rule was applied per surface category |
| `surface` | object | yes | Added, removed, and unchanged capability-surface values by category, after canonicalization |
| `binding_ids` | object | yes | Out-of-scope marker; see [Binding Ids — Out Of Scope](#binding-ids-out-of-scope) |
| `policy_outcomes` | object | yes | Out-of-scope marker; see [Policy Outcomes — Out Of Scope](#policy-outcomes-out-of-scope) |
| `sdk_metadata` | object | yes | Side-band runtime provenance; see [SDK Metadata — Side-Band Provenance](#sdk-metadata-side-band-provenance) |
| `unbound` | object | yes | Evidence buckets that could not be safely compared in v0 |
| `non_claims` | array[string] | yes | Stable code-prefixed non-claim assertions; see [Non-Claims](#non-claims) |
| `ambiguities` | array[string] | yes | Stable code-prefixed ambiguity strings |
| `notes` | array[string] | yes | Stable code-prefixed human-readable notes |

All arrays MUST serialize in stable lexicographic order. All object keys
follow the order shown in this table for byte-determinism.

## Runtime Identifiers

`base_runtime` and `head_runtime` accept only the following values in
v0:

| Value | Maps to fixture |
|---|---|
| `s5_openai_agents` | S5 OpenAI Agents (`@openai/agents`) fixture |
| `gemini_google_genai` | Gemini Python `google-genai` direct fixture |

Adding a third runtime identifier requires a separate Phase 2C+
contract slice and re-opening
[`second-runtime-candidate-selection.md`](second-runtime-candidate-selection.md).
v0 implementations MUST reject any other value with `status=failed`.

`base_runtime == head_runtime` MUST yield `status=failed`. Same-runtime
diffs are covered by [`capability-diff-v0`](capability-diff-v0.md); the
cross-runtime contract does not duplicate intra-runtime semantics.

## Preconditions

`preconditions` records why a diff is or is not clean. v0 cross-runtime
preconditions extend the `capability-diff-v0` preconditions with one
cross-runtime-specific check:

| Field | Type | Required | Clean value |
|---|---|---:|---|
| `base_health_clean` | boolean | yes | `true` |
| `head_health_clean` | boolean | yes | `true` |
| `base_correlation_clean` | boolean | yes | `true` |
| `head_correlation_clean` | boolean | yes | `true` |
| `stable_tool_call_ids_required` | boolean | yes | `true` (within each side) |
| `stable_tool_call_ids_present` | boolean | yes | `true` (within each side) |
| `runtimes_distinct` | boolean | yes | `true` |

A health set is clean only when:

- `kernel_layer=complete`
- `ringbuf_drops=0`
- `policy_layer=present`
- `sdk_layer=self_reported`
- `cgroup_correlation=clean`

`stable_tool_call_ids_*` apply *within each side*, not across them.
Per B3, cross-runtime binding-id stability is not modeled.

## Scope

`scope` separates preconditions from projection scope.

| Field | Type | Required | v0 value |
|---|---|---:|---|
| `projection` | string | yes | `surface_set` |
| `uses_raw_telemetry` | boolean | yes | `false` |
| `uses_proof_pack` | boolean | yes | `false` |
| `per_binding_capability_values` | boolean | yes | `false` |
| `cross_runtime` | boolean | yes | `true` |

`cross_runtime=true` is load-bearing. A `false` value here means the
diff is intra-runtime and the consumer must use
`capability-diff-v0` instead.

## Canonicalization

The `canonicalization` object declares which canonicalization rule was
applied to each `capability-surface` category. v0 defines exactly one
rule:

| Rule | Applies to | Operational definition |
|---|---|---|
| `work_dir_prefix_only` | `filesystem_paths` | For each side independently, strip the side's declared work-dir prefix from any path that begins with that prefix and replace it with the canonical placeholder `<work>/`. The prefix is a per-side declared input; see [Per-Side Prefix Declaration](#per-side-prefix-declaration) |

For every category v0 declares a canonicalization key:

| Field | Type | Required | v0 value |
|---|---|---:|---|
| `filesystem_paths` | string | yes | `work_dir_prefix_only` |
| `network_endpoints` | string | yes | `none` |
| `process_execs` | string | yes | `none` |
| `mcp_tools` | string | yes | `none` |
| `policy_decisions` | string | yes | `none` |

Implementations MUST NOT introduce a new canonicalization rule string
without updating this contract. Implementations MUST NOT apply a rule
implicitly: the `canonicalization` block declares what was applied; the
projector emits exactly that.

### Per-Side Prefix Declaration

The work-dir prefix used to canonicalize `filesystem_paths` is **not**
a global, contract-frozen set of strings. The acceptance and
three-run-determinism wrappers create work directories with
`mktemp -d` (including randomized suffixes and platform-dependent
`TMPDIR` resolution), and may be further overridden by
`ASSAY_RUNNER_ACCEPTANCE_WORK_DIR`. A static prefix list cannot match
real run paths and would silently fail to canonicalize.

Instead, the projector accepts the prefix as a declared, required,
per-side configuration value:

| Configuration value | Required | Shape |
|---|---:|---|
| `base_work_dir_prefix` | yes | Absolute filesystem path, non-empty, ending with `/`. The exact prefix used by the base side's wrapper for `WORK_DIR` |
| `head_work_dir_prefix` | yes | Absolute filesystem path, non-empty, ending with `/`. The exact prefix used by the head side's wrapper for `WORK_DIR` |

Operational rules:

- The two prefixes MUST be declared by the side that produced the
  evidence (e.g. wrapper-emitted manifest, environment variable, or
  CLI flag). The projector MUST NOT infer them from input path values,
  mktemp templates, or runtime-name heuristics.
- The projector MUST treat the prefixes as opaque syntactic strings.
  It MUST NOT decode them for runtime intent, fixture name, or
  determinism-vs-acceptance variant.
- Absent, empty, or non-absolute prefix on either side → `status=failed`.
- A path that does not begin with its side's declared prefix is emitted
  unchanged. Non-matching paths are not an error; they simply mean the
  side's evidence contains paths outside the work directory (these
  remain rare under the existing telemetry-versus-evidence filter, but
  are not forbidden by v0).
- The wiring mechanism (manifest file, env var, CLI flag, or future
  declared-input artifact) is implementation detail and out of scope
  for this contract. v0 freezes the *shape and discipline* of the
  declaration, not the transport.
- Prefix values MUST NOT appear in the output. The `canonicalization`
  block records only the *rule* that was applied per category. This
  keeps the golden shape stable across runs whose mktemp suffixes
  differ. Forensic reproduction of the prefix value, if needed, lives
  in the proof-pack manifest, not the diff output.

## Surface Diff

`surface` contains one object per `capability-surface.v0` category:

- `filesystem_paths`
- `network_endpoints`
- `process_execs`
- `mcp_tools`
- `policy_decisions`

Each category object has the same fields as
[`capability-diff-v0`](capability-diff-v0.md#surface-diff):

| Field | Type | Required | Semantics |
|---|---|---:|---|
| `added` | array[string] | yes | Values present in head (after canonicalization) and absent from base |
| `removed` | array[string] | yes | Values present in base (after canonicalization) and absent from head |
| `unchanged` | array[string] | yes | Values present in both base and head (after canonicalization) |

All arrays serialize in stable lexicographic order. The presence of a
fixture-local filename in `unchanged` is a syntactic equality after the
declared canonicalization rule. It is **not** a semantic-equivalence
claim across runtimes; see [Non-Claims](#non-claims).

## Binding Ids — Out Of Scope

Per B3, binding-id comparability is not modeled in v0 cross-runtime
diffs. The `binding_ids` field retains its schema slot for envelope
parity with `capability-diff-v0`, but its only valid v0 shape is:

```json
"binding_ids": {
  "comparison": "out_of_scope_cross_runtime_v0"
}
```

v0 implementations MUST NOT emit `added`, `removed`, `changed`, or
`unchanged` keys inside `binding_ids` in a cross-runtime diff.
Within-runtime binding-id stability is still required as a precondition
on each side; it just does not produce a cross-runtime comparison.

## Policy Outcomes — Out Of Scope

`capability-diff-v0` reports `policy_outcomes.changed` per stable
binding id. Per B3, stable binding ids are not comparable across
runtimes, so v0 also marks `policy_outcomes` out of scope. The only
valid v0 shape is:

```json
"policy_outcomes": {
  "comparison": "out_of_scope_cross_runtime_v0"
}
```

Set-level policy comparison remains available through
`surface.policy_decisions`, which compares decision-summary strings
(e.g. `allow:read_file`) as a set. Per-binding attribution is the part
that v0 does not model cross-runtime.

## SDK Metadata — Side-Band Provenance

Per C1, SDK metadata is reported as runtime provenance, not as
capability-surface added/removed/unchanged.

```json
"sdk_metadata": {
  "comparison": "side_band_provenance",
  "base": {"sdk_name": "@openai/agents", "sdk_version": "0.11.4"},
  "head": {"sdk_name": "google-genai", "sdk_version": "2.6.0"}
}
```

`sdk_name` and `sdk_version` are sourced exclusively from each side's
`layers/sdk.ndjson` file, declared as a [required side-band provenance
input](#required-side-band-provenance-input) above. They are read from
SDK events whose schema is `assay.runner.sdk_event.v0`. When the layer
stream contains multiple events with consistent `sdk_name` and
`sdk_version` values (the v0 expected case for the accepted fixtures),
the projector emits those values; when the layer stream contains
inconsistent values within a single side, the projector MUST emit
`status=failed` rather than picking one silently.

These values are visible for diagnostics; they MUST NOT participate in
any `surface.added`/`removed`/`unchanged` projection.

Adding further `sdk_metadata` fields (e.g. SDK feature flags) requires
a separate contract update. v0 accepts exactly `sdk_name` and
`sdk_version` under `base` and `head`. Sourcing `sdk_metadata` from
anywhere other than `layers/sdk.ndjson` (for example from fixture
filenames, environment variables, or proof-pack metadata) is
explicitly forbidden in v0.

## Unbound Evidence

`unbound` uses the same category names as `surface`, each as a stable
`array[string]`. For a clean v0 cross-runtime diff, every `unbound`
category MUST be empty.

`partial:unbound` is reserved for a future per-binding capability
artifact, identical to `capability-diff-v0`. v0 implementations MUST
keep `unbound` arrays empty; inputs that suggest per-value unbinding
without an explicit versioned source must produce `status=failed`, not
an invented `partial:unbound` projection.

## Non-Claims

`non_claims` is a stable array of code-prefixed non-claim assertions.
Every v0 cross-runtime diff with `status=clean` MUST include all of
these entries:

| Code | Asserts |
|---|---|
| `cross_runtime_no_filename_semantic_equivalence` | An `unchanged` filesystem path after work-dir prefix canonicalization is a syntactic equality, not a semantic equivalence of capability across runtimes |
| `cross_runtime_no_derived_binding_identity` | The diff does not derive or report a cross-runtime binding identifier; binding ids remain out of scope |
| `cross_runtime_no_sdk_capability_equivalence` | SDK metadata is provenance, not a capability-equivalence claim |
| `cross_runtime_no_declared_capability_input` | The diff is over observed capability evidence only; a declared-capability comparison requires a separate contract |
| `cross_runtime_no_acceptability_judgment` | The diff does not decide whether any difference is acceptable for a project |

`partial:*` and `failed` diffs MUST still include the
`cross_runtime_no_*` non-claims that apply to the projection that was
produced.

Implementations MUST NOT introduce new non-claim codes without updating
this contract. Consumers MAY assert presence of these codes as a
contract conformance check.

## Status Semantics

| Status | Semantics |
|---|---|
| `clean` | All preconditions true (including `runtimes_distinct=true`), all required artifacts validate, within-runtime correlation clean on both sides, declared canonicalization applied per the rules above, `unbound` arrays empty, and `non_claims` complete |
| `partial:health` | At least one evidence set can be parsed but has incomplete health (e.g. ring-buffer drops, incomplete cgroup correlation) |
| `partial:correlation` | Health is sufficient to parse, but at least one within-runtime correlation report is partial, ambiguous, or lacks stable binding identity |
| `partial:unbound` | Reserved for a future per-binding capability artifact; v0 producers MUST NOT emit this status from run-global capability-surface values |
| `failed` | Required artifacts are missing (including `layers/sdk.ndjson` on either side), schema strings are unsupported, run ids are internally inconsistent, deterministic parsing fails, `base_runtime == head_runtime`, `base_runtime`/`head_runtime` is not in the [runtime identifier table](#runtime-identifiers), `base_work_dir_prefix` or `head_work_dir_prefix` is absent, empty, or not an absolute path ending with `/`, or `layers/sdk.ndjson` on a single side contains inconsistent `sdk_name`/`sdk_version` values |

`partial:*` diffs may be useful for triage, but they are not clean
evidence for any further claim. `ringbuf_drops > 0` always prevents
`status=clean`.

## Idempotence And Meaning-Preservation

Cross-runtime canonicalization MUST be idempotent and
meaning-preserving:

- **Idempotent.** Applying the declared canonicalization rules twice
  MUST produce the same output as applying them once. Formally, for the
  projector `P`: `P(P(x)) == P(x)` for any valid input `x`.
- **Meaning-preserving.** The rules MUST only remove explicitly
  declared fixture plumbing as defined by the per-side prefix
  declaration in
  [Per-Side Prefix Declaration](#per-side-prefix-declaration). They
  MUST NOT infer semantic equivalence, synthesize binding identity,
  rewrite tool names, rewrite policy decision summaries, or alter
  capability values beyond the A1 prefix rule.

Intra-runtime idempotence (`diff(X, X)` where both sides come from the
same runtime fixture) is **not** an acceptance check for this contract.
It is the acceptance check for
[`capability-diff-v0`](capability-diff-v0.md#idempotence) and remains
there. The cross-runtime contract is acceptance-checked by the
S5 ↔ Gemini golden shape below.

## Golden Shape

The frozen v0 cross-runtime golden shape for the S5 ↔ Gemini case is
[`golden/cross-runtime-diff-s5-gemini-v0.json`](golden/cross-runtime-diff-s5-gemini-v0.json).

This file is a normative shape example. Each field, key order, and
non-claim entry follows the contract above. Implementations of a future
cross-runtime projector MUST be able to produce this exact output (up
to deterministic serialization) when given the S5 and Gemini accepted
fixture evidence sets.

The golden does not pre-commit to any future projector implementation:
this contract slice intentionally lands without a validator, without a
CLI, and without a workflow change. The next slice may extend
`scripts/ci/assay_runner_capability_diff_validate.py` to project and
validate against this golden, or introduce a separate cross-runtime
projector. That decision is out of scope here.

## Notes Vocabulary

`notes` follows the code-prefixed convention from
`capability-diff-v0`. v0 reserves the `cross_runtime_diff_` prefix.
First note codes:

- `cross_runtime_diff_work_dir_prefix_canonicalized` — work-dir prefix
  canonicalization was applied to `filesystem_paths` per the A1 rule
- `cross_runtime_diff_binding_ids_out_of_scope` — emitted on every
  clean diff to reinforce B3 at consumption time
- `cross_runtime_diff_sdk_metadata_side_band` — emitted on every clean
  diff to reinforce C1 at consumption time

Implementations MUST NOT introduce new note codes without updating this
contract.

## Forward Compatibility

This contract freezes A1 + B3 + C1. A future revision may relax B3 only
if a cross-runtime binding-identity projection is contracted separately
under its own schema slice and lineage. A future revision may broaden
A1 only if a filename-layer canonicalization is contracted separately.
A future revision may extend C1 only if SDK metadata gains
capability-bearing fields and is contracted separately.

v0 consumers MUST treat any output emitted under a `cross_runtime_diff`
schema string other than `assay.runner.cross_runtime_diff.v0` as
unsupported. v0 producers MUST NOT downgrade a `v1+` evidence input
into a v0 output silently.

## Non-Goals

v0 does not include:

- declared-capability input; declared-capability is a separate Phase 2C+
  slice
- per-binding path/process/endpoint attribution across runtimes
- third-runtime support; third-runtime evaluation reopens
  [`second-runtime-candidate-selection.md`](second-runtime-candidate-selection.md)
- derived cross-runtime identity scheme
- filename-layer canonicalization
- promotion of SDK metadata into capability-surface comparison
- call-id-less order fallback
- macOS or Windows runner support
- raw telemetry diffing
- proof-pack ingestion as a required input
- OTel or GenAI semantic-convention mapping of cross-runtime output
- acceptability or policy judgment
- delegated `gates=all` requirement for cross-runtime diff; the diff is
  a projection over existing evidence, not a runtime gate

## Implementation Placement

The [boundary map](boundary-map.md) places capability-diff projection
semantics on the Trust Basis / Harness side while Runner delivers clean
measured-run input bundles. Cross-runtime diff inherits the same
placement. A future implementation may start as an Assay-side reference
projector, but it MUST NOT silently move artifact meaning into the
runner candidate.

## References

- [Runner capability-diff v0 contract](capability-diff-v0.md)
- [Runner cross-runtime diff Phase 2C mini-plan](cross-runtime-diff-plan.md)
- [Runner cross-runtime diff Phase 2C decisions (A1+B3+C1)](cross-runtime-diff-decisions.md)
- [Runner artifact v0 contracts](artifacts-v0.md)
- [Runner acceptance fixture v0 contract](fixtures-v0.md)
- [Runner second runtime candidate selection](second-runtime-candidate-selection.md)
- Decision gate (resolved): <https://github.com/Rul1an/assay/issues/1310>
