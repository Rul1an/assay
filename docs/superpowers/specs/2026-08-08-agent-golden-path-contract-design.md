# Agent Golden Path Contract Design

Date: 2026-08-08
Issue: [#2154](https://github.com/Rul1an/assay/issues/2154)

## Goal

Describe the existing #1975 install-to-verified-evidence journey for a caller
that can rely only on process stdout and the exit code. The description is
driven by the released command surfaces and kept true by binary-level tests.

## Journey

The contract uses the current executable equivalent of the eight command
families proposed in #1975:

1. verify the installed CLI with `assay version`;
2. run preflight with `assay doctor --format json`;
3. create starter files with `assay init --preset dev --hello-trace`;
4. validate the generated policy with `assay policy validate`;
5. protect one privileged call with `assay-mcp-server proxy-enforce`;
6. inspect the resulting bundle with `assay evidence show --format json`;
7. recompute the profile report offline with
   `assay evidence verify-privileged-mcp-action --format json`;
8. project enforcement decisions with
   `assay-mcp-server enforcement-sarif --output -`.

Commands in the public table use `<...>` placeholders only for paths and the
upstream server command. Tests replace those placeholders with temporary or
committed fixtures and run the real binaries built by Cargo.

## Contract Shape

`docs/guides/agent-golden-path.md` carries one row per step with these columns:

| step | command | exit code | stdout | on failure |
|---|---|---|---|---|

Each row describes both a successful invocation and one deliberate failure
where a process-level failure is reachable. `assay version` is the exception:
once that binary is running there is no install/spawn failure to observe, so
the row states that host-level spawn failure has no Assay stdout or exit code.

The table pins machine fields and schemas, not timestamps, platform strings,
absolute paths, ANSI decoration, or free-form diagnostics. JSON outputs must
parse. The proxy denial is JSON-RPC and pins `error.code`, `error.data.origin`,
and `error.data.reason` rather than treating a policy denial as a process
failure.

## Measured Gaps

The contract must record absence honestly. Measurements on base
`18abbebf025e4a9243b1ec80f798db5ccad2afed` found these separate gaps:

- [#2160](https://github.com/Rul1an/assay/issues/2160): doctor failure has no
  top-level `reason_code` or `next_step`;
- [#2161](https://github.com/Rul1an/assay/issues/2161): init failure is not
  machine-readable on stdout;
- [#2162](https://github.com/Rul1an/assay/issues/2162): policy validation has
  no machine-readable stdout report;
- [#2163](https://github.com/Rul1an/assay/issues/2163): enforcing-proxy startup
  failure has empty stdout;
- [#2164](https://github.com/Rul1an/assay/issues/2164): evidence inspection
  writes no JSON report on integrity failure;
- [#2165](https://github.com/Rul1an/assay/issues/2165): the profile verifier
  failure report omits `reason_code` and `next_step`;
- [#2166](https://github.com/Rul1an/assay/issues/2166): SARIF projection drops
  malformed non-empty NDJSON and returns a clean exit.

Tests pin these as explicit `gap` states linked from the table. That is not an
acceptance of the behavior: when a gap is fixed, the observed assertion and
the table must change together. Missing evidence is never rendered as a clean
result.

## Verification Design

Two integration tests own the commands of their package:

- `crates/assay-cli/tests/agent_golden_path_contract.rs` drives version,
  doctor, init, policy validation, bundle inspection, and offline profile
  verification through `CARGO_BIN_EXE_assay`.
- `crates/assay-mcp-server/tests/agent_golden_path_contract.rs` drives the
  enforcing proxy and SARIF projection through
  `CARGO_BIN_EXE_assay-mcp-server`.

Both tests read the one checked-in guide through a fixed repository-relative
path and require their command, schemas, stable fields, and linked gap issue to
be present. Each process assertion consumes stdout and the exit status; stderr
is included only in assertion diagnostics and is never used to decide that a
row passed.

The first red run is non-trivial: the tests fail because the contract guide is
absent while all command-driving setup compiles. After green, targeted
mutations must prove that the tests reject at least:

- a changed verifier schema in the guide;
- a changed proxy denial reason in the guide;
- removal of one linked gap issue.

## Non-Goals

- No new command, flag, output format, or production behavior.
- No fix for #2160 through #2166 in this slice.
- No `SKILL.md`; #2152 step 3 consumes this contract later.
- No full #1975 human quickstart, installer, support matrix, or release E2E.
- No claim that an allow proves delivery or an external side effect.
- No claim that profile verification certifies safety or compliance.
