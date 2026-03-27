# T-R1 Plan — `tests/decision_emit_invariant.rs` Integration Target Decomposition

## Goal

Split `crates/assay-core/tests/decision_emit_invariant.rs` into a multi-file integration-test
target without changing emitted decision-contract behavior, test target identity, or black-box
coverage meaning.

This plan intentionally follows Rust/Cargo integration-test conventions:

- keep this suite under `tests/`
- keep it as one coherent integration-test target by default
- decompose it internally via `tests/<target>/main.rs` plus submodules
- avoid fragmenting one contract surface into many top-level integration-test crates unless
  later CI or ownership pressure clearly justifies that

Current hotspot baseline on `origin/main @ 66c424c1`:

- `crates/assay-core/tests/decision_emit_invariant.rs`: `1293` LOC
- `crates/assay-core/src/mcp/decision.rs`: already split in Wave43 and now the primary emitted-decision companion
- `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`: already split in Wave44 and now a behavior companion
- `crates/assay-core/src/mcp/policy/engine.rs`: already split in Wave45 and now a policy-semantic companion

## Why this plan exists

`decision_emit_invariant.rs` is now one of the largest handwritten Rust files left on `main`,
but it is still a **black-box contract target**, not a production runtime hotspot.

That means the right split shape is:

- one integration-test binary
- one coherent emitted-decision contract surface
- shared fixtures/helpers kept inside that target
- no conversion into multiple top-level `tests/*.rs` crates as a first move

The primary reason for this shape is contract coherence, not just runtime. One integration target
keeps the emitted JSON assertions, fixture setup, and reviewer context together and avoids
duplicating setup across many integration crates.

## Frozen target surface

T-R1 freezes the expectation that any later split keeps these test-surface properties stable:

- the target name remains `decision_emit_invariant`
- the suite remains an integration-test target under `crates/assay-core/tests`
- the suite continues to validate emitted-decision behavior through black-box request/emitter flows
- the existing helpers remain semantically equivalent in setup intent:
  - `TestEmitter`
  - `make_tool_request`
  - `make_tool_request_with_args`
  - `approval_required_policy`
  - `restrict_scope_policy_with_contract`
  - `redact_args_policy_with_contract`
  - `approval_artifact`
- the current emitted-contract families remain stable in meaning:
  - allow/deny emission
  - delegation additive fields
  - approval-required deny paths
  - restrict-scope deny/additive-field paths
  - redact-args deny/additive-field paths
  - guard drop/panic emission
  - required emitted field coverage
  - G3 auth projection filtering

## Step1 (freeze)

Step1 should be docs/gates only.

Step1 constraints:

- no edits under `crates/assay-core/tests/decision_emit_invariant.rs`
- no edits under `crates/assay-core/src/mcp/**`
- no edits under `crates/assay-core/src/mcp/policy/**`
- no workflow edits
- no event-shape, reason-code, or emitted JSON contract drift

Step1 gate should pin representative tests from each family, for example:

- `cargo test -q -p assay-core --test decision_emit_invariant test_policy_allow_emits_once -- --exact`
- `cargo test -q -p assay-core --test decision_emit_invariant approval_required_missing_denies -- --exact`
- `cargo test -q -p assay-core --test decision_emit_invariant restrict_scope_target_missing_denies -- --exact`
- `cargo test -q -p assay-core --test decision_emit_invariant redact_args_target_missing_denies -- --exact`
- `cargo test -q -p assay-core --test decision_emit_invariant test_guard_emits_on_panic -- --exact`
- `cargo test -q -p assay-core --test decision_emit_invariant test_event_contains_required_fields -- --exact`
- `cargo test -q -p assay-core --test decision_emit_invariant g3_auth_projection_emits_allowlisted_scheme_trimmed_issuer_principal_in_decision_json -- --exact`

## Step2 (mechanical split preview)

Step2 should replace the single-file target with one multi-file target directory while preserving
the test target name and its black-box contract role.

Target layout:

- `crates/assay-core/tests/decision_emit_invariant/main.rs`
- `crates/assay-core/tests/decision_emit_invariant/fixtures.rs`
- `crates/assay-core/tests/decision_emit_invariant/emission.rs`
- `crates/assay-core/tests/decision_emit_invariant/approval.rs`
- `crates/assay-core/tests/decision_emit_invariant/restrict_scope.rs`
- `crates/assay-core/tests/decision_emit_invariant/redaction.rs`
- `crates/assay-core/tests/decision_emit_invariant/guard.rs`
- `crates/assay-core/tests/decision_emit_invariant/delegation.rs`
- `crates/assay-core/tests/decision_emit_invariant/g3_auth.rs`

Step2 principles:

- keep one integration-test binary by default
- keep `main.rs` as the test-target root and top-level module wiring only
- move helper/setup code into `fixtures.rs`
- move test bodies by scenario family, not by arbitrary file-size chunks
- keep the suite black-box; do not convert any part of it into white-box/private-item tests
- do not introduce a second integration-test target in the first cut

Step2 family ownership:

- `fixtures.rs`: emitters, request builders, policy builders, artifact builders
- `emission.rs`: allow/deny, multiple-call emission, event-source, tool-call-id, required fields, non-tool-call, obligation basics
- `approval.rs`: `approval_required_*`
- `restrict_scope.rs`: `restrict_scope_*`
- `redaction.rs`: `redact_args_*`
- `guard.rs`: guard drop/panic tests
- `delegation.rs`: additive delegation-field tests
- `g3_auth.rs`: G3 auth projection filtering tests

## Step3 (closure)

Step3 should be docs/gates only.

Step3 constraints:

- keep `crates/assay-core/tests/decision_emit_invariant/main.rs` as the stable target root
- no new module cuts
- no promotion to multiple integration binaries
- no drift in emitted-contract assertions
- no fixture cleanup beyond notes/follow-ups

## Reviewer notes

Primary failure modes:

- splitting one contract target into many top-level integration crates without a real need
- moving shared fixtures into duplicated per-file setup
- drifting emitted decision JSON expectations under a “test-only refactor” label
- leaking white-box assumptions into a suite that should stay black-box
- mixing production behavior changes into a test decomposition wave

## Non-goals

- No production edits under `crates/assay-core/src/**`.
- No new integration-test binaries in the first split.
- No test-helper sharing via `tests/common/mod.rs` unless a later wave actually introduces
  multiple top-level integration targets.
- No ownership reshuffle of emitted decision semantics.
- No renaming of test target, contract families, or assertion intent.
