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

**Objective:** Launch compliance and signing features with zero infrastructure cost.

**Strategy:** BYOS-first (Bring Your Own Storage) per [ADR-015](./architecture/ADR-015-BYOS-Storage-Strategy.md). Users provide their own S3-compatible storage. Managed infrastructure deferred until PMF.

### Prioritized Deliverables

| Priority | Item | Effort | Value | Status |
|----------|------|--------|-------|--------|
| **P0** | GitHub Action v2 | Medium | High | ✅ Complete |
| **P1** | BYOS CLI Commands | Low | High | ✅ Complete |
| **P1** | Tool Signing (`x-assay-sig`) | Medium | High | 🔜 Next |
| **P2** | EU AI Act Compliance Pack | Medium | High | Pending |
| **P2** | GitHub Action v2.1 | Low | Medium | Pending |
| **P3** | Transparency Log Verification | Low | Medium | Pending |
| **Defer** | Managed Evidence Store | High | Medium | Q3+ if demand |
| **Defer** | Dashboard | High | Medium | Q3+ |

See ADRs: [ADR-011 (Signing)](./architecture/ADR-011-Tool-Signing.md), [ADR-013 (EU AI Act)](./architecture/ADR-013-EU-AI-Act-Pack.md), [ADR-014 (Action)](./architecture/ADR-014-GitHub-Action-v2.md), [ADR-015 (BYOS)](./architecture/ADR-015-BYOS-Storage-Strategy.md)

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

### A. BYOS CLI Commands ✅ Complete

Per [ADR-015](./architecture/ADR-015-BYOS-Storage-Strategy.md), evidence storage uses user-provided S3-compatible buckets:

```bash
# CLI commands
assay evidence push bundle.tar.gz --store s3://bucket/prefix
assay evidence pull --bundle-id sha256:... --store s3://bucket/prefix
assay evidence list --run-id run_123 --store s3://bucket/prefix
```

- [x] **Generic S3 Client**: Using `object_store` crate
- [x] **push command**: Upload verified bundle with immutability-safe writes
- [x] **pull command**: Download bundle by ID or run
- [x] **list command**: List bundles with filtering, JSON/table/plain output
- [x] **Conditional writes**: `If-None-Match: "*"` for immutability
- [x] **Content-addressed keys**: SHA-256 bundle_id as source of truth

Supported backends: AWS S3, Backblaze B2, Wasabi, Cloudflare R2, MinIO, Azure Blob, GCS, local filesystem

### B. Tool Signing (Open Core)
- [ ] **`x-assay-sig` field**: Ed25519 signature in bundle manifest
- [ ] **Local signing**: `assay evidence sign bundle.tar.gz --key private.pem`
- [ ] **Local verification**: `assay evidence verify bundle.tar.gz --pubkey public.pem`
- [ ] **Keyless (future)**: Sigstore Fulcio + Rekor integration

### C. Compliance Packs (Open Core)
- [ ] **Pack Engine**: `--pack` CLI flag, pack composition
- [ ] **EU AI Act Pack**: Article 12 mapping, SARIF output with `article_ref`
- [ ] **Pack Registry**: Local packs in `~/.assay/packs/`

### D. GitHub Action v2.1

After P1/P2 features:
- [ ] `assay init` workflow generator
- [ ] Compliance pack support (`--pack eu-ai-act`)
- [ ] Coverage badge generation
- [ ] Store integration (push to BYOS)

---

## Q3 2026: Enterprise Scale (Growth)

**Objective:** Integration with the broader security ecosystem.

### A. Connectors
- [ ] **SIEM**: Splunk / Microsoft Sentinel export adapters
- [x] **CI/CD**: GitHub Actions v2 ([Rul1an/assay-action@v2](https://github.com/Rul1an/assay-action)) / GitLab CI integration
- [ ] **GitHub App**: Native policy drift detection in PRs
- [ ] **GitLab CI**: Native integration

### B. Additional Compliance Packs
- [ ] **SOC 2 Pack**: Control mapping for Type II audits
- [ ] **MCPTox**: Regression testing against jailbreak/poisoning patterns
- [ ] **Industry Packs**: Healthcare (HIPAA), Finance (PCI-DSS)

### C. Managed Evidence Store (Evaluate)

Only proceed if:
1. Users explicitly request managed hosting
2. Revenue model supports infrastructure costs
3. PMF is validated

If yes, implement per [ADR-009](./architecture/ADR-009-WORM-Storage.md) and [ADR-010](./architecture/ADR-010-Evidence-Store-API.md):
- [ ] **Cloudflare Workers + R2**: Non-SEC-compliant tier (lowest cost)
- [ ] **Backblaze B2 Proxy**: SEC 17a-4 compliant tier
- [ ] **Pricing**: Pass-through storage + margin

---

## Q4 2026: Platform Features

**Objective:** Advanced capabilities for enterprise adoption.

### A. Governance Dashboard (If Managed Store Exists)
- [ ] **Policy Drift**: Trend lines, anomaly detection
- [ ] **Degradation Reports**: Evidence health score
- [ ] **Env Strictness Score**: Compliance posture metrics

### B. Advanced Signing
- [ ] **Sigstore Keyless**: Fulcio certificate + Rekor transparency log
- [ ] **Org Trust Policies**: Managed identity verification

### C. Managed Isolation (Future)
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

---

## Open Core Philosophy

Assay follows the **open core model**: the core evidence tooling is fully open source (Apache 2.0), while enterprise governance features are commercially licensed.

### Open Source (Apache 2.0)

Everything needed to create, verify, and analyze evidence locally:

| Category | Components |
|----------|------------|
| **Evidence Contract** | Schema v1, JCS canonicalization, content-addressed IDs, deterministic bundles |
| **CLI Workflow** | `export`, `verify`, `lint`, `diff`, `explore`, `show` |
| **BYOS Storage** | `push`, `pull`, `list` with S3/Azure/GCS/local backends |
| **Basic Signing** | Ed25519 local key signing and verification |
| **Runtime Security** | Policy engine, MCP proxy, eBPF/LSM monitor |
| **Developer Experience** | Python SDK, pytest plugin, GitHub Action |
| **Output Formats** | SARIF, JUnit, JSON, console |

**Why open:** Standards adoption requires broad accessibility. The evidence format should become infrastructure, not a product moat.

### Enterprise Features (Commercial)

Governance, compliance, and scale capabilities for organizations:

| Category | Components |
|----------|------------|
| **Identity & Access** | SSO/SAML/SCIM, RBAC, teams, approval workflows |
| **Compliance** | EU AI Act Pack, SOC 2 mapping, compliance reporting |
| **Advanced Signing** | Sigstore keyless, transparency log verification, trust policies |
| **Managed Storage** | WORM retention, legal hold, compliance attestation |
| **Integrations** | SIEM connectors (Splunk/Sentinel), OTel pipeline templates |
| **Fleet Management** | Policy distribution, runtime agent management |

**Principle:** Gate platform scale and operations, not basic workflow. Developers should always be able to create and verify evidence locally for free.
