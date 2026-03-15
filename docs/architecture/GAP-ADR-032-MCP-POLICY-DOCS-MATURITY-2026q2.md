# ADR-032 Documentation Maturity Gap Analysis (2026 Q2)

> Status: Current-state assessment after Wave42
> Scope: ADR-032 architecture, evidence, replay, and consumer-facing documentation
> Canonical ADR: [ADR-032](./ADR-032-MCP-Policy-Obligations-and-Evidence-v2.md)
> Current architecture view: [ADR-032 Implementation Overview](./OVERVIEW-ADR-032-MCP-POLICY-STACK-2026q2.md)

This page compares the current ADR-032 documentation set to current documentation and architecture best practices.
It is not a new source of truth. Its job is to make gaps and next steps explicit.

## Summary

Assay is already strong on:

- canonical docs-as-code in the repo
- ADR discipline
- wave-by-wave rollout history
- release-note framing for consumer impact
- Mermaid-based overview diagrams
- an internal Obsidian view layer that does not compete with repo truth

The highest-leverage remaining gaps were:

1. a formal building-block view,
2. explicit quality scenarios,
3. machine-readable component metadata for catalog/discovery,
4. stronger doc routing between decision, shape, rollout, and consumer framing.

Those gaps are now addressed in bounded form by:

- [ADR-032 Building Block View](./BUILDING-BLOCKS-ADR-032-MCP-POLICY-STACK-2026q2.md)
- [ADR-032 Quality Scenarios](./QUALITY-SCENARIOS-ADR-032-MCP-POLICY-STACK-2026q2.md)
- repository-level [`catalog-info.yaml`](../../catalog-info.yaml)
- [ADR-032 Structurizr Workspace](./STRUCTURIZR-ADR-032-WORKSPACE-2026q2.md)

## Comparative Matrix

| Practice area | Assay before this pass | Remaining gap before this pass | Recommended posture | Status after this pass |
|---|---|---|---|---|
| Docs as code | Strong | None material | Keep repo canonical | Good |
| ADR discipline | Strong | Needed richer downstream structure links | Keep ADR normative and link to supporting views | Improved |
| Historical rollout ledger | Strong | None material | Keep plan historical only | Good |
| Current architecture overview | Good | Missing explicit building-block decomposition | Add building-block view | Improved |
| Quality attributes | Partial | No dedicated quality-scenario document | Add explicit quality scenarios | Improved |
| Machine-readable discovery metadata | Missing | No catalog metadata | Add `catalog-info.yaml` | Improved |
| Published navigation | Partial | New architecture docs not yet first-class in docs IA | Wire into MkDocs + indexes | Improved |
| Internal knowledge view | Good | Navigation existed but topic routing was thin | Keep Obsidian as view layer only | Improved |
| Architecture-as-code model | Missing | No Structurizr/C4 workspace yet | Add bounded initial workspace, then add inspections/export later | Improved |

## External Benchmark Patterns

These patterns are the nearest fit to the current Assay line:

- Diataxis-style separation between explanation, reference, and operational guidance
- arc42-style separation of building blocks and quality scenarios
- Backstage/TechDocs-style repo-owned metadata and docs discovery
- C4/Structurizr-style architecture views and machine-readable model as the next maturity step

Assay should not copy these frameworks wholesale. The practical target is to adopt the parts that strengthen maintainability and consumer clarity without widening product scope.

## What Assay Already Does Well

### 1. Canonical ownership is clear
The repo is the source of truth.
Obsidian is the view layer.
This avoids split-brain architecture documentation.

### 2. Product and architecture boundaries are explicit
ADR-032 clearly states what Assay is and is not:

- policy enforcement and evidence layer
- not an IdP
- not a control-plane rewrite
- not a hidden policy-backend migration

### 3. Rollout discipline is unusually strong
The wave structure produced traceable history and deterministic closure on `main`.
That is already beyond what many architecture document sets achieve.

## The Main Gaps That Mattered

### Gap 1: No formal building-block view
The overview described the line well, but did not explicitly break the stack into stable building blocks and responsibilities.
That made it harder to reason about where future changes belong.

### Gap 2: No explicit quality scenarios
Determinism, replay stability, fail-closed behavior, and consumer compatibility were visible in the implementation line, but not captured as explicit architecture quality scenarios.
That limited reviewability.

### Gap 3: No machine-readable catalog metadata
The docs were readable by people, but not discoverable in the way a catalog or docs portal expects.

### Gap 4: Update routing was implicit
Maintainers still had to infer whether a change belongs in the ADR, overview, plan, or release notes.

## What Was Added In This Follow-Up

### Building block view
Added a dedicated building-block page that separates:

- policy bundle
- PEP runtime hook
- PDP/evaluator
- context envelope / PIP inputs
- obligation executor
- fail-closed selector
- decision projection
- evidence emitter
- replay/diff basis
- consumer readers

### Quality scenarios
Added an explicit quality-scenario document for:

- deterministic replay
- typed fail-closed behavior
- additive compatibility
- bounded runtime evolution
- consumer robustness
- auditability and evidence reconstruction

### Catalog metadata
Added `catalog-info.yaml` so the repo can participate cleanly in catalog/discovery flows without changing product scope.

## What Is Still Intentionally Not Done

### Full Structurizr automation
An initial bounded Structurizr/C4 workspace is now present.
What remains intentionally deferred is the heavier follow-up:

- CI-based validate/inspect gates
- automated export/publish flow
- deeper model generation or ADR import

### Full Backstage adoption
Not necessary unless multi-repo/service discovery becomes a real operational need.
The catalog file is enough to keep that option open.

### A second source of truth in Obsidian
Explicitly rejected.
Obsidian stays a navigation and insight layer.

## Recommended Next Step

If the team wants the next highest-leverage improvement after this pass, it should be:

1. inspection and export automation around the ADR-032 Structurizr workspace,
2. generated from the now-stable building-block and quality-scenario docs,
3. without changing runtime behavior or reopening the ADR line.

## Maintainer Rule of Thumb

If you are unsure where a future architecture change belongs:

- product meaning change -> ADR
- current shape change -> overview or building blocks
- quality requirement change -> quality scenarios
- historical wave landing -> plan
- downstream impact -> release notes
- discovery/catalog metadata -> `catalog-info.yaml`
