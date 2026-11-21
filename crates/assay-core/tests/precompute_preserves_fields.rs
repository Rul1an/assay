use std::{fs, path::Path, sync::Arc};

use assay_core::config::load_config;
use assay_core::judge::{JudgeRuntimeConfig, JudgeService};
use assay_core::providers::{embedder::fake::FakeEmbedder, llm::fake::FakeClient};
use assay_core::storage::{judge_cache::JudgeCache, Store};
use assay_core::trace::precompute::{precompute_embeddings, precompute_judge};
use serde_json::{json, Value};

fn judge() -> anyhow::Result<JudgeService> {
    let store = Store::memory()?;
    store.init_schema()?;
    let client = FakeClient::new("test".into())
        .with_response(r#"{"passed":false,"rationale":"fixture verdict"}"#.into());
    Ok(JudgeService::new(
        JudgeRuntimeConfig {
            enabled: true,
            provider: "fake".into(),
            model: Some("test".into()),
            samples: 1,
            temperature: 0.0,
            max_tokens: 100,
            refresh: true,
            reliability: Default::default(),
            system_prompt_version: String::new(),
        },
        JudgeCache::new(store),
        Some(Arc::new(client)),
    ))
}

const EXTRA: &str = r#""opaque":{"n":18446744073709551617,"n":1e400},"#;

fn read_rows(path: &Path) -> anyhow::Result<Vec<Value>> {
    Ok(fs::read_to_string(path)?
        .lines()
        .map(|line| {
            if !line.contains("positional-control") {
                assert!(line.contains(EXTRA), "producer JSON changed: {line}");
            }
            serde_json::from_str(&line.replacen(EXTRA, "", 1))
        })
        .collect::<Result<_, _>>()?)
}

async fn exercise(judging: bool) -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let cfg = tmp.path().join("eval.yaml");
    fs::write(
        &cfg,
        r#"configVersion: 1
suite: precompute
model: trace
tests:
- id: similarity
  input: {prompt: first}
  expected: {type: semantic_similarity_to, semantic_similarity_to: reference}
- id: judgement
  input: {prompt: second}
  expected: {type: relevance}
"#,
    )?;
    let cfg = load_config(&cfg, false, true)?;
    let input = tmp.path().join("source.jsonl");
    let output = tmp.path().join("judged.jsonl");
    let mut rows = Vec::new();
    for (id, prompt) in [("similarity", "first"), ("judgement", "second")] {
        let meta = json!({"fixture":id});
        rows.push(json!({
            "type":"assay.trace", "schema_version":1, "request_id":id, "prompt":prompt,
            "response":"answer", "meta":meta, "model":"recorded-model",
            "tool_calls":[{"tool_name":"lookup"}], "extension":{"future":[null,42]}
        }));
    }
    for id in ["absent", "null"] {
        let mut row = json!({"type":"assay.trace","schema_version":1,"request_id":id,
                             "prompt":id,"response":"answer"});
        if id == "null" {
            row["meta"] = Value::Null;
        }
        rows.push(row);
    }
    fs::write(
        &input,
        rows.iter()
            .map(|r| {
                format!(
                    "{}\n",
                    r.to_string().replacen('{', &format!("{{{EXTRA}"), 1)
                )
            })
            .collect::<String>(),
    )?;
    use std::io::Write;
    writeln!(
        fs::OpenOptions::new().append(true).open(&input)?,
        r#"[1,"assay.trace","positional-control","other","answer",{{}}]"#
    )?;
    rows.push(
        json!({"schema_version":1,"type":"assay.trace","request_id":"positional-control",
                     "prompt":"other","response":"answer","meta":{}}),
    );
    let embedder = Arc::new(FakeEmbedder::new("test", vec![0.25, 0.75]));
    if judging {
        precompute_judge(&input, &output, &judge()?, &cfg).await?;
    } else {
        precompute_embeddings(&input, &output, embedder.clone(), "test", &cfg).await?;
        assert_eq!(embedder.calls(), 2);
    }
    let mut actual = read_rows(&output)?;
    assert_eq!(actual.len(), rows.len());
    for (row, original) in actual.iter_mut().zip(rows) {
        let mut original = original;
        if original.get("meta").is_none() {
            original["meta"] = Value::Null; // Legacy default on output.
        }
        if let Some(assay) = row
            .get_mut("meta")
            .and_then(Value::as_object_mut)
            .and_then(|m| m.remove("assay"))
        {
            match original["meta"]["fixture"].as_str().unwrap() {
                "similarity" => assert_eq!(assay["embeddings"]["reference"], json!([0.25, 0.75])),
                "judgement" => assert_eq!(assay["judge"]["relevance"]["passed"], false),
                _ => unreachable!(),
            }
        } else {
            assert_ne!(
                original["meta"]["fixture"].as_str(),
                Some(if judging { "judgement" } else { "similarity" }),
                "missing enrichment"
            );
        }
        assert_eq!(*row, original, "only enrichment metadata may change");
    }
    Ok(())
}

#[tokio::test]
async fn embeddings_preserve_the_recorded_fields() -> anyhow::Result<()> {
    exercise(false).await
}

#[tokio::test]
async fn judge_preserves_the_recorded_fields() -> anyhow::Result<()> {
    exercise(true).await
}

#[tokio::test]
async fn enriches_interleaved_episodes_at_their_replay_carriers() -> anyhow::Result<()> {
    use assay_core::providers::{llm::LlmClient, trace::TraceClient};
    let dir = tempfile::tempdir()?;
    let config = dir.path().join("config.yaml");
    fs::write(
        &config,
        "configVersion: 1\nsuite: test\nmodel: trace\ntests:
- id: similarity
  input: {prompt: first}
  expected: {type: semantic_similarity_to, semantic_similarity_to: reference}
- id: judgement
  input: {prompt: second}
  expected: {type: relevance}
",
    )?;
    let cfg = load_config(&config, false, true)?;
    let input = dir.path().join("original");
    let embedded = dir.path().join("embedded");
    let output = dir.path().join("judged");
    let stream = concat!(
        "\n",
        r#"{"type":"episode_start","episode_id":"a","timestamp":0,"meta":{"marker":"a","opaque":{"x":18446744073709551617,"x":4},"assay":{"counter":18446744073709551617}}}"#,
        "  \r\n",
        r#"{"type":"episode_start","episode_id":"b","timestamp":0,"input":{"prompt":"second"}}"#,
        "  \r\n",
        r#"{"type":"step","episode_id":"a","step_id":"1","timestamp":1,"kind":"model","meta":{"gen_ai.prompt":"first"},"extra":"keep"}"#,
        "\n",
        r#"{"type":"episode_end","episode_id":"b","timestamp":2,"final_output":"second final"}"#,
        "\n",
        r#"{"type":"episode_end","episode_id":"a","timestamp":2,"final_output":"first final"}"#
    );
    fs::write(&input, stream)?;
    let embedder = Arc::new(FakeEmbedder::new("fake", vec![0.25, 0.75]));
    precompute_embeddings(&input, &embedded, embedder.clone(), "fake", &cfg).await?;
    precompute_judge(&embedded, &output, &judge()?, &cfg).await?;
    assert_eq!(embedder.calls(), 2);
    let replay = TraceClient::from_path(&output)?;
    let first = replay.complete("first", None).await?;
    assert_eq!(first.text, "first final");
    assert_eq!(first.meta["marker"], "a");
    assert_eq!(
        first.meta["assay"]["embeddings"]["reference"],
        json!([0.25, 0.75])
    );
    let second = replay.complete("second", None).await?;
    assert_eq!(second.text, "second final");
    assert!(second.meta.get("marker").is_none());
    assert_eq!(second.meta["assay"]["judge"]["relevance"]["passed"], false);
    let second_pass = dir.path().join("again");
    precompute_embeddings(&output, &second_pass, embedder.clone(), "fake", &cfg).await?;
    assert_eq!(embedder.calls(), 2);
    assert_eq!(fs::read(&second_pass)?, fs::read(&output)?);
    let enriched = fs::read_to_string(output)?;
    assert!(enriched.contains(r#""opaque":{"x":18446744073709551617,"x":4}"#));
    assert!(enriched.contains(r#""counter":18446744073709551617"#));
    assert!(enriched.starts_with('\n'));
    assert_eq!(enriched.matches("  \r\n").count(), 2);
    assert!(enriched.ends_with(stream.split_once(r#"{"type":"step""#).unwrap().1));
    assert_eq!(fs::read_to_string(input)?, stream);
    Ok(())
}

#[tokio::test]
async fn ambiguous_reference_is_refused_before_embedding_or_replacing_output() -> anyhow::Result<()>
{
    let dir = tempfile::tempdir()?;
    let input = dir.path().join("input");
    let output = dir.path().join("output");
    let config = dir.path().join("config.yaml");
    let cases = [
        ("a", "first", "fine"),
        ("b", "second", "one"),
        ("c", "second", "two"),
    ];
    let mut cfg = String::from("configVersion: 1\nsuite: test\nmodel: trace\ntests:\n");
    for (id, prompt, reference) in cases {
        cfg.push_str(&format!("- id: {id}\n  input: {{prompt: {prompt}}}\n  expected: {{type: semantic_similarity_to, semantic_similarity_to: {reference}}}\n"));
    }
    fs::write(&config, cfg)?;
    let cfg = load_config(&config, false, true)?;
    let mut stream = String::new();
    for prompt in ["first", "second"] {
        stream.push_str(&format!("{}\n{}\n",
            json!({"type":"episode_start","episode_id":prompt,"timestamp":0,"input":{"prompt":prompt}}),
            json!({"type":"episode_end","episode_id":prompt,"timestamp":1,"final_output":"answer"})));
    }
    fs::write(&input, stream)?;
    fs::write(&output, "keep this output")?;
    let embedder = Arc::new(FakeEmbedder::new("fake", vec![1.0]));
    let error = precompute_embeddings(&input, &output, embedder.clone(), "fake", &cfg)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("ambiguous precompute embeddings"),
        "{error:#}"
    );
    assert_eq!(fs::read_to_string(output)?, "keep this output");
    assert_eq!(embedder.calls(), 0);
    Ok(())
}

#[tokio::test]
async fn a_late_embedding_error_preserves_an_existing_episode_output() -> anyhow::Result<()> {
    struct RejectSecond;
    #[async_trait::async_trait]
    impl assay_core::providers::embedder::Embedder for RejectSecond {
        async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
            match text {
                "first answer" | "reference" => Ok(vec![1.0]),
                "second answer" => anyhow::bail!("provider refused the second answer"),
                _ => anyhow::bail!("unexpected provider input"),
            }
        }
        fn name(&self) -> &'static str {
            "fixture"
        }
        fn model_id(&self) -> String {
            "fixture".into()
        }
    }
    let dir = tempfile::tempdir()?;
    let input = dir.path().join("input");
    let output = dir.path().join("output");
    let config = dir.path().join("config.yaml");
    let mut cases = String::from("configVersion: 1\nsuite: test\nmodel: trace\ntests:\n");
    let mut stream = String::new();
    for prompt in ["first", "second"] {
        cases.push_str(&format!("- id: test-{prompt}\n  input: {{prompt: {prompt}}}\n  expected: {{type: semantic_similarity_to, semantic_similarity_to: reference}}\n"));
        stream.push_str(&format!("{}\n{}\n",
            json!({"type":"episode_start","episode_id":prompt,"timestamp":0,"input":{"prompt":prompt}}),
            json!({"type":"episode_end","episode_id":prompt,"timestamp":1,"final_output":format!("{prompt} answer")})));
    }
    fs::write(&config, cases)?;
    fs::write(&input, &stream)?;
    fs::write(&output, "previous result")?;
    let cfg = load_config(&config, false, true)?;
    let error = precompute_embeddings(&input, &output, Arc::new(RejectSecond), "fixture", &cfg)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("provider refused the second answer"),
        "{error:#}"
    );
    assert_eq!(fs::read_to_string(output)?, "previous result");
    assert_eq!(fs::read_to_string(input)?, stream);
    Ok(())
}

#[tokio::test]
async fn an_alias_of_the_episode_input_is_refused_before_using_a_provider() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let input = dir.path().join("input");
    let config = dir.path().join("config.yaml");
    fs::write(&config,"configVersion: 1\nsuite: alias\nmodel: trace\ntests:\n- id: first\n  input: {prompt: hello}\n  expected: {type: semantic_similarity_to, semantic_similarity_to: reference}\n")?;
    let cfg = load_config(&config, false, true)?;
    let stream = concat!(
        r#"{"type":"episode_start","timestamp":0,"episode_id":"producer","input":{"prompt":"hello"}}"#,
        "\n",
        r#"{"type":"episode_end","timestamp":1,"episode_id":"producer","final_output":"world"}"#,
        "\n"
    );
    fs::write(&input, stream)?;
    let embedder = Arc::new(FakeEmbedder::new("fake", vec![1.0]));
    let alias = dir.path().join("alias");
    fs::hard_link(&input, &alias)?;
    let error = precompute_embeddings(&input, &alias, embedder.clone(), "fake", &cfg)
        .await
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("overwrite an input file"),
        "{error:#}"
    );
    assert_eq!(fs::read(&input)?, stream.as_bytes());
    #[cfg(unix)]
    assert_eq!(
        embedder.calls(),
        0,
        "alias should be refused before provider use"
    );
    Ok(())
}

#[tokio::test]
async fn precompute_refuses_to_publish_an_episode_too_large_to_read_again() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let input = dir.path().join("trace");
    let output = dir.path().join("result");
    let config = dir.path().join("config.yaml");
    fs::write(&config, "configVersion: 1\nsuite: size\nmodel: trace\ntests:\n- id: t\n  input: {prompt: q}\n  expected: {type: semantic_similarity_to, semantic_similarity_to: reference}\n")?;
    let cfg = load_config(&config, false, true)?;
    let mut stream =
        br#"{"type":"episode_start","episode_id":"a","timestamp":0,"input":{"prompt":"q"}}
{"type":"episode_end","episode_id":"a","timestamp":1,"final_output":"answer"}
"#
        .to_vec();
    stream.resize(16 * 1024 * 1024, b' ');
    fs::write(&input, stream)?;
    fs::write(&output, "previous")?;
    let provider = Arc::new(FakeEmbedder::new("fake", vec![1.0]));
    let error = precompute_embeddings(&input, &output, provider, "fake", &cfg)
        .await
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("episode output exceeds"),
        "{error:#}"
    );
    assert_eq!(fs::read_to_string(output)?, "previous");
    Ok(())
}

#[tokio::test]
async fn an_unclassified_first_record_cannot_truncate_the_destination() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let input = dir.path().join("input");
    let output = dir.path().join("output");
    let config = dir.path().join("config");
    fs::write(
        &config,
        "configVersion: 1\nsuite: probe\nmodel: trace\ntests:\n- id: probe\n  input: {prompt: q}\n  expected: {type: semantic_similarity_to, semantic_similarity_to: reference}\n",
    )?;
    let cfg = load_config(&config, false, true)?;
    let fake = Arc::new(FakeEmbedder::new("fake", vec![1.0]));
    let start = br#"{"type":"episode_start","episode_id":"a","timestamp":0}"#;
    for (name, mut bytes) in [
        ("long-start", {
            let mut b = start.to_vec();
            b.resize(16 * 1024 * 1024 + 1, b' ');
            b
        }),
        ("long-prefix", {
            let mut b = vec![b' '; 16 * 1024 * 1024];
            b.push(b'\n');
            b.extend(start);
            b
        }),
        ("bad-first", br#"{"type":"step","timestamp":0}"#.to_vec()),
        (
            "duplicate-type",
            br#"{"type":"episode_start","type":"step"}"#.to_vec(),
        ),
    ] {
        bytes.push(b'\n');
        fs::write(&input, bytes)?;
        fs::write(&output, b"keep")?;
        assert!(
            precompute_embeddings(&input, &output, fake.clone(), "fake", &cfg)
                .await
                .is_err(),
            "{name}"
        );
        assert_eq!(fs::read(&output)?, b"keep", "{name}");
    }
    // Old-format rows have always streamed without a fixed byte ceiling. This row does not
    // match a configured test, so its existing metadata and original bytes should survive.
    use std::io::Read;
    let mut legacy = br#"{"schema_version":1,"type":"assay.trace","request_id":"x","prompt":"q","response":"a","meta":null}"#.to_vec();
    legacy.resize(16 * 1024 * 1024 + 1, b' ');
    legacy.push(b'\n');
    fs::write(&input, &legacy)?;
    precompute_embeddings(&input, &output, fake, "fake", &cfg).await?;
    let mut emitted = Vec::new();
    fs::File::open(output)?
        .take(legacy.len() as u64 + 1)
        .read_to_end(&mut emitted)?;
    assert!(emitted == legacy, "the padded legacy row changed");
    Ok(())
}
