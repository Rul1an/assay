# ADR-038: OTLP Exporter for Assay Observations

## Status
Proposed (June 2026) — decision recorded; code lands as a tracked slice.

Depends on ADR-034 (contract seam).

## Context

`assay-core` already ingests OTel-shaped traces and models the GenAI semantic
conventions (`assay-core/src/otel/`, `trace/otel_ingest.rs`, `config/otel.rs`), but
emits nothing over OTLP. Assay observations therefore cannot be viewed next to the
self-reported spans in a user's existing telemetry backend. As of mid-2026 the OTel
GenAI semantic conventions are still in Development status with no published
stabilisation timeline; agent spans (create/invoke agent, execute tool) and MCP
tool-execution instrumentation exist but will churn.

## Decision

Add an opt-in, feature-gated OTLP/HTTP exporter that maps Assay observations and the
claim-class outcome onto the OTel GenAI agent-spans and execute-tool conventions. Off
by default; no OTLP dependency in the default build. Pin the semconv version Assay
maps to and gate behind `OTEL_SEMCONV_STABILITY_OPT_IN=gen_ai_latest_experimental`.
Emit the claim-class outcome (supported / degraded / blocked / not_evaluable) as span
attributes on the execute-tool span so the declared (server-returned, SEP-2448) and
the observed views sit in one trace.

## Implementation slice

Code lands in `assay-core/src/otel/` together with this ADR, sequenced after the
in-flight evidence-crate refactor waves to avoid churn. This ADR records the
decision; it is not satisfied until the exporter ships.

## Consequences

- Assay observations land in any OTLP collector (Phoenix, Langfuse, vendor), making
  claimed-versus-actual a single trace view.
- Adds a feature-gated dependency and a semconv-mapping surface to re-pin as the
  conventions move toward stable.

## Best-practice basis (2026)

- OTel GenAI semconv (still Development): target agent-spans + execute-tool, pin the
  version, use the stability opt-in flag, expect churn.
- Additive, opt-in instrumentation over an OTLP-compatible backend.

## Non-claims

- Assay does not stabilise OTel semconv; it pins to an experimental version and the
  mapping is expected to change until the conventions are stable.

## References

- `assay-core/src/otel/semconv.rs`, `trace/otel_ingest.rs`
- ADR-034 (contract seam)
