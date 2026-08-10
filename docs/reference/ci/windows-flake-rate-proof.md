# Windows Flake-Rate Proof Contract

> Instrument, not a gate. It measures; it never decides.

`.github/workflows/windows-flake-rate-proof.yml` runs one `cargo test` selection
repeatedly on `windows-latest` and uploads a JSON artifact that records **every
iteration**. It exists because a developer without a Windows host cannot answer a
question about a *rate*, and because a single green run cannot separate a fixed
test from one that won the race that time.

It mirrors the delegated proof lane's shape — a manually started producer of a
measurement the author cannot take locally, a manifest binding the numbers to an
exact commit, digests over the subjects, content-addressed reuse, and a claim
ceiling stated in the artifact — while making none of that lane's claims.

## Claim ceiling

The artifact states its own ceiling:

```text
hosted_windows_flake_rate_measurement_only_not_a_verdict
```

The delegated lane runs on a dedicated privileged host because eBPF load, cgroup
placement and kernel-to-policy correspondence are unprovable anywhere else, and
its pack is attested over OIDC. **This instrument has no such story and must not
borrow its language.** It runs on the GitHub-hosted `windows-latest` image: there
is no dedicated machine, no privileged-host claim, and no attestation. Its
`non_claims` list says so in the artifact rather than leaving it to the reader.

## Boundaries

- Not in any required context, not in the `CI` rollup, and never a substitute for
  a red required check. A proof that quietly discharges a required check is the
  defect `AGENTS.md` forbids, wearing a better name.
- A failing iteration is the datum, so the job exits 0. Two things are failures.
  A subject that will not build exits 2. **An iteration that executed no test
  exits 40** — a filter matching nothing, or a binary that never ran. This copies
  the delegated lane's stance that an environment skip is a failure and not a
  neutral outcome: a run that could not measure is not a run that measured zero
  failures, which is the most likely way a failure-rate instrument lies quietly.
- It proves nothing about macOS, Linux, or any host other than the
  GitHub-hosted image version named in the artifact.

## What the artifact carries

`schema: assay.windows-flake-rate-proof/v1`:

- `manifest` — `claim_ceiling` and `non_claims`; `head_sha`, `repository`, `ref`,
  `workflow_sha`, `workflow_path`, `run_id`, `run_attempt`, `run_url`; and
  `worktree_clean`. The binding set is the delegated manifest's, minus its
  attestation, so a number is addressable rather than asserted.
- `manifest.subject_digests` — blob OID and SHA-256 for each file the rate is a
  measurement *of*, and `manifest.instrument_digests` for the files that define
  the measurement itself. A rate taken with a different instrument is a different
  rate.
- `runner.image_os` / `runner.image_version` — stated explicitly, because if a
  failure turns out to be an image rollout, that field is what proves it.
- `toolchain.rustc` / `toolchain.cargo`.
- `load.mode` — `none` or `workspace-tests`. The second keeps a full
  `cargo test --workspace` (the Windows CI selection) running alongside the
  measured iterations, so the load hypothesis is testable by comparing two arms
  at one SHA on one image instead of inferred from which branches were red.
- `iterations[]` — per iteration: exit code, duration, pass/fail/ignore counts,
  `could_not_measure`, the names of failing tests, and each panic's site and
  first message line.
- `summary.failure_rate` — failed runs over *measured* runs, with the indices of
  any iteration that measured nothing listed beside it.

## Content-addressed reuse

A rate carries to a later head only when `manifest.subject_digests` blob OIDs,
`manifest.instrument_digests`, `toolchain` and `runner.image_version` are
identical; otherwise the number describes different content and a fresh run is
required. Unlike the delegated lane, nothing enforces this — no verifier consumes
this artifact — so it is a reading rule for whoever quotes the rate.

## Running it

Diagnostic discipline follows the delegated lane's: start as narrow as possible —
one arm, few iterations, a tight filter — and run the full form (both arms, the
full iteration count) on the definitive candidate commit.

```bash
gh workflow run windows-flake-rate-proof.yml \
  -f ref=<sha> -f iterations=20 -f load=both \
  -f package=assay-mcp-server -f test_target=agent_golden_path_contract \
  -f filter=bounded_process
```

`workflow_dispatch` resolves the workflow from the default branch, so an
instrument that has not landed cannot be dispatched — and measuring an unlanded
fix is the point. Pushing to `proof/windows-flake-rate/**` runs the definition
from that branch with the defaults. That namespace carries no pull request, so it
can never attach a check to one, and it never matches `main`.

Reading a proof back:

```bash
gh run download <run-id> --name windows-flake-rate-proof-<run-id>-none
```

## Reporting a rate

Quote the rate with its iteration count, its `head_sha`, its subject blob OIDs,
and its runner image version. A rate without those is not a measurement of
anything addressable.
