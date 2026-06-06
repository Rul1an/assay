# ADR-037: Runner Standalone Boundary

## Status
Accepted (June 2026) — records existing discipline; pointer ADR.

Depends on ADR-034 (contract seam).

## Context

The runner crates (`assay-runner-schema`, `assay-runner-core`, `assay-runner-linux`)
publish to crates.io and are standalone-useful. The standalone-versus-extraction
question is already governed by `docs/reference/runner/extraction-roadmap.md`, which
sequences the extraction slices, gates the repository split, defines kill criteria,
and states explicitly that standalone usefulness is not the same as repository
extraction.

## Decision

This ADR does not re-decide the extraction question. It records the invariant the
rest of the interop program must preserve: every interop slice consumes the Runner
through its published schema (ADR-034 contract seam) and keeps it standalone-useful,
so the options in the extraction roadmap stay open. Any move to make the Runner its
own product follows that roadmap's gates and kill criteria, not this ADR.

## Consequences

- Interop work cannot accidentally re-couple the Runner into core.
- The extraction decision stays in the document that owns it.

## Non-claims

- This ADR makes no product, packaging, or repository-split claim. It only protects
  the option by fixing the consume-via-contract invariant.

## References

- `docs/reference/runner/extraction-roadmap.md`
- ADR-034 (contract seam)
