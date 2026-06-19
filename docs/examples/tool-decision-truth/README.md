# Tool-decision truth-layer: end-to-end example

A runnable walkthrough of the experimental tool-decision truth-layer public surface. A single supplied
**carrier** goes through **import, verify, project**, so you can see what each step produces and what it
proves. See [the reference](../../reference/tool-decision-truth.md) for the full contract, the claim
ceiling, and the non-claims. This is EXPERIMENTAL (unstable); names and digests may change.

The pieces, in order: the carrier and its recipe row are the content-addressed **evidence** (the record),
and the OTel projection is a lossy **view** over already-verified evidence, never the authority.

## The carrier

[`carrier.json`](carrier.json) is one real `assay.tool_decision_truth.v0` carrier (taken from the
committed conformance vectors, keyed with the neutral `fixture-kid-v0`). It records one observed tool
decision, `deploy` with a `match` verdict, as digests only: the keyed `args_digest`, the
`observed_input_digest`, and the `declared_policy_digest`. No raw arguments are present, by design.

## 1. Import: bind the carrier into an evidence bundle

```bash
assay evidence import tool-decision-truth \
  --carrier carrier.json \
  --bundle-out tdt.tar.gz \
  --run-id example-tdt
```

```
Imported tool-decision-truth carrier + recipe row to tdt.tar.gz
```

The importer is the gate where an external carrier enters a pack. It validates the carrier fail-closed
(schema, no raw arguments, well-formed `sha256:`/`hmac-sha256:` digests, `key_id` consistent with the
`args_digest` framing, `decision_identity` equal to its two digests, and `observed_input_digest`
**recomputed** from `{tool_name, args_digest, order}`), then binds it into a recipe row cited by
`carrier_content_digest`, and writes the carrier event plus the recipe-row event into one bundle. An
invalid carrier never reaches the bundle.

## 2. Verify: semantic, fail-closed

```bash
assay evidence verify-tool-decision-truth tdt.tar.gz
```

```
Tool-Decision-Truth Verification
================================
OK:             yes
Carriers:       1
Rows:           1
Verified rows:  1

row_example-tdt:1_verifies                   ok   row coheres with the carrier it cites

Claims not made: policy_correctness, intent_or_maliciousness, runtime_enforcement, tool_result_truth
```

`BundleReader` checks bundle integrity (manifest hashes + Merkle root) first; this command layers the
tool-decision-truth semantics on top. It pairs every recipe row with the carrier it cites **by content
digest**, then runs the fail-closed check. A tampered carrier or row, a stale or understated verdict, a
duplicate carrier content digest, two rows citing one digest, or a payload that does not self-declare the
carrier schema all make it exit non-zero.

## 3. Project: a lossy OTel view over verified evidence

```bash
assay project-otel --evidence-bundle tdt.tar.gz
```

```json
{
  "schema": "assay.tool_decision_truth.otel_projection.v0",
  "spans": [
    {
      "name": "execute_tool deploy",
      "kind": "INTERNAL",
      "attributes": {
        "gen_ai.operation.name": "execute_tool",
        "gen_ai.tool.name": "deploy",
        "openinference.span.kind": "TOOL",
        "assay.claim_class": "derived",
        "assay.tdt.decision_verdict": "match",
        "assay.tdt.observed_input_digest": "sha256:abb703de…",
        "assay.tdt.declared_policy_digest": "sha256:4c3c6a7a…",
        "assay.tdt.decision_identity_digest": "sha256:f173bd2f…",
        "assay.tdt.carrier_content_digest": "sha256:345448027…",
        "assay.tdt.source_class": "authoritative_boundary"
      }
    }
  ],
  "lossy": true,
  "source_of_truth": "assay artifacts",
  "non_claims": [ "…" ]
}
```

The projection runs **only over verified pairs**: the bundle is verified in full first, and if any row
fails, nothing is written, not even to `--out`. Each decision becomes one `TOOL` span; the verdict and
the digests ride in `assay.tdt.*`, and `assay.claim_class="derived"` marks the span as a derived
comparison over observed and declared data, not a raw observation (unlike the capability-surface tool
spans) and not enforcement. No raw arguments and no `args_digest` are projected. `lossy:true` and
`source_of_truth:"assay artifacts"` are the contract that this view is not the record.

## What this example does NOT show

It uses a *supplied* carrier rather than minting one in a live run. Carriers can also be minted live by
the opt-in, evidence-only producer (`assay mcp wrap --tool-decision-truth-out`); see the **Live
producer** section of the [reference](../../reference/tool-decision-truth.md). This example takes the
carrier as given, and it does not act on the verdict (a consumer gate is a separate step). The verdict
is a contract statement: `match` means "inside the declared set", not "safe", "intended", or
"enforced". See the [reference](../../reference/tool-decision-truth.md) for the full boundaries.
