//! What this client asked for and received of the server's tool definitions.
//!
//! A definition counts only when it answers a `tools/list` this client sent. Anything that leaves
//! what the client holds unknown drops every observation, so a pinned tool is refused until the
//! client lists again: "no drift observed" is never "no drift".

use super::tools::observe_tool_definition;
use crate::mcp::identity::ToolIdentity;
use crate::mcp::policy::ToolObservation;
use crate::mcp::tool_definition::ToolDefinitionBinding;
#[cfg(test)]
use crate::mcp::unique_json::parse_server_frame;
use serde_json::Value;
use std::collections::HashMap;

const TOOLS_LIST_CHANGED: &str = "notifications/tools/list_changed";
#[derive(Debug, Clone, PartialEq, Eq)]
struct ObservedDefinition {
    identity: ToolIdentity,
    binding: Option<ToolDefinitionBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ObservedTool {
    Definition(Box<ObservedDefinition>),
    /// One name carried different definitions in what the client holds.
    Conflicting,
}

#[derive(Debug, Default)]
pub(super) struct ToolObservations {
    pending_list: Option<Value>,
    budget: crate::mcp::unique_json::ListByteBudget,
    /// The cursor the last reply of that list gave, which is the only page that can continue it.
    next_cursor: Option<String>,
    tools: HashMap<String, ObservedTool>,
}

impl ToolObservations {
    /// Called before the request is forwarded, so its reply cannot arrive first.
    pub(super) fn client_sent_list(&mut self, id: &Value, params: &Value) {
        let requested = params.get("cursor").and_then(Value::as_str);
        let continuation = requested.is_some()
            && requested == self.next_cursor.as_deref()
            && self.pending_list.is_none();
        let first_page = requested.is_none_or(str::is_empty);
        if !continuation {
            self.invalidate();
        }
        if first_page || continuation {
            self.pending_list = Some(id.clone());
        }
    }

    pub(super) fn cancel_list(&mut self, id: &Value) {
        if self.pending_list.as_ref() == Some(id) {
            self.invalidate();
        }
    }

    /// What the record and the pin check may say about this tool.
    pub(super) fn lookup(
        &self,
        tool_name: &str,
    ) -> (ToolObservation, Option<ToolDefinitionBinding>) {
        if self.next_cursor.is_some() || self.pending_list.is_some() {
            return (ToolObservation::Absent, None);
        }
        match self.tools.get(tool_name) {
            None => (ToolObservation::Absent, None),
            Some(ObservedTool::Conflicting) => (ToolObservation::Conflicting, None),
            Some(ObservedTool::Definition(definition)) => (
                ToolObservation::Definition(definition.identity.clone()),
                definition.binding.clone(),
            ),
        }
    }

    /// Take in one server line and return the bytes the client gets, which change only for a
    /// reply this client asked for.
    #[cfg(test)]
    pub(super) fn server_line(&mut self, line: &str, server_id: &str) -> String {
        if line.trim().is_empty() {
            return line.to_string();
        }
        let Ok(frame) = parse_server_frame(line) else {
            self.invalidate();
            return line.to_string();
        };

        self.accepted_server_frame(line, server_id, frame)
    }

    pub(super) fn accepted_server_frame(
        &mut self,
        line: &str,
        server_id: &str,
        mut frame: Value,
    ) -> String {
        if let Some(method) = frame.get("method") {
            if method == TOOLS_LIST_CHANGED {
                self.invalidate();
            }
            return line.to_string();
        }
        let id = frame.get("id").filter(|id| !id.is_null());
        if id.is_none() || id != self.pending_list.as_ref() {
            if crate::mcp::unique_json::ambiguous_list_reply(&frame, self.pending_list.as_ref()) {
                self.invalidate();
            }
            return line.to_string();
        }
        self.pending_list = None;
        if !self.budget.admit_page(line.len()) {
            self.invalidate();
            return line.to_string();
        }
        let Some((_, next_cursor)) = crate::mcp::unique_json::live_list_page(&frame) else {
            self.invalidate();
            return line.to_string();
        };
        let next_cursor = next_cursor.map(str::to_owned);
        let tools = frame
            .pointer_mut("/result/tools")
            .and_then(Value::as_array_mut)
            .expect("live_list_page validated the tools array");
        self.next_cursor = next_cursor;
        for tool in tools {
            if let Some(observation) = observe_tool_definition(tool, server_id) {
                self.record(
                    observation.name,
                    ObservedTool::Definition(Box::new(ObservedDefinition {
                        identity: observation.identity,
                        binding: observation.binding,
                    })),
                );
            }
        }
        serde_json::to_string(&frame).map_or_else(|_| line.to_string(), |json| json + "\n")
    }

    fn record(&mut self, name: String, observed: ObservedTool) {
        match self.tools.get_mut(&name) {
            Some(existing) if *existing != observed => *existing = ObservedTool::Conflicting,
            Some(_) => {}
            None => {
                self.tools.insert(name, observed);
            }
        }
    }

    pub(super) fn invalidate(&mut self) {
        self.next_cursor = None;
        self.tools.clear();
        self.pending_list = None;
        self.budget = Default::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn list_reply(id: Value, tools: Value) -> String {
        format!(
            "{}\n",
            json!({"jsonrpc":"2.0","id":id,"result":{"tools":tools}})
        )
    }

    fn deploy() -> Value {
        json!({"name":"deploy","description":"Deploy.","inputSchema":{"type":"object"}})
    }

    fn observed() -> ToolObservations {
        let mut observations = ToolObservations::default();
        observations.client_sent_list(&json!(1), &json!({}));
        observations.server_line(&list_reply(json!(1), json!([deploy()])), "s");
        assert!(matches!(
            observations.lookup("deploy").0,
            ToolObservation::Definition(_)
        ));
        observations
    }

    fn observed_with_next_page() -> ToolObservations {
        let mut observations = ToolObservations::default();
        observations.client_sent_list(&json!(1), &json!({}));
        let first_page = format!(
            "{}\n",
            json!({"jsonrpc":"2.0","id":1,"result":{"tools":[deploy()],"nextCursor":"p2"}})
        );
        observations.server_line(&first_page, "s");
        observations
    }

    #[test]
    fn a_page_must_carry_the_cursor_the_list_gave() {
        let mut observations = observed_with_next_page();
        observations.client_sent_list(&json!(2), &json!({"cursor": "other"}));
        observations.server_line(&list_reply(json!(2), json!([deploy()])), "s");
        assert!(matches!(
            observations.lookup("deploy").0,
            ToolObservation::Absent
        ));
    }

    #[test]
    fn empty_continuation_keeps_definitions_from_the_first_page() {
        let mut observations = ToolObservations::default();
        observations.client_sent_list(&json!(1), &json!({}));
        let page = json!({"jsonrpc":"2.0","id":1,"result":{"tools":[deploy()],"nextCursor":""}});
        observations.server_line(&page.to_string(), "s");
        assert!(matches!(
            observations.lookup("deploy").0,
            ToolObservation::Absent
        ));
        observations.client_sent_list(&json!(2), &json!({"cursor":""}));
        observations.server_line(&list_reply(json!(2), json!([])), "s");
        assert!(matches!(
            observations.lookup("deploy").0,
            ToolObservation::Definition(_)
        ));
    }

    #[test]
    fn empty_start_replaces_a_list_with_a_different_continuation() {
        let mut observations = observed_with_next_page();
        observations.client_sent_list(&json!(2), &json!({"cursor":""}));
        let mut second = deploy();
        second["name"] = json!("status");
        observations.server_line(&list_reply(json!(2), json!([second])), "s");
        assert!(matches!(
            observations.lookup("deploy").0,
            ToolObservation::Absent
        ));
        assert!(matches!(
            observations.lookup("status").0,
            ToolObservation::Definition(_)
        ));
    }

    /// A client may send an empty or null cursor on its first request. Both start a list.
    #[test]
    fn an_empty_cursor_starts_a_list_that_counts() {
        for cursor in [json!(""), json!(null), json!(7)] {
            let mut observations = ToolObservations::default();
            observations.client_sent_list(&json!(1), &json!({"cursor": cursor}));
            observations.server_line(&list_reply(json!(1), json!([deploy()])), "s");
            assert!(
                matches!(
                    observations.lookup("deploy").0,
                    ToolObservation::Definition(_)
                ),
                "{cursor}"
            );
        }
    }

    /// The last page gave no cursor, so nothing can continue that list.
    #[test]
    fn a_page_after_the_last_page_does_not_count() {
        let mut observations = observed();
        observations.client_sent_list(&json!(2), &json!({"cursor": "p2"}));
        observations.server_line(&list_reply(json!(2), json!([deploy()])), "s");
        assert!(matches!(
            observations.lookup("deploy").0,
            ToolObservation::Absent
        ));
    }

    #[test]
    fn abandoned_lists_stay_bounded_and_a_new_list_still_counts() {
        let mut observations = ToolObservations::default();
        for id in 0..3000 {
            observations.client_sent_list(&json!(id), &json!({}));
            assert_eq!(observations.pending_list, Some(json!(id)));
        }
        observations.client_sent_list(&json!("late"), &json!({}));
        observations.server_line(&list_reply(json!("late"), json!([deploy()])), "s");
        assert!(matches!(
            observations.lookup("deploy").0,
            ToolObservation::Definition(_)
        ));
    }

    /// The control beside the near-miss rule: replies to a client's other requests arrive while
    /// a list is unanswered, and must not cost it what it holds.
    #[test]
    fn a_reply_to_another_request_changes_nothing() {
        let mut observations = observed();
        observations.client_sent_list(&json!(2), &json!({}));
        for id in [json!(5), json!("a"), json!(2.5), Value::Null] {
            let line = format!(
                "{}\n",
                json!({"jsonrpc":"2.0","id":id,"result":{"content":[]}})
            );
            assert_eq!(observations.server_line(&line, "s"), line);
        }
        assert!(matches!(
            observations.lookup("deploy").0,
            ToolObservation::Absent
        ));
        assert_eq!(observations.pending_list, Some(json!(2)));
        observations.server_line(&list_reply(json!(2), json!([deploy()])), "s");
        assert!(matches!(
            observations.lookup("deploy").0,
            ToolObservation::Definition(_)
        ));
    }

    #[test]
    fn another_json_id_cannot_complete_a_pending_list() {
        for id in [json!(2.0), json!("2"), json!(" 2 "), json!("2e0")] {
            let mut observations = observed();
            observations.client_sent_list(&json!(2), &json!({}));
            let line = format!("{}\n", json!({"jsonrpc":"2.0","id":id,"result":{}}));
            observations.server_line(&line, "s");
            assert!(
                matches!(observations.lookup("deploy").0, ToolObservation::Absent),
                "{id}"
            );
        }
    }

    #[test]
    fn tools_with_no_id_invalidate_while_a_list_is_unanswered() {
        let mut observations = observed();
        observations.client_sent_list(&json!(2), &json!({}));
        let line = format!("{}\n", json!({"jsonrpc":"2.0","result":{"tools":[]}}));
        assert_eq!(observations.server_line(&line, "s"), line);
        assert!(matches!(
            observations.lookup("deploy").0,
            ToolObservation::Absent
        ));
    }

    #[test]
    fn anything_but_a_readable_frame_invalidates() {
        for line in [
            "{\"x\":1e400}\n",
            "[info] server started\n",
            "server log\n",
            "\u{feff}{}\n",
            "42\n",
            "\"text\"\n",
        ] {
            let mut observations = observed();
            assert_eq!(observations.server_line(line, "s"), line);
            assert!(
                matches!(observations.lookup("deploy").0, ToolObservation::Absent),
                "{line}"
            );
        }
    }

    /// The client still holds a cursor after an invalidation, and may keep using it.
    #[test]
    fn a_page_sent_after_an_invalidation_counts_for_nothing() {
        let mut observations = observed_with_next_page();
        observations.server_line("a log line\n", "s");
        observations.client_sent_list(&json!(2), &json!({"cursor": "p2"}));
        observations.server_line(&list_reply(json!(2), json!([deploy()])), "s");
        assert!(matches!(
            observations.lookup("deploy").0,
            ToolObservation::Absent
        ));
    }

    #[test]
    fn a_new_list_after_an_invalidation_counts() {
        let mut observations = observed();
        observations.server_line("a log line\n", "s");
        observations.client_sent_list(&json!(2), &json!({}));
        observations.server_line(&list_reply(json!(2), json!([deploy()])), "s");
        assert!(matches!(
            observations.lookup("deploy").0,
            ToolObservation::Definition(_)
        ));
    }

    #[test]
    fn a_blank_line_changes_nothing() {
        let mut observations = observed();
        assert_eq!(observations.server_line("  \r\n", "s"), "  \r\n");
        assert!(matches!(
            observations.lookup("deploy").0,
            ToolObservation::Definition(_)
        ));
    }

    #[test]
    fn a_new_list_replaces_the_old_one_and_a_null_cursor_starts_one() {
        let mut observations = observed();
        observations.client_sent_list(&json!(2), &json!({"cursor": null}));
        observations.server_line(&list_reply(json!(2), json!([])), "s");
        assert!(matches!(
            observations.lookup("deploy").0,
            ToolObservation::Absent
        ));
    }

    #[test]
    fn the_same_definition_on_two_pages_is_not_a_conflict() {
        let mut observations = observed_with_next_page();
        observations.client_sent_list(&json!(2), &json!({"cursor": "p2"}));
        observations.server_line(&list_reply(json!(2), json!([deploy()])), "s");
        assert!(matches!(
            observations.lookup("deploy").0,
            ToolObservation::Definition(_)
        ));
    }

    /// Neither digest covers annotations, so this difference cannot be seen here. A limit of the
    /// pin, stated so that closing it shows up as a change.
    #[test]
    fn side_fields_the_digests_do_not_cover_are_not_a_conflict() {
        let mut observations = observed_with_next_page();
        let mut annotated = deploy();
        annotated["annotations"] = json!({"title": "Another"});
        observations.client_sent_list(&json!(2), &json!({"cursor": "p2"}));
        observations.server_line(&list_reply(json!(2), json!([annotated])), "s");
        assert!(matches!(
            observations.lookup("deploy").0,
            ToolObservation::Definition(_)
        ));
    }

    #[test]
    fn a_malformed_next_cursor_invalidates() {
        let mut observations = observed();
        observations.client_sent_list(&json!(2), &json!({}));
        let line = format!(
            "{}\n",
            json!({"jsonrpc":"2.0","id":2,"result":{"tools":[deploy()],"nextCursor":7}})
        );
        observations.server_line(&line, "s");
        assert!(matches!(
            observations.lookup("deploy").0,
            ToolObservation::Absent
        ));
    }

    #[test]
    fn a_reply_with_no_tools_invalidates() {
        let mut observations = observed();
        observations.client_sent_list(&json!(2), &json!({}));
        observations.server_line("{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{}}\n", "s");
        assert!(matches!(
            observations.lookup("deploy").0,
            ToolObservation::Absent
        ));
    }
}
