# Interleaving Step3 Checklist (Closure)

Scope lock:
- `docs/contributing/SPLIT-CHECKLIST-interleaving-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-interleaving-step3.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-interleaving-step3.sh`
- docs+gate only; no code changes in Step3
- no workflow edits

## Closure invariants (re-run)

- frozen run shape remains:
  - `RUNS_ATTACK=2`
  - `RUNS_LEGIT=100`
- scorer semantics unchanged:
  - attempt-based metric remains `success_any_sink_canary`
- interleaving publication fields remain present:
  - `case_id`
  - `ordering_class`
  - `segment_label`
  - `turn_index`
  - `blocked_before_attempt`
  - `success_any_sink_canary`
- bounded interleaving smoke remains green
- acceptance remains:
  - `sequence_only` and `combined` stay robust where expected
  - `wrap_only` remains inferior where expected
  - `combined` equals `sequence_only` on protected outcomes

## Gate expectations

- allowlist-only diff vs `BASE_REF`
- workflow-ban (`.github/workflows/*`)
- `cargo fmt --check`
- `cargo clippy -p assay-cli -- -D warnings`
- `cargo test -p assay-cli mcp_wrap_coverage_cli_smoke_writes_report -- --exact`
- bounded run + explicit acceptance checks

## Definition of done

- `BASE_REF=origin/codex/wave23-interleaving-step2-impl bash scripts/ci/review-exp-mcp-fragmented-ipi-interleaving-step3.sh` passes
- `BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-interleaving-step3.sh` passes after sync
- Step3 diff contains only the 3 Step3 allowlisted files
