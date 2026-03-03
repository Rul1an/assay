# Rerun — MCP Fragmented IPI Second Sink Generality (2026Q1)

## Preconditions
- Repo checkout includes second-sink Step2 on `main`
- Compat host available at:
  - `/Users/roelschuurkes/assay/scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py`
- Fixtures present under:
  - `/Users/roelschuurkes/assay/scripts/ci/fixtures/exp-mcp-fragmented-ipi/`

## Reference batch driver
A local batch driver equivalent to the paper-grade run was used:
- `/tmp/run_second_sink_live_batch.sh`

## Live batch command
```bash
RUN_LIVE=1 \
COMPAT_ROOT="/Users/roelschuurkes/assay/scripts/ci/fixtures/exp-mcp-fragmented-ipi" \
MCP_HOST_CMD="python3 /Users/roelschuurkes/assay/scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py" \
ASSAY_CMD="/Users/roelschuurkes/assay/target/debug/assay" \
bash /tmp/run_second_sink_live_batch.sh /Users/roelschuurkes/assay
```

## Paper-grade reference run
- commit: `7c04a70115c0`
- artifact root:
  - `/tmp/assay-exp-second-sink-live/target/exp-mcp-fragmented-ipi-second-sink/runs/live-main-20260303-180209-7c04a70115c0`
- build provenance:
  - `/tmp/assay-exp-second-sink-live/target/exp-mcp-fragmented-ipi-second-sink/runs/live-main-20260303-180209-7c04a70115c0/build-info.json`

## Audit checklist
For each path and mode, confirm:
- `protected.log` includes:
  - `ABLATION_MODE=...`
  - `SIDECAR=enabled|disabled`
  - `ASSAY_POLICY=...`
  - `SEQUENCE_POLICY_FILE=second_sink_sequence.yaml` when sidecar is enabled
- summaries report:
  - `requested_paths`
  - `observed_path_classes`
  - `blocked_by_wrap`
  - `blocked_by_sequence`

## Expected causal shape
- `primary_only`
  - `wrap_only`: label-specific handling only
  - `sequence_only` / `combined`: block
- `alt_only`
  - `wrap_only`: leak persists
  - `sequence_only` / `combined`: block
- `mixed`
  - `wrap_only`: alternate sink leaks before primary-sink handling matters
  - `sequence_only` / `combined`: block
