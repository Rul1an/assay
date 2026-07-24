# privileged-mcp-action/v0 conformance corpus

Vectors for the [Privileged MCP Action Evidence Profile v0](../../docs/profiles/privileged-mcp-action/v0.md):
13 evidence bundles (5 accept, 8 reject) plus a machine-readable profile descriptor.

- `MANIFEST.json` declares each vector's expected `bundle_integrity`, profile `verdict`, and (for
  accepts) the full expected claim matrix. That triple is the **normative comparison surface**; the
  `first_failure_informative` codes are this generator's own vocabulary and informative only. An
  independent implementation SHOULD report a free-form reason per reject in its own words, so a
  wrong-reason reject cannot silently score as agreement.
- `descriptor.json` is the machine-readable profile descriptor (record set, closed vocabularies,
  binding key, stages, report shape).
- `gen_vectors.py` regenerates everything deterministically (standard library only; two runs are
  byte-identical). The corpus digest in the manifest changes on any vector edit.
- The corpus digest is a **candidate** until an independent, non-author implementation reproduces
  the expected outcomes from the specification text alone.

Verify a vector's bundle integrity with the shipped tooling:

```bash
assay evidence lint vectors/ok-001-deny-bound-observation.bundle.tar.gz
```

The accept vectors and shape rejects verify as well-formed bundles; `bad-101-tampered-bundle` fails
bundle verification by construction. `ok-005`, `bad-105`, and `bad-108` reproduce the shipped
ASSAY-W004 contradiction and not-backed findings, so the profile's binding semantics and the
linter's are checkably the same.
