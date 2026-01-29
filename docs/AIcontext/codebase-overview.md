# Assay Codebase Overview

## What is Assay?

**Assay** is a **Policy-as-Code** engine for Model Context Protocol (MCP) that validates AI agent behavior. It provides:

- **Deterministic testing**: Replay recorded traces without LLM API calls (milliseconds, $0 cost, 0% flakiness)
- **Runtime security**: Kernel-level enforcement on Linux to block unauthorized tool access
- **Compliance gates**: Validate tool arguments, sequences, and blocklists before production

Assay replaces flaky, network-dependent evals with deterministic replay testing. Record agent behavior once, then validate every PR in milliseconds.

## High-Level Architecture

Assay is a **Rust monorepo** with multiple crates, a **Python SDK**, and comprehensive documentation.

### Core Crates

| Crate | Purpose | Key Responsibilities |
|-------|---------|---------------------|
| `assay-core` | Central evaluation engine | Runner, storage, metrics API, MCP integration, trace handling, baseline/quarantine, providers |
| `assay-cli` | Command line interface | Config loading, Runner construction, test suite execution, reporting |
| `assay-metrics` | Standard metrics library | MustContain, SemanticSimilarity, RegexMatch, JsonSchema, ArgsValid, SequenceValid, ToolBlocklist |
| `assay-mcp-server` | MCP server/proxy | Streaming/online policy enforcement via JSON-RPC over stdio |
| `assay-monitor` | Runtime monitoring | eBPF/LSM integration, kernel-level enforcement |
| `assay-policy` | Policy compilation | Compiles policies into Tier 1 (kernel/LSM) and Tier 2 (userspace) |
| `assay-evidence` | Evidence management | Generates verifiable evidence artifacts for audit/compliance (CloudEvents v1.0, JCS canonicalization, content-addressed IDs) |
| `assay-registry` | Pack Registry client | Secure pack fetching (JCS canonicalization, DSSE verification, OIDC auth, local caching, lockfile v2) |
| `assay-common` | Shared types | Common structs for eBPF/userspace communication |
| `assay-sim` | Attack simulation | Hardening/compliance testing via attack suites |

### Python SDK

Located in `assay-python-sdk/python/assay/`:

- **`client.py`**: `AssayClient` for recording traces to JSONL
- **`coverage.py`**: `Coverage` for analyzing policy coverage
- **`explain.py`**: Human-readable explanations of policy violations
- **`pytest_plugin.py`**: Pytest integration for automatic trace capture

### GitHub Action

**Repository:** https://github.com/Rul1an/assay/tree/main/assay-action

```yaml
- uses: Rul1an/assay/assay-action@v2
```

Features:
- Zero-config evidence bundle discovery
- SARIF integration with GitHub Security tab
- PR comments (only when findings)
- Baseline comparison via cache
- Artifact upload

See [ADR-014](../architecture/ADR-014-GitHub-Action-v2.md) for design details.

### Documentation & Examples

- **`docs/`**: Concepts, use cases, integration guides, reference documentation
- **`examples/`**: Concrete YAML configs, traces, and scenarios (RAG, baseline gate, negation safety)

## Core Components in Detail

### `assay-core` Structure

The core crate is organized into these main modules:

#### Engine (`engine/`)
- **`Runner`**: Central orchestrator
  - `run_suite()`: Parallel test execution with semaphore
  - `run_test_with_policy()`: Retries, policy checks, quarantine, agent assertions
  - `run_test_once()`: Fingerprinting, cache lookup, LLM call/replay, metrics evaluation, baseline check

#### Storage (`storage/`)
- **`Store`**: SQLite wrapper for runs, results, attempts, embeddings, judge cache
- Schema: runs, results, attempts, embeddings, episodes/steps (for trace ingestion)
- Methods: `create_run()`, `insert_result_embedded()`, `get_last_passing_by_fingerprint()`

#### Trace (`trace/`)
- **`ingest`**: JSONL traces → database
- **`precompute`**: Pre-compute embeddings and judge results for deterministic, fast runs
- **`verify`**, **`upgrader`**, **`otel_ingest`**: Schema validation, version migration, OpenTelemetry ingest

#### MCP (`mcp/`)
- JSON-RPC parsing, tool call mapping to policies, audit logging
- **`mapper_v2`**: Maps MCP tool calls to policy checks
- **`proxy`**: Intercepts and validates tool calls, `ProxyConfig` with logging paths
- **`identity`**: Tool identity management (Phase 9) - tool metadata hashing and pinning
- **`policy`**: `McpPolicy` with `tool_pins` for integrity verification
- **`jcs`**: JCS canonicalization (RFC 8785) for deterministic JSON
- **`signing`**: Ed25519 tool signing with DSSE PAE encoding (`sign_tool`, `verify_tool`)
- **`trust_policy`**: Trust policy loading (`require_signed`, `trusted_key_ids`)
- **`decision`**: `DecisionEmitter` for tool.decision events, reason codes (P_*, M_*, S_*)
- **`lifecycle`**: `LifecycleEmitter` for mandate.used/revoked events (CloudEvents)
- **`tool_call_handler`**: Central handler integrating policy + mandate authorization

#### Runtime (`runtime/`)
- **`mandate_store`**: SQLite-backed mandate consumption tracking
  - `AuthzReceipt` with `was_new` flag for idempotent retries
  - `RevocationRecord` for mandate cancellation
  - Deterministic `use_id` computation (content-addressed SHA256)
  - Tables: `mandates`, `mandate_uses`, `nonces`, `mandate_revocations`
- **`authorizer`**: 7-step authorization flow per SPEC-Mandate §7.6-7.8
  - Validity window check (with ±30s skew)
  - Revocation check (no skew - hard cutoff)
  - Scope and kind verification
  - transaction_ref verification for commit tools
  - Atomic consumption
- **`schema`**: SQLite DDL for mandate runtime tables (schema v3)

#### Report (`report/`)
- Output formatters: `console` (summary), `json`, `junit`, `sarif`
- **`RunArtifacts`**: Container for run_id, suite, results

#### Providers & Metrics API
- **`providers/`**: LLM clients (OpenAI, fake, trace replay), embedders, strict mode wrappers
- **`metrics_api.rs`**: Trait definitions that `assay-metrics` implements

#### Other Key Modules
- **`baseline/`**: Compares new scores with historical baselines
- **`quarantine.rs`**: Marks and skips flaky tests
- **`agent_assertions/`**: Enforces sequence and structural expectations on traces (e.g., tool call order)

### `assay-cli` Flow

1. **Entry**: `main.rs` parses `Cli` args → calls `dispatch()`
2. **Command handling**: `dispatch()` matches command → calls handler (e.g., `cmd_run()`)
3. **Runner construction**: `build_runner()` creates `Runner`:
   - Opens `Store` (SQLite)
   - Creates `VcrCache`
   - Selects LLM client (trace replay or live)
   - Loads metrics from `assay-metrics`
   - Configures embedder/judge/baseline if provided
4. **Execution**: `Runner::run_suite()` → parallel `run_test_with_policy()` → `run_test_once()` → LLM call → metric evaluation → store results
5. **Reporting**: `RunArtifacts` → formatters (console/JSON/JUnit/SARIF)

### `assay-metrics` Metrics

Metrics are composable building blocks:

- **Content metrics**: `MustContain`, `MustNotContain`, `RegexMatch`
- **Semantic metrics**: `SemanticSimilarity`, `Faithfulness`, `Relevance` (using embedder/judge)
- **Structure/usage**: `ArgsValid`, `SequenceValid`, `ToolBlocklist`, `Usage`
- **JSON validation**: `JsonSchema` for argument validation

Integration: CLI loads a standard set via `default_metrics()`, and policies reference these metrics per testcase.

### MCP, Policies, Monitor & LSM

#### Policy Compilation (`assay-policy`)
- Policies are compiled into a `CompiledPolicy` with:
  - **Tier 1**: Kernel/LSM rules (exact paths, CIDRs, ports)
  - **Tier 2**: Userspace rules (glob/regex, complex constraints)

#### Monitor & eBPF (`assay-monitor`, `assay-common`, `assay-ebpf`)
- eBPF programs run in kernel
- Userspace monitor reads events and applies Tier 1 rules
- `assay-common` contains no_std-compatible structs for event types, keys, etc.

#### MCP Server (`assay-mcp-server`)
- Runs as MCP proxy via stdio (JSON-RPC)
- Inspects tool calls, applies policies, makes deny/allow decisions
- Handles rate limiting, audit logging

## Execution Flow (CLI → Core)

```
User Command
    ↓
CLI (main.rs)
    ↓
dispatch() → Command Handler
    ↓
build_runner()
    ├─→ Store (SQLite)
    ├─→ VcrCache
    ├─→ LLM Client (trace replay or live)
    ├─→ Metrics (from assay-metrics)
    ├─→ Embedder (optional)
    ├─→ Judge (optional)
    └─→ Baseline (optional)
    ↓
Runner::run_suite()
    ↓
Parallel run_test_with_policy()
    ↓
run_test_once()
    ├─→ Fingerprinting
    ├─→ Cache lookup
    ├─→ LLM call (or replay)
    ├─→ Metrics evaluation
    └─→ Baseline check
    ↓
Store results
    ↓
Report (console/JSON/JUnit/SARIF)
```

## Key Design Principles

1. **Determinism**: Same input + same policy = same result (zero flakiness)
2. **Statelessness**: Validation requires only policy file + trace list
3. **Policy-as-Code**: Uses logic, not LLMs, for evaluation
4. **Separation of Concerns**: CLI handles UX/config, core handles evaluation logic
5. **Extensibility**: Metrics, providers, and policies are pluggable via traits

## Extension Points

### New Metrics
- Implement in `crates/assay-metrics/src/` following the `Metric` trait
- Register in factory (e.g., `default_metrics()`) so policies can use them

### New CLI Commands
- Add to `assay-cli` (CLI structure, command handler)
- Wire to `build_runner()` / `Runner` if needed

### New Policy Features
- Extend policy engine in `assay-core` (parser/validator, constraints)
- Map to `assay-policy` for Tier 1/2 compilation

### New Python SDK Features
- Add thin wrappers in `assay-python-sdk/python/assay/` around existing CLI/core functionality

## Related Documentation

- [User Flows](user-flows.md) - How users interact with the system
- [Interdependencies](interdependencies.md) - Crate relationships and interfaces
- [Architecture Diagrams](architecture-diagrams.md) - Visual architecture representations
- [Entry Points](entry-points.md) - All interaction points

## Architecture Decision Records

Key ADRs for understanding the codebase:

| ADR | Topic | Summary |
|-----|-------|---------|
| [ADR-006](../architecture/ADR-006-Evidence-Contract.md) | Evidence Contract | Schema v1, JCS canonicalization, content-addressed IDs |
| [ADR-007](../architecture/ADR-007-Deterministic-Provenance.md) | Deterministic Provenance | Reproducible bundle generation |
| [ADR-008](../architecture/ADR-008-Evidence-Streaming.md) | Evidence Streaming | OTel Collector pattern, CloudEvents out of hot path |
| [ADR-009](../architecture/ADR-009-WORM-Storage.md) | WORM Storage | S3 Object Lock for compliance retention |
| [ADR-010](../architecture/ADR-010-Evidence-Store-API.md) | Evidence Store API | Multi-tenant REST API for bundle storage |
| [ADR-011](../architecture/ADR-011-Tool-Signing.md) | Tool Signing | Ed25519 `x-assay-sig` for supply chain security ✅ |
| [SPEC-Tool-Signing-v1](../architecture/SPEC-Tool-Signing-v1.md) | Tool Signing Spec | Formal spec: JCS, DSSE PAE, key_id trust ✅ |
| [ADR-013](../architecture/ADR-013-EU-AI-Act-Pack.md) | EU AI Act Pack | Compliance pack system with Article 12 mapping |
| [ADR-014](../architecture/ADR-014-GitHub-Action-v2.md) | GitHub Action v2 | Separate repo, SARIF discipline, zero-config ✅ |
| [ADR-015](../architecture/ADR-015-BYOS-Storage-Strategy.md) | BYOS Storage | Bring-your-own S3 storage strategy ✅ |
| [SPEC-Pack-Registry-v1](../architecture/SPEC-Pack-Registry-v1.md) | Pack Registry | Secure pack fetching: JCS, DSSE sidecar, no-TOFU trust ✅ |

See [ADR Index](../architecture/adrs.md) for the complete list.
