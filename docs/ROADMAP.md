# Assay Roadmap 2026

> **Status sync (2026-03-27, K1 formalized):** Q1 DX/refactor convergence is closed on `main` (RFC-001/002/003/004).
> Evidence-as-a-product (ADR-025), protocol adapters (ADR-026), and MCP governance/enforcement (ADR-032 Wave24-Wave42) are materially implemented on `main`.
> Governance support ADRs [ADR-027](architecture/ADR-027-Tool-Taxonomy.md) through [ADR-031](architecture/ADR-031-Coverage-v1.1-DX-Polish.md) are implemented on `main` and should be read as delivered contracts, not pending proposals.
> **BYOS truth (ADR-015):** Phase 1 is complete on `main`: `push`, `pull`, `list`, `store-status`, `.assay/store.yaml` config, and provider quickstart docs (S3, B2, MinIO) are all shipped.
> Split refactor program is closed loop through Wave7C Step3 on `main` (see [plan](architecture/PLAN-split-refactor-2026q1.md), [report](architecture/REPORT-split-refactor-2026q1.md), [program review pack](contributing/SPLIT-REVIEW-PACK-2026q1-program.md)).
> `E1`, `G1`, `G2`, `P1`, `T1a`, `T1b`, and `G3` are merged on `main`; workspace version is **`3.6.0`** and OWASP signal-aware pack floors align to `>=3.2.3`.
> **Sequence:** `T1a → T1b → G3 → P2a → H1 → P2b` shipped on `main` (**`a2a-signal-followup`** first publicly released in **`v3.3.0`**). **`G4-A` Phase 1** (A2A `payload.discovery` evidence seam), **`P2c`** — built-in **`a2a-discovery-card-followup`** (A2A-DC-001 / A2A-DC-002), and **`K1-A` Phase 1** (`payload.handoff`) are part of the public **`v3.4.0`** line. **`K2-A` Phase 1** (MCP authorization-discovery evidence) first became public in **`v3.5.0`** and is carried forward through **`v3.5.1`**, which also adds the honest official-MCP-Registry publication foundation for `assay-mcp-server`. The prepared **`v3.6.0`** line adds the first external-eval receipt lane: selected eval outcomes can enter the trust-compiler path as bounded receipts, starting with Promptfoo assertion-component results. **G4-A** follow-up remains post-merge verification / release-truth hygiene only (no new G4 semantics). **`K1`** remains shipped as the active A2A evidence-first line, adapter-first and explicitly **not** a pack wave. **`K2`** is now the active public MCP authorization-discovery line, still evidence-first and CI-native, and still **not** a pack in the same slice — see [PLAN-K1](architecture/PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md), [PLAN-K2](architecture/PLAN-K2-MCP-AUTHORIZATION-DISCOVERY-EVIDENCE-2026q2.md), [K1-A freeze](architecture/K1-A-PHASE1-FREEZE.md), [K2-A freeze](architecture/K2-A-PHASE1-FREEZE.md), and [K2-A freeze prep](architecture/K2-A-PHASE1-FREEZE-PREP.md). References: [PLAN-T1b](architecture/PLAN-T1b-TRUST-CARD-2026q2.md), [PLAN-G3](architecture/PLAN-G3-AUTHORIZATION-CONTEXT-EVIDENCE-2026q2.md), [PLAN-H1](architecture/PLAN-H1-TRUST-KERNEL-ALIGNMENT-RELEASE-HARDENING.md), [MIGRATION-TRUST-COMPILER-3.2.md](architecture/MIGRATION-TRUST-COMPILER-3.2.md).
> **Shipped (trust compiler line):** **`P2a`** **`mcp-signal-followup`**, **`H1`** alignment, **`P2b`** **`a2a-signal-followup`** — see [PLAN-P2b](architecture/PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md), §Q3 2026, [RFC-005](architecture/RFC-005-trust-compiler-mvp-2026q2.md) §6. Optional small **test-only** follow-ups (e.g. shared `tests/common` bundle builders for H1/P2a parity) do not change product semantics.
> **Evidence portability (P31-P45b):** Prepared for the post-`v3.6.0` trust-compiler line: supported external eval outcomes, runtime decision details, and inventory/provenance surfaces can enter the trust-compiler path as bounded receipts. The first three families are Promptfoo assertion-component results, OpenFeature boolean `EvaluationDetails`, and CycloneDX ML-BOM model components. These are downstream evidence-portability lanes, not integration, partnership, correctness, safety, or compliance-truth claims — see the [receipt family matrix](reference/receipt-family-matrix.json) and [From Promptfoo JSONL to Evidence Receipts](notes/FROM-PROMPTFOO-JSONL-TO-EVIDENCE-RECEIPTS.md).
> **Follow-up engineering (parallel, small):** optional alignment between `assay-core` auth-context normalization/redaction and `assay-evidence` trust-basis classification (shared predicates and/or cross-crate tests) to prevent drift — **not** a blocker for `P2`.

**Strategic Focus:** CI-Native Protocol Governance, Agent Runtime Evidence & Control Plane.
**Core Value:** Canonical Evidence + Protocol-Aware Policy Checks for Agent Systems.

## Executive Summary

Assay is evolving from an evidence recorder into a **CI-native evidence compiler and protocol-governance layer for agent systems**. The product thesis is no longer "help teams inspect traces better"; it is "compile agent runtime truth into canonical evidence, protocol-aware policy checks, and verifiable security claims with explicit evidence levels." Assay turns traces, protocol events, and bundle artifacts into canonical evidence, signal-aware packs, SARIF findings, offline-verifiable bundles, and trust artifacts such as a signed Trust Card.

This keeps Assay out of the wrong competitive lane. Promptfoo, Langfuse, LangSmith, and vendor eval platforms already own large parts of evals, dashboards, and red-team loops. Assay's moat is different: **claim provenance, bounded security semantics, and portable proof-bearing outputs**.

**Standards Alignment:**
- **CloudEvents v1.0** envelope — lingua franca for event routers and SIEM pipelines
- **W3C Trace Context** (`traceparent`) — correlation with existing distributed tracing
- **SARIF 2.1.0** — GitHub Code Scanning integration with explicit `automationDetails.id` uniqueness/stability contract
- **EU AI Act Article 12** — record-keeping requirements make "evidence" commercially relevant; pack mappings are pinned to EUR-Lex text, and phased dates are treated as guidance
- **OTel GenAI Semantic Conventions** — vendor-agnostic observability bridge for LLM/agent workloads; conventions are evolving, so integrations are version-pinned with mapping tests
- **ENISA / SBOM / SLSA** — Supply-chain assurance (SBOM, provenance, attestation) aligns with ENISA priorities; SLSA-aligned attestation per ADR-018

### 2026 Product Thesis: Claims-as-Code For Agent Systems

Assay should be understood as:

- **Input**: OTel spans, protocol events, Assay traces, and proof-bearing bundle artifacts
- **Compile**: canonical evidence + bounded claim classification + pack evaluation
- **Output**: findings, SARIF, verifiable bundles, and eventually a signed Trust Card

`OTel-native` does **not** mean "OTel semconv is the only truth." The stable truth layer remains Assay's canonical evidence contract. OTel is a first-class ingest path and ecosystem bridge, but it must compile into Assay's own evidence model before stronger trust claims are made.

The core differentiator is not "more detections." It is **better claim epistemology**:

| Evidence Level | Meaning |
|----------------|---------|
| `verified` | Backed by direct evidence or offline verification in the bundle/runtime path |
| `self_reported` | Emitted by the system itself without stronger independent corroboration |
| `inferred` | Derived from bounded, documented interpretation rules |
| `absent` | No trustworthy evidence currently supports the claim |

This is the line that recent bounded waves now support on `main`:

- `E1` unlocked a small, typed engine seam rather than a broad policy language.
- `G1` made supported weaker-than-requested containment fallback visible in evidence.
- `G2` made explicit delegation context visible on supported decision evidence.
- `P1` productized those signals in a companion pack without broadening the baseline.
- `R2` closed the only post-`P1` release-line mismatch by moving the workspace and OWASP pack floors to `3.2.3`.
- `T1a` now ships the first canonical trust-basis compiler output on `main` as deterministic `trust-basis.json` derived from verified bundles.

See [ADR-033](architecture/ADR-033-OTel-Trust-Compiler-Positioning.md) for the product-positioning decision and [RFC-005](architecture/RFC-005-trust-compiler-mvp-2026q2.md) for the bounded MVP execution frame.

### North Star Guardrails

- **Claim-first, not dashboard-first**: a prettier trace UI is not the product wedge. The wedge is evidence-classified trust claims.
- **Canonical evidence first**: OTel is an ingest bridge, not the sole semantic authority.
- **Canonical evidence wins operationally**: new ingest paths may enrich or map into canonical evidence, but they must not semantically override claim classification directly from raw upstream formats.
- **Trust Card, not trust score**: the primary artifact must show what is `verified`, `self_reported`, `inferred`, or `absent`, not collapse into `trusted/untrusted`.
- **No aggregate trust score in MVP**: no primary scalar trust score, no `safe/unsafe` badge, and no maturity badge as the main output.
- **Fixed order**: `T1a → T1b → G3 → P2a → H1` before broadening protocol packs (`P2b`+); that ordering stays ahead of dashboard work, unconstrained pack expansion, or heavier reference/temporal semantics.
- **No broad correctness claims**: delegation validation, chain integrity, sandbox correctness, and temporal correctness remain out of scope until dedicated evidence and semantics exist.
- **Anti-scope rule**: Assay is not a tracing platform, eval platform, or observability dashboard. Those may be integration surfaces, but not the product category.

### Strategic Fit Test

This direction should continue only if all three answers stay "yes":

1. **External demand fit**
   The external line in 2026 is identity/authz, auditability, protocol-level defenses, and bounded deployment controls for agents. Assay fits that line better as a trust compiler than as a generic eval or observability tool.

2. **Repo capability fit**
   Assay already ships the substrate this direction needs: canonical evidence, offline verification, signal-aware packs, proof-bearing bundles, OTel ingest, delegation context, and containment degradation signals.

3. **Wedge fit vs alternatives**
   A Trust Card and trust-compiler story differentiate Assay better than another pack wave, another engine feature, or a broader dashboard surface. Those alternatives are easier to explain, but they are also where the category is more crowded and Assay is less structurally unique.

If any of these answers turns into "no", the default action is to stop a broader product-positioning wave:

- if **external demand fit** is weak, do not broaden packaging or positioning
- if **repo capability fit** is weak, close the missing signal/engine seam first
- if **wedge fit** is weak, do not start a new product lane until the differentiator is sharper

### Primary Risks

- **Abstract product story**: "trust compiler" is less immediately legible than "eval" or "observability." The Trust Card is the required wedge that makes the compiler story tangible.
- **Category confusion**: if Assay is marketed as a tracing platform, dashboard, firewall, or generic eval suite, it loses the category it is best positioned to own.
- **Standards churn**: OTel GenAI and agent semantic conventions are still evolving. Assay must keep its canonical evidence layer stable and treat OTel as ingest, not truth.

---

## Strategic Positioning: Protocol-Agnostic Governance

The protocol landscape table below is a planning snapshot (hypothesis-driven) and is revisited as specs/programs evolve. It is a monitoring frame for protocol-agnostic governance, not a commitment matrix that Assay will pursue every protocol surface equally or simultaneously.

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

**2026 runtime-governance positioning:** Recent fragmented-IPI experiments on `main` sharpen this moat. Assay's value is not "better regex"; it is deterministic governance on the tool bus plus canonical evidence for what the agent runtime actually exposed. Wrap-only lexical enforcement is useful but brittle against multi-step leakage and tool-hopping. Sequence/state policies stay robust because they govern behavioral routes across sink labels and payload variants. The bounded claim remains important: these experiments demonstrate sink-call exfiltration control with audit-grade evidence and low decision overhead, not a universal solution to semantic hijacking or raw network egress.

### Why This Matters

1. **Tool Signing** becomes critical: "tool substitution" and "merchant tool spoofing" are real commerce risks
2. **Mandates/Intents** need audit trails: AP2's authorization model requires provable evidence
3. **Agent Identity** is enterprise-core: who/what authorized a transaction?

The protocol landscape table above is the public planning summary for this area; deeper protocol research currently remains internal.

### Market Validation (Feb 2026)

The CI/CD-for-agents market is validating Assay's core assumptions:

- **AAIF (Agentic AI Foundation)**: MCP, goose and AGENTS.md are under Linux Foundation governance (Dec 2025). MCP as a vendor-neutral standard reduces protocol fragmentation risk and supports Assay's MCP-first bet.
- **GitHub "Continuous AI"** (Feb 2026, evolving/preview signal): repo-agents with read-only default + "Safe Outputs" — explicit contracts defining what agents may produce. This aligns with Assay's policy-as-code model.
- **Policy-as-code as best practice**: Multiple sources (V2Solutions, Skywork, Gartner) now list policy-as-code, least privilege, auditability and kill switches as enterprise requirements for agent deployment. Not a niche compliance need anymore.
- **Fleet-of-small-agents pattern**: The dominant deployment pattern is many small specialized agents, not one generalist. More agents = more policies = more Assay usage per repo.
- **Gartner risk signal**: >40% of agentic AI projects will be cancelled by end 2027 due to costs, unclear value, or inadequate risk controls. Governance tooling is a prerequisite, not a nice-to-have.

**Competitive differentiation**: Agent CI (eval-as-service), Langfuse/LangSmith (observability), and agent runtimes such as Dagger cover adjacent layers. Assay's differentiator is the combination of deterministic replay, integrity-bearing evidence bundles, and bounded claim packs. The unique position is governance + audit, not observability or eval-as-service.

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

Historical close-out note:
- Q1 close-out is complete on `main`
- the active product lane is now the trust-compiler line described above
- detailed historical delivery status lives in the relevant ADRs and supporting docs rather than in the roadmap head

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
- **Not universal semantic-hijacking detection**: LLM-as-judge or probabilistic semantic gates are not the core enforcement model. Assay stays deterministic and evidence-first.
- **Not a full outbound egress firewall (yet)**: raw network containment belongs to OS/runtime isolation layers. Assay governs tool routes and records policy decisions; it does not replace platform egress controls as an MVP.
- **Not a magic trust score**: primary outputs should stay evidence-classified (`verified`, `self_reported`, `inferred`, `absent`), not collapse into a single opaque number.

## Post-P1 Product Lane (March 2026, Historical)

The substrate that was on `main` after `P1` was strong enough to shift from "more packs" toward a clearer product line. This lane is historical context; the active trust-compiler line is now `K1` / `K1-A` above.

| Order | Lane | Why now | Boundary |
|-------|------|---------|----------|
| **1** | `T1a` OTel-native Trust Compiler MVP | ✅ Merged on `main`: verified bundle -> deterministic `trust-basis.json` with bounded claim classification. | Kept small: no Trust Card rendering, no new signals, no packs, no score-first surface. |
| **2** | `T1b` Trust Card MVP | ✅ Merged on `main`: deterministic `trustcard.json` + `trustcard.md` from verified bundles (`assay trustcard generate --out-dir`), derived from T1a only. | Evidence-classified output; signed/attestation later; no generic risk score. |
| **3** | `G3` Authorization Evidence Signal | ✅ Merged on `main`: bounded `auth_scheme` / `auth_issuer` / `principal` on policy-projected MCP decision evidence; Trust Basis + Trust Card schema `2` / seven claims (see [PLAN-G3](architecture/PLAN-G3-AUTHORIZATION-CONTEXT-EVIDENCE-2026q2.md)). | Supported flows only; no auth validation, issuer trust proof, scope checks, or temporal correctness. Optional follow-up: reduce core ↔ evidence drift (normalization vs classification). |
| **4** | `P2` Protocol Claim Packs | Historical next lane at that point — protocol-aware claim packs as honest product surfaces once auth context became visible in evidence. | Small MCP/A2A claim packs, not broad compliance theater. |
| **Later** | Reference/temporal/capability attestation | These semantics are valuable but heavier. | Ship only after the claim product line is stable. |

**G3 closure (content):** Policy-projected authorization context lands on decision evidence in a frozen supported flow, is redacted, and flows through to Trust Basis and Trust Card without overclaiming validation or issuer trust. The main remaining engineering nuance is **core/evidence alignment** (above) as a hardening pass, not a G3 scope gap.

Work that primarily improves dashboarding, generic observability UX, or score-first reporting should be treated as secondary until this sequence is complete.

---

## Delivered Foundation (Historical)

The roadmap above is the live decision path. Historical delivery detail remains important, but it no longer belongs in the main decision flow.

Closed lines on `main`:

- **Evidence Contract v1**: canonical evidence, offline verification, SARIF, diff/explore, and OTel trace context are shipped
- **OTel ingest and evidence pipeline**: `trace ingest-otel` plus the `ProfileCollector -> EvidenceMapper -> EvidenceEvent` collector-style pipeline are shipped
- **Supply chain and governance surfaces**: BYOS Phase 1, tool signing, GitHub Action v2/v2.1, mandate evidence, and starter/baseline pack infrastructure are shipped
- **Evidence-as-a-product**: soak, closure, completeness, and the OTel bridge slices from ADR-025 are shipped
- **External eval receipts**: P31-P39 are merged on `main` as the first evidence-portability lane for selected external eval outcomes, starting with Promptfoo assertion components and ending in Trust Basis diff / Harness reporting without importing full eval-run truth
- **Inventory receipts**: P43/P45 add the first inventory/provenance receipt lane, starting with CycloneDX ML-BOM model components and a bounded Trust Basis boundary-visibility claim without importing full BOM, model-card, dataset, graph, or compliance truth
- **Bounded claim waves**: `E1`, `G1`, `G2`, `P1`, `T1a`, `T1b`, `G3`, and the `3.2.3` release-line truth fix are shipped

For detailed historical delivery records, see the relevant ADRs and companion docs:

- [ADR-015](./architecture/ADR-015-BYOS-Storage-Strategy.md)
- [ADR-017](./architecture/ADR-017-Mandate-Evidence.md)
- [ADR-018](./architecture/ADR-018-GitHub-Action-v2.1.md)
- [ADR-023](./architecture/ADR-023-CICD-Starter-Pack.md)
- [ADR-025](./architecture/ADR-025-Evidence-as-a-Product.md)
- [ADR-026](./architecture/ADR-026-Protocol-Adapters.md)
- [ADR-032](./architecture/ADR-032-MCP-Policy-Obligations-and-Evidence-v2.md)

---

## Q3 2026: Trust Compiler Productization

**Objective:** Productize Assay as an OTel-native trust compiler and make protocol-aware claims portable before expanding dashboards or broader enterprise surfaces.

### Trust Compiler Core (Highest Priority)

March 2026 evidence and signal waves change the ordering inside Q3. `T1a`, `T1b`, `G3`, **`P2a`**, **`H1`**, **`P2b`** (`a2a-signal-followup`), **`G4-A` Phase 1** (A2A `payload.discovery`), **`P2c`** (`a2a-discovery-card-followup`), and **`K1-A` Phase 1** (`payload.handoff`) are public in **`v3.4.0`**. **`K2-A` Phase 1** is now public in **`v3.5.0`**. **G4-A** track: post-merge verification / release-truth hygiene only. **`K1`** remains the shipped A2A evidence-first line; any widening beyond `K1-A` Phase 1 still needs a new bounded decision. **`K2`** is now the active public bounded MCP authorization-discovery wave — see [PLAN-K1](architecture/PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md), [K1-A freeze](architecture/K1-A-PHASE1-FREEZE.md), [PLAN-K2](architecture/PLAN-K2-MCP-AUTHORIZATION-DISCOVERY-EVIDENCE-2026q2.md), [K2-A freeze](architecture/K2-A-PHASE1-FREEZE.md), and [K2-A freeze prep](architecture/K2-A-PHASE1-FREEZE-PREP.md).

| Priority | Capability | Why now | MVP boundary |
|----------|------------|---------|--------------|
| **P0** | `T1a` OTel-native Trust Compiler MVP | ✅ Shipped on `main`: verified bundle → `trust-basis.json` / bounded claim classification. | Compiler inputs/outputs, claim basis export; no dashboard-first surface. |
| **P0** | `T1b` Trust Card MVP | ✅ Shipped on `main`: portable `trustcard.json` + `trustcard.md` for review and diff. | Evidence-level rows + frozen non-goals; no opaque global score. |
| **P1** | `G3` Authorization Evidence Signal | ✅ Shipped on `main`: authorization context fields on supported MCP decision evidence; Trust Card schema `2` / seven trust-basis claims. | Supported flows only; no cryptographic or temporal auth-validation semantics. |
| **P1** | `P2a` MCP companion pack | ✅ Shipped on `main`: built-in `mcp-signal-followup` (MCP-001..003). | G3-aligned lint; see [PLAN-P2a](architecture/PLAN-P2a-MCP-SIGNAL-FOLLOWUP-CLAIM-PACK.md). |
| **P1** | `H1` Trust kernel alignment & release hardening | ✅ Shipped on `main`: migration SSOT, alignment tests, docs; no new signals. | [PLAN-H1](architecture/PLAN-H1-TRUST-KERNEL-ALIGNMENT-RELEASE-HARDENING.md), [MIGRATION-TRUST-COMPILER-3.2.md](architecture/MIGRATION-TRUST-COMPILER-3.2.md). |
| **P1** | `K1` A2A handoff / delegation-route evidence | Public in **`v3.4.0`** via `K1-A` Phase 1: bounded route / handoff seam before any further pack slice. | Adapter-first, visibility-only, no pack in the same wave; see [PLAN-K1](architecture/PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md) and [K1-A freeze](architecture/K1-A-PHASE1-FREEZE.md). |
| **P1** | `K2` MCP authorization-discovery evidence | Active public evidence wave after `K1-A`: first-class visibility for MCP authorization-discovery surfaces before any auth-discovery pack. `K2-A` Phase 1 is now public in **`v3.5.0`**. | Evidence-first, CI-native, no pack or engine bump in the same wave; see [PLAN-K2](architecture/PLAN-K2-MCP-AUTHORIZATION-DISCOVERY-EVIDENCE-2026q2.md), [K2-A freeze](architecture/K2-A-PHASE1-FREEZE.md), and [K2-A freeze prep](architecture/K2-A-PHASE1-FREEZE-PREP.md). |
| **P1** | `P2` Protocol Claim Packs (further slices) | ✅ **`P2b`** `a2a-signal-followup` (A2A-001..003) in **`v3.3.0`**. ✅ **`G4-A` Phase 1** and ✅ **`P2c`** `a2a-discovery-card-followup` (A2A-DC-001..002) are now public in **`v3.4.0`** (`requires >=3.3.0`). **Next:** only after new first-class evidence is established. | [PLAN-P2b](architecture/PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md); [PLAN-P2c](architecture/PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md); [PLAN-G4](architecture/PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md); [G4-A freeze](architecture/G4-A-PHASE1-FREEZE.md). |
| **P2** | Collector processor / sidecar form factor | This is the "outside-the-box" deployment surface that competitors are not targeting. | OTel-native compile path that emits canonical evidence, not a dashboard. |

These items outrank growth-only work because Assay's strongest differentiator is now trace -> evidence -> claim -> proof, not surface-area expansion.

### T1 MVP Non-Goals

For `T1a` and `T1b`, the roadmap stays explicit about what the MVP does **not** do:

- no aggregate trust score
- no `safe/unsafe` badge
- no direct claim classification from raw OTel spans
- no protocol-wide correctness claims
- no dashboard-first product surface

### T1 Mapping Rule

- canonical evidence schema remains the stable product contract
- OTel and protocol mappings are version-pinned bridges into that contract
- every upstream semconv or protocol mapping bump must come with explicit mapping tests

### A. Protocol Adapters (Adapter-First Strategy)

Lightweight adapters that map protocol-specific events to Assay's `EvidenceEvent` + policy hooks:

| Adapter | Protocol | Focus |
|---------|----------|-------|
| `assay-adapter-acp` | Agentic Commerce Protocol | OpenAI/Stripe checkout flows |
| `assay-adapter-ucp` | Universal Commerce Protocol | Google/Shopify commerce journeys |
| `assay-adapter-a2a` | Agent2Agent | Agent discovery/tasks/messages |

- [x] **Adapter trait**: Common interface for protocol → EvidenceEvent mapping
- [x] **ACP adapter**: Tool calls, checkout events, payment intents (leverages v2.11.0 mandate support)
- [x] **UCP adapter**: Discover/buy/post-purchase state transitions
- [x] **A2A adapter**: Agent capabilities, task delegation, artifacts

Status on `main`:
- `assay-adapter-api`, `assay-adapter-acp`, `assay-adapter-a2a`, and `assay-adapter-ucp` are merged in open core.
- ADR-026 stabilization through E4 is merged on `main` (metadata identity, lossiness preservation, host attachment policy, canonical digests, parser hardening).
- UCP now follows the same A/B/C rollout discipline as ACP and A2A: Step1 freeze, Step2 MVP + fixtures, Step3 closure docs.

**Why adapters:** The market is fragmenting (ACP vs UCP vs AP2 vs x402). Assay's value is protocol-agnostic governance, not protocol lock-in.

**Enabled by v2.11.0:** The mandate evidence module provides the foundation for AP2-style authorization tracking in these adapters.

**AAIF governance note:** MCP and A2A are now under the Agentic AI Foundation (Linux Foundation, Dec 2025). This reduces protocol fragmentation risk and makes adapter investments more durable.

### B. Connectors
- [ ] **SIEM**: Splunk / Microsoft Sentinel export adapters
- [x] **CI/CD**: GitHub Actions v2 ([Rul1an/assay-action@v2](https://github.com/marketplace/actions/assay-ai-agent-security)) / GitLab CI integration
- [ ] **GitHub App**: Native policy drift detection in PRs
- [ ] **GitLab CI**: Native integration
- [ ] **OTel GenAI**: Align evidence export with [OTel GenAI semantic conventions](https://opentelemetry.io/docs/specs/semconv/gen-ai/) — conventions still experimental but Pydantic AI already follows them; monitor for stability before building bridge

### C. Protocol Claim Packs (`P2`) and kernel hardening (`H1`)
- [x] **MCP companion pack (`mcp-signal-followup`, P2a)**: built-in pack MCP-001..003 — G3-aligned auth context check + delegation + containment degradation; see [PLAN-P2a](architecture/PLAN-P2a-MCP-SIGNAL-FOLLOWUP-CLAIM-PACK.md)
- [x] **`H1` Trust kernel alignment & release hardening**: migration SSOT, alignment tests, docs — [PLAN-H1](architecture/PLAN-H1-TRUST-KERNEL-ALIGNMENT-RELEASE-HARDENING.md), [MIGRATION-TRUST-COMPILER-3.2.md](architecture/MIGRATION-TRUST-COMPILER-3.2.md) (sequence: **H1 before P2b** — ✅ shipped)
- [x] **`a2a-signal-followup` (P2b)**: built-in pack A2A-001..003 — bounded presence on shipped `assay.adapter.a2a.*` — [PLAN-P2b](architecture/PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md); ships in **v3.3.0**
- [x] **`G4-A` — A2A discovery / card evidence (Phase 1)** — `payload.discovery` on emitted canonical A2A evidence — [PLAN-G4](architecture/PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md), [G4-A freeze](architecture/G4-A-PHASE1-FREEZE.md), [`assay-adapter-a2a`](../crates/assay-adapter-a2a/) — now public in **`v3.4.0`**
- [x] **`P2c` — A2A discovery/card follow-up pack** — [PLAN-P2c](architecture/PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md): built-in **`a2a-discovery-card-followup`** (A2A-DC-001 / A2A-DC-002), **`json_path_exists.value_equals`** for boolean `true` at G4-A `/data/discovery/*` pointers; `requires.assay_min_version: ">=3.3.0"` — now public in **`v3.4.0`**
- [x] **`K1-A` Phase 1 — A2A handoff / delegation-route evidence** — first bounded `payload.handoff` seam; route / handoff visibility only; no pack in the same wave — [PLAN-K1](architecture/PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md), [K1-A freeze](architecture/K1-A-PHASE1-FREEZE.md) — now public in **`v3.4.0`**
- [ ] **Further `K1` widening only if separately justified** — any second adapter slice or downstream pack remains a new maintainer decision, not an automatic follow-on
- [x] **`K2-A` Phase 1 — MCP authorization-discovery evidence** — first bounded MCP authorization-discovery seam now public in **`v3.5.0`**; visibility-only and still no auth-discovery pack in the same wave — [PLAN-K2](architecture/PLAN-K2-MCP-AUTHORIZATION-DISCOVERY-EVIDENCE-2026q2.md), [K2-A freeze](architecture/K2-A-PHASE1-FREEZE.md), [K2-A freeze prep](architecture/K2-A-PHASE1-FREEZE-PREP.md)
- [ ] **Further MCP / A2A protocol packs** (beyond P2b / P2c): broader capability/delegation/provenance claims only with first-class evidence
- [ ] **Additional domain packs only after signals exist**: broader compliance surfaces remain downstream of evidence reality

This section is the **`P2` execution surface**: ship small, honest packs first; defer broad compliance theater.

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
| **Baseline Packs** | `eu-ai-act-baseline` (Article 12 mapping, v2.10.0), `soc2-baseline` (Common Criteria baseline, ADR-022) |
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
