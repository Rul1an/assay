# ADR-033: Assay as an OTel-Native Trust Compiler for Agent Systems

## Status
Accepted (March 2026)

## Context

Assay's strongest 2026 delivery line is no longer "more agent tooling breadth." It is a sequence of small, bounded claim and evidence moves:

- `C2` shipped a narrow, honest control-evidence baseline instead of a broad OWASP story.
- `E1` added a minimal typed engine seam rather than a wider policy language.
- `G1` surfaced supported weaker-than-requested containment fallback paths as evidence.
- `G2` surfaced explicit delegation context on supported decision evidence.
- `P1` productized those signals as a signal-aware companion pack without broadening the baseline.
- the only post-`P1` release-line mismatch was closed on `main` by the `3.2.3` workspace bump and aligned OWASP pack version floors.

At the same time, the external line is moving toward practical identity/authz metadata, auditability, and protocol-level measurable defenses:

- OWASP MCP Top 10 emphasizes authorization and audit/telemetry as distinct control layers.
- OWASP Top 10 for Agentic Applications 2026 keeps identity/privilege abuse and execution risk as first-order categories.
- NIST NCCoE and CAISI work on software/AI agent identity and authorization centers explicit metadata and bounded controls.
- Protocol-aware benchmarks such as A2ASecBench and MCP-SafetyBench reinforce that the frontier is in verifiable protocol/runtime claims, not generic prompt safety or dashboard breadth.

Repo truth already supports this direction:

- the evidence pipeline follows the OTel Collector pattern
- `assay trace ingest-otel` already exists on `main`
- evidence bundles, verification, signing, and proof-bearing packs are all implemented
- trust-chain experiments on `main` already reason about provenance, delegation spoofing, and consumer-side evidence interpretation

This means Assay is best positioned not as "another eval platform" or "another observability dashboard," but as the system that compiles runtime truth into verifiable security claims.

## Strategic Fit Test

This direction is considered strategically sound only while it passes all three tests below.

### 1. External Demand Fit

The strongest external demand in 2026 is not generic agent analytics. It is practical control surfaces around:

- identity and authorization
- audit and telemetry
- protocol-level security posture
- bounded, reviewable deployment claims

That makes Assay a better fit for a trust-compiler category than for a broad observability or eval category.

### 2. Repo Capability Fit

Assay already has the substrate this direction requires:

- canonical evidence and offline verification
- OTel-style ingest and transformation
- proof-bearing bundles and signing surfaces
- bounded signal waves for containment degradation and delegation visibility
- signal-aware companion packs

This means the trust-compiler direction is a composition of real shipped capabilities, not a jump into a new product genus.

### 3. Wedge Fit Against Alternatives

The main alternatives are:

- broader pack expansion
- another engine/semantics wave
- dashboards / observability UX
- generic eval or red-team positioning

Those may be easier to explain, but they are weaker wedges for Assay. The stronger wedge is to make claim provenance and evidence status portable and explicit through a Trust Card and associated claim surfaces.

## Decision

Assay is positioned as an **OTel-native trust compiler for agent systems**.

Trust compiler describes the product category; OTel-native describes the preferred ingest and ecosystem posture.

The product model is:

- **Input**: OTel spans, protocol/runtime events, Assay traces, and bundle artifacts
- **Compile**: canonical evidence, bounded claim classification, and pack evaluation
- **Output**: findings, SARIF, verifiable bundles, and a future signed Trust Card

`OTel-native` is a direction for ingress and ecosystem fit, not a surrender of semantic control. Assay's own canonical evidence layer remains the stable source of truth for trust claims. OTel semantic conventions may evolve, and Assay should ingest and map them, not couple its truth model to any single moving semconv shape.
Claims are classified on canonical evidence, not directly on raw OTel spans or other upstream ingest formats.

### North Star Freeze

The following constraints are normative for roadmap and product decisions unless a later ADR explicitly supersedes them:

1. **Claim-first, not dashboard-first**
   Assay's primary product surface is evidence-classified trust claims. Dashboards, trace browsers, and visual analytics are supporting surfaces, not the wedge.

2. **Canonical evidence over ingest format**
   OTel, protocol adapters, and other sources are ingest paths. Trust claims must be grounded in Assay's canonical evidence contract and offline-verifiable bundle reality.

   Operational rule: new ingest paths may be additive or translational, but they must not replace the canonical evidence layer as the semantic authority for claim classification.
   Any upstream OTel or protocol mapping change that could affect claim semantics must be covered by canonical evidence mapping tests before adoption.

3. **Trust Card over trust score**
   The iconic artifact is a Trust Card that shows what is `verified`, `self_reported`, `inferred`, or `absent`. A scalar trust score or binary `trusted/untrusted` output must not become the primary interface.

   MVP rule: no aggregate trust score, no `safe/unsafe` badge, and no maturity badge as the primary artifact.

4. **Fixed execution order**
   The default execution order is `T1a -> T1b -> G3 -> P2`, then only later heavier semantics such as reference existence, temporal validity, or capability attestation, unless a later ADR explicitly supersedes it.

5. **No premature correctness claims**
   Delegation validation, chain integrity/completeness, sandbox correctness, inherited-scope correctness, and temporal correctness remain out of scope until dedicated signals and semantics exist.

### Claim Epistemology Is A First-Class Product Surface

Assay differentiates by making the evidence level of a claim explicit, rather than by maximizing raw detection counts.

The primary evidence levels are:

| Level | Meaning |
|-------|---------|
| `verified` | Backed by direct runtime evidence or offline bundle verification |
| `self_reported` | Reported by the observed system without stronger corroboration |
| `inferred` | Derived by bounded, documented interpretation rules |
| `absent` | No trustworthy evidence currently supports the claim |

These evidence levels are the preferred external framing for future trust artifacts. Assay should not collapse them into a primary opaque trust score.

### Trust Card Is The First Iconic Artifact

The first product artifact of this compiler direction is a **Trust Card**:

- portable
- machine-readable
- reviewable by humans
- potentially signable / attestable later

The Trust Card is an output of the compiler, not a separate dashboard product.
The Trust Card is a portable manifestation of compiler output, not the full product category.

### Protocol Claim Packs Are The Preferred Downstream Productization Path

After the compiler and Trust Card surfaces stabilize, Assay should extend via **small protocol claim packs**, not via broad compliance theater.

Examples:

- delegated authority context surfaced
- weaker-than-requested containment surfaced
- provenance-backed vs provenance-absent distinguished
- capability overclaim detected

### Deliberate Non-Plays

This direction explicitly rejects:

- becoming a tracing platform
- becoming a general observability dashboard
- becoming eval-as-a-service
- becoming a generic red-team framework
- shipping a delegation-validation or sandbox-correctness story that the signals do not support
- using an opaque scalar trust score as the primary product output
- binding Assay's truth model to any one evolving OTel/agent semconv form

## Main Risks And Mitigations

### Risk 1 — Abstract Product Story

`trust compiler` is more abstract than `evals`, `guardrails`, or `observability`.

Mitigation:

- keep the first artifact concrete: Trust Card
- keep the claim levels explicit and simple
- tie the story to CI, release governance, procurement review, and vendor comparison

### Risk 2 — Category Confusion

If Assay presents itself as a tracing platform, dashboard, firewall, or generic eval suite, it competes in denser categories with a weaker wedge.

Mitigation:

- keep the north star claim-first
- treat dashboards and visual analytics as supporting surfaces only
- keep protocol claim packs and Trust Card artifacts as the visible outputs

### Risk 3 — Standards Churn

OTel GenAI and agent semantic conventions are still evolving, and protocol extensions will continue to move.

Mitigation:

- keep Assay's canonical evidence contract as the truth layer
- ingest and map OTel/protocol forms into that layer
- avoid coupling primary trust semantics to any single moving upstream semconv

## Consequences

### Positive

- Assay's moat becomes clearer and more defensible: trace -> evidence -> claim -> proof.
- The product aligns better with the strongest parts of the existing architecture: deterministic evidence, pack discipline, offline verification, and OTel-friendly ingestion.
- Trust artifacts can become portable CI/CD, audit, and procurement objects rather than dashboard-only screenshots.
- Future signal waves such as authorization context fit naturally into the compiler story.

### Negative

- Assay intentionally does less in categories where competitors are already strong, such as experiments, dashboards, or generic eval UX.
- Claim discipline must remain strict; overclaiming would undermine the entire positioning.
- The first deliverables need careful wording so the compiler story does not sound like a full identity-validation or protocol-verification engine.
- This positioning is less immediately legible than dashboard/eval categories and therefore depends on concrete artifacts and examples to remain understandable.

### Neutral

- Existing evidence, pack, and verification surfaces remain valid. This ADR changes product posture and next-step ordering more than it changes the core architecture.

## Immediate Follow-On Sequence

1. `T1a` — OTel-native Trust Compiler MVP
2. `T1b` — Trust Card MVP
3. `G3` — Authorization Evidence Signal
4. `P2` — Protocol Claim Packs
5. Later: reference existence, temporal validity, capability attestation, and richer compliance packs

Any proposal that primarily improves dashboards, generic observability UX, or score-first reporting should be considered out-of-lane until this sequence is materially complete.

## References

- [ADR-006: Evidence Contract](./ADR-006-Evidence-Contract.md)
- [ADR-008: Evidence Streaming](./ADR-008-Evidence-Streaming.md)
- [ADR-025: Evidence-as-a-Product](./ADR-025-Evidence-as-a-Product.md)
- [ADR-026: Protocol Adapters](./ADR-026-Protocol-Adapters.md)
- [SYNTHESIS-TRUST-CHAIN-TRIFECTA-2026q2](./SYNTHESIS-TRUST-CHAIN-TRIFECTA-2026q2.md)
- [RESULTS-EXPERIMENT-DELEGATION-SPOOFING-2026q2](./RESULTS-EXPERIMENT-DELEGATION-SPOOFING-2026q2.md)
- [RESULTS-EXPERIMENT-PROTOCOL-EVIDENCE-INTERPRETATION-2026q2](./RESULTS-EXPERIMENT-PROTOCOL-EVIDENCE-INTERPRETATION-2026q2.md)
- [OWASP Agentic A1/A3/A5 C1 Mapping](../security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md)
