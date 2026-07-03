# Runner delegated proof hardening layer 2

Date: 2026-07-03

Status: producer-side attestation emission merged in #1787; consumer-side
lane-check acceptance is implemented in the follow-up slice.

## Goal

Move the delegated runner proof from "a PR comment says this run passed for this
commit" to "a signed GitHub attestation binds these proof-pack bytes and this
eBPF object to the expected workflow, repository, source, and gate inputs".

Layer 2 keeps the existing comment contract as a transition fallback, but the
primary proof path becomes attested and content-addressed:

- attested subjects: `target/assay-ebpf.o` and the delegated proof-pack content;
- replay input: the proof-pack `manifest.json` and eBPF provenance document;
- acceptance key: gated-path tree OIDs, not only the commit SHA;
- visible status: the existing `lane-check/proof` PR-head commit status.

## Non-goals

- Do not enable merge queue in this PR.
- Do not move the self-hosted runner to ephemeral mode in this PR.
- Do not make the proof pack an Assay evidence bundle yet.
- Do not remove the comment parser until at least one attestation-backed proof
  has passed on a real runner-impacting PR.
- Do not change branch-protection requirements in the same code PR.
- Do not make old proof packs durable beyond artifact retention. If the
  proof-pack artifact expires, the attested path is unavailable and the PR needs
  a fresh delegated run.

## Current Layer 1 inputs

Layer 1 already gives Layer 2 the pieces it should reuse:

- `scripts/ci/assay_runner_gated_paths.json` is the single source for gated
  prefixes and content-provenance paths.
- `.github/actions/canonical-ebpf-build` emits
  `target/assay-ebpf.provenance.json`, including every configured path-tree
  entry, and fails closed if a required tree is absent.
- `runner-spike-delegated.yml` uploads the proof pack as
  `assay-runner-delegated-proof-pack-${GITHUB_RUN_ID}`.
- `assay-runner-lane-check.yml` can refresh the PR-head
  `lane-check/proof` status from a `workflow_run` context.

## Implementation cut

### 1. Attest the proof subjects

Update `runner-spike-delegated.yml` after the proof pack is built and before
cleanup:

1. Add job permissions:
   - `contents: read`
   - `id-token: write`
   - `attestations: write`
2. Create a deterministic checksum file for the subjects:
   - `target/assay-ebpf.o`
   - `assay-runner-proof-upload/manifest.json`
   - `assay-runner-proof-upload/assay-ebpf.provenance.json`
   - every retained gate result file under `assay-runner-proof-upload/gates/`
3. Generate a provenance attestation with `actions/attest@v4` and
   `subject-checksums`.
4. Upload the generated Sigstore bundle path into the same proof-pack artifact
   so lane-check verifies the bundle carried by the pack first and uses online
   lookup only as a fallback diagnostic path.

Use `actions/attest@v4` rather than `actions/attest-build-provenance` for new
code. The build-provenance wrapper remains supported, but the upstream README
now describes it as a wrapper over `actions/attest`.

Pin the action by the v4.1.1 commit SHA in the implementation PR and register
that pin in `docs/PINNED-ACTIONS.md`; do not use a floating `@v4` reference in
the workflow.

### 2. Make the proof-pack manifest attestable

Extend `scripts/ci/assay_runner_delegated_proof_pack.py` so `manifest.json`
records:

- `proof_pack.schema = assay.runner.delegated_proof_pack.v1`;
- `proof_pack.subjects[]` with subject path, sha256, size, and role;
- `source.repository`, `source.head_sha`, `source.ref`, `source.workflow_name`,
  `source.workflow_path`, `source.run_id`, `source.run_attempt`;
- `inputs.gates` and `inputs.build_ebpf`;
- `content_provenance.path_trees`, copied from the eBPF provenance document;
- `claim_ceiling = delegated_gate_execution_only_not_runtime_safety`.

The manifest must fail closed when any configured content-provenance path is
missing, null, or carries an error. Do not silently drop absent paths.

### 3. Verify attestations in lane-check

Keep `assay_runner_lane_check.py` stdlib-only. The helper downloads the
delegated proof-pack artifact, validates the manifest, subject checksums, DSSE
payload, SLSA predicate shape, and content tree OIDs itself, then shells out to
`gh attestation verify ... --bundle ... --format json` for the cryptographic
GitHub artifact-attestation verification.

Verification should be deterministic across GitHub CLI releases:

- invoke `gh attestation verify --bundle <proof-pack-bundle> --format json`;
- keep the Python-side DSSE/SLSA payload checks load-bearing, so small changes
  in the `gh` JSON wrapper cannot silently weaken the contract;
- treat unavailable or malformed `gh` verification as a rejected attested path
  with comment-proof fallback during the transition.

Online attestation lookup is not used as the primary proof source. The uploaded
bundle inside the proof-pack artifact is the replayed evidence. If the artifact
expires or the bundle is absent, the attested path is unavailable.

The script should verify:

- repository matches `Rul1an/assay`;
- workflow identity matches `Runner Spike Delegated`;
- workflow file/path is the delegated workflow;
- source SHA matches the proof-pack manifest source SHA;
- run ID and run attempt match the proof-pack manifest;
- subject digests match the downloaded proof-pack files and eBPF object digest;
- gate input is at least as strong as the classifier-required gate;
- `build_ebpf=true` when the required proof includes eBPF build facts.

If attestation verification is unavailable or malformed, fall back to the
existing comment contract during the transition and mark the PR-head status
description as `comment-fallback`.

Implementation note: the first code slice may stop after producer-side
attestation emission plus manifest v1, then use that real delegated proof pack
to implement this consumer-side verifier in the next PR. Do not claim
attestation acceptance in lane-check until a real artifact bundle from
`Runner Spike Delegated` has been verified end-to-end.

### 4. Accept content-equivalent branch updates

Add a content-addressed acceptance rule:

1. If proof `head_sha == pr.head_sha`, accept by SHA as today.
2. Otherwise, compare every content-provenance tree OID from the proof-pack
   manifest with the same path at `pr.head_sha`.
3. Accept the proof if all required paths match and the proof gate covers the
   required gate.
4. Reject with a specific reason if any gated path tree differs, is absent, or
   is missing from the proof manifest.

This is the claim ceiling: the proof carries the tested content trees, not all
possible repository state at the later commit.

The lane-check workflow still checks out trusted base code. To inspect the
current PR-head tree OIDs, fetch only the PR head commit object and trees:
`git fetch origin <pr.head_sha> --depth=1`, then use `git rev-parse
<pr.head_sha>:<path>`. Do not check out or execute PR-head code.

### 5. Keep the status surface stable

Continue writing `lane-check/proof` for refresh runs. Do not reuse the Actions
job name `lane-check` as a commit-status context. The branch-protection note in
`docs/reference/runner/ci-lanes.md` already documents why those two names stay
separate.

## Test plan

Add `--self-test` coverage for:

- attestation JSON with matching manifest and subjects: accepted;
- same proof over a different commit with identical content-provenance trees:
  accepted;
- same proof over a different commit with one changed gated tree: rejected;
- manifest path tree entry with `error`: rejected;
- missing subject digest: rejected;
- weaker gate than required: rejected;
- malformed attestation JSON: rejected with comment fallback only if a valid
  comment proof exists;
- status payload labels `attestation` versus `comment-fallback` clearly.

Run locally:

- `python3 scripts/ci/assay_runner_lane_check.py --self-test`
- `python3 scripts/ci/assay_runner_delegated_proof_pack.py --self-test`
- workflow/action syntax lint for touched YAML

Run on GitHub before ready-for-review:

- ordinary PR checks;
- a fresh delegated `gates=all`, `build_ebpf=true` run on the final branch head;
- lane-check refresh proving the PR through the attested path;
- one forced branch update that does not touch gated trees, to confirm the
  tree-OID acceptance path avoids a second delegated run.

## Rollout

1. Ship attestation emission, manifest v1, and the pure content-tree
   comparison primitives behind the existing proof-pack upload.
2. Use the first real attested delegated proof pack as the fixture for the
   lane-check verifier slice.
3. Teach lane-check to prefer attestation proof but keep comment fallback.
4. Record both proof paths in step summary for one PR cycle.
5. After one real runner-impacting PR passes attestation proof, decide whether
   to make attestation proof required and demote comments to diagnostics only.

## References

- GitHub artifact attestation docs: permissions require `id-token: write`,
  `contents: read`, and `attestations: write` for binary provenance.
- `actions/attest@v4`: supports `subject-path`, `subject-digest`, and
  `subject-checksums`; multiple subjects can be attested together.
- Existing lane contract:
  `docs/reference/runner/ci-lanes.md`.
