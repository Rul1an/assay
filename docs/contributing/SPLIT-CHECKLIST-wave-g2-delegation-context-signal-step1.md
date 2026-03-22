# Split checklist: wave-g2-delegation-context-signal-step1

## Scope lock

- existing carrier stays `assay.tool.decision`
- no new event type
- no pack YAML changes
- no engine/pack semantics beyond explicit metadata plumbing

## Contract checks

- `delegated_from` is optional and additive
- `delegation_depth` is optional and additive
- `delegation_depth` is emitted only when explicitly present and valid
- direct flows emit no delegation fields
- unstructured hints emit no delegation fields

## Review anchors

- `_meta.delegation` is the only supported source
- `delegated_from` is the gating signal
- docs explicitly say no chain completeness or integrity is implied
- existing consumers without the new fields remain valid

## Validation

- `cargo fmt --check`
- `cargo clippy -q -p assay-core -p assay-evidence --all-targets -- -D warnings`
- targeted core/evidence tests
- `BASE_REF=origin/main bash scripts/ci/review-wave-g2-delegation-context-signal-step1.sh`
