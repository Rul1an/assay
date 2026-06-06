# Wave60 CLI Profile Review Pack

Scope:
- Move-only split of the CLI profile command behind a stable facade.
- Review paths are limited to `profile.rs`, `profile_next/*`, Wave60 docs, and the Wave60 review gate.

Reviewer checks:
- `profile.rs` should only route to `profile_next` and re-export the previous public command/event surface.
- `profile_next/mod.rs` should own command flow, args, scope guard, and perf telemetry.
- `profile_next/input.rs` should own only event decoding/loading.
- `profile_next/aggregate.rs` should own only aggregation and merge logic.
- `profile_next/display.rs` should own only summary rendering and stability display.
- `profile_next/tests.rs` should contain the moved aggregation/merge/scope tests.

Required local gate:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave60-cli-profile-split.sh
```

Expected behavior:
- No profile CLI output, schema, merge, scope, run-id, or perf telemetry behavior changes.
- No Cargo/workflow/dependency drift.
- No watch/run/dispatch/args drift.

Known residual risk:
- The split is mechanical; residual risk is import-path equivalence and module visibility. The moved unit tests and scoped gate cover the command surface and critical helper ownership.
