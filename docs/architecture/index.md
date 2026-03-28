# Architecture

Assay is a governance and evidence platform for AI agents, built as a Rust workspace.

## Structure

- [Crate Structure](./crates.md) — workspace organization and module layout
- [Data Flow](./data-flow.md) — trace → gate → evidence pipeline
- [Split Refactor Plan (Q1-Q2 2026)](./PLAN-split-refactor-2026q1.md) — wave-by-wave execution plan
- [Split Refactor Report (Q1 2026)](./REPORT-split-refactor-2026q1.md) — verified closure and LOC outcomes
- [Split / Refactor Hotspot Inventory (Q2 2026)](./INVENTORY-split-refactor-hotspots-2026q2.md) — current Rust hotspot baseline and next-wave ordering
- [ADR-032 Implementation Overview (Q2 2026)](./OVERVIEW-ADR-032-MCP-POLICY-STACK-2026q2.md) — current MCP policy stack on `main`
- [ADR-032 Building Block View (Q2 2026)](./BUILDING-BLOCKS-ADR-032-MCP-POLICY-STACK-2026q2.md) — structural decomposition of the MCP policy stack
- [ADR-032 Quality Scenarios (Q2 2026)](./QUALITY-SCENARIOS-ADR-032-MCP-POLICY-STACK-2026q2.md) — explicit quality attributes and review scenarios
- [ADR-032 Structurizr Workspace (Q2 2026)](./STRUCTURIZR-ADR-032-WORKSPACE-2026q2.md) — bounded architecture-as-code workspace and C4 model
- [ADR-032 Obsidian View Layer Recommendations (Q2 2026)](./OBSIDIAN-ADR-032-VIEW-LAYER-2026q2.md) — recommended internal view-layer setup
- [ADR-032 Documentation Maturity Gap Analysis (Q2 2026)](./GAP-ADR-032-MCP-POLICY-DOCS-MATURITY-2026q2.md) — current-state gap analysis and follow-up posture
- [ADR-032 Execution Plan (Q2 2026)](./PLAN-ADR-032-MCP-POLICY-ENFORCEMENT-2026q2.md) — MCP policy/obligation rollout status
- [ADR-033 Trust Compiler Positioning (Q2 2026)](./ADR-033-OTel-Trust-Compiler-Positioning.md) — product north star for Assay as an OTel-native trust compiler
- [RFC-005 Trust Compiler MVP (Q2 2026)](./RFC-005-trust-compiler-mvp-2026q2.md) — bounded plan for `T1a` compiler and `T1b` Trust Card
- [PLAN — T1a Trust Basis Compiler MVP (Q2 2026)](./PLAN-T1a-TRUST-BASIS-COMPILER-2026q2.md) — first execution wave for canonical `trust-basis.json`
- [Trust Compiler Audit Matrix (2026-03-26)](./AUDIT-MATRIX-TRUST-COMPILER-2026-03-26.md) — wave-by-wave audit of the trust-compiler line from `T1b` through `K1-A` Phase 1
- [Discovery — Next Evidence Wave (Q2 2026)](./DISCOVERY-NEXT-EVIDENCE-WAVE-2026Q2.md) — historical discovery note that ranked post-`P2c` candidates and led to `K1`
- [PLAN — K1 A2A Handoff / Delegation-Route Evidence (Q2 2026)](./PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md) — formal next-wave plan after `P2c`, adapter-first and evidence-first
- [K1-A Phase 1 Freeze (Q2 2026)](./K1-A-PHASE1-FREEZE.md) — executable freeze for the first bounded typed `handoff` seam in A2A canonical adapter output
- [Assay Architecture & Roadmap Gap Analysis (Q2 2026)](./GAP-ASSAY-ARCHITECTURE-ROADMAP-2026q2.md) — repo-wide truth sync and next-step ordering

## Active RFCs

| RFC | Status | Summary |
|-----|--------|---------|
| [RFC-001: DX/UX & Governance](./RFC-001-dx-ux-governance.md) | Historical (Wave A/B delivered; Wave C remains data-gated) | Normative DX/refactor invariants and historical execution framing |
| [RFC-002: Code Health Remediation](./RFC-002-code-health-remediation-q1-2026.md) | Complete (E1–E4 merged, E5→RFC-003) | Store, metrics, registry, comment cleanup |
| [RFC-003: Generate Decomposition](./RFC-003-generate-decomposition-q1-2026.md) | Complete (G1–G6 merged) | `generate.rs` split into focused modules |
| [RFC-004: Open Items Convergence](./RFC-004-open-items-convergence-q1-2026.md) | Closed (O1–O6 merged on `main`) | Historical closure ledger for the Q1 convergence line |
| [RFC-005: Trust Compiler MVP](./RFC-005-trust-compiler-mvp-2026q2.md) | Active (`T1a`..`P2c` on `main`; `K1` formalized; `K1-A` Phase 1 shipped on `main`) | Bounded plan for the trust-compiler and Trust Card line |

## Architecture Decision Records

See the full [ADR index](./adrs.md) for all accepted and proposed architecture decisions.

Key ADRs:
- [ADR-003: Gate Semantics](./ADR-003-Gate-Semantics.md) — Pass/Fail/Warn/Flaky
- [ADR-006: Evidence Contract](./ADR-006-Evidence-Contract.md) — schema v1
- [ADR-014: GitHub Action v2](./ADR-014-GitHub-Action-v2.md) — CI integration
- [ADR-015: BYOS Strategy](./ADR-015-BYOS-Storage-Strategy.md) — bring your own storage
- [ADR-032: MCP Policy Enforcement v2](./ADR-032-MCP-Policy-Obligations-and-Evidence-v2.md) — typed decisions + obligations + evidence
- [ADR-033: Trust Compiler Positioning](./ADR-033-OTel-Trust-Compiler-Positioning.md) — claims-as-code north star and Trust Card wedge

## Reference

- [Code Analysis Report](./CODE-ANALYSIS-REPORT.md) — finding snapshot (remediation tracked in RFCs)
- [Assay Architecture & Roadmap Gap Analysis](./GAP-ASSAY-ARCHITECTURE-ROADMAP-2026q2.md) — repo-wide truth sync across architecture and roadmap
- [Pipeline Decomposition Plan](./PLAN-pipeline-decomposition.md) — run/ci shared pipeline design
