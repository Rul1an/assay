# Promptfoo Assertion GradingResult Field Presence

Discovery was run locally on 2026-04-24 with:

- `promptfoo@0.119.0`
- Node `v20.16.0`
- public CLI JSONL output path
- one deterministic `equals` assertion
- one matching output and one non-matching output

The latest Promptfoo CLI available during discovery required a newer local
Node runtime than this workspace provided, and `promptfoo@0.120.0` hit a local
Drizzle migration bootstrap failure. The probe therefore pins the newest
Promptfoo version that worked cleanly in the current environment.

## Captured layers

The probe keeps the layers separate:

- `promptfoo.inputs.json` records emitted model outputs and assertion config
- `valid.full-jsonl-row.json` records the full successful Promptfoo JSONL row
- `failure.full-jsonl-row.json` records the full failed Promptfoo JSONL row
- `valid.surfaced.assertion-result.json` extracts the single assertion
  component result from the successful row
- `failure.surfaced.assertion-result.json` extracts the single assertion
  component result from the failed row

## Observed full JSONL row

The full JSONL row is too broad for the v1 artifact. It naturally carries:

- provider metadata
- prompt text
- raw output
- test vars
- response body
- row-level `success`
- row-level `score`
- latency and cost
- test indexes
- token usage
- test case wrapper
- aggregate `gradingResult`

That wrapper is useful discovery evidence, but it is not the canonical seam.

## Observed assertion component

For this one-assertion run, the actual assertion-level result was found at:

```text
gradingResult.componentResults[0]
```

The extracted component carried:

- `pass`
- `score`
- `reason`
- `assertion.type`
- `assertion.value`

The reduced v1 artifact keeps only:

- `assertion_type`, reduced from `assertion.type`
- `result.pass`
- `result.score`
- short `result.reason` only when it does not smuggle compared payloads

The failed Promptfoo assertion reason included the compared output and expected
text. The canonical failure fixture therefore drops `reason` and keeps only the
bounded assertion outcome.

## Reduction boundary

The v1 artifact intentionally does not include:

- full JSONL rows
- aggregate `gradingResult`
- `componentResults` arrays
- raw prompt text
- raw output
- raw expected values
- assertion `value`
- provider metadata
- row-level success
- latency, cost, token usage, or stats
- test indexes or eval ids

That keeps the first P28 lane on one extracted surfaced assertion result rather
than Promptfoo eval-run or export-wrapper truth.
