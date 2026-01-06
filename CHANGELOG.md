# Changelog

All notable changes to this project will be documented in this file.

## [v1.4.1] - 2026-01-06

### 🩹 Consistency & SARIF Polish
Post-release hardening for Agentic Contract and SARIF compliance.

### 🛠️ Fixes
-   **Contract Consistency**: Internal severity normalization (`warning` -> `warn`) now applied strictly to exit code logic and CLI text output logic.
-   **SARIF**: `invocations.exitCode` now accurately reflects the CLI exit code (0/1/2).
-   **Contract**: Text output summary counts now strictly match JSON output counts.



## [v1.4.0] - 2026-01-06

### 🛡️ Agentic Security Edition
The "CI Gate" release. This major update transforms Assay into a comprehensive CI/CD guardrail for Agentic systems.

### ✨ Major Features
-   **`assay init`**: Interactive wizard that auto-detects your project type (Python/Node/MCP) and generates secure policy + CI config in < 5s.
-   **`assay validate`**: Dedicated CI command with strict exit codes (0=Pass, 1=Fail, 2=Error) and zero overhead.
-   **Agentic Contract**: `--format json` output is now strictly typed, stable, and designed for AI self-correction loops.
-   **GitHub Advanced Security**: `--format sarif` support for direct integration with GitHub Code Scanning.

### 📚 Documentation
-   **Overhaul**: Complete rewrite of `Quickstart`, `CLI Reference`, and `Architecture` guides.
-   **GetAssay.dev**: One-line install script and landing page sync.

## [v1.3.0] - 2026-01-06

### ✨ New Feature: `assay mcp config-path`
Simplified 1-step setup for Claude Desktop, Cursor, and other MCP clients.
-   **Auto-detection**: Automatically finds config files on macOS, Windows, and Linux.
-   **Generation**: Generates secure JSON snippets for your `mcpServers` config.
-   **Security**: Enforces policy file usage by default.

### 🛡️ Security Hardening
-   **Fail-Secure**: CLI now fatal-errors if specified policy file is missing (no insecure fallbacks).
-   **Policy**: clarifications on rate limit fields.
-   **Proxy**: Improved logging for unknown tool calls.

### 🐛 CI Fixes
-   **Python Wheels**: Fixed extensive artifact corruption issue in Release workflow (`release.yml`).
-   **Linting**: Strict `clippy` and `rustfmt` compliance across the board.

## [v1.2.12] - 2026-01-05

### 🩹 Fix
-   **README**: Fixed broken CI status badge (pointed to non-existent `assay.yml`).

## [v1.2.11] - 2026-01-05

### 📖 Docs Pages Update
-   **Index**: Aligned landing page with new "Vibecoder + Senior" positioning.
-   **User Guide**: Rewritten to focus on CI/CD, Doctor, and Python workflows (removed legacy RAG metrics noise).
-   **Consistency**: Unified messaging across README and documentation site.

## [v1.2.10] - 2026-01-05

### 📖 Documentation Refresh
-   **README**: Overhauled for "Vibecoder + Senior" audience.
-   **Guides**: Updated Python Quickstart and Identity docs.
-   **Consistency**: `assay-it` is now the canonical package name in docs.

## [v1.2.9] - 2026-01-05

### 🧹 Code Sweep
-   Removed redundant directories (`test-*/`, `assay-doctor-*`).
-   Refactored `doctor` module to remove verbose comments.
-   Zero fluff policy applied.

## [v1.2.8] - 2026-01-05

### 📚 SOTA DX Features
-   **Python Docs**: Added comprehensive docstrings to `assay.Coverage`, `assay.validate`, and `AssayClient` wrappers. IDEs will now show rich tooltips. (Google-style)
-   **Stability**: Added CLI verification tests for `assay init-ci`.

## [v1.2.7] - 2026-01-05

### 🩹 Formatting Fix
Patch release to verify `cargo fmt` compliance after `v1.2.6` refactoring.

## [v1.2.6] - 2026-01-05

### 🩹 Clippy Fix
Patch release to fix a stable-clippy lint `regex_creation_in_loops`.
-   **Performance**: Regex is now compiled once per doctor suite, not per policy.

## [v1.2.5] - 2026-01-05

### 📦 PyPI Metadata Fix (Real)
Updated `pyproject.toml` to explicitly use `assay-it` as the package name, ensuring `maturin` builds the correct wheel metadata for PyPI.
-   **Distribution Name**: `assay-it` (Final Fix)

## [v1.2.4] - 2026-01-05

### 📦 PyPI Package Rename
Renamed the Python SDK distribution package to `assay-it` to match the PyPI project name.
-   **Distribution Name**: `assay-it` (PyPI)
-   **Import Name**: `import assay` (Unchanged)

## [v1.2.3] - 2026-01-05

### 🩹 CI Stabilization
Patch release to resolve build pipeline issues.

-   **Fix**: Resolved artifact corruption in wheel generation (PyPI Release).
-   **Fix**: Corrected formatting in `doctor/mod.rs` to pass strict CI linting.

## [v1.2.2] - 2026-01-05

### 💅 Polish & Fixes
Strictness doesn't have to be unfriendly. This release polishes the "Strict Schema" experience.

-   **Friendly Hints**: When unknown fields are detected (e.g. `requre_args`), Doctor now suggests the closest valid field ("Did you mean `require_args`?").
-   **Output**: `assay doctor` now correctly displays diagnostic messages in human-readable output (previously they were counted but hidden).
-   **Release Fix**: Removed legacy workflows to ensure smooth PyPI publishing.


## [v1.2.1-ext] - 2026-01-05

### 🩺 Smart Doctor (SOTA Agentic Edition)
Transformed `assay doctor` into a "System 2" diagnostic engine for Agentic workflows.

-   **Analyzers**:
    -   **Trace Drift**: Detects legacy `function_call` usage (recommends `tool_calls`).
    -   **Integrity**: Validates existence of all referenced policy/config files.
    -   **Logic**: Detects alias shadowing (e.g. `Search` alias hiding `Search` tool).
-   **Agentic Contract**:
    -   Output via `--format json` is strict, machine-readable, and deterministic.
    -   Includes `fix_steps` for automated self-repair.
    -   **Robust JSON Errors**: Even config parsing failures return valid JSON envelopes (when requested), ensuring Agents never crash on plain text errors.

### ⚠️ Breaking Changes (Strict Schema)
To prevent "Silent Failures" (phantom configs), we now enforce **Strict Schema Validation**:
-   **Unknown fields in `assay.yaml` or `policy.yaml` now cause a HARD ERROR.**
-   Previously, typos or incorrect nesting (e.g. `tools: ToolName:`) were silently ignored. Now you will see `E_CFG_PARSE` with "unknown field".
-   *Why*: Required for reliable Agentic generation and debugging.

### 🐛 Fixes
-   **Demo**: `assay demo` now generates canonical, schema-compliant policies.
-   **DX**: Restored `request_id` uniqueness check in trace client.

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
