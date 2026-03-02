# Rerun Instructions — MCP Fragmented IPI Ablation (2026Q1)

## Preconditions
- Repository checked out at commit: `c6358730456a`
- Ablation harness present from PR #499
- Current harness mode is **local mock only**

## One-command rerun
From repo root:

```bash
bash scripts/ci/test-exp-mcp-fragmented-ipi-ablation.sh
```

## Extended run matching the published result
From repo root:

```bash
RUN_ID="$(date -u +%Y%m%d-%H%M%S)-$(git rev-parse --short=12 HEAD)"
ART_ROOT="target/exp-mcp-fragmented-ipi-ablation/runs/$RUN_ID"
FIX_DIR="scripts/ci/fixtures/exp-mcp-fragmented-ipi"

for SET in deterministic variance; do
  SET_ROOT="$ART_ROOT/$SET"
  mkdir -p "$SET_ROOT"
  for MODE in wrap_only sequence_only combined; do
    RUNS_ATTACK=10 RUNS_LEGIT=10 RUN_SET="$SET" \
      bash scripts/ci/exp-mcp-fragmented-ipi/ablation/run_variant.sh "$SET_ROOT" "$FIX_DIR" "$MODE"
  done
  python3 scripts/ci/exp-mcp-fragmented-ipi/ablation/score_ablation.py \
    --root "$SET_ROOT" \
    --out "$SET_ROOT/ablation-summary.json"
done
```

## Artifact output
Artifacts are written under:
- `target/exp-mcp-fragmented-ipi-ablation/runs/<timestamp>-<gitsha>/`

Published run root:
- `/tmp/assay-exp-mcp-fragmented-ipi-ablation-promote/target/exp-mcp-fragmented-ipi-ablation/runs/20260302-231411-c6358730456a`

## Interpretation notes
- `wrap_only` should report `protected_sequence_sidecar_enabled = false`
- `sequence_only` and `combined` should report `protected_sequence_sidecar_enabled = true`
- Current harness is mock-only; these results should not be presented as live-host measurements

## Troubleshooting
- If hooks fail on `libsqlite3-sys` artifacts during push: run `cargo clean -p libsqlite3-sys`
- If summaries are missing, inspect per-mode directories under the chosen run root
