# v0.3.4 Adoption Scoreboard

**Goal**: Stabilize "Adoption Hardening" release with < 5% failure rate in Partner CI.

## Top 10 Failure Modes
*Tracking issues reported by Design Partners.*

| Rank | Failure Mode | Label | Count | Fixed by v0.3.4? | Notes |
|------|--------------|-------|-------|-------------------|-------|
| 1 | Trace Miss / Prompts | `trace-miss` | 0 | Yes (E_TRACE_MISS) | |
| 2 | Baseline Churn | `baseline-churn` | 0 | Yes (E_BASE_MISMATCH) | |
| 3 | Schema Drift | `schema-drift` | 0 | Yes (verdict validate) | |
| 4 | Cache Confusion | `cache-confusion` | 0 | Yes (cache split) | |
| 5 | Embedding Dimension Mismatch | `emb-dims` | 0 | Yes (E_EMB_DIMS) | |
| 6 | Judge Variance | `judge-variance` | 0 | Partial (re-run logic) | |
| 7 | Fork Permissions | `fork-permissions` | 0 | Yes (sarif auto) | |
| 8 | Monorepo Paths | `monorepo-paths` | 0 | Yes (workdir input) | |
| 9 | Large Trace Performance | `large-trace` | 0 | No (Warn only) | |
| 10 | Unknown / No Next Step | `no-next-step` | 0 | Yes (Troubleshooting) | |

## Rollout Progress
- [ ] Partner 1 (Onboarded)
- [ ] Partner 2
- [ ] Partner 3
- [ ] Partner 4
- [ ] Partner 5

## Critical Issues (Blockers)
*None*
