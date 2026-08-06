//! Non-canonical diagnostic projection for a failed delegated run (#1271).
//!
//! # What this is not
//!
//! It is not a runner mode and it is not evidence. The runner does not measure differently when a
//! projection is rendered: this reads an already-finished [`KernelLayerCapture`] and formats it.
//! `ringbuf_drops = 0` remains required for acceptance, and nothing here can move it — the
//! projection has no path to the gate and takes the capture by shared reference.
//!
//! # Why it exists
//!
//! A delegated run that fails with `ringbuf_drops > 0` reports one number. #1271: "Conflating them
//! into one ring-buffer drops number is precisely what makes future failures hard to triage." The
//! data to un-conflate it is already collected — [`KernelLayerCapture`] holds the per-ring, per-hook
//! and filtering breakdowns as private fields that nothing could read. This is the read-out.
//!
//! # The three layers, and the honesty about layer B
//!
//! A. **Kernel-side loss** — per-ring drops, per-hook attribution, and what no hook claims.
//! B. **Userspace reader pressure** — decoder rejects. Poll lag and reader backlog are *not*
//!    observable today, and the projection says so by name rather than printing zero. #1271 asks
//!    for these "where available without changing capture semantics"; making them available means
//!    touching the consumer loop, which is the one thing the acceptance criteria forbid this
//!    change from doing. An absent measurement reported as `0` is the failure this whole issue is
//!    about, one layer up.
//! C. **Normalizer / evidence filtering** — raw, filtered, retained, and the reduction ratio.
//!
//! # Output boundary
//!
//! Text, for stderr or `$GITHUB_STEP_SUMMARY`. It is not in the evidence-bundle schema, it is not
//! content-addressed, and no consumer parses it. That is deliberate: a diagnostic that acquired a
//! schema would become something a reader could depend on, and then something acceptance could
//! quietly start resting on.

use std::fmt::Write as _;

use crate::kernel::KernelLayerCapture;

/// Host facts a repeated failure has to be compared across.
///
/// Collected by the caller rather than read here, because this crate must not decide what a host
/// is: the delegated runner knows its own label and the CLI knows the ring size it configured.
/// A field nobody can fill is `None` and renders as `unknown`, never as a plausible default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostContext {
    pub kernel_version: Option<String>,
    pub cpu_count: Option<u32>,
    pub ringbuf_pages: Option<u32>,
    pub runner_label: Option<String>,
    pub attach_mode: Option<String>,
    pub cgroup_correlation: Option<String>,
}

impl HostContext {
    /// Read what this process can observe about its own host.
    ///
    /// Deliberately narrow. `uname` and the CPU count are process-visible; the ring size, runner
    /// label and cgroup status are configuration the caller holds, so they stay `None` here and are
    /// filled by whoever knows them.
    pub fn from_environment() -> Self {
        Self {
            kernel_version: std::fs::read_to_string("/proc/sys/kernel/osrelease")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            cpu_count: std::thread::available_parallelism()
                .ok()
                .map(|n| n.get() as u32),
            ..Self::default()
        }
    }
}

fn or_unknown(value: Option<&str>) -> &str {
    value.unwrap_or("unknown")
}

/// Render the diagnostic projection for a capture.
///
/// Takes `&KernelLayerCapture`: the projection cannot mutate the capture, so "observation-only" is
/// a property of the signature rather than a promise in a comment.
pub fn render_projection(capture: &KernelLayerCapture, host: &HostContext) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "=== delegated run diagnostics (non-canonical) ===");
    let _ = writeln!(
        out,
        "run_id={} events={} ringbuf_drops={} size_mismatch={}",
        capture.run_id, capture.event_count, capture.ringbuf_drops, capture.event_size_mismatch
    );

    let _ = writeln!(out, "\n-- A. kernel-side loss --");
    let a = capture.drop_layers();
    let _ = writeln!(
        out,
        "  ring          tracepoint={} lsm={} socket={}",
        a.tracepoint, a.lsm, a.socket
    );
    let _ = writeln!(
        out,
        "  tracepoint    openat={} openat2={} connect={} sendto={} sendmsg={}",
        a.openat, a.openat2, a.connect, a.sendto, a.sendmsg
    );
    // Named, not omitted. `sched_process_fork` bumps the ring counter and has no per-hook counter,
    // so a breakdown that printed only the five it can name would not add up to the ring total and
    // a reader would have a gap with nothing to explain it.
    let _ = writeln!(
        out,
        "  unattributed  {}  (hooks with no per-hook counter, today sched_process_fork)",
        a.unattributed
    );

    let _ = writeln!(out, "\n-- B. userspace reader pressure --");
    let _ = writeln!(out, "  decoder rejects   {}", capture.event_size_mismatch);
    let _ = writeln!(
        out,
        "  poll lag          not observable (would require changing the consumer loop)"
    );
    let _ = writeln!(out, "  reader backlog    not observable (as above)");

    let _ = writeln!(out, "\n-- C. normalizer / evidence filtering --");
    let (filtered, retained, top) = capture.filtering_layers();
    let raw = filtered + retained;
    let _ = writeln!(
        out,
        "  raw={} filtered={} retained={} reduction={}",
        raw,
        filtered,
        retained,
        reduction_ratio(raw, retained)
    );
    for (value, count) in top {
        let _ = writeln!(out, "    filtered  {count:>6}  {value}");
    }

    let _ = writeln!(out, "\n-- host --");
    let _ = writeln!(
        out,
        "  kernel={} cpus={} ringbuf_pages={} runner={} attach={} cgroup={}",
        or_unknown(host.kernel_version.as_deref()),
        host.cpu_count.map_or("unknown".into(), |n| n.to_string()),
        host.ringbuf_pages
            .map_or("unknown".into(), |n| n.to_string()),
        or_unknown(host.runner_label.as_deref()),
        or_unknown(host.attach_mode.as_deref()),
        or_unknown(host.cgroup_correlation.as_deref()),
    );
    out
}

/// Retained over raw, to three decimals, or `n/a` when nothing was seen.
///
/// `n/a` rather than `1.000`: a run that observed nothing did not retain everything, and a ratio
/// that reads as perfect retention on an empty denominator is the same shape as a pass rate with no
/// denominator.
fn reduction_ratio(raw: u64, retained: u64) -> String {
    if raw == 0 {
        return "n/a".to_string();
    }
    format!("{:.3}", retained as f64 / raw as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::KernelLayerBuilder;
    use assay_monitor::MonitorStatsSnapshot;

    /// A capture with drops on two attributed hooks and one that no hook claims.
    ///
    /// Built through the real `finish(before, after)` rather than by hand, so the projection is
    /// rendering the same shape the runner produces.
    fn capture_with_drops() -> KernelLayerCapture {
        let before = MonitorStatsSnapshot::default();
        let after = MonitorStatsSnapshot {
            tracepoint_ringbuf_dropped: 9,
            openat_ringbuf_dropped: 3,
            sendto_ringbuf_dropped: 2,
            lsm_ringbuf_dropped: 1,
            ..MonitorStatsSnapshot::default()
        };
        KernelLayerBuilder::new("run-diagnostics")
            .expect("run id")
            .finish(&before, &after)
    }

    fn capture() -> KernelLayerCapture {
        let empty = MonitorStatsSnapshot::default();
        KernelLayerBuilder::new("run-diagnostics")
            .expect("run id")
            .finish(&empty, &empty)
    }

    #[test]
    fn the_projection_names_all_three_layers_and_the_host() {
        let text = render_projection(&capture(), &HostContext::default());
        for expected in [
            "A. kernel-side loss",
            "B. userspace reader pressure",
            "C. normalizer / evidence filtering",
            "-- host --",
        ] {
            assert!(text.contains(expected), "missing {expected}:\n{text}");
        }
    }

    /// An unmeasurable quantity is named as unmeasurable, not printed as zero.
    ///
    /// A `0` here would be indistinguishable from "measured, and there was none" — which is the
    /// exact conflation this issue exists to remove, one layer up.
    #[test]
    fn an_unobservable_measurement_says_so_rather_than_reporting_zero() {
        let text = render_projection(&capture(), &HostContext::default());
        assert!(text.contains("poll lag          not observable"));
        assert!(text.contains("reader backlog    not observable"));
        assert!(!text.contains("poll lag          0"));
    }

    /// A host fact nobody filled reads as unknown, never as a plausible default.
    #[test]
    fn an_unfilled_host_fact_is_unknown() {
        let text = render_projection(&capture(), &HostContext::default());
        assert!(text.contains("runner=unknown"));
        assert!(text.contains("ringbuf_pages=unknown"));
    }

    #[test]
    fn a_filled_host_fact_is_reported() {
        let host = HostContext {
            kernel_version: Some("6.8.0-generic".into()),
            cpu_count: Some(8),
            ringbuf_pages: Some(256),
            runner_label: Some("assay-bpf-runner".into()),
            attach_mode: Some("lsm+tracepoint".into()),
            cgroup_correlation: Some("correlated".into()),
        };
        let text = render_projection(&capture(), &host);
        assert!(text.contains("kernel=6.8.0-generic cpus=8 ringbuf_pages=256"));
        assert!(text.contains("runner=assay-bpf-runner"));
    }

    /// The unattributed remainder is printed and explained, not hidden.
    #[test]
    fn the_unattributed_remainder_is_named() {
        let text = render_projection(&capture(), &HostContext::default());
        assert!(text.contains("unattributed"));
        assert!(text.contains("sched_process_fork"));
    }

    /// The layers un-conflate a real drop count, which is the whole point of the projection.
    ///
    /// Nine tracepoint drops: three `openat`, two `sendto`, and four no hook claims. One number
    /// becomes three answers, and the three add back up to the one.
    #[test]
    fn a_real_drop_count_is_split_by_hook_and_adds_back_up() {
        let capture = capture_with_drops();
        let a = capture.drop_layers();
        assert_eq!((a.openat, a.sendto), (3, 2));
        assert_eq!(a.unattributed, 4);
        assert_eq!(
            a.openat + a.openat2 + a.connect + a.sendto + a.sendmsg + a.unattributed,
            a.tracepoint,
            "the split must reconstruct the ring counter it came from"
        );

        let text = render_projection(&capture, &HostContext::default());
        assert!(text.contains("openat=3"), "{text}");
        assert!(text.contains("sendto=2"), "{text}");
        assert!(text.contains("unattributed  4"), "{text}");
    }

    /// An empty run has no reduction ratio rather than a perfect one.
    #[test]
    fn an_empty_run_has_no_reduction_ratio() {
        assert_eq!(reduction_ratio(0, 0), "n/a");
        assert_eq!(reduction_ratio(10, 4), "0.400");
    }

    /// Observation-only, asserted rather than promised: the capture is unchanged by rendering.
    #[test]
    fn rendering_does_not_change_the_capture() {
        let before = capture();
        let after = before.clone();
        let _ = render_projection(&after, &HostContext::default());
        assert_eq!(before, after, "the projection mutated the capture it read");
    }

    /// The projection is text and nothing parses it back.
    ///
    /// Pinned because the moment it acquires a schema it becomes something a consumer can depend
    /// on, and then something acceptance can quietly start resting on — which is the one thing
    /// #1271's non-goal forbids.
    #[test]
    fn the_projection_is_not_a_parseable_artifact() {
        let text = render_projection(&capture(), &HostContext::default());
        assert!(
            serde_json::from_str::<serde_json::Value>(&text).is_err(),
            "the diagnostic projection parsed as JSON, which invites a consumer to depend on it"
        );
    }
}
