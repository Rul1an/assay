# Assay Roadmap: Q1-Q2 2026

**Strategic Focus:** Evidence-as-a-Product & Stability Assurance.
**Context:** Shifting from "Observability/Sim" to "Audit Kit & Policy Soak" [ADR-025].

## Timeline & Priorities

### Phase 1: Audit Kit Baseline (Iteration 1) [HIGH PRIORITY]
**Target: Weeks 1-2**
*Goal: Ship the MVP "Audit Kit" enabling users to produce and verify evidence with compliance context.*

*   **Evidence Engine:**
    *   [ ] Manifest schema extensions: `x-assay.packs_applied` & `mappings` (provenance).
    *   [ ] CLI: `assay evidence lint` displays applied packs and provenance.
*   **Sim Engine (Pivot):**
    *   [ ] `assay sim soak` MVP: Run N iterations, seeded, with pass^k reporting.
    *   [ ] Report: JSON output with strict schema `soak-report-v1`.
*   **Compliance Packs:**
    *   [ ] `soc2-baseline` & `cicd-starter`: Documentation hygiene, "Required Signals" definitions.

### Phase 2: Closure & Explainability (Iteration 2) [MEDIUM PRIORITY]
**Target: Weeks 3-4**
*Goal: Make evidence "Replay-Ready" and actionable.*

*   **Evidence Engine:**
    *   [ ] Completeness Matrix: Pack-relative signal gaps (`required` vs `captured`).
    *   [ ] Closure Score v1: Deterministic 0.0-1.0 score for replayability.
    *   [ ] Explainability: `assay evidence lint --explain closure` guidance.

### Phase 3: OTEL & Advanced Stability (Iteration 3) [LOWER PRIORITY]
**Target: Weeks 5-6**
*Goal: Adoption via Standards & Deep Assurance.*

*   **Integrations:**
    *   [ ] OTEL Bridge: Export Assay events to OTLP/GenAI SemConv (opt-in).
    *   [ ] Mapping Loss Report: "What didn't fit in OTEL?".
*   **Sim Engine:**
    *   [ ] Advanced Soak: Drift metrics, P95 latency tracking, `infra` vs `policy` error breakdown.
*   **Security (Pro prep):**
    *   [ ] Attestation Envelope (DSSE) prototype.

---

## Future / "Pro" Horizon (Q2+)
*   **Enforcement Gateway:** Real-time blocking based on policy packs.
*   **Signed Evidence:** Key management & transparency logs.
*   **Air-Gapped Replay:** Hermetic replay with closure constraints.
