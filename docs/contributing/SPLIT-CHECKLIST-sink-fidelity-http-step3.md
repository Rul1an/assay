# Sink-fidelity HTTP Step3 Checklist (Closure)

Scope lock:
- `docs/contributing/SPLIT-CHECKLIST-sink-fidelity-http-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-sink-fidelity-http-step3.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-sink-fidelity-http-step3.sh`

Closure constraints:
- docs+gate only
- no workflow edits
- no code edits under `scripts/ci/exp-mcp-fragmented-ipi/**`
- no edits to `scripts/ci/test-exp-mcp-fragmented-ipi-sink-failure.sh`

## Step2 invariants to re-check

- run shape stays frozen:
  - `RUNS_ATTACK=2`
  - `RUNS_LEGIT=100`
- fidelity marker stays frozen:
  - `SINK_FIDELITY_MODE=http_local`
- primary metric semantics remain unchanged:
  - `success_any_sink_canary`
- completion-layer fields remain published:
  - `egress_http_status_class`
  - `payload_delivered`
  - `response_observed`

## Acceptance re-check

- `wrap_only` remains inferior where expected
- `sequence_only` and `combined` remain robust on protected attacks
- `combined == sequence_only` on protected outcomes
- protected legit false-positive rate stays `0.0`

## Gate expectations

- docs+script-only allowlist vs `BASE_REF`
- workflow-ban
- marker checks (run shape + fidelity + completion fields)
- `cargo fmt --check`
- `cargo clippy -p assay-cli -- -D warnings`
- `cargo test -p assay-cli mcp_wrap_coverage_cli_smoke_writes_report -- --exact`
- bounded run + explicit acceptance checks

## Definition of done

- `BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-sink-fidelity-http-step3.sh` passes
- Step3 diff contains only the 3 closure files
