// Internal request routing for the proxy-originated establish flow (P61e, Increment 2 slice 1).
//
// The proxy spawns a single upstream stdio child and runs exactly ONE task that owns the child's
// stdout (the upstream reader). The establish flow (slice 2) needs the proxy to originate its own
// `tools/list` to the child and read the matching response WITHOUT adding a second stdout consumer
// (two readers would race and lose/misroute frames). This module is the routing scaffold the single
// reader uses: a reserved request-id namespace, a pending-request registry, the pure routing decision
// that diverts a matching response to an internal channel (suppressing it from the client stream), and
// a guard that rejects client requests whose id collides with the reserved namespace.
//
// Slice 1 wires the suppression decision and the collision guard into the live relay in a strictly
// BEHAVIOR-PRESERVING way: nothing registers a reserved id yet, so `is_pending` is always false and
// every upstream line relays to the client exactly as before; no legitimate client uses the reserved
// prefix. The registration side (`mint_reserved_id`, `register`, `await_establish`) is exercised by the
// tests and wired by slice 2; it carries `allow(dead_code)` until then.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use tokio::sync::oneshot;

/// Reserved id namespace for proxy-originated requests. A high-entropy nonce is appended per request
/// (slice 2). A client request whose id is a string in this namespace is rejected, so a client id can
/// never be mistaken for — or swallow — an establish response.
pub const RESERVED_ID_PREFIX: &str = "assay-establish-";

/// True iff `id` is a JSON string in the reserved namespace. Numeric ids and non-prefixed string ids
/// (the only forms a normal client or upstream uses) are never reserved.
pub fn is_reserved_id(id: &Value) -> bool {
    id.as_str()
        .is_some_and(|s| s.starts_with(RESERVED_ID_PREFIX))
}

/// Mint a reserved request id from a high-entropy nonce (the caller supplies the nonce; slice 2 uses a
/// CSPRNG). Pure/deterministic for testability.
#[allow(dead_code)] // registration side: wired by slice 2.
pub fn mint_reserved_id(nonce: &str) -> String {
    format!("{RESERVED_ID_PREFIX}{nonce}")
}

/// Does this client request carry a reserved id? Such a request MUST be rejected, never forwarded:
/// otherwise a malicious or unlucky client id collision could route a real client response into (and be
/// swallowed by) the establish registry.
pub fn client_id_is_reserved(v: &Value) -> bool {
    v.get("id").map(is_reserved_id).unwrap_or(false)
}

/// Where the single upstream reader should send a parsed upstream line.
#[derive(Debug, PartialEq, Eq)]
pub enum UpstreamRoute {
    /// Relay verbatim to the client (the default for all normal traffic).
    RelayToClient,
    /// Divert to the establish registry under this reserved id; suppress from the client stream.
    DivertToEstablish(String),
}

/// Pure routing decision. A line is diverted ONLY when it carries a reserved id that is CURRENTLY
/// PENDING in the registry. A reserved id that is not pending still relays to the client (never
/// silently swallowed), and everything else (normal responses, notifications without an id) relays.
pub fn route_upstream(v: &Value, is_pending: impl Fn(&str) -> bool) -> UpstreamRoute {
    match v.get("id") {
        Some(id) if is_reserved_id(id) => {
            let id = id.as_str().unwrap_or_default().to_string();
            if !id.is_empty() && is_pending(&id) {
                UpstreamRoute::DivertToEstablish(id)
            } else {
                UpstreamRoute::RelayToClient
            }
        }
        _ => UpstreamRoute::RelayToClient,
    }
}

/// Registry of in-flight proxy-originated establish requests, shared between the establish caller
/// (registers an id and awaits the response, slice 2) and the single upstream reader (resolves it).
/// One sender per reserved id. The map uses a std mutex: every critical section is a tiny insert /
/// contains / remove with no `.await` held.
#[derive(Clone, Default)]
pub struct EstablishRegistry {
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>,
}

impl EstablishRegistry {
    /// Register a reserved id; returns the receiver the caller awaits (slice 2).
    #[allow(dead_code)] // registration side: wired by slice 2.
    pub fn register(&self, id: String) -> oneshot::Receiver<Value> {
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .expect("establish registry mutex")
            .insert(id, tx);
        rx
    }

    /// Is this reserved id currently awaiting a response? Drives the reader's routing decision.
    pub fn is_pending(&self, id: &str) -> bool {
        self.pending
            .lock()
            .expect("establish registry mutex")
            .contains_key(id)
    }

    /// Deliver a matching upstream response to its waiting caller and remove the entry. Returns true if
    /// a waiter was found (so the reader suppresses the line from the client), false otherwise (so the
    /// reader falls back to relaying — never silently dropping a non-pending line). A dropped receiver
    /// (caller already timed out) still counts as resolved/suppressed.
    pub fn resolve(&self, id: &str, value: Value) -> bool {
        let sender = self
            .pending
            .lock()
            .expect("establish registry mutex")
            .remove(id);
        match sender {
            Some(tx) => {
                let _ = tx.send(value);
                true
            }
            None => false,
        }
    }
}

/// Await an establish response under one per-operation timeout. `None` on timeout or a dropped sender,
/// which the caller treats as a failed establish (fail-closed). Wired by slice 2.
#[allow(dead_code)] // registration side: wired by slice 2.
pub async fn await_establish(rx: oneshot::Receiver<Value>, budget: Duration) -> Option<Value> {
    match tokio::time::timeout(budget, rx).await {
        Ok(Ok(v)) => Some(v),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- reserved id namespace ---

    #[test]
    fn reserved_id_recognizes_only_prefixed_strings() {
        assert!(is_reserved_id(&json!("assay-establish-abc123")));
        assert!(!is_reserved_id(&json!("tools-list-1")));
        assert!(!is_reserved_id(&json!(42)));
        assert!(!is_reserved_id(&json!(null)));
    }

    #[test]
    fn mint_reserved_id_uses_the_prefix() {
        let id = mint_reserved_id("nonce-deadbeef");
        assert!(id.starts_with(RESERVED_ID_PREFIX));
        assert!(is_reserved_id(&json!(id)));
    }

    #[test]
    fn client_request_with_reserved_id_is_flagged() {
        assert!(client_id_is_reserved(
            &json!({"id": "assay-establish-x", "method": "tools/call"})
        ));
        assert!(!client_id_is_reserved(
            &json!({"id": 7, "method": "tools/call"})
        ));
        assert!(!client_id_is_reserved(
            &json!({"id": "client-7", "method": "tools/call"})
        ));
        // a notification (no id) is never reserved
        assert!(!client_id_is_reserved(
            &json!({"method": "notifications/initialized"})
        ));
    }

    // --- routing decision ---

    #[test]
    fn non_reserved_response_relays_to_client() {
        let v = json!({"id": 12, "result": {"tools": []}});
        assert_eq!(route_upstream(&v, |_| true), UpstreamRoute::RelayToClient);
    }

    #[test]
    fn notification_without_id_relays_to_client() {
        let v = json!({"method": "notifications/tools/list_changed"});
        assert_eq!(route_upstream(&v, |_| true), UpstreamRoute::RelayToClient);
    }

    #[test]
    fn reserved_response_diverts_only_when_pending() {
        let v = json!({"id": "assay-establish-abc", "result": {"tools": []}});
        // pending -> divert (suppressed from client)
        assert_eq!(
            route_upstream(&v, |id| id == "assay-establish-abc"),
            UpstreamRoute::DivertToEstablish("assay-establish-abc".to_string())
        );
        // reserved but NOT pending -> relay, never silently swallow
        assert_eq!(route_upstream(&v, |_| false), UpstreamRoute::RelayToClient);
    }

    // --- registry + timeout ---

    #[tokio::test]
    async fn register_then_resolve_delivers_value_and_suppresses() {
        let reg = EstablishRegistry::default();
        let id = "assay-establish-1".to_string();
        let rx = reg.register(id.clone());
        assert!(reg.is_pending(&id));
        let resolved = reg.resolve(&id, json!({"result": {"tools": []}}));
        assert!(
            resolved,
            "a pending id resolves (line suppressed from client)"
        );
        assert!(!reg.is_pending(&id), "entry removed after resolve");
        let got = await_establish(rx, Duration::from_secs(1)).await;
        assert_eq!(got, Some(json!({"result": {"tools": []}})));
    }

    #[tokio::test]
    async fn resolving_unknown_id_returns_false_so_reader_relays() {
        let reg = EstablishRegistry::default();
        assert!(!reg.resolve("assay-establish-missing", json!({})));
    }

    #[tokio::test]
    async fn establish_times_out_fail_closed_when_never_resolved() {
        let reg = EstablishRegistry::default();
        let rx = reg.register("assay-establish-timeout".to_string());
        // never resolved -> the per-operation timeout elapses -> None (caller fails closed)
        let got = await_establish(rx, Duration::from_millis(20)).await;
        assert_eq!(got, None);
    }
}
