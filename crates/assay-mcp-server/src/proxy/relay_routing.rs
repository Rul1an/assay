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

/// Reserved id namespace for proxy-originated requests. A high-entropy suffix is appended per request
/// (slice 2). A client request whose id is a string in this namespace is rejected, so a client id can
/// never be mistaken for — or swallow — an establish response.
pub const RESERVED_ID_PREFIX: &str = "assay-establish-";

/// True iff `id` is a JSON string in the reserved namespace. Numeric ids and non-prefixed string ids
/// (the only forms a normal client or upstream uses) are never reserved.
pub fn is_reserved_id(id: &Value) -> bool {
    id.as_str()
        .is_some_and(|s| s.starts_with(RESERVED_ID_PREFIX))
}

/// Mint a reserved request id from a high-entropy suffix (the caller supplies it; slice 2 uses a
/// CSPRNG). Pure/deterministic for testability.
#[allow(dead_code)] // registration side: wired by slice 2.
pub fn mint_reserved_id(entropy: &str) -> String {
    format!("{RESERVED_ID_PREFIX}{entropy}")
}

/// Pure id-only check: does this message carry a reserved id? Used by `is_reserved_client_request`.
pub fn client_id_is_reserved(v: &Value) -> bool {
    v.get("id").map(is_reserved_id).unwrap_or(false)
}

/// True iff `v` is a client REQUEST (has a `method`) carrying a reserved id. ONLY requests are
/// rejected: a client RESPONSE to an upstream-initiated request has no `method`, so it is never matched
/// even with a reserved id and still relays to the upstream via the normal response path. Rejecting
/// such requests prevents a malicious or unlucky client id from routing a real response into — and being
/// swallowed by — the establish registry.
pub fn is_reserved_client_request(v: &Value) -> bool {
    v.get("method").is_some() && client_id_is_reserved(v)
}

/// Where the single upstream reader should send a parsed upstream line.
#[derive(Debug, PartialEq, Eq)]
pub enum UpstreamRoute {
    /// Relay verbatim to the client (the default for all normal traffic — any non-reserved id, and
    /// notifications without an id).
    RelayToClient,
    /// A reserved id that is CURRENTLY PENDING: divert to the establish registry under this id and
    /// suppress it from the client stream.
    DivertToEstablish(String),
    /// A reserved id that is NOT (or no longer) pending. The reserved namespace is the proxy's own
    /// (client requests carrying a reserved id are rejected by the collision guard), so any reserved-id
    /// upstream response is proxy-originated: a late or duplicate establish reply, or an unprompted
    /// reserved id from the upstream. It must be suppressed from the client stream — relaying it would
    /// leak a synthetic, proxy-originated response to the client.
    SuppressReserved,
}

/// Pure routing decision. Reserved-id routing applies ONLY to RESPONSE objects (no `method`, with
/// `result` or `error`): a reserved-id response routes to `DivertToEstablish` when pending and
/// `SuppressReserved` otherwise — a reserved-id RESPONSE is never relayed to the client, because the
/// reserved namespace belongs to the proxy (client requests in it are rejected), so it is always
/// proxy-originated. An upstream-to-client REQUEST or notification (has a `method`) is relayed verbatim
/// even if its id is reserved — it is not a response to a proxy-originated establish request, so it must
/// not be diverted into a waiter or suppressed. Everything else relays.
pub fn route_upstream(v: &Value, is_pending: impl Fn(&str) -> bool) -> UpstreamRoute {
    let is_response =
        v.get("method").is_none() && (v.get("result").is_some() || v.get("error").is_some());
    if is_response {
        if let Some(id) = v.get("id") {
            if is_reserved_id(id) {
                let id = id.as_str().unwrap_or_default().to_string();
                if !id.is_empty() && is_pending(&id) {
                    return UpstreamRoute::DivertToEstablish(id);
                }
                return UpstreamRoute::SuppressReserved;
            }
        }
    }
    UpstreamRoute::RelayToClient
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
    /// Register a reserved id; returns a `PendingEstablish` guard (removes the id on drop) and the
    /// receiver the caller awaits (slice 2). Returns `None` if the id is ALREADY pending: the registry
    /// is one-id/one-waiter, so a duplicate is refused rather than clobbering the existing waiter — the
    /// caller treats `None` as a failed establish and fails closed.
    #[allow(dead_code)] // registration side: wired by slice 2.
    pub fn register(&self, id: String) -> Option<(PendingEstablish, oneshot::Receiver<Value>)> {
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().expect("establish registry mutex");
            if pending.contains_key(&id) {
                return None;
            }
            pending.insert(id.clone(), tx);
        }
        Some((
            PendingEstablish {
                registry: self.clone(),
                id,
            },
            rx,
        ))
    }

    /// Remove a pending id without delivering a value (idempotent). Used by `PendingEstablish::drop`, so
    /// a timed-out or abandoned establish can never leave a stale entry growing the map unbounded.
    pub fn cancel(&self, id: &str) {
        self.pending
            .lock()
            .expect("establish registry mutex")
            .remove(id);
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

/// RAII guard for an in-flight establish request: removes its id from the registry on drop, so a
/// timed-out or abandoned request can never leave the `pending` map growing unbounded. A successful
/// `resolve` already removed the id, which makes the drop a no-op.
#[allow(dead_code)] // registration side: wired by slice 2.
pub struct PendingEstablish {
    registry: EstablishRegistry,
    id: String,
}

impl Drop for PendingEstablish {
    fn drop(&mut self) {
        self.registry.cancel(&self.id);
    }
}

/// Await an establish response under one per-operation timeout. `None` on timeout or a dropped sender,
/// which the caller treats as a failed establish (fail-closed). The caller keeps the `PendingEstablish`
/// guard alive across this await and drops it afterward, so the registry entry is always reclaimed —
/// on success (via `resolve`) or on timeout/abandon (via the guard's drop). Wired by slice 2.
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
        let id = mint_reserved_id("unit-sample-id");
        assert!(id.starts_with(RESERVED_ID_PREFIX));
        assert!(is_reserved_id(&json!(id)));
    }

    #[test]
    fn reserved_client_request_matches_only_requests_not_responses() {
        // a REQUEST (has method) with a reserved id is rejected
        assert!(is_reserved_client_request(
            &json!({"id": "assay-establish-x", "method": "tools/call"})
        ));
        // a RESPONSE (no method) with a reserved id is NOT matched -> still relays via the response path
        assert!(!is_reserved_client_request(
            &json!({"id": "assay-establish-x", "result": {"ok": true}})
        ));
        // a request with a normal id is not matched
        assert!(!is_reserved_client_request(
            &json!({"id": "client-7", "method": "tools/call"})
        ));
        // a notification (no id) is never matched
        assert!(!is_reserved_client_request(
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
    fn reserved_response_diverts_when_pending_else_suppressed_never_relayed() {
        let v = json!({"id": "assay-establish-abc", "result": {"tools": []}});
        // pending -> divert to the establish caller (suppressed from client)
        assert_eq!(
            route_upstream(&v, |id| id == "assay-establish-abc"),
            UpstreamRoute::DivertToEstablish("assay-establish-abc".to_string())
        );
        // reserved but NOT pending -> SUPPRESSED, never relayed to the client (it is proxy-originated:
        // a late/duplicate establish reply or an unprompted reserved id). Relaying would leak it.
        assert_eq!(
            route_upstream(&v, |_| false),
            UpstreamRoute::SuppressReserved
        );
    }

    #[test]
    fn reserved_id_request_or_non_response_is_relayed_not_diverted() {
        // An upstream-to-client REQUEST (has `method`) carrying a reserved id is NOT a response to a
        // proxy-originated establish: it relays to the client even when the id is pending, and is never
        // diverted into a waiter or suppressed.
        let req = json!({"id": "assay-establish-1", "method": "ping"});
        assert_eq!(route_upstream(&req, |_| true), UpstreamRoute::RelayToClient);
        assert_eq!(
            route_upstream(&req, |_| false),
            UpstreamRoute::RelayToClient
        );
        // A reserved id with neither method nor result/error is not a well-formed response either.
        let bare = json!({"id": "assay-establish-1"});
        assert_eq!(
            route_upstream(&bare, |_| true),
            UpstreamRoute::RelayToClient
        );
        // An error response with a reserved id is still a response -> suppressed when not pending.
        let err = json!({"id": "assay-establish-1", "error": {"code": -1, "message": "x"}});
        assert_eq!(
            route_upstream(&err, |_| false),
            UpstreamRoute::SuppressReserved
        );
    }

    // --- registry + timeout ---

    #[tokio::test]
    async fn register_then_resolve_delivers_value_and_suppresses() {
        let reg = EstablishRegistry::default();
        let id = "assay-establish-1".to_string();
        let (guard, rx) = reg.register(id.clone()).expect("first register succeeds");
        assert!(reg.is_pending(&id));
        let resolved = reg.resolve(&id, json!({"result": {"tools": []}}));
        assert!(
            resolved,
            "a pending id resolves (line suppressed from client)"
        );
        assert!(!reg.is_pending(&id), "entry removed after resolve");
        let got = await_establish(rx, Duration::from_secs(1)).await;
        assert_eq!(got, Some(json!({"result": {"tools": []}})));
        drop(guard); // idempotent: entry already removed by resolve
        assert!(!reg.is_pending(&id));
    }

    #[tokio::test]
    async fn resolving_unknown_id_returns_false_so_reader_relays() {
        let reg = EstablishRegistry::default();
        assert!(!reg.resolve("assay-establish-missing", json!({})));
    }

    #[tokio::test]
    async fn establish_times_out_fail_closed_and_guard_reclaims_entry() {
        let reg = EstablishRegistry::default();
        let id = "assay-establish-timeout".to_string();
        let (guard, rx) = reg.register(id.clone()).expect("register succeeds");
        assert!(reg.is_pending(&id));
        // never resolved -> the per-operation timeout elapses -> None (caller fails closed)
        let got = await_establish(rx, Duration::from_millis(20)).await;
        assert_eq!(got, None);
        // the guard's drop reclaims the entry, so a non-responsive upstream cannot grow the map.
        drop(guard);
        assert!(
            !reg.is_pending(&id),
            "timed-out establish must not leave a stale pending entry"
        );
    }

    #[tokio::test]
    async fn duplicate_register_is_refused_and_does_not_clobber_the_first_waiter() {
        let reg = EstablishRegistry::default();
        let id = "assay-establish-dup".to_string();
        let (_guard, rx1) = reg.register(id.clone()).expect("first register succeeds");
        // a second register for the same id is refused (one-id/one-waiter), not a silent overwrite.
        assert!(reg.register(id.clone()).is_none());
        // the first waiter is intact: resolving delivers to it.
        assert!(reg.resolve(&id, json!({"result": {"tools": []}})));
        let got = await_establish(rx1, Duration::from_secs(1)).await;
        assert_eq!(got, Some(json!({"result": {"tools": []}})));
    }
}
