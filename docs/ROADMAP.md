# Assay Roadmap 2026

**Strategic Focus:** Agent Runtime Evidence & Control Plane.
**Moat:** Verifiable Evidence (Open Standard) + Governance Platform (Paid Service).

---

## 🛑 Current Priority: Phase 10 (Evidence Contract)

We are currently building the **Evidence Contract v1** to establish Assay as the standard for agent auditability.

*   **Goal**: Replace "logging" with "verifiable evidence" (CloudEvents + OTel + Deterministic IDs).
*   **Deliverable**: `assay evidence export` (NDJSON Bundle) and `assay-evidence` crate.
*   **ADRs**: [ADR-006 (Contract)](./architecture/ADR-006-Evidence-Contract.md), [ADR-007 (Provenance)](./architecture/ADR-007-Deterministic-Provenance.md).

---

## 1. Foundation (Completed 2025)

The core execution and policy engine is stable and production-ready.

- [x] **Core Sandbox**: CLI runner with Landlock isolation (v1-v4 ABI).
- [x] **Policy Engine v2**: Standardized on JSON Schema for argument validation ([ADR-001](./adr/001-unify-policy-engines-final.md)).
- [x] **Profiling**: "Learning Mode" to generate policies from trace data.
- [x] **Enforcement**: strict/fail-closed modes, environment scrubbing.
- [x] **Tool Integrity (Phase 9)**: Tool metadata hashing and pinning (Anti-Poisoning).

---

## 2. Q1 2026: Trust & Telemetry (Open Standard)

**Objective**: Make Assay the "Evidence Recorder" for any agentic workflow.

### A. Evidence Core (Phase 10)
- [ ] **`assay-evidence` Crate**:
    - [ ] Schema v1 (`assay.evidence.event.v1`) definitions.
    - [ ] JCS (RFC 8785) Canonicalization logic.
    - [ ] Content-Addressed ID generation (`sha256(canonical)`).
- [ ] **CLI Exporter**:
    - [ ] `assay evidence export`: Generate `.tar.gz` bundles (Events + Manifest).
    - [ ] `assay evidence verify`: Offline integrity verification.

### B. Telemetry Integration
- [ ] Refactor `ProfileCollector` to emit `EvidenceEvent` types.
- [ ] Add OTel Trace/Span context to all events.

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
- [ ] **CI/CD**: GitHub Actions Reporting App (Block PR on Policy Drift).

### B. Compliance Packs
- [ ] **EU AI Act**: Pre-configured logging profiles for Article 12 compliance.
- [ ] **MCPTox**: Automated regression testing against known jailbreak/poisoning patterns.

---

## 5. Q4 2026: Managed Execution (Platform)

**Objective**: Zero-configuration secure runtime for untrusted code.

- [ ] **Managed Runners**: Cloud-hosted MicroVMs (Firecracker/gVisor) controlled via Assay CLI.
- [ ] **Zero-Infra**: `assay run --remote ...` transparent offloading.

---

## Reference Architecture

### Open Core (The Standard)
*   Likely License: Apache 2.0 / MIT.
*   Components: `assay-cli`, `assay-core`, `assay-evidence`, `assay-mcp-server`.
*   Value: Local reproducibility, developer DX, standard schemas.

### Proprietary (The Product)
*   Components: Evidence Store Control Plane, Signing Service, Compliance Analytics.
*   Value: Governance, Scale, Retention, Indemnification.
