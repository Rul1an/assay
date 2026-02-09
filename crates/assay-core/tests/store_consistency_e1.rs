use assay_core::model::{EvalConfig, Expected, Settings, TestCase, TestInput};
use assay_core::storage::store::Store;
use rusqlite::params;

fn sample_cfg(suite: &str) -> EvalConfig {
    EvalConfig {
        version: 1,
        suite: suite.to_string(),
        model: "trace".to_string(),
        settings: Settings::default(),
        thresholds: Default::default(),
        otel: Default::default(),
        tests: vec![TestCase {
            id: "smoke".to_string(),
            input: TestInput {
                prompt: "hello".to_string(),
                context: None,
            },
            expected: Expected::MustContain {
                must_contain: vec!["ok".to_string()],
            },
            assertions: None,
            on_error: None,
            tags: vec![],
            metadata: None,
        }],
    }
}

#[test]
fn e1_runs_write_contract_insert_and_create() -> anyhow::Result<()> {
    let store = Store::memory()?;
    store.init_schema()?;

    let insert_id = store.insert_run("suite-insert")?;
    let create_id = store.create_run(&sample_cfg("suite-create"))?;

    let conn = store.conn.lock().unwrap();

    let (suite_i, status_i, started_i, cfg_i): (String, String, String, Option<String>) = conn
        .query_row(
            "SELECT suite, status, started_at, config_json FROM runs WHERE id = ?1",
            params![insert_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
    assert_eq!(suite_i, "suite-insert");
    assert_eq!(status_i, "running");
    assert!(cfg_i.is_none());
    let parsed_i = chrono::DateTime::parse_from_rfc3339(&started_i)?;
    assert_eq!(parsed_i.offset().local_minus_utc(), 0);

    let (suite_c, status_c, started_c, cfg_c): (String, String, String, Option<String>) = conn
        .query_row(
            "SELECT suite, status, started_at, config_json FROM runs WHERE id = ?1",
            params![create_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
    assert_eq!(suite_c, "suite-create");
    assert_eq!(status_c, "running");
    assert!(cfg_c.is_some());
    let parsed_c = chrono::DateTime::parse_from_rfc3339(&started_c)?;
    assert_eq!(parsed_c.offset().local_minus_utc(), 0);

    Ok(())
}

#[test]
fn e1_latest_run_selection_is_id_based_not_timestamp_string() -> anyhow::Result<()> {
    let store = Store::memory()?;
    store.init_schema()?;
    let conn = store.conn.lock().unwrap();

    conn.execute(
        "INSERT INTO runs(suite, started_at, status) VALUES (?1, ?2, ?3)",
        params!["suite-order", "9999-12-31T23:59:59.999Z", "running"],
    )?;
    let first_id = conn.last_insert_rowid();

    conn.execute(
        "INSERT INTO runs(suite, started_at, status) VALUES (?1, ?2, ?3)",
        params!["suite-order", "unix:1", "running"],
    )?;
    let second_id = conn.last_insert_rowid();
    assert!(second_id > first_id);
    drop(conn);

    let latest = store.get_latest_run_id("suite-order")?;
    assert_eq!(latest, Some(second_id));
    Ok(())
}

#[test]
fn e1_stats_read_compat_keeps_legacy_started_at() -> anyhow::Result<()> {
    let store = Store::memory()?;
    store.init_schema()?;

    let conn = store.conn.lock().unwrap();
    conn.execute(
        "INSERT INTO runs(suite, started_at, status) VALUES (?1, ?2, ?3)",
        params!["suite-legacy", "unix:1700000000", "running"],
    )?;
    let inserted_id = conn.last_insert_rowid();
    drop(conn);

    let stats = store.stats_best_effort()?;
    assert_eq!(stats.last_run_id, Some(inserted_id));
    assert_eq!(stats.last_run_at.as_deref(), Some("unix:1700000000"));
    Ok(())
}
