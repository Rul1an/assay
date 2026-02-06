# Code Map

This document provides a detailed mapping of important files, modules, and their responsibilities in the Assay codebase.

## File Structure Overview

```
assay/                         # Version 2.16.0
├── crates/                    # Rust crates
│   ├── assay-core/            # Core evaluation engine
│   ├── assay-cli/             # CLI interface
│   │   └── src/cli/commands/
│   │       ├── evidence/      # Evidence subcommands (lint, diff, explore, push, pull, list)
│   │       ├── tool/          # Tool signing (keygen, sign, verify)
│   │       └── policy/        # Policy subcommands (fmt, validate, migrate)
│   ├── assay-metrics/         # Standard metrics
│   ├── assay-mcp-server/      # MCP server
│   ├── assay-monitor/         # Runtime monitoring
│   ├── assay-policy/          # Policy compilation
│   ├── assay-evidence/        # Evidence management (CloudEvents, JCS, bundles)
│   ├── assay-registry/         # Pack Registry client
│   ├── assay-common/          # Shared types
│   ├── assay-ebpf/            # eBPF programs
│   └── assay-sim/             # Attack simulation
├── assay-python-sdk/          # Python SDK
├── assay-action/              # GitHub Action (legacy, see below)
├── docs/                      # Documentation
│   ├── architecture/          # ADRs and architecture docs
│   └── AIcontext/             # This directory
├── examples/                  # Example configs and traces
├── tests/                     # Integration tests
└── .github/workflows/         # CI/CD workflows

## GitHub Action (Separate Repository)

**Repository:** https://github.com/Rul1an/assay/tree/main/assay-action

The GitHub Action is maintained in a separate repository for GitHub Marketplace publication.

**Usage:**
```yaml
- uses: Rul1an/assay/assay-action@v2
```

**Note:** The `assay-action/` directory in this monorepo is legacy and redirects to the separate repository.
```

## Core Crate (`assay-core`)

### Entry Point
- **`src/lib.rs`**: Public API exports, module declarations

### Engine Module (`src/engine/`)
- **`runner.rs`**:
  - `Runner` struct: Central orchestrator
  - `run_suite()`: Parallel test execution
  - `run_test_with_policy()`: Retry logic, quarantine, error policies
  - `run_test_once()`: Single test execution with caching

### Storage Module (`src/storage/`)
- **`store.rs`**:
  - `Store` struct: SQLite database wrapper
  - `create_run()`, `insert_result_embedded()`, `get_last_passing_by_fingerprint()`
- **`schema.rs`**: Database schema definitions
- **`rows.rs`**: Row type definitions
- **`judge_cache.rs`**: Judge result caching

### Trace Module (`src/trace/`)
- **`ingest.rs`**: JSONL trace ingestion into database
- **`precompute.rs`**: Pre-compute embeddings and judge results
- **`verify.rs`**: Trace schema validation
- **`upgrader.rs`**: Trace version migration
- **`otel_ingest.rs`**: OpenTelemetry trace ingestion
- **`schema.rs`**: Trace schema definitions
- **`truncation.rs`**: Trace truncation logic

### MCP Module (`src/mcp/`)
- **`mod.rs`**: Module exports
- **`proxy.rs`**: `McpProxy` - Intercepts and validates MCP tool calls
- **`policy.rs`**: `McpPolicy` - Policy wrapper with `tool_pins` for integrity
- **`mapper_v2.rs`**: Maps MCP tool calls to policy checks
- **`jsonrpc.rs`**: JSON-RPC parsing
- **`parser.rs`**: MCP message parsing
- **`types.rs`**: MCP type definitions
- **`audit.rs`**: Audit logging
- **`identity.rs`**: Tool identity management (Phase 9) - `ToolIdentity`, metadata hashing, pinning
- **`runtime_features.rs`**: Runtime feature flags
- **`jcs.rs`**: JCS canonicalization (RFC 8785) for tool signing
- **`signing.rs`**: Ed25519 tool signing with DSSE PAE encoding
- **`trust_policy.rs`**: Trust policy loading and key_id matching

### Report Module (`src/report/`)
- **`console.rs`**: Console output formatter; **`print_run_footer(seeds, judge_metrics)`** — prints `Seeds: seed_version=1 order_seed=… judge_seed=…` and judge metrics line (PR #159)
- **`summary.rs`**: **`Summary`** with `seeds: Seeds`, `judge_metrics: Option<JudgeMetrics>`; **`Seeds`** (order_seed, judge_seed as string|null via serde_seed); **`with_seeds()`**; **`write_summary()`**
- **`json.rs`**: JSON output formatter
- **`junit.rs`**: JUnit XML output formatter
- **`sarif.rs`**: SARIF output (write_sarif, write_sarif_with_limit); deterministic truncation, runs[0].properties.assay when truncated (PR #160)

### Providers Module (`src/providers/`)
- **`llm/mod.rs`**: LLM client trait and implementations
  - **`openai.rs`**: OpenAI API client
  - **`fake.rs`**: Mock LLM client for testing
- **`embedder/mod.rs`**: Embedder trait and implementations
  - **`openai.rs`**: OpenAI embeddings client
  - **`fake.rs`**: Mock embedder
- **`trace.rs`**: Trace replay client
- **`strict.rs`**: Strict mode wrappers

### Policy Engine (`src/policy_engine.rs`)
- Policy parsing and validation
- Policy evaluation logic
- Constraint checking

### Metrics API (`src/metrics_api.rs`)
- `Metric` trait definition
- Used by `assay-metrics` for implementations

### Replay Bundle Module (`src/replay/`)
- **`mod.rs`**: Module exports, public API
- **`manifest.rs`**: `ReplayManifest` (schema v1), `ReplaySeeds`, `ReplayCoverage`, `ScrubPolicy`, `ToolchainMeta`, `RunnerMeta`, `FileManifestEntry`
- **`bundle.rs`**: `write_bundle_tar_gz()` (deterministic .tar.gz), `bundle_digest()` (SHA256), `validate_entry_path()` (fail-closed path validation), `build_file_manifest()`
- **`toolchain.rs`**: `capture_toolchain()` for rustc/cargo metadata

### Other Key Modules
- **`config.rs`**: Configuration loading and resolution
- **`model.rs`**: Core data models (EvalConfig, TestCase, etc.)
- **`cache/`**: VCR-style caching
- **`baseline/`**: Baseline regression detection
- **`quarantine.rs`**: Flaky test quarantine
- **`judge/`**: LLM-as-judge for semantic metrics
- **`agent_assertions/`**: Tool call sequence assertions
- **`explain.rs`**: Violation explanation
- **`coverage.rs`**: Coverage calculation
- **`doctor/`**: Diagnostic tools
- **`validate.rs`**: Stateless validation
- **`discovery/`**: Auto-discovery of configs and MCP servers
- **`kill_switch/`**: Process termination on violations

## CLI Crate (`assay-cli`)

### Entry Point
- **`src/main.rs`**:
  - CLI argument parsing
  - Calls `dispatch()` to route commands
  - Exit code handling

### Command Dispatch (`src/cli/commands/mod.rs`)
- **`dispatch()`**: Routes commands to handlers
- **`build_runner()`**: Constructs `Runner` with all dependencies
- **`write_extended_run_json()`**: Writes run.json with exit_code, reason_code, reason_code_version, seed_version, order_seed, judge_seed (string|null), judge_metrics (PR #159), sarif.omitted when truncated (PR #160)
- **`write_run_json_minimal()`**: Early-exit run.json (seeds null when unknown)
- **`print_run_footer(seeds, judge_metrics)`**: Calls assay_core report::console; prints Seeds line and judge metrics to stderr
- Command handlers for each subcommand (cmd_run, cmd_ci set summary.with_seeds and call print_run_footer)

### Command Handlers (`src/cli/commands/`)
- **`run.rs`**: `assay run` command
- **`validate.rs`**: `assay validate` command
- **`init.rs`**: `assay init` command
- **`import.rs`**: `assay import` command
- **`trace.rs`**: `assay trace` command
- **`generate.rs`**: `assay generate` command
- **`record.rs`**: `assay record` command
- **`migrate.rs`**: `assay migrate` command
- **`doctor.rs`**: `assay doctor` command
- **`explain.rs`**: `assay explain` command
- **`coverage.rs`**: `assay coverage` command
- **`baseline.rs`**: `assay baseline` command
- **`ci.rs`**: `assay ci` command
- **`init_ci.rs`**: `assay init-ci` command
- **`mcp.rs`**: `assay mcp` command
- **`monitor.rs`**: `assay monitor` command
- **`sandbox.rs`**: `assay sandbox` command
- **`discover.rs`**: `assay discover` command
- **`kill.rs`**: `assay kill` command
- **`quarantine.rs`**: `assay quarantine` command
- **`calibrate.rs`**: `assay calibrate` command
- **`profile.rs`**: `assay profile` command
- **`evidence/mod.rs`**: `assay evidence` command with subcommands:
  - **`evidence/lint.rs`**: `assay evidence lint` - SARIF output, rule registry
  - **`evidence/diff.rs`**: `assay evidence diff` - Semantic bundle comparison
  - **`evidence/explore.rs`**: `assay evidence explore` - TUI viewer (feature-gated)
  - **`evidence/mapping.rs`**: Profile to EvidenceEvent mapping
  - **`evidence/push.rs`**: `assay evidence push` - Upload to BYOS storage
  - **`evidence/pull.rs`**: `assay evidence pull` - Download from BYOS storage
  - **`evidence/list.rs`**: `assay evidence list` - List bundles in storage
- **`tool/mod.rs`**: `assay tool` command with subcommands:
  - **`tool/keygen.rs`**: `assay tool keygen` - Generate ed25519 keypair
  - **`tool/sign.rs`**: `assay tool sign` - Sign tool definition
  - **`tool/verify.rs`**: `assay tool verify` - Verify signature
- **`demo.rs`**: `assay demo` command
- **`fix.rs`**: `assay fix` command (agentic policy fixing)
- **`sim.rs`**: `assay sim` command
- **`setup.rs`**: `assay setup` command
- **`policy.rs`**: `assay policy` command

### CLI Args (`src/cli/args.rs`)
- `Cli` struct: Top-level CLI structure
- `Command` enum: All subcommands
- Argument structs for each command

### Backend (`src/backend.rs`)
- Backend configuration and setup

## Metrics Crate (`assay-metrics`)

### Entry Point
- **`src/lib.rs`**:
  - `default_metrics()`: Factory function
  - Metric implementations

### Metric Implementations (`src/`)
- **`must_contain.rs`**: `MustContain` metric
- **`must_not_contain.rs`**: `MustNotContain` metric
- **`regex_match.rs`**: `RegexMatch` metric
- **`json_schema.rs`**: `JsonSchema` metric
- **`semantic.rs`**: `SemanticSimilarity`, `Faithfulness`, `Relevance` metrics
- **`args_valid.rs`**: `ArgsValid` metric
- **`sequence_valid.rs`**: `SequenceValid` metric
- **`tool_blocklist.rs`**: `ToolBlocklist` metric
- **`usage.rs`**: `Usage` metric

## MCP Server Crate (`assay-mcp-server`)

### Entry Point
- **`src/main.rs`**: MCP server binary entry point

### Server Implementation (`src/`)
- JSON-RPC server over stdio
- Policy enforcement proxy
- Tool call auditing

## Monitor Crate (`assay-monitor`)

### Entry Point
- **`src/lib.rs`**: Monitor library exports

### Implementation (`src/`)
- eBPF program loading
- Event stream handling
- Tier 1 policy enforcement

## Policy Crate (`assay-policy`)

### Entry Point
- **`src/lib.rs`**: Policy compilation exports

### Implementation (`src/`)
- Policy parsing
- Tier 1/2 compilation
- `CompiledPolicy` generation

## Python SDK (`assay-python-sdk`)

### Rust Bindings (`src/lib.rs`)
- PyO3 bindings to `assay-core`
- Python module exports

### Python Module (`python/assay/`)
- **`__init__.py`**: Module initialization, `validate()` function
- **`client.py`**: `AssayClient` class
- **`coverage.py`**: `Coverage` class
- **`explain.py`**: `Explainer` class
- **`pytest_plugin.py`**: Pytest integration
- **`_native.pyi`**: Type stubs for native bindings

## Configuration Files

### Workspace Config (`Cargo.toml`)
- Workspace members
- Shared dependencies
- Version management

### Crate Configs (`crates/*/Cargo.toml`)
- Crate-specific dependencies
- Feature flags
- Build configuration

## Documentation

### User Documentation (`docs/`)
- **`getting-started/`**: Installation, quickstart, first test
- **`concepts/`**: Core concepts (traces, policies, metrics, replay)
- **`guides/`**: User guides and tutorials
- **`reference/`**: CLI reference, config reference
- **`use-cases/`**: Use case examples
- **`architecture/`**: Architecture documentation and ADRs
- **`mcp/`**: MCP integration documentation
- **`python-sdk/`**: Python SDK documentation

### AI Context (`docs/AIcontext/`)
- This directory: AI-focused documentation
- Codebase overview, user flows, interdependencies, etc.

## Test Files

### Integration Tests (`tests/`)
- **`e2e/`**: End-to-end CLI tests
- **`fixtures/`**: Test fixtures and golden files
- **`integration/`**: Integration tests
- **`security_audit/`**: Security tests
- **`mcp_*.sh`**: MCP integration tests

### Unit Tests (`crates/*/tests/`)
- Crate-specific unit tests
- Golden file tests
- Smoke tests

## CI/CD

### GitHub Workflows (`.github/workflows/`)
- **`ci.yml`**: Main CI pipeline
- **`parity.yml`**: Parity tests (batch vs streaming)
- **`assay-security.yml`**: Security policy validation
- **`kernel-matrix.yml`**: Kernel version matrix tests
- **`release.yml`**: Release workflow
- **`docs.yml`**: Documentation deployment
- **`action-v2-test.yml`**: GitHub Action v2 tests

### GitHub Action (Separate Repo)
- **Repository:** https://github.com/Rul1an/assay/tree/main/assay-action
- **Marketplace:** https://github.com/marketplace/actions/assay-ai-agent-security
- **Usage:** `Rul1an/assay/assay-action@v2`

## Key Data Structures

### `EvalConfig` (`assay-core/src/model.rs`)
- Complete evaluation configuration
- Suite name, tests, model config, settings

### `TestCase` (`assay-core/src/model.rs`)
- Individual test case definition
- Test ID, prompt, expected, metrics

### `TestResultRow` (`assay-core/src/model.rs`)
- Test execution result
- Status, score, details, fingerprint

### `RunArtifacts` (`assay-core/src/report/mod.rs`)
- Complete run results
- Run ID, suite, results list

### `Policy` (`assay-core/src/policy_engine.rs`)
- Parsed policy structure
- Tool constraints, sequences, blocklists

### `CompiledPolicy` (`assay-policy/src/`)
- Compiled policy with Tier 1/2 split
- Ready for runtime enforcement

## Important Constants

### Exit Codes (`assay-cli/src/exit_codes.rs`, `commands/mod.rs`)
- `EXIT_SUCCESS = 0`: Success
- `EXIT_TEST_FAILURE = 1`: Test failure; **E_JUDGE_UNCERTAIN** when judge abstains (PR #159)
- `EXIT_CONFIG_ERROR = 2`: Configuration error
- `EXIT_INFRA_ERROR = 3`: Judge unavailable, rate limit, timeout (E_JUDGE_UNAVAILABLE)
- `EXIT_WOULD_BLOCK = 4`: Sandbox/policy would block execution

### Error Codes (`assay-core/src/errors/diagnostic.rs`)
- Diagnostic error codes for user-friendly messages

## File Naming Conventions

- **Modules**: `mod.rs` or `{name}.rs`
- **Tests**: `{name}_test.rs` or in `tests/` directory
- **Examples**: `examples/{name}.rs`
- **Configs**: `{name}.yaml` or `{name}.toml`
- **Traces**: `{name}.jsonl`

## Module Organization Principles

1. **Separation of Concerns**: Each module has a single responsibility
2. **Trait-Based Design**: Interfaces defined via traits (Metric, LlmClient, Embedder)
3. **Workspace Structure**: Related functionality grouped in crates
4. **Feature Flags**: Optional functionality behind feature gates
5. **Platform-Specific**: Linux-only code in `#[cfg(target_os = "linux")]` blocks

## Related Documentation

- [Codebase Overview](codebase-overview.md) - High-level architecture
- [Interdependencies](interdependencies.md) - How files/modules connect
- [Entry Points](entry-points.md) - Where to start when adding features
