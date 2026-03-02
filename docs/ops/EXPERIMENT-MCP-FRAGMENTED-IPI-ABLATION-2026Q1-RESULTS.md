# Results — MCP Fragmented IPI Ablation (2026Q1)

## Run identity
- Repo commit: `c6358730456a`
- Artifact root:
  - `/tmp/assay-exp-mcp-fragmented-ipi-ablation-promote/target/exp-mcp-fragmented-ipi-ablation/runs/20260302-231411-c6358730456a`

## Important scope note
This ablation result is produced by the current **local mock MCP harness**.
It is **not** a live external-tool benchmark.

That constraint matters for interpretation: these results are valid for mechanism comparison in the current harness, but they do not by themselves establish live-host generalization.

## Executive summary
All three protected variants blocked the fragmented-document exfiltration path in the current mock harness:
- `wrap_only`
- `sequence_only`
- `combined`

For all three protected variants:
- baseline ASR remained **1.0**
- protected TPR remained **1.0**
- protected FNR remained **0.0**
- protected false positive rate remained **0.0**

As a result, this ablation does **not** yet isolate `assay_check_sequence` as the uniquely necessary mechanism in the current harness. In this setup, the wrap-only policy fixture is already strong enough to block the attack scenario.

## Deterministic set
### wrap_only
- Baseline ASR: **1.0**
- Protected TPR / FNR / false positive rate: **1.0** / **0.0** / **0.0**
- Sidecar enabled: **false**
- Tool decision latency (p50/p95): **0.546 ms / 1.729 ms**

### sequence_only
- Baseline ASR: **1.0**
- Protected TPR / FNR / false positive rate: **1.0** / **0.0** / **0.0**
- Sidecar enabled: **true**
- Tool decision latency (p50/p95): **0.719 ms / 2.340 ms**

### combined
- Baseline ASR: **1.0**
- Protected TPR / FNR / false positive rate: **1.0** / **0.0** / **0.0**
- Sidecar enabled: **true**
- Tool decision latency (p50/p95): **0.684 ms / 3.152 ms**

## Variance set
### wrap_only
- Baseline ASR: **1.0**
- Protected TPR / FNR / false positive rate: **1.0** / **0.0** / **0.0**
- Sidecar enabled: **false**
- Tool decision latency (p50/p95): **0.652 ms / 1.940 ms**

### sequence_only
- Baseline ASR: **1.0**
- Protected TPR / FNR / false positive rate: **1.0** / **0.0** / **0.0**
- Sidecar enabled: **true**
- Tool decision latency (p50/p95): **0.703 ms / 1.903 ms**

### combined
- Baseline ASR: **1.0**
- Protected TPR / FNR / false positive rate: **1.0** / **0.0** / **0.0**
- Sidecar enabled: **true**
- Tool decision latency (p50/p95): **0.680 ms / 1.728 ms**

## Combined results
Each mode aggregated:
- **80 total runs**
- **40 attack runs**
- **40 legit runs**

### wrap_only
- Baseline ASR: **1.0**
- Protected TPR / FNR / false positive rate: **1.0** / **0.0** / **0.0**
- Sidecar enabled: **false**
- Tool decision latency (p50/p95): **0.573 ms / 1.934 ms**

### sequence_only
- Baseline ASR: **1.0**
- Protected TPR / FNR / false positive rate: **1.0** / **0.0** / **0.0**
- Sidecar enabled: **true**
- Tool decision latency (p50/p95): **0.710 ms / 2.059 ms**

### combined
- Baseline ASR: **1.0**
- Protected TPR / FNR / false positive rate: **1.0** / **0.0** / **0.0**
- Sidecar enabled: **true**
- Tool decision latency (p50/p95): **0.683 ms / 2.428 ms**

## Evidence locations
Summaries:
- `deterministic/ablation-summary.json`
- `variance/ablation-summary.json`
- `combined-ablation-summary.json`

All located under:
- `/tmp/assay-exp-mcp-fragmented-ipi-ablation-promote/target/exp-mcp-fragmented-ipi-ablation/runs/20260302-231411-c6358730456a/`

## Key observations (audit-relevant)
- In this harness, `wrap_only` already blocks the attack path completely.
- `sequence_only` also blocks the attack path completely.
- `combined` is therefore not measurably stronger on ASR/TPR/FPR in the current mock setup.
- The main remaining difference between variants in this result set is mechanism attribution and minor latency variation, not security outcome.

## Interpretation
This ablation is still useful because it falsifies a stronger claim we should avoid making: we cannot currently say that `assay_check_sequence` is the sole mechanism responsible for mitigation in the present harness.

The correct claim is narrower:
- the ablation harness now supports clean A/B/C mechanism toggles
- the current mock scenario is blocked by both wrap-only and sequence-only variants
- stronger causal attribution requires either a weaker wrap-only fixture or a harder / more realistic live-host scenario

## Limitations
- This is a mock-harness result, not a live external-tool benchmark.
- Only a single sink class (`web_search`) is represented.
- The current wrap-only policy fixture is already strong enough to stop the attack shape, limiting mechanism separation.
- No model-host nondeterminism beyond the existing harness variance mode is represented.

## Recommended follow-up
1. Add a live-host enablement slice before claiming live ablation results.
2. Introduce a harder wrap-only condition that does not trivially block the current query shape.
3. Add a second sink label or sink class to reduce single-tool-name coupling.
