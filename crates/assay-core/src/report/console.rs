use crate::model::{TestResultRow, TestStatus};

pub fn print_summary(results: &[TestResultRow], explain_skip: bool) {
    let mut pass = 0;
    let mut fail = 0;
    let mut flaky = 0;
    let mut unstable = 0;
    let mut warn = 0;
    let mut error = 0;
    let mut skipped = 0;

    eprintln!("\nRunning {} tests...", results.len());

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
            TestStatus::Pass => {
                pass += 1;
                eprintln!("✅ {:<20} {}  {}", r.test_id, score_str, duration);
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
