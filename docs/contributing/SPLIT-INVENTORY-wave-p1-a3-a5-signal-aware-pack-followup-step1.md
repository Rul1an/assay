# Wave P1 Inventory

## Goal

Ship a small companion pack that lets `G1` and `G2` land productmatig without
broadening the baseline pack:

- `A3-003` for supported delegated flows
- `A5-002` for supported containment fallback paths

## In Scope

- new built-in companion pack YAML
- open-pack mirror, README, and LICENSE
- pack registry update
- focused assay-evidence tests
- narrow `C1` mapping / probe alignment
- reviewer gate + wave artifacts

## Out Of Scope

- baseline pack changes
- engine changes
- signal-emitter changes
- delegation validation / chain integrity
- inherited-scope validation
- temporal/reference semantics
- sandbox correctness claims

## Key Files

- `crates/assay-evidence/packs/owasp-agentic-a3-a5-signal-followup.yaml`
- `packs/open/owasp-agentic-a3-a5-signal-followup/pack.yaml`
- `packs/open/owasp-agentic-a3-a5-signal-followup/README.md`
- `packs/open/owasp-agentic-a3-a5-signal-followup/LICENSE`
- `crates/assay-evidence/src/lint/packs/mod.rs`
- `crates/assay-evidence/tests/owasp_agentic_p1_signal_followup.rs`
- `crates/assay-evidence/tests/owasp_agentic_c1_mapping.rs`
- `crates/assay-evidence/tests/fixtures/packs/owasp-agentic-a3-probe.yaml`
- `docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md`

## Behavior Freeze

- baseline pack stays exactly `A1-002`, `A3-001`, `A5-001`
- companion pack ships exactly `A3-003`, `A5-002`
- `A3-003` requires only `/data/delegated_from`
- `delegation_depth` remains optional supporting context
- `A5-002` remains `event_type_exists(pattern=assay.sandbox.degraded)`
