//! Latency benchmark for Assay policy checks
//!
//! Run with:
//!   cargo bench -p assay-core --bench latency
//!
//! Or for quick check:
//!   cargo test -p assay-core --test latency_check -- --nocapture

use std::time::{Duration, Instant};

/// SLA targets for v1.0
#[cfg(not(debug_assertions))]
const P50_TARGET_MS: f64 = 2.0;
#[cfg(debug_assertions)]
const P50_TARGET_MS: f64 = 50.0; // Relaxed for debug

#[cfg(not(debug_assertions))]
const P95_TARGET_MS: f64 = 10.0;
#[cfg(debug_assertions)]
const P95_TARGET_MS: f64 = 150.0; // Relaxed for debug

#[cfg(not(debug_assertions))]
const P99_TARGET_MS: f64 = 50.0;
#[cfg(debug_assertions)]
const P99_TARGET_MS: f64 = 500.0; // Relaxed for debug

/// Number of iterations for benchmark
const ITERATIONS: usize = 1000;

/// Benchmark results
#[derive(Debug)]
struct BenchmarkResults {
    min_ms: f64,
    p50_ms: f64,
    mean_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
    samples: usize,
}

impl BenchmarkResults {
    fn from_durations(mut durations: Vec<Duration>) -> Self {
        durations.sort();

        let samples = durations.len();
        let to_ms = |d: Duration| d.as_secs_f64() * 1000.0;

        let sum: Duration = durations.iter().sum();
        let mean_ms = to_ms(sum) / samples as f64;

        Self {
            min_ms: to_ms(durations[0]),
            p50_ms: to_ms(durations[samples * 50 / 100]),
            mean_ms,
            p95_ms: to_ms(durations[samples * 95 / 100]),
            p99_ms: to_ms(durations[samples * 99 / 100]),
            max_ms: to_ms(durations[samples - 1]),
            samples,
        }
    }

    fn check_sla(&self) -> bool {
        let p50_ok = self.p50_ms <= P50_TARGET_MS;
        let p95_ok = self.p95_ms <= P95_TARGET_MS;
        let p99_ok = self.p99_ms <= P99_TARGET_MS;

        println!("\nSLA Check:");
        println!(
            "  p50: {} ({:.3}ms <= {:.1}ms)",
            if p50_ok { "✓ PASS" } else { "✗ FAIL" },
            self.p50_ms,
            P50_TARGET_MS
        );
        println!(
            "  p95: {} ({:.3}ms <= {:.1}ms)",
            if p95_ok { "✓ PASS" } else { "✗ FAIL" },
            self.p95_ms,
            P95_TARGET_MS
        );
        println!(
            "  p99: {} ({:.3}ms <= {:.1}ms)",
            if p99_ok { "✓ PASS" } else { "✗ FAIL" },
            self.p99_ms,
            P99_TARGET_MS
        );

        p50_ok && p95_ok && p99_ok
    }

    fn print_summary(&self) {
        println!("\n========================================");
        println!("Latency Benchmark Results ({} samples)", self.samples);
        println!("========================================");
        println!("  Min:  {:>8.3} ms", self.min_ms);
        println!("  p50:  {:>8.3} ms", self.p50_ms);
        println!("  Mean: {:>8.3} ms", self.mean_ms);
        println!("  p95:  {:>8.3} ms", self.p95_ms);
        println!("  p99:  {:>8.3} ms", self.p99_ms);
        println!("  Max:  {:>8.3} ms", self.max_ms);
    }
}

use assay_core::policy_engine::{evaluate_tool_args, VerdictStatus};

/// Simulated policy check (replace with actual implementation)
fn check_args_valid(schema: &serde_json::Value, args: &serde_json::Value) -> bool {
    let tool_name = "test_tool";
    let policy = serde_json::json!({
        tool_name: schema
    });

    let verdict = evaluate_tool_args(&policy, tool_name, args);
    verdict.status == VerdictStatus::Allowed
}

use assay_core::policy_engine::evaluate_sequence;

/// Simulated sequence check
fn check_sequence_valid(rules: &[&str], trace: &[&str]) -> bool {
    let policy_str = rules.join(" THEN ");
    // evaluate_sequence expects regex-like policy and slice of tool names
    // Convert slice of &str to Vec<String>
    let trace_strings: Vec<String> = trace.iter().map(|s| s.to_string()).collect();
    let verdict = evaluate_sequence(&policy_str, &trace_strings);
    verdict.status == VerdictStatus::Allowed
}

/// Simulated blocklist check
fn check_blocklist(blocked: &[&str], tool_name: &str) -> bool {
    // Simulate blocklist check
    !blocked.contains(&tool_name)
}

fn main() {
    println!("Run benchmarks with: cargo test -p assay-core --test latency_check -- --nocapture");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_args_valid() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "percent": { "type": "number", "maximum": 30 },
                "reason": { "type": "string", "minLength": 10 }
            },
            "required": ["percent", "reason"]
        });

        let args = serde_json::json!({
            "percent": 15,
            "reason": "Loyalty program discount"
        });

        // Warmup
        for _ in 0..100 {
            let _ = check_args_valid(&schema, &args);
        }

        // Benchmark
        let mut durations = Vec::with_capacity(ITERATIONS);
        for _ in 0..ITERATIONS {
            let start = Instant::now();
            let _ = check_args_valid(&schema, &args);
            durations.push(start.elapsed());
        }

        let results = BenchmarkResults::from_durations(durations);
        results.print_summary();

        assert!(
            results.check_sla(),
            "SLA targets not met - this blocks v1.0 release"
        );
    }

    #[test]
    fn benchmark_sequence_valid() {
        let rules = vec!["VerifyIdentity", "ConfirmAction"];
        let trace = vec![
            "VerifyIdentity",
            "CheckPermissions",
            "ConfirmAction",
            "Execute",
        ];

        // Warmup
        for _ in 0..100 {
            let _ = check_sequence_valid(&rules, &trace);
        }

        // Benchmark
        let mut durations = Vec::with_capacity(ITERATIONS);
        for _ in 0..ITERATIONS {
            let start = Instant::now();
            let _ = check_sequence_valid(&rules, &trace);
            durations.push(start.elapsed());
        }

        let results = BenchmarkResults::from_durations(durations);
        results.print_summary();

        assert!(results.check_sla(), "SLA targets not met");
    }

    #[test]
    fn benchmark_blocklist() {
        let blocked = vec![
            "DeleteDatabase",
            "DropTable",
            "ExecuteRawSQL",
            "AdminOverride",
        ];
        let tool = "LookupCustomer";

        // Warmup
        for _ in 0..100 {
            let _ = check_blocklist(&blocked, tool);
        }

        // Benchmark
        let mut durations = Vec::with_capacity(ITERATIONS);
        for _ in 0..ITERATIONS {
            let start = Instant::now();
            let _ = check_blocklist(&blocked, tool);
            durations.push(start.elapsed());
        }

        let results = BenchmarkResults::from_durations(durations);
        results.print_summary();

        assert!(results.check_sla(), "SLA targets not met");
    }

    #[test]
    fn benchmark_combined_checks() {
        // Simulate a realistic policy check with all three types
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "customer_id": { "type": "string", "pattern": "^CUST-\\d{5}$" }
            }
        });
        let args = serde_json::json!({ "customer_id": "CUST-12345" });
        let rules = vec!["VerifyIdentity"];
        let trace = vec!["VerifyIdentity", "LookupCustomer"];
        let blocked = vec!["DeleteDatabase"];
        let tool = "LookupCustomer";

        // Warmup
        for _ in 0..100 {
            let _ = check_args_valid(&schema, &args);
            let _ = check_sequence_valid(&rules, &trace);
            let _ = check_blocklist(&blocked, tool);
        }

        // Benchmark combined
        let mut durations = Vec::with_capacity(ITERATIONS);
        for _ in 0..ITERATIONS {
            let start = Instant::now();
            let _ = check_args_valid(&schema, &args);
            let _ = check_sequence_valid(&rules, &trace);
            let _ = check_blocklist(&blocked, tool);
            durations.push(start.elapsed());
        }

        let results = BenchmarkResults::from_durations(durations);
        results.print_summary();

        println!("\n[Combined check = args_valid + sequence_valid + blocklist]");

        assert!(
            results.check_sla(),
            "Combined SLA targets not met - this blocks v1.0 release"
        );
    }
}
