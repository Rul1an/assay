# A2A Signal Follow-Up (P2b)

**License:** Apache-2.0
**Version:** 1.0.0
**Scope:** Companion pack for bounded **presence-only** checks on canonical A2A adapter evidence (`assay.adapter.a2a.*`), aligned with shipped `assay-adapter-a2a` — not external A2A spec completeness.

## Overview

This pack does **not** broaden baseline compliance packs. It checks that a bundle **observes** canonical adapter-emitted surfaces:

- **A2A-001** — `assay.adapter.a2a.agent.capabilities` (capability-**discovery** evidence present, not richness of metadata)
- **A2A-002** — `assay.adapter.a2a.task.*` (covers `task.requested` and `task.updated` via glob)
- **A2A-003** — `assay.adapter.a2a.artifact.shared` (artifact **exchange visibility** — shared signal observed)

Uses standard pack checks (`event_type_exists`); **no** pack engine bump beyond existing **v1.2** line — rules do **not** add G3-style authorization predicates.

## Rules

| Rule ID | Severity | Description |
| --- | --- | --- |
| `A2A-001` | `warning` | Evidence includes at least one canonical agent-capabilities discovery event (adapter-emitted capability-discovery signal present). |
| `A2A-002` | `warning` | Evidence includes at least one canonical task lifecycle event (`task.requested` and/or `task.updated` from the adapter). |
| `A2A-003` | `warning` | Evidence includes at least one canonical artifact.shared event (artifact exchange visibility on the adapter-emitted surface). |

## Non-Goals

This pack does not prove:

- authorization validity, issuer trust, or G3-equivalent decision semantics on A2A paths
- signed Agent Card, verified provenance, or extended-card authentication
- delegation/handoff correctness (including inferring handoff from `task.kind` alone)
- artifact integrity, provenance, or “safe” sharing — only **exchange visibility** on the canonical event type
- full coverage of an “A2A v1.0” marketing label — the adapter gates supported upstream **0.x** lines today; see [PLAN-P2b](../../../docs/architecture/PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md)

## Usage

```bash
assay evidence lint --pack a2a-signal-followup bundle.tar.gz
```

## Design Constraints

- companion pack only; baselines unchanged
- discovery-first: rules justified by adapter-emitted canonical types in [PLAN-P2b](../../../docs/architecture/PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md)

## License

Apache-2.0 — see [LICENSE](./LICENSE)
