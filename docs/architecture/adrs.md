# Architecture Decision Records

This directory contains Architecture Decision Records (ADRs) for the Assay project.

## Index

The index lists primary ADRs and links companions from their parent.

| ADR | Title | Status | Priority |
|-----|-------|--------|----------|
| [ADR-001](./adr-001-sandbox-design.md) | Assay Sandbox Architecture (SOTA Refined) | Accepted | - |
| [ADR-002](./ADR-002-Trace-Replay.md) | Trace Replay as Input Adapter | Accepted | - |
| [ADR-003](./ADR-003-Gate-Semantics.md) | Gate Semantics and Strict Mode | not stated | - |
| [ADR-004](./ADR-004-Judge-Metrics.md) | Judge Metrics Strategy | Accepted (v2) | - |
| [ADR-005](./ADR-005-Relative-Thresholds.md) | Relative Thresholds & Baselines | Accepted (v2) | - |
| [ADR-006](./ADR-006-Evidence-Contract.md) | Evidence Contract for Agent Runtime | **Adopted** (Q1 2026 Strategy) | - |
| [ADR-007](./ADR-007-Deterministic-Provenance.md) | Deterministic Identification & Provenance | **Adopted** (Q1 2026 Strategy) | - |
| [ADR-008](./ADR-008-Evidence-Streaming.md) | Evidence Streaming Architecture | Proposed (January 2026) | Backlog |
| [ADR-009](./ADR-009-WORM-Storage.md) | WORM Storage for Evidence Retention | **Deferred** (January 2026) | Q3+ |
| [ADR-010](./ADR-010-Evidence-Store-API.md) | Evidence Store Ingest API | **Deferred to Phase 3** (January 2026) | Q3+ |
| [ADR-011](./ADR-011-Tool-Signing.md) | MCP Tool Signing with Sigstore | Proposed (January 2026; boundary sync February 2026) | **P1** |
| [ADR-012](./ADR-012-Transparency-Log.md) | Transparency Log Integration with Rekor | Proposed (January 2026; boundary sync February 2026) | **P3** |
| [ADR-013](./ADR-013-EU-AI-Act-Pack.md) | EU AI Act Compliance Pack | Accepted (January 2026) | **P2** |
| [ADR-014](./ADR-014-GitHub-Action-v2.md) | GitHub Action v2 Design | Implemented | ✅ |
| [ADR-015](./ADR-015-BYOS-Storage-Strategy.md) | BYOS (Bring Your Own Storage) Strategy | Accepted (January 2026) | **P1** |
| [ADR-016](./ADR-016-Pack-Taxonomy.md) | Pack Taxonomy (Baseline vs Pro) | Accepted (January 2026; boundary sync February 2026) | - |
| [ADR-017](./ADR-017-Mandate-Evidence.md) | Mandate/Intent Evidence | Accepted (January 2026, updated v1.0.5) | - |
| [ADR-018](./ADR-018-GitHub-Action-v2.1.md) | GitHub Action v2.1 - Attestation, OIDC & Compliance | Accepted (implemented v2.12.0) | - |
| [ADR-019](./ADR-019-PR-Gate-2026-SOTA.md) | PR Gate 2026 SOTA — Implementation Plan v1 | Partially Implemented | - |
| [ADR-020](./ADR-020-Dependency-Governance.md) | Dependency Governance | Accepted | - |
| [ADR-021](./ADR-021-Local-Pack-Discovery.md) | Local Pack Discovery and Pack Resolution Order | Accepted (February 2026) | **P2** |
| [ADR-022](./ADR-022-SOC2-Baseline-Pack.md) | SOC2 Baseline Pack (AICPA Trust Service Criteria) | Accepted. Implemented (pack in `packs/open/soc2-baseline/`, built-in in assay-evidence). | **P2** |
| [ADR-023](./ADR-023-CICD-Starter-Pack.md) | CICD Starter Pack (Adoption Floor) | Accepted (February 2026) | **P1** |
| [ADR-024](./ADR-024-Sim-Engine-Hardening.md) | Sim Engine Hardening (Limits + Time Budget) | Superseded (February 2026, by ADR-025 Reliability Surface / I1 soak rollout) | **P2** |
| [ADR-025](./ADR-025-Evidence-as-a-Product.md) | Evidence-as-a-Product — Reliability Surfaces, Completeness/Closure, and Portable Verifiability | Accepted (March 2026; I1/I2/I3 rollout slices implemented and closed-loop on `main`) | **P1/P2** |
| | Companions: [ADR-025 Index](./ADR-025-INDEX.md) | | |
| [ADR-026](./ADR-026-Protocol-Adapters.md) | Protocol Adapters (Adapter-First Strategy) | Accepted (February 2026; ACP + A2A + UCP adapter rollout and E0-E4 stabilization merged on `main`) | **P1** |
| | Companions: [ADR-026 Adapter Metadata Contract](./ADR-026-ADAPTER-METADATA-CONTRACT.md), [ADR-026 AttachmentWriter Host Boundary](./ADR-026-ATTACHMENT-WRITER-BOUNDARY.md), [ADR-026 Adapter Distribution Policy](./ADR-026-Adapter-Distribution-Policy.md), [ADR-026 Adjacent Notes](./ADR-026-Adjacent-Notes.md), [ADR-026 Canonicalization and Hash Boundary](./ADR-026-CANONICALIZATION-HASH-BOUNDARY.md), [ADR-026 Parser Hardening Boundary](./ADR-026-PARSER-HARDENING-BOUNDARY.md) | | |
| [ADR-027](./ADR-027-Tool-Taxonomy.md) | Tool Taxonomy and Class-Based Route Policies | Accepted (March 2026; implemented on `main` via PRs #560, #561, and #572) | **P1** |
| [ADR-028](./ADR-028-Coverage-Report.md) | Coverage Report (Tool & Route Completeness) | Accepted (March 2026; implemented on `main` via PRs #563, #565, #567, and #572) | **P1** |
| [ADR-029](./ADR-029-Session-State-Window.md) | Session & State Window Contract (MCP Governance) | Accepted (March 2026; implemented on `main` via PRs #569, #574, and #576) | **P1** |
| [ADR-030](./ADR-030-Coverage-Wrap-DX-Polish.md) | Coverage + Wrap DX Polish | Accepted (March 2026; implemented on `main` via PRs #578, #580, and #582) | **P2** |
| [ADR-031](./ADR-031-Coverage-v1.1-DX-Polish.md) | Coverage v1.1 DX Polish | Accepted (March 2026; implemented on `main` via PRs #585, #587, and #588) | **P2** |
| [ADR-032](./ADR-032-MCP-Policy-Obligations-and-Evidence-v2.md) | MCP Policy Enforcement, Obligations, and Evidence v2 | Accepted (March 2026) | **P1** |
| [ADR-033](./ADR-033-OTel-Trust-Compiler-Positioning.md) | Assay as an OTel-Native Trust Compiler for Agent Systems | Accepted (March 2026) | **P1** |
| [ADR-034](./ADR-034-Assay-Runner-Harness-Contract-Seam.md) | Assay / Runner / Harness Contract Seam | Proposed (June 2026) | - |
| [ADR-034](./ADR-034-Evidence-Redaction-At-Capture.md) | Evidence Redaction at Capture (runner-side secret hygiene) | Proposed (June 2026). DRAFT, reviewer feedback incorporated; design decisions resolved (see Decisions). | - |
| [ADR-035](./ADR-035-sandbox-the-agent-evidence.md) | Sandbox-the-Agent Evidence Path | Proposed (June 2026) | - |
| [ADR-036](./ADR-036-editor-mcp-wrap-recipe.md) | Editor MCP Wrap Recipe | Proposed (June 2026; remote/OAuth section finalises after the 28 July 2026 MCP spec) | - |
| [ADR-037](./ADR-037-runner-standalone-boundary.md) | Runner Standalone Boundary | Accepted (June 2026) — records existing discipline; pointer ADR. | - |
| [ADR-038](./ADR-038-otlp-exporter-for-observations.md) | OTLP Exporter for Assay Observations | Proposed (June 2026) — decision recorded; code lands as a tracked slice. | - |
| [ADR-039](./ADR-039-evidence-bundle-attestation.md) | Evidence Bundle as in-toto / SCITT Attestation | Proposed (June 2026) — trigger-gated. | - |
| [ADR-040](./ADR-040-inspect-claim-support-scorer.md) | Public Inspect Scorer for Claim Support | Proposed (June 2026) — depends on the sandbox evidence slice (ADR-035). | - |
| [ADR-041](./ADR-041-ebpf-policy-substrate-vs-bespoke.md) | eBPF and Policy, Substrate-versus-Bespoke | Proposed (June 2026) — decision recorded; default posture set. | - |
| [ADR-042](./ADR-042-evidence-first-positioning.md) | Evidence-first positioning and scope freeze | Accepted | - |
| [ADR-043](./ADR-043-evidence-chain-integrity-invariants.md) | Evidence-chain integrity invariants | Accepted | - |
| [ADR-044](./ADR-044-attestation-subject-is-the-artifact.md) | The attestation subject is the artifact, not the semantic chain | Accepted | - |
| [ADR-045](./ADR-045-aee-substrate-signed-run-end-seal.md) | AEE-compatible substrate-signed run-end seal primitive | Proposed | - |
| [ADR-046](./ADR-046-reason-code-registries-stay-separate.md) | The reason-code registries stay separate, because they were never two answers to one question | Accepted | - |
| [ADR-047](./ADR-047-session-scope-findings-are-events.md) | A session-scope finding is an event; a post-run disposition is not | Accepted | - |
| [ADR-048](./ADR-048-claim-gate-construction-moves-readings-stay.md) | The claim gate shares one lattice and one invariant across three tables that legitimately differ — the two enums move to `assay-common`, the tables and the fold stay home, and the policy/trace path already makes absence claims it cannot base | Accepted | - |
| [ADR-049](./ADR-049-attestation-extent.md) | Optional artifact-derived attestation extent | Accepted; implementation pending | - |
| [ADR-050](./ADR-050-trace-truncation-observations-on-6x.md) | Truncation observations ride next to the trace rows on 6.x | Proposed | **P1** |

*Note on ADR-034 duplicate:* The number ADR-034 was assigned twice: `ADR-034-Assay-Runner-Harness-Contract-Seam.md` (Assay / Runner / Harness Contract Seam) and `ADR-034-Evidence-Redaction-At-Capture.md` (Evidence Redaction at Capture). Issue #2919 proposes resolving the collision by either having one file keep 034 while the other is renumbered, or keeping both with a documented a/b suffix alongside updating all in-repo references.

## Q2 2026 Priorities

**Strategy:** BYOS-first (Bring Your Own Storage) per ADR-015. Focus on CLI features, defer managed infrastructure until PMF.

| Priority | ADR | Status | Notes |
|----------|-----|--------|-------|
| ✅ | [ADR-014](./ADR-014-GitHub-Action-v2.md) | Implemented | [Marketplace](https://github.com/marketplace/actions/assay-ai-agent-security) |
| **P1** | [ADR-015](./ADR-015-BYOS-Storage-Strategy.md) | Accepted | `push/pull/list` shipped on `main`; `store-status`, richer config ergonomics, and fuller provider docs remain open |
| **P1** | [ADR-011](./ADR-011-Tool-Signing.md) | Proposed | `x-assay-sig` + local-key signing in OSS; Sigstore keyless deferred to enterprise |
| **P1** | [ADR-023](./ADR-023-CICD-Starter-Pack.md) | Accepted | OSS starter adoption floor (implemented) |
| **P2** | [ADR-021](./ADR-021-Local-Pack-Discovery.md) | Accepted | Local pack discovery + safe resolution order (implemented) |
| **P2** | [ADR-022](./ADR-022-SOC2-Baseline-Pack.md) | Accepted | SOC2 baseline OSS pack (implemented) |
| **P1/P2** | [ADR-025](./ADR-025-Evidence-as-a-Product.md) | Accepted | I1/I2/I3 slices merged on `main`; formal accept complete |
| **P1** | [ADR-026](./ADR-026-Protocol-Adapters.md) | Accepted | ACP + A2A + UCP adapter slices and E0-E4 stabilization are merged on `main` |
| **P1** | [ADR-027](./ADR-027-Tool-Taxonomy.md) | Accepted | Implemented on `main` via PRs #560, #561, and #572 (taxonomy + class-aware tool matching + closure) |
| **P1** | [ADR-028](./ADR-028-Coverage-Report.md) | Accepted | Implemented on `main` via PRs #563, #565, #567, and #572 (coverage contract + generator + wrap emission + closure) |
| **P1** | [ADR-029](./ADR-029-Session-State-Window.md) | Accepted | Implemented on `main` via PRs #569, #574, and #576 (session/state contract + informational export + closure) |
| **P2** | [ADR-030](./ADR-030-Coverage-Wrap-DX-Polish.md) | Accepted | Implemented on `main` via PRs #578, #580, and #582 (coverage markdown/file input + wrap export log consistency + closure) |
| **P2** | [ADR-031](./ADR-031-Coverage-v1.1-DX-Polish.md) | Accepted | Implemented on `main` via PRs #585, #587, and #588 (`--out-md`, `--routes-top`, and closure docs/gates) |
| **P1** | [ADR-032](./ADR-032-MCP-Policy-Obligations-and-Evidence-v2.md) | Accepted | Wave24-Wave42 merged on `main`; see overview + plan for capability grouping and historical rollout |
| **P1** | [ADR-033](./ADR-033-OTel-Trust-Compiler-Positioning.md) | Accepted | Product direction after `P1`: Trust Compiler MVP, Trust Card, then auth signals and protocol claim packs |
| **P2** | [ADR-013](./ADR-013-EU-AI-Act-Pack.md) | Accepted | Article 12 mapping, `--pack` flag |
| **P3** | [ADR-012](./ADR-012-Transparency-Log.md) | Proposed | Builds on ADR-011 |
| Deferred | [ADR-009](./ADR-009-WORM-Storage.md) | Deferred | Managed WORM → Q3+ if demand |
| Deferred | [ADR-010](./ADR-010-Evidence-Store-API.md) | Deferred | Managed API → Q3+ if demand |

## ADR-032 Companion Docs

The ADR-032 line has supporting architecture documents with separate roles:

- [ADR-032 Implementation Overview](./OVERVIEW-ADR-032-MCP-POLICY-STACK-2026q2.md) — current-state maintainer map
- [ADR-032 Building Block View](./BUILDING-BLOCKS-ADR-032-MCP-POLICY-STACK-2026q2.md) — structural decomposition
- [ADR-032 Quality Scenarios](./QUALITY-SCENARIOS-ADR-032-MCP-POLICY-STACK-2026q2.md) — explicit quality attributes
- [ADR-032 Structurizr Workspace](./STRUCTURIZR-ADR-032-WORKSPACE-2026q2.md) — bounded architecture-as-code model
- [ADR-032 Obsidian View Layer Recommendations](./OBSIDIAN-ADR-032-VIEW-LAYER-2026q2.md) — internal view-layer guidance
- [ADR-032 Execution Plan](./PLAN-ADR-032-MCP-POLICY-ENFORCEMENT-2026q2.md) — historical rollout log
- [ADR-032 Documentation Maturity Gap Analysis](./GAP-ADR-032-MCP-POLICY-DOCS-MATURITY-2026q2.md) — current-state gap analysis

## Repo-wide Architecture & Roadmap

- [Assay Architecture & Roadmap Gap Analysis](./GAP-ASSAY-ARCHITECTURE-ROADMAP-2026q2.md) — current-state truth sync across roadmap, ADRs, RFCs, and delivery gaps

## Template

New ADRs should follow this structure:

```markdown
# ADR-XXX: Title

## Status
Proposed | Accepted | Deprecated | Superseded

## Context
What is the issue that we're seeing that is motivating this decision?

## Decision
What is the change that we're proposing and/or doing?

## Consequences
What becomes easier or more difficult to do because of this change?
```
