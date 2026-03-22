# Wave G1 Step 1 Inventory

## Goal

Close the smallest remaining sandbox signal gap:

- emit typed `assay.sandbox.degraded` evidence
- only for weaker-than-requested containment while execution continued
- only for the two supported Landlock fallback paths

## In Scope

- `crates/assay-evidence/src/types.rs`
- `crates/assay-cli/src/profile/events.rs`
- `crates/assay-cli/src/profile/mod.rs`
- `crates/assay-cli/src/cli/commands/profile_types.rs`
- `crates/assay-cli/src/cli/commands/evidence/mapping.rs`
- `crates/assay-cli/src/cli/commands/evidence/mod.rs`
- `crates/assay-cli/src/cli/commands/sandbox.rs`
- `crates/assay-cli/tests/evidence_test.rs`
- `crates/assay-cli/tests/profile_integration_test.rs`
- `crates/assay-evidence/tests/owasp_agentic_c1_mapping.rs`
- `docs/architecture/ADR-006-Evidence-Contract.md`
- `docs/architecture/evidence-metrics-mapping.md`
- `docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md`
- `docs/reference/cli/sandbox.md`
- `docs/concepts/traces.md`
- G1 wave artifacts and reviewer gate

## Out Of Scope

- new A5 shipped pack rules
- delegation-chain signals
- eBPF or monitor broadening
- engine or pack-language changes
- general sandbox health telemetry
- fail-closed evidence events
- intentional audit/permissive mode signaling

## Frozen Contract

- emit only when stronger containment was requested, weaker containment became effective, and execution continued
- no event for intentional audit/permissive runs
- no event for fail-closed denial or abort
- only the following reason codes in `G1`:
  - `backend_unavailable`
  - `policy_conflict`
- only the following degradation mode in `G1`:
  - `audit_fallback`
- only the following component in `G1`:
  - `landlock`
- `detail` is optional and non-authoritative
- at most one degradation event per run per `(component, reason_code)`
