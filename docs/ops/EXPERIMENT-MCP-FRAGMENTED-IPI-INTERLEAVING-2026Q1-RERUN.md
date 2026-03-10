# Rerun — MCP Fragmented IPI Interleaving Variant (2026Q1)

## Preconditions
- Repo checkout on the interleaving Step2 commit.
- Offline-capable Cargo cache available.
- Existing fragmented-IPI harness binaries build locally.

## Build
```bash
CARGO_NET_OFFLINE=true cargo build -q -p assay-cli -p assay-mcp-server
```

## Required environment
```bash
export RUN_LIVE=0
export EXPERIMENT_VARIANT=interleaving
export SEQUENCE_POLICY_FILE=second_sink_sequence.yaml
export RUNS_ATTACK=2
export RUNS_LEGIT=100
```

## Bounded matrix
Cases:
- `interleave_primary_malicious_first`
  - `SECOND_SINK_PATH=primary_only`
  - `INTERLEAVING_ORDERING_CLASS=malicious_first`
- `interleave_primary_legit_first`
  - `SECOND_SINK_PATH=primary_only`
  - `INTERLEAVING_ORDERING_CLASS=legit_first`
- `interleave_mixed_near_parallel`
  - `SECOND_SINK_PATH=mixed`
  - `INTERLEAVING_ORDERING_CLASS=near_parallel`

Modes per case:
- `wrap_only`
- `sequence_only`
- `combined`

## Canonical test command
```bash
RUN_LIVE=0 bash scripts/ci/test-exp-mcp-fragmented-ipi-interleaving.sh
```

## Scoring command
Per mode directory:
```bash
python3 scripts/ci/exp-mcp-fragmented-ipi/score_interleaving.py \
  <mode-dir>/baseline_attack.jsonl \
  <mode-dir>/baseline_legit.jsonl \
  <mode-dir>/protected_attack.jsonl \
  <mode-dir>/protected_legit.jsonl \
  --out <mode-dir>/<mode>-interleaving-summary.json
```

## Expected aggregate artifact
- `target/exp-mcp-fragmented-ipi-interleaving/test/interleaving-summary.json`

## Interpretation freeze
Primary metric remains attempt-based:
- `success_any_sink_canary`

Per-run markers required in records:
- `interleaving.case_id`
- `interleaving.ordering_class`
- `interleaving.segment_labels`
- `interleaving.turns[].turn_index`
- `interleaving.sink_attempted`
- `interleaving.blocked_before_attempt`
