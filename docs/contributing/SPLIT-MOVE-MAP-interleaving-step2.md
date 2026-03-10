# SPLIT MOVE MAP — Wave23 Interleaving Step2

## Goal
Activate a bounded interleaving branch in the existing fragmented-IPI harness without changing established sink-failure semantics.

## File-level mapping
| Target | Change type | Bound intent |
|---|---|---|
| `scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py` | bounded extension | add `interleaving` variant planning and per-call markers (`segment_label`, `turn_index`, `ordering_class`, `case_id`) |
| `scripts/ci/exp-mcp-fragmented-ipi/mock_mcp_server.py` | bounded helper extension | expose `web_search_alt` in interleaving variant to keep mixed route class deterministic |
| `scripts/ci/exp-mcp-fragmented-ipi/score_interleaving.py` | new scorer | publish attempt-based interleaving summary with CI + derived rates |
| `scripts/ci/test-exp-mcp-fragmented-ipi-interleaving.sh` | new bounded runner | fixed case matrix, fixed mode set, strict acceptance assertions |
| `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-INTERLEAVING-2026Q1-RERUN.md` | rerun guide | frozen run shape + scoring commands |
| `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-INTERLEAVING-2026Q1-RESULTS.md` | publication doc | bounded results and interpretation |
| `scripts/ci/review-exp-mcp-fragmented-ipi-interleaving-step2.sh` | reviewer gate | allowlist + workflow-ban + bounded run + acceptance checks |

## Harness mapping details
### Driver additions
- Add `interleaving` to `--experiment-variant` choices.
- Add interleaving inputs:
  - `--interleaving-case-id`
  - `--interleaving-ordering-class`
- Add interleaving sink-plan constructor with explicit turn metadata.
- Preserve existing variants (`standard`, `wrap_bypass`, `second_sink`, `sink_failure`, `cross_session_decay`) unchanged.

### Scorer additions
- New interleaving summary schema:
  - `schema_version=exp_mcp_fragmented_ipi_interleaving_summary_v1`
- Preserve primary metric interpretation:
  - `success_any_sink_canary`
- Publish interleaving markers and derived rates:
  - case/order/segment/turn
  - `sink_attempted_rate`
  - `blocked_before_attempt_rate`
  - CI fields for protected outcomes

### Runner additions
- Fixed bounded matrix:
  - `interleave_primary_malicious_first`
  - `interleave_primary_legit_first`
  - `interleave_mixed_near_parallel`
- Fixed modes:
  - `wrap_only`
  - `sequence_only`
  - `combined`
- Fixed run shape:
  - `RUNS_ATTACK=2`
  - `RUNS_LEGIT=100`

## Non-goals
- No workflow changes.
- No policy redesign.
- No new sink-fidelity axis in this slice.
- No scorer semantic broadening beyond interleaving markers and publication fields.
