//! Bounded accumulation of a remote object stream.
//!
//! ADR-043 §1 requires an ingest entrypoint to apply its ceiling to the source *before* the input
//! is materialized. `BundleStore::get_bundle` ends in `GetResult::bytes().await`, which reads the
//! whole object into memory and hands the caller a finished `Bytes`. A ceiling applied after that
//! call describes something that has already happened.
//!
//! Only the accumulation lives here. The vocabulary stays local: a store ceiling bounds a remote
//! download and says nothing about how an evidence bundle is verified afterwards.
//!
//! Scope of the bound: the accumulator never appends the chunk that would cross the ceiling and
//! stops polling at that point. Because the stream yields whole `Bytes` values, one chunk above
//! the ceiling may already have been delivered by the backend before its length can be tested, so
//! peak resident input is the ceiling plus at most one such chunk.

use std::time::Duration;

use bytes::{Bytes, BytesMut};
use futures::stream::{BoxStream, StreamExt};

use super::StoreError;

/// A chunk delivering fewer than this many bytes on average is not a transfer, it is a stall with
/// extra steps. Used only to derive a chunk allowance from the byte ceiling, never as a per-chunk
/// minimum: a legitimate final chunk is usually short.
const MIN_AVERAGE_CHUNK_BYTES: u64 = 512;

/// Floor on the chunk allowance, so a small byte ceiling still tolerates a chatty backend.
const MIN_CHUNK_ALLOWANCE: usize = 1024;

/// How long a single poll may go without the stream producing anything.
///
/// Deliberately longer than `object_store`'s own 30s request timeout, so this does not pre-empt the
/// transport's error with a less informative one. It exists because that timeout is not guaranteed
/// to be in play: `read_timeout` defaults to `None` in `object_store` 0.14.1, the options can be
/// disabled outright, and the `file://` and in-memory backends never consult them at all.
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// The ceilings applied to a download.
///
/// `max_source_bytes` is the only number a caller supplies, and it comes from the same place
/// `evidence push` gets its own. The transport bounds are derived from it rather than exposed as a
/// second knob, because they do not describe a policy an operator has an opinion about — they
/// describe the shape of a transfer that is still making progress.
///
/// A byte ceiling alone bounds how much is retained, not how long the loop runs. A backend that
/// yields empty chunks forever adds nothing to the total and is never refused by a byte count, so
/// the byte ceiling has to be paired with something that bounds the iteration itself.
/// Fields are crate-visible rather than public: `new` is the whole public surface, since the
/// transport bounds are derived and a caller has no number to supply for them. Private fields
/// already prevent outside construction and exhaustive matching; `#[non_exhaustive]` is kept on
/// top so the guarantee survives someone later widening one of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct StreamCeiling {
    /// Bytes retained from the source.
    pub(crate) max_source_bytes: u64,
    /// Chunks accepted from the stream, counting empty ones.
    pub(crate) max_chunks: usize,
    /// How long one poll may wait before the download is treated as stalled.
    pub(crate) idle_timeout: Duration,
}

impl StreamCeiling {
    /// Derive a full ceiling from the byte budget.
    pub fn new(max_source_bytes: u64) -> Self {
        let derived = max_source_bytes / MIN_AVERAGE_CHUNK_BYTES;
        let max_chunks = usize::try_from(derived)
            .unwrap_or(usize::MAX)
            .max(MIN_CHUNK_ALLOWANCE);
        Self {
            max_source_bytes,
            max_chunks,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
        }
    }

    /// Override the transport bounds so a stall and a chunk flood are expressible without
    /// transferring a real budget's worth of data. Test-only: there is no caller-facing reason to
    /// set these, and exposing them would invite an operator to tune a number that describes
    /// whether a transfer is progressing rather than a policy they hold an opinion about.
    #[cfg(test)]
    pub(crate) fn with_transport_bounds(
        mut self,
        max_chunks: usize,
        idle_timeout: Duration,
    ) -> Self {
        self.max_chunks = max_chunks;
        self.idle_timeout = idle_timeout;
        self
    }
}

/// Failure from a bounded download.
///
/// A new, narrow type rather than a `StoreError` variant. `StoreError` is public, exported from
/// the crate root, and is not `#[non_exhaustive]`, so adding a variant breaks every downstream
/// exhaustive match — and `cargo semver-checks` runs against this crate in CI. Widening an
/// existing type to carry a new outcome would also hand the ceiling refusal to every caller of
/// the trait, which is not what this slice bounds.
///
/// `#[non_exhaustive]` from the start, so this type does not recreate the trap it was introduced
/// to avoid. `StoreError` is closed and therefore cannot grow a variant without a major bump;
/// declaring that here while the enum is new costs a catch-all arm at the one external match site
/// and keeps a future dimension additive.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BoundedGetError {
    /// Anything the underlying store reports, including `NotFound`, passed through unchanged so a
    /// caller keeps the classification it already handles.
    #[error(transparent)]
    Store(#[from] StoreError),

    /// The download exceeded its configured byte ceiling.
    ///
    /// The rendered message is a constant. The number of bytes that arrived, the object key and
    /// the bundle id are all influenced or chosen by whoever produced the object, and echoing any
    /// of them writes remote-controlled text into an operator's terminal and into every log that
    /// ingests it. The configured ceiling travels as data on the variant for a caller that wants
    /// to report it; it is deliberately absent from the message.
    #[error("download refused: the source exceeded its configured byte ceiling")]
    SourceCeiling { limit: u64 },

    /// The source delivered more chunks than the transfer is allowed.
    ///
    /// This is the bound that terminates a stream of empty chunks. Such a stream adds nothing to
    /// the byte total, so no byte ceiling will ever refuse it, and because each poll resolves
    /// immediately the task never yields — which means a timeout wrapped around the loop is never
    /// polled either. Counting is the only one of the three bounds that ends it.
    #[error("download refused: the source delivered more chunks than the configured maximum")]
    ChunkCount { limit: usize },

    /// A single poll went longer than the idle timeout without producing anything.
    ///
    /// A different failure from the one above: here the stream is genuinely pending, so the task
    /// does yield and the timer does fire. Neither bound subsumes the other.
    #[error("download refused: the source stalled beyond the configured idle timeout")]
    IdleTimeout { limit: Duration },
}

impl BoundedGetError {
    /// True when the download was refused by a configured budget rather than failing in the store.
    ///
    /// One predicate covering every resource dimension, so a caller handles them in a single arm.
    /// Enumerating the variants at the call site would send any dimension added later to whatever
    /// catch-all follows, which for the CLI means an unrelated exit code for what is still a
    /// budget refusal.
    pub fn is_resource_refusal(&self) -> bool {
        matches!(
            self,
            Self::SourceCeiling { .. } | Self::ChunkCount { .. } | Self::IdleTimeout { .. }
        )
    }
}

/// Would appending `chunk_len` to `total` cross `ceiling`?
///
/// Split out so the arithmetic is testable on its own. `checked_add` is not decoration: a running
/// total near `u64::MAX` would wrap on a plain add and a wrapped total compares below any ceiling,
/// turning an overflow into an accepted download. That total is not reachable over a real network,
/// which is exactly why it would never be caught by an end-to-end test.
///
/// The boundary is inclusive: a source of exactly `ceiling` bytes is accepted, `ceiling + 1` is
/// refused.
fn would_exceed(total: u64, chunk_len: usize, ceiling: u64) -> bool {
    match total.checked_add(chunk_len as u64) {
        Some(next) => next > ceiling,
        None => true,
    }
}

/// Accumulate a chunked object stream, refusing as soon as the ceiling would be crossed.
///
/// On refusal the crossing chunk is never appended and the stream is not polled again.
///
/// This is not the same as the bytes stopping exactly at the ceiling, and the distinction is worth
/// stating rather than glossing. The stream yields whole `Bytes` values, so by the time
/// `chunk.len()` can be tested the backend has already delivered that chunk. Peak resident input
/// can therefore be one already-delivered chunk above the ceiling. What the ceiling bounds is what
/// this accumulator retains and how far it keeps reading, not the granularity at which the
/// transport hands over data.
pub(crate) async fn accumulate_bounded(
    mut stream: BoxStream<'static, object_store::Result<Bytes>>,
    ceiling: StreamCeiling,
) -> Result<Bytes, BoundedGetError> {
    let limit = ceiling.max_source_bytes;
    let mut total: u64 = 0;
    let mut chunks: usize = 0;
    // Never sized from `meta.size`. That value is supplied by the remote and using it as a
    // capacity hint lets a claimed size allocate memory before a single byte has been received,
    // which is the allocation the ceiling exists to prevent.
    let mut out = BytesMut::new();

    loop {
        // Bound the wait for each chunk, not the transfer as a whole: a long download that keeps
        // making progress is fine, a stalled one is not.
        let next = match tokio::time::timeout(ceiling.idle_timeout, stream.next()).await {
            Ok(next) => next,
            Err(_) => {
                drop(stream);
                return Err(BoundedGetError::IdleTimeout {
                    limit: ceiling.idle_timeout,
                });
            }
        };

        let Some(chunk) = next else { break };

        // Constant text. `object_store::Error`'s own rendering carries the store name and the
        // object path, and a path is a key an operator does not choose — a bucket or prefix can
        // be named by whoever writes to the store, and the message travels into a terminal and
        // into every log that ingests it. The three refusals above are value-free; a transport
        // failure on the same path leaking backend text would make that consistency decorative.
        //
        // The cost is real and worth naming: the backend's own explanation is dropped, so a
        // genuine network fault reads the same as any other. That is the same trade the ceiling
        // diagnostics already make, and the store failure is still distinguishable from a budget
        // refusal by variant rather than by text.
        let chunk = chunk.map_err(|_| {
            BoundedGetError::Store(StoreError::Io {
                message: "failed to read object stream".to_string(),
            })
        })?;

        // Counted before any length is looked at, so an empty chunk still consumes allowance.
        // Empty chunks are exactly the case a byte ceiling cannot see.
        //
        // `checked_add` for the same reason the byte total uses it: a wrapped counter compares
        // below the allowance and would hand the loop back to the stream that overflowed it. The
        // count is the bound that terminates an endless stream, so it is the last one that may
        // silently wrap.
        chunks = match chunks.checked_add(1) {
            Some(n) => n,
            None => {
                drop(stream);
                return Err(BoundedGetError::ChunkCount {
                    limit: ceiling.max_chunks,
                });
            }
        };
        if chunks > ceiling.max_chunks {
            drop(stream);
            return Err(BoundedGetError::ChunkCount {
                limit: ceiling.max_chunks,
            });
        }

        if would_exceed(total, chunk.len(), limit) {
            // Drop the stream without appending any part of this chunk, and do not poll again.
            // This chunk has already been delivered by the backend; what is controlled here is
            // that it is not retained and that no further chunk is requested. Truncating to the
            // ceiling instead would hand the caller a prefix of an archive as though it were the
            // archive.
            drop(stream);
            return Err(BoundedGetError::SourceCeiling { limit });
        }

        total += chunk.len() as u64;
        out.extend_from_slice(&chunk);
    }

    Ok(out.freeze())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    fn chunks(sizes: &[usize]) -> BoxStream<'static, object_store::Result<Bytes>> {
        let items: Vec<object_store::Result<Bytes>> = sizes
            .iter()
            .map(|n| Ok(Bytes::from(vec![b'x'; *n])))
            .collect();
        stream::iter(items).boxed()
    }

    async fn accumulate(sizes: &[usize], ceiling: u64) -> Result<Bytes, BoundedGetError> {
        accumulate_bounded(chunks(sizes), StreamCeiling::new(ceiling)).await
    }

    /// The boundary the acceptance bar names, on the accumulator itself.
    #[tokio::test]
    async fn exactly_the_ceiling_is_accepted_and_one_more_byte_is_refused() {
        assert_eq!(accumulate(&[99], 100).await.unwrap().len(), 99);
        assert_eq!(
            accumulate(&[100], 100).await.unwrap().len(),
            100,
            "a source of exactly the ceiling must be accepted"
        );
        assert!(
            accumulate(&[101], 100).await.is_err(),
            "ceiling + 1 must be refused"
        );
    }

    /// A stream that delivers a byte at a time must not be able to walk past the ceiling.
    #[tokio::test]
    async fn short_chunks_cannot_walk_past_the_ceiling() {
        let ones = vec![1usize; 101];
        assert!(accumulate(&ones, 100).await.is_err());

        let exact = vec![1usize; 100];
        assert_eq!(accumulate(&exact, 100).await.unwrap().len(), 100);
    }

    /// The realistic remote shape: several sizeable chunks that only cross the ceiling together.
    #[tokio::test]
    async fn chunked_delivery_is_measured_in_total_not_per_chunk() {
        assert_eq!(accumulate(&[40, 40, 20], 100).await.unwrap().len(), 100);
        assert!(accumulate(&[40, 40, 21], 100).await.is_err());
    }

    /// One chunk larger than the whole ceiling is refused on its own, before it is appended.
    #[tokio::test]
    async fn a_single_oversized_chunk_is_refused_whole() {
        let err = accumulate(&[10_000], 100).await.unwrap_err();
        assert!(err.is_resource_refusal(), "{err}");
    }

    /// Polling stops at the crossing chunk, rather than the whole object being drained and judged
    /// afterwards. The crossing chunk itself has already been delivered; what is asserted is that
    /// nothing after it is requested.
    ///
    /// This is the property worth a test here. "No partial bytes survive" is not: the only way out
    /// of a refusal is `Err`, so the accumulated buffer is unreachable no matter what was appended
    /// to it, and a test asserting it would pass against an implementation that truncates to the
    /// ceiling and returns the prefix. Counting what the stream was asked for cannot be satisfied
    /// that way — an implementation that drains first pulls every chunk.
    #[tokio::test]
    async fn a_refusal_stops_pulling_from_the_stream() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let pulled = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&pulled);
        // Ten chunks of 50; the ceiling is crossed on the third.
        let inner = stream::iter((0..10).map(|_| Bytes::from(vec![b'x'; 50])));
        let counted = inner
            .map(move |b| {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(b)
            })
            .boxed();

        let err = accumulate_bounded(counted, StreamCeiling::new(100))
            .await
            .expect_err("must refuse");
        assert!(err.is_resource_refusal(), "{err}");
        assert_eq!(
            pulled.load(Ordering::SeqCst),
            3,
            "the stream must stop at the chunk that crosses the ceiling, not be drained"
        );
    }

    /// The overflow arm of `checked_add`, which no end-to-end test can reach. A wrapped total
    /// compares below any ceiling, so a plain add would turn this into an accepted download.
    #[test]
    fn an_arithmetic_overflow_refuses_rather_than_wrapping() {
        assert!(
            would_exceed(u64::MAX - 1, 8, u64::MAX),
            "an addition that overflows must refuse, not wrap to a small total"
        );
        assert!(!would_exceed(0, 100, 100), "the exact boundary is accepted");
        assert!(!would_exceed(90, 10, 100));
        assert!(would_exceed(90, 11, 100));
    }

    /// The refusal names no remote-chosen value.
    #[tokio::test]
    async fn the_refusal_is_value_free() {
        let err = accumulate(&[10_000], 100).await.unwrap_err();
        let rendered = err.to_string();
        for remote_chosen in ["10000", "10_000", "9900", "bundles/", "sha256:"] {
            assert!(
                !rendered.contains(remote_chosen),
                "remote-chosen value {remote_chosen:?} reached the message: {rendered}"
            );
        }
        // The configured ceiling travels as data, not as text.
        match err {
            BoundedGetError::SourceCeiling { limit } => assert_eq!(limit, 100),
            other => panic!("expected a ceiling refusal, got {other:?}"),
        }
    }

    /// The acceptance twin: an empty object is not refused by a zero-sized edge case.
    #[tokio::test]
    async fn an_empty_stream_is_accepted() {
        assert_eq!(accumulate(&[], 100).await.unwrap().len(), 0);
        assert_eq!(accumulate(&[0], 100).await.unwrap().len(), 0);
    }
}

#[cfg(test)]
mod transport_bounds {
    use super::*;
    use futures::stream;
    use std::time::Duration;

    fn ceiling(max_chunks: usize) -> StreamCeiling {
        StreamCeiling::new(1_000_000).with_transport_bounds(max_chunks, Duration::from_secs(60))
    }

    /// The case a byte ceiling structurally cannot see.
    ///
    /// Empty chunks add nothing to the total, so `would_exceed` never fires however many arrive.
    /// Worse, each poll resolves immediately, so the task never yields and a timeout wrapped
    /// around the whole loop is never polled either — this spins at full CPU rather than hanging
    /// politely. Counting accepted chunks is the only one of the three bounds that ends it, which
    /// is why the count includes empty chunks.
    #[tokio::test]
    async fn an_endless_stream_of_empty_chunks_is_refused_by_the_chunk_count() {
        let endless = stream::repeat_with(|| Ok(Bytes::new())).boxed();
        let err = accumulate_bounded(endless, ceiling(64))
            .await
            .expect_err("an endless empty stream must be refused, not accumulated forever");
        match err {
            BoundedGetError::ChunkCount { limit } => assert_eq!(limit, 64),
            other => panic!("expected a chunk-count refusal, got {other:?}"),
        }
    }

    /// Tiny non-empty chunks stay far under the byte ceiling and still exhaust the allowance.
    #[tokio::test]
    async fn many_tiny_chunks_are_refused_before_the_byte_ceiling() {
        let tiny = stream::repeat_with(|| Ok(Bytes::from_static(b"x"))).boxed();
        let err = accumulate_bounded(tiny, ceiling(32))
            .await
            .expect_err("a chatty stream must be refused on count");
        assert!(matches!(err, BoundedGetError::ChunkCount { .. }), "{err:?}");
    }

    /// Exactly the allowance is accepted; one chunk more is refused. Same inclusive rule as bytes.
    #[tokio::test]
    async fn the_chunk_allowance_boundary_is_inclusive() {
        let ten = |n: usize| stream::iter((0..n).map(|_| Ok(Bytes::from_static(b"x")))).boxed();
        assert_eq!(
            accumulate_bounded(ten(10), ceiling(10))
                .await
                .unwrap()
                .len(),
            10,
            "exactly the allowance must be accepted"
        );
        assert!(
            accumulate_bounded(ten(11), ceiling(10)).await.is_err(),
            "allowance + 1 must be refused"
        );
    }

    /// A stream that is genuinely pending is the case the timeout does catch. The task yields, so
    /// the timer runs; `start_paused` advances the clock the moment the runtime goes idle, so this
    /// costs no wall-clock time.
    #[tokio::test(start_paused = true)]
    async fn a_stalled_stream_is_refused_by_the_idle_timeout() {
        let stalled = stream::once(async {
            futures::future::pending::<()>().await;
            Ok(Bytes::new())
        })
        .boxed();

        let ceiling =
            StreamCeiling::new(1_000_000).with_transport_bounds(1024, Duration::from_secs(5));
        let err = accumulate_bounded(stalled, ceiling)
            .await
            .expect_err("a stalled stream must be refused");
        match err {
            BoundedGetError::IdleTimeout { limit } => assert_eq!(limit, Duration::from_secs(5)),
            other => panic!("expected an idle-timeout refusal, got {other:?}"),
        }
    }

    /// A slow but progressing transfer is not a stall. Each poll lands inside the idle window even
    /// though the whole transfer takes longer than it, which is the distinction the per-poll bound
    /// exists to make.
    #[tokio::test(start_paused = true)]
    async fn a_slow_but_progressing_transfer_is_not_a_stall() {
        let slow = stream::iter(0..10)
            .then(|_| async {
                tokio::time::sleep(Duration::from_secs(3)).await;
                Ok(Bytes::from_static(b"xxxx"))
            })
            .boxed();

        let ceiling =
            StreamCeiling::new(1_000_000).with_transport_bounds(1024, Duration::from_secs(5));
        let got = accumulate_bounded(slow, ceiling)
            .await
            .expect("30s of steady progress in 3s steps is not a stall");
        assert_eq!(got.len(), 40);
    }

    /// The transport refusals are value-free too, and they are resource refusals like the byte
    /// ceiling, so a caller handles all three in one arm.
    #[tokio::test]
    async fn transport_refusals_are_value_free_resource_refusals() {
        let endless = stream::repeat_with(|| Ok(Bytes::new())).boxed();
        let err = accumulate_bounded(endless, ceiling(8)).await.unwrap_err();
        assert!(err.is_resource_refusal(), "{err}");
        let rendered = err.to_string();
        for remote_chosen in ["bundles/", "sha256:", "9", "8"] {
            assert!(
                !rendered.contains(remote_chosen),
                "value {remote_chosen:?} reached the message: {rendered}"
            );
        }
    }

    /// The derivation, so the numbers are not folklore: the allowance follows the byte budget and
    /// never drops below the floor.
    #[test]
    fn the_allowance_is_derived_from_the_byte_budget() {
        let big = StreamCeiling::new(100 * 1024 * 1024);
        assert_eq!(big.max_chunks, (100 * 1024 * 1024) / 512);
        assert_eq!(big.idle_timeout, Duration::from_secs(60));

        let small = StreamCeiling::new(100);
        assert_eq!(
            small.max_chunks, MIN_CHUNK_ALLOWANCE,
            "a small budget still tolerates a chatty backend"
        );
    }
}
