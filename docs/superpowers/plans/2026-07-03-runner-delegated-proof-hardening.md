# Runner delegated proof hardening plan

Date: 2026-07-03

## Context

The Runner delegated lane currently proves success through a manually recorded workflow URL, gate value, and commit SHA. That worked for the first phase, but the July 2026 incidents exposed three avoidable failure classes:

- concurrent jobs can share the same `assay-bpf-runner` host even when they target different refs;
- the eBPF build/provenance steps are duplicated across workflows and can drift;
- lane-check proof is bound to a commit SHA and a comment, not to the content that the delegated host actually built and exercised.

This plan keeps the first implementation narrow. It hardens the host and build/provenance surface without changing branch protection semantics or replacing the existing comment contract in one step.

## Layer 1: immediate hardening

Ship as the first PR.

1. Serialize `assay-bpf-runner` work with a host-global concurrency group on the trusted delegated workflows. This avoids `_actions` cache and workspace races across branches.
2. Add `.github/actions/canonical-ebpf-build`, a local composite action that is the canonical eBPF build entrypoint for the delegated runner lane and the monitor attach smoke.
3. Have the action produce `target/assay-ebpf.provenance.json` with object path, SHA-256, size, mtime, head SHA, workflow metadata, and key tree OIDs from the shared runner gated-path manifest for future content-addressed validation.
4. Copy the provenance file into the delegated proof-pack upload directory so the existing artifact already carries the eBPF build facts.
5. Add a `workflow_run` trigger to lane-check so a completed delegated run can refresh the PR comment and the PR-head commit status from a default-branch context.

Known Layer 1 semantics:

- The commit-status context emitted by lane-check is `lane-check/proof`, deliberately separate from the Actions job named `lane-check`. If the refresh path should satisfy branch protection without manually rerunning the pull-request check, `lane-check/proof` must be configured as the required status.
- GitHub Actions added `queue: max` for concurrency groups in May 2026, which removes the default single-pending-slot eviction behavior. The bpf-host workflows now opt into that queue so delegated proofs, smoke runs, and kernel-matrix jobs are serialized instead of evicting each other. Upstream `actionlint` does not recognize the key yet (rhysd/actionlint#657), so the pre-commit hook carries a targeted parser-lag ignore until that support lands. Layer 3 still moves the host to an ephemeral-runner model so serialization is not also a workspace-contamination boundary.

Non-goals for Layer 1:

- no GitHub artifact attestations yet;
- no tree-OID proof acceptance yet;
- no merge queue enablement;
- no Assay evidence-bundle dogfood path;
- no broad rewrite of all experimental self-hosted workflows.

## Layer 2: attested, content-addressed proof

Follow-up PR.

- Generate build provenance attestations for the proof pack and `target/assay-ebpf.o` using GitHub artifact attestations.
- Teach lane-check to verify attestations and read workflow identity, source SHA, and gate inputs from the attestation/proof pack rather than from comments.
- Keep comment parsing as a temporary fallback.
- Accept a proof across rebases when the tree OIDs for gated paths match the attested/proven path trees.

Concrete implementation plan: `docs/superpowers/plans/2026-07-03-runner-delegated-proof-layer2.md`.

## Layer 3: runner platform

Operational change, not a normal code-only PR.

- Move the bpf host to an ephemeral self-hosted runner model.
- Keep reusable compiler/cache state outside `GITHUB_WORKSPACE`; do not persist workspace or downloaded actions between jobs.
- Document the systemd registration/cleanup process and runner minimum version requirements.

## Layer 4: merge queue and dogfood

After Layer 2.

- Add `merge_group` coverage for required checks before enabling merge queue.
- Let the heavy delegated proof run once during PR review; let merge queue perform only cheap content-addressed proof validation.
- Optionally make the proof pack an Assay evidence bundle chained to the GitHub attestation.

## Verification for Layer 1

- `python3 scripts/ci/assay_runner_lane_check.py --self-test`
- `python3 scripts/ci/assay_runner_delegated_proof_pack.py --self-test`
- workflow syntax lint for the touched workflows/actions
- review the produced diff for branch-protection compatibility: required checks still run on PRs, no path-filter skip introduced.
