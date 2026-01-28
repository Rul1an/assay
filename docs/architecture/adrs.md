# Architecture Decision Records

This directory contains Architecture Decision Records (ADRs) for the Assay project.

## Index

| ADR | Title | Status | Priority |
|-----|-------|--------|----------|
| [ADR-001](./adr-001-sandbox-design.md) | Sandbox Design | Accepted | - |
| [ADR-002](./ADR-002-Trace-Replay.md) | Trace Replay | Accepted | - |
| [ADR-003](./ADR-003-Gate-Semantics.md) | Gate Semantics | Accepted | - |
| [ADR-004](./ADR-004-Judge-Metrics.md) | Judge Metrics | Accepted | - |
| [ADR-005](./ADR-005-Relative-Thresholds.md) | Relative Thresholds | Accepted | - |
| [ADR-006](./ADR-006-Evidence-Contract.md) | Evidence Contract | Accepted | - |
| [ADR-007](./ADR-007-Deterministic-Provenance.md) | Deterministic Provenance | Accepted | - |
| [ADR-008](./ADR-008-Evidence-Streaming.md) | Evidence Streaming Architecture | Proposed | Backlog |
| [ADR-009](./ADR-009-WORM-Storage.md) | WORM Storage for Evidence Retention | Proposed | **P0** |
| [ADR-010](./ADR-010-Evidence-Store-API.md) | Evidence Store Ingest API | Proposed | **P1** |
| [ADR-011](./ADR-011-Tool-Signing.md) | MCP Tool Signing with Sigstore | Proposed | **P2** |
| [ADR-012](./ADR-012-Transparency-Log.md) | Transparency Log Integration | Proposed | **P3** |
| [ADR-013](./ADR-013-EU-AI-Act-Pack.md) | EU AI Act Compliance Pack | Proposed | **P2** |

## Q2 2026 Priorities

The following ADRs are prioritized for Q2 2026:

1. **P0: ADR-009** - WORM Storage (Low effort, High compliance value)
2. **P1: ADR-010** - Evidence Store API (Medium effort, Core commercial offering)
3. **P2: ADR-011** - Tool Signing (Medium effort, Security differentiator)
4. **P2: ADR-013** - EU AI Act Compliance Pack (Medium effort, Regulatory differentiator)
5. **P3: ADR-012** - Transparency Log (Low effort, Builds on ADR-011)

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
