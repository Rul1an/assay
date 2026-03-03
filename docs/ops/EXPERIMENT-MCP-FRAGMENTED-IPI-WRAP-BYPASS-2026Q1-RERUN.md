# Rerun - MCP Fragmented IPI Wrap-bypass Variant (2026Q1)

## Preconditions
- Repository includes the wrap-bypass Step2 harness from PR `#526`.
- Build environment has the required Rust toolchain:
  - `rustc 1.92.0`
  - `cargo 1.92.0`
- Compat host is available in-repo:
  - `scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py`
- Fixture root:
  - `scripts/ci/fixtures/exp-mcp-fragmented-ipi`

## Smoke rerun
From repo root:

```bash
EXPERIMENT_VARIANT=wrap_bypass \
RUN_LIVE=1 \
COMPAT_ROOT="$PWD/scripts/ci/fixtures/exp-mcp-fragmented-ipi" \
MCP_HOST_CMD="python3 $PWD/scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py" \
ASSAY_CMD="$PWD/target/debug/assay" \
bash scripts/ci/test-exp-mcp-fragmented-ipi-wrap-bypass.sh
```

This is a small smoke run. It is useful for configuration validation, not as the paper-grade batch.

## Full matrix rerun
From repo root:

```bash
set -euo pipefail
ROOT="$PWD"
FIX_DIR="$ROOT/scripts/ci/fixtures/exp-mcp-fragmented-ipi"
SHA="$(git rev-parse --short=12 HEAD)"
STAMP="$(date -u +%Y%m%d-%H%M%S)"
RUN_ROOT="$ROOT/target/exp-mcp-fragmented-ipi-wrap-bypass/runs/live-main-$STAMP-$SHA"
mkdir -p "$RUN_ROOT/deterministic" "$RUN_ROOT/variance"

CARGO_NET_OFFLINE=true cargo build -q -p assay-cli -p assay-mcp-server

for set_name in deterministic variance; do
  case "$set_name" in
    deterministic) RUN_SET=deterministic ;;
    variance) RUN_SET=variance ;;
  esac
  for mode in wrap_only sequence_only combined; do
    EXPERIMENT_VARIANT=wrap_bypass \
    RUN_LIVE=1 \
    COMPAT_ROOT="$FIX_DIR" \
    COMPAT_AUDIT_LOG="$RUN_ROOT/compat-audit.jsonl" \
    MCP_HOST_CMD="python3 $ROOT/scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py" \
    ASSAY_CMD="$ROOT/target/debug/assay" \
    RUNS_ATTACK=10 RUNS_LEGIT=10 RUN_SET="$RUN_SET" \
      bash "$ROOT/scripts/ci/exp-mcp-fragmented-ipi/ablation/run_variant.sh" "$RUN_ROOT/$set_name" "$FIX_DIR" "$mode"

    python3 "$ROOT/scripts/ci/exp-mcp-fragmented-ipi/score_wrap_bypass.py" \
      "$RUN_ROOT/$set_name/$mode/baseline_attack.jsonl" \
      "$RUN_ROOT/$set_name/$mode/baseline_legit.jsonl" \
      "$RUN_ROOT/$set_name/$mode/protected_attack.jsonl" \
      "$RUN_ROOT/$set_name/$mode/protected_legit.jsonl" \
      --expected "$FIX_DIR/wrap_bypass/expected_fragments.json" \
      --out "$RUN_ROOT/$set_name/$mode/wrap-bypass-summary.json"
  done
done
```

## Paper-grade reference rerun
The reference artifact used in the current results doc is:
- `/tmp/assay-exp-wrap-bypass-live-main/target/exp-mcp-fragmented-ipi-wrap-bypass/runs/live-main-20260303-122018-8bf0d17ffb1d`

Reference provenance file:
- `/tmp/assay-exp-wrap-bypass-live-main/target/exp-mcp-fragmented-ipi-wrap-bypass/runs/live-main-20260303-122018-8bf0d17ffb1d/build-info.json`

## Output locations
Expected outputs under the run root:
- `deterministic/wrap-bypass-ablation-summary.json`
- `variance/wrap-bypass-ablation-summary.json`
- `combined-summary.json`
- `build-info.json`
- `compat-audit.jsonl`

Per mode, per set:
- `<set>/<mode>/baseline_attack.jsonl`
- `<set>/<mode>/baseline_legit.jsonl`
- `<set>/<mode>/protected_attack.jsonl`
- `<set>/<mode>/protected_legit.jsonl`
- `<set>/<mode>/wrap-bypass-summary.json`

## Audit checklist
For each protected mode, verify:
- `wrap_only`
  - `SIDECAR=disabled`
  - `ASSAY_POLICY=...ablation_wrap_only.yaml`
- `sequence_only`
  - `SIDECAR=enabled`
  - `ASSAY_POLICY=...ablation_sequence_only.yaml`
- `combined`
  - `SIDECAR=enabled`
  - `ASSAY_POLICY=...ablation_combined.yaml`

## Rebuild-grade rerun checklist
- scripts and binaries built from the same checkout SHA
- `Cargo.lock` hash recorded
- `build-info.json` stored next to artifacts
- `cargo_net_offline=true` or equivalent cache mode recorded
- deterministic and variance sets both completed
- combined summary regenerated from the same run root
