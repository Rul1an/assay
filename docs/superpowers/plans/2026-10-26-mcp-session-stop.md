# MCP session stop implementation plan

> Execute with failing behavioral tests, one writer and an independent final-head review.

**Goal:** Let a receiver distinguish recorded termination, known decision-log loss, and missing closure evidence for one wrapped process.

**Architecture:** Keep existing proxy APIs and decision bytes. Retain a concrete file emitter alongside its trait object, expose its final counters, and return a report through an additive proxy entry point. The CLI reserves a separate opt-in output before spawn and writes one experimental CloudEvent after bounded cleanup. Reuse the existing capture wrapper and bounded receipt reader for receiver checks.

**Spec:** User-approved next-step contract, ADR-042/043, and PR #3417 sketch, amended below.

## Contract and decisions

- Existing CLI outputs and exit codes are unchanged without the new option.
- No changes to `DecisionEmitter` or fields of `ProxyConfig`.
- Reuse #3419 cleanup: try_wait before kill, bounded polling afterward. Never wait indefinitely for cleanup.
- Report observed child status separately from an attempted Assay kill; a successful kill syscall is not proof of causality.
- Written counts complete write_all calls; dropped includes the failing event and later refused events. Partial final bytes are possible. No fsync or power-loss guarantee.
- A finalized snapshot is available only after the decision-emitting thread has joined. Missing or malformed stop data is unknown, never complete.
- Require a fresh decision log when stop output accompanies an explicit decision log, so counts never describe a historical append stream.
- Companion context uses the existing event identifier. No new global agent identity or authenticity claim.
- Full A/B/C and a separate claim-check product remain out of scope.

## Steps

- [ ] Add CLI regressions for clean/nonzero/signal termination, missing/new-output behavior, alias rejection, unchanged decisions and context linkage. Observe failure against current Linux binary.
- [ ] Add writer counter/failure/concurrency tests. Expose a snapshot without changing the emitter trait.
- [ ] Add report-producing proxy entry point and use one cleanup function in report and Drop paths. Test error cleanup and honest unknown status.
- [ ] Wire --session-stop-out and register/document the experimental event. Reserve output before spawn; output failure cannot produce a successful requested capture.
- [ ] Extend the capture wrapper and bounded receiver, preserving original bytes and provenance. Test missing, stale, damaged records and known loss with positive controls.
- [ ] Run Linux focused and affected suites, formatting and clippy. Review the final committed tree independently; retain exact-source evidence.

## Review focus

Partial writes, poisoned locks, nonzero vs signal status, errors before threads start, output aliases, stale context pairing, concurrent counters, and false completeness from absent data. No live-agent capture is claimed by synthetic tests. One real capture follows only if the existing authenticated harness is available and its bounded cleanup remains verified.
