# SPLIT PLAN — Wave23 Interleaving (MCP Fragmented IPI)

## Summary
Wave23 opens a new bounded hypothesis after the Wave20-Wave22 line closure:
- mixed legit + malicious interleaving in one bounded run shape
- optional near-parallel tool-call ordering pressure inside the same harness family

The experiment line remains bounded:
- no workflow changes
- no policy-semantic rewrites
- no sink-fidelity expansion beyond current line in Step2
- primary governance metric stays attempt-based (`success_any_sink_canary`)

## A/B/C slicing
### Step1 freeze (this slice)
Branch: `codex/wave23-interleaving-step1-freeze` (base: `main`)

Deliverables:
- `docs/contributing/SPLIT-PLAN-wave23-interleaving.md`
- `docs/contributing/SPLIT-CHECKLIST-interleaving-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-interleaving-step1.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-interleaving-step1.sh`

Step1 is docs+gate only.

### Step2 bounded implementation (preview)
Intended scope (bounded):
- `scripts/ci/test-exp-mcp-fragmented-ipi-interleaving.sh`
- `scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py`
- `scripts/ci/exp-mcp-fragmented-ipi/score_interleaving.py` (new)
- `scripts/ci/fixtures/exp-mcp-fragmented-ipi/interleaving/**` (new)
- `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-INTERLEAVING-2026Q1-RERUN.md`
- `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-INTERLEAVING-2026Q1-RESULTS.md`
- Step2 checklist/review-pack/reviewer script

Frozen Step2 intent:
- keep mode set: `wrap_only`, `sequence_only`, `combined`
- keep attempt-based primary metric semantics
- publish per-run interleaving markers (ordering class + route class)
- keep legit controls explicit and report false-positive with CI

### Step3 closure (preview)
Docs+gate only:
- Step3 checklist
- Step3 review pack
- Step3 reviewer gate

## Step1 reviewer gate requirements
- allowlist-only on Step1 files
- workflow-ban (`.github/workflows/*`)
- fail if tracked diff touches:
  - `scripts/ci/exp-mcp-fragmented-ipi/**`
  - `scripts/ci/test-exp-mcp-fragmented-ipi*.sh`
- fail if untracked files appear under:
  - `scripts/ci/exp-mcp-fragmented-ipi/**`
- `cargo fmt --check`
- `cargo clippy -p assay-cli -- -D warnings`
- deterministic smoke:
  - `cargo test -p assay-cli mcp_wrap_coverage_cli_smoke_writes_report -- --exact`

## Acceptance (Step1)
- freeze scope and boundaries are explicit
- no runtime/harness mutation
- reviewer gate passes against `origin/main`

## Non-goals
- no external egress or new network dependencies in Step1
- no semantic claim expansion in Step1
- no CI workflow edits
