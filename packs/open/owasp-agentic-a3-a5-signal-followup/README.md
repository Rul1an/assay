# OWASP Agentic A3/A5 Signal-Aware Follow-Up

**License:** Apache-2.0
**Version:** 1.0.0
**Scope:** Signal-aware follow-up for supported delegated flows and supported containment fallback paths

## Overview

This companion pack ships a deliberately small follow-up surface derived from
the `C1` feasibility map in
[OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md](../../../docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md).

It does not broaden the baseline pack. It checks only whether evidence records:

- delegated authority context on `assay.tool.decision` for supported delegated flows
- containment degradation fallback evidence for supported weaker-than-requested paths

## Rules

| Rule ID | Category | Severity | Description |
| --- | --- | --- | --- |
| `A3-003` | `ASI03` | `warning` | Decision evidence surfaces delegated authority context for supported delegated flows. |
| `A5-002` | `ASI05` | `warning` | Evidence records supported containment degradation fallback paths. |

## What The Rules Actually Check

- `A3-003` passes when at least one `assay.tool.decision` event for supported delegated flows contains `delegated_from`.
- `delegation_depth` may appear as supporting context for `A3-003`, but it is not required by the shipped rule.
- `A5-002` passes when at least one event matches `assay.sandbox.degraded`.

## Non-Goals

This pack does not prove:

- delegation chain integrity
- delegation validity
- inherited-scope correctness
- temporal delegation correctness
- sandbox correctness
- all containment failures detected

This pack proves only signal-aware evidence for supported delegated flows and
supported containment fallback paths. It does not validate delegation chains,
cryptographic provenance, inherited scopes, temporal authorization, or overall
containment guarantees.

## Usage

```bash
assay evidence lint --pack owasp-agentic-a3-a5-signal-followup bundle.tar.gz
```

Or alongside the narrow baseline:

```bash
assay evidence lint --pack owasp-agentic-control-evidence-baseline,owasp-agentic-a3-a5-signal-followup bundle.tar.gz
```

## Design Constraints

- this is a companion pack; the baseline pack remains unchanged
- `A3-003` is bounded to supported delegated flows
- `A5-002` stays presence-only
- no delegation validation, chain integrity, temporal semantics, or broader containment-assurance claims are shipped

## License

Apache-2.0 — see [LICENSE](./LICENSE)
