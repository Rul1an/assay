# CICD Starter Pack

**License:** Apache-2.0 | **Version:** 1.0.0
Minimal traceability for CI pipelines — compatibility floor for adoption.

These checks verify evidence hygiene for CI pipelines. They do not constitute compliance certification.

## Rules

| Rule ID | Check | Severity | Evidence required |
|---------|-------|----------|-------------------|
| CICD-001 | Event presence | error | ≥1 event |
| CICD-002 | Lifecycle pair | warning | assay.profile.started + .finished |
| CICD-003 | Correlation ID | warning | assayrunid, traceparent, tracestate, or run_id |
| CICD-004 | Build identity | info | data.build_id or data.version |

## Quickstart (copy/paste)

```yaml
- uses: Rul1an/assay/assay-action@v2   # Pin to commit SHA in production
  with:
    pack: cicd-starter
    fail_on: error   # Use 'warning' to enforce CICD-002/003 in CI
```

(Bundles are auto-discovered; set `bundles` input to override the glob pattern.)

**Note:** Warnings (CICD-002, CICD-003) won't fail CI by default. Set `fail_on: warning` to enforce.

## CLI

```bash
# Default pack (when no --pack specified)
assay evidence lint bundle.tar.gz

# Explicit
assay evidence lint bundle.tar.gz --pack cicd-starter
```

## Next steps

- `--pack soc2-baseline` — SOC2 Common Criteria checks
- `--pack eu-ai-act-baseline` — EU AI Act Article 12 mapping

## Provenance

These checks help build toward **provenance maturity** (SLSA, attestations). Even without DSSE/in-toto checks, the pack prepares evidence hygiene for future attestation workflows.

## CICD-002: Custom lifecycle types

The default patterns `assay.profile.started` and `assay.profile.finished` match Assay's standard evidence. If you use custom lifecycle event types, create a derived pack that overrides the check:

```yaml
# custom-pack.yaml
name: my-cicd
version: "1.0.0"
kind: quality
# ... base fields ...
rules:
  - id: CICD-002
    check:
      type: event_pairs
      start_pattern: "myapp.run.started"    # Your custom type
      finish_pattern: "myapp.run.finished"
```

Then: `assay evidence lint --pack cicd-starter,./custom-pack.yaml bundle.tar.gz` (order: cicd-starter loads first; composition rules apply).

## Reference

- [ADR-023: CICD Starter Pack](../../docs/architecture/ADR-023-CICD-Starter-Pack.md)
- [SPEC-Pack-Engine-v1](../../docs/architecture/SPEC-Pack-Engine-v1.md)
