# Assay - AI Agent Context

> Cross-agent work must first read and follow [AGENTS.md](AGENTS.md). That file is the shared
> source of truth for branch ownership, ADR-042/043 scope, review quorum, and verification. This
> file adds Assay-specific technical context and does not override that contract.

## What is Assay?

Assay is a **Policy-as-Code** engine for Model Context Protocol (MCP) that validates AI agent behavior. It provides deterministic testing (trace replay), runtime security (eBPF/LSM kernel enforcement on Linux), and policy gates (tool argument/sequence validation).

## Workspace Structure

Rust monorepo, workspace version `5.1.0`, 22 workspace packages (21 under `crates/` plus
`assay-python-sdk`; 9 are `publish = false`). Curated view, grouped by role:

```
crates/
  # Core engine + CLI
  assay-core/       Core evaluation engine (Runner, Store, MCP, Trace, Report, Providers, VCR, Replay Bundle)
  assay-cli/        CLI binary ("assay") - all user-facing commands
  assay-metrics/    Standard metrics (MustContain, RegexMatch, ArgsValid, SequenceValid, etc.)
  assay-common/     Shared types (no_std compatible for eBPF)
  assay-canonical/  Deterministic canonicalization: RFC 8785 (JCS) bytes, sha256 content IDs, semantic digests

  # Evidence + distribution
  assay-evidence/   Evidence bundles (tar.gz with manifest.json + events.ndjson), lint, diff, sanitize
  assay-registry/   Pack Registry client (HTTP, DSSE verification, OIDC auth, local caching, lockfile v2)
  gateway-evidence-replay/  Deterministic replay verifier for gateway-path evidence bundles (standalone)

  # Policy + runtime enforcement
  assay-policy/     Policy compilation (Tier 1: kernel, Tier 2: userspace)
  assay-mcp-server/ MCP server/proxy for runtime policy enforcement (JSON-RPC over stdio)
  assay-monitor/    Runtime eBPF/LSM monitoring (Linux only)
  assay-ebpf/       Kernel eBPF programs (LSM hooks + tracepoints)

  # Measured-run substrate (internal/experimental "Assay-Runner", API unstable)
  assay-runner-core/    Runner orchestration, archive assembly, layer normalizers
  assay-runner-linux/   Linux-only platform adapter, cgroup placement primitives
  assay-runner-schema/  Versioned schema types + constants for Runner v0 contracts

  # Protocol adapters (evidence translation)
  assay-adapter-api/  Adapter API contracts (shared trait surface)
  assay-adapter-a2a/  A2A protocol adapter
  assay-adapter-acp/  ACP protocol adapter
  assay-adapter-ucp/  UCP protocol adapter

  # Simulation + tooling
  assay-sim/        Attack simulation harness (chaos, differential, integrity testing)
  assay-xtask/      Build tooling
assay-python-sdk/   Python SDK (PyO3 bindings + pytest plugin; crate name "assay-it")
```

## Key Commands

```bash
cargo build -p assay-cli                    # Build CLI
cargo test --workspace                      # Run all tests
cargo test -p assay-sim                     # Run sim tests only
cargo clippy --workspace --all-targets -- -D warnings  # Lint
cargo xtask build-ebpf                      # Build eBPF (Linux)
```

## CLI Entry Points

All commands defined in `crates/assay-cli/src/cli/args/mod.rs`, dispatched in `crates/assay-cli/src/cli/commands/mod.rs`. The table below is a representative subset; the CLI has ~40 subcommands (see `commands/` for the full set, including `import`, `project-otel`, `inventory`, `discover`, and the `verify-*` evidence family).

| Command | Purpose | Entry File |
|---------|---------|------------|
| `assay run` | Execute test suite against traces | `commands/mod.rs::cmd_run()` |
| `assay validate` | Stateless policy validation | `commands/validate.rs` |
| `assay sim run` | Attack simulation suite | `commands/sim.rs` |
| `assay evidence lint` | Lint bundles (JSON/SARIF output) | `commands/evidence/lint.rs` |
| `assay evidence diff` | Verified-only bundle comparison | `commands/evidence/diff.rs` |
| `assay evidence explore` | Read-only TUI explorer | `commands/evidence/explore.rs` |
| `assay evidence export` | Export evidence bundles | `commands/evidence.rs` |
| `assay-mcp-server` | MCP proxy with policy enforcement. A **separate binary**, not an `assay` subcommand: `assay mcp` exists and is a different thing (wrap, discover, inventory, kill, tool) | `assay-mcp-server/src/main.rs` |
| `assay monitor` | eBPF runtime monitoring (Linux) | `commands/monitor.rs` |
| `assay sandbox` | Landlock sandbox execution | `commands/sandbox.rs` |
| `assay doctor` | Diagnostic tool | `commands/doctor.rs` |

## Core Architecture

### Execution Flow (CLI -> Core)

```
CLI main.rs -> dispatch() -> build_runner() -> Runner::run_suite()
  Runner creates: Store (SQLite), VcrCache, LLM Client, Metrics, Embedder, Judge, Baseline
  Per test: fingerprint -> cache lookup -> LLM call/replay -> metrics eval -> baseline check -> store
  Output: RunArtifacts -> formatters (console/JSON/JUnit/SARIF)
```

### Key Interfaces

- **`Metric` trait** (`assay-core::metrics_api`): `evaluate(&self, response, expected) -> MetricResult`
- **`LlmClient` trait** (`assay-core::providers::llm`): OpenAI, Fake, Trace replay, Strict wrapper
- **`Embedder` trait** (`assay-core::providers::embedder`): OpenAI, Fake
- **`Store`** (`assay-core::storage`): SQLite wrapper for runs, results, attempts, embeddings
- **`VcrClient`** (`assay-core::vcr`): HTTP record/replay for deterministic LLM testing

### Policy Enforcement (Two-Tier)

- **Tier 1** (Kernel/LSM): Exact paths, CIDRs, ports -> enforced via eBPF in kernel
- **Tier 2** (Userspace): Glob/regex patterns, complex constraints -> MCP server proxy

### Evidence Bundle Format

Evidence bundles are `.tar.gz` files containing:
- `manifest.json`: Schema v1, run metadata, file hashes (SHA-256), and a deterministic SHA-256 run-root digest
- `events.ndjson`: CloudEvents-style evidence events (JCS canonicalized, content-addressed IDs)

Verification: `assay_evidence::verify_bundle_with_limits()` with `VerifyLimits` (100MB compressed, 1GB decompressed, 100k events).

Error classification: `ErrorClass` (Integrity/Contract/Security/Limits) + `ErrorCode` (28+ codes).

## Crate Dependency Graph

```
assay-cli -> assay-core, assay-metrics, assay-monitor, assay-common, assay-policy, assay-evidence, assay-registry, assay-mcp-server, assay-runner-core, assay-runner-schema, assay-runner-linux, assay-sim
assay-mcp-server -> assay-core, assay-common, assay-metrics
assay-monitor -> assay-common, assay-policy
assay-metrics -> assay-core, assay-common
assay-core -> assay-adapter-api, assay-common
assay-evidence -> assay-canonical, assay-common  (+ assay-runner-schema, DEV-ONLY)
assay-registry -> assay-common
assay-adapter-api -> assay-evidence
assay-adapter-{a2a,acp,ucp} -> assay-adapter-api, assay-evidence
assay-runner-core -> assay-common, assay-monitor, assay-runner-schema
assay-sim -> assay-core, assay-evidence
assay-ebpf -> assay-common
```

Leaf crates (no internal dependencies): `assay-common`, `assay-canonical`, `assay-policy`, `assay-runner-schema`, `assay-runner-linux`, `gateway-evidence-replay`, `assay-xtask`.

No circular dependencies. All dependencies flow in one direction.

The one dev-only edge is marked as such above and is deliberately not a production dependency.
`assay-evidence`'s claim gate re-states, over a different vocabulary, the occurrence-versus-absence rule
`assay-runner-schema::RunnerClaimGate` has enforced since 2026-06-01, so the two are pinned against each
other by `tests/claim_gate_parity.rs` rather than left to drift. Per one-rule-one-function a parity test
is the sanctioned fallback when one rule cannot simply call the other; promoting it to a real dependency
is an ADR question, not a test fixture. Note `docs/generated/crate-deps.mermaid` draws dependency edges
without distinguishing kind, so that edge appears there as if it were architectural.

`assay-evidence -> assay-common` carries two shared primitives, and the test for admitting one
is the same in both cases: a mechanism whose second implementation would silently mean something
different. The bounded-ingest primitive (`assay_common::limits::LimitReader`, ADR-043 §1) is
shared with the replay verifier in `assay-core` so both apply one ceiling. `assay_common::dsse`
holds the DSSE Pre-Authentication Encoding for the same reason: PAE defines what a signature
covers, so two constructions of it are two definitions of what a signature means, and both
`assay-evidence`'s mandate signing and `assay-core`'s MCP signing call it.

`assay-registry` now calls the shared PAE too, which is its first internal dependency and was
taken deliberately rather than in passing: PAE defines what a signature covers, so a crate-local
construction of it is a second answer to that question. The portability objection that held it
back was checked and is void, since the release workflow ships an `x86_64-pc-windows-msvc` binary
of `assay-cli`, which already depends on both crates. Two PAEs stay unshared, in `assay-registry`'s `sigstore_bundle` tests and `assay-cli`'s
supply-chain conformance tests: each builds a PAE, signs it, and asserts the production verifier
accepts it, so they are independent constructions that would catch the shared one drifting.
Sharing them would leave those tests signing with the same code they verify with.

Vocabularies still stay domain-local. `VerifyLimits` describes an evidence bundle and does not
travel; neither does a payload type or a key policy. What travels is the construction, never the
domain's reading of it.

## assay-sim (Attack Simulation)

Suite tiers: `Quick` (<30s, PR gate), `Nightly` (5-15 min), `Stress`, `Chaos` (long-running).

```
assay sim run --suite quick --seed 42 --target bundle.tar.gz --report sim.json
```

Exit codes: 0=clean, 1=bypass (security regression), 2=infra error.

Key modules:
- `suite.rs`: Orchestrator, `SuiteConfig`, `SuiteTier`, `TimeBudget`, `catch_unwind` shielding
- `attacks/integrity.rs`: 8 attack vectors (bitflip, truncate, inject, zip bomb, tar duplicate, BOM, CRLF, bundle size)
- `attacks/chaos.rs`: `IOChaosReader` (fault injection: Interrupted, WouldBlock, short reads), malformed gzip
- `attacks/differential.rs`: Reference verifier (in-memory, non-streaming) + parity check
- `differential.rs`: Write-then-verify round-trip invariant testing
- `report.rs`: `SimReport`, `AttackResult`, `AttackStatus` (Passed/Failed/Blocked/Bypassed/Error)
- `mutators/`: `Mutator` trait, BitFlip, Truncate, InjectFile

## Evidence DX Tooling (ADR-007)

### Lint (`assay evidence lint`)
- SARIF 2.1.0 output with `partialFingerprints`, `automationDetails`, `security-severity`
- Rule registry: `ASSAY-E001` (error), `ASSAY-W001` (warning) etc.
- Verifies bundle first, then applies lint rules per event
- Module: `crates/assay-evidence/src/lint/` (engine.rs, rules.rs, sarif.rs)

### Diff (`assay evidence diff`)
- Verifies both bundles before diffing (security invariant)
- Semantic diff: network hosts, filesystem paths, process subjects
- `--baseline-dir` + `--key` with path traversal protection (`validate_baseline_key()`)
- Module: `crates/assay-evidence/src/diff/`

### Explore TUI (`assay evidence explore`)
- ratatui + crossterm, behind `tui` feature flag
- Terminal sanitization: strips ESC/CSI/OSC/BEL, replaces control chars with U+FFFD
- Raw-mode restore guaranteed via wrapper pattern (even on error)
- Input filtering: rejects control chars, caps query length
- Module: `crates/assay-evidence/src/sanitize.rs`, `crates/assay-cli/src/cli/commands/evidence/explore.rs`

## Python SDK

Located in `assay-python-sdk/python/assay/`:
- `client.py`: `AssayClient` for recording traces to JSONL
- `coverage.py`: Policy coverage analysis
- `explain.py`: Human-readable violation explanations
- `pytest_plugin.py`: Automatic trace capture in pytest

## CI/CD

- `.github/workflows/ci.yml`: Main CI (clippy, tests, parity)
- `.github/workflows/release.yml`: Release workflow (binaries + crates.io + PyPI)
- `.github/workflows/perf_main.yml`: Bencher baseline (main), percentage test 25% threshold
- `.github/workflows/perf_pr.yml`: Bencher PR compare, clone thresholds, `--err`
- `.github/workflows/perf_nightly.yml`: Forensic tail-latency analysis, BMF JSON → Bencher
- `scripts/ci/publish_idempotent.sh`: Publish order: assay-common -> assay-evidence -> assay-core -> assay-metrics -> assay-policy -> assay-mcp-server -> assay-monitor -> assay-sim -> assay-cli
- Pre-commit hooks: merge conflicts, YAML/TOML check, trailing whitespace, typos, cargo fmt
- Pre-push hooks: cargo clippy, linux compile gate
- All third-party actions SHA-pinned (see `docs/PINNED-ACTIONS.md`)

## VCR Middleware (HTTP Record/Replay)

Module: `crates/assay-core/src/vcr/mod.rs`

HTTP record/replay for deterministic testing of LLM/embedding calls without network.

### Usage

```rust
use assay_core::vcr::{VcrClient, VcrMode};

// Replay mode (CI default)
let vcr = VcrClient::new(VcrMode::ReplayStrict, cassette_dir);
let resp = vcr.post_json(url, &body, auth).await?;

// Record mode (local, needs API key)
let vcr = VcrClient::new(VcrMode::Record, cassette_dir);
```

### Environment Variables

- `ASSAY_VCR_MODE`: `replay_strict` (default), `replay`, `record`, `auto`, `off`
- `ASSAY_VCR_DIR`: Cassette directory (default: `tests/fixtures/perf/semantic_vcr/cassettes`)

### Provider Integration

OpenAI embedder and LLM client support VCR via:
- `OpenAIEmbedder::with_vcr(model, api_key, vcr)` — explicit VCR injection
- `OpenAIEmbedder::from_env(model, api_key)` — auto-enable based on `ASSAY_VCR_MODE`

Cassettes: `tests/fixtures/perf/semantic_vcr/cassettes/openai/{embeddings,judge}/`

## Performance Assessment

### Scripts

| Script | Purpose |
|--------|---------|
| `scripts/perf_assess.sh` | Smoke tests + parallel matrix + store metrics |
| `scripts/perf_e2e.sh` | Hyperfine e2e benchmarks (small/file_backed/ci) |

### Forensic Mode

```bash
FORENSIC=1 ./scripts/perf_assess.sh           # Tail-latency deep dive
FORENSIC=1 BMF_JSON=1 ./scripts/perf_assess.sh  # Bencher Metric Format output
```

Outputs: median, p95, p99, max, stddev, tail_ratio (p99/median), sqlite_busy_count

### Alarm Thresholds

| Metric | Healthy | Warn | Fail |
|--------|---------|------|------|
| tail_ratio | < 1.5 | 1.5-2.0 | > 2.0 |
| p95 drift | < +15% | +15-25% | > +25% |
| sqlite_busy_count | 0 | 1-5 | > 5 |

### Criterion Benchmarks

```bash
cargo bench -p assay-core --bench store_write_heavy
cargo bench -p assay-cli --bench suite_run_worstcase
```

Benches: `swc/500x400b`, `swc/120xlarge`, `sw/50x400b`, `sw/12xlarge`, `sr/wc`

`store_write_heavy` measures one code path twice. `swc/*` runs it with `journal_mode=MEMORY`,
so no commit waits on the device and none creates or unlinks a journal file; that measures
what our code costs, and it is the group compared on pull requests. `sw/*` runs the store
exactly as `Store::open` ships it (`journal_mode=delete`, `synchronous=FULL`), where 98.2% of
wall time is filesystem work — it stays a main-only trend under an outlier-robust model,
because per-PR it reports which disk the runner drew. Measured run-to-run spread across five
consecutive local runs: 1.31x for `sw/*`, 1.03x for `swc/*`.

Do not switch `swc/*` to WAL. It looked faster and measured worse (2.03x spread), and its
untracked `-wal`/`-shm` sidecars leaked 37k file pairs in one sweep until SQLite failed.

A Bencher Threshold is scoped to (Branch, Testbed, Measure) and covers every benchmark in that
scope; there is no per-benchmark threshold. Testbed is therefore used to separate alert models
(`ubuntu-latest-store-code`, `ubuntu-latest-store-fsync`, `ubuntu-latest`) even though all
three run on the same machine.

Which benchmarks a PR runs is derived from the `cargo metadata` dependency closure by
`scripts/ci/perf_bench_relevance.py`, not from a path pattern — `^crates/` matched all 21
crates and alerted on changes outside the benchmark's compilation unit.

See `docs/PERFORMANCE-ASSESSMENT.md` for full documentation, including measurement provenance.

## Conventions

- Workspace version lives in root `Cargo.toml`
- Internal crate deps use `workspace = true` with path + version
- `#[deny(unsafe_code)]` on all crates except assay-ebpf
- Error handling: `anyhow` for applications, `thiserror` for libraries
- Async runtime: `tokio`
- Serialization: `serde` + `serde_json` + `serde_yaml`
- Platform-specific code behind `#[cfg(target_os = "linux")]` or `#[cfg(unix)]`

## Exit Codes

| Code | CLI (assay run) | Sim (assay sim) | Lint (assay evidence lint) |
|------|----------------|-----------------|---------------------------|
| 0 | All tests pass | All attacks blocked | No findings above threshold |
| 1 | Test failure | Bypass found (regression) | Findings found |
| 2 | Config error | Infra error (panic/timeout) | Verification failure |
| 3 | Infra/judge unavailable | — | — |
| 4 | Would block (sandbox/policy) | — | — |

## Security Considerations

- All bundle content treated as hostile input
- Terminal sanitization on all TUI-rendered strings (OSC8, OSC52, CSI, BEL stripped)
- Path traversal protection on baseline keys and tar paths
- Verify-before-render / verify-before-diff invariants
- VerifyLimits prevent resource exhaustion (zip bombs, oversized bundles)
- Writer path normalization: always POSIX-style `/`, reject `..` components
