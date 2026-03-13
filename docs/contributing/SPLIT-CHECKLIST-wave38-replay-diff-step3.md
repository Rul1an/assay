# SPLIT CHECKLIST - Wave38 Replay Diff Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-CHECKLIST-wave38-replay-diff-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave38-replay-diff-step3.md`
  - `scripts/ci/review-wave38-replay-diff-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes
- [ ] No scope leaks outside Wave38 Step3

## Closure intent
- [ ] Step3 is docs+gate only
- [ ] Step3 introduces no runtime behavior changes
- [ ] Step3 introduces no schema changes
- [ ] Step3 reruns Step2 invariants without widening scope
- [ ] Step3 preserves the bounded replay-diff contract

## Replay-diff invariants
- [ ] Replay-diff markers remain present:
  - `ReplayDiffBasis`
  - `ReplayDiffBucket`
  - `basis_from_decision_data`
  - `classify_replay_diff`
  - `Unchanged`
  - `Stricter`
  - `Looser`
  - `Reclassified`
  - `EvidenceOnly`
- [ ] Existing convergence markers remain present
- [ ] Existing normalization markers remain present
- [ ] Existing deny-path separation remains present

## Non-goals still enforced
- [ ] No new obligation types added
- [ ] No runtime enforcement expansion added
- [ ] No policy backend replacement added
- [ ] No control-plane semantics added
- [ ] No auth transport changes added

## Validation
- [ ] Step3 gate passes against stacked Step2 base
- [ ] Step3 gate can also pass against `origin/main` after Step2 merge
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned tests remain green
