# Wave 3 Step 2 move map (function-first)

Scope:
- `crates/assay-cli/src/cli/commands/monitor.rs`
- `crates/assay-core/src/providers/trace.rs`

All public entrypoints remain in facade files. Bodies moved mechanically to
`*_next` modules.

## Module responsibility legend

Monitor split (`monitor_next/*`):
- `mod.rs`: orchestration only.
- `normalize.rs`: path and cgroup normalization only.
- `rules.rs`: rule compile/match only.
- `events.rs`: event decode/dispatch + enforcement hook.
- `output.rs`: output sink (stdout/stderr + formatting).
- `syscall_linux.rs`: Linux syscall/`unsafe` boundary.

Trace split (`trace_next/*`):
- `mod.rs`: load orchestration only.
- `io.rs`: file IO only.
- `parse.rs`: JSONL/legacy parse + line-context errors.
- `v2.rs`: typed event precedence/state handling.
- `normalize.rs`: trace fingerprint normalization.
- `errors.rs`: error constructor helpers.

## Monitor (`cli::commands::monitor`)

| Old symbol/path | New implementation |
| --- | --- |
| `run(args)` | facade in `monitor.rs` -> `monitor_next::run` |
| Linux run-loop path | `monitor_next::mod::run_linux` |
| path normalization helpers | `monitor_next::normalize::{normalize_path_syntactic, resolve_cgroup_id}` |
| rule compile/match path | `monitor_next::rules::{compile_globset, compile_active_rules, find_violation_rule}` |
| event handling path | `monitor_next::events::handle_event` |
| syscall + unsafe helpers | `monitor_next::syscall_linux::{kill_pid, open_path_no_symlink, fstat_fd, close_fd}` |
| printing/format helpers | `monitor_next::output::{out, err, log_monitor_event, log_violation, log_kill, decode_utf8_cstr, dump_prefix_hex}` |

## Trace (`providers::trace`)

| Old symbol/path | New implementation |
| --- | --- |
| `TraceClient::from_path` | facade in `trace.rs` -> `trace_next::from_path_impl` |
| open reader | `trace_next::io::open_reader` |
| parse JSON line + diagnostics | `trace_next::parse::parse_trace_line_json` |
| legacy line parsing | `trace_next::parse::parse_legacy_record` |
| record insert + duplicate guards | `trace_next::parse::insert_trace_record` |
| EOF flush for active episodes | `trace_next::parse::flush_active_episodes` |
| typed event handling | `trace_next::v2::{handle_typed_event, merge_tool_calls_into_meta}` |
| fingerprint | `trace_next::normalize::compute_trace_fingerprint` |
| trace-specific error constructors | `trace_next::errors::{open_trace_file_error, invalid_trace_format, duplicate_request_id, duplicate_prompt}` |

## Drift-sensitive note

- Commit `63d96d74` restored trace diagnostic wording parity after the mechanical move.
  This is intentional and should remain stable for Step1 freeze assertions.
- Diagnostics wording parity: restored the pre-split parse-error prefix/formatting in trace
  so assertions keep matching with no semantic behavior drift.
