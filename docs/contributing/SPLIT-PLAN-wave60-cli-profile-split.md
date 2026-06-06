# Wave60 CLI Profile Split Plan

Goal:
- Mechanically split `crates/assay-cli/src/cli/commands/profile.rs` behind a stable command facade.
- Keep profile CLI behavior, output strings, scope guard semantics, run-id idempotency, merge logic, and perf telemetry unchanged.
- Close the Wave53/Wave59 command-hotspot follow-up without mixing behavior or schema changes.

Baseline:
- `crates/assay-cli/src/cli/commands/profile.rs`: 547 LOC on `origin/main` before Wave60.
- Existing tests cover aggregation, merge behavior, and scope guard behavior.

Split shape:
- `profile.rs`: stable facade that re-exports `run` and existing public CLI/event types.
- `profile_next/mod.rs`: CLI args, command dispatch, init/update/show command flow, scope guard, and perf telemetry.
- `profile_next/input.rs`: event schema and JSONL/stdin event loading.
- `profile_next/aggregate.rs`: per-run aggregation and profile merge logic.
- `profile_next/display.rs`: summary output and stability ranking display.
- `profile_next/tests.rs`: moved unit tests.

Non-goals:
- No profile CLI behavior, output, or exit-code changes.
- No profile schema or serialization changes.
- No evidence/profile type changes.
- No watch, run, dispatch, argument parser, Cargo, workflow, or dependency changes.

Review posture:
- Review as a move-only command split.
- Any profile behavior, output, or schema changes belong in a follow-up PR with explicit CLI/output contracts.
