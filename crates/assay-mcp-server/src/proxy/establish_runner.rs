use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use super::{establish, io::forward_line, observer::Observer, relay_routing};

/// The detailed result of a proxy-originated establish run. The carrier (`assay.manifest_establish.v0`)
/// collapses every non-complete variant to `EstablishFailed` per #1659; the runner returns the specific
/// reason so the wiring can log/diagnose timeout vs partial vs transport vs upstream error without
/// expanding the coarse carrier contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EstablishRunOutcome {
    /// A terminal (no-`nextCursor`) page completed the chain.
    Complete,
    /// The total deadline elapsed before any page was received.
    TimedOut,
    /// At least one page arrived but the chain did not reach a terminal page within the deadline.
    Partial,
    /// Failed to write the synthetic request, or the response channel dropped.
    TransportError,
    /// The upstream answered a synthetic request with a JSON-RPC error.
    ErrorResponse,
    /// A reserved id was already pending (unreachable with the monotonic counter; fail-closed anyway).
    RegisterRefused,
}

impl EstablishRunOutcome {
    /// Map the detailed run result to the coarse carrier outcome. Only `Complete` is an establish
    /// success; everything else fails closed.
    pub(super) fn to_carrier(self) -> establish::EstablishOutcome {
        match self {
            EstablishRunOutcome::Complete => establish::EstablishOutcome::EstablishedComplete,
            _ => establish::EstablishOutcome::EstablishFailed,
        }
    }

    /// snake_case label for the `run_outcome` field of `assay.manifest_establish.v0`. Diagnostic only;
    /// never a verdict. The no-establish case (`not_performed`) is supplied by the caller, not here.
    pub(super) fn as_str(self) -> &'static str {
        match self {
            EstablishRunOutcome::Complete => "complete",
            EstablishRunOutcome::TimedOut => "timed_out",
            EstablishRunOutcome::Partial => "partial",
            EstablishRunOutcome::TransportError => "transport_error",
            EstablishRunOutcome::ErrorResponse => "error_response",
            EstablishRunOutcome::RegisterRefused => "register_refused",
        }
    }
}

/// Drive a proxy-originated, possibly paginated `tools/list` against the upstream child to establish a
/// current complete observation, under one total deadline across all pages (never a per-page timeout).
pub(super) async fn run_establish<W: AsyncWriteExt + Unpin>(
    child_stdin: &mut W,
    registry: &relay_routing::EstablishRegistry,
    observer: &Arc<Mutex<Observer>>,
    id_counter: &std::sync::atomic::AtomicU64,
    budget: std::time::Duration,
) -> EstablishRunOutcome {
    use std::sync::atomic::Ordering;
    let deadline = tokio::time::Instant::now() + budget;
    let mut cursor: Option<String> = None;
    let mut pages_received: u32 = 0;
    // A timeout before any page is TimedOut; after at least one page it is Partial.
    let timed_out = |pages: u32| {
        if pages > 0 {
            EstablishRunOutcome::Partial
        } else {
            EstablishRunOutcome::TimedOut
        }
    };
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return timed_out(pages_received);
        }
        let remaining = deadline - now;
        let n = id_counter.fetch_add(1, Ordering::Relaxed);
        let id = relay_routing::mint_reserved_id(&n.to_string());
        // Register with the REGISTRY first. No successful registration means no observer mutation.
        let (guard, rx) = match registry.register(id.clone()) {
            Some(pair) => pair,
            None => return EstablishRunOutcome::RegisterRefused,
        };
        observer
            .lock()
            .await
            .on_client_tools_list(Some(&Value::String(id.clone())), cursor.is_some());
        let mut req = serde_json::Map::new();
        req.insert("jsonrpc".to_string(), Value::String("2.0".to_string()));
        req.insert("id".to_string(), Value::String(id.clone()));
        req.insert(
            "method".to_string(),
            Value::String("tools/list".to_string()),
        );
        if let Some(c) = &cursor {
            req.insert("params".to_string(), json!({ "cursor": c }));
        }
        let line = Value::Object(req).to_string();
        match tokio::time::timeout(remaining, forward_line(child_stdin, &line)).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => return EstablishRunOutcome::TransportError,
            Err(_) => return timed_out(pages_received),
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return timed_out(pages_received);
        }
        let resp = relay_routing::await_establish(rx, deadline - now).await;
        drop(guard); // reclaim the pending entry on every outcome (success, timeout, or error)
        let resp = match resp {
            Some(v) => v,
            None => return timed_out(pages_received),
        };
        if resp.get("error").is_some() {
            return EstablishRunOutcome::ErrorResponse;
        }
        pages_received += 1;
        // Completion MUST match the Observer's rule: any non-null nextCursor means more pages.
        let next = resp.pointer("/result/nextCursor");
        if next.map(|c| !c.is_null()).unwrap_or(false) {
            match next.and_then(|c| c.as_str()) {
                Some(s) if !s.is_empty() => cursor = Some(s.to_string()),
                _ => return EstablishRunOutcome::ErrorResponse,
            }
        } else {
            return EstablishRunOutcome::Complete;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::AtomicU64;
    use tokio::io::{AsyncBufReadExt, BufReader};

    fn tool(name: &str) -> Value {
        json!({"name": name, "description": "d", "inputSchema": {"type": "object"}})
    }

    #[tokio::test]
    async fn run_establish_paginates_to_complete_and_updates_observer() {
        let observer = Arc::new(Mutex::new(Observer::default()));
        let registry = relay_routing::EstablishRegistry::default();
        let counter = AtomicU64::new(1);
        let (mut wr, rd) = tokio::io::duplex(8192);

        let fake_reader = {
            let observer = observer.clone();
            let registry = registry.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(rd).lines();
                let id1 = req_id(&lines.next_line().await.unwrap().unwrap());
                let page1 = json!({
                    "id": id1,
                    "result": {
                        "tools": [tool("github.create_deploy_key")],
                        "nextCursor": "cursor-2"
                    }
                });
                observer.lock().await.on_upstream_response(&page1);
                assert!(registry.resolve(&id1, page1));
                let id2 = req_id(&lines.next_line().await.unwrap().unwrap());
                let page2 = json!({"id": id2, "result": {"tools": [tool("github.list_repos")]}});
                observer.lock().await.on_upstream_response(&page2);
                assert!(registry.resolve(&id2, page2));
            })
        };

        let outcome = run_establish(
            &mut wr,
            &registry,
            &observer,
            &counter,
            std::time::Duration::from_secs(2),
        )
        .await;
        fake_reader.await.unwrap();

        assert_eq!(outcome, EstablishRunOutcome::Complete);
        assert_eq!(
            outcome.to_carrier(),
            establish::EstablishOutcome::EstablishedComplete
        );
        assert!(matches!(
            observer
                .lock()
                .await
                .observed_tool_digest("github.create_deploy_key"),
            super::super::enforce::ObservedToolDigest::Present(_)
        ));
        assert!(!registry.is_pending("assay-establish-1"));
        assert!(!registry.is_pending("assay-establish-2"));
    }

    #[tokio::test]
    async fn run_establish_times_out_fail_closed_and_leaves_no_pending() {
        let observer = Arc::new(Mutex::new(Observer::default()));
        let registry = relay_routing::EstablishRegistry::default();
        let counter = AtomicU64::new(1);
        let (mut wr, _rd) = tokio::io::duplex(8192);

        let outcome = run_establish(
            &mut wr,
            &registry,
            &observer,
            &counter,
            std::time::Duration::from_millis(20),
        )
        .await;

        assert_eq!(outcome, EstablishRunOutcome::TimedOut);
        assert_eq!(
            outcome.to_carrier(),
            establish::EstablishOutcome::EstablishFailed
        );
        assert!(!registry.is_pending("assay-establish-1"));
    }

    #[tokio::test]
    async fn run_establish_register_refused_does_not_mutate_observer() {
        let observer = Arc::new(Mutex::new(Observer::default()));
        let registry = relay_routing::EstablishRegistry::default();
        let counter = AtomicU64::new(1);
        let (_held, _rx) = registry
            .register("assay-establish-1".to_string())
            .expect("pre-register the colliding id");
        let (mut wr, _rd) = tokio::io::duplex(8192);

        let outcome = run_establish(
            &mut wr,
            &registry,
            &observer,
            &counter,
            std::time::Duration::from_secs(2),
        )
        .await;

        assert_eq!(outcome, EstablishRunOutcome::RegisterRefused);
        assert_eq!(observer.lock().await.observed_list_operations(), 0);
        assert!(registry.is_pending("assay-establish-1"));
    }

    #[tokio::test]
    async fn run_establish_unusable_nextcursor_never_completes() {
        let observer = Arc::new(Mutex::new(Observer::default()));
        let registry = relay_routing::EstablishRegistry::default();
        let counter = AtomicU64::new(1);
        let (mut wr, rd) = tokio::io::duplex(8192);

        let fake_reader = {
            let observer = observer.clone();
            let registry = registry.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(rd).lines();
                let id1 = req_id(&lines.next_line().await.unwrap().unwrap());
                let page = json!({"id": id1, "result": {"tools": [tool("x")], "nextCursor": 7}});
                observer.lock().await.on_upstream_response(&page);
                registry.resolve(&id1, page);
            })
        };

        let outcome = run_establish(
            &mut wr,
            &registry,
            &observer,
            &counter,
            std::time::Duration::from_secs(2),
        )
        .await;
        fake_reader.await.unwrap();

        assert_eq!(outcome, EstablishRunOutcome::ErrorResponse);
        assert!(matches!(
            observer.lock().await.observed_tool_digest("x"),
            super::super::enforce::ObservedToolDigest::NoCompleteManifest
        ));
    }

    #[tokio::test]
    async fn run_establish_write_is_bounded_by_total_deadline() {
        let observer = Arc::new(Mutex::new(Observer::default()));
        let registry = relay_routing::EstablishRegistry::default();
        let counter = AtomicU64::new(1);
        let (mut wr, _rd) = tokio::io::duplex(1);

        let outcome = run_establish(
            &mut wr,
            &registry,
            &observer,
            &counter,
            std::time::Duration::from_millis(20),
        )
        .await;

        assert_eq!(outcome, EstablishRunOutcome::TimedOut);
        assert!(!registry.is_pending("assay-establish-1"));
    }

    fn req_id(line: &str) -> String {
        serde_json::from_str::<Value>(line).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    }
}
