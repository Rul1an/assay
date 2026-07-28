//! Verification answers yes or no without holding the events stream.
//!
//! `BundleReader::open` verifies *and* retains `events.ndjson` in full so a caller can iterate it.
//! That is its job. The defect was that callers needing only the answer used it anyway: `assay
//! evidence verify`, `evidence diff`'s baseline check and `evidence attest` all opened a reader,
//! took nothing or only the manifest, and dropped the rest. On a 5.7 MB bundle whose events
//! decompress to roughly half a gigabyte that cost 530 MB of peak RSS against the stdin path's 33
//! MB — same bytes, same limits, sixteen times the residency, decided by whether the argument was
//! a path or a dash.
//!
//! The retention is bounded only by `max_events_bytes` (500 MiB), five times `max_bundle_bytes`.
//! A stream bound and a residency bound are different quantities, and the gap between them is the
//! whole defect — the same shape as counting lines where events were meant.
//!
//! A comment saying "verification does not retain" would go stale the first time someone routed a
//! verify-only path back through the reader, and nothing would notice. This measures it instead:
//! a counting allocator records peak live bytes across a verification, and the assertion is that
//! the peak stays far below the decompressed size. It is a property of the call, not of a call
//! site, so it holds for every caller that reaches it.

use assay_evidence::bundle::{verify_bundle_with_limits, BundleWriter, VerifyLimits};
use assay_evidence::types::EvidenceEvent;
use serde_json::json;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Tracks live bytes and the high-water mark.
///
/// Deliberately not a sampling profiler: peak RSS is a residency measure the OS may compress or
/// evict under pressure, which is why the same binary reported 198 MB and 332 MB across two debug
/// runs of the original defect. Counting allocations is exact and reproducible.
struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every method forwards to `System` unchanged and only adds relaxed atomic counters
// around it, so allocation behaviour is the system allocator's. The workspace denies unsafe code;
// this is a test-only measurement harness that never ships, and the alternative is a comment
// claiming the property instead of a test measuring it — which is the failure this file exists to
// prevent. Scoped to the impl, as `assay-core::incident` and `assay-monitor::events` scope theirs.
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // Saturating: allocations made before a measurement window are freed inside it, and a
        // wrapping counter turns that into a peak of usize::MAX rather than a small number.
        let _ = LIVE.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
            Some(v.saturating_sub(layout.size()))
        });
        System.dealloc(ptr, layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = System.realloc(ptr, layout, new_size);
        if !p.is_null() {
            if new_size >= layout.size() {
                let live = LIVE.fetch_add(new_size - layout.size(), Ordering::Relaxed) + new_size
                    - layout.size();
                PEAK.fetch_max(live, Ordering::Relaxed);
            } else {
                LIVE.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        p
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

/// A bundle that is small on disk and large decompressed, which is the shape that made the
/// retention visible. Highly compressible filler, so the archive stays a few megabytes.
fn compressible_bundle(events: u64) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut w = BundleWriter::new(&mut out);
        let filler = "a".repeat(5_000);
        for seq in 0..events {
            w.add_event(EvidenceEvent::new(
                "assay.retention.probe",
                "urn:assay:retention",
                "run_retention_0001",
                seq,
                json!({"seq": seq, "filler": filler}),
            ));
        }
        w.finish().expect("write bundle");
    }
    out
}

#[test]
fn verification_peak_stays_far_below_the_decompressed_events() {
    // 20k events x ~5 KB is about 100 MB decompressed, enough that retaining it is unmistakable
    // in the measurement while keeping the test quick.
    let bundle = compressible_bundle(20_000);
    let on_disk = bundle.len();

    // Measure the rise above whatever is already live, not from zero: the bundle itself is held
    // by this test and would otherwise count against the verifier.
    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);

    let result = verify_bundle_with_limits(bundle.as_slice(), VerifyLimits::default())
        .expect("the bundle must verify");
    assert_eq!(result.event_count, 20_000);

    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(baseline);
    let decompressed = result.manifest.files["events.ndjson"].bytes as usize;

    // The ceiling is generous on purpose: this pins "does not hold the stream", not a byte budget
    // that a harmless refactor would have to chase. Retaining the events would put the peak at or
    // above `decompressed`; streaming keeps it near the input plus a line buffer.
    let ceiling = decompressed / 4;
    assert!(
        peak < ceiling,
        "verification held {peak} bytes at peak for a bundle whose events decompress to \
         {decompressed} bytes ({on_disk} on disk). Anything at or near the decompressed size means \
         the events stream is being retained — check whether a caller was routed through \
         `BundleReader::open`, which retains by design, where `verify_bundle` would do."
    );
}

/// The reader is allowed to retain — that is what it is for — so the property above is about
/// verification, not about memory in general. Pinning both sides keeps the first assertion from
/// being read as "nothing may allocate", which would be false and would invite someone to weaken
/// the reader instead of routing around it.
#[test]
fn the_reader_does_retain_and_that_is_its_contract() {
    let bundle = compressible_bundle(20_000);

    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);

    let reader = assay_evidence::bundle::BundleReader::open(std::io::Cursor::new(bundle))
        .expect("the bundle must open");
    let retained = reader.events_raw().len();
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(baseline);

    assert!(
        peak >= retained,
        "the reader is documented to load events into memory; if this no longer holds, the \
         retention ceiling question in the audit has been answered by streaming and the first \
         test's rationale needs rewriting rather than deleting"
    );
}
