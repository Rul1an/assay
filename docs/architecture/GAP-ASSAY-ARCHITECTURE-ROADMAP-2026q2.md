# Assay Architecture & Roadmap Gap Analysis (Q2 2026)

## Status

Current-state review as of 2026-03-15 against code on `main`, merged PR history, and the canonical architecture/roadmap documents.

## Executive Read

Assay's architecture is currently ahead of its roadmap bookkeeping.

The strongest, most closed-loop lines are:
- evidence and replay
- MCP policy enforcement and obligations
- protocol adapters
- split-wave delivery discipline

The weakest lines are not missing architecture, but status convergence and a small number of user-facing delivery gaps:
- roadmap/status drift across ADRs and RFCs
- partial closure of ADR-015 BYOS Phase 1
- changelog/release aggregation that is noisier than the underlying architecture

## Capability Matrix

| Lane | Assay already has | What is still missing or partial | Recommended next move |
|------|-------------------|----------------------------------|-----------------------|
| Core governance + evidence | Evidence bundles, verification, lint/diff/explore, deterministic replay contracts | Release/changelog aggregation is less mature than the architecture itself | Keep release notes consumer-facing and treat changelog cleanup as a bounded hygiene pass |
| MCP runtime governance (ADR-032) | Wave24-Wave42 closed-loop on `main`: typed decisions, obligations execution, approval/restrict/redact enforcement, convergence and consumer hardening | No open execution blocker in ADR-032 itself | Keep compatibility paths stable; open only new bounded waves |
| Protocol adapters (ADR-026) | ACP/A2A/UCP adapter posture is accepted and implemented | Cross-repo discoverability could improve | Keep adapter architecture stable; use catalog metadata and published docs for discovery |
| DX/refactor governance (RFC-001/002/003/004) | Wave A/B landed, RFC-002 and RFC-003 complete, RFC-004 closure merged | Wave C remains intentionally data-gated, not an active delivery track | Treat RFC-001 as historical governance guidance unless new performance data justifies revival |
| BYOS storage (ADR-015) | Phase 1 complete: `push/pull/list/store-status`, `.assay/store.yaml` config, provider quickstart docs (S3, B2, MinIO) | Phase 2 (GitHub Action integration) and Phase 3 (managed store) remain future work | Phase 1 is closed; next bounded move is Phase 2 if demand justifies it |
| Tool signing and transparency | OSS signing is shipped; keyless/transparency remain explicitly later-stage | Sigstore keyless and transparency-log layers are still future work | Keep deferred until demand justifies the operational cost |
| Docs-as-code maturity | ADR-032 overview, building blocks, quality scenarios, Structurizr workspace, catalog metadata, MkDocs wiring | Repo-wide architecture maturity was less explicit before this pass | Keep the repo-wide gap view current and use it to decide sequencing |

## What Assay Already Has

### Product-strong lanes

- **Evidence-as-a-product**: the evidence bundle, verification, and replay stack is no longer just support plumbing; it is a first-class product surface.
- **Governance on the tool bus**: Assay has moved beyond static lexical checks into deterministic runtime governance with replayable decisions and evidence.
- **Protocol-agnostic positioning**: adapters keep protocol churn out of the evidence/governance core.
- **Closed-loop wave discipline**: architecture, implementation, gates, and closure docs have been landing together instead of drifting apart.

### Documentation-strong lanes

- ADR-032 now has:
  - normative ADR
  - maintainer overview
  - building block view
  - quality scenarios
  - execution plan
  - Structurizr workspace
  - Obsidian view-layer guidance
- Architecture routing in `docs/architecture/` is substantially stronger than it was pre-Wave42.

## What Assay Still Needs

### 1. ~~ADR-015 Phase 1 product closure~~ ✅ Closed

ADR-015 Phase 1 is now complete on `main`:
- shipped: `push`, `pull`, `list`, `store-status`
- shipped: `.assay/store.yaml` structured config (option A, separate from eval config)
- shipped: provider quickstart docs (AWS S3, Backblaze B2, MinIO)
- shipped: CLI integration tests with `file://` backend

This was the biggest strategy-to-delivery gap; it is now closed.

### 2. Release/changelog hygiene

The architecture and rollout docs are currently cleaner than the aggregate changelog.

That is not a product blocker, but it weakens external readability and release accounting. The right fix is a bounded hygiene pass, not new architecture.

### 3. Next architecture-as-code step

The current Structurizr/C4 workspace is intentionally bounded and docs-first.

Still missing, if Assay wants to move from strong docs-as-code to stronger architecture-as-code:
- automated Structurizr validation/export
- inspections in CI
- broader component-view discipline outside ADR-032

This is valuable, but should come after the roadmap truth and BYOS closure work.

## Recommended Order

1. ~~**Roadmap truth sync**~~ ✅ Done (PR #857)

2. ~~**ADR-015 Phase 1 closure**~~ ✅ Done (PR #859 Step 1, PR #860 Step 2, this PR Step 3)

3. **Release/changelog hygiene**
   - Keep release notes as the consumer-facing truth.
   - Reduce duplicated changelog aggregation.

4. **Next architecture-as-code slice**
   - Add CI validation/export around the Structurizr workspace.
   - Do not mix that work with runtime behavior changes.

## Decision Rule

When choosing the next bounded slice, prefer the one that improves the most truthfulness with the least new runtime surface:

- first: status convergence
- then: incomplete but already-decided product lanes
- only then: new capability lanes

That ordering fits Assay's current state on `main`.
