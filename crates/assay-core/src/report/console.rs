use crate::model::{TestResultRow, TestStatus};
use crate::report::progress::{ProgressEvent, ProgressSink};
use crate::report::summary::{JudgeMetrics, Seeds};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// --- E4.3 Progress N/M (throttled, completion-order) ---

/// Format a single progress line for display. Deterministic, unit-testable.
/// ETA optional (not used in initial PR4; add when ETA is implemented).
#[must_use]
pub fn format_progress_line(done: usize, total: usize, _eta_secs: Option<u64>) -> String {
    format!("Running test {}/{}...", done, total)
}

/// Write a progress line to stderr. Used by the default progress sink after throttling.
pub fn emit_progress_line(line: &str) {
    eprintln!("{}", line);
}

/// Minimum interval between progress updates to avoid log spam.
const PROGRESS_MIN_INTERVAL_MS: u64 = 200;
/// For large suites, emit at most every this many tests (e.g. 10% step).
pub(crate) fn progress_step(total: usize) -> usize {
    if total <= 10 {
        1
    } else {
        std::cmp::max(1, total / 10)
    }
}

/// Returns a progress sink that throttles updates and prints to stderr via format_progress_line.
/// Skips intermediate updates when total == 1 (no "1/1"). Always emits on done == total.
pub fn default_progress_sink(total: usize) -> Option<ProgressSink> {
    if total <= 1 {
        return None;
    }
    let step = progress_step(total);
    let state = Mutex::new(ThrottleState {
        last_emit: None,
        last_done: 0,
    });
    let state = std::sync::Arc::new(state);
    Some(Arc::new(move |ev: ProgressEvent| {
        if ev.total == 0 {
            return;
        }
        let now = Instant::now();
        let should_emit = {
            let mut g = state.lock().expect("progress throttle lock");
            let emit_final = ev.done == ev.total;
            let emit_step = ev.done.is_multiple_of(step) || ev.done == 1;
            let interval_ok = g
                .last_emit
                .map(|t| {
                    now.saturating_duration_since(t)
                        >= Duration::from_millis(PROGRESS_MIN_INTERVAL_MS)
                })
                .unwrap_or(true);
            let ok = emit_final || (emit_step && interval_ok);
            if ok {
                g.last_emit = Some(now);
                g.last_done = ev.done;
            }
            ok
        };
        if should_emit {
            let line = format_progress_line(ev.done, ev.total, None);
            emit_progress_line(&line);
        }
    }))
}

struct ThrottleState {
    last_emit: Option<Instant>,
    last_done: usize,
}

/// Print seeds and judge metrics to stderr (E7.2/E7.3 job summary visibility in CI logs).
pub fn print_run_footer(seeds: Option<&Seeds>, judge_metrics: Option<&JudgeMetrics>) {
    if let Some(s) = seeds {
        let order = s
            .order_seed
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".into());
        let judge = s
            .judge_seed
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".into());
        eprintln!(
            "Seeds: seed_version={} order_seed={} judge_seed={}",
            s.seed_version, order, judge
        );
    }
    if let Some(m) = judge_metrics {
        let abstain = m
            .abstain_rate
            .map(|r| format!("{:.2}", r))
            .unwrap_or_else(|| "—".into());
        let flip = m
            .flip_rate
            .map(|r| format!("{:.2}", r))
            .unwrap_or_else(|| "—".into());
        let consensus = m
            .consensus_rate
            .map(|r| format!("{:.2}", r))
            .unwrap_or_else(|| "—".into());
        let unavail = m
            .unavailable_count
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".into());
        eprintln!(
            "Judge metrics: abstain_rate={} flip_rate={} consensus_rate={} unavailable_count={}",
            abstain, flip, consensus, unavail
        );
    }
}

pub fn print_summary(results: &[TestResultRow], explain_skip: bool) {
    let mut pass = 0;
    let mut fail = 0;
    let mut flaky = 0;
    let mut unstable = 0;
    let mut warn = 0;
    let mut error = 0;
    let mut skipped = 0;

    eprintln!();
    for r in results {
        let duration = r
            .duration_ms
            .map(|d| format!("({:.1}s)", d as f64 / 1000.0))
            .unwrap_or_default();
        let score_str = r
            .score
            .map(|s| format!("{:.2}", s))
            .unwrap_or_else(|| "PASS".into());

        match r.status {
            TestStatus::Pass | TestStatus::AllowedOnError => {
                pass += 1;
                let icon = if r.status == TestStatus::AllowedOnError {
                    "⚡"
                } else {
                    "✅"
                };
                eprintln!("{} {:<20} {}  {}", icon, r.test_id, score_str, duration);
                if r.status == TestStatus::AllowedOnError {
                    eprintln!("    (Allowed by error policy: {})", r.message);
                }
            }
            TestStatus::Skipped => {
                skipped += 1;
                if explain_skip {
                    let skip = r.details.get("skip");
                    let reason = skip
                        .and_then(|s| s.get("reason"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    eprintln!("⏭️  {}", r.test_id);
                    eprintln!("    Status: SKIPPED ({})", reason.replace('_', " "));

                    if let Some(s) = skip {
                        let rid = s.get("previous_run_id").and_then(|v| v.as_i64());
                        let pat = s.get("previous_at").and_then(|v| v.as_str());
                        let sc = s.get("previous_score").and_then(|v| v.as_f64());

                        if let Some(id) = rid {
                            let mut parts = vec![format!("run #{}", id)];
                            if let Some(ts) = pat {
                                parts.push(format!("@ {}", ts));
                            }
                            parts.push("(passed".into());
                            if let Some(score) = sc {
                                parts.push(format!(", score: {:.2})", score));
                            } else {
                                parts.push(")".into());
                            }
                            eprintln!("    Previous: {}", parts.join(" "));
                        } else {
                            eprintln!("    Previous: (details unavailable)");
                        }

                        if let Some(fp) = s.get("fingerprint").and_then(|v| v.as_str()) {
                            let trunc = if fp.len() > 50 {
                                format!("{}...", &fp[..50])
                            } else {
                                fp.to_string()
                            };
                            eprintln!("    Fingerprint: {}", trunc);
                        }
                    }
                    eprintln!("    To rerun: assay run --refresh-cache");
                } else {
                    eprintln!("⏭️  {:<20} SKIPPED ({})", r.test_id, r.message);
                }
            }
            TestStatus::Flaky => {
                flaky += 1;
                eprintln!("⚠️  {:<20} FLAKY {}", r.test_id, duration);
                if !r.message.is_empty() {
                    eprintln!("    {}", r.message);
                }
            }
            TestStatus::Unstable => {
                unstable += 1;
                eprintln!("⚠️  {:<20} UNSTABLE {}", r.test_id, duration);
                if !r.message.is_empty() {
                    eprintln!("    {}", r.message);
                }
            }
            TestStatus::Warn => {
                warn += 1;
                eprintln!("⚠️  {:<20} WARN {}", r.test_id, duration);
                if !r.message.is_empty() {
                    eprintln!("    {}", r.message);
                }
            }
            TestStatus::Fail => {
                fail += 1;
                eprintln!("❌ {:<20} {}  {}", r.test_id, r.message, duration);
                if let Some(prompt) = r.details.get("prompt").and_then(|p| p.as_str()) {
                    // Truncate if extremely long, but typically we want to see it
                    let display_prompt = if prompt.len() > 100 {
                        format!("{}...", &prompt[..100])
                    } else {
                        prompt.to_string()
                    };
                    eprintln!("      Prompt: \"{}\"", display_prompt);
                }
                if let Some(failures) = r.details.get("assertions").and_then(|a| a.as_array()) {
                    for f in failures {
                        if let Some(msg) = f.get("message").and_then(|m| m.as_str()) {
                            eprintln!("      → {}", msg);
                        }
                    }
                }
            }
            TestStatus::Error => {
                error += 1;
                eprintln!("💥 {:<20} ERROR: {}", r.test_id, r.message);
            }
        }
    }

    eprintln!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    let skip_hint = if skipped > 0 && !explain_skip {
        " (use --explain-skip for details)"
    } else {
        ""
    };
    eprintln!(
        "Summary: {} passed, {} failed, {} skipped{}, {} flaky, {} unstable, {} warn, {} error",
        pass, fail, skipped, skip_hint, flaky, unstable, warn, error
    );
}

#[cfg(test)]
mod tests {
    use super::{default_progress_sink, format_progress_line, progress_step};

    #[test]
    fn format_progress_line_contains_done_and_total() {
        let s = format_progress_line(3, 10, None);
        assert!(s.contains("3/10"), "expected '3/10' in {:?}", s);
        assert!(
            s.contains("Running test"),
            "expected 'Running test' in {:?}",
            s
        );
    }

    #[test]
    fn format_progress_line_final() {
        let s = format_progress_line(5, 5, None);
        assert!(s.contains("5/5"));
    }

    #[test]
    fn default_progress_sink_none_for_total_0_or_1() {
        assert!(default_progress_sink(0).is_none());
        assert!(default_progress_sink(1).is_none());
    }

    #[test]
    fn progress_step_logic() {
        assert_eq!(progress_step(5), 1);
        assert_eq!(progress_step(10), 1);
        assert_eq!(progress_step(25), 2); // 25/10 = 2
        assert_eq!(progress_step(100), 10);
    }
}
