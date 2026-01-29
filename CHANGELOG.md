# Changelog

All notable changes to this project will be documented in this file.

## [v2.12.0] - 2026-01-29

### 🔐 Pack Registry: Enterprise-Grade Supply Chain Security

This release introduces the **Pack Registry Client** (`assay-registry` crate) - a complete implementation of SPEC-Pack-Registry-v1.0.3 for secure remote pack distribution.

### ✨ Major Features

-   **Pack Registry Client** (`crates/assay-registry/`):
    -   HTTP client with token + OIDC authentication
    -   Pack resolution: local → bundled → registry → BYOS
    -   Local caching with TOCTOU protection (integrity verified on every read)
    -   Lockfile v2 for reproducible builds (`assay.packs.lock`)

-   **JCS Canonicalization (RFC 8785)**:
    -   Deterministic JSON serialization for pack digests
    -   Uses `serde_jcs::to_vec()` (bytes, not string) to eliminate encoding issues
    -   Canonical digest format: `sha256:{hex}`

-   **Strict YAML Validation (SPEC §6.1)**:
    -   Pre-scan rejects anchors (`&`), aliases (`*`), tags (`!!`), multi-document (`---`)
    -   Duplicate key detection with correct list-item scoping
    -   DoS limits: max depth 50, keys 10k, string 1MB, input 10MB
    -   Integer range checks: ±2^53 (IEEE 754 safe integer)

-   **DSSE Signature Verification**:
    -   Ed25519 + PAE encoding per DSSE spec
    -   Sidecar endpoint (`GET /packs/{name}/{version}.sig`) for large signatures
    -   Client always prefers sidecar over `X-Pack-Signature` header

-   **Trust Model (No-TOFU)**:
    -   Pinned root keys compiled into binary
    -   Key rotation via signed manifest
    -   Pinned roots survive remote revocation attempts
    -   Runtime expiry checks for manifest keys

### 🧪 GitHub Action v2.1 Test Coverage

-   Contract tests for all v2.1 features:
    -   Pack lint with `eu-ai-act-baseline` + SARIF validation
    -   Fork PR SARIF skip logic
    -   OIDC provider auto-detection (AWS/GCP/Azure patterns)
    -   Attestation gating (push-only, default branch, verified)
    -   Coverage calculation formula

### 🐛 Security Fixes (P0)

-   **Duplicate Key Detection**: Pre-scan catches block mapping duplicates; serde_yaml catches flow mapping duplicates
-   **DSSE Verification**: Signature verification uses canonical JCS bytes (not raw YAML)
-   **List-Item Scoping**: Each list item gets its own scope (fixes false positives for `- a: 1\n- a: 2`)

### 📦 New Crate Published

-   `assay-registry` v2.11.0 on [crates.io](https://crates.io/crates/assay-registry)

### 📚 Documentation

-   `docs/architecture/SPEC-Pack-Registry-v1.md` updated to v1.0.3
-   `docs/architecture/ADR-018-GitHub-Action-v2.1.md` - Action v2.1 design
-   `docs/architecture/SPEC-GitHub-Action-v2.1.md` - Action v2.1 specification
-   Security review documentation in `crates/assay-registry/docs/`

### Test Coverage

-   185 tests in `assay-registry` crate
-   Golden vectors for JCS digest verification
-   DSSE real signature verification tests
-   Trust rotation and revocation tests
-   Cache tamper detection tests
-   Protocol edge cases (304/410/429)

---

## [v2.10.0] - 2026-01-28

### 🎯 Pack Engine: Compliance Rule Packs

This release introduces the **Pack Engine** - a YAML-driven compliance/security/quality rule system for evidence bundle linting, with the first built-in pack for EU AI Act Article 12.

### ✨ Major Features

-   **Pack Engine** (`crates/assay-evidence/src/lint/packs/`):
    -   YAML-defined rule packs with typed checks
    -   Check types: `event_count`, `event_pairs`, `event_field_present`, `event_type_exists`, `manifest_field`
    -   JSON Pointer (RFC 6901) for field addressing
    -   JCS canonicalization (RFC 8785) for deterministic pack digests
    -   Collision policy: compliance packs hard-fail, security/quality last-wins

-   **EU AI Act Baseline Pack** (`packs/eu-ai-act-baseline.yaml`):
    -   `EU12-001`: Event recording (Article 12(1))
    -   `EU12-002`: Operation monitoring - started/finished pairs (Article 12(2)(c))
    -   `EU12-003`: Post-market monitoring - correlation IDs (Article 12(2)(b))
    -   `EU12-004`: Risk identification - policy/denial fields (Article 12(2)(a))

-   **CLI Integration**:
    -   `--pack`: Comma-separated pack references (built-in or file path)
    -   `--max-results`: Limit findings for GitHub SARIF size limits (default: 500)

-   **GitHub Code Scanning Compatible SARIF**:
    -   `locations[]` on all results (including global findings)
    -   `primaryLocationLineHash` for GitHub deduplication
    -   Pack metadata in `tool.driver.properties.assayPacks[]`
    -   `run.properties.disclaimer` for compliance packs
    -   Truncation policy with `run.properties.truncated/truncatedCount`

### 📚 Documentation

-   `docs/architecture/SPEC-Pack-Engine-v1.md` - Complete implementation spec
-   `docs/architecture/ADR-013-EU-AI-Act-Pack.md` - EU AI Act pack design
-   `docs/architecture/ADR-016-Pack-Taxonomy.md` - Pack taxonomy and open core model

### Usage

```bash
# Run EU AI Act baseline checks
assay evidence lint bundle.tar.gz --pack eu-ai-act-baseline

# SARIF output for GitHub Code Scanning
assay evidence lint bundle.tar.gz --pack eu-ai-act-baseline --format sarif

# Custom pack file
assay evidence lint bundle.tar.gz --pack ./my-pack.yaml
```

## [v2.4.0] - 2026-01-26

### 🛡️ Phase 5: SOTA Sandbox Hardening

This release delivers **State-of-the-Art** sandbox hardening, addressing MCP security guidance for credential isolation, honest capability reporting, and fork-safe enforcement.

### ✨ Major Features

-   **Environment Scrubbing** (`env_filter.rs`):
    -   Default-deny for secrets (`*_TOKEN`, `*_KEY`, `*_SECRET`, `AWS_*`, `GITHUB_*`)
    -   CLI flags: `--env-allow=VAR=value`, `--env-passthrough=VAR`
    -   Always sets `TMPDIR` to scoped sandbox directory
-   **Landlock Deny-wins Correctness** (`landlock_check.rs`):
    -   Detects "deny inside allow" conflicts that Landlock cannot enforce
    -   Automatic degradation to Audit mode with explicit warning
    -   Prevents false sense of security from unenforceable policies
-   **Fork-Safe pre_exec**:
    -   Eliminated heap allocations in `pre_exec` closure
    -   Uses `std::io::Error::from_raw_os_error()` instead of `anyhow::bail!()`
    -   Syscall-only in critical fork-exec window
-   **Scoped /tmp Isolation**:
    -   UID-based (not `$USER` env which can be spoofed)
    -   Per-run isolation via PID in path
    -   0700 permissions (owner-only)
    -   Prefers `XDG_RUNTIME_DIR` when available
-   **Doctor Deep Dive v2**:
    -   Reports Phase 5 hardening feature status
    -   Reads actual Landlock ABI version from sysfs
    -   Net enforcement correctly reports ABI >= 4 requirement

### 🛠️ CI Improvements

-   **`scripts/ci/phase5-check.sh`**: New quality gate script
    -   `CARGO_TARGET_DIR=/tmp/assay-target` for VM mount compatibility
    -   `--locked` on all cargo commands
    -   Strict Clippy `-D warnings`

### 🐛 Fixes

-   Fixed `unused_assignments` warning on macOS via `#[cfg(target_os = "linux")]`
-   Fixed `io_other_error` Clippy lint (Rust 1.93)
-   Added `#[allow(dead_code)]` for non-Linux Landlock stubs

## [v2.2.0] - 2026-01-23

### 🛡️ SOTA Hardening (Jan 2026)

This release delivers "State-of-the-Art" infrastructure hardening, specifically targeting ARM/Self-Hosted stability and CI reliability. It eliminates supply chain risks and ensures deterministic builds across all platforms.

### ✨ Major Features
-   **Robust ARM Infrastructure**: Implemented a "GoFoss -> Ubuntu Ports" failover loop for all ARM runners. This eliminates flaky `404` errors caused by the unstable `ports.ubuntu.com` mirror.
    -   **Generic Logic**: The failover script aggressively rewrites *any* `ubuntu-ports` source, scrubbing legacy/broken mirrors (e.g. `edge.kernel.org`) from self-hosted runners.
    -   **Optimization**: Automatically skips logic on AMD64 runners (`ubuntu-latest`) to preserve "Fast Path" performance.
-   **Intelligent Gating**:
    -   **Fork Safety**: Self-hosted runners are now strictly gated (`if: fork == false`) to prevent malicious code execution from PR forks.
    -   **Split Smoke**: `ebpf-smoke` is split into `-ubuntu` (for signal) and `-self-hosted` (for depth), ensuring forks still get CI feedback.
-   **Performance "Fast Path"**:
    -   **Install-First**: All apt jobs now attempt `install` before `update`, leveraging fresh runner caches for significant speedups.
    -   **Hardened Flags**: Ubiquitous use of `DEBIAN_FRONTEND=noninteractive` and `--no-install-recommends`.

### 🐛 Fixes
-   **Artifact Sequencing**: Fixed a race condition in `kernel-matrix.yml` (`matrix-test`) where install scripts ran before artifact download.
-   **Supply Chain**: Enforced `--locked` / pinned versions for all `bpf-linker` installations.
-   **Cleanup**: Removed legacy `actions/cache` usage for apt-lists (native disk caching is superior on self-hosted).

## [v2.1.1] - 2026-01-15

### 🛡️ LSM Hardening & Safety

Critical release hardening the BPF-LSM implementation for production readiness.

-   **Verifier Fix**: Resolved BPF verifier rejection (exit code 40) by optimizing `emit_event` (removed zeroing loop).
-   **RingBuf Safety**: Implemented secure, full-buffer copy to prevent uninitialized memory leakage to userspace.
-   **Explicit Deny**: Validated E2E `action: "deny"` enforcement (EPERM blocking).
-   **CI Gate**: Hardened `verify_lsm_docker.sh` to enforce hard failures on blocking misses.

## [v2.0.0] - 2026-01-12

### 🛡️ SOTA Hardening (Phase 5)

This major release delivers the **State-of-the-Art (SOTA)** architecture for robust runtime security, transitioning from "Best Effort" to "Forensically Sound" monitoring.

### ✨ Major Features
-   **Cgroup-First Architecture**: `assay-monitor` and `assay-ebpf` now prioritize cgroup membership over PID tracking, using `bpf_get_current_ancestor_cgroup_id` to prevent nested cgroup escapes. This ensures 100% coverage of short-lived processes.
-   **Forensic Incident Bundles**:
    -   **Secure Atomic Writes**: Implementation of `IncidentBuilder` using `openat`, `O_NOFOLLOW`, `O_EXCL`, and `renameat` to prevent TOCTOU vulnerabilities.
    -   **Unique Identity**: Incident files now use UUID v4 suffixes to guarantee uniqueness.
    -   **Detailed Metadata**: Includes kernel version, session UUID, and process tree context.
-   **eBPF Hardening**:
    -   **Dynamic Offsets**: Removed all hardcoded kernel offsets in favor of runtime resolution via `/sys/kernel/tracing/events/.../format`.
    -   **Extended Coverage**: Added `sys_enter_openat2` probe for modern kernels (Linux 5.6+).
    -   **Safety**: Uses `read_user_str_bytes` with explicit bounds checking safe slices.

### 🐛 Fixes & Polish
-   **CI Reliability**: Complete overhaul of CI pipelines using `sccache` (local backend), `mold` linker (Linux), and single-pass testing. Zero 400 errors from GH Actions Cache.
-   **Windows Support**: Fixed compilation issues in `assay-cli` by guarding Unix-specific cgroup logic.
-   **Golden Tests**: Resolved output mismatches for strict reproducibility.

## [v1.8.0] - 2026-01-11

### 🚀 Runtime Features (System 2 Security)

This release transforms Assay from a static analyzer into a complete **Runtime Security Platform**. It introduces the "System 2" capabilities: detecting and stopping dangerous behavior as it happens.

### ✨ Major Features
-   **Runtime Monitor (`assay monitor`)** *(Linux Only)*:
    -   Uses **eBPF** (extended Berkeley Packet Filter) to trace process behavior safely in kernel space.
    -   Detects file access (`openat`) and network connections (`connect`) in real-time.
    -   **Zero-Overhead**: Highly optimized "Read-First" ring buffer implementation.
-   **Discovery (`assay discover`)**:
    -   Automatically inventory running MCP servers and local configurations.
    -   Detects unmanaged servers and security gaps.
-   **Kill Switch (`assay kill`)**:
    -   Emergency termination of rogue agent processes.
    -   Supports graceful shutdown (SIGTERM) and immediate kill (SIGKILL).

### 🛡️ Hardening
-   **Native eBPF Builds**: CI now builds eBPF artifacts natively (no Docker required), ensuring determinism and stability.
-   **Host Build Protection**: The `assay-ebpf` crate is feature-gated to prevent accidental linking on non-Linux hosts.
-   **Strict Dependencies**: All upstream dependencies are strictly pinned for reproducibility.

### 📚 Documentation
-   **Unified Reference**: Consolidated runtime documentation into `docs/runtime-monitor.md`.
-   **Handoff**: Comprehensive architecture & maintenance guide available for contributors.

## [v1.7.0] - 2026-01-09

### 🛡️ Strict Deprecation Mode
- **Refined Deprecations**: Formal deprecation of v1.x constraints syntax.
- **Strict Mode**: New `--deny-deprecations` flag (and `ASSAY_STRICT_DEPRECATIONS=1` env var) to enforce strict compliance in CI.
- **Migration Guide**: New detailed [v1-to-v2 Migration Guide](docs/migration/v1-to-v2.md).
- **Startup Warnings**: Server/Proxy now emit clear warnings when loading legacy policies.

### Added
- **CLI**: `assay policy validate --deny-deprecations` (and for `run`/`wrap` modes).
- **Docs**: Comprehensive `docs/migration/v1-to-v2.md`.

## [v1.6.0] - 2026-01-09

### Added
- **Policy v2.0 (JSON Schema)**: Official support for JSON Schema constraints (`schemas:`) replacing regex loops.
- **Unified Policy Engine**: `assay-core`, `assay-cli`, and `assay-mcp-server` now share the exact same evaluation logic (`McpPolicy::evaluate`).
- **New Commands**: `assay policy validate`, `migrate`, and `fmt`.
- **Enforcement Modes**: `enforcement.unconstrained_tools: warn|deny|allow` for finer control over headless/legacy tools.
- **Scoped Refs**: `$ref` support within single policy documents (`#/schemas/$defs/...`).

### Changed
- **Runtime Consistency**: `assay mcp wrap` (proxy) and `assay-mcp-server` enforce the exact same rules as `assay coverage`.
- **Auto-Migration**: Legacy v1 policies (`constraints:`) are auto-migrated in-memory with deprecation warnings.

### Deprecated
- **v1 Constraints**: The `constraints:` syntax is deprecated and will be removed in Assay v2.0.0. Use `assay policy migrate` to upgrade.

### Fixed
- **JSON Casing**: Stabilized `structuredContent` vs `structured_content` in error contracts.
- **Symlink Resolution**: Fixed policy resolution issues on macOS `/tmp`.



### 🛠️ Autofix & Policy Packs
A major productivity release introducing automated self-repair (`assay fix`) and instant policy scaffolding (`assay init --pack`).

### ✨ Major Features
-   **`assay fix`**: Interactively repair configuration issues.
    -   **Automated Patches**: Fixes config errors, schema violations, and missing policies based on diagnostics.
    -   **Dry Run**: Preview changes before applying them.
    -   **Atomic Writes**: Cross-platform safe file updates (Windows/Linux/macOS).
-   **Policy Packs (`assay init --pack`)**:
    -   `default`: Balanced security (blocks RCE, audits sensitive ops).
    -   `hardened`: Maximum security (allowlist-only, strict args).
    -   `dev`: Permissive for rapid prototyping (logs warnings).

### 🛡️ Hardening
-   **Patch Engine**: Strict traversal prevents partial mutations during `remove`/`replace` operations.
-   **Module Cleanup**: Extracted shared logic to `assay-cli::util` for better maintainability.
-   **Windows Support**: Robust atomic file replacement strategy.

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

-   **Friendly Hints**: When unknown fields are detected (e.g. `require_args`), Doctor now suggests the closest valid field ("Did you mean `require_args`?").
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
