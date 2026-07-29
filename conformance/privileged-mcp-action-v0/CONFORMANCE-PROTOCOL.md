# Privileged MCP Action v0 conformance protocol

This protocol lets an independently authored verifier receive opaque cases before their canonical
expectations are disclosed. It standardizes invocation and reporting only. It does not supply
verifier logic, interpret the profile, or establish that an implementation is independent.

## Authorship order

1. Obtain the clean-room pack.
2. Read `spec.md` and `descriptor.json`.
3. Implement a command that consumes one bundle path and emits the profile report.
4. Freeze the implementation commit and record all materials consulted.
5. Run the scorer. It snapshots the canonical oracle into scorer-private memory for tamper resistance,
   executes every opaque case without passing that oracle to the candidate, and only then compares.
6. Publish the implementation source, run record, and completed implementation report.

Reading Assay's verifier, `gen_vectors.py`, the canonical `MANIFEST.json`, or a prior scored report
before step 4 changes the methodological classification. Disclose that access rather than describing
the run as blind.

## Candidate command

The scorer appends one opaque bundle path to the command supplied as `--entrypoint`:

```text
<entrypoint tokens> <bundle-path>
```

The command:

- MUST emit exactly one UTF-8 JSON object on stdout;
- MUST use the report shape in Section 3 of the profile;
- MUST NOT require a network connection;
- SHOULD emit at least one free-form finding for an invalid verdict;
- MAY write diagnostics to stderr;
- MAY return a non-zero process status for rejected cases, because exit status is recorded but not
  part of the normative comparison surface.

The scorer compares only `bundle_integrity`, `verdict` when integrity passes, and the full `claims`
object when the verdict is valid. Free-form findings are reviewer-visible and not scored.

The process timeout and output ceilings are operational guardrails, not a code-execution sandbox.
On POSIX the scorer kills the initial process group it creates, but candidate code can deliberately
move a child into another group. Run code you do not trust in a separate job, VM, or container and do
not place credentials in that environment.

## Reproduction modes

The implementation report self-declares one mode:

| Mode | Meaning |
|---|---|
| `blind_from_spec` | The implementation was frozen before any expected outcome, semantic vector name, generator, Assay verifier, or prior scored report was read. |
| `from_spec_then_conformance` | The implementation is independently authored, but conformance materials informed a later revision. |
| `commissioned_clean_room` | The implementation was commissioned, with payment independent of agreement or divergence, and authorship inputs were limited to the disclosed clean-room set. |
| `other_disclosed` | The preceding labels do not fit; the implementation report explains the actual sequence. |

These are provenance descriptions, not scores. The scorer records the selected label but cannot
verify it.

## Scoring

Run from an Assay source checkout at the pinned activation-kit revision:

```bash
python3 conformance/privileged-mcp-action-v0/scripts/score_candidate.py \
  --pack privileged-mcp-action-v0-clean-room.tar.gz \
  --manifest conformance/privileged-mcp-action-v0/MANIFEST.json \
  --entrypoint "./my-verifier --format json" \
  --implementation-name "my verifier" \
  --implementation-source "https://github.com/example/my-verifier" \
  --implementation-commit "<full commit>" \
  --reproduction-mode blind_from_spec \
  --output implementation-report.json
```

Exit status is `0` for complete normative agreement, `1` for a completed run with at least one
normative mismatch, and `2` for an execution or harness error. A mismatch is a useful result and
should be published rather than repaired by reading another implementation.

The scorer records the pack SHA-256 and its declared source commit, but does not verify release
provenance. Verify the release attestation before relying on that declaration and record the
verification result in the implementation report.

## GitHub Actions

Build the candidate verifier before invoking the composite action. The action appends
each opaque bundle path to `entrypoint` and writes the machine-readable report; it never imports
Assay's verifier.

```yaml
permissions:
  contents: read

steps:
  - uses: actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09 # v5.1.0

  - name: Build candidate
    run: ./build-my-verifier.sh

  - name: Download clean-room pack
    env:
      GH_TOKEN: ${{ github.token }}
    run: |
      tag=privileged-mcp-action-v0-candidate.3
      source_digest="$(gh api "repos/Rul1an/assay/commits/$tag" --jq .sha)"
      gh release download "$tag" \
        --repo Rul1an/assay \
        --pattern privileged-mcp-action-v0-clean-room.tar.gz \
        --pattern SHA256SUMS \
        --pattern attestation-bundle.json
      sha256sum -c SHA256SUMS
      gh attestation verify privileged-mcp-action-v0-clean-room.tar.gz \
        --repo Rul1an/assay \
        --bundle attestation-bundle.json \
        --signer-workflow Rul1an/assay/.github/workflows/privileged-mcp-action-pack-release.yml \
        --source-digest "$source_digest" \
        --source-ref refs/heads/main

  - name: Run conformance
    uses: Rul1an/assay/.github/actions/privileged-mcp-action-conformance@16ea2b84e472412e3e5c4d9dcabff61b7fac72f8
    with:
      pack: privileged-mcp-action-v0-clean-room.tar.gz
      entrypoint: ./target/release/my-verifier
      implementation-name: my-verifier
      implementation-source: ${{ github.server_url }}/${{ github.repository }}
      implementation-commit: ${{ github.sha }}
      reproduction-mode: blind_from_spec
      report: privileged-mcp-action-conformance.json
```

The release controller runs only from `main`, validates `candidate-release.json` against the
checked-out corpus, and creates the annotated tag after the pack, transformation check, and
attestation succeed. Pack verification resolves that tag to the attested source commit. The
composite action is pinned separately to the full commit carrying the action implementation; the
`candidate.3` release-preparation change does not alter the invoked scoring path. Keep full-commit
pinning when updating the action used by a long-lived workflow.

## Claim ceiling

A matching run demonstrates agreement on the pinned 14-case corpus. It does not establish
implementation independence, security, compliance, complete profile determinacy, or any provider
outcome. Those claims require separate evidence.
