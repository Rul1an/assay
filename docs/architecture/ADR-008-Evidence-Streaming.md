# ADR-008: Evidence Streaming Architecture

## Status

Proposed (January 2026)

## Context

The current evidence pipeline follows the OTel Collector pattern:

```
ProfileCollector → Profile (YAML) → EvidenceMapper → EvidenceEvent (CloudEvents)
```

This architecture is correct for **offline export** use cases:
- Batch evidence bundle creation (`assay evidence export`)
- Deterministic replay and comparison
- Compliance archives

However, there is emerging demand for **near-real-time evidence** for:
- Evidence Store ingest (policy drift alerts, live dashboards)
- OTel pipeline integration (existing observability stacks)
- SOC/SIEM workflows (security operations)

The question: should `ProfileCollector` emit `EvidenceEvent` types directly?

## Decision

**No.** We will NOT refactor `ProfileCollector` to emit `EvidenceEvent` directly.

Instead, we will introduce an **optional Streaming Mode** with:
1. Native events emitted to a channel/sink (lightweight, hot path)
2. Async mapping to `EvidenceEvent` in a separate layer (heavyweight, off hot path)

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        ProfileCollector                          │
│  (runtime capture: syscalls, fs, net, exec)                     │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
              ┌───────────────────────────────┐
              │         EventSink trait        │
              └───────────────────────────────┘
                    │                   │
         ┌─────────┴─────────┐   ┌─────┴──────────────┐
         ▼                   ▼   ▼                    ▼
┌─────────────────┐  ┌─────────────────────────────────────┐
│ AggregatingProfileSink │  │      StreamingSink               │
│ (default)              │  │  (feature-gated: `streaming`)   │
│                        │  │                                  │
│ Collects → ProfileAgg  │  │  Writes to channel/pipe         │
│ Finishes → Profile     │  │  with backpressure               │
└─────────────────┘  └─────────────────────────────────────┘
         │                              │
         ▼                              ▼
┌─────────────────┐          ┌─────────────────────────┐
│ EvidenceMapper  │          │  StreamingMapper        │
│ (offline batch) │          │  (async, bounded buffer)│
└─────────────────┘          └─────────────────────────┘
         │                              │
         ▼                              ▼
┌─────────────────┐          ┌─────────────────────────┐
│ assay evidence  │          │ assay evidence stream   │
│ export          │          │ (new command, opt-in)   │
└─────────────────┘          └─────────────────────────┘
```

### EventSink Trait (Conceptual)

```rust
pub trait EventSink: Send + Sync {
    fn record(&self, event: ProfileEvent);
    fn note(&self, message: String);
    fn finish(self) -> Result<(), Error>;
}

// Default implementation (current behavior)
pub struct AggregatingProfileSink { /* ... */ }

// Streaming implementation (feature-gated)
#[cfg(feature = "streaming")]
pub struct StreamingSink {
    tx: tokio::sync::mpsc::Sender<ProfileEvent>,
    // bounded channel for backpressure
}
```

## Non-Goals

This ADR explicitly does **NOT** include:

1. **CloudEvents construction in hot path** — The `EvidenceEvent` type requires:
   - `specversion`, `id`, `source`, `type` (mandatory CloudEvents context)
   - `trace_parent`, `trace_state` (OTel correlation)
   - `content_hash` computation (SHA-256)
   - Timestamp anchoring for determinism

   None of these belong in the syscall/fs/net capture path.

2. **Per-event timestamps/hashing in runtime** — Determinism requires anchored timestamps. Real-time emission would create non-reproducible bundles.

3. **`assay-evidence` dependency in `ProfileCollector`** — This would couple the runtime capture layer to the export contract, creating semver/compatibility maintenance burden.

4. **Refactoring existing `ProfileCollector`** — The current aggregation model is correct and will remain the default.

## Rationale

### Why NOT direct EvidenceEvent emission?

| Concern | Impact |
|---------|--------|
| **Performance** | CloudEvents construction adds ~10-50μs per event (hashing, serialization) |
| **Determinism** | Per-event timestamps break reproducible bundle generation |
| **Coupling** | Runtime depends on export contract versioning |
| **Memory** | Buffering full EvidenceEvents vs lightweight ProfileEvents |

### Why streaming mode IS valuable

| Use Case | Requirement |
|----------|-------------|
| Evidence Store | Near-real-time ingest for live dashboards |
| OTel integration | Events flow into existing observability pipelines |
| SOC workflows | Security teams need live policy violation alerts |

### OTel Collector Pattern Alignment

The OpenTelemetry Collector uses exactly this pattern:
- **Receivers**: Collect telemetry in native formats
- **Processors**: Transform, filter, enrich (async)
- **Exporters**: Convert to target format (CloudEvents, OTLP, etc.)

Our architecture mirrors this:
- **ProfileCollector**: Receiver (native `ProfileEvent`)
- **EvidenceMapper**: Processor (transformation, scrubbing)
- **BundleWriter / StreamingExporter**: Exporter (CloudEvents bundle)

## Acceptance Criteria

For the streaming mode to be considered complete:

- [ ] `EventSink` trait with `AggregatingProfileSink` (default) and `StreamingSink`
- [ ] Feature flag: `--features streaming` (not in default build)
- [ ] Backpressure handling via bounded channel (configurable buffer size)
- [ ] Memory-bounded: no unbounded growth under slow consumers
- [ ] Deterministic mapping preserved: same events produce same `content_hash`
- [ ] New CLI command: `assay evidence stream` (writes NDJSON to stdout/file)
- [ ] Integration test: streaming output can be piped to `assay evidence verify`

## Consequences

### Positive
- Clear separation: runtime capture vs export contract
- Opt-in complexity: streaming is feature-gated
- Future-proof: easy to add new sinks (Kafka, OTLP, etc.)
- Backward compatible: existing `ProfileCollector` unchanged

### Negative
- Two code paths to maintain (aggregating vs streaming)
- Streaming mode requires async runtime (`tokio`)
- Documentation complexity: when to use which mode

### Neutral
- No changes to Evidence Contract v1
- No changes to existing CLI commands

## References

- [OpenTelemetry Collector Architecture](https://opentelemetry.io/docs/collector/architecture/)
- [CloudEvents Spec v1.0](https://github.com/cloudevents/spec/blob/v1.0.2/cloudevents/spec.md)
- [ADR-006: Evidence Contract](./ADR-006-Evidence-Contract.md)
- [ADR-007: Deterministic Provenance](./ADR-007-Deterministic-Provenance.md)
