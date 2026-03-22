# Wave P1 Move Map

## New shipped surface

- `owasp-agentic-a3-a5-signal-followup`
  - `A3-003`
  - `A5-002`

## Registry

- add new built-in pack entry in `crates/assay-evidence/src/lint/packs/mod.rs`

## Open mirror

- add open mirror YAML
- add README with explicit non-goals
- copy Apache-2.0 LICENSE

## Probe alignment

- narrow `owasp-agentic-a3-probe.yaml` so `A3-003` rests on `delegated_from`
- keep `delegation_depth` as optional context in docs/tests only

## Doc truth

- update `docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md`
  - `A3-003` -> supported delegated flows, `delegated_from` seam
  - `A5-002` -> presence-only supported fallback-path seam
  - baseline unchanged; companion pack shipped separately
