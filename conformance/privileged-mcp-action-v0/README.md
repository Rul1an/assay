# privileged-mcp-action/v0 conformance corpus

Vectors for the [Privileged MCP Action Evidence Profile v0](../../docs/profiles/privileged-mcp-action/v0.md):
13 evidence bundles (5 accept, 8 reject) plus a machine-readable profile descriptor.

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

## Attempting the independent reproduction

The corpus digest stays a **candidate** until a non-author implementation reproduces the expected
outcomes from the specification text alone. The open invitation is
[#1840](https://github.com/Rul1an/assay/issues/1840), which names the exact commit the current
digest describes.

What the claim needs is a reimplementation from the spec, so two **implementation surfaces** in this
repository are deliberately out of bounds for such an attempt:

| Out of bounds | Why |
|---|---|
| `gen_vectors.py` (in this directory) | It is the generator. Reading it turns a reimplementation into a port, and the resulting agreement would only show that the code was copied correctly. |
| `crates/assay-cli` — `assay evidence import privileged-mcp-action` and `assay evidence verify-privileged-mcp-action` | Same reason: our implementation of the same spec. |

In bounds, and enough on their own: this README, [`../../docs/profiles/privileged-mcp-action/v0.md`](../../docs/profiles/privileged-mcp-action/v0.md),
`descriptor.json`, `MANIFEST.json`, and the vector bundles. The spec restates the bundle essentials
on purpose so it can be implemented without reading ours.

To materialise only those inputs — no generator, no `crates/` — use a sparse checkout at the commit
the invitation pins:

```bash
git clone --filter=blob:none --sparse https://github.com/Rul1an/assay.git assay-corpus
cd assay-corpus
git checkout <commit named in #1840>
git sparse-checkout set --no-cone \
  '/docs/profiles/privileged-mcp-action/v0.md' \
  '/conformance/privileged-mcp-action-v0/**' \
  '!/conformance/privileged-mcp-action-v0/gen_vectors.py'
```

This does not materialize either out-of-bounds surface in the working tree. A sparse clone still
carries the repository index and can fetch other blobs later, so it is a convenience for keeping the
boundary rather than a guarantee of it. Nothing here is enforced and nothing needs to be — it is a
boundary an attempt keeps for its own result to mean anything, and saying which paths those are
costs nothing.

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
