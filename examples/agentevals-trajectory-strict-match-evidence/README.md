# AgentEvals Trajectory Strict-Match Evidence Sample

This example turns one tiny frozen artifact derived from AgentEvals'
deterministic trajectory strict-match evaluator into bounded, reviewable
external evidence for Assay.

It is intentionally small:

- start with one reduced artifact derived from one returned strict-match result
- keep the sample to one valid artifact, one failure artifact, and one
  malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep raw trajectories, evaluator config, LangSmith wrappers, and runtime
  truth out of Assay truth

## What is in here

- `capture_probe.py`: runs one positive and one negative strict-match
  evaluation and saves raw discovery artifacts
- `requirements.txt`: local probe dependency for the checked-in evaluator path
- `discovery/valid.evaluator.inputs.json`: the exact caller-side inputs used
  for the valid strict-match capture
- `discovery/valid.returned.result.json`: the raw returned result object for
  the valid strict-match capture
- `discovery/failure.evaluator.inputs.json`: the exact caller-side inputs used
  for the negative strict-match capture
- `discovery/failure.returned.result.json`: the raw returned result object for
  the negative strict-match capture
- `discovery/FIELD_PRESENCE.md`: input-vs-returned notes and reduction
  rationale
- `map_to_assay.py`: turns one reduced AgentEvals strict-match artifact into an
  Assay-shaped placeholder envelope
- `fixtures/valid.agentevals.json`: one bounded positive artifact derived from
  the returned strict-match result
- `fixtures/failure.agentevals.json`: one bounded negative artifact derived
  from the returned strict-match result
- `fixtures/malformed.agentevals.json`: one malformed wrapper/import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import
  time

## Why this seam

AgentEvals sits next to much broader evaluation surfaces than this sample uses:

- full trajectory payloads
- richer evaluator families
- LangSmith evaluation-run wrappers
- LLM-as-judge paths
- graph-trajectory evaluators

This sample intentionally does **not** start there.

It starts on the smaller deterministic returned-result seam:

- one `create_trajectory_match_evaluator(trajectory_match_mode="strict")` path
- one returned result object
- one bounded result bag

That keeps the first wedge smaller than:

- raw `outputs` or `reference_outputs` truth
- LangSmith runs or experiments
- evaluator prompt or model truth
- broader LangChain runtime truth

## Live discovery note

This sample is grounded in a small local probe run on **2026-04-24** against
`agentevals==0.0.9`.

The important boundary is now clear:

- evaluator inputs are not the same thing as the returned public result
- the returned public result is a very small dict
- the raw returned dict contains `key`, `score`, `comment`, and `metadata`
- only `key` and boolean `score` are needed for the v1 canonical artifact
- `comment` stays optional and is omitted here because the strict-match probe
  returned `null`
- `metadata` stays out of the canonical artifact because it was `null` and is
  not part of the small reviewable seam

That is why the reduced artifact is derived from the returned result object
rather than from:

- the caller-side trajectory payloads
- evaluator factory/config state
- LangSmith or experiment wrappers
- README examples alone

For the reduced artifact:

- `evaluator_key` is reduced from returned `key`
- `result.score` is copied from returned boolean `score`
- `target_kind` is fixed to `trajectory` because that is the level being
  evaluated, not because a stable trajectory id was returned
- raw `outputs`, raw `reference_outputs`, raw `comment`, raw `metadata`, and
  any evaluator config fields stay out of the canonical artifact

The repo corpus uses `failure` naming to match the established examples
convention. In this lane, that file still represents a valid bounded negative
evaluation artifact, not an infrastructure failure.

## Re-run the local discovery probe

```bash
python3.12 -m venv /tmp/p26-agentevals-venv
/tmp/p26-agentevals-venv/bin/python -m pip install --upgrade pip
/tmp/p26-agentevals-venv/bin/python -m pip install \
  -r examples/agentevals-trajectory-strict-match-evidence/requirements.txt
/tmp/p26-agentevals-venv/bin/python \
  examples/agentevals-trajectory-strict-match-evidence/capture_probe.py
```

The probe prints the observed package version and writes raw input plus returned
artifacts into `discovery/`.

## Map the checked-in valid artifact

```bash
python3 examples/agentevals-trajectory-strict-match-evidence/map_to_assay.py \
  examples/agentevals-trajectory-strict-match-evidence/fixtures/valid.agentevals.json \
  --output examples/agentevals-trajectory-strict-match-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-24T07:40:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 examples/agentevals-trajectory-strict-match-evidence/map_to_assay.py \
  examples/agentevals-trajectory-strict-match-evidence/fixtures/failure.agentevals.json \
  --output examples/agentevals-trajectory-strict-match-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-24T07:41:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/agentevals-trajectory-strict-match-evidence/map_to_assay.py \
  examples/agentevals-trajectory-strict-match-evidence/fixtures/malformed.agentevals.json \
  --output /tmp/agentevals-malformed.assay.ndjson \
  --import-time 2026-04-24T07:42:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture tries to
smuggle raw trajectory payloads and evaluator config into a single-result v1
lane.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat AgentEvals, LangSmith, or LangChain evaluation semantics as Assay truth
- import raw trajectories or reference trajectories
- import evaluator config, prompts, or model metadata
- partially import larger evaluation wrappers
- claim that the reduced artifact is a stable upstream wire-format contract

This sample targets the smallest honest AgentEvals strict-match seam, not a
broader LangChain evaluation lane.
