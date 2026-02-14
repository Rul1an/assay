//! Cache policy boundary scaffold for cache split.
//!
//! Planned ownership (Step2+):
//! - TTL/eviction policy helpers
//! - no filesystem writes

use chrono::{DateTime, Duration, Utc};

use crate::types::PackHeaders;

pub(crate) fn parse_cache_control_expiry_impl(
    headers: &PackHeaders,
    default_ttl_secs: i64,
) -> DateTime<Utc> {
    let now = Utc::now();
    let default_ttl = Duration::seconds(default_ttl_secs);

    let ttl = headers
        .cache_control
        .as_ref()
        .and_then(|cc| {
            cc.split(',')
                .find(|part| part.trim().starts_with("max-age="))
                .and_then(|part| {
                    part.trim()
                        .strip_prefix("max-age=")
                        .and_then(|v| v.parse::<i64>().ok())
                })
        })
        .map(Duration::seconds)
        .unwrap_or(default_ttl);

    now + ttl
}
