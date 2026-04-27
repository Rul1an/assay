# PLAN — P34 Trust Basis Diff Gate (Q2 2026)

Status: implemented in this slice
Owner: Assay core / CLI
Scope: compare two canonical Trust Basis artifacts, not raw external evidence

---

## 1. Why This Exists

P31 made the Promptfoo compiler path real:

```text
Promptfoo assertion component result -> Assay evidence receipt bundle
```

P33 made that receipt boundary visible to the Trust Basis compiler:

```text
receipt bundle -> trust-basis.json with external_eval_receipt_boundary_visible
```

P34 adds the next small bridge:

```text
baseline trust-basis.json + candidate trust-basis.json -> claim-level diff
```

That gives Harness a stable gate foundation without asking Harness to parse
Promptfoo JSONL, understand external eval receipt payloads, or re-run Trust
Basis classification logic.

---

## 2. Boundary

P34 compares compiled Assay artifacts only.

It does not:

- parse Promptfoo JSONL
- inspect raw prompts, outputs, expected values, vars, provider payloads, stats, or full rows
- compare evidence bundles directly
- infer model correctness or Promptfoo run success
- add Trust Card rendering changes
- add Harness baseline/candidate UI

The command is deliberately generic:

```bash
assay trust-basis diff baseline.trust-basis.json candidate.trust-basis.json
```

Promptfoo is only the first motivating receipt lane. The diff layer is about
Trust Basis claims, not Promptfoo semantics.

---

## 3. Comparison Semantics

Trust Basis claim levels are ordered:

```text
absent < inferred < self_reported < verified
```

A candidate is a regression when:

- a baseline claim is missing from the candidate
- a candidate claim level is lower than the baseline claim level

A candidate is an improvement when:

- a candidate claim level is higher than the baseline claim level

The diff also reports:

- added claims
- removed claims
- source/boundary metadata changes
- unchanged claim count

Metadata changes are visible but do not fail by default. They may represent a
spec or compiler evolution rather than a runtime regression.

---

## 4. Gate Posture

The default command reports differences and exits successfully.

Use this mode for local inspection:

```bash
assay trust-basis diff baseline.trust-basis.json candidate.trust-basis.json
```

Use `--fail-on-regression` when the diff should become a gate:

```bash
assay trust-basis diff \
  baseline.trust-basis.json \
  candidate.trust-basis.json \
  --fail-on-regression
```

This keeps the compiler path and the gate policy separate:

- Assay core compiles Trust Basis artifacts.
- `assay trust-basis diff` compares those artifacts.
- Harness can later decide how to surface regressions in PR feedback.

---

## 5. Acceptance Criteria

P34 is complete when:

- `assay trust-basis diff` accepts two canonical Trust Basis JSON files.
- text and JSON output are available.
- `--fail-on-regression` exits non-zero only for missing/lowered baseline claims.
- Promptfoo-origin Trust Basis claim improvements and regressions are covered by CLI tests.
- docs explain that this command compares Trust Basis artifacts, not external eval payloads.

---

## 6. Follow-Ups

Future slices may add:

- Harness baseline/candidate wiring over `trust-basis diff` JSON output
- SARIF/JUnit projection for Trust Basis regressions
- stricter metadata-change policy for release gates
- multi-artifact comparison summaries

Those should stay above this generic diff layer.
