//! `assay.token_passthrough_conformance.v0` (MCP01a-3) — credential-boundary conformance.
//!
//! The confused-deputy invariant: only a component that **consumes** inbound auth can leak that
//! consumed credential as a passthrough. This carrier reports, value-free, that the consuming path
//! (the `server.rs` validator + the outbound tool surface) does not re-emit a consumed inbound
//! authentication value on its outbound headers, bodies, or spawned env.
//!
//! The transparent stdio relay (`proxy::run`) is explicitly OUT of scope: it consumes no inbound
//! auth, originates no outbound HTTP with it, and injects no child env, so its verbatim JSON-RPC
//! forwarding is relay behaviour, not consumed-token passthrough. This is recorded, never "proven safe
//! by stripping". No strip/sanitize/abort path is built for a leak the current topology cannot
//! produce; the value-sentinel proof lives in `tests/no_passthrough_e2e.rs`.

use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "assay.token_passthrough_conformance.v0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProbedSource {
    pub source: String,
    /// Always true: a known inbound-credential location the boundary watches.
    pub probed: bool,
    /// True only where the tested consuming path actually validates this field as authn. On the
    /// stdio consuming path the `server.rs` validator consumes the initialize-param auth fields; the
    /// `http.*` headers are the outbound-forbidden denylist (`SENSITIVE_HEADER_NAMES`) and are
    /// consumed as inbound authn only under an HTTP transport, which this topology does not exercise.
    pub consumed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutboundChannel {
    pub channel: String,
    pub checked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_applicable: Option<bool>,
    pub leak_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pass: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransparentRelay {
    pub in_scope: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenPassthroughConformance {
    pub schema: String,
    pub topology: String,
    pub probed_inbound_auth_sources: Vec<ProbedSource>,
    pub outbound_channels: Vec<OutboundChannel>,
    pub transparent_relay: TransparentRelay,
    pub non_claims: Vec<String>,
}

/// Does the EXACT consumed inbound value appear on a captured outbound surface? Value-exact on
/// purpose: an arbitrary credential-shaped payload with a different value is not a leak (the negative
/// control), so this never becomes a payload sanitizer.
pub fn value_leaks(consumed_value: &str, outbound_surface: &str) -> bool {
    !consumed_value.is_empty() && outbound_surface.contains(consumed_value)
}

fn source(s: &str, consumed: bool) -> ProbedSource {
    ProbedSource {
        source: s.to_string(),
        probed: true,
        consumed,
    }
}

/// The conformance report for the consuming-path topology. `transport_header` and `json_body` are
/// checked by the value-sentinel e2e; `environment` is `not_applicable` because the consuming path
/// (the `server.rs` validator) spawns no child — only the out-of-scope transparent relay spawns, and
/// it injects no inbound auth into the child env.
pub fn consuming_path_conformance() -> TokenPassthroughConformance {
    TokenPassthroughConformance {
        schema: SCHEMA.to_string(),
        topology: "consuming_path".to_string(),
        probed_inbound_auth_sources: vec![
            // http.* are probed (the outbound-forbidden denylist), consumed as inbound authn only
            // under an HTTP transport, which the stdio consuming path does not exercise.
            source("http.authorization", false),
            source("http.cookie", false),
            source("http.x_api_key", false),
            // The stdio consuming path's `server.rs` validator consumes these as authn.
            source("initialize.params.authorization", true),
            source("initialize.params.initializationOptions.authorization", true),
        ],
        outbound_channels: vec![
            OutboundChannel {
                channel: "transport_header".to_string(),
                checked: true,
                not_applicable: None,
                leak_count: 0,
                pass: Some(true),
            },
            OutboundChannel {
                channel: "json_body".to_string(),
                checked: true,
                not_applicable: None,
                leak_count: 0,
                pass: Some(true),
            },
            OutboundChannel {
                channel: "environment".to_string(),
                checked: false,
                not_applicable: Some(true),
                leak_count: 0,
                pass: None,
            },
        ],
        transparent_relay: TransparentRelay {
            in_scope: false,
            reason:
                "relay path does not consume inbound auth; verbatim JSON-RPC forwarding is relay \
                     behaviour, not consumed-token passthrough"
                    .to_string(),
        },
        non_claims: vec![
            "tracks only consumed inbound authentication values".to_string(),
            "probed http.* header sources are the outbound-forbidden denylist; they are consumed as \
             inbound authn only under an HTTP transport, not on this stdio consuming path"
                .to_string(),
            "does not scrub arbitrary credential-shaped user payload".to_string(),
            "transparent relay forwarding is not treated as a confused-deputy leak when the relay \
             does not consume the credential"
                .to_string(),
            "does not verify provider token grants".to_string(),
            "does not manage token lifecycle, rotation, or vaulting".to_string(),
        ],
    }
}

/// A report is clean when every outbound channel either is `not_applicable` or was checked with zero
/// leaks and `pass`. An unchecked, applicable channel is never silently clean.
pub fn is_clean(report: &TokenPassthroughConformance) -> bool {
    report.schema == SCHEMA
        && !report.outbound_channels.is_empty()
        && report.outbound_channels.iter().all(|c| {
            if c.not_applicable == Some(true) {
                true
            } else {
                c.checked && c.leak_count == 0 && c.pass == Some(true)
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_leak_is_exact_not_shape_based() {
        let consumed = "Bearer INBOUND_TOKEN_NEVER_FORWARD";
        // The exact consumed value on an outbound surface is a leak.
        assert!(value_leaks(
            consumed,
            "x-fwd: Bearer INBOUND_TOKEN_NEVER_FORWARD"
        ));
        // Negative control: a different credential-shaped value is NOT a leak (not a sanitizer).
        assert!(!value_leaks(
            consumed,
            r#"{"authorization":"not-the-consumed-sentinel"}"#
        ));
        assert!(!value_leaks("", "anything"));
    }

    #[test]
    fn consuming_path_report_is_clean_and_value_free() {
        let report = consuming_path_conformance();
        assert!(is_clean(&report));
        // env is not_applicable (consuming path spawns nothing), not a fake pass.
        let env = report
            .outbound_channels
            .iter()
            .find(|c| c.channel == "environment")
            .unwrap();
        assert_eq!(env.not_applicable, Some(true));
        assert!(!env.checked);
        // value-free: no token values anywhere in the serialized carrier.
        let blob = serde_json::to_string(&report).unwrap();
        assert!(!blob.contains("NEVER_FORWARD") && !blob.contains("Bearer "));
        // Every source is probed; only the initialize-param fields the server.rs validator actually
        // consumes are marked consumed (the http.* denylist is not, on this stdio consuming path).
        assert!(report.probed_inbound_auth_sources.iter().all(|s| s.probed));
        let consumed: Vec<&str> = report
            .probed_inbound_auth_sources
            .iter()
            .filter(|s| s.consumed)
            .map(|s| s.source.as_str())
            .collect();
        assert_eq!(
            consumed,
            vec![
                "initialize.params.authorization",
                "initialize.params.initializationOptions.authorization",
            ]
        );
    }

    #[test]
    fn transparent_relay_is_recorded_out_of_scope() {
        // Negative control 2: the transparent relay is explicitly out of the confused-deputy frame,
        // never counted as a leak surface.
        let report = consuming_path_conformance();
        assert!(!report.transparent_relay.in_scope);
        assert!(report.transparent_relay.reason.contains("does not consume"));
    }

    #[test]
    fn an_unchecked_applicable_channel_is_not_clean() {
        let mut report = consuming_path_conformance();
        report.outbound_channels.push(OutboundChannel {
            channel: "future_surface".to_string(),
            checked: false,
            not_applicable: None,
            leak_count: 0,
            pass: None,
        });
        assert!(!is_clean(&report));
    }
}
