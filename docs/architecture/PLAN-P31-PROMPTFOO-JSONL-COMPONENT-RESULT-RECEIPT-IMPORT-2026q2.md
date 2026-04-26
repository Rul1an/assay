# PLAN — P31 Promptfoo JSONL Component Result Receipt Import

- **Date:** 2026-04-26
- **Owner:** Evidence / External Interop
- **Status:** Planning lane
- **Scope (current repo state):** Plan the first Promptfoo compiler path from
  one JSONL assertion component result to one portable Assay evidence receipt.
  This follows the P28 Promptfoo sample now restored on `main`. It does not
  propose broad Promptfoo JSONL import, full eval-run import, red-team report
  import, Trust Card rendering, or Assay Harness regression gating.

## 1. Why this plan exists

P28 proved the smallest honest Promptfoo evidence sample:

- one deterministic `equals` assertion
- one surfaced JSONL row
- one extracted `gradingResult.componentResults[]` item
- one reduced placeholder Assay event

That was sample/reducer work.

P31 is the next compiler-path step.

Promptfoo makes AI behavior testable in CI. Assay should be able to preserve
selected Promptfoo outcomes as small, reviewable, provenance-bearing evidence
receipts.

The important shift is from:

- checked-in example fixtures
- ad hoc mapper output
- placeholder event shape

to:

- a named import surface
- a receipt contract
- bundleable output
- Trust Basis-readable provenance and integrity claims

P31 should still stay smaller than "Promptfoo support." The lane is not the
Promptfoo eval run. It is not the JSONL export schema. It is one assertion
component result reduced into one portable receipt.

## 2. What this plan is and is not

This plan is for:

- a Promptfoo CLI JSONL input file
- one extracted `gradingResult.componentResults[]` item at a time
- one deterministic assertion component result
- one Assay evidence receipt per component result
- source artifact digesting without importing raw prompt/output/expected values
- bundle and Trust Basis readiness after the importer exists

This plan is not for:

- full Promptfoo support
- full Promptfoo JSON, JSONL, YAML, or XML export modeling
- Promptfoo row import as canonical evidence
- Promptfoo eval-run truth
- Promptfoo red-team report import
- Promptfoo web viewer, sharing, or platform state
- raw prompt, output, expected, assertion value, or vars import
- provider response, model, token, cost, latency, or stats import
- Trust Card rendering
- Assay Harness baseline/candidate comparison

## 3. Hard positioning rule

P31 compiles selected Promptfoo assertion component outcomes into portable
Assay receipts. It does not verify that the Promptfoo assertion outcome is
semantically correct.

That means:

- Promptfoo remains the source of the observed assertion outcome
- Assay records the imported component result boundary
- Assay records reducer version and source artifact digest
- Assay excludes raw evaluated payloads from the receipt
- Assay can later verify receipt and bundle integrity
- Assay must not inherit Promptfoo eval-run truth as Assay truth

The product line should stay:

Assay core compiles selected Promptfoo outcomes into portable evidence
receipts. Assay Harness can later compare those receipts across runs and turn
regressions into PR feedback.

Harness does not parse Promptfoo JSONL in P31.

## 4. Recommended import surface

P31 v1 freezes exactly one public surfaced path:

- Promptfoo CLI JSONL output
- `gradingResult.componentResults[]`
- deterministic `equals` assertion component result

Other Promptfoo output paths remain out of scope for v1, including Node package
results, full JSON output, YAML/XML exports, and JSONL as a broad family. If a
future path proves useful, it should be planned as a separate lane rather than
quietly folded into this one.

Candidate CLI shape, to confirm during implementation:

```bash
assay evidence import promptfoo-jsonl \
  --input results.jsonl \
  --bundle-out promptfoo-evidence.tar.gz \
  --source-artifact-ref results.jsonl
```

The exact command group needs implementation discovery because Assay already
has both `assay import` and `assay evidence` command families. The semantic
owner should be evidence import, not harness execution.

The importer should:

- stream JSONL rows instead of reading the full file into memory
- inspect each row for `gradingResult.componentResults[]`
- reduce each component result independently
- emit one receipt event per accepted component result
- write accepted receipt events through the existing evidence bundle path
- fail closed on malformed rows or unsupported component shapes
- allow deterministic `imported_at` injection for tests and fixtures

It should not:

- treat JSONL row `success` as assertion result truth
- treat JSONL row `score` as assertion result truth
- emit one receipt containing multiple component results
- import raw Promptfoo input/output payloads
- import provider, stats, cost, or token fields
- resolve Promptfoo config files or run IDs to make the receipt look richer

## 5. Receipt v1 thesis

The v1 receipt should be a portable, small, provenance-bearing representation
of one Promptfoo assertion component result.

It should be frozen from the P28 live discovery shape plus a P31 importer
fixture, not from Promptfoo docs alone.

The default P31 decision is that this receipt is emitted as an Assay
`EvidenceEvent` under the existing CloudEvents-compatible envelope. The receipt
body lives under `data`. A standalone non-CloudEvents receipt artifact would
require new bundling and verification tooling, so it is not the preferred first
path.

Illustrative `EvidenceEvent` shape:

```json
{
  "specversion": "1.0",
  "type": "assay.receipt.promptfoo.assertion_component.v1",
  "source": "urn:assay:external:promptfoo:assertion-component",
  "id": "import-promptfoo-jsonl:0",
  "time": "2026-04-26T12:00:00Z",
  "datacontenttype": "application/json",
  "assayrunid": "import-promptfoo-jsonl",
  "assayseq": 0,
  "assayproducer": "assay-cli",
  "assayproducerversion": "3.5.1",
  "assaygit": "unknown",
  "assaypii": false,
  "assaysecrets": false,
  "assaycontenthash": "sha256:...",
  "data": {
    "schema": "assay.receipt.promptfoo.assertion-component.v1",
    "source_system": "promptfoo",
    "source_surface": "cli-jsonl.gradingResult.componentResults",
    "source_artifact_ref": "results.jsonl",
    "source_artifact_digest": "sha256:...",
    "reducer_version": "assay-promptfoo-jsonl-component-result@0.1.0",
    "imported_at": "2026-04-26T12:00:00Z",
    "assertion_type": "equals",
    "result": {
      "pass": true,
      "score": 1,
      "reason": "Assertion passed"
    }
  }
}
```

This shape is intentionally not a Promptfoo JSONL row.

It is also not yet a Trust Card. It is the receipt that later compiler steps
can bundle, verify, summarize, and display.

Implementation must either register this as a new evidence event type with the
normal Evidence Contract v1 bar, or keep it explicitly experimental until the
registry row, payload contract, and conformance test exist.

## 6. Field boundaries

Unless otherwise noted, the fields below describe the receipt body inside the
`data` payload of the Assay `EvidenceEvent`.

### 6.1 `schema`

`schema` identifies the Assay receipt contract.

Expected v1 value:

- `assay.receipt.promptfoo.assertion-component.v1`

It must not be reused for:

- full JSONL rows
- Promptfoo eval-run summaries
- red-team findings
- model-graded rubric results
- provider output payloads

### 6.2 `source_system`

Expected v1 value:

- `promptfoo`

This names the external system that produced the observed outcome. It does not
make Promptfoo truth Assay truth.

### 6.3 `source_surface`

Expected v1 value:

- `cli-jsonl.gradingResult.componentResults`

This is the narrow public surfaced path used by P28 discovery.

It must not widen to:

- `jsonl`
- `eval-run`
- `gradingResult`
- `promptfoo-output`

Those names are too broad for the first receipt contract.

### 6.4 `source_artifact_ref`

`source_artifact_ref` is a reviewer aid for the source artifact that was
imported.

It should be:

- user-provided when possible
- short
- non-secret
- stable enough for review

It must not default to leaking absolute local paths, usernames, CI workspace
paths, or private bucket URLs.

For local CLI use, a basename such as `results.jsonl` is enough unless the user
passes a more deliberate logical reference.

### 6.5 `source_artifact_digest`

`source_artifact_digest` binds receipts to the source artifact bytes without
embedding the source artifact.

Expected v1 format:

- `sha256:<hex>`

The digest is for provenance and integrity. It does not authorize importing raw
Promptfoo rows into the receipt.

### 6.6 `reducer_version`

`reducer_version` identifies the Assay Promptfoo receipt reducer.

It should be explicit and visible because changes in reduction policy affect
review.

Expected v1 pattern:

- `assay-promptfoo-jsonl-component-result@0.1.0`

The version is reducer truth, not Promptfoo package truth.

Promptfoo package version can remain discovery metadata or future optional
provenance only if it is useful and small.

### 6.7 `imported_at`

`imported_at` is Assay import provenance.

It is not:

- Promptfoo run timestamp
- assertion execution timestamp
- provider response timestamp
- CI job timestamp unless explicitly supplied by the importer environment

The implementation should support a test-only or explicit import-time override
so fixture output stays deterministic.

### 6.8 `assertion_type`

For the first lane, expected value:

- `equals`

This should come from the component result or adjacent assertion descriptor
when naturally available on the chosen surfaced path.

If Promptfoo JSONL exposes the component outcome without repeating the
assertion type, the reducer may carry the explicitly invoked deterministic
assertion type from the row context. In that case, `assertion_type` is
derived/reducer-carried invocation context, not surfaced-result truth. That
must be documented in the importer and fixtures.

Do not import full assertion config.

### 6.9 `result.pass`

`result.pass` is the assertion component outcome.

It must be:

- required
- boolean
- sourced from the component result

It must not be confused with:

- JSONL row `success`
- Promptfoo run success
- CI job success
- threshold success
- weighted aggregate success

### 6.10 `result.score`

`result.score` is the component score.

For the first deterministic `equals` lane, v1 accepts only binary component
scores:

- `0`
- `1`

If implementation discovery proves a different naturally surfaced score shape,
the lane must be recut before fixture freeze rather than widened silently.

It should be:

- required
- numeric
- exactly `0` or `1` for v1

It must not be:

- row aggregate score
- named score bundle
- weighted run score
- threshold decision

### 6.11 `result.reason`

`result.reason` is optional reviewer support.

It should be included only when it is:

- naturally present
- short
- non-empty after trimming
- bounded
- not a raw prompt/output/expected leak

It may be omitted even when present on the source component if it is too long,
too rich, multiline, rubric-like, provider-generated, or includes compared
values that would leak raw evaluated payloads.

P31 should prefer omission over unsafe convenience.

## 7. Explicitly excluded fields

The v1 receipt must not contain:

- raw `prompt`
- raw `output`
- raw `expected`
- raw `vars`
- raw assertion `value`
- raw Promptfoo config
- full JSONL row
- row `success`
- row aggregate `score`
- row aggregate `gradingResult`
- `componentResults` arrays
- `namedScores`
- `tokensUsed`
- `cost`
- `latencyMs`
- provider response bodies
- model names
- prompt IDs synthesized from line positions
- hashes of raw output or expected values as target identifiers

Hashing raw evaluated payloads into synthetic identifiers still imports target
semantics. V1 should not do that.

## 8. Cardinality rule

The receipt unit is:

one `gradingResult.componentResults[]` item -> one Assay receipt.

If a JSONL row contains multiple component results, the importer may emit
multiple receipts. Each emitted receipt must remain single-component.

Receipt uniqueness comes from Assay import-run provenance and envelope
sequence:

- `assayrunid`
- `assayseq`
- CloudEvents `id`

It must not come from synthetic target identity. JSONL row index and component
index may be used as importer-local addressing for diagnostics, but not as
domain identity and not as a substitute for prompt/output/expected identity.

The importer must not emit:

- one receipt for an entire JSONL row
- one receipt for an entire Promptfoo eval run
- one receipt containing a `componentResults` array
- one receipt combining assertion and row aggregate truth

## 9. Malformed and fail-closed rules

P31 should fail closed when a component result cannot be reduced without
crossing the boundary.

Initial malformed cases:

- no JSONL rows
- invalid JSONL line
- missing `gradingResult`
- missing `gradingResult.componentResults`
- `componentResults` is not an array
- component result missing `pass`
- component result missing `score`
- component `pass` is not boolean
- component `score` is not exactly `0` or `1`
- unsupported assertion type for v1
- attempted receipt contains raw prompt/output/expected/vars/assertion value
- attempted receipt contains full row or aggregate row fields
- attempted receipt contains red-team, model-graded, rubric, provider, token,
  cost, latency, or stats fields

P31 v1 operates in strict mode only. Rows without
`gradingResult.componentResults[]` are errors, not silently skipped.

Permissive mixed-row ingestion can be future work, but it should not be part of
the first compiler-path implementation.

## 10. Trust Basis posture

P31 should prepare the Trust Basis layer, not implement the whole presentation
line too early.

Future Trust Basis claims can speak to:

- external evaluation receipt is present
- Promptfoo component-result surface is visible
- source surface is visible
- source artifact digest is present
- reducer version is visible
- raw Promptfoo payloads are excluded
- receipt bundle integrity is verifiable

Future Trust Basis claims must not say:

- the model output is correct
- the expected value is true
- the Promptfoo assertion is semantically valid
- the Promptfoo run passed as a whole
- the system is safe or compliant

Trust Basis is allowed to verify integrity, provenance visibility, and boundary
discipline. It should not upgrade external outcomes into universal truth.

## 11. Bundle path

The P31 implementation should make receipts easy to bundle after import, but
the current CLI does not accept arbitrary receipt NDJSON via
`assay bundle create --evidence`.

Current bundle commands are profile/bundle-oriented, for example:

```bash
assay evidence export --profile profile.yaml --out evidence.tar.gz
assay evidence verify evidence.tar.gz
```

P31 chooses the first real bundle integration path now: write Promptfoo receipt
events through the existing `BundleWriter`.

The implementation may add a debug or fixture-only event stream output if that
helps tests, but the first production-facing compiler path should produce a
verifiable evidence bundle directly. If `BundleWriter` integration is blocked,
the lane should be recut before the implementation PR instead of producing a
dead-end receipt NDJSON artifact.

Implementation target, shown as pseudocode until the CLI exists:

```bash
promptfoo eval --output results.jsonl
assay evidence import promptfoo-jsonl \
  --input results.jsonl \
  --bundle-out promptfoo-evidence.tar.gz \
  --source-artifact-ref results.jsonl
assay evidence verify promptfoo-evidence.tar.gz
```

The architecture requirement is that receipt output must not be a dead-end
example file. It must become an Assay evidence bundle that the existing
verification and later Trust Basis path can consume.

## 12. Assay Harness boundary

Assay Harness comes after P31.

Harness should eventually compare receipt sets across baseline and candidate
runs, then surface regressions as PR feedback.

Harness should not become the Promptfoo parser.

Correct split:

- Promptfoo produces assertion outcomes
- Assay core imports selected outcomes into receipts
- Assay core bundles and verifies receipt integrity
- Trust Basis summarizes boundary and provenance claims
- Harness compares receipts across runs and reports regressions

This keeps the compiler path independent from CI gating policy.

## 13. Outward strategy

Do not open another Promptfoo upstream thread for P31.

Promptfoo upstream already received:

- a JSONL shape clarification contribution
- a JSONL assertion component parsing example contribution

P31 should now prove value inside Assay first.

After importer, bundle, and Trust Basis-readiness exist on `main`, the right
public note is an Assay-side thesis:

```text
From Promptfoo JSONL to Evidence Receipts
```

Core message:

```text
Promptfoo already makes AI behavior testable in CI. The next step is evidence
portability for the outcomes that matter.
```

Promptfoo should be framed as the CI/eval runner example, not as an endorsement
or integration claim.

## 14. Acceptance criteria for P31 implementation

P31 implementation can be considered ready when:

- there is a first-class import command or clearly named importer entry point
- importer input is Promptfoo CLI JSONL
- importer streams rows
- importer extracts assertion component results only
- importer emits one receipt per component result
- importer operates in strict mode for v1
- component scores are binary only for v1
- receipt output excludes raw prompt/output/expected/vars/assertion value
- source artifact digest is present
- source surface is visible
- reducer version is present
- deterministic fixture import is possible
- malformed fixtures fail closed
- receipt events are emitted as Assay `EvidenceEvent`s and written through
  `BundleWriter` into a verifiable evidence bundle
- docs explain that Harness comparison and Trust Card rendering are follow-ups

## 15. Suggested implementation slices

Recommended order:

1. Add receipt contract and reducer tests around the P28 fixture shape.
2. Add streaming JSONL importer with deterministic fixture output.
3. Add CLI entry point and README usage.
4. Write receipt events through `BundleWriter` into a verifiable evidence
   bundle.
5. Add Trust Basis claims for receipt presence, digest presence, reducer
   visibility, source-surface visibility, and raw payload exclusion.
6. Only after that, add Trust Card summarization.
7. Only after that, add Assay Harness baseline/candidate comparison.

Keep each slice reviewable. The first implementation PR should not try to
solve Trust Card and Harness in the same move.

## 16. Decision

Proceed with P31 as a Promptfoo JSONL component-result receipt import lane.

The lane is intentionally narrower than Promptfoo import:

- component result in
- portable receipt out
- bundleable and Trust Basis-readable next
- Harness regression gates later

This is the smallest credible compiler-path step after P28.
