# Rerun — MCP Fragmented IPI Ablation (2026Q1)

## Preconditions
- Repository checked out at commit `dd6c0c9952a3` or later with equivalent experiment files
- Compat-host present at:
  - `scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py`
- Live ablation harness present from:
  - PR `#499`
  - PR `#503`
  - PR `#510`
  - PR `#511`
  - PR `#513`

## One-command live pilot
From repo root:

```bash
RUN_LIVE=1 \
COMPAT_ROOT="$PWD/scripts/ci/fixtures/exp-mcp-fragmented-ipi" \
MCP_HOST_CMD="python3 $PWD/scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py" \
ASSAY_CMD="$PWD/target/debug/assay" \
bash scripts/ci/test-exp-mcp-fragmented-ipi-ablation.sh
```

## Extended live run matching the published batch
From repo root:

```bash
RUN_ID="$(date -u +%Y%m%d-%H%M%S)-$(git rev-parse --short=12 HEAD)"
ART_ROOT="target/exp-mcp-fragmented-ipi-ablation/runs/$RUN_ID"
FIX_DIR="scripts/ci/fixtures/exp-mcp-fragmented-ipi"
HOST_CMD="python3 $PWD/scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py"
ASSAY_BIN="$PWD/target/debug/assay"

for SET in deterministic variance; do
  SET_ROOT="$ART_ROOT/$SET"
  mkdir -p "$SET_ROOT"
  for MODE in wrap_only sequence_only combined; do
    RUN_LIVE=1 \
    COMPAT_ROOT="$PWD/$FIX_DIR" \
    COMPAT_AUDIT_LOG="$PWD/$SET_ROOT/$MODE/compat-audit.jsonl" \
    MCP_HOST_CMD="$HOST_CMD" \
    ASSAY_CMD="$ASSAY_BIN" \
    RUNS_ATTACK=10 \
    RUNS_LEGIT=10 \
    RUN_SET="$SET" \
    bash scripts/ci/exp-mcp-fragmented-ipi/ablation/run_variant.sh "$PWD/$SET_ROOT" "$PWD/$FIX_DIR" "$MODE"
  done
  python3 scripts/ci/exp-mcp-fragmented-ipi/ablation/score_ablation.py \
    --root "$PWD/$SET_ROOT" \
    --out "$PWD/$SET_ROOT/ablation-summary.json"
done
```

## Audit checklist
Per protected mode log, confirm:
- `wrap_only`
  - `ABLATION_MODE=wrap_only`
  - `SIDECAR=disabled`
  - `ASSAY_POLICY=...ablation_wrap_only.yaml`
- `sequence_only`
  - `ABLATION_MODE=sequence_only`
  - `SIDECAR=enabled`
  - `ASSAY_POLICY=...ablation_sequence_only.yaml`
- `combined`
  - `ABLATION_MODE=combined`
  - `SIDECAR=enabled`
  - `ASSAY_POLICY=...ablation_combined.yaml`

Also confirm in each per-mode `summary.json`:
- `conditions.protected.mode`
- `conditions.protected.sidecar_enabled`
- `protected_wrap_policies`
- `protected_sequence_policy_files`

## Rebuild-grade rerun checklist
Use this checklist before treating a rerun as the final publication artifact:
1. Build `target/debug/assay` and `target/debug/assay-mcp-server` from the same checkout used for scripts.
2. Record `git rev-parse HEAD`.
3. Record `rustc --version`.
4. Record `cargo --version`.
5. Record a hash of `Cargo.lock`.
6. Ensure `ASSAY_CMD` points to the freshly built `target/debug/assay`.
7. Ensure `COMPAT_ROOT` points to the exact fixture tree used in the run.
8. Archive the full run root, including `compat-audit.jsonl` files.
9. Confirm protected logs contain the correct `SIDECAR` and `ASSAY_POLICY` markers per mode.
10. Confirm `combined-summary.json` is generated from the final rerun root, not copied from earlier pilot runs.

## Published run caveat
The currently published live ablation run used:
- scripts/tree from `dd6c0c9952a3`
- binaries copied from `f4364a09a09b`

That is acceptable as strong experiment evidence, but it is not the cleanest single-source provenance run.

## Troubleshooting
- If local builds fail because Cargo cannot download crates, fix the Cargo cache or use a network-enabled environment, then rerun from a clean checkout.
- If the compat host fails immediately, verify `COMPAT_ROOT` is set and points to the fixture directory containing `canary.txt`.
- If `RUN_LIVE=1` fails preflight, verify `MCP_HOST_CMD` and `ASSAY_CMD` are non-empty.
