//! What this client asked for and received of the server's tool definitions.
//!
//! A definition counts only when it answers a `tools/list` this client sent. Anything that leaves
//! what the client holds unknown drops every observation, so a pinned tool is refused until the
//! client lists again: "no drift observed" is never "no drift".

use super::tools::{observe_tool_definition, tool_name};
use crate::mcp::era::UniqueValue;
use crate::mcp::identity::ToolIdentity;
use crate::mcp::policy::ToolObservation;
use crate::mcp::tool_definition::ToolDefinitionBinding;
use serde_json::Value;
use std::collections::HashMap;

const TOOLS_LIST_CHANGED: &str = "notifications/tools/list_changed";
/// A client that abandons list requests must not grow this without end. Forgetting them makes a
/// late reply unsolicited, which refuses a pinned tool until the next list and allows nothing.
const MAX_OUTSTANDING_LISTS: usize = 1024;

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

/// Which list an unanswered request belongs to, by the position lists were sent in.
#[derive(Debug, Clone, Copy)]
enum ListRequest {
    Start {
        sent: u64,
    },
    PageOf {
        start: u64,
    },
    /// A page whose cursor is not the one the current list gave.
    UntiedPage,
}

#[derive(Debug, Default)]
pub(super) struct ToolObservations {
    /// Unanswered `tools/list` requests by request id.
    outstanding_lists: HashMap<String, ListRequest>,
    lists_sent: u64,
    /// The position of the start that `tools` holds.
    start_applied: u64,
    /// The cursor the last reply of that list gave, which is the only page that can continue it.
    next_cursor: Option<String>,
    tools: HashMap<String, ObservedTool>,
}

impl ToolObservations {
    /// Called before the request is forwarded, so its reply cannot arrive first.
    pub(super) fn client_sent_list(&mut self, id: &Value, params: &Value) {
        if self.outstanding_lists.len() >= MAX_OUTSTANDING_LISTS {
            self.outstanding_lists.clear();
        }
        self.lists_sent += 1;
        // Only a non-empty string can continue a list: a server testing `if (!cursor)` starts over
        // on anything else, so this has to read it as starting over too.
        let cursor = params
            .get("cursor")
            .and_then(Value::as_str)
            .filter(|cursor| !cursor.is_empty());
        let request = if cursor.is_none() {
            ListRequest::Start {
                sent: self.lists_sent,
            }
        } else if self.next_cursor.as_deref() == cursor {
            ListRequest::PageOf {
                start: self.start_applied,
            }
        } else {
            ListRequest::UntiedPage
        };
        self.outstanding_lists.insert(id.to_string(), request);
    }

    /// What the record and the pin check may say about this tool.
    pub(super) fn lookup(
        &self,
        tool_name: &str,
    ) -> (ToolObservation, Option<ToolDefinitionBinding>) {
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
    pub(super) fn server_line(&mut self, line: &str, server_id: &str) -> String {
        if line.trim().is_empty() {
            return line.to_string();
        }
        // The only line believed is one this parser reads exactly as a client must. Parsers differ
        // on more than can be listed (JSON.parse takes an out-of-range number and deep nesting that
        // this one refuses, `trim()` in JavaScript removes a byte order mark that Rust's keeps, a
        // universal-newline reader splits on a bare CR), and MCP allows nothing else on stdout.
        // The client-frame byte ceiling does not apply: a large reply is an ordinary one.
        let frame = if line.trim_end().contains('\r') {
            None
        } else {
            serde_json::from_str::<UniqueValue>(line).ok()
        };
        let Some(UniqueValue(mut frame)) = frame.filter(|frame| frame.0.is_object()) else {
            self.invalidate();
            return line.to_string();
        };

        let has_error = frame.get("error").is_some();
        let has_result = frame.get("result").is_some();
        match frame.get("method") {
            None => {}
            Some(Value::String(method)) if !has_error && !has_result => {
                if method == TOOLS_LIST_CHANGED {
                    self.invalidate();
                }
                return line.to_string();
            }
            // Neither plainly a request nor plainly a reply.
            Some(_) => {
                self.invalidate();
                return line.to_string();
            }
        }

        let id = frame.get("id").filter(|id| !id.is_null());
        let answered = id.and_then(|id| self.outstanding_lists.remove(&id.to_string()));
        let Some(answered) = answered else {
            if self.could_answer_a_list(&frame) {
                self.invalidate();
            }
            return line.to_string();
        };

        if has_error && !has_result {
            // The client received no definition, so it still holds what it had.
            return line.to_string();
        }
        // What a client keeps of lists answered out of order depends on how it tracks them.
        let current = match answered {
            ListRequest::Start { sent } => sent >= self.start_applied,
            ListRequest::PageOf { start } => start == self.start_applied,
            ListRequest::UntiedPage => false,
        };
        if !current {
            self.invalidate();
            return line.to_string();
        }
        let next_cursor_is_valid = frame
            .pointer("/result/nextCursor")
            .is_none_or(|cursor| cursor.is_string() || cursor.is_null());
        let next_cursor = frame
            .pointer("/result/nextCursor")
            .and_then(Value::as_str)
            .map(str::to_string);
        let tools = frame
            .pointer_mut("/result/tools")
            .and_then(Value::as_array_mut);
        let tools = tools.filter(|tools| {
            !has_error && next_cursor_is_valid && tools.iter().all(|t| tool_name(t).is_some())
        });
        let Some(tools) = tools else {
            self.invalidate();
            return line.to_string();
        };

        if let ListRequest::Start { sent } = answered {
            self.tools.clear();
            self.start_applied = sent;
        }
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

    /// A reply that is not the exact one asked for, yet that a client could take for it. Clients
    /// match ids in more ways than can be listed: JavaScript reads `2.0` as `2`, and one that
    /// coerces with `Number()` reads `"2"` and `true` too. So a set of tools under any other id
    /// counts, as does an id JSON-RPC does not allow, or one equal to an unanswered id as a number.
    /// A reply to a client's other request has none of these.
    fn could_answer_a_list(&self, frame: &Value) -> bool {
        if frame.pointer("/result/tools").is_some() {
            return true;
        }
        let id = frame.get("id");
        if id.is_some_and(|id| !(id.is_null() || id.is_string() || id.is_number())) {
            return true;
        }
        let Some(number) = id.and_then(id_as_number) else {
            return false;
        };
        self.outstanding_lists.keys().any(|key| {
            serde_json::from_str::<Value>(key)
                .ok()
                .as_ref()
                .and_then(id_as_number)
                == Some(number)
        })
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
        self.outstanding_lists.clear();
    }
}

fn id_as_number(id: &Value) -> Option<f64> {
    match id {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
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
        for id in 0..(MAX_OUTSTANDING_LISTS as u64 * 3) {
            observations.client_sent_list(&json!(id), &json!({}));
            assert!(observations.outstanding_lists.len() <= MAX_OUTSTANDING_LISTS);
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
            ToolObservation::Definition(_)
        ));
        assert!(observations.outstanding_lists.contains_key("2"));
    }

    #[test]
    fn an_id_equal_as_a_number_invalidates_without_any_tools() {
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
