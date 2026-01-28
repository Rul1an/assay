# Assay Roadmap 2026

**Strategic Focus:** Agent Runtime Evidence & Control Plane.
**Core Value:** Verifiable Evidence (Open Standard) + Governance Platform.

---

## Executive Summary

Assay is the "Evidence Recorder" for agentic workflows. We create verifiable, machine-readable audit trails that integrate with existing security/observability stacks.

**Standards Alignment:**
- **CloudEvents v1.0** envelope — lingua franca for event routers and SIEM pipelines
- **W3C Trace Context** (`traceparent`) — correlation with existing distributed tracing
- **SARIF 2.1.0** — GitHub Code Scanning integration (single run + `automationDetails.id` discipline)
- **EU AI Act Article 12** — record-keeping requirements make "evidence" commercially relevant

---

## Current State: Evidence Contract v1 ✅ Complete

The **Evidence Contract v1** is production-ready.

| Component | Status | Notes |
|-----------|--------|-------|
| `assay-evidence` crate | ✅ | Schema v1, JCS canonicalization, content-addressed IDs |
| Evidence pipeline | ✅ | `ProfileCollector` → `Profile` → `EvidenceMapper` → `EvidenceEvent` (OTel Collector pattern) |
| CLI commands | ✅ | export, verify, show, lint, diff, explore |
| OTel integration | ✅ | `trace_parent`, `trace_state` on all events |

**Architecture Note:** The current pipeline follows the OTel Collector pattern (native format emission → transformation layer → canonical export). This is the recommended SOTA approach per OpenTelemetry best practices. See [ADR-008: Evidence Streaming](./architecture/ADR-008-Evidence-Streaming.md) for the decision to keep CloudEvents construction out of the hot path.

### 🎯 Immediate Next Steps (Q1 Close-out)

1. **v1 Contract Freeze** — Publish versioning policy, deprecation rules, golden bundle fixtures
2. **Compatibility Tests** — No new event types without schema + tests
3. **Docs Positioning** — "Assay Evidence = CloudEvents + Trace Context + Deterministic Bundle"

---

## CLI Surface: Two-Layer Positioning

To reduce "surface area tax" and improve adoption, CLI commands are positioned in two tiers:

### Happy Path (Core Workflow)
```bash
assay run              # Execute with policy enforcement
assay evidence export  # Create verifiable bundle
assay evidence verify  # Offline integrity check
assay evidence lint    # Security/quality findings (SARIF)
assay evidence diff    # Compare bundles
assay evidence explore # Interactive TUI viewer
```

### Power Tools (Advanced/Experimental)
All other commands (`quarantine`, `fix`, `demo`, `sim`, `discover`, `kill`, `mcp`, etc.) are documented separately as advanced tooling.

---

## Q1 2026: Trust & Telemetry ✅ Complete

**Objective:** Establish Assay as the standard for agent auditability.

### Evidence Core
- [x] Schema v1 (`assay.evidence.event.v1`) definitions
- [x] JCS (RFC 8785) canonicalization
- [x] Content-addressed ID generation (`sha256(canonical)`)
- [x] CLI: export, verify, show

### Evidence DX (Lint/Diff/Explore)
- [x] **Linting**: Rule registry, SARIF output with `partialFingerprints`, `--fail-on` threshold
- [x] **Diff**: Semantic comparison (hosts, file access), baseline support
- [x] **Explore**: TUI viewer with ANSI/control char sanitization (`tui` feature flag)

### Hardening (Chaos/Differential Testing)
- [x] IO chaos (intermittent failures, short reads, `Interrupted`/`WouldBlock`)
- [x] Stream chaos (partial writes, truncation)
- [x] Differential verification (reference parity, spec drift, platform matrix)

### Telemetry
- [x] OTel Trace/Span context on all events
- [x] OTel trace ingest (`assay trace ingest-otel`)
- [x] OTel export in test results

---

## Q2 2026: Supply Chain Security

**Objective:** Launch the initial commercial features.

### Prioritized Deliverables

| Priority | Item | Effort | Value | ADR |
|----------|------|--------|-------|-----|
| **P0** | GitHub Action v2 | Medium | High | [ADR-014](./architecture/ADR-014-GitHub-Action-v2.md) ✅ |
| **P0** | WORM Storage (90-day retention) | Low | High | [ADR-009](./architecture/ADR-009-WORM-Storage.md) |
| **P1** | Evidence Store MVP (Ingest API) | Medium | High | [ADR-010](./architecture/ADR-010-Evidence-Store-API.md) |
| **P2** | Tool Signing (`x-assay-sig`) | Medium | High | [ADR-011](./architecture/ADR-011-Tool-Signing.md) |
| **P2** | EU AI Act Compliance Pack | Medium | High | [ADR-013](./architecture/ADR-013-EU-AI-Act-Pack.md) |
| **P3** | Transparency Log Verification | Low | Medium | [ADR-012](./architecture/ADR-012-Transparency-Log.md) |
| **Defer** | Registry Pinning | Medium | Low (Q2) | Requires signing + hosted service |
| **Defer** | Dashboard | High | Medium | CLI queries first, UI in Q3 |

### GitHub Action v2 ✅ Complete

Published to GitHub Marketplace: [Rul1an/assay-action](https://github.com/Rul1an/assay-action)

```yaml
- uses: Rul1an/assay-action@v2
```

Features:
- Zero-config evidence bundle discovery
- SARIF integration with GitHub Security tab
- PR comments (only when findings)
- Baseline comparison via cache
- Job Summary reports

### A. Evidence Store MVP (Paid)
- [ ] **CLI Commands** (Open Core): `assay evidence push/pull/list`
- [ ] **Ingest API**: REST endpoint for bundle upload → S3 with Object Lock
- [ ] **WORM Storage**: 90-day immutable retention (SEC 17a-4, CFTC, FINRA compliant)
- [ ] **Query API**: Basic bundle retrieval by `run_id`, `bundle_id`
- [ ] **Legal Hold**: Endpoint for investigation freeze

### B. Tool Signing (Open Core Hooks + Paid Verification)
- [ ] **Open Core**: `x-assay-sig` field, ed25519 local signing/verification
- [ ] **Sigstore Keyless**: Fulcio certificate + Rekor transparency log
- [ ] **Paid**: Managed identity monitoring, org trust policies

### C. Compliance Packs (Open Baseline + Paid Managed)
- [ ] **Pack Engine**: `--pack` CLI flag, pack composition
- [ ] **EU AI Act Pack**: Article 12 mapping, SARIF output with `article_ref`
- [ ] **Paid**: Org-specific exceptions, PDF audit reports

---

## Q3 2026: Enterprise Scale (Growth)

**Objective:** Integration with the broader security ecosystem.

### A. Connectors (Paid)
- [ ] **SIEM**: Splunk / Microsoft Sentinel export adapters
- [x] **CI/CD**: GitHub Actions v2 ([Rul1an/assay-action@v2](https://github.com/Rul1an/assay-action)) / GitLab CI integration
- [ ] **GitHub App**: Native policy drift detection in PRs

### B. Compliance Packs (Open Baseline + Paid Managed)
- [ ] **EU AI Act Pack**: Pre-configured Article 12 logging profiles
- [ ] **MCPTox**: Regression testing against jailbreak/poisoning patterns
- [ ] **Managed Packs**: Org-specific exceptions, reporting templates

### C. Governance Dashboard (Paid)
- [ ] **Policy Drift**: Trend lines, anomaly detection
- [ ] **Degradation Reports**: Evidence health score
- [ ] **Env Strictness Score**: Compliance posture metrics

---

## Q4 2026: Managed Execution (Platform)

**Objective:** Zero-configuration secure runtime for untrusted code.

### A. Managed Isolation (Paid)
- [ ] **Managed Runners**: Cloud-hosted MicroVMs (Firecracker/gVisor)
- [ ] **Zero-Infra**: `assay run --remote ...` transparent offloading

---

## Backlog / Deferred

### Evidence Streaming Mode (Optional)
- [ ] **Streaming Mode**: Native events + async mapping for real-time OTel export and Evidence Store ingest
  - `EventSink` trait with `AggregatingProfileSink` (default) and `StreamingSink` (feature-gated)
  - `assay evidence stream` command (NDJSON to stdout/file)
  - Backpressure handling, bounded memory
  - See [ADR-008](./architecture/ADR-008-Evidence-Streaming.md) for design

**Note:** This is a product capability, not a refactoring item. The current `ProfileCollector` → `EvidenceMapper` pipeline is correct per OTel Collector pattern. Streaming mode adds an alternative path for real-time use cases without changing the default behavior.

### Runtime Extensions (Epic G)
- [ ] ABI 6/7: Signal scoping (v6), Audit Logging (v7)
- [ ] Learn from Denials: Policy improvement from blocked requests

### Hash Chains (Epic K)
- [ ] Tool Metadata Linking: Link tool definitions to policy snapshots
- [ ] Integrity Verification: Cryptographic tool-to-policy binding

### HITL Implementation (Epic L)
- [ ] Decision Variant + Receipts: Human-in-the-loop tracking
- [ ] Guardrail Hooks: NeMo/Superagent integration

---

## Foundation (Completed 2025)

The core execution and policy engine is stable and production-ready.

### Core Engine
- [x] Core Sandbox: CLI runner with Landlock isolation (v1-v4 ABI)
- [x] Policy Engine v2: JSON Schema for argument validation
- [x] Profiling: "Learning Mode" to generate policies from traces
- [x] Enforcement: strict/fail-closed modes, environment scrubbing
- [x] Tool Integrity (Phase 9): Tool metadata hashing and pinning

### Runtime Security
- [x] Runtime Monitor: eBPF/LSM kernel-level enforcement
- [x] Policy Compilation: Tier 1 (kernel/LSM) and Tier 2 (userspace)
- [x] MCP Server: Runtime policy enforcement proxy

### Testing & Validation
- [x] Trace Replay: Deterministic replay without LLM API calls
- [x] Baseline Regression: Compare runs against historical baselines
- [x] Agent Assertions: Sequence and structural expectations
- [x] Quarantine: Flaky test management

### Developer Experience
- [x] Python SDK: `AssayClient`, `Coverage`, `Explainer`, pytest plugin
- [x] Doctor: Diagnostic tool for common issues
- [x] Explain: Human-readable violation explanations
- [x] Coverage Analysis: Policy coverage calculation
- [x] Auto-Fix: Agentic policy fixing with risk levels
- [x] Demo: Demo environment generator
- [x] Setup: Interactive system setup

### Reporting & Integration
- [x] Multiple Formats: Console, JSON, JUnit, SARIF
- [x] OTel Integration: Trace ingest and export
- [x] CI Integration: GitHub Actions / GitLab CI workflows

### Advanced Features
- [x] Attack Simulation: `assay sim` hardening/compliance testing
- [x] MCP Discovery: Auto-discovery of MCP servers
- [x] MCP Management: Kill/terminate MCP servers
- [x] Experimental: MCP process wrapper (hidden command)
