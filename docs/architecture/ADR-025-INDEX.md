# ADR-025 Index (Contract)

## Intent
Provide a single, stable entry point for ADR-025 deliverables (I1/I2/I3), including:
- Where to start (first reads)
- Where artifacts live (schemas, scripts, workflows)
- How to validate locally (reviewer gates + test runners)
- What is informational vs release-lane evidence vs fail-closed

This document defines the **structure/contract** of the ADR-025 index.
Content population happens in PR-B.

## Scope
In-scope:
- Document structure/sections and naming
- Link targets and canonical paths (to be populated in PR-B)
- Non-goals and update policy

Out-of-scope:
- Any runtime changes
- Any workflow changes
- Refactoring existing docs

## Index structure (frozen)
1. **Quick Start (Where to look first)**
   - One-screen “start here” for new devs
2. **Iterations Overview**
   - I1: Soak + readiness + release enforcement
   - I2: Closure (completeness/provenance) + release attach
   - I3: OTel bridge + release attach
3. **Artifacts Map**
   - Schemas
   - Scripts (generators, enforce/attach, tests)
   - Workflows (informational lanes + release integration)
4. **Reviewer Gates Map**
   - Step-level reviewer scripts (A/B/C)
   - Stabilization gates (where applicable)
5. **Operational Runbooks**
   - I2 closure release runbook
   - I3 otel release runbook
6. **Contracts**
   - Artifact names + retention
   - Mode contracts (off|attach|warn|enforce)
   - Exit contracts (0/1/2; plus infra distinctions where used)
7. **Maintenance policy**
   - When to update the index
   - What changes require a freeze PR

## Maintenance policy (frozen)
- The index is updated only via small PRs following A/B/C discipline.
- PR-A defines structure; PR-B populates content; PR-C closes with checklist/review-pack + plan/roadmap sync.
