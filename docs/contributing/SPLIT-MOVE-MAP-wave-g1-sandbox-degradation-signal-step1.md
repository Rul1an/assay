# Wave G1 Step 1 Move Map

## Runtime Observation

- `crates/assay-cli/src/cli/commands/sandbox.rs`
  - adds the two frozen degradation emit points
  - writes a machine-readable evidence-profile sidecar

## Structured Observation

- `crates/assay-cli/src/profile/events.rs`
  - adds structured `SandboxDegraded` profile event support
- `crates/assay-cli/src/profile/mod.rs`
  - stores deduplicated degradation observations
  - converts a finished sandbox profile report into an evidence-export profile

## Evidence Contract / Export

- `crates/assay-evidence/src/types.rs`
  - tightens `PayloadSandboxDegraded` into typed fields
- `crates/assay-cli/src/cli/commands/profile_types.rs`
  - adds `sandbox_degradations` to the profile schema
- `crates/assay-cli/src/cli/commands/evidence/mapping.rs`
  - maps stored degradation observations into `assay.sandbox.degraded`

## Validation / Repo Truth

- `crates/assay-cli/tests/evidence_test.rs`
- `crates/assay-cli/tests/profile_integration_test.rs`
- `crates/assay-evidence/tests/owasp_agentic_c1_mapping.rs`
- docs/spec updates for the narrowed `A5-002` truth
