//! The incremental sink a live producer drives to emit liveness records.
//!
//! [`super::verify_liveness`] can read a cadence off a finished run, but nothing could produce one:
//! Assay's bundle path maps an already-finished profile in one batch, and heartbeats written at
//! archive time from timestamps that already exist prove nothing. A heartbeat is only worth
//! anything if a live process wrote it while it was alive, so the emission has to happen where the
//! producer is, one record at a time. That is what this type is for.
//!
//! # Fail-closed, deliberately unlike its neighbours
//!
//! The two existing incremental emitters in this workspace drop write errors on the floor
//! (`AuditLog::log` ends in `.ok()`, `FileLifecycleEmitter::emit` in `let _ = writeln!`). For an
//! audit convenience that is a defensible trade. Here it would be self-defeating: a heartbeat whose
//! write silently failed is indistinguishable, in the artifact, from a producer that went quiet, and
//! detecting exactly that difference is the only reason these records exist. An evidence sink that
//! fails quietly turns into the ambiguity it was installed to remove. So every method that writes
//! returns [`Result`] and the caller has to decide, and [`LivenessWriter::close`] flushes rather
//! than trusting drop order.
//!
//! # Time
//!
//! Cadence is judged on wall-clock timestamps because those are what the records carry and what a
//! verifier can re-check. That inherits wall-clock's flaws: a clock stepped backwards mid-run can
//! make a real gap look small. The declared tolerance absorbs jitter, not tampering, and this is one
//! more reason the rung above this one exists.

use super::{
    close_payload, open_payload, LivenessDeclaration, TYPE_RUN_CLOSE, TYPE_RUN_HEARTBEAT,
    TYPE_RUN_OPEN,
};
use crate::crypto::id::compute_content_hash;
use crate::types::EvidenceEvent;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::io::Write;

/// Emits an open record, keeps the declared cadence, and closes with a commitment.
///
/// There is deliberately no `Drop` behaviour. A run that ends without [`LivenessWriter::close`] is
/// `Open`, and that is the honest reading of a producer that died: synthesising a close on drop
/// would fabricate the very statement the close record exists to make, and would make a crash
/// indistinguishable from a clean shutdown.
///
/// Records are handed to the sink as they happen and are also retained, because the close record
/// has to commit to a chain head over every record before it and that value is not knowable until
/// the run ends. Retention is therefore inherent to the commitment, not an implementation shortcut:
/// a producer that cannot hold its own run cannot close it honestly.
pub struct LivenessWriter<W: Write> {
    sink: W,
    run_id: String,
    source: String,
    declaration: LivenessDeclaration,
    emitted: Vec<EvidenceEvent>,
    last_at: DateTime<Utc>,
}

impl<W: Write> LivenessWriter<W> {
    /// Open a run: writes the run-open record carrying the declared cadence.
    pub fn open(
        sink: W,
        run_id: impl Into<String>,
        source: impl Into<String>,
        declaration: LivenessDeclaration,
        now: DateTime<Utc>,
    ) -> Result<Self> {
        let mut writer = Self {
            sink,
            run_id: run_id.into(),
            source: source.into(),
            declaration: declaration.clone(),
            emitted: Vec::new(),
            last_at: now,
        };
        let open = writer.build(TYPE_RUN_OPEN, open_payload(&declaration), now);
        writer.emit(open)?;
        Ok(writer)
    }

    /// The events written so far, in sequence order.
    pub fn emitted(&self) -> &[EvidenceEvent] {
        &self.emitted
    }

    /// Record a real event, first covering any silence that has built up since the last record.
    ///
    /// The backfill matters: a producer that was busy for longer than `H` and then emitted normally
    /// would otherwise leave a gap indistinguishable from having been dead. Emitting the heartbeat
    /// first keeps the promise the run declared, at the cost of an extra record.
    pub fn record(&mut self, type_: impl Into<String>, payload: serde_json::Value) -> Result<()> {
        let now = Utc::now();
        self.beat_until(now)?;
        let event = self.build(type_, payload, now);
        self.emit(event)
    }

    /// Drive the cadence without having anything to say. A long-running producer calls this on a
    /// timer; it writes at most the heartbeats the elapsed time requires, and nothing if none are
    /// due, so calling it too often is harmless.
    pub fn tick(&mut self, now: DateTime<Utc>) -> Result<()> {
        self.beat_until(now)
    }

    /// Whether a heartbeat is due, so a caller can schedule instead of poll.
    pub fn heartbeat_due_at(&self) -> DateTime<Utc> {
        self.last_at + chrono::Duration::milliseconds(self.declaration.interval_ms as i64)
    }

    fn beat_until(&mut self, now: DateTime<Utc>) -> Result<()> {
        // Step by the declared interval rather than jumping straight to `now`, so a long silence
        // produces the beats it should have produced instead of one record papering over it.
        let interval = chrono::Duration::milliseconds(self.declaration.interval_ms as i64);
        if interval <= chrono::Duration::zero() {
            return Ok(());
        }
        while now - self.last_at > interval {
            let at = self.last_at + interval;
            let beat = self.build(TYPE_RUN_HEARTBEAT, serde_json::json!({}), at);
            self.emit(beat)?;
        }
        Ok(())
    }

    /// Close the run: writes the close record committing to the record count and chain head.
    ///
    /// Flushes explicitly. Leaving the last write to drop would make the difference between a
    /// closed run and a truncated one depend on whether a buffer happened to be flushed, which is
    /// the ambiguity this whole module removes.
    pub fn close(mut self, now: DateTime<Utc>) -> Result<W> {
        self.beat_until(now)?;
        let payload = close_payload(&self.emitted);
        let close = self.build(TYPE_RUN_CLOSE, payload, now);
        self.emit(close)?;
        self.sink.flush().context("flush liveness sink")?;
        Ok(self.sink)
    }

    fn build(
        &self,
        type_: impl Into<String>,
        payload: serde_json::Value,
        at: DateTime<Utc>,
    ) -> EvidenceEvent {
        let seq = self.emitted.len() as u64;
        let mut event = EvidenceEvent::new(type_, &self.source, &self.run_id, seq, payload);
        event.time = at;
        event
    }

    fn emit(&mut self, mut event: EvidenceEvent) -> Result<()> {
        event.content_hash =
            Some(compute_content_hash(&event).context("content hash for liveness record")?);
        let line = crate::crypto::jcs::to_vec(&event).context("canonicalize liveness record")?;
        // One write, then a newline, both checked. A partially written record would corrupt the
        // stream for every later reader, so the error has to reach the caller rather than be logged.
        self.sink
            .write_all(&line)
            .context("write liveness record")?;
        self.sink.write_all(b"\n").context("write record newline")?;
        self.last_at = event.time;
        self.emitted.push(event);
        Ok(())
    }
}

#[cfg(test)]
mod writer_tests;
