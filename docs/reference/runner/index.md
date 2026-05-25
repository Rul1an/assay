# Assay-Runner Reference

> **Status:** Assay-Runner is an internal measured-run subsystem of Assay,
> now split into extraction-ready Rust crates (`assay-runner-schema`,
> `assay-runner-core`, `assay-runner-linux`) — all `publish = false` —
> plus the `runner-fixtures/` package tree (Node fixture marked
> `"private": true`; Python fixture has no distribution surface).
> Everything stays inside this repository. Not a standalone product.
> No release commitment. The extraction question is gated by the
> [Phase 2D consolidation audit](phase-2d-consolidation-audit.md).

Internal reference material for the Assay-Runner measured-run candidate.

Assay-Runner is not a released product surface yet. These references freeze
the Phase 2A internal contracts needed to keep the delegated Linux/eBPF proof
reviewable while the runner boundary is consolidated, then anchor the first
Phase 2B capability-diff planning slice.

- [Runner artifact v0 contracts](artifacts-v0.md)
- [Runner artifact golden shapes](golden/index.md)
- [Runner acceptance fixture v0 contract](fixtures-v0.md)
- [Runner CI lane contract](ci-lanes.md)
- [Runner Dependabot lane flow](dependabot-lane-flow.md)
- [Runner capability-diff Phase 2B plan](capability-diff-plan.md)
- [Runner capability-diff v0 contract](capability-diff-v0.md)
- [Runner cross-runtime diff Phase 2C plan](cross-runtime-diff-plan.md)
- [Runner cross-runtime diff Phase 2C decisions (A1+B3+C1)](cross-runtime-diff-decisions.md)
- [Runner cross-runtime diff v0 contract](cross-runtime-diff-v0.md)
- [Runner projection roadmap](projection-roadmap.md)
- [Runner cross-runtime diff v0 clean-output JSON Schema](schema/cross-runtime-diff-v0-clean.schema.json)
- [Runner second runtime Phase 2B plan](second-runtime-plan.md)
- [Runner second runtime candidate selection](second-runtime-candidate-selection.md)
- [Runner Gemini fixture design](gemini-fixture-design.md)
- [Assay-Runner boundary and extraction map](boundary-map.md)
- [Runner platform and extraction readiness](platform-and-extraction-readiness.md)
- [Assay-Runner extraction roadmap (Phase 2D slice sequence)](extraction-roadmap.md)
- [Assay consumes Runner as external — Slice 6A design note](assay-consumes-runner-external.md)
- [Phase 2D consolidation audit (burn-in criteria, not calendar wait)](phase-2d-consolidation-audit.md)
- [Measured-run proof-bundle walkthrough (read-only, explainable demo)](examples/measured-run-proof-bundle.md)
- [Phase 1 delegated proof pack](proof-packs/phase1-delegated-2026-05-21.md)
