# Privileged MCP Action v0 conformance protocol

This protocol lets an independently authored verifier receive opaque cases before their canonical
expectations are disclosed. It standardizes invocation and reporting only. It does not supply
verifier logic, interpret the profile, or establish that an implementation is independent.

## Authorship order

1. Obtain the clean-room pack.
2. Run `canonicalization/rfc8785-vectors.json` against your canonicalizer before anything else. It
   is a prerequisite check, not profile work: passing it is not progress and agreement with it is
   not conformance. It is here because a wrong canonicalizer makes every later result
   uninterpretable, and because that is what broke the one completed cross-language attempt.
3. Read `spec.md` and `descriptor.json`.
4. Implement a command that consumes one bundle path and emits the profile report.
5. Freeze the implementation commit and record all materials consulted.
6. Run the scorer. It snapshots the canonical oracle into scorer-private memory for tamper resistance,
   executes every opaque case without passing that oracle to the candidate, and only then compares.
7. Publish the implementation source, run record, and completed implementation report.

Reading Assay's verifier, `gen_vectors.py`, the canonical `MANIFEST.json`, or a prior scored report
before step 5 changes the methodological classification. Disclose that access rather than describing
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

## Authorship method

A second provenance axis, orthogonal to the mode above. The mode says what was read and when; this
says what wrote it. Both are self-declared, neither is a score, and the scorer cannot verify either.

The labels are not ours. They are the trailers open source projects already converged on, so a
contributor does not learn a private vocabulary to report here:

| Label | Meaning |
|---|---|
| `Authored-By: human` | No generative assistance in the implementation. |
| `Assisted-By: <system> <version>` | A human directed the work and used a generative system for part of it. |
| `Generated-By: <system> <version>` | The implementation was produced end to end by a generative system from the profile text. |

`Assisted-By:` and `Generated-By:` sit at two points on an autonomy spectrum; Apache originated the
generated form and OpenInfra added the assisted one and made disclosure mandatory. Where assistance
was used, the report states the system and version, which parts of the implementation it produced,
and how the output was checked. That is the shape IEEE asks for, plus the verification note that
recent policy guidance adds, and it is the part a reader actually needs.

### Why this axis exists here

Not for attribution. It changes what a matching run licenses, and the reason is measured rather than
assumed.

Ron, Baudry and Monperrus re-ran the Knight-Leveson experiment with contemporary coding agents
(*N-Version Programming with Coding Agents*, arXiv:2606.20158, June 2026): 48 agent-generated
implementations of one specification, one million randomized inputs. Agreement was not independent.
The campaign produced **429 coincident-failure cases where the independence model predicts 115.36**,
and the concentration is the part that matters for a conformance corpus:

> Failures overwhelmingly concentrate in LICs 9 and 14, indicating that the failures are driven by a
> small number of difficult or ambiguous parts of the specification.

Their diagnosis is that specification ambiguity and underspecification are the primary drivers of
shared failure. Those are exactly the regions a corpus exists to probe, so agreement between two
generatively-written implementations is weakest evidence precisely where a vector carries the most
information.

The same paper is why this is a disclosure rather than a restriction. Diversity from coding agents
still helped: majority voting over three-version units dropped mean failures from 387.44 to 130.99,
and 11,844 units showed no observed failure. Independence is a degree to be measured, not a property
to be assumed or a gate to be passed.

### How to read a result under it

Agreement is cheapest where the profile is unambiguous, so an aggregate score is the least
informative summary available. Two consequences:

- **Divergence is the informative event.** A candidate that disagrees on a vector has found either
  its own defect or an ambiguity in the profile, and both are worth more than another agreeing run.
  Report per vector; the scorer already does.
- **The interpretation decisions should be declared in advance, and are not yet.** Where the profile
  required a reading rather than a transcription, naming those points before a candidate runs would
  make a candidate's answers there the measured signal instead of an aggregate that hides them. No
  such list exists for this profile today. Stating the obligation here rather than implying the
  property: until it is written, a divergence has to be diagnosed after the fact, which is weaker
  and slower but not unusable.

### The symmetric obligation

This applies to the profile authors before it applies to anyone else. Where an Assay-side
implementation or checker was written with generative assistance, that is disclosed in the same
terms, in the same place as its materials-consulted record. A project asking third parties for a
disclosure it does not make itself is asking for deference rather than evidence.

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
      tag=privileged-mcp-action-v0-candidate.4
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
`candidate.4` release-preparation change does not alter the invoked scoring path. Keep full-commit
pinning when updating the action used by a long-lived workflow.

## Claim ceiling

A matching run demonstrates agreement on the pinned 14-case corpus. It does not establish
implementation independence, security, compliance, complete profile determinacy, or any provider
outcome. Those claims require separate evidence.

Nor does it establish independence of failure. Where either implementation was generatively written,
a match is bounded further by the result cited under Authorship method: agreement is expected to be
correlated on the profile's ambiguous regions, which is where a corpus carries its information. A
report may state agreement; it may not state that agreement was reached independently unless the
authorship methods make that a claim someone can check.

### Which badge a run earns, and the one it cannot

Reports here carry an [ACM Artifact Review and Badging](https://www.acm.org/publications/policies/artifact-review-and-badging-current)
class, because the distinction between kinds of reproduction is easy to blur and
ACM already drew it. The two terms are the reverse of the common intuition:
*Results Reproduced* means a different team obtained the result **using** the
author's artifacts; *Results Replicated* means **without** them.

A blind from-spec run against this pack earns **Results Reproduced**. The
implementation is the reproducer's own and imports nothing from Assay, but the
opaque cases, the descriptor and the specification all come from the author.

**Results Replicated is not reachable against this or any conformance corpus, and
that is a property of corpora rather than a shortfall of any reproducer.** ACM
assumes the author-supplied artifact is the author's *code*, so obtaining the
result without it is meaningful. A conformance corpus inverts that: the corpus is
the artifact, and no reproduction can avoid using it. A report claiming
*Replicated* against this pack has either misread the badge or not used the pack.

So the fact worth stating has no ACM badge, and a report should state it in full
rather than reach for a stronger label: **an independently written implementation,
run against author-supplied cases, deriving every outcome from the specification
alone.** That is a stronger and more checkable sentence than either badge, and it
is what `reproduction_mode: blind_from_spec` is claiming.
