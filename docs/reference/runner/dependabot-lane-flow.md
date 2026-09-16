# Assay-Runner Dependabot Lane Flow

> Internal Phase 2A reference. This page defines how maintainers handle
> dependency PRs when the Assay-Runner lane check requires delegated Linux/eBPF
> proof.

## Scope

The `Assay-Runner Lane Check / lane-check` workflow treats runner-impacting
dependency bumps the same way it treats maintainer-authored runner changes: the
PR must record a successful manual `Runner Spike Delegated` run that matches
the PR head SHA and required gate.

Dependabot cannot perform the manual parts of that flow. It cannot dispatch the
self-hosted delegated workflow, decide whether fixture assertions need coupled
updates, or add the final proof comment. A maintainer owns those steps.

This page does not make the delegated workflow automatic. The delegated lane
remains `workflow_dispatch` only.

If GitHub's PR metadata API is temporarily unavailable, the lane-check helper
may classify changed files from the local git diff plus `GITHUB_EVENT_PATH`
payload. Comment evidence is optional on that degradation path: the PR body
remains the primary evidence carrier, and runner-impacting PRs still fail unless
matching delegated proof can be read from the body or available comments.

## Runner-Impacting Dependency Surfaces

Treat these dependency changes as runner-impacting:

- `runner-fixtures/openai-agents/package.json`
- `runner-fixtures/openai-agents/package-lock.json`
- `@openai/agents`, `zod`, and related OpenAI Agents fixture dependencies
- `aya`, `aya-ebpf`, `aya-log-ebpf`, and BPF/runtime dependency bumps
- workspace dependency bumps that can affect `assay-runner-spike`,
  `assay-monitor`, `assay-ebpf`, `assay-cli`, policy correlation, or runner
  fixtures

When in doubt, follow the [CI lane contract](ci-lanes.md) and default to the
highest applicable delegated gate.

## Maintainer Flow

1. Inspect the dependency bump and the lane-check comment.
2. If the bump requires coupled fixture or assertion updates, push those changes
   from a maintainer branch or open a replacement PR. Do not ask Dependabot to
   carry manual runner-contract edits.
3. Wait until the PR head SHA is final.
4. Dispatch `Runner Spike Delegated` manually with the gate named by the
   lane-check comment.
5. Add a maintainer comment to the PR:

   ```text
   Assay-Runner delegated proof:
   - gate: <kernel-only|kernel-policy|openai-agents-kernel-policy|all>
   - run: https://github.com/Rul1an/assay/actions/runs/<run_id>
   - sha: <current-pr-head-sha>
   ```

6. Confirm `Assay-Runner Lane Check / lane-check` passes after the comment is
   posted.
7. If Dependabot rebases or force-pushes, repeat the delegated dispatch. Proof
   for an older head SHA must not satisfy the check.

Grouped Dependabot PRs follow the highest required gate across all bumped
dependencies. If a grouped PR mixes fixture bumps with workspace or runtime
bumps, dispatch the broader gate and verify the lane-check comment names the
same gate.

## Fixture Dependency Bumps

For `@openai/agents` or fixture dependency updates:

See the [fixture dependency upgrade contract](fixtures-v0.md#dependency-upgrade-contract)
for the full fixture procedure. This section captures only the
Dependabot-specific maintainer path for delegated-proof recording.

- verify the deterministic fixture still emits the accepted SDK event sequence;
- update the SDK version assertion only when the dependency bump intentionally
  changes the accepted fixture instance;
- dispatch `gates=openai-agents-kernel-policy` unless the change also touches a
  broader runner surface that requires `gates=all`;
- keep live model calls and live credentials out of the fixture.

If the bump changes tool-call identity behavior, stop. The v0 contract requires
stable `tool_call_id`; supporting call-id-less behavior requires a separate
fallback contract, fixture, and ambiguity model.

## BPF or Runtime Dependency Bumps

For `aya`, `aya-ebpf`, `aya-log-ebpf`, or workspace dependency bumps that can
change monitor, eBPF, cgroup, or archive behavior:

- dispatch `gates=all`;
- keep `build_ebpf=true`;
- require `ringbuf_drops=0`, `kernel_layer=complete`, and
  `cgroup_correlation=clean` exactly as in the Phase 1 acceptance lane.

## Queue Maintenance Identity

`Dependabot Queue Maintenance` (`.github/workflows/dependabot-maintenance.yml`,
script `scripts/ci/dependabot-maintenance.sh`) updates `BEHIND` dependency PRs
and enables auto-merge on them. It does both as the repository's dedicated
GitHub App, `assay-dependabot-lane`, never as `GITHUB_TOKEN` (#3035):

- Events caused by `GITHUB_TOKEN` start no workflow runs, so a merge that
  `GITHUB_TOKEN` enabled ran no `push` workflows on `main`, including `CI` and
  this maintenance workflow, and the next queued PR was not refreshed.
- A pull request updated through `GITHUB_TOKEN` gets its `pull_request` runs
  created in an approval-required state, with zero jobs until a maintainer
  approves each run. The Actions approval setting cannot remove that hold; only
  a different pushing identity does.

The job runs in environment `dependabot-maintenance`, restricted to `main`,
with `deployment: false`. The environment holds variables
`DEPENDABOT_APP_CLIENT_ID` and `DEPENDABOT_APP_SLUG` and secret
`DEPENDABOT_APP_PRIVATE_KEY`. The token is minted per run with
`contents: write` and `pull-requests: write` for this repository only, and is
revoked when the job ends. The App itself holds Contents and Pull requests
read and write plus Metadata read, and no `workflows` permission. The script
refuses to act unless the token step's App slug equals
`DEPENDABOT_APP_SLUG`, and a refused branch update or auto-merge fails the run.
`scripts/ci/test-dependabot-maintenance.sh` pins both behaviours.

Not yet measured: that the first App-updated head runs its `pull_request`
workflows without approval, that an App-enabled merge runs the `push`
workflows on `main`, and that a workflow-file bump updates without the
`workflows` permission, and that the maintenance run started by such a merge
refreshes the remaining `BEHIND` PRs on its own. Until #3035 records those, keep approving held runs
and dispatching `CI` on `main` by hand when they are missing.

To rotate the key, generate a new private key on the App, replace the
environment secret, run the workflow once with `workflow_dispatch`, then delete
the old key on the App.

## Interaction With Auto-Merge

If Dependabot auto-merge is enabled, runner-impacting bumps stay blocked until
a maintainer dispatches the delegated gate and records matching proof. This is
intentional. Auto-merge must not bypass the delegated proof requirement.

## Non-Goals

- Do not add `pull_request`, `push`, or `schedule` triggers to
  `Runner Spike Delegated`.
- Do not auto-dispatch the self-hosted delegated runner from Dependabot PRs in
  Phase 2A.
- Do not let a Dependabot PR merge with a delegated run from an older head SHA.
- Do not weaken the delegated acceptance bar for dependency updates.
