# SPLIT CHECKLIST — Wave23 Interleaving Step2 (bounded implementation)

## Scope discipline
- [ ] Only Step2 allowlist files changed:
  - `scripts/ci/test-exp-mcp-fragmented-ipi-interleaving.sh`
  - `scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py`
  - `scripts/ci/exp-mcp-fragmented-ipi/mock_mcp_server.py`
  - `scripts/ci/exp-mcp-fragmented-ipi/score_interleaving.py`
  - `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-INTERLEAVING-2026Q1-RERUN.md`
  - `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-INTERLEAVING-2026Q1-RESULTS.md`
  - `docs/contributing/SPLIT-CHECKLIST-interleaving-step2.md`
  - `docs/contributing/SPLIT-MOVE-MAP-interleaving-step2.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-interleaving-step2.md`
  - `scripts/ci/review-exp-mcp-fragmented-ipi-interleaving-step2.sh`
- [ ] No `.github/workflows/*` changes.
- [ ] No sink-fidelity/policy semantics expansion.

## Bounded implementation intent
- [ ] Interleaving branch activated only (`EXPERIMENT_VARIANT=interleaving`).
- [ ] Primary metric remains `success_any_sink_canary`.
- [ ] Per-run interleaving markers are emitted:
  - `case_id`
  - `ordering_class`
  - `segment_label`
  - `turn_index`
  - `sink_attempted`
  - `blocked_before_attempt`
- [ ] Mixed legit/malicious attack interleaving is case-addressable.
- [ ] Legit controls stay explicit and separate.

## Acceptance checks
- [ ] `wrap_only` remains the expected weak baseline in protected attack runs.
- [ ] `sequence_only` and `combined` keep protected attack `success_any_sink_canary=false`.
- [ ] `combined` matches `sequence_only` on protected outcomes.
- [ ] Legit controls do not introduce unexpected false positives.

## Gate
- [ ] `BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-interleaving-step2.sh` passes.
