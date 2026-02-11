# Assay Roadmap 2026

> **Status sync (2026-02-10):** Q2 items verified against codebase — all P0/P1/P2 DX features delivered.
> Code health (RFC-002) and generate decomposition (RFC-003 G1–G6) merged.
> Golden path, drift-aware feedback, GitHub Action v2.1 confirmed complete.
> Remaining structural items tracked in [RFC-004](architecture/RFC-004-open-items-convergence-q1-2026.md).

**Strategic Focus:** Agent Runtime Evidence & Control Plane.
**Core Value:** Verifiable Evidence (Open Standard) + Governance Platform.

## Executive Summary

Assay is the "Evidence Recorder" for agentic workflows. We create verifiable, machine-readable audit trails that integrate with existing security/observability stacks. Assay aims to become the **standard evidence lint runtime** for agentic CI, with open engine + baseline packs and strong CI/SARIF integration as the adoption motor.

**Standards Alignment:**
- **CloudEvents v1.0** envelope — lingua franca for event routers and SIEM pipelines
- **W3C Trace Context** (`traceparent`) — correlation with existing distributed tracing
- **SARIF 2.1.0** — GitHub Code Scanning integration (single run + `automationDetails.id` discipline)
- **EU AI Act Article 12** — record-keeping requirements make "evidence" commercially relevant
- **OTel GenAI Semantic Conventions** — vendor-agnostic observability for LLM/agent workloads
- **ENISA / SBOM / SLSA** — Supply-chain assurance (SBOM, provenance, attestation) aligns with ENISA priorities; SLSA-aligned attestation per ADR-018

---

## Strategic Positioning: Protocol-Agnostic Governance

The agentic commerce/interop space is fragmenting (Jan 2026):

| Protocol | Owner | Focus |
|----------|-------|-------|
| **ACP** (Agentic Commerce Protocol) | OpenAI/Stripe | Buyer/agent/business transactions |
| **UCP** (Universal Commerce Protocol) | Google/Shopify | Discover→buy→post-purchase journeys |
| **AP2** (Agent Payments Protocol) | Google | Secure transactions + mandates |
| **A2A** (Agent2Agent) | Google | Agent discovery/capabilities/tasks |
| **x402** | Community | Internet-native (crypto) agent payments |

**Assay's moat:** Protocol-agnostic evidence + governance layer.

> "Regardless of protocol: verifiable evidence, policy enforcement, trust verification, SIEM/OTel-ready."

All these protocols converge on "tool calls + state transitions" — exactly what Assay captures as trace-linked evidence.

### Why This Matters

1. **Tool Signing** becomes critical: "tool substitution" and "merchant tool spoofing" are real commerce risks
2. **Mandates/Intents** need audit trails: AP2's authorization model requires provable evidence
3. **Agent Identity** is enterprise-core: who/what authorized a transaction?

See [Protocol Landscape Analysis](.private/docs/strategy/PROTOCOL-LANDSCAPE-2026.md) for detailed research

### Market Validation (Feb 2026)

The CI/CD-for-agents market is validating Assay's core assumptions:

- **AAIF (Agentic AI Foundation)**: MCP, goose and AGENTS.md are now under Linux Foundation governance (Dec 2025). MCP as a vendor-neutral standard reduces protocol fragmentation risk and validates Assay's MCP-first bet.
- **GitHub "Continuous AI"** (Feb 2026): repo-agents with read-only default + "Safe Outputs" — explicit contracts defining what agents may produce. This is Assay's policy-as-code model, validated by the largest development platform.
- **Policy-as-code as best practice**: Multiple sources (V2Solutions, Skywork, Gartner) now list policy-as-code, least privilege, auditability and kill switches as enterprise requirements for agent deployment. Not a niche compliance need anymore.
- **Fleet-of-small-agents pattern**: The dominant deployment pattern is many small specialized agents, not one generalist. More agents = more policies = more Assay usage per repo.
- **Gartner risk signal**: >40% of agentic AI projects will be cancelled by end 2027 due to costs, unclear value, or inadequate risk controls. Governance tooling is a prerequisite, not a nice-to-have.

**Competitive differentiation**: Agent CI (eval-as-service), Langfuse/LangSmith (observability), Dagger (agentic runtime) cover adjacent layers. None offers deterministic replay, evidence bundles with integrity guarantees, or compliance packs. Assay's unique position: governance + audit, not observability or eval-as-service.

See [RESEARCH-ci-cd-ai-agents-feb2026.md](architecture/RESEARCH-ci-cd-ai-agents-feb2026.md) for detailed analysis

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

## Developer UX/DX Strategy (Feb 2026 Refresh)

Assay execution priorities are now explicitly evaluated against five developer-facing dimensions:

| Dimension | Why it matters for Assay | 2026 Direction |
|-----------|---------------------------|----------------|
| Time-to-first-signal | Teams adopt if first value arrives fast | Keep the golden path short (`init` -> trace -> run -> PR signal) |
| Quality-of-feedback | Red/green alone is not enough for agent systems | Keep reason codes, explainability, and rerun hints as first-class contracts |
| Workflow fit | Adoption follows existing surfaces | Prioritize CI/PR/SARIF/Security-tab integrations over new standalone UI |
| Trust & auditability | Security/compliance requires reproducibility | Preserve deterministic outputs, seeds, and evidence bundle integrity |
| Change resilience | MCP/tools/policies drift over time | Make drift visible with diff/explain flows before it becomes a blocking surprise |

Execution rule: if a proposal does not clearly improve at least one of these dimensions without raising cognitive load, it is deferred.

### Deliberate Non-Plays

Based on [competitive landscape analysis](architecture/RESEARCH-ci-cd-ai-agents-feb2026.md):

- **Not observability**: Langfuse, LangSmith, Arize do this better and it's a different market. Integrate via OTel where needed, don't build dashboards.
- **Not eval-as-a-service**: Agent CI and LangSmith do evals. Assay does policy enforcement + evidence. Overlap on PR-gates, but the value proposition is different.
- **Not agent-building**: Dagger, Zencoder build agents. Assay validates them. Complementary, not competitive.

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
| **P0** | Exit Codes (V2) | Low | High | ✅ Complete (v2.12.0) |
| **P0** | Report IO Robustness (Warnings) | Low | High | ✅ Complete (v2.12.0) |
| **P1** | BYOS CLI Commands | Low | High | ✅ Complete |
| **P1** | Tool Signing (`x-assay-sig`) | Medium | High | ✅ Complete (v2.9.0) |
| **P2** | Pack Engine (OSS) | Medium | High | ✅ Complete (v2.10.0) |
| **P2** | EU AI Act Baseline Pack (OSS) | Low | High | ✅ Complete (v2.10.0) |
| **P2** | Mandate/Intent Evidence | Medium | High | ✅ Complete (v2.11.0) |
| **P1** | Judge Reliability (SOTA E7) | High | High | ✅ Complete (Audit Grade) |
| **P1** | Progress N/M (E4.3) | Low | High | ✅ Complete (PR #164) |
| **P2** | GitHub Action v2.1 | Low | Medium | ✅ Complete (PR #185) |
| **P1** | Golden path (<30 min first signal) | Medium | High | ✅ Complete (PR #187, `init --hello-trace --ci`) |
| **P1** | Drift-aware feedback (`explain` + policy/tool diffs) | Medium | High | ✅ Complete (`generate --diff` PR #177, `explain` PR #179) |
| **P1** | CLI debt reduction (Wave A/B: typed errors, pipeline, config) | Medium | High | ✅ Wave A/B merged, Wave C gated |
| **P1** | Starter packs (OSS) | Low | High | ✅ Complete (ADR-023) |
| **P2** | Sim Engine Hardening (limits + budget) | Low | Medium | Pending |
| **P3** | Sigstore Keyless (Enterprise) | Medium | Medium | Pending |
| **Defer** | Managed Evidence Store | High | Medium | Q3+ if demand |
| **Defer** | Dashboard | High | Medium | Q3+ |

See ADRs: [ADR-011 (Signing)](./architecture/ADR-011-Tool-Signing.md), [ADR-013 (EU AI Act)](./architecture/ADR-013-EU-AI-Act-Pack.md), [ADR-014 (Action)](./architecture/ADR-014-GitHub-Action-v2.md), [ADR-015 (BYOS)](./architecture/ADR-015-BYOS-Storage-Strategy.md), [ADR-016 (Pack Taxonomy)](./architecture/ADR-016-Pack-Taxonomy.md)
See Spec: [SPEC-Tool-Signing-v1](./architecture/SPEC-Tool-Signing-v1.md)
See Debt: [RFC-001 DX/UX Governance](./architecture/RFC-001-dx-ux-governance.md) (Wave A: correctness ✅, Wave B: maintainability ✅, Wave C: performance — gated)

### GitHub Action v2 ✅ Complete

Published to GitHub Marketplace: [assay-ai-agent-security](https://github.com/marketplace/actions/assay-ai-agent-security)

```yaml
- uses: Rul1an/assay/assay-action@v2
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

### B. Tool Signing ✅ Complete

Per [SPEC-Tool-Signing-v1](./architecture/SPEC-Tool-Signing-v1.md):

```bash
assay tool keygen --out ~/.assay/keys/      # Generate PKCS#8/SPKI keypair
assay tool sign tool.json --key priv.pem --out signed.json
assay tool verify signed.json --pubkey pub.pem  # Exit: 0=ok, 2=unsigned, 3=untrusted, 4=invalid
```

- [x] **`x-assay-sig` field**: Ed25519 + DSSE PAE encoding
- [x] **JCS canonicalization**: RFC 8785 deterministic JSON
- [x] **key_id trust model**: SHA-256 of SPKI bytes
- [x] **Trust policies**: `require_signed`, `trusted_key_ids`
- [ ] **Keyless (Enterprise)**: Sigstore Fulcio + Rekor integration

### C. Mandate/Intent Evidence (P2) ✅ Complete (v2.11.0)

Full mandate evidence implementation per [SPEC-Mandate-v1.0.5](./architecture/SPEC-Mandate-v1.md):

- [x] **Evidence types**: Mandate content (intent/transaction), signed envelopes, lifecycle events
- [x] **Runtime enforcement**: MandateStore (SQLite), 7-step Authorizer flow, revocation support
- [x] **CloudEvents**: `mandate.used`, `mandate.revoked`, `tool.decision` lifecycle events
- [x] **CLI integration**: `--audit-log`, `--decision-log`, `--event-source` flags
- [x] **Idempotent retries**: Deterministic `use_id` + `was_new` flag for audit-log deduplication
- [x] **Revocation**: Hard cutoff (no clock skew tolerance) per SPEC §7.6

See [ADR-017](./architecture/ADR-017-Mandate-Evidence.md) for architecture decision.

This strengthens EU AI Act Articles 12 & 14 compliance for commerce workflows.

### C. Compliance Packs (P2) — Semgrep Model

Per [ADR-016](./architecture/ADR-016-Pack-Taxonomy.md), we follow the Semgrep open core pattern:
- **Engine + Baseline packs** = Open Source (Apache 2.0)
- **Pro packs + Managed workflows** = Enterprise (Commercial)

#### Pack Engine (OSS) ✅ Complete (v2.10.0)

```bash
assay evidence lint --pack eu-ai-act-baseline        # Single pack
assay evidence lint --pack eu-ai-act-baseline,soc2   # Composition
assay evidence lint --pack ./custom-pack.yaml        # Custom pack
```

- [x] **Pack loader**: YAML schema with `pack_kind` (compliance/security/quality)
- [x] **Rule ID namespacing**: `{pack}@{version}:{rule_id}` for collision handling
- [x] **Pack composition**: `--pack a,b` with deterministic merge
- [x] **Version resolution**: `assay_min_version` + `evidence_schema_version`
- [x] **Pack digest**: SHA256 (JCS RFC 8785) for supply chain integrity
- [x] **SARIF output**: Pack metadata in `properties` bags (GitHub Code Scanning compatible)
- [x] **Disclaimer enforcement**: `pack_kind == compliance` requires disclaimer
- [x] **GitHub dedup**: `primaryLocationLineHash` fingerprint
- [x] **Truncation**: `--max-results` for SARIF size limits

#### EU AI Act Baseline Pack (OSS) ✅ Complete (v2.10.0)

Direct Article 12(1) + 12(2)(a)(b)(c) mapping:

| Rule ID | Article | Check | Status |
|---------|---------|-------|--------|
| EU12-001 | 12(1) | Evidence bundle contains automatically recorded events | ✅ |
| EU12-002 | 12(2)(c) | Events include lifecycle fields for operation monitoring | ✅ |
| EU12-003 | 12(2)(b) | Events include correlation IDs for post-market monitoring | ✅ |
| EU12-004 | 12(2)(a) | Events include fields for risk situation identification | ✅ |

See [ADR-013](./architecture/ADR-013-EU-AI-Act-Pack.md) for detailed mapping and [SPEC-Pack-Engine-v1](./architecture/SPEC-Pack-Engine-v1.md) for implementation spec.

#### EU AI Act Pro Pack (Enterprise)

- [ ] Biometric-specific rules (Article 12(3))
- [ ] Retention policy validation
- [ ] Advanced risk scoring
- [ ] Org-specific exception workflows
- [ ] PDF audit report generation

#### Additional Packs (Future)

- [ ] **Commerce Pack**: Mandate/intent required, signed-tools required (enabled by v2.11.0 mandate support)
- [ ] **SOC2 Baseline/Pro**: Control mapping packs (baseline = Common Criteria only; Pro = assurance depth)
- [x] **Starter packs (OSS)**: CICD hygiene, minimal traceability — compatibility floor; see §F
- [ ] **Pack Registry**: Local packs in `~/.assay/packs/` (ADR-021)

### E. GitHub Action v2.1 ✅ Complete

Per [ADR-018](./architecture/ADR-018-GitHub-Action-v2.1.md):

| Priority | Feature | Rationale |
|----------|---------|-----------|
| **P1** | Compliance pack support | EU AI Act compliance story |
| **P2** | BYOS push with OIDC | Zero-credential enterprise posture |
| **P3** | Artifact attestation | Supply chain integrity |
| **P4** | Coverage badge | Developer DX |

**Key design decisions:**
- Write operations (push, attest, badge) only on `push` to main (fork PR threat model)
- OIDC authentication per provider (explicit, not auto-detect)
- Attestations provide "SLSA-aligned provenance" (no specific level claims)
- EU AI Act timeline accurately documented (phased: Feb 2025, Aug 2025, Aug 2026)

See [ADR-018](./architecture/ADR-018-GitHub-Action-v2.1.md) for full specification.

### F. Starter Packs (OSS) (P1)

CICD-hygiene pack as compatibility floor for adoption—minimal traceability so teams get first value from `assay evidence lint` with minimal config. See [ADR-023](./architecture/ADR-023-CICD-Starter-Pack.md).

```bash
assay evidence lint --pack cicd-starter bundle.tar.gz
assay evidence lint --pack cicd-starter,eu-ai-act-baseline bundle.tar.gz
```

**Status per step (codebase-check):**

| Check | Status |
|-------|--------|
| `cicd-starter` or similar pack | ✅ Present (packs/open + vendored) |
| Pack in `packs/open/` or `BUILTIN_PACKS` | ✅ cicd-starter in both |
| CICD-hygiene rules | ✅ CICD-001..004 in pack |

**Scope:**
- [x] **Pack**: `cicd-starter` (kind: quality), in `packs/open/cicd-starter/`
- [x] **Default**: cicd-starter when no `--pack` specified (PLG)
- [x] **Rules**: CICD-001 (event count); CICD-002 (`assay.profile.started`/`.finished`); CICD-003 (traceparent/tracestate/run_id); CICD-004 (build_id/version, info)
- [x] **BUILTIN_PACKS** + vendoring: packs/open vendored to crates/assay-evidence/packs/
- [ ] **Docs**: README per ADR-023 Appendix A; pinned GH Action; `--fail-on warning`; Next steps (follow-up)

**Design decisions:**
- `kind: quality` (no disclaimer; distinct from compliance packs)
- Light rules only—reuse existing check types: `event_count`, `event_pairs`, `event_field_present`
- Composable with eu-ai-act-baseline for teams graduating to compliance

### G. Sim Engine Hardening (P2)

Configurable verification limits and time budget enforcement for `assay sim`—resource-control best practices (OWASP-aligned, fail-fast under load).

```bash
assay sim run --suite quick --target bundle.tar.gz
assay sim run --limits '{"max_bundle_bytes": 10485760}' --target bundle.tar.gz
assay sim run --limits-file limits.json --target bundle.tar.gz
```

**Scope:**
- [ ] **CLI**: Parse `--limits` (JSON string) and `--limits-file` into `Option<VerifyLimits>`
- [ ] **Limits model**: Partial overrides merged with defaults; stable JSON schema; clear parse errors (invalid JSON → exit 3)
- [ ] **Integrity attacks**: Pass limits and `TimeBudget`; use `verify_bundle_with_limits`; budget check before each verify/iteration
- [ ] **Dynamic bundle_size**: Generate `limit + 1` bytes for `limits.bundle_size` attack (regression-proof)
- [ ] **Exit codes**: "Blocked by verify limits" distinguishable from bypass; attack correctly blocked = test success

**Design decisions:**
- Global suite `TimeBudget` (single deadline, not per-attack) for predictable CI behavior
- Budget-check points before every expensive operation (verify, decompress, loop iteration)
- `--limits-file` or `--limits @path` for CI/shell-friendly config (avoid JSON escaping)

---

## Q3 2026: Enterprise Scale (Growth)

**Objective:** Integration with the broader security ecosystem + agentic protocol support.

### A. Protocol Adapters (Adapter-First Strategy)

Lightweight adapters that map protocol-specific events to Assay's `EvidenceEvent` + policy hooks:

| Adapter | Protocol | Focus |
|---------|----------|-------|
| `assay-adapter-acp` | Agentic Commerce Protocol | OpenAI/Stripe checkout flows |
| `assay-adapter-ucp` | Universal Commerce Protocol | Google/Shopify commerce journeys |
| `assay-adapter-a2a` | Agent2Agent | Agent discovery/tasks/messages |

- [ ] **Adapter trait**: Common interface for protocol → EvidenceEvent mapping
- [ ] **ACP adapter**: Tool calls, checkout events, payment intents (leverages v2.11.0 mandate support)
- [ ] **UCP adapter**: Discover/buy/post-purchase state transitions
- [ ] **A2A adapter**: Agent capabilities, task delegation, artifacts

**Why adapters:** The market is fragmenting (ACP vs UCP vs AP2 vs x402). Assay's value is protocol-agnostic governance, not protocol lock-in.

**Enabled by v2.11.0:** The mandate evidence module provides the foundation for AP2-style authorization tracking in these adapters.

**AAIF governance note:** MCP and A2A are now under the Agentic AI Foundation (Linux Foundation, Dec 2025). This reduces protocol fragmentation risk and makes adapter investments more durable.

### B. Connectors
- [ ] **SIEM**: Splunk / Microsoft Sentinel export adapters
- [x] **CI/CD**: GitHub Actions v2 ([Rul1an/assay/assay-action@v2](https://github.com/marketplace/actions/assay-ai-agent-security)) / GitLab CI integration
- [ ] **GitHub App**: Native policy drift detection in PRs
- [ ] **GitLab CI**: Native integration
- [ ] **OTel GenAI**: Align evidence export with [OTel GenAI semantic conventions](https://opentelemetry.io/docs/specs/semconv/gen-ai/) — conventions still experimental but Pydantic AI already follows them; monitor for stability before building bridge

### C. Additional Compliance Packs
- [ ] **SOC 2 Pack**: Control mapping for Type II audits
- [ ] **MCPTox**: Regression testing against jailbreak/poisoning patterns
- [ ] **Industry Packs**: Healthcare (HIPAA), Finance (PCI-DSS)

### D. Managed Evidence Store (Evaluate)

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

### B. Assurance & Audit Readiness (If Managed Store Exists)
- [ ] **Policy Exceptions**: Waivers with expiry, owner, rationale; audit trail for compliance exceptions
- [ ] **Auditor Portal**: Read-only export of packs + results + fingerprints; "audit-ready bundles" for external auditors

### C. Advanced Signing & Attestation
- [ ] **Sigstore Keyless**: Fulcio certificate + Rekor transparency log
- [ ] **SCITT Integration**: Transparency log for signed statements (IETF draft)
- [ ] **Org Trust Policies**: Managed identity verification

### D. Identity & Authorization Stack

Enterprise identity for agentic workloads:

- [ ] **SPIFFE/SPIRE**: Workload identity for non-human actors
- [ ] **FAPI 2.0 Profile**: High-security OAuth for agent commerce APIs
- [ ] **OpenID4VP/VCI**: Verifiable credentials for mandate attestation
- [ ] **OAuth 2.0 BCP (RFC 9700)**: DPoP sender-constrained tokens

**Why:** AP2/UCP mandate flows require provable authorization. FAPI/OpenID4VP are the emerging standards.

### E. Managed Isolation (Future)
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

### Pack Marketplace (Future)
- [ ] **Partner packs**: Third-party packs via marketplace (rev share model)

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

Assay follows the **open core model** (Semgrep pattern): engine + baseline packs are open source, managed workflows + pro packs are enterprise.

See [ADR-016: Pack Taxonomy](./architecture/ADR-016-Pack-Taxonomy.md) for formal definition.

### Open Source (Apache 2.0)

Everything needed to create, verify, and analyze evidence locally:

| Category | Components |
|----------|------------|
| **Evidence Contract** | Schema v1, JCS canonicalization, content-addressed IDs, deterministic bundles |
| **CLI Workflow** | `export`, `verify`, `lint`, `diff`, `explore`, `show` |
| **BYOS Storage** | `push`, `pull`, `list` with S3/Azure/GCS/local backends |
| **Basic Signing** | Ed25519 local key signing and verification (v2.9.0) |
| **Pack Engine** | `--pack` loader, composition, SARIF output, digest verification (v2.10.0) |
| **Baseline Packs** | `eu-ai-act-baseline` (Article 12 mapping, v2.10.0), future `soc2-baseline` |
| **Mandate Evidence** | Mandate types, signing, runtime enforcement, CloudEvents lifecycle (v2.11.0) |
| **Runtime Security** | Policy engine, MCP proxy, eBPF/LSM monitor, mandate authorization |
| **Developer Experience** | Python SDK, pytest plugin, GitHub Action |
| **Output Formats** | SARIF, JUnit, JSON, console, NDJSON (audit/decision logs) |

**Why open:** Standards adoption requires broad accessibility. The evidence format and baseline compliance checks should become infrastructure, not a product moat.

### Enterprise Features (Commercial)

Governance workflows and premium compliance for organizations:

| Category | Components |
|----------|------------|
| **Identity & Access** | SSO/SAML/SCIM, RBAC, teams, approval workflows |
| **Pro Compliance Packs** | `eu-ai-act-pro` (biometric rules, PDF reports), `soc2-pro`, industry packs — assurance depth + maintained mappings + auditor-friendly reporting |
| **Managed Workflows** | Exception approvals, policy exceptions (waivers with expiry/owner/rationale), scheduled scans, compliance dashboards |
| **Auditor Portal** | Read-only export, audit-ready bundles, packs + results + fingerprints (when Managed Store exists) |
| **Advanced Signing** | Sigstore keyless, transparency log verification, org trust policies |
| **Managed Storage** | WORM retention, legal hold, compliance attestation |
| **Integrations** | SIEM connectors (Splunk/Sentinel), OTel pipeline templates |
| **Fleet Management** | Policy distribution, runtime agent management |

**Principle:** Gate *workflow scale* and *org operations*, not basic compliance checks. The "workflow moat" strategy: engine free, baseline free, managed workflows paid.
