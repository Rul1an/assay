# Wave E1 Step 1 Inventory

## Goal

Unlock the smallest real Pack Engine seam already present in the repo:

- typed per-event conditional presence semantics
- executable `event_types` filtering
- real execution of `MANDATE-001`

## In Scope

- `crates/assay-evidence/src/lint/packs/schema.rs`
- `crates/assay-evidence/src/lint/packs/checks.rs`
- `crates/assay-evidence/packs/mandate-baseline.yaml`
- `crates/assay-evidence/tests/pack_engine_conditional_test.rs`
- `crates/assay-evidence/tests/owasp_agentic_c1_mapping.rs`
- `crates/assay-evidence/tests/fixtures/packs/owasp-agentic-a3-probe.yaml`
- `docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md`
- `docs/architecture/SPEC-Mandate-v1.md`
- `docs/architecture/ADR-017-Mandate-Evidence.md`
- `docs/architecture/SPEC-Pack-Engine-v1.md`
- E1 wave artifacts and reviewer gate

## Out Of Scope

- reference existence checks
- temporal validity checks
- multi-event joins or correlation
- delegation-chain signals
- sandbox-degradation signals
- MCP/runtime behavior changes
- general policy-language expansion

## Frozen Contract

- `event_types` filters execution by exact event type match
- supported conditional shape:
  - `condition.all`
  - clauses with `path` + `equals`
  - `equals` is JSON-type-faithful and case-sensitive
  - `then.type = json_path_exists`
  - `then.paths` contains exactly one required path
- conditional execution is per-event only
- if no event matches the condition, the rule passes
- unsupported conditional shapes remain unsupported
- `MANDATE-001` becomes executable
- `MANDATE-002+` remain version-gated beyond `v1.1`
