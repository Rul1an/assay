# Architecture

Assay is a CI-native evidence and trust compiler for agent systems, built as a Rust workspace.

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
- [Release Plan — Trust Compiler 3.6 Evidence Portability](./RELEASE-PLAN-TRUST-COMPILER-3.6.md) — release-prep checklist for the first external-eval receipt lane
- [Release Plan — Trust Compiler 3.7 Evidence Portability](./RELEASE-PLAN-TRUST-COMPILER-3.7.md) — release record for the first three-family receipt boundary line
- [PLAN — T1a Trust Basis Compiler MVP (Q2 2026)](./PLAN-T1a-TRUST-BASIS-COMPILER-2026q2.md) — first execution wave for canonical `trust-basis.json`
- [Trust Compiler Audit Matrix (2026-03-26)](./AUDIT-MATRIX-TRUST-COMPILER-2026-03-26.md) — wave-by-wave audit of the trust-compiler line from `T1b` through `K2-A` Phase 1
- [Discovery — Next Evidence Wave (Q2 2026)](./DISCOVERY-NEXT-EVIDENCE-WAVE-2026Q2.md) — historical discovery note that ranked post-`P2c` candidates and led to `K1`
- [PLAN — K1 A2A Handoff / Delegation-Route Evidence (Q2 2026)](./PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md) — formal next-wave plan after `P2c`, adapter-first and evidence-first
- [K1-A Phase 1 Freeze (Q2 2026)](./K1-A-PHASE1-FREEZE.md) — executable freeze for the first bounded typed `handoff` seam in A2A canonical adapter output
- [PLAN — K2 MCP Authorization-Discovery Evidence (Q2 2026)](./PLAN-K2-MCP-AUTHORIZATION-DISCOVERY-EVIDENCE-2026q2.md) — active bounded MCP authorization-discovery wave, focused on visibility before any auth-discovery pack
- [K2-A Phase 1 Freeze (Q2 2026)](./K2-A-PHASE1-FREEZE.md) — active contract for the first bounded MCP authorization-discovery seam now public in `v3.5.0`
- [K2-A Phase 1 Freeze Prep (Q2 2026)](./K2-A-PHASE1-FREEZE-PREP.md) — pre-freeze source inventory and guardrails for the first bounded MCP authorization-discovery seam
- [PLAN — P11A Visa TAP Intent Verification Evidence Interop (Q2 2026)](./PLAN-P11A-VISA-TAP-INTENT-VERIFICATION-EVIDENCE-2026q2.md) — planned frontier commerce / trust-proof lane built around TAP verification-result evidence, not payment truth
- [TODO — Next Upstream Interop Lanes (Q2 2026)](./TODO-NEXT-UPSTREAM-INTEROP-LANES-2026q2.md) — ranked post-Agno queue that now tracks Langfuse as the current platform-adjacent lane and APS as a promote-only `P11D` watchlist under the commerce / trust-proof family
- [PLAN — P12 Browser Use History / Output Evidence Interop (Q2 2026)](./PLAN-P12-BROWSER-USE-HISTORY-OUTPUT-EVIDENCE-2026q2.md) — planned adjacent-space lane built around Browser Use local run history and output, not observability export
- [PLAN — P13 Langfuse Experiment Result Evidence Interop (Q2 2026)](./PLAN-P13-LANGFUSE-EXPERIMENT-RESULT-EVIDENCE-2026q2.md) — planned platform-adjacent lane built around bounded experiment item results and evaluations, not Langfuse trace export
- [PLAN — P14 Mastra Scorer / Experiment-Result Evidence Interop (Q2 2026)](./PLAN-P14-MASTRA-SCORER-EXPERIMENT-RESULT-EVIDENCE-2026q2.md) — planned scorer-first Mastra lane built around bounded experiment-item evidence, not tracing or Studio exports
- [PLAN — P14b Mastra ScoreEvent / ExportedScore Evidence Interop (Q2 2026)](./PLAN-P14B-MASTRA-SCORE-EVENT-EVIDENCE-2026q2.md) — maintainer-guided Mastra recut around `ObservabilityExporter` + `ScoreEvent` + `ExportedScore`, explicitly pre-proof on the live callback path
- [PLAN — P14c Mastra ScoreEvent Receipt Import (Q2 2026)](./PLAN-P14C-MASTRA-SCOREEVENT-RECEIPT-IMPORT-2026q2.md) — planned Assay-side compiler path for bounded Mastra score-event artifacts into portable receipts, not Mastra observability, scorer, trace, or runtime truth
- [PLAN — P15 x402 Requirement / Verification Evidence Interop (Q2 2026)](./PLAN-P15-X402-REQUIREMENT-VERIFICATION-EVIDENCE-2026q2.md) — planned requirement-and-verification-first x402 lane built around `PaymentRequired` plus `VerifyResponse`, not settlement or fulfillment truth
- [PLAN — P16 LiveKit Agents Testing-Result / RunEvent Evidence Interop (Q2 2026)](./PLAN-P16-LIVEKIT-AGENTS-TESTING-RESULT-RUNEVENT-EVIDENCE-2026q2.md) — planned testing-result-first LiveKit lane built around `voice.testing.RunResult.events`, not telemetry or transcript export
- [PLAN — P17 LlamaIndex EvaluationResult Evidence Interop (Q2 2026)](./PLAN-P17-LLAMAINDEX-EVALUATIONRESULT-EVIDENCE-2026q2.md) — planned eval-result-first LlamaIndex lane built around bounded `EvaluationResult` evidence, not traces or callback exports
- [PLAN — P18 Vercel AI SDK UIMessage Evidence Interop (Q2 2026)](./PLAN-P18-VERCEL-AI-SDK-UIMESSAGE-EVIDENCE-2026q2.md) — planned message-first Vercel AI SDK lane built around bounded `UIMessage` artifacts, with show-and-tell-first outward strategy rather than question-first
- [PLAN — P19 Mem0 Add Memories Result Evidence Interop (Q2 2026)](./PLAN-P19-MEM0-ADD-MEMORIES-RESULT-EVIDENCE-2026q2.md) — planned mutation-result-first Mem0 lane built around bounded `Add Memories` results, not retrieval or profile truth
- [PLAN — P20 AG-UI Compacted Message Snapshot Artifact Evidence Interop (Q2 2026)](./PLAN-P20-AG-UI-COMPACTED-MESSAGE-SNAPSHOT-ARTIFACT-EVIDENCE-2026q2.md) — planned compacted-message-history AG-UI lane built around one bounded run envelope and one `MESSAGES_SNAPSHOT`, not general serialization or full stream fidelity
- [PLAN — P21 Stagehand Observe-Derived Selector-Scoped Extract Artifact Evidence Interop (Q2 2026)](./PLAN-P21-STAGEHAND-OBSERVE-DERIVED-SELECTOR-SCOPED-EXTRACT-ARTIFACT-EVIDENCE-2026q2.md) — planned selector-scoped Stagehand lane built around one observe-derived selector anchor plus one scoped extract result, not broad browser-agent support or snapshot truth
- [PLAN — P22 OpenAI Agents JS Tool Approval Interruption / Resumable-State Evidence Interop (Q2 2026)](./PLAN-P22-OPENAI-AGENTS-JS-TOOL-APPROVAL-INTERRUPTION-RESUMABLE-STATE-EVIDENCE-2026q2.md) — planned paused-run OpenAI Agents JS lane built around bounded `interruptions` plus one resumable continuation anchor, not transcript, session, or provider-chaining truth
- [PLAN — P23B Assay Paused Human-in-the-Loop Evidence Pattern (Q2 2026)](./PLAN-P23B-ASSAY-PAUSED-HUMAN-IN-THE-LOOP-EVIDENCE-PATTERN-2026q2.md) — planned Assay-side reference pattern for bounded paused HITL evidence, standardizing `pause_reason`, `interruptions`, `call_id_ref`, and derived `resume_state_ref` without importing transcript, session, or full serialized-state truth
- [PLAN — P24 Phoenix Span Annotation Evaluation-Signal Evidence Interop (Q2 2026)](./PLAN-P24-PHOENIX-SPAN-ANNOTATION-EVALUATION-SIGNAL-EVIDENCE-2026q2.md) — planned annotation-first Phoenix lane built around one bounded span annotation artifact, not trace, experiment, evaluator, or platform truth
- [PLAN — P25 LangWatch Custom Span Evaluation Signal Evidence Interop (Q2 2026)](./PLAN-P25-LANGWATCH-CUSTOM-SPAN-EVALUATION-SIGNAL-EVIDENCE-2026q2.md) — planned custom-evaluation-first LangWatch lane built around one bounded span-linked evaluation signal, not trace, dataset, evaluation-session, or platform truth
- [PLAN — P26 AgentEvals Trajectory Strict-Match Result Signal Evidence (Q2 2026)](./PLAN-P26-AGENTEVALS-TRAJECTORY-STRICT-MATCH-RESULT-SIGNAL-EVIDENCE-2026q2.md) — planned strict-match-first AgentEvals lane built around one returned deterministic trajectory match result, not LangSmith runs, LLM-as-judge outputs, or raw trajectory truth
- [PLAN — P27 AutoEvals ExactMatch Score Evidence (Q2 2026)](./PLAN-P27-AUTOEVALS-EXACTMATCH-SCORE-EVIDENCE-2026q2.md) — planned ExactMatch-first AutoEvals lane built around one returned deterministic output/expected comparison score, not Braintrust runs, JSON/list scorer bundles, LLM judge outputs, or raw payload truth
- [PLAN — P28 Promptfoo Assertion GradingResult Evidence (Q2 2026)](./PLAN-P28-PROMPTFOO-ASSERTION-GRADING-RESULT-EVIDENCE-2026q2.md) — planned deterministic-assertion-first Promptfoo lane built around one surfaced `GradingResult`, not full eval exports, prompt matrices, red-team reports, or raw provider output truth
- [PLAN — P29 Guardrails Validation Outcome Evidence (Q2 2026)](./PLAN-P29-GUARDRAILS-VALIDATION-OUTCOME-EVIDENCE-2026q2.md) — planned outcome-first Guardrails AI lane built around one bounded validation outcome, not prompt, corrected-output, reask, or guard-history truth
- [PLAN — P30 OpenFeature EvaluationDetails Evidence (Q2 2026)](./PLAN-P30-OPENFEATURE-EVALUATION-DETAILS-EVIDENCE-2026q2.md) — planned governance-adjacent OpenFeature lane built around one returned `EvaluationDetails` object, not provider config, targeting, rollout, telemetry, or application correctness truth
- [PLAN — P31 Promptfoo JSONL Component Result Receipt Import (Q2 2026)](./PLAN-P31-PROMPTFOO-JSONL-COMPONENT-RESULT-RECEIPT-IMPORT-2026q2.md) — planned compiler-path follow-up to P28 that imports one Promptfoo JSONL assertion component result into one portable Assay evidence receipt, not full Promptfoo eval-run truth or Harness regression gating
- [PLAN — P32 Promptfoo Receipt Trust Basis Readiness (Q2 2026)](./PLAN-P32-PROMPTFOO-RECEIPT-TRUST-BASIS-READINESS-2026q2.md) — execution slice that proves P31 receipt bundles feed the current Trust Basis compiler without adding a Promptfoo-specific claim row or Trust Card schema bump
- [PLAN — P33 External Eval Receipt Trust Basis Claim (Q2 2026)](./PLAN-P33-EXTERNAL-EVAL-RECEIPT-TRUST-BASIS-CLAIM-2026q2.md) — execution slice that adds one bounded Trust Basis claim for supported external evaluation receipt boundaries, starting with Promptfoo assertion-component receipts
- [PLAN — P34 Trust Basis Diff Gate (Q2 2026)](./PLAN-P34-TRUST-BASIS-DIFF-GATE-2026q2.md) — execution slice that compares canonical Trust Basis artifacts for claim-level regressions without parsing Promptfoo JSONL or external eval payloads
- [PLAN — P41 OpenFeature EvaluationDetails Decision Receipt Import (Q2 2026)](./PLAN-P41-OPENFEATURE-EVALUATION-DETAILS-DECISION-RECEIPT-IMPORT-2026q2.md) — execution slice that imports bounded boolean OpenFeature decision details as portable Assay receipts, not provider config, targeting, metadata, or application correctness truth
- [PLAN — P43 CycloneDX ML-BOM Model Component Receipt Import (Q2 2026)](./PLAN-P43-CYCLONEDX-MLBOM-MODEL-COMPONENT-RECEIPT-IMPORT-2026q2.md) — execution slice that imports one selected CycloneDX `machine-learning-model` component as a portable inventory receipt, not full BOM, model-card, dataset, graph, or compliance truth
- [PLAN — P45 Inventory Receipt Trust Basis Claim (Q2 2026)](./PLAN-P45-INVENTORY-RECEIPT-TRUST-BASIS-CLAIM-2026q2.md) — execution slice that adds one bounded Trust Basis claim for supported inventory receipt boundaries, starting with CycloneDX ML-BOM model-component receipts
- [PLAN — P45b Decision Receipt Trust Basis Claim (Q2 2026)](./PLAN-P45B-DECISION-RECEIPT-TRUST-BASIS-CLAIM-2026q2.md) — execution slice that adds one bounded Trust Basis claim for supported decision receipt boundaries, starting with OpenFeature boolean EvaluationDetails receipts
- [PLAN — P52-P56 Assay Product Surface Consolidation Program (Q2 2026)](./PLAN-P52-P56-CONSOLIDATION-PROGRAM-2026q2.md) — post-v3.8.0 consolidation program for product truth sync, Trust Basis assertions, receipt schema CLI, static Trust Card HTML, and policy/tool digest binding
- [PLAN — P56a Policy Snapshot Digest Visibility (Q2 2026)](./PLAN-P56A-POLICY-SNAPSHOT-DIGEST-VISIBILITY-2026q2.md) — execution slice that projects canonical policy snapshot digest metadata onto supported MCP decision evidence without claiming policy correctness
- [Assay Architecture & Roadmap Gap Analysis (Q2 2026)](./GAP-ASSAY-ARCHITECTURE-ROADMAP-2026q2.md) — repo-wide truth sync and next-step ordering

## Active RFCs

| RFC | Status | Summary |
|-----|--------|---------|
| [RFC-001: DX/UX & Governance](./RFC-001-dx-ux-governance.md) | Historical (Wave A/B delivered; Wave C remains data-gated) | Normative DX/refactor invariants and historical execution framing |
| [RFC-002: Code Health Remediation](./RFC-002-code-health-remediation-q1-2026.md) | Complete (E1–E4 merged, E5→RFC-003) | Store, metrics, registry, comment cleanup |
| [RFC-003: Generate Decomposition](./RFC-003-generate-decomposition-q1-2026.md) | Complete (G1–G6 merged) | `generate.rs` split into focused modules |
| [RFC-004: Open Items Convergence](./RFC-004-open-items-convergence-q1-2026.md) | Closed (O1–O6 merged on `main`) | Historical closure ledger for the Q1 convergence line |
| [RFC-005: Trust Compiler MVP](./RFC-005-trust-compiler-mvp-2026q2.md) | Active (`T1a`..`H1` public in `v3.3.0`; `G4-A`, `P2c`, and `K1-A` public in `v3.4.0`; `K2-A` Phase 1 is now public in `v3.5.0`) | Bounded plan for the trust-compiler and Trust Card line |

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
