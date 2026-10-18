# MCP Proxy-Enforce Runtime & Resource Pilot (`#2494`)

Bounded, single-host Linux pilot measuring the runtime and evidence-output cost of wrapping a local stdio MCP server with `assay-mcp-server proxy-enforce` (`#2494`).

## Design & Arms

- **Arm A (`direct_stdio`):** Caller communicates directly over stdio with the deterministic synthetic upstream (`crates/assay-mcp-server/tests/fixtures/proxy/mock_upstream.py` in `MOCK_UPSTREAM_MODE=p60a`).
- **Arm B (`proxy_enforce_allow`):** Caller communicates over stdio with `assay-mcp-server proxy-enforce` wrapping the same upstream under `examples/privileged-action-gate/policies/allow.yaml` and `examples/privileged-action-gate/baseline-approved.json`, writing `assay.enforcement_decision.v0` records to `decisions.ndjson`.
- **Counterbalanced Pairing:** Sessions run sequentially in paired `AB` / `BA` blocks on the same host. Analysis is performed at the paired block level (`proxy_enforce_allow - direct_stdio`) with median paired deltas and a deterministic block-level bootstrap 95% confidence interval (`BOOTSTRAP_SEED = 24942026`).
- **Separate Deny Control (`proxy_enforce_deny`):** One session runs under `examples/privileged-action-gate/policies/no-allowance.yaml` and verifies positive upstream initialization/discovery (`initialize` and `tools/list` observed on the upstream), zero forwarded `tools/call` requests, caller error `-31999` (`no_declared_allowance`), and both `assay.enforcement_decision.v0` and `assay.denied_call_observation.v1` outputs.

## Phase Separation & Memory Scope

1. **Startup / Manifest Establishment (`startup_ms`):** Spawn through `initialize`, `notifications/initialized`, and `tools/list` completion (including proxy-owned upstream establishment).
2. **In-Session Sequential Calls (`in_session_call_median_ms`, `in_session_call_total_ms`):** Fixed `github.add_deploy_key` `tools/call` round-trip times inside the established session.
3. **Closure / Evidence Flush (`closure_flush_ms`):** Caller `stdin` EOF through stdout drain and `wait4` process exit (when `assay-mcp-server` has flushed its output files prior to exit); post-exit `decisions.ndjson` parsing and validation occur after the timer stops.
4. **Memory (`wait4_max_rss_kib`):** Labeled `maximum_single_process_high_water` (`comparable_across_arms: false`, `combined_process_tree_claimed: false`). Linux `wait4` `ru_maxrss` reports the maximum single-process resident high-water mark across the waited process image and its waited children, not a combined process-tree sum.

## Commands

Run the behavioral regression suite locally:

```bash
python3 -m unittest -v docs/experiments/mcp-proxy-cost-2026-10/test_proxy_cost_pilot.py
```

Run the Linux pilot against the pinned `v6.14.1` `assay-mcp-server` binary (requires an exclusive `--out-dir` that does not yet exist):

```bash
python3 docs/experiments/mcp-proxy-cost-2026-10/proxy_cost_pilot.py \
  --mcp-binary /abs/path/to/assay-mcp-server \
  --expected-binary-sha256 787403002c677892593c5bafe89ebe183cb1516e959c5feac1ae72249882960c \
  --out-dir /abs/path/to/exclusive-pilot-output \
  --blocks 6 \
  --calls-per-session 5 \
  --timeout-seconds 30
```

## Non-Claims

- `execution_valid: true` records that harness, protocol, and evidence checks completed; it is not an automatic publishable conclusion and does not replace external archive attestation verification.
- Exact sent request frames and received response wire bytes are retained on the caller side (`caller_requests.ndjson`, `caller_responses.ndjson`); the upstream fixture log (`upstream_raw.jsonl`) is strip-normalized (`fixture_strip_normalized_jsonl`) and used for semantic call parity.
- Does not claim tail-latency SLAs (`p95`/`p99`), multi-client concurrency, network/HTTP transport overhead, combined process-tree memory, or zero-overhead equivalence.
