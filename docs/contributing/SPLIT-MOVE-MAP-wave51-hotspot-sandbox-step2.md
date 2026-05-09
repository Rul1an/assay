# SPLIT MOVE MAP - Wave 51 Sandbox Step2

## Intent

Keep `crates/assay-cli/src/cli/commands/sandbox.rs` as the command facade and move distinct helper responsibilities into `sandbox/*` modules.

## Moves

| From | To | Notes |
| --- | --- | --- |
| `build_env_filter` | `sandbox/env.rs` | Preserves env passthrough, strict, strip exec, allowlist, and safe PATH options. |
| `create_scoped_tmp` | `sandbox/tmp.rs` | Preserves runtime-dir fallback and owner-only permissions, with unique tempdir creation and cleanup on drop. |
| profile begin/finish and evidence run id | `sandbox/profile.rs` | Preserves atomic writes, report generation, evidence profile naming, and deterministic run id hashing. |
| degradation payload helpers | `sandbox/degradation.rs` | Preserves backend-unavailable and policy-conflict evidence semantics. |
| child spawn, env application, timeout, dry-run profile handling | `sandbox/child.rs` | Preserves command execution, TMP env, Landlock pre-exec, timeout, and profile closeout behavior. |

## Data Flow

1. `sandbox.rs::run` performs backend detection, env filtering, policy load, tmp/profile start, and Landlock compatibility decisions.
2. `sandbox.rs::run` delegates child execution to `sandbox::child::run_child`.
3. `sandbox::child::run_child` owns process setup, filtered env application, scoped temp env injection, timeout handling, profile finish, and final exit-code mapping.
4. Tests remain under `sandbox.rs` and import moved helpers through the private submodules.

## Reviewer Focus

- No user-facing CLI behavior changes.
- No broadening to public module API.
- No duplicate profile event emission after the child split.
- `assay.sandbox.degraded` evidence semantics stay unchanged.
- Non-Linux behavior stays compile-safe while Linux Landlock code remains behind target cfgs.
