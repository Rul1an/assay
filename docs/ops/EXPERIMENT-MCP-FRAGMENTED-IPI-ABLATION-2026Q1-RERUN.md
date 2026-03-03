# Rerun — MCP Fragmented IPI Ablation (2026Q1)

## Preconditions
- Repository checked out at commit `33208d4b4ddb` or an equivalent later commit
- Compat-host present at:
  - `scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py`
- Live ablation harness present from:
  - PR `#499`
  - PR `#503`
  - PR `#510`
  - PR `#511`
  - PR `#513`

## Reference paper-grade artifact
- run root:
  - `/tmp/assay-exp-hermetic-rerun/target/exp-mcp-fragmented-ipi-ablation/runs/hermetic-20260303-110905-33208d4b4ddb`
- build metadata:
  - `/tmp/assay-exp-hermetic-rerun/target/exp-mcp-fragmented-ipi-ablation/runs/hermetic-20260303-110905-33208d4b4ddb/build-info.json`

## One-command live pilot
From repo root:

```bash
RUN_LIVE=1 \
COMPAT_ROOT="$PWD/scripts/ci/fixtures/exp-mcp-fragmented-ipi" \
MCP_HOST_CMD="python3 $PWD/scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py" \
ASSAY_CMD="$PWD/target/debug/assay" \
bash scripts/ci/test-exp-mcp-fragmented-ipi-ablation.sh
```

## Extended live run matching the paper-grade batch
From repo root:

```bash
RUN_ID="hermetic-$(date -u +%Y%m%d-%H%M%S)-$(git rev-parse --short=12 HEAD)"
ART_ROOT="target/exp-mcp-fragmented-ipi-ablation/runs/$RUN_ID"
FIX_DIR="scripts/ci/fixtures/exp-mcp-fragmented-ipi"
HOST_CMD="python3 $PWD/scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py"
ASSAY_BIN="$PWD/target/debug/assay"
LOCK_HASH="$(shasum -a 256 Cargo.lock | awk '{print $1}')"

mkdir -p "$ART_ROOT"

python3 - <<'PY' "$ART_ROOT/build-info.json" "$(git rev-parse --short=12 HEAD)" "$LOCK_HASH"
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
}, indent=2, sort_keys=True))
PY

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

## Note on offline build provenance
The reference rerun was built from local Cargo cache and produced a provenance-clean scripts/binaries artifact.
The current `build-info.json` does not explicitly include `CARGO_NET_OFFLINE=true`; if that matters for a future artifact line, extend the metadata capture step to include an allowlisted env snapshot.

## Troubleshooting
- If local builds fail because Cargo cannot download crates, prime the Cargo cache first or use a network-enabled environment, then rerun from a clean checkout.
- If the compat host fails immediately, verify `COMPAT_ROOT` is set and points to the fixture directory containing `canary.txt`.
- If `RUN_LIVE=1` fails preflight, verify `MCP_HOST_CMD` and `ASSAY_CMD` are non-empty.
