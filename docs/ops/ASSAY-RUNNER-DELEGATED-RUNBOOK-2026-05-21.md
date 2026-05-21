# Assay-Runner Delegated Runner Runbook

## Intent

Operational guide for the internal `Runner Spike Delegated` workflow:

- workflow: `.github/workflows/runner-spike-delegated.yml`
- runner labels: `[self-hosted, linux, assay-bpf-runner]`
- trigger: `workflow_dispatch` only
- proof scope: Phase 1 Linux/eBPF runner-spike gates

This runbook keeps the delegated lane explicit. It is not a general-purpose
self-hosted runner recipe and does not make macOS, Windows, live LLM, hosted
service, or production-load claims.

## What This Lane Proves

The delegated workflow runs the Linux/eBPF gates that hosted CI cannot prove:

| Gate input | Script | Proof shape |
|---|---|---|
| `kernel-only` | `scripts/ci/runner-spike-kernel-only-three-run-determinism.sh` | kernel observation, cgroup correlation, bundle verification, three-run determinism |
| `kernel-policy` | `scripts/ci/runner-spike-kernel-policy-three-run-determinism.sh` | kernel plus policy correlation, bundle verification, three-run determinism |
| `openai-agents-kernel-policy` | `scripts/ci/runner-spike-openai-agents-kernel-policy-three-run-determinism.sh` | kernel plus policy plus real `@openai/agents` SDK runtime correlation, bundle verification, three-run determinism |
| `all` | all of the above, sequentially | full Phase 1 delegated proof |

Each three-run script executes its single-run acceptance three times and then
compares the deterministic artifacts. Kernel raw event ordering is not used as
a causal-ordering claim; the bundle claim remains set/window based.

## Host Requirements

The `assay-bpf-runner` host must be dedicated to this lane. The workflow may
wipe Docker caches, GitHub Actions `_actions` caches, workspace contents, and
runner-spike temp directories.

Required baseline:

- Linux host with cgroup v2 mounted at `/sys/fs/cgroup`
- self-hosted GitHub Actions runner registered with labels:
  - `self-hosted`
  - `linux`
  - `assay-bpf-runner`
- `sudo` available for the runner user
- Rust toolchain with `cargo` and `rustc`
- Node.js 22 or newer plus `npm`
- `python3`, `git`, and `tar`
- eBPF-capable kernel and enough permissions for BPF program load/attach
- Docker optional; when present, the workflow builds the eBPF artifact through
  `cargo xtask build-image` and `cargo xtask build-ebpf --docker`

The workflow preflight checks only the minimum executable and cgroup
conditions. Kernel capability, BPF verifier behavior, cgroup layout, and
runner cleanliness are proven by the gates themselves.

## Security Model

This lane is intentionally narrower than ordinary CI:

- `workflow_dispatch` only; no `pull_request`, `push`, or `schedule` trigger
- job permissions are `contents: read`
- checkout uses `persist-credentials: false`
- the runner must be dedicated to this workflow class because cleanup is
  destructive
- no OpenAI API key or live LLM secret is required; the OpenAI Agents fixture
  uses a deterministic local model provider
- the gated command runs under `sudo`; the workflow explicitly sets `PATH`,
  `ASSAY_BIN`, and `ASSAY_EBPF_PATH` for that command, but the environment is
  not scrubbed down to only those variables

Do not add broad secret-bearing environment variables to this workflow. If a
future gate needs credentials, treat that as a new security design review, not
as a small runbook or workflow edit.

## Dispatch Procedure

1. Open GitHub Actions.
2. Select `Runner Spike Delegated`.
3. Click `Run workflow`.
4. Choose:
   - `gates=all` for Phase 1 acceptance or final regression checks
   - one narrower gate for focused diagnosis
   - `build_ebpf=true` for the documented workflow path
   - use `build_ebpf=false` only if the workflow has been explicitly updated
     to restore or copy a prebuilt `target/assay-ebpf.o` into place after the
     job cleanup and checkout steps
5. Wait for the `Phase 1 delegated gates (...)` job.
6. Record:
   - workflow run URL
   - commit SHA
   - selected `gates`
   - pass/fail result
   - relevant PASS lines or failure diagnostics

Recommended progression during diagnosis:

1. `kernel-only`
2. `kernel-policy`
3. `openai-agents-kernel-policy`
4. `all`

Use `all` once the narrower failing gate has been fixed. This avoids mixing
multiple failure signals in the same diagnostic run.

For merge-time gate selection, use the
[`Runner CI lane contract`](../reference/runner/ci-lanes.md). It classifies
which runner surfaces require delegated proof and which minimum gate applies.

For acceptance or regression evidence, use `build_ebpf=true`. The prepare and
workspace cleanup steps remove `target/`, so a previously built
`target/assay-ebpf.o` will not survive the documented workflow path.
`build_ebpf=false` is only meaningful after adding an explicit, deterministic
artifact restore or copy step.

On a fresh host, the Docker-based eBPF image/artifact build can take a few
minutes. That is expected; do not cancel the run just because the build step is
quiet for the first minute or two.

## Skip Semantics

The underlying scripts still use exit `40` for environmental skips in
cross-platform CI contexts. In this delegated lane, a skip is a failure.

Reason: this host is the environment where Linux, cgroup v2, Node 22, and the
eBPF artifact are expected to exist. If a delegated script skips, the runner or
workflow has drifted from its contract.

## Workflow Lifecycle Notes

The workflow uses two jobs:

1. `prepare-delegated-runner`
2. `phase1-delegated-gates`

The split is intentional. GitHub downloads `uses:` actions during "Set up
job", before shell steps run. Deleting `_actions` inside the same job that
uses `actions/checkout` can remove action repositories after GitHub has
prepared them. The prepare job is shell-only, so it may safely wipe stale
Actions caches before the gates job starts.

Do not merge the prepare and gates jobs unless the checkout path is redesigned.

## Cleanup Model

The workflow performs destructive cleanup on a dedicated runner:

- kills leftover `assay` processes
- clears immutable bits on known test paths
- removes workspace contents before checkout
- removes `/tmp/assay-runner-*`, `/dev/shm/assay-runner-*`, and
  `/tmp/assay-test`
- prunes Docker images and volumes when Docker is available
- removes and recreates stale GitHub Actions `_actions` caches in the prepare
  job
- restores workspace ownership to the runner user

The workspace emptiness check is a hard precondition. If it fails, inspect the
listed files before rerunning; the failure usually means a permission,
immutable-bit, mount, or stale root-owned state problem.

## Failure Triage

### Job waits for runner

Symptom:

```text
Waiting for a runner to pick up this job...
```

Check:

- runner service is online
- runner has all three labels: `self-hosted`, `linux`, `assay-bpf-runner`
- no other delegated run is occupying the runner
- workflow concurrency is not waiting behind an in-flight run on the same ref

### Checkout or action setup fails

Likely causes:

- root-owned `_actions` cache survived cleanup
- prepare job failed or did not run on the expected work directory
- runner filesystem has immutable files or stale mounts

Actions:

- inspect `prepare-delegated-runner` logs first
- ensure `_actions` was removed before the gates job
- keep the two-job lifecycle split

### Preflight fails

Use the exact error:

| Error | Likely fix |
|---|---|
| required command missing | install the named command on the runner |
| Node.js version below 22 | upgrade the runner Node install |
| missing `/sys/fs/cgroup/cgroup.controllers` | boot or configure the host with cgroup v2 |

### eBPF build fails

Likely causes:

- Docker unavailable or misconfigured
- `cargo xtask build-image` fails after Docker prune
- missing LLVM/BPF toolchain in non-Docker path

Actions:

- rerun with `build_ebpf=true`
- inspect `Build eBPF artifact`
- check whether Docker path or native path was used
- confirm `target/assay-ebpf.o` exists before the gate step

### BPF verifier rejects program load

Symptom:

```text
BPF_PROG_LOAD syscall failed
Verifier output:
```

Actions:

- capture the full verifier output from the job log
- check recent changes under `crates/assay-ebpf/`
- compare runner kernel version from the job summary
- rebuild the eBPF artifact on the delegated host
- prefer a small verifier-output-driven fix over changing acceptance criteria

### Cgroup placement fails with `Operation not supported`

Likely cause:

- runner service cgroup is `domain threaded`; child cgroups below it can be
  `domain invalid`

Expected protection:

- runner sessions resolve from the nearest cgroup v2 domain root before
  creating the measured cgroup

Actions:

- inspect `/sys/fs/cgroup/.../cgroup.type`
- confirm the domain-root resolution path is still covered by tests

### Cgroup correlation is not clean

Symptom:

```text
cgroup_correlation=partial
cgroup_correlation=failed
```

Likely causes:

- runner child placement happened outside the measured cgroup
- the delegated cgroup subtree changed under systemd
- the host is missing expected cgroup v2 controllers or delegation
- a kernel/cgroup behavior difference changed process placement semantics

Actions:

- check the runner kernel version from the job summary
- inspect `/sys/fs/cgroup/cgroup.controllers`
- inspect the delegated subtree that the runner user actually owns
- confirm the runner session is still created below a valid cgroup v2 domain
  root
- prefer rerunning the narrow failing gate after cleanup before changing
  attribution logic

### Ring-buffer drops

Symptom:

```json
"ringbuf_drops": N
```

or:

```text
kernel_layer=partial_ringbuf_drops
```

Meaning:

- kernel observation was incomplete for that run
- Phase 1 delegated gates must fail

Actions:

- inspect observation-health diagnostics and top filtered values
- check whether a recent normalizer/eBPF change increased event volume
- verify the monitor listener still keeps persistent ring-buffer consumers
- rerun the narrow gate once after a clean runner preparation

Do not change the gate to accept drops for Phase 1.

### Determinism drift

Symptom:

```text
FAIL: observation-health.json changed
FAIL: capability-surface.json changed
FAIL: correlation-report.json changed
```

Actions:

- read the printed unified diff
- compare drift fields before changing filters
- distinguish telemetry drift from attribution evidence drift
- keep the acceptance discipline strict; make failures more diagnostic rather
  than weaker

Common prior root causes:

- loader/runtime paths entering kernel evidence
- cold/warm fixture setup differences
- ring-buffer consumer replay
- SDK or policy logs written inside the measured work directory

### OpenAI Agents fixture fails

Likely causes:

- Node below 22
- `npm ci` did not install fixture dependencies
- installed `@openai/agents` metadata does not match the expected version
- SDK hook behavior changed after dependency upgrade

Actions:

- inspect `Install OpenAI Agents fixture deps`
- run `node --check tests/fixtures/runner-spike/openai-agents-js/fixture-agent.js`
- validate the installed package metadata
- for dependency bumps, update the expected version only after the delegated
  `openai-agents-kernel-policy` gate passes

For `@openai/agents` version drift, check both sources:

- `tests/fixtures/runner-spike/openai-agents-js/package.json`
- the acceptance wrapper's expected SDK version environment, when set

The fixture emits SDK metadata from the installed package. The acceptance gate
should fail if the installed package version and expected version diverge; do
not paper over that failure without rerunning the delegated
`openai-agents-kernel-policy` gate.

## Expected PASS Lines

A successful `gates=all` run should include:

```text
PASS: runner-spike kernel-only acceptance
PASS: runner-spike kernel-only three-run determinism
PASS: runner-spike kernel+policy acceptance
PASS: runner-spike kernel+policy three-run determinism
PASS: runner-spike OpenAI Agents kernel+policy acceptance
PASS: runner-spike OpenAI Agents kernel+policy three-run determinism
```

Each acceptance line appears once per run inside its three-run wrapper. The
determinism line appears once per gate.

## Result Recording

For Phase 1 or runner-impacting regression evidence, record:

- workflow run URL
- job id if relevant
- commit SHA
- selected gate input
- pass/fail status
- PASS lines or the first failing diagnostic block
- whether `build_ebpf` was enabled

If the run is used as an acceptance anchor, also record the bounded claims and
non-claims in a note under `docs/notes/`.

## Boundaries

This delegated lane does not prove:

- macOS or Windows attribution
- live LLM calls
- arbitrary SDK compatibility
- production load or sustained runner-fleet behavior
- causal ordering between individual SDK, policy, and syscall events

It proves the bounded deterministic Linux/eBPF fixture path described by the
Assay-Runner Phase 1 spike and acceptance notes.

## References

- Phase 1 acceptance:
  `docs/notes/ASSAY-RUNNER-PHASE1-ACCEPTANCE-2026-05-21.md`
- Phase 1 spike plan:
  `docs/notes/ASSAY-RUNNER-PHASE1-SPIKE-PLAN-2026-05-20.md`
- Delegated workflow:
  `.github/workflows/runner-spike-delegated.yml`
