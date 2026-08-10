# Windows Flake-Rate Proof Contract

> Instrument, not a gate. It measures; it never decides.

`.github/workflows/windows-flake-rate-proof.yml` runs one `cargo test` selection
repeatedly on `windows-latest` and uploads a JSON artifact that records **every
iteration**. It exists because a developer without a Windows host cannot answer a
question about a *rate*, and because a single green run cannot separate a fixed
test from one that won the race that time.

It mirrors the delegated proof lane's shape — a manually started producer of a
measurement the author cannot take locally, a manifest binding the numbers to an
exact commit, digests over the subjects, and a claim ceiling stated in the
artifact — while making none of that lane's claims.

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

## Correlation is not causation

Two arms at one commit produce two *observed* rates. That is evidence about a
hypothesis, and it is not a causal measurement of load:

- the samples are small, so the intervals around each rate overlap widely;
- the arms run on two separate hosted machines, so machine and image scheduling
  differences are confounded with the load difference;
- the arms are not interleaved or randomised, so drift over the run is not
  separated from the arm.

Report both rates with their provenance and describe load as a hypothesis. To
claim causation the design has to change: both arms on one runner, interleaved in
randomised order, with a pre-registered iteration count and effect size, and at
least one further manipulation of the suspected mechanism (parallelism, spawn
concurrency) that moves the rate the way the hypothesis predicts.

## Boundaries

- Not in any required context, not in the `CI` rollup, and never a substitute for
  a red required check. A proof that quietly discharges a required check is the
  defect `AGENTS.md` forbids, wearing a better name.
- A failing iteration is the datum, so the job exits 0. Three things are failures,
  because each produces a number describing something other than what it claims:

  | exit | meaning |
  |---|---|
  | `2` | the subject would not build |
  | `40` | an iteration executed no test at all |
  | `41` | a load arm lost its load, so an iteration labelled loaded was not |

  `40` and `41` copy the delegated lane's stance that an environment skip is a
  failure and not a neutral skip. A run that could not measure is not a run that
  measured zero failures, and a green load arm that had no load reads as "no
  defect" when it means "no load" — the quietest way a rate lies.
- Each iteration has an explicit ceiling (`iteration_ceiling_s`, default 300s).
  A breach is recorded as data, the process tree is killed, and the measurement
  continues, so one hung iteration cannot consume the job and take the artifact
  with it. One iteration has been observed at 462 seconds.
- It proves nothing about macOS, Linux, or any host other than the
  GitHub-hosted image version named in the artifact.

## What the artifact carries

`schema: assay.windows-flake-rate-proof/v2`:

- `manifest` — `claim_ceiling`, `non_claims` and `reuse_rule`; `head_sha`,
  `repository`, `ref`, `workflow_sha`, `workflow_path`, `run_id`, `run_attempt`,
  `run_url`; and `worktree_clean`. The binding set is the delegated manifest's,
  minus its attestation, so a number is addressable rather than asserted.
- `manifest.subject_digests` and `manifest.instrument_digests` — blob OID and
  SHA-256, as provenance for the reader.
- `runner.image_os` / `runner.image_version` — stated explicitly, because if a
  failure turns out to be an image rollout, that field is what proves it.
- `toolchain.rustc` / `toolchain.cargo`.
- `load.mode` — `none` or `workspace-tests`. The second keeps a full
  `cargo test --workspace` (the Windows CI selection) running alongside the
  measured iterations, supervised continuously and restarted as often as needed.
  Its selection is built before the first iteration (`load.prebuilt`), so the arm
  reproduces test concurrency rather than a compile storm: run 31417348260
  breached a 300s ceiling on iteration 2 and spent 243s on iteration 3 in both
  trees it compared, which was the load arm's build and not either tree.
- `iterations[]` — per iteration: exit code, duration, pass/fail/ignore counts,
  `timed_out`, `could_not_measure`, `load.alive_fraction` and `load.gap`, the
  names of failing tests, and each panic's site and first message line.
- `summary.observed_failure_rate` — failed runs over *measured* runs, beside the
  indices of any iteration that timed out, measured nothing, or lost its load.

## Reuse

A rate carries only to an **identical `head_sha`**, with identical
`instrument_digests`, `toolchain` and `runner.image_version`.

An earlier version of this document allowed carry when one test file's blob OID
matched. That was wrong: the number also depends on the test target, the package
and workspace manifests, the lockfile, and what cargo does with them, so a
file-level rule could carry a rate across changed test code or a changed
dependency. The subject closure is not something this instrument can honestly
enumerate, so reuse is pinned to the commit instead. Nothing enforces it — no
verifier consumes this artifact — so it binds whoever quotes the rate.

## Running it

Diagnostic discipline follows the delegated lane's: start as narrow as possible —
one arm, few iterations, a tight filter — and run the full form (both arms, the
full iteration count) on the definitive candidate commit.

```bash
gh workflow run windows-flake-rate-proof.yml \
  -f ref=<sha> -f iterations=20 -f load=both -f iteration_ceiling_s=300 \
  -f package=assay-mcp-server -f test_target=agent_golden_path_contract \
  -f filter=bounded_process
```

`workflow_dispatch` resolves the workflow from the default branch, so an
instrument that has not landed cannot be dispatched — and measuring an unlanded
fix is the point. Pushing to `proof/windows-flake-rate/**` runs the definition
from that branch with the defaults, and never matches `main`. By convention no
pull request is opened from that namespace, which is why a push there does not put
a check on one; that is a convention, not a guarantee.

Reading a proof back:

```bash
gh run download <run-id> --name windows-flake-rate-proof-<run-id>-none
```

## Self-test

`scripts/ci/windows_flake_rate_proof_selftest.py` pins the classifications a rate
depends on — pass, failure, timeout, executed-nothing, the rate arithmetic, load
absence, and the fail-closed verdict for each. A pre-commit hook runs it, and so
does the workflow, on the host that is about to measure.

## Reporting a rate

Quote the rate with its iteration count, its `head_sha`, its runner image version
and its toolchain, and call it observed. A rate without those is not a
measurement of anything addressable; a rate presented as a cause is not a
measurement of anything the design supports.
