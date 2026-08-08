# Agent Golden Path Contract Design

Date: 2026-08-08
Issue: [#2154](https://github.com/Rul1an/assay/issues/2154)

## Goal

Describe the existing #1975 install-to-verified-evidence journey for a caller
that can rely only on process stdout and the exit code. The canonical
description is machine-readable, the human table is generated from the same
source, and binary-level tests keep both tied to the released command surfaces.

## Journey

The contract uses the current executable equivalent of nine command
families proposed in #1975:

1. verify the installed CLI with `assay version`;
2. run preflight with `assay doctor --format json`;
3. create starter files with `assay init --preset dev --hello-trace`;
4. validate the generated policy with `assay policy validate`;
5. distinguish a completed passing run from a completed test failure with
   `assay run --format json`;
6. protect one privileged call with `assay-mcp-server proxy-enforce`;
7. inspect the resulting bundle with `assay evidence show --format json`;
8. recompute the profile report offline with
   `assay evidence verify-privileged-mcp-action --format json`;
9. project enforcement decisions with
   `assay-mcp-server enforcement-sarif --output -`.

Commands in the public table use `<...>` placeholders only for paths and the
upstream server command. Tests replace those placeholders with temporary or
committed fixtures and run the real binaries built by Cargo.

## Contract Shape

`docs/generated/agent-golden-path.json` is the canonical contract. It identifies
itself as `assay.agent_golden_path.v1` with integer `schema_version: 1`, following
the narrow convention introduced by #2159. The repository-wide convention and
migration remain owned by #2167; this document does not decide them.

`scripts/docs/generate-agent-golden-path.py` owns the structured step and
outcome definitions. It emits the JSON document and renders the marked table in
`docs/guides/agent-golden-path.md` with these columns:

| step | command | exit code | stdout | on failure |
|---|---|---|---|---|

Each row describes both a successful invocation and one deliberate failure
where a process-level failure is reachable. `assay version` is the exception:
once that binary is running there is no install/spawn failure to observe, so
the row states that host-level spawn failure has no Assay stdout or exit code.

The contract pins machine fields and document identities, not timestamps, platform strings,
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

Tests pin these as explicit `gap_issue` states linked from the generated table. That is not an
acceptance of the behavior: when a gap is fixed, the observed assertion and
the table must change together. Missing evidence is never rendered as a clean
result.

## Verification Design

Two integration tests own the commands of their package:

- `crates/assay-cli/tests/agent_golden_path_contract.rs` drives version,
  doctor, init, policy validation, completed run results, bundle inspection,
  and offline profile verification through `CARGO_BIN_EXE_assay`.
- `crates/assay-mcp-server/tests/agent_golden_path_contract.rs` drives the
  enforcing proxy and SARIF projection through
  `CARGO_BIN_EXE_assay-mcp-server`.

Both tests read the generated JSON through a workspace root derived with
`Path::parent()`, select outcomes by stable step and outcome ids, and compare
the driven process with the corresponding exit, stdout identity, stable fields,
and linked gap issue. Each outcome carries structured `argv`; tests resolve only
declared fixture placeholders and pass that resulting vector to the Cargo-built
binary. The rendered command is derived from the primary outcome's `argv`, so
display and execution cannot drift independently. Each process assertion
consumes stdout and the exit status; stderr is included only in assertion
diagnostics and is never used to decide that a row passed.

The enforcing-proxy exchange reuses the crate's bounded `jsonrpc_conn::Conn`:
30 seconds for a response and 10 seconds to reap after stdin EOF. The upstream
inherits stderr instead of retaining the write side of a test-owned pipe, so a
grandchild cannot keep `wait_with_output` blocked after the direct child exits.

The revised red run is non-trivial: all command-driving setup compiles and both
test binaries fail specifically because
`docs/generated/agent-golden-path.json` is absent. After green, the existing
generated-docs gate runs the generator in a tracked-files-only scratch copy.
Its self-test proves that the gate rejects at least:

- a hand-edited generated dependency diagram;
- a hand-edited machine contract;
- a hand-edited rendered golden-path table.

The tests separately pin the verifier identity, proxy denial fields, every gap
owner, and exit `1` for a completed run whose stdout remains
`assay.run_report.v1` rather than becoming a diagnosis document.
Before writing either generated artifact, the generator compares every contract
`(step id, outcome name)` pair with the literal `expected_outcome` drivers in
both integration tests. Generation refuses an undriven outcome or a stale test
driver, and requires exactly one literal driver per outcome. The drift self-test
mutates in an eighteenth outcome and a duplicate driver to prove both gates
fail.

## Non-Goals

- No new command, flag, output format, or production behavior.
- No fix for #2160 through #2166 in this slice.
- No repository-wide schema identity decision; #2167 owns that convention.
- No `SKILL.md`; #2152 step 3 consumes this contract later.
- No full #1975 human quickstart, installer, support matrix, or release E2E.
- No claim that an allow proves delivery or an external side effect.
- No claim that profile verification certifies safety or compliance.
