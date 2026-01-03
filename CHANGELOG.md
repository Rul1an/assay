# Changelog

All notable changes to this project will be documented in this file.



## [v1.2.0] - 2026-01-04

### 🐍 Python SDK (`assay-python-sdk`)
Native Python bindings for seamless integration into Pytest and other Python workflows.

-   **`AssayClient`**: Record traces directly from python code using `client.record_trace(obj)`.
-   **`Coverage`**: Analyze trace coverage with `assay.Coverage(policy_path).analyze(traces)`.
-   **`Explainer`**: Generate human-readable explanations of tool usage vs policy.
-   **Performance**: Built on `PyO3` + `maturin` for high-performance Rust bindings.

### 🛡️ Coverage Thresholds & Gates (`assay coverage`)
New `assay coverage` command to enforce quality gates in CI.

-   **Min Coverage**: Fail build if coverage drops below threshold (`--min-coverage 80`).
-   **Baseline Regressions**: Compare against a baseline and fail on regression (`--baseline base.json`).
-   **High Risk Gaps**: Detect and fail if critical `deny`-listed tools are never exercised.
-   **Export**: Save baselines with `--export-baseline`.

### 📉 Baseline Foundation (`assay baseline`)
Manage and track baselines to detect behavioral shifts.

-   `assay baseline record`: Capture current run metrics.
-   `assay baseline check`: Diff current run against stored baseline.
-   **Determinism**: Guaranteed deterministic output for reliable regression testing.

### Added
-   **`assay-python-sdk`** package on PyPI (upcoming).
-   `TraceExplainer` logic exposed to Python.

## [v1.1.0] - 2026-01-02

### Added

#### Policy DSL v2 - Temporal Constraints

New sequence operators for complex agent workflow validation:

- **`max_calls`** - Rate limiting per tool
  ```yaml
  sequences:
    - type: max_calls
      tool: FetchURL
      max: 10  # Deny on 11th call
  ```

- **`after`** - Post-condition enforcement
  ```yaml
  sequences:
    - type: after
      trigger: ModifyData
      then: AuditLog
      within: 3  # AuditLog must appear within 3 calls after ModifyData
  ```

- **`never_after`** - Forbidden sequences
  ```yaml
  sequences:
    - type: never_after
      trigger: Logout
      forbidden: AccessData  # Once logged out, cannot access data
  ```

- **`sequence`** - Exact ordering with strict mode
  ```yaml
  sequences:
    - type: sequence
      tools: [Authenticate, Authorize, Execute]
      strict: true  # Must be consecutive, no intervening calls
  ```

#### Aliases

Define tool groups for cleaner policies:

```yaml
aliases:
  Search:
    - SearchKnowledgeBase
    - SearchWeb
    - SearchDatabase

sequences:
  - type: eventually
    tool: Search  # Matches any alias member
    within: 5
```

#### Coverage Metrics

New `assay coverage` command for CI/CD integration:

```bash
# Check tool and rule coverage
assay coverage --policy policy.yaml --traces traces.jsonl --min-coverage 80

# Output formats: summary, json, markdown, github
assay coverage --policy policy.yaml --traces traces.jsonl --format github
```

Features:
- Tool coverage: which policy tools were exercised
- Rule coverage: which rules were triggered
- High-risk gaps: blocklisted tools never tested
- Unexpected tools: tools in traces but not in policy
- Exit codes: 0 (pass), 1 (fail), 2 (error)
- GitHub Actions annotations for PR feedback

#### GitHub Action

```yaml
- uses: assay-dev/assay-action@v1
  with:
    policy: policies/agent.yaml
    traces: traces/
    min-coverage: 80
```

#### One-liner Installation

```bash
curl -sSL https://assay.dev/install.sh | sh
```

### Changed

- Policy version bumped to `1.1`
- Improved error messages with actionable hints
- Better alias resolution performance

### Experimental

The following features are available but not yet stable:

- `assay explain` - Trace debugging and visualization (use `--experimental` flag)

### Migration from v1.0

v1.1 is fully backwards compatible with v1.0 policies. To use new features:

1. Update `version: "1.0"` to `version: "1.1"` in your policy files
2. Add `aliases` section if using tool groups
3. Add new sequence rules as needed

Existing v1.0 policies will continue to work without modification.

## [v1.0.0] - 2025-12-29
### Added
-   **Structured Logging**: `assay-core` now uses `tracing` for fail-safe events (`assay.failsafe.triggered`), enabling direct Datadog/OTLP integration.
-   **Protocol Feedback**: `assay-mcp-server` now includes a `warning` field in the response when `on_error: allow` is active and an error occurs, allowing clients to adapt logic.
-   **Documentation**: Added "Look-behind Workarounds" to `docs/guides/migration-regex.md`.

## [v1.0.0-rc.2] - 2025-12-28

### 🚀 Release Candidate 2
Rapid-response release addressing critical Design Partner feedback regarding MCP protocol compliance and operational visibility.

### ✨ Features
- **Structured Fail-Safe Logging**: Introduced `assay.failsafe.triggered` JSON event when `on_error: allow` is active, enabling machine-readable audit trails.
- **Fail-Safe UX**: Logging now occurs via standard `stderr` to avoid polluting piping outputs.

### 🐛 Fixes
- **MCP Compliance**: `assay-mcp-server` tool results are now wrapped in standard `CallToolResult` structure (`{ content: [...], isError: bool }`), enabling clients to parse error details and agents to self-correct.


### 🚀 Release Candidate 1
First Release Candidate for Assay v1.0.0, introducing the "One Engine, Two Modes" guarantee and unified policy enforcement.

### ✨ Features
- **Unified Policy Engine**: Centralized validation logic (`assay-core::policy_engine`) shared between CLI, SDK, and MCP Server.
- **Fail-Safe Configuration**: New `on_error: block | allow` settings for graceful degradation.
- **Parity Test Suite**: New `tests/parity_batch_streaming.rs` ensuring identical behavior between batch and streaming modes.
- **False Positive Suite**: `tests/fp_suite.yaml` validation for legitimate business flows.
- **Latency Benchmarks**: confirmed core decision latency <0.1ms (p95).

### 🐛 Fixes
- Resolved schema validation discrepancies between local CLI and MCP calls.
- Fixed `sequence_valid` assertions to support regex-based policy matching.

## [v0.9.0] - 2025-12-27

### 🚀 Hardened & Release Ready

This release marks the transition to a hardened, production-grade CLI. It introduces strict contract guarantees, robust migration checks, and full CI support.

### ✨ Features
- **Official CI Template**: `.github/workflows/assay.yml` for drop-in GitHub Actions support.
- **Assay Check**: New `assay migrate --check` command to guard against unmigrated configs in CI.
- **CLI Contract**: Formalized exit codes:
  - `0`: Success / Clean
  - `1`: Test Failure
  - `2`: Configuration / Migration Error
- **Soak Tested**: Validated with >50 consecutive runs for 0-flake guarantee.
- **Strict Mode Config**: `configVersion: 1` removes top-level `policies` in favor of inline declarations.

### ⚠️ Breaking Changes
- **Configuration**: Top-level `policies` field is no longer supported in `configVersion: 1`. You must run `assay migrate` to update your config.
- **Fail-Fast**: `assay migrate` and `validate` now fail hard (Exit 2) on unknown standard fields.

### 🐛 Fixes
- Fixed "Silent Drop" issue where unknown YAML fields were ignored during parsing.
- Resolved argument expansion bug in test scripts on generic shells.

## [v0.8.0] - 2025-12-27
### Added
- Soak test hardening for legacy configs
- Unit tests for backward compatibility
- `EvalConfig::validate()` method

### Changed
- Prepared `configVersion: 1` logic (opt-in)
