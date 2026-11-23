use anyhow::{Context, Result};
use assay_mcp_server::manifest_observed::{self, Completeness};
use serde_json::{json, Value};
use std::path::PathBuf;

use super::{enforce, io::write_json_atomic};

const HEALTH_SCHEMA: &str = "assay.proxy_observation_health.v0";

// ---------------------------------------------------------------------------------------------------
// Observer: a read-only model of the upstream tool-surface observation. One active tools/list chain
// per session in v0; a new cursorless tools/list starts a new operation, leaving any unfinished chain
// partial. complete requires a start (no-cursor request) through a terminal page (no nextCursor); a
// chain whose start was never observed is unknown; an unfinished started chain is partial.
// ---------------------------------------------------------------------------------------------------

#[derive(Default)]
pub(super) struct Observer {
    server_id: Option<String>,
    /// tools/list request ids seen (so a response can be recognized as a list response).
    list_req_id: Option<Value>,
    budget: assay_core::mcp::unique_json::ListByteBudget,
    client_list_seen: bool,
    acc: Vec<Value>,
    chain_started: bool,
    next_cursor: Option<String>,
    op_active: bool,
    latest_complete: Option<Vec<Value>>,
    /// True once a `tools/list_changed` is observed after `latest_complete` was set, until a fresh
    /// complete chain replaces it. The drift gate must not authorize against a stale manifest.
    complete_superseded_by_change: bool,
    /// Most recent non-complete accumulation and its kind ("partial" | "unknown").
    best_incomplete: Option<(Vec<Value>, &'static str)>,
    observed_list_operations: u64,
    tools_list_changed_observed: bool,
    later_incomplete_chain_observed: bool,
    any_tools_list_observed: bool,
}

pub(super) struct Emission {
    server_id: String,
    tools: Option<Vec<Value>>,
    completeness: Option<Completeness>,
    source: &'static str,
}

impl Observer {
    /// An active operation ended without a terminal page (superseded, or shutdown): record it as the
    /// best non-complete observation, and note honestly if it came after a complete chain.
    fn close_active_as_incomplete(&mut self) {
        if !self.op_active {
            return;
        }
        let kind = if self.chain_started {
            "partial"
        } else {
            "unknown"
        };
        self.best_incomplete = Some((std::mem::take(&mut self.acc), kind));
        if self.latest_complete.is_some() {
            self.later_incomplete_chain_observed = true;
        }
        self.op_active = false;
    }

    pub(super) fn latest_page_complete(&self) -> bool {
        self.latest_complete.is_some() && !self.complete_superseded_by_change
    }

    pub(super) fn can_establish_privately(&self) -> bool {
        !self.client_list_seen
    }

    pub(super) fn on_client_tools_list(&mut self, id: Option<&Value>, cursor: Option<&Value>) {
        self.client_list_seen = true;
        self.start_page(id, cursor);
    }

    pub(super) fn on_private_tools_list(&mut self, id: &Value, cursor: Option<&Value>) {
        if self.can_establish_privately() {
            self.start_page(Some(id), cursor);
        }
    }

    fn start_page(&mut self, id: Option<&Value>, cursor: Option<&Value>) {
        self.any_tools_list_observed = true;
        self.complete_superseded_by_change = true;
        let continued = self.op_active
            && self.list_req_id.is_none()
            && self
                .next_cursor
                .as_deref()
                .is_some_and(|expected| cursor.and_then(Value::as_str) == Some(expected));
        if !continued {
            self.close_active_as_incomplete();
            self.acc.clear();
            self.budget = Default::default();
            self.chain_started = cursor.is_none() && self.list_req_id.is_none();
            self.op_active = true;
            self.observed_list_operations += 1;
        }
        self.next_cursor = None;
        self.list_req_id = id.filter(|id| id.is_string() || id.is_number()).cloned();
    }

    pub(super) fn on_cancelled(&mut self, id: &Value) {
        if self.list_req_id.as_ref() == Some(id) {
            self.invalidate_chain();
        }
    }

    pub(super) fn invalidate_chain(&mut self) {
        self.close_active_as_incomplete();
        self.complete_superseded_by_change = true;
        self.list_req_id = None;
        self.next_cursor = None;
    }

    /// Tap an upstream response. Returns true iff a chain just COMPLETED (so the caller may write the
    /// latest-complete manifest mid-run).
    #[cfg(test)]
    pub(super) fn on_upstream_response(&mut self, v: &Value) -> bool {
        self.observe_server_frame(v, v.to_string().len())
    }

    pub(super) fn observe_server_frame(&mut self, v: &Value, wire_bytes: usize) -> bool {
        // A message carrying a `method` is a request or notification, never a response to a tools/list
        // request — never fold it into the accumulation, even if its id matches a list request id.
        if v.get("method").is_some() {
            return false;
        }
        if self.server_id.is_none() {
            if let Some(name) = v
                .pointer("/result/serverInfo/name")
                .and_then(|n| n.as_str())
            {
                self.server_id = Some(name.to_string());
            }
        }
        let id = v.get("id").filter(|id| !id.is_null());
        if id.is_none() || id != self.list_req_id.as_ref() {
            if assay_core::mcp::unique_json::ambiguous_list_reply(v, self.list_req_id.as_ref()) {
                // A relayed list-shaped reply belongs to no current list. It
                // cannot establish what a client chose to retain.
                self.client_list_seen = true;
                self.invalidate_chain();
            }
            return false;
        }
        self.list_req_id = None;
        if !self.budget.admit_page(wire_bytes) {
            self.invalidate_chain();
            return false;
        }
        let Some((tools, cursor)) = assay_core::mcp::unique_json::live_list_page(v) else {
            return false;
        };
        self.acc.extend_from_slice(tools);
        if let Some(cursor) = cursor {
            self.next_cursor = Some(cursor.to_owned());
            return false;
        }
        // Terminal page.
        self.next_cursor = None;
        self.op_active = false;
        if self.chain_started {
            self.latest_complete = Some(std::mem::take(&mut self.acc));
            self.complete_superseded_by_change = false;
            true
        } else {
            self.best_incomplete = Some((std::mem::take(&mut self.acc), "unknown"));
            if self.latest_complete.is_some() {
                self.later_incomplete_chain_observed = true;
            }
            false
        }
    }

    pub(super) fn on_list_changed(&mut self) {
        self.tools_list_changed_observed = true;
        self.invalidate_chain();
        // Invalidate the current complete observation for the enforcing drift gate.
        self.complete_superseded_by_change = true;
    }

    pub(super) fn latest_complete_snapshot(&self) -> Option<(String, Vec<Value>)> {
        self.latest_complete.as_ref().map(|tools| {
            (
                self.server_id
                    .clone()
                    .unwrap_or_else(|| "upstream".to_string()),
                tools.clone(),
            )
        })
    }

    /// The current observed per-tool `tool_digest` for `tool_name`, from the latest COMPLETE observed
    /// manifest. Every non-`Present` outcome is fail-closed.
    pub(super) fn observed_tool_digest(&self, tool_name: &str) -> enforce::ObservedToolDigest {
        let tools = match &self.latest_complete {
            Some(t) if !self.complete_superseded_by_change => t,
            _ => return enforce::ObservedToolDigest::NoCompleteManifest,
        };
        let server = self
            .server_id
            .clone()
            .unwrap_or_else(|| "upstream".to_string());
        let manifest = manifest_observed::build_observed(&server, tools, Completeness::Complete);
        if manifest["status"].as_str() != Some("observed") {
            return enforce::ObservedToolDigest::Ambiguous;
        }
        if let Some(arr) = manifest["observed"]["tool_digests"].as_array() {
            for e in arr {
                if e.get("name").and_then(|n| n.as_str()) == Some(tool_name) {
                    if let Some(d) = e.get("tool_digest").and_then(|d| d.as_str()) {
                        return enforce::ObservedToolDigest::Present(d.to_string());
                    }
                }
            }
        }
        enforce::ObservedToolDigest::CompleteButToolAbsent
    }

    /// The effective per-tool observation for `tool_name`, from the same current complete manifest the
    /// drift gate reads via `observed_tool_digest`.
    pub(super) fn effective_tool_observation(
        &self,
        tool_name: &str,
    ) -> (enforce::ObservedToolDigest, Option<Value>) {
        let digest = self.observed_tool_digest(tool_name);
        let annotations = if matches!(digest, enforce::ObservedToolDigest::Present(_)) {
            self.latest_complete.as_ref().and_then(|tools| {
                tools
                    .iter()
                    .find(|t| t.get("name").and_then(|n| n.as_str()) == Some(tool_name))
                    .map(|t| t.get("annotations").cloned().unwrap_or(Value::Null))
            })
        } else {
            None
        };
        (digest, annotations)
    }

    /// Compute the final emission: latest complete wins; otherwise the best observed; otherwise
    /// not_observed. Closes any still-active chain as incomplete first.
    pub(super) fn finalize(&mut self) -> Emission {
        if self.op_active {
            self.close_active_as_incomplete();
        }
        let server_id = self
            .server_id
            .clone()
            .unwrap_or_else(|| "upstream".to_string());
        if !self.any_tools_list_observed {
            return Emission {
                server_id,
                tools: None,
                completeness: None,
                source: "not_observed",
            };
        }
        if let Some(tools) = &self.latest_complete {
            return Emission {
                server_id,
                tools: Some(tools.clone()),
                completeness: Some(Completeness::Complete),
                source: "latest_complete",
            };
        }
        if let Some((tools, kind)) = &self.best_incomplete {
            let (c, src) = if *kind == "partial" {
                (Completeness::Partial, "best_partial")
            } else {
                (Completeness::Unknown, "best_unknown")
            };
            return Emission {
                server_id,
                tools: Some(tools.clone()),
                completeness: Some(c),
                source: src,
            };
        }
        // tools/list seen but nothing accumulated/finalized: inconclusive, never clean.
        Emission {
            server_id,
            tools: Some(vec![]),
            completeness: Some(Completeness::Unknown),
            source: "best_unknown",
        }
    }

    #[cfg(test)]
    pub(super) fn observed_list_operations(&self) -> u64 {
        self.observed_list_operations
    }
}

pub(super) fn manifest_from(em: &Emission) -> Value {
    match (&em.tools, em.completeness) {
        (Some(tools), Some(c)) => manifest_observed::build_observed(&em.server_id, tools, c),
        _ => manifest_observed::not_observed(&em.server_id),
    }
}

pub(super) fn health_from(manifest: &Value, em: &Emission, obs: &Observer) -> Value {
    let status = manifest["status"].as_str().unwrap_or("not_observed");
    let source = if status == "ambiguous" {
        "ambiguous"
    } else {
        em.source
    };
    json!({
        "schema": HEALTH_SCHEMA,
        "manifest_observation": {
            "tools_list_observed": manifest["observed"]["tools_list_observed"],
            "tools_list_complete": manifest["observed"]["tools_list_complete"],
            "status": status,
            "emitted_state_source": source,
            "observed_list_operations": obs.observed_list_operations,
            "tools_list_changed_observed": obs.tools_list_changed_observed,
            "later_incomplete_chain_observed": obs.later_incomplete_chain_observed
        },
        "non_claims": [
            "does not imply tool-call forwarding",
            "does not prove the upstream full tool set if observation was partial or unknown"
        ]
    })
}

/// Emit a not_observed manifest/health pair (upstream never produced a tools/list).
pub(super) fn emit_not_observed(
    manifest_out: Option<PathBuf>,
    health_out: Option<PathBuf>,
) -> Result<()> {
    if manifest_out.is_none() && health_out.is_none() {
        return Ok(());
    }
    let obs = Observer::default();
    let em = Emission {
        server_id: "upstream".to_string(),
        tools: None,
        completeness: None,
        source: "not_observed",
    };
    let manifest = manifest_from(&em);
    if let Some(path) = &manifest_out {
        write_json_atomic(&path.clone(), &manifest)
            .with_context(|| format!("writing manifest artifact to {}", path.display()))?;
    }
    if let Some(path) = &health_out {
        let health = health_from(&manifest, &em, &obs);
        write_json_atomic(path, &health).with_context(|| {
            format!("writing observation-health artifact to {}", path.display())
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(name: &str) -> Value {
        json!({"name": name, "description": "d", "inputSchema": {"type": "object"}})
    }

    /// Drive a single complete cursorless tools/list (request + terminal response) into the observer.
    fn observe_complete(obs: &mut Observer, id: i64, tools: Vec<Value>) {
        let idv = json!(id);
        obs.on_client_tools_list(Some(&idv), None);
        let _ = obs.on_upstream_response(&json!({"id": id, "result": {"tools": tools}}));
    }

    #[test]
    fn client_refresh_requires_an_answer_before_authorizing_again() {
        let mut obs = Observer::default();
        observe_complete(&mut obs, 1, vec![tool("echo")]);
        obs.on_client_tools_list(Some(&json!(2)), None);
        assert!(matches!(
            obs.observed_tool_digest("echo"),
            enforce::ObservedToolDigest::NoCompleteManifest
        ));
        assert!(!obs.can_establish_privately());
        assert!(obs.on_upstream_response(&json!({"id":2,"result":{"tools":[tool("echo")]}})));
        assert!(matches!(
            obs.observed_tool_digest("echo"),
            enforce::ObservedToolDigest::Present(_)
        ));
    }

    #[test]
    fn an_unmatched_list_page_requires_a_new_complete_client_list() {
        let mut obs = Observer::default();
        observe_complete(&mut obs, 1, vec![tool("echo")]);
        obs.on_upstream_response(&json!({"id":2,"result":{"tools":[]}}));
        assert!(!obs.latest_page_complete());
        assert!(!obs.can_establish_privately());
        observe_complete(&mut obs, 3, vec![tool("echo")]);
        assert!(obs.latest_page_complete());
    }

    #[test]
    fn observed_digest_none_when_no_complete_manifest() {
        let obs = Observer::default();
        assert!(matches!(
            obs.observed_tool_digest("github.add_deploy_key"),
            enforce::ObservedToolDigest::NoCompleteManifest
        ));
    }

    #[test]
    fn observed_digest_present_then_invalidated_by_list_changed_then_restored() {
        let mut obs = Observer::default();
        observe_complete(&mut obs, 2, vec![tool("github.add_deploy_key")]);
        assert!(matches!(
            obs.observed_tool_digest("github.add_deploy_key"),
            enforce::ObservedToolDigest::Present(_)
        ));

        obs.on_list_changed();
        assert!(matches!(
            obs.observed_tool_digest("github.add_deploy_key"),
            enforce::ObservedToolDigest::NoCompleteManifest
        ));

        observe_complete(&mut obs, 4, vec![tool("github.add_deploy_key")]);
        assert!(matches!(
            obs.observed_tool_digest("github.add_deploy_key"),
            enforce::ObservedToolDigest::Present(_)
        ));
    }

    #[test]
    fn observed_digest_ambiguous_on_duplicate_tool_names() {
        let mut obs = Observer::default();
        observe_complete(&mut obs, 2, vec![tool("dup"), tool("dup")]);
        assert!(matches!(
            obs.observed_tool_digest("dup"),
            enforce::ObservedToolDigest::Ambiguous
        ));
    }

    #[test]
    fn observed_digest_tool_absent_is_complete_but_tool_absent() {
        let mut obs = Observer::default();
        observe_complete(&mut obs, 2, vec![tool("search")]);
        assert!(matches!(
            obs.observed_tool_digest("github.add_deploy_key"),
            enforce::ObservedToolDigest::CompleteButToolAbsent
        ));
    }

    #[test]
    fn on_upstream_response_ignores_method_bearing_messages() {
        let mut obs = Observer::default();
        obs.on_client_tools_list(Some(&json!("assay-establish-1")), None);
        let completed =
            obs.on_upstream_response(&json!({"id": "assay-establish-1", "method": "ping"}));
        assert!(!completed);
        assert!(matches!(
            obs.observed_tool_digest("anything"),
            enforce::ObservedToolDigest::NoCompleteManifest
        ));
    }
}
