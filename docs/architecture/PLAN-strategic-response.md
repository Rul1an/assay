# Strategic Plan: Assay Positioning & Development Priorities

**Status:** Approved
**Context:** Response to external critical validation report (Feb 2026)
**Scope:** Product positioning, development priorities, risk mitigation

---

## Executive Summary

Assay's differentiation is the **closed-loop governance workflow**: observe agent behavior → generate policy → profile for stability → lock → CI replay gate → verifiable evidence → signed compliance packs. No competitor (OPA, Cedar, MCPTrust, LiteLLM, ToolHive) offers this end-to-end. That is the claim to make — not "only MCP-native tool."

The development direction has three wedges (where the market can't easily copy) and three explicit "don't do" zones:

**Build on:**
1. Trace replay + CI gating (the wedge — highest switching cost, best DX)
2. Evidence bundles as compliance primitive (CloudEvents + BYOS + SIEM export)
3. Pack registry + lockfiles as supply-chain discipline for policy content

**Don't prioritize:**
- Kernel enforcement as primary pitch (keep as defense-in-depth)
- Replacing OPA/Cedar in enterprise authz expressivity
- Unverifiable performance claims

**Positioning line:**
> "End-to-end MCP governance pipeline: trace capture → policy generation → deterministic CI replay gating → verifiable evidence bundles → signed/locked compliance packs."

---

## Report Validation: What's Correct and How to Respond

### 1. "Only MCP-native tool" — Remove

The claim is not tenable in 2026. MCPTrust (firewall/allowlist/drift), GitHub (allowlist in Copilot org policy), Teleport (deny-by-default RBAC), LiteLLM (tool/server permissions), and ToolHive (Cedar-based) all provide MCP governance.

**Action:** Replace all "only/enige/eerste" positioning with testable differentiation: the end-to-end pipeline. This is concrete: no competitor covers the full trace → generate → profile → lock → CI gate → evidence → pack → audit export chain.

### 2. "Microsecond overhead" — Deprioritize, Then Prove

Without a public benchmark protocol, this is marketing. In agentic workflows, network/tool/LLM-latency dominate. Enforcement-latency is rarely the bottleneck.

**Action:** Position performance as "nice but not the point." The real value is correctness and governance. Then publish a reproducible benchmark artifact (workload + p99/p999, cold/warm, I/O, trace-size) to back it up — exactly how modern security/runtime projects maintain credibility. The internal infra (Bencher, criterion, forensic tail-latency) already exists; make it public.

### 3. Tamper-evident != Completeness — Document Honestly

Bundle integrity (what's in it hasn't been modified) is not the same as collection completeness (everything was captured). A compromised host can selectively log.

**Action:** Frame tamper-evidence as deployment guidance, not core claim: "Bundles are tamper-evident. For audit-grade completeness, deploy with append-only BYOS storage + independent timestamping." Publish an Evidence Bundle Threat Model document.

### 4. Compliance Packs != Legal Shield — Frame as Evidence Structuring

EU AI Act obligations are time-phased (GPAI Aug 2025, full application Aug 2026, regulated products Aug 2027). Packs help structure engineering evidence early — they don't deliver compliance.

**Action:** Every compliance pack gets a scope document: what it covers, what it doesn't cover, what organizational controls are additionally needed. Frame as "evidence structuring" throughout all copy.

### 5. Kernel Enforcement — Defense in Depth Only

Landlock is explicitly rootless sandboxing but remains environment-sensitive. eBPF/LSM requires privileges and varies by distro/kernel. Not a primary pitch.

**Action:** Keep as "optional Linux hardening." No new feature investment without signed enterprise demand. Move sandbox/monitor to "Advanced" section in docs.

---

## Where the Report Underestimates Assay

### The Closed-Loop Workflow is a Product Paradigm, Not a Feature

The report treats "trace replay + CI gating" as one feature among many. It's the core paradigm:

```
observe → generate → profile → lock → gate → evidence → audit
```

OPA/Cedar provide policy *engines*. Assay provides a policy *lifecycle* for agent governance. This distinction matters for positioning and must be the central thesis of all external communication.

### Hybrid OPA/Cedar + Assay is Correct but Operationally Expensive

Two policy engines = double the maintenance, debugging, and synchronization overhead. The report should explicitly differentiate:

- **Greenfield MCP:** Start Assay-only. Deny-by-default, trace-driven governance. Graduate to OPA/Cedar later if enterprise ABAC/RBAC becomes a hard requirement.
- **Existing OPA/Cedar stack:** Add Assay as CI/evidence layer. OPA/Cedar stays the runtime PDP; Assay handles agent governance, regressie-gating, and audit evidence.

---

## Development Priorities

### P0: Positioning Reframe

**Deliverables:**
- [ ] Audit all public copy for "only"/"enige"/"eerste" claims — replace with end-to-end pipeline framing
- [ ] Add feature comparison table (Assay vs OPA vs Cedar vs MCPTrust) to docs — focus on workflow coverage, not feature checkboxes
- [ ] Update README.md tagline and docs/index.md subtitle to reflect pipeline positioning
- [ ] Update conductor/product.md core features to emphasize closed-loop workflow

### P0: Trace Replay + CI Gating (The Wedge)

**Why:** Highest switching cost, best DX, no competitor equivalent. The "record → replay → validate" paradigm is what makes Assay sticky.

**Deliverables:**
- [ ] `assay init --from-trace ci.jsonl` — generates working config + first policy + CI workflow in one command. Target: time-to-first-PR-gate < 5 minutes.
- [ ] SARIF/Code Scanning showcase — prominent in docs and README. Screenshot/demo of a blocked PR from a policy violation.
- [ ] Public benchmark report with reproducible protocol (workload definition, transport, cold/warm, p99/p999, trace-size). Honest framing: enforcement-latency is not the bottleneck; correctness is the value.

### P1: Evidence Bundles as Compliance Primitive

**Deliverables:**
- [ ] Evidence Bundle Threat Model document (public) — what bundles guarantee (integrity, non-repudiation), what they don't (completeness, collection controls), deployment guidance for audit-grade (append-only BYOS, timestamping)
- [ ] SIEM export target — at minimum one of: Splunk HEC, S3+Athena-ready format, generic OTLP export. The OTel JSONL export is a start; make it a full pipeline.
- [ ] RFC 3161 timestamping as opt-in (stretch goal for high-trust deployments)

### P1: Pack Registry + Supply-Chain Discipline

**Deliverables:**
- [ ] Per compliance pack: scope document (what articles/criteria it covers, what it doesn't, required organizational controls)
- [ ] Core/baseline packs under open governance (community-extensible). Enterprise/vertical packs remain commercial.
- [ ] SLSA provenance (Level 2+) for pack releases and CLI releases

### P2: Complementary Positioning vs OPA/Cedar

**Deliverables:**
- [ ] Architecture Guide documenting three deployment scenarios:
  - A: Greenfield MCP → Assay-only
  - B: Existing OPA/Cedar → Assay as CI/evidence layer
  - C: Enterprise compliance → hybrid (OPA/Cedar for authz, Assay for governance + evidence)
- [ ] OPA integration guide (decision logs + Assay evidence side-by-side)
- [ ] Cedar integration guide (ToolHive pattern as reference)
- [ ] Explicit framing: "Assay governs agent behavior & tool drift; OPA/Cedar governs identity/context authorization"

### P2: Trust & Maturity

**Deliverables:**
- [ ] SLSA provenance for all releases (low-effort via GitHub Actions SLSA generators)
- [ ] Public roadmap on GitHub Projects
- [ ] Evaluate governance model for core specs (bundle format, pack manifest, policy schema) — consider neutral governance aligned with broader MCP standardization movement (Q3 2026)

### P3: Kernel Enforcement (Maintenance Only)

**Deliverables:**
- [ ] Move sandbox/monitor to "Advanced" section in docs
- [ ] Document deployment requirements (kernel version, capabilities, distro support matrix)
- [ ] No new feature investment without signed enterprise demand

---

## Concrete Changes to Existing Docs

### README.md
- Line 8-9: Add "End-to-end governance pipeline" framing below "Policy-as-Code for AI Agents"
- Section "3. Evidence Bundles": Add one line on deployment guidance for audit-grade (BYOS + integrity verification)
- Section "4. Compliance Packs": Add disclaimer line: "Packs structure engineering evidence — they do not constitute legal compliance"
- Section "Kernel-Level Sandbox": Rename to "Defense in Depth: Kernel Sandbox (Linux)" or move below main workflow sections

### docs/index.md
- Line 11: Extend description to mention the closed-loop workflow
- Compliance Packs card: Add "evidence structuring" framing
- Runtime Enforcement section: Add "(Linux, optional)" qualifier

### docs/open-core.md
- Add row for "Pack Provenance" in the comparison table (SLSA, reproducible builds)
- Add note on community governance for baseline packs

### conductor/product.md
- Reframe core features around the closed-loop workflow paradigm
- Add "evidence structuring" language to compliance bullet

---

## Prioritization Matrix

| Prio | Item | Impact | Effort | Rationale |
|------|------|--------|--------|-----------|
| P0 | Positioning reframe | High | Low | Credibility blocker |
| P0 | `init --from-trace` quick-start | High | Medium | Adoption bottleneck |
| P0 | SARIF/Code Scanning showcase | High | Low | DX magnet, already built |
| P1 | Evidence threat model doc | High | Low | Security evaluator credibility |
| P1 | Pack scope documents | High | Low | Honest framing prevents disappointment |
| P1 | Public benchmark protocol | Medium | Medium | Makes performance claim verifiable |
| P1 | SLSA provenance | Medium | Low | Low-effort trust signal |
| P2 | SIEM export pipeline | High | Medium | Enterprise adoption enabler |
| P2 | Architecture Guide (3 scenarios) | Medium | Medium | Positions complementary vs OPA/Cedar |
| P2 | OPA/Cedar integration guides | Medium | Medium | Complementary positioning |
| P3 | Community packs governance | Medium | Medium | Lock-in mitigation |
| P3 | Spec governance evaluation | Medium | High | Strategic decision, Q3 2026 |
| P3 | Kernel enforcement docs cleanup | Low | Low | Maintenance only |
