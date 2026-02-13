# Wave 3 Step 2 checklist (mechanical monitor/trace split)

Scope lock:
- Step 2 is a mechanical split only.
- Public facades stay stable:
  - `crates/assay-cli/src/cli/commands/monitor.rs`
  - `crates/assay-core/src/providers/trace.rs`
- No behavior/perf changes intended.
- `demo/` untouched.

## Commit slicing

- Commit A: compile-safe scaffolds (`monitor_next/*`, `trace_next/*`).
- Commit B: mechanical 1:1 function moves behind stable facades.
- Commit C: review artifacts + hard-fail boundary gates.

## Target layout

```text
crates/assay-cli/src/cli/commands/monitor_next/
  mod.rs
  normalize.rs
  rules.rs
  errors.rs
  events.rs
  output.rs
  syscall_linux.rs
  tests.rs

crates/assay-core/src/providers/trace_next/
  mod.rs
  errors.rs
  io.rs
  parse.rs
  v2.rs
  normalize.rs
  tests.rs
```

## Boundary intent

`monitor_next`
- `mod.rs`: orchestration only.
- `normalize.rs`: syntactic path/cgroup normalization only.
- `rules.rs`: allow/not compile and match only.
- `events.rs`: event handling + enforcement dispatch only.
- `output.rs`: all stdout/stderr and formatting helpers.
- `syscall_linux.rs`: Linux syscalls + all `unsafe`.

`trace_next`
- `mod.rs`: load orchestration only.
- `io.rs`: file opening/reader creation only.
- `parse.rs`: JSONL parse + line-context diagnostics.
- `v2.rs`: typed event precedence/state transitions only.
- `normalize.rs`: fingerprint normalization only.
- `errors.rs`: trace-specific error constructors only.

## Contract notes

- `monitor.rs` stays a thin facade delegating to `monitor_next::run`.
- `trace.rs` stays a thin facade delegating to `trace_next::from_path_impl`.
- Step1 freeze tests remain in original files for this step.
- Diagnostic wording parity restored for trace parse errors in Commit B follow-up.

## Reviewer commands (copy/paste)

```bash
# Full Step2 review script (includes fmt/clippy/check/tests/gates/allowlist)
BASE_REF=origin/codex/wave3-step1-behavior-freeze-v2 bash scripts/ci/review-wave3-step2.sh

# Optional explicit base override for stacked PRs
BASE_REF=origin/<stacked-base-branch> bash scripts/ci/review-wave3-step2.sh
```

## Key hard gates (copy/paste)

```bash
# Monitor facade must stay thin (code-only)
awk 'BEGIN{in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} {if(!in_tests) print}' \
  crates/assay-cli/src/cli/commands/monitor.rs | \
  rg -n 'globset|nix::|libc|syscall|\bbpf\b|MonitorEvent|kill_pid|decode_utf8_cstr|dump_prefix_hex|println!\(|eprintln!\('
# expect: empty

# Unsafe only in syscall_linux.rs
rg -n 'unsafe[[:space:]]*\{|unsafe[[:space:]]+fn' \
  crates/assay-cli/src/cli/commands/monitor_next -g'*.rs' | \
  rg -v 'monitor_next/syscall_linux.rs'
# expect: empty

# Printing only in output.rs
rg -n 'println!\(|eprintln!\(' crates/assay-cli/src/cli/commands/monitor_next -g'*.rs' -g'!output.rs'
# expect: empty

# Trace facade contains no serde/io/parse internals (code-only)
awk 'BEGIN{in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} {if(!in_tests) print}' \
  crates/assay-core/src/providers/trace.rs | \
  rg -n 'serde_json::|simd_json::|BufRead|read_line|lines\(|fs::|File|OpenOptions|parse_|normalize_|v2_|EpisodeState|ParsedTraceRecord'
# expect: empty
```

## Definition of done

- Facades remain stable and thin.
- Step1 freeze tests stay green.
- Boundary gates pass with hard fail semantics.
- Scope allowlist is clean.
