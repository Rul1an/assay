//! A store fault that is not a missing episode must not be reported as one.
//!
//! The primary lookup and the latest-stored fallback are separate causes.
//! A database failure on the fallback must stay a database failure.

use assay_core::storage::Store;

#[test]
fn database_error_during_episode_lookup_is_not_labeled_missing() {
    let store = Store::memory().expect("memory store");
    store.init_schema().expect("schema");
    {
        let conn = store.conn.lock().expect("lock");
        conn.execute_batch("PRAGMA foreign_keys = OFF; DROP TABLE episodes;")
            .expect("drop episodes");
    }
    // `expect_err` needs `Debug` on `EpisodeGraph`, which the graph does not implement.
    // Match so a database failure is still the value under test.
    let err = match store.get_episode_graph(1, "suite-test") {
        Ok(_graph) => panic!("lookup must fail"),
        Err(err) => err,
    };
    assert_database_fault_not_missing(&err);
}

fn assert_database_fault_not_missing(err: &anyhow::Error) {
    let text = err.to_string();
    assert!(
        !text.contains("episode_missing"),
        "database failure was labeled episode_missing: {text}"
    );
    assert!(
        !text.contains("E_TRACE_EPISODE_MISSING"),
        "database failure was labeled as a missing episode: {text}"
    );
    assert!(
        !text.contains("meta.test_id"),
        "database failure inherited the missing-episode remedy: {text}"
    );
}

#[test]
fn latest_stored_fallback_does_not_relabel_a_database_error_as_missing() {
    let store = Store::memory().expect("memory store");
    store.init_schema().expect("schema");
    store.set_latest_stored_episode_eval(true).expect("opt in");
    {
        let conn = store.conn.lock().expect("lock");
        // Blob primary keys stay blobs under TEXT affinity. The primary lookup
        // asks for run_id 1 and misses this row, so evaluation enters the
        // latest-stored fallback. That fallback then reads `id` as text.
        conn.execute(
            "INSERT INTO episodes (id, run_id, test_id, timestamp) VALUES (X'00', NULL, 'suite-test', 1)",
            [],
        )
        .expect("insert blob episode id");
    }

    let err = match assay_core::agent_assertions::verify_assertions_with_meta(
        &store,
        1,
        "suite-test",
        &[
            assay_core::agent_assertions::model::TraceAssertion::TraceMustNotCallTool {
                tool: "delete_repository".into(),
            },
        ],
        &serde_json::Value::Null,
    ) {
        Ok(_outcome) => panic!("lookup must fail"),
        Err(err) => err,
    };
    assert_database_fault_not_missing(&err);
    let text = err.to_string();
    assert!(
        text.to_lowercase().contains("column") || text.to_lowercase().contains("blob"),
        "database failure must stay visible: {text}"
    );
}
