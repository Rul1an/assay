use assay_core::providers::{llm::LlmClient, trace::TraceClient};
use assay_core::trace::ingest::ingest_file;
use serde_json::{json, Value};

#[tokio::test]
async fn legacy_response_alias_matches_replay() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let input = dir.path().join("source.jsonl");
    let output = dir.path().join("events.jsonl");
    for (fields, expected) in [
        (
            json!({"response":"primary","text":"alternate"}),
            Some("primary"),
        ),
        (json!({"response":"","text":"alternate"}), Some("")),
        (json!({"response":null,"text":"alternate"}), None),
        (json!({"response":42,"text":"alternate"}), None),
        (json!({"text":42}), None),
        (json!({"text":"alternate"}), Some("alternate")),
    ] {
        let mut row =
            json!({"type":"assay.trace","schema_version":1,"request_id":"id","prompt":"P"});
        row.as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        std::fs::write(&input, row.to_string())?;
        assert!(TraceClient::from_path(&input).is_err());
        let converted = ingest_file(&input, &output);
        if let Some(expected) = expected {
            converted?;
            let after = TraceClient::from_path(&output)?.complete("P", None).await?;
            assert_eq!(after.text, expected, "{fields}");
        } else {
            assert!(converted.is_err(), "{fields}");
        }
    }
    let unknown: Value = json!({"type":"foreign","schema_version":1,"request_id":"id","prompt":"P","text":"alternate"});
    std::fs::write(&input, unknown.to_string())?;
    assert!(ingest_file(&input, &output).is_err());
    Ok(())
}

#[test]
fn newer_or_invalid_version_cannot_enable_the_v1_alias() {
    use assay_core::trace::upgrader::StreamUpgrader;
    for version in [0, 2, u32::MAX] {
        let row = json!({"type":"assay.trace","schema_version":version,"request_id":"id","prompt":"P","text":"alternate"});
        let bytes = row.to_string();
        assert!(
            StreamUpgrader::new(bytes.as_bytes())
                .next()
                .unwrap()
                .is_err(),
            "version {version}"
        );
    }
}

#[tokio::test]
async fn alias_with_json_text_preserves_literal_and_model() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let input = dir.path().join("source.jsonl");
    let output = dir.path().join("events.jsonl");
    let literal = r#"{"completion":"inner","model":"embedded"}"#;
    for model in [None, Some("recorded")] {
        let mut row = json!({"type":"assay.trace","schema_version":1,"request_id":"id","prompt":"P","text":literal});
        if let Some(model) = model {
            row["model"] = json!(model);
        }
        std::fs::write(&input, row.to_string())?;
        ingest_file(&input, &output)?;
        let replay = TraceClient::from_path(&output)?.complete("P", None).await?;
        assert_eq!(replay.text, literal);
        assert_eq!(replay.model, model.unwrap_or("trace"));
    }
    Ok(())
}

#[tokio::test]
async fn untagged_conversion_preserves_replay_and_recorded_identity() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let input = dir.path().join("source.jsonl");
    let output = dir.path().join("events.jsonl");
    let tool_meta = |args| json!({"tool_calls":[{"id":"legacy-v1","tool_name":"Search","args":args,"result":null,"error":null,"index":0,"ts_ms":0}]});
    for (prompt, fields, text, model, meta) in [
        (
            "P",
            json!({"prompt":"P","response":"primary","text":"other","model":"recorded","meta":{"unused":true}}),
            "primary",
            "recorded",
            json!({}),
        ),
        (
            "P",
            json!({"prompt":"P","text":"alternate","tool_calls":[{"tool_name":"Search","args":{"x":1}}]}),
            "alternate",
            "trace",
            json!({"tool_calls":[{"tool_name":"Search","args":{"x":1}}]}),
        ),
        (
            "ignore",
            json!({"tool":"Search","args":{"q":"query"},"result":{"ok":true}}),
            r#"{"ok":true}"#,
            "trace",
            tool_meta(json!({"q":"query"})),
        ),
        (
            "ignore",
            json!({"tool":"Search"}),
            "",
            "trace",
            tool_meta(json!({})),
        ),
    ] {
        let mut row = fields;
        row["request_id"] = json!("recorded-id");
        std::fs::write(&input, row.to_string())?;
        assert!(TraceClient::from_path(&input).is_err());
        ingest_file(&input, &output)?;
        let after = TraceClient::from_path(&output)?
            .complete(prompt, None)
            .await?;
        assert_eq!(
            (after.text.as_str(), after.model.as_str(), after.meta),
            (text, model, meta),
            "{row}"
        );
        let bytes = std::fs::read_to_string(&output)?;
        let start: Value = serde_json::from_str(bytes.lines().next().unwrap())?;
        assert_eq!(start["episode_id"], "recorded-id");
        let step: Value = serde_json::from_str(bytes.lines().nth(1).unwrap())?;
        assert_eq!(step["kind"], "legacy_replay");
        assert!(step["name"].is_null());
    }
    Ok(())
}

#[test]
fn idless_rows_get_replayable_episode_ids_without_claiming_a_request_id() {
    use assay_core::trace::{schema::TraceEvent, upgrader::StreamUpgrader};
    use std::io::Cursor;
    let convert = |lines: &[String]| {
        StreamUpgrader::new(Cursor::new(lines.join("\n"))).collect::<Result<Vec<_>, _>>()
    };
    let mut previous = String::new();
    for row in [
        json!({"type":"assay.trace","prompt":"named","response":"ok"}),
        json!({"prompt":"untagged","text":"ok"}),
        json!({"tool":"Search","args":{},"result":"ok"}),
    ] {
        let raw = row.to_string();
        let events =
            convert(std::slice::from_ref(&raw)).expect("a recorded request_id is optional");
        let TraceEvent::EpisodeStart(start) = &events[0] else {
            panic!("missing episode")
        };
        let id = &start.episode_id;
        use sha2::{Digest, Sha256};
        assert_eq!(
            id,
            &format!(
                "assay:legacy:sha256:{}",
                hex::encode(Sha256::digest(raw.as_bytes()))
            )
        );
        assert_ne!(id, &previous);
        assert!(!serde_json::to_value(start)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("request_id"));
        assert_eq!(events, convert(std::slice::from_ref(&raw)).unwrap());

        let mut explicit = row.clone();
        explicit["request_id"] = json!(id);
        let native = json!({"type":"episode_end","episode_id":id,"timestamp":1});
        for other in [explicit.to_string(), native.to_string()] {
            for pair in [[other.clone(), raw.clone()], [raw.clone(), other.clone()]] {
                assert!(convert(&pair).is_err(), "collision accepted: {pair:?}");
            }
            convert(&[other]).expect("the recorded identifier is usable on its own");
        }
        previous.clone_from(id);
    }
}

#[test]
fn derived_step_does_not_claim_a_recorded_step_id() -> anyhow::Result<()> {
    use assay_core::trace::{schema::TraceEvent, upgrader::StreamUpgrader};
    let convert =
        |input: &str| StreamUpgrader::new(input.as_bytes()).collect::<Result<Vec<_>, _>>();
    let raw = r#"{"prompt":"P","response":"ok"}"#;
    let generated = convert(raw).unwrap();
    let TraceEvent::Step(step) = &generated[1] else {
        panic!("expected step")
    };
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("input.jsonl");
    let db = dir.path().join("output.db");
    let output = dir.path().join("output.jsonl");
    std::fs::write(&source, raw)?;
    ingest_file(&source, &db)?;
    ingest_file(&source, &output)?;
    let original = std::fs::read(&output)?;
    for native in [
        json!({"type":"step","episode_id":"recorded","step_id":step.step_id,"timestamp":1,"kind":"model","content":"other"}),
        json!({"type":"tool_call","episode_id":"recorded","step_id":step.step_id,"timestamp":1,"tool_name":"Search"}),
    ] {
        for pair in [format!("{raw}\n{native}"), format!("{native}\n{raw}")] {
            std::fs::write(&source, pair)?;
            for dest in [&db, &output] {
                let err = ingest_file(&source, dest)
                    .err()
                    .expect("derived step collision");
                assert!(
                    format!("{err:#}").contains("conflicting derived"),
                    "{err:#}"
                );
            }
            assert_eq!(std::fs::read(&output)?, original);
            let stored = rusqlite::Connection::open(&db)?.query_row(
                "SELECT episode_id, content FROM steps WHERE id=?1",
                [&step.step_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )?;
            assert_eq!((&stored.0, stored.1.as_str()), (&step.episode_id, "ok"));
        }
        assert!(convert(&native.to_string()).is_ok());
    }
    Ok(())
}

#[test]
fn sqlite_step_ownership_survives_separate_imports() -> anyhow::Result<()> {
    use assay_core::trace::{schema::TraceEvent, upgrader::StreamUpgrader};
    let raw = r#"{"prompt":"P","response":"derived"}"#;
    let events = StreamUpgrader::new(raw.as_bytes()).collect::<Result<Vec<_>, _>>()?;
    let TraceEvent::Step(step) = &events[1] else {
        panic!("missing derived step")
    };
    let native = [
        json!({"type":"episode_start","episode_id":"native","timestamp":0,"input":{},"meta":{}}),
        json!({"type":"step","episode_id":"native","step_id":step.step_id,"timestamp":1,"kind":"model","content":"recorded"}),
    ].map(|r|r.to_string()).join("\n");
    let foreign_call = json!({"type":"tool_call","episode_id":"another","step_id":step.step_id,"timestamp":2,"tool_name":"Search"}).to_string();
    for (first, second, owner, content) in [
        (raw, native.as_str(), step.episode_id.as_str(), "derived"),
        (native.as_str(), raw, "native", "recorded"),
        (native.as_str(), foreign_call.as_str(), "native", "recorded"),
    ] {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("source.jsonl");
        let db = dir.path().join("previous.sqlite");
        let keep = |response: &str| {
            json!({"type":"assay.trace","request_id":"keep","prompt":"keep","response":response})
                .to_string()
        };
        std::fs::write(&input, format!("{}\n{first}", keep("original")))?;
        ingest_file(&input, &db)?;
        ingest_file(&input, &db)?; // Replacing the same episode remains valid.
        std::fs::write(&input, format!("{}\n{second}", keep("changed")))?;
        let err = ingest_file(&input, &db)
            .err()
            .expect("step belongs to a different episode");
        assert!(format!("{err:#}").contains("episode"), "{err:#}");
        let conn = rusqlite::Connection::open(&db)?;
        for (id, expected_owner, expected_content) in [
            (step.step_id.as_str(), owner, content),
            ("keep-step-0", "keep", "original"),
        ] {
            let stored = conn.query_row(
                "SELECT episode_id,content FROM steps WHERE id=?1",
                [id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )?;
            assert_eq!(
                (stored.0.as_str(), stored.1.as_str()),
                (expected_owner, expected_content)
            );
        }
        assert_eq!(
            conn.query_row("SELECT count(*) FROM tool_calls", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
    }
    Ok(())
}
