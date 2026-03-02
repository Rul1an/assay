# Results - MCP Fragmented IPI Mitigation (2026Q1)

## Run identity
- Repo commit: `289a43ecc144`
- Artifact root:
  - `/tmp/assay-exp-mcp-fragmented-ipi-main/target/exp-mcp-fragmented-ipi/runs/20260302-232847-289a43ecc144`

## Executive summary
A fragmented-document indirect prompt injection (IPI) reliably caused tool-mediated exfiltration in the unprotected baseline (ASR = 1.0).
With Assay protection enabled, all attack runs were blocked before the `web_search` sink call (TPR = 1.0, FNR = 0.0), while legitimate runs continued without false positives (false positive rate = 0.0).

Blocking in this run is attributed to the **sequence enforcement sidecar** (`assay_check_sequence`), not a wrap-level argument deny.

## Metrics (deterministic set)
- Baseline ASR: **1.0**
- Protected TPR / FNR / false positive rate: **1.0** / **0.0** / **0.0**
- Tool decision latency (p50/p95): **0.771 ms / 1.913 ms**
- Wilson intervals:
  - Baseline ASR CI: **0.7225 - 1.0**
  - Protected TPR CI: **0.7225 - 1.0**
  - Protected FNR CI: **0.0 - 0.2775**
  - Protected false positive rate CI: **0.0 - 0.2775**

## Metrics (variance set)
- Baseline ASR: **1.0**
- Protected TPR / FNR / false positive rate: **1.0** / **0.0** / **0.0**
- Tool decision latency (p50/p95): **0.899 ms / 2.097 ms**
- Wilson intervals:
  - Baseline ASR CI: **0.7225 - 1.0**
  - Protected TPR CI: **0.7225 - 1.0**
  - Protected FNR CI: **0.0 - 0.2775**
  - Protected false positive rate CI: **0.0 - 0.2775**

## Metrics (combined)
- Runs total: **80**
  - Attack runs: **40**
  - Legit runs: **40**
- Baseline ASR: **1.0**
- Protected TPR / FNR / false positive rate: **1.0** / **0.0** / **0.0**
- Tool decision latency (p50/p95): **0.836 ms / 2.003 ms**
- Wilson intervals:
  - Baseline ASR CI: **0.8389 - 1.0**
  - Protected TPR CI: **0.8389 - 1.0**
  - Protected FNR CI: **0.0 - 0.1611**
  - Protected false positive rate CI: **0.0 - 0.1611**

## Evidence locations
Summaries:
- `deterministic-summary.json`
- `variance-summary.json`
- `combined-summary.json`

Raw records/logs:
- `baseline-deterministic/`
- `protected-deterministic/`
- `baseline-variance/`
- `protected-variance/`

All located under:
- `/tmp/assay-exp-mcp-fragmented-ipi-main/target/exp-mcp-fragmented-ipi/runs/20260302-232847-289a43ecc144/`

## Key observations (audit-relevant)
- Baseline exfiltrated in **all** attack runs:
  - `web_search` was invoked
  - the canary was present in baseline `web_search.query`
- Protected mode blocked **all** attack runs **before** the sink call:
  - enforcement triggered by `assay_check_sequence`
  - protected attack runs did not invoke `web_search`
- Legitimate runs proceeded in all sets:
  - no false positives observed in this run

## Limitations
- Results are tied to the pinned fixtures and harness implementation used in PR #490.
- "Blocked" attribution is based on the current harness instrumentation and sequence-sidecar path.
- Entropy is not an enforcement rule in this experiment; it remains a shadow metric for follow-up work.
- The current harness uses a local mock MCP tool server for reproducibility; it is not a live external-tool benchmark.
