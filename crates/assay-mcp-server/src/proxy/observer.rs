use anyhow::{Context, Result};
use assay_mcp_server::manifest_observed::{self, Completeness};
use serde_json::{json, Value};
use std::collections::HashMap;
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
    list_req_ids: HashMap<String, bool>,
    acc: Vec<Value>,
    chain_started: bool,
    outstanding_cursor: bool,
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
    fn id_key(id: &Value) -> String {
        id.to_string()
    }

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
        self.best_incomplete = Some((self.acc.clone(), kind));
        if self.latest_complete.is_some() {
            self.later_incomplete_chain_observed = true;
        }
        self.op_active = false;
    }

    /// Tap a forwarded client tools/list request. `had_cursor` is whether `params.cursor` was present.
    pub(super) fn on_client_tools_list(&mut self, id: Option<&Value>, had_cursor: bool) {
        self.any_tools_list_observed = true;
        if !had_cursor {
            // A cursorless request starts a new operation; an unfinished prior chain becomes partial.
            if self.op_active && self.outstanding_cursor {
                self.close_active_as_incomplete();
            }
            self.acc.clear();
            self.chain_started = true;
            self.outstanding_cursor = false;
            self.op_active = true;
            self.observed_list_operations += 1;
        } else if !self.op_active {
            // A cursored request with no active operation: joined mid-stream, start unprovable.
            self.acc.clear();
            self.chain_started = false;
            self.outstanding_cursor = false;
            self.op_active = true;
            self.observed_list_operations += 1;
        }
        if let Some(id) = id {
            self.list_req_ids.insert(Self::id_key(id), had_cursor);
        }
    }

    /// Tap an upstream response. Returns true iff a chain just COMPLETED (so the caller may write the
    /// latest-complete manifest mid-run).
    pub(super) fn on_upstream_response(&mut self, v: &Value) -> bool {
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
        let id = match v.get("id") {
            Some(i) if !i.is_null() => Self::id_key(i),
            _ => return false,
        };
        if !self.list_req_ids.contains_key(&id) {
            return false;
        }
        if let Some(tools) = v.pointer("/result/tools").and_then(|t| t.as_array()) {
            self.acc.extend(tools.iter().cloned());
        }
        let has_next = v
            .pointer("/result/nextCursor")
            .map(|c| !c.is_null())
            .unwrap_or(false);
        if has_next {
            self.outstanding_cursor = true;
            return false;
        }
        // Terminal page.
        self.outstanding_cursor = false;
        self.op_active = false;
        if self.chain_started {
            self.latest_complete = Some(self.acc.clone());
            self.complete_superseded_by_change = false;
            true
        } else {
            self.best_incomplete = Some((self.acc.clone(), "unknown"));
            if self.latest_complete.is_some() {
                self.later_incomplete_chain_observed = true;
            }
            false
        }
    }

    pub(super) fn on_list_changed(&mut self) {
        self.tools_list_changed_observed = true;
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
        obs.on_client_tools_list(Some(&idv), false);
        let _ = obs.on_upstream_response(&json!({"id": id, "result": {"tools": tools}}));
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
        obs.on_client_tools_list(Some(&json!("assay-establish-1")), false);
        let completed =
            obs.on_upstream_response(&json!({"id": "assay-establish-1", "method": "ping"}));
        assert!(!completed);
        assert!(matches!(
            obs.observed_tool_digest("anything"),
            enforce::ObservedToolDigest::NoCompleteManifest
        ));
    }
}
