# Assay - AI Agent Context

## What is Assay?

Assay is a **Policy-as-Code** engine for Model Context Protocol (MCP) that validates AI agent behavior. It provides deterministic testing (trace replay), runtime security (eBPF/LSM kernel enforcement on Linux), and compliance gates (tool argument/sequence validation).

## Workspace Structure

Rust monorepo with workspace version `2.7.0`.

```
crates/
  assay-core/       Core evaluation engine (Runner, Store, MCP, Trace, Report, Providers)
  assay-cli/        CLI binary ("assay") - all user-facing commands
  assay-metrics/    Standard metrics (MustContain, RegexMatch, ArgsValid, SequenceValid, etc.)
  assay-mcp-server/ MCP server/proxy for runtime policy enforcement (JSON-RPC over stdio)
  assay-monitor/    Runtime eBPF/LSM monitoring (Linux only)
  assay-policy/     Policy compilation (Tier 1: kernel, Tier 2: userspace)
  assay-evidence/   Evidence bundles (tar.gz with manifest.json + events.ndjson), lint, diff, sanitize
  assay-common/     Shared types (no_std compatible for eBPF)
  assay-ebpf/       Kernel eBPF programs (LSM hooks + tracepoints)
  assay-sim/        Attack simulation harness (chaos, differential, integrity testing)
  assay-xtask/      Build tooling
assay-python-sdk/   Python SDK (PyO3 bindings + pytest plugin)
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

All commands defined in `crates/assay-cli/src/cli/args.rs`, dispatched in `crates/assay-cli/src/cli/commands/mod.rs`.

| Command | Purpose | Entry File |
|---------|---------|------------|
| `assay run` | Execute test suite against traces | `commands/mod.rs::cmd_run()` |
| `assay validate` | Stateless policy validation | `commands/validate.rs` |
| `assay sim run` | Attack simulation suite | `commands/sim.rs` |
| `assay evidence lint` | Lint bundles (JSON/SARIF output) | `commands/evidence/lint.rs` |
| `assay evidence diff` | Verified-only bundle comparison | `commands/evidence/diff.rs` |
| `assay evidence explore` | Read-only TUI explorer | `commands/evidence/explore.rs` |
| `assay evidence export` | Export evidence bundles | `commands/evidence.rs` |
| `assay mcp-server` | MCP proxy with policy enforcement | `assay-mcp-server/src/main.rs` |
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

### Policy Enforcement (Two-Tier)

- **Tier 1** (Kernel/LSM): Exact paths, CIDRs, ports -> enforced via eBPF in kernel
- **Tier 2** (Userspace): Glob/regex patterns, complex constraints -> MCP server proxy

### Evidence Bundle Format

Evidence bundles are `.tar.gz` files containing:
- `manifest.json`: Schema v1, run metadata, file hashes (SHA-256), Merkle root
- `events.ndjson`: CloudEvents-style evidence events (JCS canonicalized, content-addressed IDs)

Verification: `assay_evidence::verify_bundle_with_limits()` with `VerifyLimits` (100MB compressed, 1GB decompressed, 100k events).

Error classification: `ErrorClass` (Integrity/Contract/Security/Limits) + `ErrorCode` (28+ codes).

## Crate Dependency Graph

```
assay-cli -> assay-core, assay-metrics, assay-monitor, assay-policy, assay-evidence, assay-mcp-server, assay-sim, assay-common
assay-mcp-server -> assay-core, assay-policy, assay-common
assay-monitor -> assay-policy, assay-common, assay-ebpf
assay-core -> assay-common, assay-metrics
assay-evidence -> assay-core, assay-common
assay-sim -> assay-core, assay-evidence
assay-ebpf -> assay-common
```

No circular dependencies. All dependencies flow in one direction.

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
- `scripts/ci/publish_idempotent.sh`: Publish order: assay-common -> assay-evidence -> assay-core -> assay-metrics -> assay-policy -> assay-mcp-server -> assay-monitor -> assay-sim -> assay-cli
- Pre-commit hooks: merge conflicts, YAML/TOML check, trailing whitespace, typos, cargo fmt
- Pre-push hooks: cargo clippy, linux compile gate

## Conventions

- Workspace version in root `Cargo.toml` (`version = "2.7.0"`)
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

## Security Considerations

- All bundle content treated as hostile input
- Terminal sanitization on all TUI-rendered strings (OSC8, OSC52, CSI, BEL stripped)
- Path traversal protection on baseline keys and tar paths
- Verify-before-render / verify-before-diff invariants
- VerifyLimits prevent resource exhaustion (zip bombs, oversized bundles)
- Writer path normalization: always POSIX-style `/`, reject `..` components
