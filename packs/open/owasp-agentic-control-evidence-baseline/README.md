# OWASP Agentic Control-Evidence Baseline

**License:** Apache-2.0
**Version:** 1.0.0
**Scope:** Narrow control-evidence subset for `ASI01`, `ASI03`, and `ASI05`

## Overview

This pack ships a deliberately small subset of control-evidence checks derived
from the `C1` feasibility map in
[OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md](../../../docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md).

It is not a broad OWASP Agentic baseline. It checks only whether a baseline
flow records narrow evidence for:

- governance rationale context on decision events
- authorization context on decision events
- process-execution evidence in the baseline flow

## Rules

| Rule ID | Category | Severity | Description |
| --- | --- | --- | --- |
| `A1-002` | `ASI01` | `warning` | Decision events include governance rationale fields. |
| `A3-001` | `ASI03` | `warning` | Decision events capture authorization context. |
| `A5-001` | `ASI05` | `warning` | Process execution evidence is present in the baseline flow. |

## What The Rules Actually Check

- `A1-002` passes when at least one decision event contains `reason_code` or `approval_state`.
- `A3-001` passes when at least one decision event contains `principal` or `approval_state`.
- `A5-001` passes when at least one event matches `assay.process.exec`.

## Non-Goals

This pack does not prove:

- goal hijack detection
- privilege abuse prevention
- mandate linkage enforcement
- temporal validity of approvals or mandates
- delegation-chain visibility
- sandbox degradation detection

This pack proves only that process-execution evidence is present in the
baseline flow; it does not prove execution authorization, containment, or
sandboxing.

## Usage

```bash
assay evidence lint --pack owasp-agentic-control-evidence-baseline bundle.tar.gz
```

Or with other packs:

```bash
assay evidence lint --pack owasp-agentic-control-evidence-baseline,soc2-baseline bundle.tar.gz
```

## Design Constraints

- all shipped checks are supported by engine `1.0`
- no `conditional` or other skip-prone checks are included
- no linkage, delegation, temporal, or sandbox-degradation claims are shipped

## License

Apache-2.0 — see [LICENSE](./LICENSE)
