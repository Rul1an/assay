//! Assay-owned proxy JSON-RPC application error mapping.
//!
//! One function answers which integer the proxy originates. Codes sit outside
//! JSON-RPC's reserved `-32768..=-32000` band. Upstream reserved codes are not
//! mapped here.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProxyErrorKind {
    Unsupported,
    Failed,
    Denied,
}

pub(super) const fn proxy_error_code(kind: ProxyErrorKind) -> i32 {
    match kind {
        ProxyErrorKind::Unsupported => -31997,
        ProxyErrorKind::Failed => -31998,
        ProxyErrorKind::Denied => -31999,
    }
}

pub(super) const PROXY_UNSUPPORTED: i32 = proxy_error_code(ProxyErrorKind::Unsupported);
pub(super) const PROXY_FAILED: i32 = proxy_error_code(ProxyErrorKind::Failed);
pub(super) const PROXY_DENIED: i32 = proxy_error_code(ProxyErrorKind::Denied);

#[cfg(test)]
mod tests {
    use super::{proxy_error_code, ProxyErrorKind, PROXY_DENIED, PROXY_FAILED, PROXY_UNSUPPORTED};

    const JSONRPC_RESERVED: std::ops::RangeInclusive<i32> = -32768..=-32000;

    #[test]
    fn mapping_is_exact() {
        assert_eq!(proxy_error_code(ProxyErrorKind::Unsupported), -31997);
        assert_eq!(proxy_error_code(ProxyErrorKind::Failed), -31998);
        assert_eq!(proxy_error_code(ProxyErrorKind::Denied), -31999);
        assert_eq!(PROXY_UNSUPPORTED, -31997);
        assert_eq!(PROXY_FAILED, -31998);
        assert_eq!(PROXY_DENIED, -31999);
    }

    #[test]
    fn codes_are_unique_and_outside_jsonrpc_reserved_band() {
        let codes = [
            proxy_error_code(ProxyErrorKind::Unsupported),
            proxy_error_code(ProxyErrorKind::Failed),
            proxy_error_code(ProxyErrorKind::Denied),
        ];
        for (i, left) in codes.iter().enumerate() {
            assert!(
                !JSONRPC_RESERVED.contains(left),
                "{left} must sit outside JSON-RPC reserved {JSONRPC_RESERVED:?}"
            );
            for right in codes.iter().skip(i + 1) {
                assert_ne!(left, right, "Assay-owned proxy codes must be unique");
            }
        }
    }

    #[test]
    fn mapping_rejects_the_abandoned_reserved_band() {
        assert_ne!(proxy_error_code(ProxyErrorKind::Unsupported), -32040);
        assert_ne!(proxy_error_code(ProxyErrorKind::Failed), -32041);
        assert_ne!(proxy_error_code(ProxyErrorKind::Denied), -32042);
    }

    #[test]
    fn denied_matches_reader_v1_marker_code() {
        assert_eq!(
            proxy_error_code(ProxyErrorKind::Denied) as i64,
            assay_evidence::PROXY_DENIED_V1
        );
    }
}
