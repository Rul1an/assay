# Rerun — MCP Fragmented IPI Cross-session Decay Variant (2026Q1)

## Preconditions
- Repo checkout at the paper-grade run commit:
  - `df9650587678`
- Offline-capable build cache available for Cargo
- Cross-session decay Step1, Step2, Step2 fix (`#547`), and Step2.5A/B already present on `main`

## Build
```bash
CARGO_NET_OFFLINE=true cargo build -q -p assay-cli -p assay-mcp-server
```

## Required environment
```bash
export RUN_LIVE=1
export COMPAT_ROOT="$PWD/scripts/ci/fixtures/exp-mcp-fragmented-ipi"
export MCP_HOST_CMD="python3 $PWD/scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py"
export ASSAY_CMD="$PWD/target/debug/assay"
```

## Canonical build-info capture
```bash
GIT_SHA="$(git rev-parse --short=12 HEAD)"
LOCK_HASH="$(shasum -a 256 Cargo.lock | awk '{print $1}')"
ART_ROOT="target/exp-mcp-fragmented-ipi-cross-session-decay-kplus/runs/live-main-$(date +%Y%m%d-%H%M%S)-${GIT_SHA}"
mkdir -p "$ART_ROOT"

python3 - <<'PY' "$ART_ROOT/build-info.json" "$GIT_SHA" "$LOCK_HASH"
import json, pathlib, platform, subprocess, sys
out = pathlib.Path(sys.argv[1])
git_sha = sys.argv[2]
lock_hash = sys.argv[3]
def run(cmd):
    return subprocess.check_output(cmd, text=True).strip()
out.write_text(json.dumps({
    'schema_version': 'exp_mcp_fragmented_ipi_build_info_v1',
    'git_sha': git_sha,
    'rustc_version': run(['rustc','--version']),
    'cargo_version': run(['cargo','--version']),
    'cargo_lock_sha256': lock_hash,
    'platform': platform.platform(),
    'machine': platform.machine(),
    'cargo_net_offline': True,
}, indent=2, sort_keys=True))
PY
```

## Matrix
Run the protected cross-session runner for:
- `DECAY_RUNS=1|2|3`
- `MODE=wrap_only|sequence_only|combined`

This Step2.5C shape measures the full active decay window:
- `k+1` for all `DECAY_RUNS`
- `k+2` when `DECAY_RUNS >= 2`
- `k+3` when `DECAY_RUNS >= 3`

```bash
FIX_DIR="$PWD/scripts/ci/fixtures/exp-mcp-fragmented-ipi"
HOST_CMD="python3 $PWD/scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py"
ASSAY_BIN="$PWD/target/debug/assay"

for DECAY in 1 2 3; do
  for MODE in wrap_only sequence_only combined; do
    OUT_DIR="$PWD/$ART_ROOT/decay_runs_${DECAY}"
    RUN_LIVE=1 \
    DECAY_RUNS="$DECAY" \
    MODE="$MODE" \
    COMPAT_ROOT="$FIX_DIR" \
    COMPAT_AUDIT_LOG="$OUT_DIR/${MODE}/compat-audit.jsonl" \
    MCP_HOST_CMD="$HOST_CMD" \
    ASSAY_CMD="$ASSAY_BIN" \
    OUT_DIR="$OUT_DIR" \
    bash scripts/ci/exp-mcp-fragmented-ipi/cross_session/run_cross_session_decay.sh

    python3 scripts/ci/exp-mcp-fragmented-ipi/score_cross_session_decay.py \
      --session-dir "$OUT_DIR/sessions/${MODE}/decay_runs_${DECAY}" \
      --out "$OUT_DIR/${MODE}-summary.json"
  done
done
```

## Canonical run root
Paper-grade reference artifact:
- `/private/tmp/assay-exp-cross-session-step2-5c/target/exp-mcp-fragmented-ipi-cross-session-decay-kplus/runs/live-main-20260304-102330-df9650587678`

Build provenance:
- `/private/tmp/assay-exp-cross-session-step2-5c/target/exp-mcp-fragmented-ipi-cross-session-decay-kplus/runs/live-main-20260304-102330-df9650587678/build-info.json`

## Scoring and aggregation
Per tuple summary:
```bash
python3 scripts/ci/exp-mcp-fragmented-ipi/score_cross_session_decay.py \
  --session-dir <session-dir> \
  --out <mode-summary.json>
```

Then aggregate the tuple summaries into `combined-summary.json` with rows for:
- `mode`
- `decay_runs`
- delayed sessions `k+1`, `k+2`, `k+3`
- same-session control
- legit control

## Interpretation notes
- Primary metric remains attempt-based:
  - `success_any_sink_canary=true` when a delayed sink session emits a canary-bearing sink query
- This Step2.5C shape closes the prior bounded horizon gap:
  - it explicitly publishes `k+1`, `k+2`, and `k+3` delayed sink sessions when those sessions exist in the active window
- Same-session and legit controls remain separate and semantically named:
  - `session_same_session_control`
  - `session_legit`
