# ADR-026 Adapter Metadata Contract (E0)

## Intent
Ensure every adapter-emitted EvidenceEvent is traceable to:
- adapter implementation identity (`adapter_id`)
- adapter build/version (`adapter_version`)

This enables audit reproducibility and regression triage across protocol/version churn.

## Required fields (v1)
- `adapter_id`: stable identifier, e.g. `assay-adapter-acp`
- `adapter_version`: semver string (crate version)
- `protocol_name`: e.g. `acp`
- `protocol_version`: spec version string or range carried by the adapter output

## Placement
Metadata must be present in the adapter output envelope such that:
- it is carried into `EvidenceEvent` payload or extensions
- it is available even in lenient fallback events
- it survives raw-payload preservation / lossiness reporting paths

## Non-goals
- No behavior changes to mapping semantics
- No workflow changes
- No release-line publication changes
