# Host-Capability Proof Gate (RFC, v0)

Status: accepted design, not yet implemented. This document is the contract; the checker and
workflows land in a follow-up PR and must match what is written here.

## Problem

Files under `crates/assay-cli/src/diagnostics/` carry host/kernel-capability claims: Landlock ABI
probing, Landlock-net ruleset usability, `no_new_privs` settability. These claims are only meaningful
when the diagnostics have actually been produced on a real eligible host for the exact code under
review. Today that evidence is attached manually as a PR comment and checked by a human.

The runner-spike delegated lanes (`gates=all`, see [ci-lanes.md](ci-lanes.md)) are the wrong vehicle:
they prove kernel-capture and runner behavior, a different proof object. Forcing diagnostics PRs
through them would demand evidence that says nothing about the change. This gate is a separate,
narrow, required check that demands exactly the right evidence: a `assay doctor --format json` run on
a real host, bound to the PR head SHA, validated by machine.

## Decision summary

- Required status check, not advisory.
- Trigger: any non-Markdown change under `crates/assay-cli/src/diagnostics/`.
- Proof: a `workflow_dispatch` run of the host-capability-proof workflow on the self-hosted host,
  validated through the GitHub Actions API. A pasted JSON block alone is never sufficient.
- Validation: presence and JSON type of the required fields, never their values. A red host is also
  evidence; whether the values are acceptable stays a reviewer judgment.

## Trigger paths

```
trigger:
  crates/assay-cli/src/diagnostics/**        (including format.rs and #[cfg(test)] code)

never trigger:
  *.md anywhere (including under crates/assay-cli/src/diagnostics/)
  docs/**
  CHANGELOG.md
  scripts/ci/assay_host_capability_check.py      (technical exemption, see below)
  .github/workflows/host-capability-check.yml    (technical exemption)
  .github/workflows/host-capability-proof.yml    (technical exemption)
```

Rendering (`format.rs`) and tests inside `diagnostics/` trigger deliberately: rendering determines how
a claim reaches the reader, and tests determine how it is validated. The cost per trigger is one
`assay doctor` run on the host.

The checker script and the two workflows are exempt from this gate as a technical exemption, not a
trust statement: a change to the gate cannot be proven by the gate it is changing. They receive
normal CI (lint, review) like any other file, and a change to them should be reviewed with the same
suspicion as a change to the lane-check classifier.

## Proof workflow (`host-capability-proof.yml`)

```yaml
on: workflow_dispatch
runs-on: [self-hosted, assay-bpf-runner]
permissions:
  contents: read
  actions: read
```

The run builds `assay-cli` from the dispatched ref and uploads an artifact containing:

- the head SHA the run was dispatched on,
- the full `assay doctor --format json` output,
- host metadata (`uname -a`, runner label).

Starting a `workflow_dispatch` run requires write access to the repository, so who-may-produce-proof
is enforced by GitHub's own permission model, not by comment-author filtering.

Operational rule: the proof run builds and executes the dispatched ref on the self-hosted host, and
`cargo build` runs build scripts and proc macros. Do not dispatch `host-capability-proof` on
untrusted fork code. For external contributions, review the change and mirror it into a trusted
branch before producing proof on the self-hosted runner. This proof route is trusted-maintainer-
operated infrastructure, not a safe arbitrary-fork runner.

## Check workflow (`host-capability-check.yml`)

```yaml
on: pull_request
permissions:
  contents: read
  actions: read
  pull-requests: read
```

The job runs the checker and passes or fails; it posts no comment (a fork PR's `GITHUB_TOKEN` is
read-only and must stay that way). No `pull_request_target` in v0: a write-token checker is exactly
the workflow shape where mistakes become supply-chain risk, and the gate does not need one. An
optional bot comment via a carefully scoped `pull_request_target` job is a possible later addition,
never a v0 requirement.

## Proof marker

When the gate triggers, the PR body or a PR comment must carry:

```text
Host-capability proof:
- workflow-run: https://github.com/<owner>/<repo>/actions/runs/<id>
- host: assay-bpf-runner
- command: assay doctor --format json
- sha: <PR head SHA, full or 12-char prefix>
```

A JSON block after the marker is welcome as reviewer convenience, but it is never the source of
truth. The checker validates the workflow run through the Actions API:

```text
event          == workflow_dispatch
head_sha       == PR head SHA (exact match against the run's immutable metadata)
conclusion     == success
workflow name  == host-capability-proof
workflow path  == .github/workflows/host-capability-proof.yml
```

Producer identity is validated by workflow path, not only display name: names are human-facing
and can duplicate or drift, the path is the proof-chain component. The marker may still say
`host-capability-proof` for the human reader.

and reads the doctor JSON from the run's artifact. A pasted JSON block without a validating
workflow-run URL fails the gate: a trusted account posting a JSON block proves authorship of a
comment, not execution on the host. Force-pushing the PR invalidates the proof (new head SHA); the
proof workflow must be re-dispatched on the new head.

## Field validation

The checker validates presence and JSON type of these seven fields inside the `landlock` object,
never their values:

| Field | Required | Type |
|---|---|---|
| `abi_probe_status` | yes | string |
| `abi_version_source` | yes | string or null |
| `abi_version` | yes | integer or null |
| `net_connect_tcp_supported` | yes | boolean |
| `net_bind_tcp_supported` | yes | boolean |
| `net_connect_ruleset_probe` | yes | string |
| `no_new_privs_settable` | yes | boolean |
| `abi_probe_errno` | optional | integer or null when present |
| `net_connect_ruleset_errno` | optional | integer or null when present |

`net_bind_tcp_supported` is required even while the connect path is the only one planned: it is part
of the same Landlock ABI 4 capability family, produced by the same doctor run, and requiring it
proves the full Landlock-net diagnostics shape was produced on the host. Requiring presence is not a
claim that bind is used.

Value validation is deliberately absent. `"abi_probe_status": "unsupported"` passes the gate: the
gate proves the diagnostics were executed on the host for the PR head, not that the host is eligible
for any particular enforcement plan. Unknown future enum values must not hard-fail the type check.

## Failure output

The checker fails with a machine-readable reason (one of a small pinned set, e.g.
`no_proof_marker`, `run_not_found`, `head_sha_mismatch`, `run_not_dispatch`, `run_not_success`,
`artifact_missing`, `field_missing:<name>`, `field_type:<name>`), plus prose for the human. Reasons
are append-only; removing or renaming one is a contract change to this document first.

## Self-test (must ship with the checker)

```
crates/assay-cli/src/diagnostics/probes.rs              -> proof required
crates/assay-cli/src/diagnostics/landlock_net_smoke.rs  -> proof required
crates/assay-cli/src/diagnostics/format.rs              -> proof required
crates/assay-cli/src/diagnostics/README.md              -> not required (Markdown exemption)
crates/assay-cli/src/cli/commands/run.rs                -> not required
CHANGELOG.md                                            -> not required
docs/reference/runner/host-capability-proof.md          -> not required
scripts/ci/assay_host_capability_check.py               -> not required (technical exemption)
```

## What this gate is not

- Not a runner-spike lane and not a reuse of `gates=all`: that proves kernel capture, a different
  proof object.
- Not a value judgment on the capability fields: evidence gate, not quality gate.
- Not an enforcement-claim gate: enforcement claims keep their own proof paths (egress real-block
  proof, `enforcement_health`).
- Not a blanket heavy-gate for `assay-cli`.
