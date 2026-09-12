use serde::Deserialize;

/// Re-exported so existing `use super::limits::LimitReader` call sites keep working; the
/// implementation now lives in `assay-common` beside the replay verifier that shares it.
pub(crate) use assay_common::limits::LimitReader;

/// Resource limits for bundle verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifyLimits {
    pub max_bundle_bytes: u64,
    pub max_decode_bytes: u64,
    pub max_manifest_bytes: u64,
    pub max_events_bytes: u64,
    pub max_events: usize,
    pub max_line_bytes: usize,
    pub max_path_len: usize,
    pub max_json_depth: usize,
}

impl Default for VerifyLimits {
    fn default() -> Self {
        Self {
            max_bundle_bytes: 100_u64 * 1024 * 1024,
            max_decode_bytes: 1024_u64 * 1024 * 1024,
            max_manifest_bytes: 10_u64 * 1024 * 1024,
            max_events_bytes: 500_u64 * 1024 * 1024,
            max_events: 100_000,
            max_line_bytes: 1024 * 1024,
            max_path_len: 256,
            max_json_depth: 64,
        }
    }
}

/// Partial overrides for `VerifyLimits`. Used for CLI/config JSON parsing.
/// Unknown keys cause deserialization to fail (deny_unknown_fields).
/// Merge with `VerifyLimits::default().apply(overrides)`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyLimitsOverrides {
    pub max_bundle_bytes: Option<u64>,
    pub max_decode_bytes: Option<u64>,
    pub max_manifest_bytes: Option<u64>,
    pub max_events_bytes: Option<u64>,
    pub max_events: Option<usize>,
    pub max_line_bytes: Option<usize>,
    pub max_path_len: Option<usize>,
    pub max_json_depth: Option<usize>,
}

impl VerifyLimits {
    /// Limits for a caller that retains decompressed `events.ndjson`.
    ///
    /// `max_events_bytes` is the compressed-bundle ceiling (100 MiB), five times
    /// tighter than [`Self::default`]. Call this at the open site so residency is a
    /// chosen bound, not the 500 MiB default inherited by [`crate::bundle::BundleReader::open`].
    pub fn for_retained_events() -> Self {
        let defaults = Self::default();
        Self {
            max_events_bytes: defaults.max_bundle_bytes,
            ..defaults
        }
    }

    /// [`Self::for_retained_events`] with the caller's event cap applied before materialization.
    ///
    /// `max_events_bytes` is also capped at `max_events * max_line_bytes` so a small
    /// `--max-events` cannot still hold the full retain ceiling.
    pub fn for_retained_events_capped(max_events: usize) -> Self {
        let mut limits = Self::for_retained_events();
        limits.max_events = max_events;
        let from_lines = (max_events as u64).saturating_mul(limits.max_line_bytes as u64);
        limits.max_events_bytes = limits.max_events_bytes.min(from_lines);
        limits
    }

    /// Apply overrides onto these defaults. Only `Some` values override.
    pub fn apply(self, overrides: VerifyLimitsOverrides) -> Self {
        Self {
            max_bundle_bytes: overrides.max_bundle_bytes.unwrap_or(self.max_bundle_bytes),
            max_decode_bytes: overrides.max_decode_bytes.unwrap_or(self.max_decode_bytes),
            max_manifest_bytes: overrides
                .max_manifest_bytes
                .unwrap_or(self.max_manifest_bytes),
            max_events_bytes: overrides.max_events_bytes.unwrap_or(self.max_events_bytes),
            max_events: overrides.max_events.unwrap_or(self.max_events),
            max_line_bytes: overrides.max_line_bytes.unwrap_or(self.max_line_bytes),
            max_path_len: overrides.max_path_len.unwrap_or(self.max_path_len),
            max_json_depth: overrides.max_json_depth.unwrap_or(self.max_json_depth),
        }
    }
}
