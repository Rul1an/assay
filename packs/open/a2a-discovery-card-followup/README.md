# A2A Discovery / Card Follow-Up (P2c)

**License:** Apache-2.0
**Version:** 1.0.0
**Scope:** Companion pack for bounded **visibility** on G4-A **`payload.discovery`** emitted on canonical A2A adapter evidence — not verification, signing, or authorization outcomes.

## Overview

Rules evaluate JSON boolean `true` at frozen discovery pointers (see [G4-A Phase 1 freeze](../../../docs/architecture/G4-A-PHASE1-FREEZE.md)):

- **A2A-DC-001** — `agent_card_visible` observed as `true`
- **A2A-DC-002** — `extended_card_access_visible` observed as `true`

Uses `json_path_exists` with **`value_equals: true`** (pack engine) so `false` does not satisfy the rule.

## Rules

| Rule ID | Severity | Description |
| --- | --- | --- |
| `A2A-DC-001` | `warning` | At least one event has `/data/discovery/agent_card_visible` equal to boolean `true`. |
| `A2A-DC-002` | `warning` | At least one event has `/data/discovery/extended_card_access_visible` equal to boolean `true`. |

## Non-Goals

- Agent Card authenticity, signature validity, or trusted provenance
- Auth/authz correctness or “secure A2A” marketing claims
- `signature_material_visible` semantics (G4-A v1 defers `true`)

See [PLAN-P2c](../../../docs/architecture/PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md).

## Usage

```bash
assay evidence lint --pack a2a-discovery-card-followup bundle.tar.gz
```

## License

Apache-2.0 — see [LICENSE](./LICENSE)
