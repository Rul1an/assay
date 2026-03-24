# MCP Signal Follow-Up (P2a)

**License:** Apache-2.0
**Version:** 1.0.0
**Scope:** Companion pack for G3 authorization context, G2 delegation visibility, and G1 containment degradation — supported flows only

## Overview

This pack does **not** broaden baseline compliance packs. It productizes shipped signals:

- **MCP-001** — Same G3 v1 predicate as Trust Basis `authorization_context_visible` (verified): policy-projected `principal`, allowlisted `auth_scheme`, and `auth_issuer` on one `assay.tool.decision` event.
- **MCP-002** — Delegation context (`delegated_from`) on supported delegated flows.
- **MCP-003** — Containment degradation (`assay.sandbox.degraded`).

Requires pack engine **v1.2** (for `g3_authorization_context_present`).

## Rules

| Rule ID | Severity | Description |
| --- | --- | --- |
| `MCP-001` | `warning` | Decision evidence surfaces policy-projected authorization context for supported MCP flows. |
| `MCP-002` | `warning` | Decision evidence surfaces delegated authority context for supported delegated flows. |
| `MCP-003` | `warning` | Evidence records supported containment degradation fallback paths. |

## Non-Goals

This pack does not prove:

- authorization validity, issuer trust, or scope sufficiency
- delegation chain integrity or temporal delegation correctness
- sandbox correctness or full containment coverage
- transport-level OAuth at MCP `initialize` or raw JWT dumps in evidence

## Usage

```bash
assay evidence lint --pack mcp-signal-followup bundle.tar.gz
```

## Design Constraints

- companion pack only; baselines unchanged
- MCP-001 aligns with Trust Basis G3 semantics (shared predicate in `assay-evidence`)
- MCP-002/003 reuse standard pack checks (presence-only)

## License

Apache-2.0 — see [LICENSE](./LICENSE)
