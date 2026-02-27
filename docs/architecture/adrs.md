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
| [ADR-009](./ADR-009-WORM-Storage.md) | WORM Storage for Evidence Retention | **Deferred** | Q3+ |
| [ADR-010](./ADR-010-Evidence-Store-API.md) | Evidence Store Ingest API | **Deferred** | Q3+ |
| [ADR-011](./ADR-011-Tool-Signing.md) | MCP Tool Signing with Sigstore | Proposed | **P1** |
| [ADR-012](./ADR-012-Transparency-Log.md) | Transparency Log Integration | Proposed | **P3** |
| [ADR-013](./ADR-013-EU-AI-Act-Pack.md) | EU AI Act Compliance Pack | Accepted | **P2** |
| [ADR-014](./ADR-014-GitHub-Action-v2.md) | GitHub Action v2 Design | **Implemented** | ✅ |
| [ADR-015](./ADR-015-BYOS-Storage-Strategy.md) | BYOS Storage Strategy | **Accepted** | **P1** |
| [ADR-021](./ADR-021-Local-Pack-Discovery.md) | Local Pack Discovery and Pack Resolution Order | **Accepted** | **P2** |
| [ADR-022](./ADR-022-SOC2-Baseline-Pack.md) | SOC2 Baseline Pack (AICPA Trust Service Criteria) | **Accepted** | **P2** |
| [ADR-023](./ADR-023-CICD-Starter-Pack.md) | CICD Starter Pack (Adoption Floor) | **Accepted** | **P1** |
| [ADR-024](./ADR-024-Sim-Engine-Hardening.md) | Sim Engine Hardening (Limits + Time Budget) | Proposed | **P2** |
| [ADR-025](./ADR-025-Evidence-as-a-Product.md) | Evidence-as-a-Product | Accepted | **P1/P2** |
| [ADR-026](./ADR-026-Protocol-Adapters.md) | Protocol Adapters | Accepted | **P1** |
| [ADR-020](./ADR-020-Dependency-Governance.md) | Dependency Governance | Accepted | - |

## Q2 2026 Priorities

**Strategy:** BYOS-first (Bring Your Own Storage) per ADR-015. Focus on CLI features, defer managed infrastructure until PMF.

| Priority | ADR | Status | Notes |
|----------|-----|--------|-------|
| ✅ | [ADR-014](./ADR-014-GitHub-Action-v2.md) | Implemented | [Marketplace](https://github.com/marketplace/actions/assay-ai-agent-security) |
| **P1** | [ADR-015](./ADR-015-BYOS-Storage-Strategy.md) | Accepted | `push/pull/list` with S3-compatible storage |
| **P1** | [ADR-011](./ADR-011-Tool-Signing.md) | Proposed | `x-assay-sig` + local-key signing in OSS; Sigstore keyless deferred to enterprise |
| **P1** | [ADR-023](./ADR-023-CICD-Starter-Pack.md) | Accepted | OSS starter adoption floor (implemented) |
| **P2** | [ADR-021](./ADR-021-Local-Pack-Discovery.md) | Accepted | Local pack discovery + safe resolution order (implemented) |
| **P2** | [ADR-022](./ADR-022-SOC2-Baseline-Pack.md) | Accepted | SOC2 baseline OSS pack (implemented) |
| **P1/P2** | [ADR-025](./ADR-025-Evidence-as-a-Product.md) | Accepted | I1/I2/I3 slices merged on `main`; closed-loop rollout complete |
| **P2** | [ADR-013](./ADR-013-EU-AI-Act-Pack.md) | Accepted | Article 12 baseline pack implemented via `--pack eu-ai-act-baseline` |
| **P1** | [ADR-026](./ADR-026-Protocol-Adapters.md) | Accepted | ACP and A2A adapter MVP slices merged on `main` |
| **P3** | [ADR-012](./ADR-012-Transparency-Log.md) | Proposed | Builds on ADR-011 |
| Deferred | [ADR-009](./ADR-009-WORM-Storage.md) | Deferred | Managed WORM → Q3+ if demand |
| Deferred | [ADR-010](./ADR-010-Evidence-Store-API.md) | Deferred | Managed API → Q3+ if demand |

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
