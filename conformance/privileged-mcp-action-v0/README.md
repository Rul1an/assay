# privileged-mcp-action/v0 conformance corpus

Vectors for the [Privileged MCP Action Evidence Profile v0](../../docs/profiles/privileged-mcp-action/v0.md):
14 evidence bundles (5 accept, 9 reject) plus a machine-readable profile descriptor.

- `MANIFEST.json` declares each vector's expected `bundle_integrity`, profile `verdict`, and (for
  accepts) the full expected claim matrix. That triple is the **normative comparison surface**; the
  `first_failure_informative` codes are this generator's own vocabulary and informative only. An
  independent implementation SHOULD report a free-form reason per reject in its own words. Reasons
  are reviewer-visible but not scored; a different reason does not change agreement on the
  normative surface.
- `descriptor.json` is the machine-readable profile descriptor (record set, closed vocabularies,
  binding key, stages, report shape).
- `gen_vectors.py` regenerates everything deterministically (standard library only; two runs are
  byte-identical). The corpus digest in the manifest changes on any vector edit.
- The corpus digest is a **candidate** until an independent, non-author implementation reproduces
  the expected outcomes from the specification text alone.

## What a reproduction establishes: read [ERRATA.md](ERRATA.md) first

Reproducing all fourteen outcomes demonstrates agreement on **six** of the profile's rules, not on
the profile. Nineteen further rules that v0.md promises are not discriminated by any vector here, so
an implementation can omit them and still reproduce this digest. Two of those — the `origin` and
`code` legs of the Stage 3 marker triple — cannot be discriminated by any vector this generator can
produce, because `gen_vectors.py` hard-codes the single marker shape.

That does not change the exercise, the expectations, or the value of doing it. It changes the claim
that can be made afterwards, by the implementer and by us. [ERRATA.md](ERRATA.md) names all
twenty-five measured rules and is pinned to the current corpus digest.

## Attempting the independent reproduction

The corpus digest stays a **candidate** until a non-author implementation reproduces the expected
outcomes from the specification text alone. The open invitation is
[#1840](https://github.com/Rul1an/assay/issues/1840), which names the exact released snapshot
available to an external implementer. When the corpus advances,
[`candidate-release.json`](candidate-release.json) binds the next release before that issue is
repinned to verified release bytes.

### Clean-room path

The active release target is `privileged-mcp-action-v0-candidate.4`. Once its GitHub
[Releases](https://github.com/Rul1an/assay/releases) entry is published, it contains a
deterministic, attested clean-room pack with `spec.md`, `descriptor.json`, fourteen opaque
cases, and the RFC 8785 canonicalization vectors. It omits expected outcomes, semantic case names,
the vector generator, and Assay's implementation. `candidate.2` remains unchanged historical input
for the superseded 13-case digest; do not use it for a new attempt against the current corpus.

`candidate.4` re-releases the **same corpus** as `candidate.3` — identical digest, identical
fourteen cases — and adds only onboarding. A reproduction against `candidate.3` remains valid and
in scope; there is nothing to redo.

#### Start with the canonicalization vectors

The pack's `canonicalization/` directory carries the 31 RFC 8785 vectors this workspace tests its
own canonicalizer against, byte-identical, plus a note. Run them before implementing any stage.

The reason is empirical rather than tidy: the one completed cross-language reproduction in this
ecosystem failed on its first attempt because native insertion-order serialization produced a
different digest. Content ids, mandate ids and the bundle run root are all sha256 over canonical
bytes, so a canonicalizer that is wrong makes every later result uninterpretable, and you find out
fourteen bundles later.

**Passing them is not progress on the profile and agreement with them is not conformance.** They
describe byte formation under a published RFC that `descriptor.json` already requires
(`canon: jcs-rfc8785`); they say nothing about what any bundle verifies to. Every profile outcome is
still yours to derive.

```bash
tag=privileged-mcp-action-v0-candidate.4
source_digest="$(gh api "repos/Rul1an/assay/commits/$tag" --jq .sha)"
gh release download "$tag" --repo Rul1an/assay \
  --pattern privileged-mcp-action-v0-clean-room.tar.gz \
  --pattern SHA256SUMS \
  --pattern attestation-bundle.json
shasum -a 256 -c SHA256SUMS
gh attestation verify privileged-mcp-action-v0-clean-room.tar.gz \
  --repo Rul1an/assay \
  --bundle attestation-bundle.json \
  --signer-workflow Rul1an/assay/.github/workflows/privileged-mcp-action-pack-release.yml \
  --source-digest "$source_digest" \
  --source-ref refs/heads/main
```

The release controller runs only from `main`, validates
[`candidate-release.json`](candidate-release.json) against the checked-out manifest, and creates
the annotated tag after the pack, transformation check, and attestation succeed. Resolving the tag
above independently confirms that the published release still names that attested source commit.

Follow [`CONFORMANCE-PROTOCOL.md`](CONFORMANCE-PROTOCOL.md): implement before scoring, preserve the
first run, disclose the materials read, and publish a machine-readable run record plus the completed
[`IMPLEMENTATION-REPORT.template.md`](IMPLEMENTATION-REPORT.template.md). The reusable composite
action at `.github/actions/privileged-mcp-action-conformance` standardizes invocation and scoring;
it does not provide verifier logic.

The implementation may be a minimal standalone command in any language. It does not need to
integrate with Assay or an MCP runtime. A disclosed mismatch is useful evidence too: it identifies a
profile or corpus boundary that needs reconciliation.

What the claim needs is a reimplementation from the spec, so three **answer or implementation
surfaces** in this repository are deliberately out of bounds until the implementation is frozen:

| Out of bounds | Why |
|---|---|
| `gen_vectors.py` (in this directory) | It is the generator. Reading it turns a reimplementation into a port, and the resulting agreement would only show that the code was copied correctly. |
| `crates/assay-cli` — `assay evidence import privileged-mcp-action` and `assay evidence verify-privileged-mcp-action` | Same reason: our implementation of the same spec. |
| `MANIFEST.json` and prior scored reports | They carry the expected outcomes. Reading them before implementation turns conformance into an answer-guided repair pass. |

In bounds for authorship, and enough on their own: the clean-room pack's `spec.md`,
`descriptor.json`, and opaque bundle cases. The spec restates the bundle essentials on purpose so it
can be implemented without reading ours.

Without the release pack, materialize only the specification:

```bash
git clone --filter=blob:none --sparse https://github.com/Rul1an/assay.git assay-corpus
cd assay-corpus
git checkout <commit named in #1840>
git sparse-checkout set --no-cone \
  '/docs/profiles/privileged-mcp-action/v0.md'
```

Use `descriptor.json` from the release pack. The canonical descriptor beside the corpus carries a
`corpus` member and is therefore answer-bearing before implementation freeze; it is deliberately not
part of this fallback checkout.

After freezing the implementation, fetch the canonical expectations and scorer for reconciliation.
A sparse clone can fetch other blobs later, so this is a convenience for keeping the boundary rather
than a guarantee of it. The run report discloses what was actually read.

The normative comparison surface is `expected.bundle_integrity`, `expected.verdict` and
`expected.claims` per vector, plus the corpus digest. `first_failure_informative` is this
generator's own vocabulary and is **not** part of that surface: report rejects in your own words.
A reject for a different reason than ours still agrees on the normative surface — the reason is
reviewer-visible rather than scored, and exists so a human comparing the two runs can see where the
readings diverge.

Verify a vector's bundle integrity with the shipped tooling:

```bash
assay evidence lint vectors/ok-001-deny-bound-observation.bundle.tar.gz
```

The accept vectors and shape rejects verify as well-formed bundles; `bad-101-tampered-bundle` fails
bundle verification by construction. `ok-005`, `bad-105`, and `bad-108` reproduce the shipped
ASSAY-W004 contradiction and not-backed findings, so the profile's binding semantics and the
linter's are checkably the same.
