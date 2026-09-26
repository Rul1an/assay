#![cfg(test)]
//! Row-attribution tests for latest-stored-episode `used` marks (#3140).

use crate::agent_assertions::model::TraceAssertion;
use crate::cache::vcr::VcrCache;
use crate::engine::runner::{RunPolicy, Runner};
use crate::model::{Expected, LlmResponse, TestCase, TestInput, TestResultRow, TestStatus};
use crate::providers::llm::fake::FakeClient;
use crate::quarantine::QuarantineMode;
use crate::storage::store::Store;
use std::sync::Arc;

use super::assertions::apply_agent_assertions_impl;

fn memory_runner() -> Runner {
    let store = Store::memory().expect("in-memory store");
    store.init_schema().expect("schema init");
    Runner {
        store: store.clone(),
        cache: VcrCache::new(store),
        client: Arc::new(FakeClient::new("fake-model".to_string()).with_response("ok".into())),
        metrics: vec![],
        policy: RunPolicy {
            rerun_failures: 0,
            quarantine_mode: QuarantineMode::Off,
            replay_strict: false,
        },
        _network_guard: None,
        embedder: None,
        refresh_embeddings: false,
        incremental: false,
        refresh_cache: false,
        judge: None,
        baseline: None,
    }
}

fn unit_blocklist_case(id: &str) -> TestCase {
    TestCase {
        id: id.to_string(),
        input: TestInput {
            prompt: "n/a".into(),
            context: None,
        },
        expected: Expected::MustContain {
            must_contain: vec!["ok".into()],
        },
        assertions: Some(vec![TraceAssertion::ToolBlocklist {
            test_tool_calls: Some(vec![]),
            policy: Some(serde_json::json!({})),
            expect: None,
        }]),
        on_error: None,
        tags: vec![],
        metadata: None,
    }
}

fn empty_row(id: &str) -> TestResultRow {
    TestResultRow {
        test_id: id.to_string(),
        status: TestStatus::Pass,
        score: None,
        cached: false,
        message: "ok".into(),
        details: serde_json::json!({}),
        duration_ms: None,
        fingerprint: None,
        skip_reason: None,
        attempts: None,
        error_policy_applied: None,
    }
}

fn resp() -> LlmResponse {
    LlmResponse {
        text: "ok".into(),
        provider: "fake".into(),
        model: "fake".into(),
        cached: false,
        meta: serde_json::json!({}),
    }
}

#[test]
fn keyed_take_keeps_distinct_ids_apart() {
    let store = Store::memory().unwrap();
    store.init_schema().unwrap();
    store.mark_latest_stored_episode_used("alpha").unwrap();
    store.mark_latest_stored_episode_used("beta").unwrap();
    let first = store.take_latest_stored_episode_used_for("beta").unwrap();
    let second = store.take_latest_stored_episode_used_for("alpha").unwrap();
    assert_eq!(first, vec!["beta".to_string()]);
    assert_eq!(second, vec!["alpha".to_string()]);
    assert!(store.take_latest_stored_episode_used().unwrap().is_empty());
}

#[test]
fn apply_does_not_attach_foreign_mark() {
    let runner = memory_runner();
    runner
        .store
        .mark_latest_stored_episode_used("alpha")
        .unwrap();

    let tc = unit_blocklist_case("beta");
    let mut row = empty_row("beta");
    apply_agent_assertions_impl(&runner, 1, &tc, &resp(), &mut row).unwrap();

    let episode = row.details.get("assertion_episode").cloned();
    assert!(
        episode.is_none(),
        "foreign used mark leaked onto beta row: {episode:?}"
    );
    assert_eq!(
        runner.store.take_latest_stored_episode_used().unwrap(),
        vec!["alpha".to_string()]
    );
}

#[test]
fn apply_attaches_own_mark_only() {
    let runner = memory_runner();
    runner
        .store
        .mark_latest_stored_episode_used("alpha")
        .unwrap();
    runner
        .store
        .mark_latest_stored_episode_used("beta")
        .unwrap();

    let tc = unit_blocklist_case("beta");
    let mut row = empty_row("beta");
    apply_agent_assertions_impl(&runner, 1, &tc, &resp(), &mut row).unwrap();

    let ids = row.details["assertion_episode"]["test_ids"]
        .as_array()
        .expect("assertion_episode.test_ids");
    let ids: Vec<&str> = ids.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(ids, vec!["beta"]);
    assert_eq!(
        runner.store.take_latest_stored_episode_used().unwrap(),
        vec!["alpha".to_string()]
    );
}

#[test]
fn public_zero_arg_take_still_drains_all() {
    let store = Store::memory().unwrap();
    store.init_schema().unwrap();
    store.mark_latest_stored_episode_used("alpha").unwrap();
    store.mark_latest_stored_episode_used("beta").unwrap();
    let all = store.take_latest_stored_episode_used().unwrap();
    assert_eq!(all, vec!["alpha".to_string(), "beta".to_string()]);
    assert!(store.take_latest_stored_episode_used().unwrap().is_empty());
}
