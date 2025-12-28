//! Usage telemetry for metered billing integration (e.g., Celonis).
//!
//! This module provides standardized logging for usage events.
//! These logs are ingested by platform observability stacks to calculate billing.

/// Logs a standardized usage event.
///
/// # Arguments
/// * `event_type` - The category of usage (e.g., "policy_check", "token_usage")
/// * `count` - The amount consumed
pub fn log_usage_event(event_type: &str, count: u64) {
    tracing::info!(
        target: "assay_billing",
        event = "assay.usage.metered",
        usage_type = %event_type,
        count = count,
    );
}
