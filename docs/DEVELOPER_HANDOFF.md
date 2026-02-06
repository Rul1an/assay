# Assay Developer Handoff Guide

> **Version:** 2.10.0 | **Last Updated:** January 2026
>
> Complete onboarding document for Rust developers joining the Assay project.

---

## Table of Contents

1. [What is Assay?](#what-is-assay)
2. [Quick Start](#quick-start)
3. [Architecture Overview](#architecture-overview)
4. [Crate Map](#crate-map)
5. [Key Data Flows](#key-data-flows)
6. [Current Priorities (Q2 2026)](#current-priorities-q2-2026)
7. [Code Conventions](#code-conventions)
8. [Testing](#testing)
9. [Common Tasks](#common-tasks)
10. [Key Files Reference](#key-files-reference)
11. [ADRs to Read](#adrs-to-read)

---

## What is Assay?

Assay is an **end-to-end governance pipeline for AI agents** via the Model Context Protocol. The core paradigm is a closed-loop workflow: observe → generate → profile → lock → gate → evidence → audit.

| Capability | Description |
|------------|-------------|
| **Deterministic Testing** | Record/replay agent traces without LLM calls (ms, $0, 0% flakiness) |
| **Policy Generation** | Auto-generate policies from observed behavior with multi-run profiling |
| **Evidence Bundles** | Tamper-evident, content-addressed audit trails (CloudEvents + JCS) |
| **Compliance Packs** | Signed rule packs with deterministic lockfiles and supply-chain verification |
| **MCP Proxy** | Runtime policy enforcement for Model Context Protocol tool calls |
| **Kernel Sandbox** | Optional eBPF/LSM defense-in-depth for Linux deployments |

**Target Users:** AI/ML Engineers, DevOps/Platform Engineers, Security/Compliance Officers

---

## Quick Start

```bash
# Clone and build
git clone https://github.com/Rul1an/assay.git
cd assay
cargo build --workspace

# Run tests
cargo test --workspace

# Run CLI
cargo run -p assay-cli -- --help

# Example: verify an evidence bundle
cargo run -p assay-cli -- evidence verify tests/fixtures/evidence/test-bundle.tar.gz
```

### Prerequisites

- Rust 1.75+ (2021 edition)
- Linux for eBPF features (macOS/Windows for core features)
- Python 3.10+ (for SDK development)

### Pre-commit Hooks

```bash
pip install pre-commit
pre-commit install
```

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                            User Interface                                │
├─────────────────┬─────────────────┬─────────────────┬───────────────────┤
│   assay-cli     │  Python SDK     │  GitHub Action  │  MCP Server       │
│   (commands)    │  (bindings)     │  (CI/CD)        │  (proxy)          │
└────────┬────────┴────────┬────────┴────────┬────────┴─────────┬─────────┘
         │                 │                 │                  │
         ▼                 ▼                 ▼                  ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                           assay-core                                     │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌──────────────┐   │
│  │ engine/      │ │ storage/     │ │ mcp/         │ │ trace/       │   │
│  │ Runner       │ │ Store        │ │ Proxy        │ │ Ingest       │   │
│  └──────────────┘ └──────────────┘ └──────────────┘ └──────────────┘   │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌──────────────┐   │
│  │ baseline/    │ │ report/      │ │ providers/   │ │ config/      │   │
│  │ Regression   │ │ SARIF/JUnit  │ │ LLM/Embedder │ │ Loading      │   │
│  └──────────────┘ └──────────────┘ └──────────────┘ └──────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
         │                                      │
         ▼                                      ▼
┌─────────────────────────┐          ┌─────────────────────────┐
│     assay-evidence      │          │     assay-metrics       │
│  - BundleWriter/Reader  │          │  - MustContain          │
│  - JCS canonicalization │          │  - ArgsValid            │
│  - CloudEvents v1.0     │          │  - SequenceValid        │
│  - Lint rules + SARIF   │          │  - SemanticSimilarity   │
└─────────────────────────┘          └─────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                     Runtime Security (Linux only)                        │
├─────────────────────────┬───────────────────────────────────────────────┤
│     assay-monitor       │              assay-ebpf                        │
│  - eBPF loader          │           - LSM programs                       │
│  - Event streaming      │           - Tracepoints                        │
│  - Tier 1 enforcement   │           - no_std kernel code                 │
└─────────────────────────┴───────────────────────────────────────────────┘
```

### Two-Tier Policy Model

| Tier | Location | Capabilities | Latency |
|------|----------|--------------|---------|
| **Tier 1** | Kernel (eBPF/LSM) | Exact paths, CIDRs, ports | <1μs |
| **Tier 2** | Userspace (MCP Proxy) | Globs, regex, JSON Schema | <1ms |

---

## Crate Map

### Core Crates (Always Build)

| Crate | Lines | Purpose | Key Types |
|-------|-------|---------|-----------|
| `assay-core` | ~15K | Central evaluation engine | `Runner`, `Store`, `McpProxy` |
| `assay-cli` | ~8K | CLI commands + reporting | `Cli`, `Command`, `dispatch()` |
| `assay-metrics` | ~2K | Evaluation metrics | `Metric` trait, `MustContain`, etc. |
| `assay-evidence` | ~4K | Evidence bundles | `BundleWriter`, `BundleReader`, `Manifest` |
| `assay-policy` | ~1K | Policy compilation | `CompiledPolicy`, Tier 1/2 split |
| `assay-common` | ~500 | Shared types | `MonitorEvent`, `InodeKey` (no_std) |

### Platform-Specific (Linux Only)

| Crate | Purpose | Notes |
|-------|---------|-------|
| `assay-monitor` | eBPF loader + event stream | Requires `aya` 0.13 |
| `assay-ebpf` | Kernel LSM programs | `#![no_std]`, cross-compiled |

### Tools & Testing

| Crate | Purpose |
|-------|---------|
| `assay-mcp-server` | Standalone MCP proxy binary |
| `assay-sim` | Attack simulation for hardening tests |
| `assay-xtask` | Build tasks (eBPF Docker build) |
| `assay-python-sdk` | PyO3 bindings + pytest plugin |

---

## Key Data Flows

### Flow 1: Test Execution (`assay run`)

```
CLI args → load_config() → build_runner()
                               │
                    ┌──────────┴──────────┐
                    │      Runner         │
                    │  - Store (SQLite)   │
                    │  - VcrCache         │
                    │  - LLM Client       │
                    │  - Metrics          │
                    └──────────┬──────────┘
                               │
                    run_suite() [parallel]
                               │
                    ┌──────────┴──────────┐
                    │  run_test_once()    │
                    │  - Fingerprint      │
                    │  - Cache lookup     │
                    │  - LLM/Replay       │
                    │  - Metrics eval     │
                    │  - Baseline check   │
                    └──────────┬──────────┘
                               │
                    Store results → Report (SARIF/JUnit/Console)
```

### Flow 2: Evidence Bundle Creation (`assay evidence export`)

```
Profile (native events)
         │
         ▼
    EvidenceMapper
         │
         ▼
    EvidenceEvent (CloudEvents v1.0)
         │
         ▼
    JCS Canonicalization (RFC 8785)
         │
         ▼
    SHA-256 → content-addressed ID
         │
         ▼
    BundleWriter → bundle.tar.gz
         │
         ├─ manifest.json (with bundle_id)
         └─ events.jsonl
```

### Flow 3: MCP Policy Enforcement

```
Agent Tool Call (JSON-RPC)
         │
         ▼
    McpProxy.handle_request()
         │
         ▼
    mapper_v2::map_to_policy_check()
         │
    ┌────┴────┐
    │ Policy  │
    │ Engine  │
    └────┬────┘
         │
    ┌────┴────────────────┐
    │                     │
    ▼                     ▼
Tier 1 (kernel)    Tier 2 (userspace)
    │                     │
    └─────────┬───────────┘
              │
         Allow/Deny + Audit Log
```

---

## Current Priorities (Q2 2026)

Per [ROADMAP.md](./ROADMAP.md) and [ADR-016](./architecture/ADR-016-Pack-Taxonomy.md):

| Priority | Feature | Crate(s) | Status |
|----------|---------|----------|--------|
| ✅ | GitHub Action v2 | External repo | Complete |
| ✅ | BYOS CLI (`push/pull/list`) | `assay-cli`, `assay-evidence` | Complete |
| ✅ | Tool Signing (`x-assay-sig`) | `assay-cli`, `assay-core` | Complete |
| ✅ | Pack Engine (OSS) | `assay-evidence`, `assay-cli` | Complete (v2.10.0) |
| ✅ | EU AI Act Baseline Pack (OSS) | `packs/` | Complete (v2.10.0) |
| **P2** | Mandate/Intent Evidence | `assay-core` | **Next** |
| **P2** | Action v2.1 | External repo | After mandate |

### ✅ BYOS CLI Commands (Complete)

S3-compatible storage support:

```bash
assay evidence push bundle.tar.gz --store s3://bucket/prefix
assay evidence pull --bundle-id sha256:... --store s3://bucket/prefix
assay evidence list --run-id run_123 --store s3://bucket/prefix
```

Supported backends: AWS S3, Backblaze B2, Wasabi, Cloudflare R2, MinIO, Azure Blob, GCS, local filesystem.

### ✅ Tool Signing (Complete)

MCP tool definition signing with `x-assay-sig` field:

```bash
assay tool keygen --out ~/.assay/keys/    # Generate PKCS#8/SPKI keypair
assay tool sign tool.json --key priv.pem --out signed.json
assay tool verify signed.json --pubkey pub.pem  # Exit: 0=ok, 2=unsigned, 3=untrusted, 4=invalid
```

**Key files:**
- `crates/assay-core/src/mcp/signing.rs` - Ed25519 sign/verify with DSSE PAE
- `crates/assay-core/src/mcp/jcs.rs` - JCS canonicalization (RFC 8785)
- `crates/assay-core/src/mcp/trust_policy.rs` - Trust policy loading
- `crates/assay-cli/src/cli/commands/tool/` - CLI commands

See [SPEC-Tool-Signing-v1](./architecture/SPEC-Tool-Signing-v1.md) for the formal specification.

### ✅ Pack Engine + EU AI Act Baseline (Complete v2.10.0)

Following the Semgrep open core model (engine + baseline free, pro + workflows enterprise):

```bash
assay evidence lint --pack eu-ai-act-baseline    # Article 12 baseline checks (OSS)
assay evidence lint --pack eu-ai-act-baseline,soc2-baseline  # Composition
assay evidence lint --pack ./custom-pack.yaml    # Custom pack
assay evidence lint --pack eu-ai-act-baseline --format sarif  # GitHub Code Scanning
```

**Key files:**
- `crates/assay-evidence/src/lint/packs/` - Pack engine modules (schema, loader, executor, checks)
- `packs/eu-ai-act-baseline.yaml` - Baseline pack with Article 12 mapping

**Key specs:**
- Rule ID namespacing: `{pack}@{version}:{rule_id}`
- Pack kind enforcement: `compliance` requires disclaimer (hard fail if missing)
- SARIF output via `properties` bags (GitHub Code Scanning compatible)
- Pack digest: `sha256(JCS(JSON(yaml)))` for supply chain integrity
- GitHub dedup: `primaryLocationLineHash` fingerprint
- Truncation: `--max-results` for SARIF size limits (default: 500)

See [ADR-013](./architecture/ADR-013-EU-AI-Act-Pack.md), [ADR-016](./architecture/ADR-016-Pack-Taxonomy.md), and [SPEC-Pack-Engine-v1](./architecture/SPEC-Pack-Engine-v1.md).

### P2: Mandate/Intent Evidence

Support for AP2-style authorization evidence (agentic commerce).

---

## Code Conventions

### Error Handling

```rust
// ✅ Good: Use Result with context
fn load_bundle(path: &Path) -> Result<Bundle> {
    let file = File::open(path)
        .with_context(| format!("Failed to open bundle: {}", path.display()))?;
    // ...
}

// ❌ Bad: Panic
fn load_bundle(path: &Path) -> Bundle {
    let file = File::open(path).unwrap(); // DON'T
}
```

### Async Patterns

```rust
// Use tokio for async, but prefer sync where possible
// Most CLI commands are sync; async for I/O-heavy operations

#[tokio::main]
async fn main() -> Result<()> {
    // Entry point
}

// In core: many functions are sync, async only where needed
impl Store {
    pub fn insert_result(&self, result: &TestResult) -> Result<()> {
        // Sync SQLite operations
    }
}
```

### Logging

```rust
use tracing::{debug, info, warn, error};

// Structured logging with context
info!(bundle_id = %manifest.bundle_id, "Bundle verified");
warn!(path = %path.display(), "File not found, skipping");
```

### Feature Flags

```rust
// Platform-specific code
#[cfg(target_os = "linux")]
pub mod monitor;

// Optional features
#[cfg(feature = "tui")]
pub mod explore;
```

---

## Testing

### Unit Tests

```bash
# All workspace tests
cargo test --workspace

# Specific crate
cargo test -p assay-evidence

# Specific test
cargo test -p assay-evidence -- verify_bundle
```

### Integration Tests

```bash
# CLI integration tests
cargo test -p assay-cli --test '*'

# E2E shell tests
./tests/e2e/run_all.sh
```

### Test Fixtures

- `tests/fixtures/golden/` - Golden file tests (expected outputs)
- `tests/fixtures/evidence/test-bundle.tar.gz` - Valid evidence bundle
- `tests/fixtures/mcp/` - MCP test scenarios

### Writing Tests

```rust
#[test]
fn test_bundle_verification() {
    let bundle_path = Path::new("tests/fixtures/evidence/test-bundle.tar.gz");
    let result = verify_bundle(File::open(bundle_path).unwrap(), VerifyLimits::default());
    assert!(result.is_ok());
    assert_eq!(result.unwrap().event_count, 5);
}

#[tokio::test]
async fn test_async_operation() {
    // Async tests need tokio::test
}
```

---

## Common Tasks

### Add a New CLI Command

1. **Define args** in `crates/assay-cli/src/cli/args.rs`:
```rust
#[derive(Subcommand)]
pub enum EvidenceCommand {
    // ... existing
    Push(PushArgs),
}

#[derive(Args)]
pub struct PushArgs {
    pub bundle: PathBuf,
    #[arg(long)]
    pub run_id: Option<String>,
}
```

2. **Implement handler** in `crates/assay-cli/src/cli/commands/evidence/push.rs`:
```rust
pub async fn run(args: PushArgs) -> Result<()> {
    // Implementation
}
```

3. **Wire up dispatch** in `crates/assay-cli/src/cli/commands/evidence/mod.rs`:
```rust
EvidenceCommand::Push(args) => push::run(args).await,
```

### Add a New Evidence Lint Rule

1. **Add rule** in `crates/assay-evidence/src/lint/rules/`:
```rust
pub struct MyNewRule;

impl LintRule for MyNewRule {
    fn id(&self) -> &str { "E0XX" }
    fn severity(&self) -> Severity { Severity::Warning }
    fn check(&self, event: &EvidenceEvent) -> Option<Finding> {
        // Check logic
    }
}
```

2. **Register** in `crates/assay-evidence/src/lint/registry.rs`:
```rust
registry.register(Box::new(MyNewRule));
```

### Add a New Metric

1. **Implement** in `crates/assay-metrics/src/my_metric.rs`:
```rust
pub struct MyMetric { /* config */ }

impl Metric for MyMetric {
    fn name(&self) -> &str { "my_metric" }
    fn evaluate(&self, response: &str, expected: Option<&str>) -> MetricResult {
        // Evaluation logic
    }
}
```

2. **Export** in `crates/assay-metrics/src/lib.rs`:
```rust
pub use my_metric::MyMetric;

pub fn default_metrics() -> Vec<Arc<dyn Metric>> {
    vec![
        // ... existing
        Arc::new(MyMetric::default()),
    ]
}
```

---

## Key Files Reference

### Entry Points

| File | Purpose |
|------|---------|
| `crates/assay-cli/src/main.rs` | CLI entry point |
| `crates/assay-cli/src/cli/commands/mod.rs` | Command dispatch |
| `crates/assay-mcp-server/src/main.rs` | MCP server entry |

### Core Engine

| File | Purpose |
|------|---------|
| `crates/assay-core/src/engine/runner.rs` | Test execution orchestrator |
| `crates/assay-core/src/storage/store.rs` | SQLite persistence |
| `crates/assay-core/src/mcp/proxy.rs` | MCP policy proxy |
| `crates/assay-core/src/mcp/mapper_v2.rs` | Tool call → policy mapping |

### Evidence

| File | Purpose |
|------|---------|
| `crates/assay-evidence/src/bundle.rs` | BundleWriter/BundleReader |
| `crates/assay-evidence/src/manifest.rs` | Manifest struct + serialization |
| `crates/assay-evidence/src/verify.rs` | Bundle verification |
| `crates/assay-evidence/src/lint/mod.rs` | Lint engine + SARIF output |

### Configuration

| File | Purpose |
|------|---------|
| `crates/assay-core/src/config/mod.rs` | Config loading |
| `crates/assay-core/src/model.rs` | Core data models |

---

## ADRs to Read

Essential reading for understanding design decisions:

| ADR | Topic | Priority |
|-----|-------|----------|
| [ADR-006](./architecture/ADR-006-Evidence-Contract.md) | Evidence schema, JCS, content-addressing | **Must read** |
| [ADR-015](./architecture/ADR-015-BYOS-Storage-Strategy.md) | BYOS storage strategy | **Must read** |
| [ADR-011](./architecture/ADR-011-Tool-Signing.md) | Tool signing design | For P1 work |
| [ADR-014](./architecture/ADR-014-GitHub-Action-v2.md) | GitHub Action design | Reference |

---

## Questions?

- **Architecture:** Check `docs/AIcontext/` for AI-friendly documentation
- **Roadmap:** See `docs/ROADMAP.md`
- **ADRs:** See `docs/architecture/adrs.md` for decision index

---

## Appendix: Useful Commands

```bash
# Build release binary
cargo build --release -p assay-cli

# Check for lint issues
cargo clippy --workspace -- -D warnings

# Format code
cargo fmt --all

# Generate docs
cargo doc --workspace --no-deps --open

# Run specific test with output
cargo test -p assay-evidence -- --nocapture verify_bundle

# Check what would be published
cargo publish -p assay-evidence --dry-run

# Build eBPF (Linux/Docker only)
cargo xtask build-ebpf --docker
```
