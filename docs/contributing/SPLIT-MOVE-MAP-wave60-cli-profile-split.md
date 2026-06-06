# Wave60 CLI Profile Move Map

| Before | After | Notes |
| --- | --- | --- |
| `profile.rs::run` | `profile_next/mod.rs::run` | Re-exported by `profile.rs`. |
| `ProfileArgs`, `ProfileCmd`, arg structs | `profile_next/mod.rs` | Re-exported by `profile.rs` for public API compatibility. |
| `Event` | `profile_next/input.rs` | Re-exported by `profile.rs`. |
| `read_events` | `profile_next/input.rs` | JSONL/stdin behavior unchanged. |
| `ProfilePerfMetrics` | `profile_next/mod.rs` | Perf JSON/output behavior unchanged. |
| `cmd_init`, `cmd_update`, `cmd_show` | `profile_next/mod.rs` | Command flow unchanged. |
| `enforce_scope` | `profile_next/mod.rs` | Scope guard behavior unchanged. |
| `RunData`, `RunEntry`, `aggregate_run`, `merge_run` | `profile_next/aggregate.rs` | Aggregation and merge behavior unchanged. |
| `show_summary`, stability display helpers | `profile_next/display.rs` | Output strings unchanged. |
| `profile/tests.rs` | `profile_next/tests.rs` | Moved tests with imports adjusted. |

LOC delta:
- `crates/assay-cli/src/cli/commands/profile.rs`: 547 -> 5.
- New `profile_next/mod.rs`: 303.
- New `profile_next/aggregate.rs`: 98.
- New `profile_next/display.rs`: 95.
- New `profile_next/input.rs`: 44.
- New `profile_next/tests.rs`: 105.

Deferred:
- No profile output golden additions in this PR.
- No profile schema, evidence mapping, or runtime collector changes in this PR.
