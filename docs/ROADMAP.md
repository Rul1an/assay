# Assay Roadmap 2026

**Strategic Focus:** Agent Runtime Evidence & Control Plane.
**Moat:** Verifiable Evidence (Open Standard) + Governance Platform (Paid Service).

---

## 🛑 Current Priority: Phase 10 (Evidence Contract) - Mostly Complete

We have largely completed the **Evidence Contract v1** to establish Assay as the standard for agent auditability.

*   **Goal**: Replace "logging" with "verifiable evidence" (CloudEvents + OTel + Deterministic IDs).
*   **Status**: Core implementation complete. Remaining: ProfileCollector refactoring to emit EvidenceEvent directly.
*   **Deliverable**: `assay evidence export` (NDJSON Bundle) and `assay-evidence` crate. ✅ Complete
*   **ADRs**: [ADR-006 (Contract)](./architecture/ADR-006-Evidence-Contract.md), [ADR-007 (Provenance)](./architecture/ADR-007-Deterministic-Provenance.md).

## Current State Summary (January 2026)

Based on the codebase analysis, Assay has achieved significant maturity:

### ✅ Production-Ready Features
- **Complete CLI**: 30+ commands covering all workflows:
  - **Core**: run, validate, init, ci, version
  - **Trace Management**: import, trace, migrate, replay
  - **Policy Management**: generate, record, policy (fmt, migrate, validate)
  - **Analysis**: doctor, explain, coverage, baseline, calibrate, profile
  - **Runtime Security**: mcp-server, monitor, sandbox, discover, kill
  - **Evidence**: evidence (export, verify, show, lint, diff, explore)
  - **Testing**: sim, quarantine, fix (auto-fix), demo
  - **Setup**: setup, init-ci
- **Python SDK**: Full-featured SDK with client, coverage analysis, explanations, and pytest integration
- **Runtime Security**: eBPF/LSM monitoring, MCP server proxy, Landlock sandboxing
- **Deterministic Testing**: Trace replay, VCR caching, baseline regression testing
- **Policy Engine**: JSON Schema validation, sequence rules, tool blocklists, Tier 1/2 compilation
- **Evidence System**: Complete evidence contract implementation with verification, linting, diffing
- **Developer Tools**: Doctor diagnostics, explain violations, coverage analysis, quarantine management
- **CI/CD Integration**: GitHub Actions / GitLab CI workflows, SARIF/JUnit reporting

### 📊 Architecture Maturity
- **10 Crates**: Well-separated concerns (core, cli, metrics, mcp-server, monitor, policy, evidence, common, ebpf, sim)
- **Python Bindings**: PyO3 integration for native performance
- **Comprehensive Tests**: E2E, integration, unit, security, parity tests
- **Documentation**: Complete user docs, architecture ADRs, AI context documentation

### 🎯 Next Steps
- Complete ProfileCollector → EvidenceEvent refactoring
- Launch commercial Evidence Store (Q2 2026)
- Enterprise connectors (SIEM, GRC)

---

## 1. Foundation (Completed 2025)

The core execution and policy engine is stable and production-ready.

### Core Engine
- [x] **Core Sandbox**: CLI runner with Landlock isolation (v1-v4 ABI).
- [x] **Policy Engine v2**: Standardized on JSON Schema for argument validation ([ADR-001](./adr/001-unify-policy-engines-final.md)).
- [x] **Profiling**: "Learning Mode" to generate policies from trace data.
- [x] **Enforcement**: strict/fail-closed modes, environment scrubbing.
- [x] **Tool Integrity (Phase 9)**: Tool metadata hashing and pinning (Anti-Poisoning).

### Runtime Security
- [x] **Runtime Monitor**: eBPF/LSM integration for kernel-level enforcement (`assay monitor`).
- [x] **Policy Compilation**: Tier 1 (kernel/LSM) and Tier 2 (userspace) policy compilation.
- [x] **MCP Server**: Runtime policy enforcement proxy (`assay mcp-server`).

### Testing & Validation
- [x] **Trace Replay**: Deterministic replay without LLM API calls.
- [x] **Baseline Regression**: Compare runs against historical baselines (`assay baseline`).
- [x] **Agent Assertions**: Sequence and structural expectations on traces.
- [x] **Quarantine**: Flaky test management (`assay quarantine`).

### Developer Experience
- [x] **Python SDK**: Complete SDK with `AssayClient`, `Coverage`, `Explainer`, and pytest plugin.
- [x] **Doctor**: Diagnostic tool for common issues (`assay doctor`).
- [x] **Explain**: Human-readable violation explanations (`assay explain`).
- [x] **Coverage Analysis**: Policy coverage calculation (`assay coverage`).
- [x] **Auto-Fix**: Agentic policy fixing (`assay fix`) - suggests and applies policy patches.
- [x] **Demo**: Demo environment generator (`assay demo`) - creates sample configs and traces.
- [x] **Setup**: Interactive system setup (`assay setup`) - Phase 2 implementation for environment configuration.

### Reporting & Integration
- [x] **Multiple Formats**: Console, JSON, JUnit, SARIF output formats.
- [x] **OTel Integration**: OpenTelemetry trace ingest and export.
- [x] **CI Integration**: GitHub Actions / GitLab CI workflows (`assay init-ci`).

### Advanced Features
- [x] **Attack Simulation**: Hardening/compliance testing (`assay sim`).
- [x] **Evidence Management**: Verifiable evidence bundles (`assay evidence`).
- [x] **Trace Management**: Import, ingest, and migrate traces (`assay import`, `assay trace`).
- [x] **MCP Discovery**: Auto-discovery of MCP servers (`assay discover`).
- [x] **MCP Management**: Kill/terminate MCP servers (`assay kill`).
- [x] **Chaos & Differential Testing (Phase 12)**: Robustness testing via `assay sim`.
    - [x] **PR1: Chaos Simulation**: IO chaos (intermittent failures, short reads), stream chaos (partial writes, truncation), runtime chaos (panic shielding, timeouts).
    - [x] **PR2: Differential Verification**: Reference implementation parity, spec drift detection, platform matrix (Windows/Linux).
- [x] **Experimental Features**: MCP process wrapper (`assay mcp`) - experimental, hidden command.

---

## 2. Q1 2026: Trust & Telemetry (Open Standard)

**Objective**: Make Assay the "Evidence Recorder" for any agentic workflow.

### A. Evidence Core (Phase 10)
- [x] **`assay-evidence` Crate**:
    - [x] Schema v1 (`assay.evidence.event.v1`) definitions.
    - [x] JCS (RFC 8785) Canonicalization logic.
    - [x] Content-Addressed ID generation (`sha256(canonical)`).
- [x] **CLI Exporter**:
    - [x] `assay evidence export`: Generate `.tar.gz` bundles (Events + Manifest).
    - [x] `assay evidence verify`: Offline integrity verification.
    - [x] `assay evidence show`: Inspect bundle contents.
    - [x] `assay evidence lint`: Quality and security linting.
    - [x] `assay evidence diff`: Compare two bundles.

### C. Evidence DX & Tooling (Phase 13 - PR5, PR6, PR7) ✅ Complete
- [x] **Evidence Linting (PR5)**:
    - [x] Core logic: `verify_bundle` gate, rule registry (ASSAY-E001).
    - [x] Formats: SARIF (single run, fingerprints), JSON.
    - [x] CLI: `assay evidence lint` with `--fail-on`.
- [x] **Evidence Diff (PR6)**:
    - [x] Semantic diff: Compare extract sets (Hosts, File Access).
    - [x] Baseline: Support `--baseline` tarball or pointer JSON.
    - [x] CLI: `assay evidence diff` (Human/JSON output).
- [x] **Evidence Explore (PR7)**:
    - [x] Safe render: ANSI stripping, control char replacement.
    - [x] TUI app: ratatui-based viewer (Timeline/Detail) - requires `tui` feature flag.
    - [x] CLI: `assay evidence explore` (ReadOnly gate).

### B. Telemetry Integration
- [ ] Refactor `ProfileCollector` to emit `EvidenceEvent` types directly (currently uses `EvidenceMapper` for conversion).
- [x] Add OTel Trace/Span context to all events (`trace_parent`, `trace_state` in `EvidenceEvent`).
- [x] OTel trace ingest (`assay trace ingest-otel`).
- [x] OTel export in test results.

---

## 3. Q2 2026: Supply Chain Moat (Commercial Alpha)

**Objective**: Launch the initial "Moat" features that Enterprises pay for.

### A. Tool Registry & Signing
- [ ] **Signed Tools**: Support for `x-assay-sig` in MCP tool definitions.
- [ ] **Registry Pinning**: `assay.yaml` support for `registry: "assay-verified"`.
- [ ] **Verification**: Runtime check of signatures against transparency log (or public keys).

### B. Evidence Store (MVP)
- [ ] **Hosted Service**: Multi-tenant ingest of Evidence Bundles.
- [ ] **Dashboard**: Policy Drift, Degradation Reports, Env Strictness Score.
- [ ] **Retention**: 90-day WORM storage for compliance.

---

## 4. Q3 2026: Enterprise Scale (Growth)

**Objective**: Integration with the broader Security Ecosystem (SIEM/GRC).

### A. Connectors
- [ ] **SIEM**: Splunk / Microsoft Sentinel export adapters.
- [x] **CI/CD**: GitHub Actions / GitLab CI integration (`assay init-ci`, SARIF upload).
- [ ] **GitHub Actions App**: Native GitHub App for policy drift detection (future).

### B. Compliance Packs
- [ ] **EU AI Act**: Pre-configured logging profiles for Article 12 compliance.
- [ ] **MCPTox**: Automated regression testing against known jailbreak/poisoning patterns.

### C. Runtime Extensions
- [ ] **ABI 6/7 Extensions (Epic G)**: Signal scoping (v6) and Audit Logging (v7).
- [ ] **Learn from Denials**: Phase 7 integration for policy improvement.
- [ ] **Hash Chains (Epic K)**: Link tool metadata to policy snapshots.
- [ ] **HITL Implementation (Epic L)**: Decision variant + receipts, Guardrail Hooks (NeMo/Superagent integration).

---

## 5. Q4 2026: Managed Execution (Platform)

**Objective**: Zero-configuration secure runtime for untrusted code.

### A. Managed Isolation (Phase 13 - PR8)
- [ ] **Managed Runners**: Cloud-hosted MicroVMs (Firecracker/gVisor) controlled via Assay CLI.
- [ ] **Zero-Infra**: `assay run --remote ...` transparent offloading.

---

## 6. Backlog / Deferred

Items from earlier phases that are deferred or require additional planning.

### Epic J: Assay Explain ✅ Complete
- [x] **Correlate events to Rule/Pack**: Event-to-rule correlation implemented.
- [x] **Blocked messages**: "Blocked because outbound not allowlisted" messages implemented.
- [x] **Auto-Fix Integration**: `assay fix` command provides agentic policy fixing with risk levels.
- **Status**: Already completed in Foundation phase.

### Epic G: ABI 6/7 Extensions
- [ ] **Signal Scoping (v6)**: Advanced signal handling for runtime security.
- [ ] **Audit Logging (v7)**: Enhanced audit capabilities.
- [ ] **Learn from Denials**: Integration with Phase 7 for policy improvement.

### Epic K: Hash Chains
- [ ] **Tool Metadata Linking**: Link tool metadata to policy snapshots.
- [ ] **Integrity Verification**: Cryptographic verification of tool-to-policy relationships.

### Epic L: HITL Implementation
- [ ] **Decision Variant + Receipts**: Human-in-the-loop decision tracking.
- [ ] **Guardrail Hooks**: Integration with NeMo/Superagent frameworks.

---

## Reference Architecture

### Open Core (The Standard)
*   Likely License: Apache 2.0 / MIT.
*   Components: `assay-cli`, `assay-core`, `assay-evidence`, `assay-mcp-server`, `assay-metrics`, `assay-monitor`, `assay-policy`, `assay-common`, `assay-ebpf`, `assay-sim`, `assay-python-sdk`.
*   Value: Local reproducibility, developer DX, standard schemas, runtime security, deterministic testing.

### Proprietary (The Product)
*   Components: Evidence Store Control Plane, Signing Service, Compliance Analytics.
*   Value: Governance, Scale, Retention, Indemnification.
