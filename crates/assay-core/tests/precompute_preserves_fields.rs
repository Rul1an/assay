use std::sync::atomic::{AtomicUsize, Ordering};
use std::{fs, sync::Arc};

use assay_core::config::load_config;
use assay_core::judge::{JudgeRuntimeConfig, JudgeService};
use assay_core::model::LlmResponse;
use assay_core::providers::llm::LlmClient;
use assay_core::providers::{embedder::fake::FakeEmbedder, llm::fake::FakeClient};
use assay_core::storage::{judge_cache::JudgeCache, Store};
use assay_core::trace::precompute::{precompute_embeddings, precompute_judge};
use serde_json::json;

/// The fixture's judge provider, counting each call it answers so a test can show none was made.
struct CountedJudge {
    inner: FakeClient,
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl LlmClient for CountedJudge {
    async fn complete(
        &self,
        prompt: &str,
        context: Option<&[String]>,
    ) -> anyhow::Result<LlmResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.complete(prompt, context).await
    }

    fn provider_name(&self) -> &'static str {
        self.inner.provider_name()
    }
}

fn judge() -> anyhow::Result<JudgeService> {
    Ok(counted_judge()?.0)
}

/// The judge service and the number of calls its provider has answered.
fn counted_judge() -> anyhow::Result<(JudgeService, Arc<AtomicUsize>)> {
    let store = Store::memory()?;
    store.init_schema()?;
    let calls = Arc::new(AtomicUsize::new(0));
    let client = CountedJudge {
        inner: FakeClient::new("test".into())
            .with_response(r#"{"passed":false,"rationale":"fixture verdict"}"#.into()),
        calls: calls.clone(),
    };
    let service = JudgeService::new(
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
    );
    Ok((service, calls))
}

/// Each form 6.x read as an older replay row. From 7.0 only `assay trace migrate` reads them
/// (ADR-052), so precompute refuses them as replay does, with the same error.
const OLD_ROWS: [&str; 3] = [
    r#"{"schema_version":1,"type":"assay.trace","request_id":"similarity","prompt":"first","response":"answer","meta":{"fixture":"similarity"}}"#,
    r#"{"request_id":"judgement","prompt":"second","response":"answer"}"#,
    r#"[1,"assay.trace","positional","first","answer",{}]"#,
];

async fn refuse_older_rows(judging: bool) -> anyhow::Result<()> {
    use assay_core::providers::trace::TraceLoadError;
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
    // One episode each command would enrich, the embedding for `first` and the judgement for
    // `second`, so a provider called before the older row is refused is seen by either command.
    let episodes = concat!(
        r#"{"type":"episode_start","episode_id":"a","timestamp":0,"input":{"prompt":"first"}}"#,
        "\n",
        r#"{"type":"episode_end","episode_id":"a","timestamp":1,"final_output":"answer"}"#,
        "\n",
        r#"{"type":"episode_start","episode_id":"b","timestamp":0,"input":{"prompt":"second"}}"#,
        "\n",
        r#"{"type":"episode_end","episode_id":"b","timestamp":1,"final_output":"answer"}"#,
        "\n"
    );
    // The line the refusal names: the older row itself, after blank lines and after native
    // episodes as well. An older trace longer than the episode ceiling is still named for what it
    // is, from its first record, rather than for its length.
    let cases = [
        (format!("{}\n", OLD_ROWS[0]), 1),
        (format!("\n  \n{}\n", OLD_ROWS[1]), 3),
        (format!("{}\n", OLD_ROWS[2]), 1),
        (format!("{episodes}{}\n", OLD_ROWS[0]), 5),
        (
            format!("{}\n{}\n", OLD_ROWS[0], " ".repeat(16 * 1024 * 1024)),
            1,
        ),
    ];
    for (source, line) in cases {
        fs::write(&input, &source)?;
        fs::write(&output, "keep")?;
        // Each command's own provider, so the count below is of the one it was given.
        let (calls, error) = if judging {
            let (judge, calls) = counted_judge()?;
            let result = precompute_judge(&input, &output, &judge, &cfg).await;
            (calls.load(Ordering::SeqCst), result)
        } else {
            let embedder = Arc::new(FakeEmbedder::new("test", vec![0.25, 0.75]));
            let result =
                precompute_embeddings(&input, &output, embedder.clone(), "test", &cfg).await;
            (embedder.calls(), result)
        };
        let error = error.expect_err("an older row must be refused");
        assert!(
            matches!(
                error.downcast_ref::<TraceLoadError>(),
                Some(TraceLoadError::LegacyReplay { line_no }) if *line_no == line
            ),
            "{source:?}: {error:#}"
        );
        assert!(
            format!("{error:#}").contains("assay trace migrate"),
            "{error:#}"
        );
        assert_eq!(fs::read_to_string(&output)?, "keep", "{source:?}");
        assert_eq!(fs::read_to_string(&input)?, source);
        assert_eq!(
            calls, 0,
            "{source:?}: refused before the provider is called"
        );
    }
    Ok(())
}

#[tokio::test]
async fn embeddings_refuse_an_older_row_as_replay_does() -> anyhow::Result<()> {
    refuse_older_rows(false).await
}

#[tokio::test]
async fn judge_refuses_an_older_row_as_replay_does() -> anyhow::Result<()> {
    refuse_older_rows(true).await
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
        // 6.x streams an older row of any length. 7.0 has no older reader, so the row meets the
        // episode ceiling like any first record, before it is read whole.
        ("long-older-row", {
            let mut b = br#"{"schema_version":1,"type":"assay.trace","request_id":"x","prompt":"q","response":"a"}"#.to_vec();
            b.resize(16 * 1024 * 1024 + 1, b' ');
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
        let error = precompute_embeddings(&input, &output, fake.clone(), "fake", &cfg)
            .await
            .expect_err(name);
        if name.starts_with("long-") {
            assert!(
                format!("{error:#}").contains("exceeds 16777216 bytes"),
                "{name}: a record past the ceiling is refused at the ceiling: {error:#}"
            );
        }
        assert_eq!(fs::read(&output)?, b"keep", "{name}");
    }
    assert_eq!(fake.calls(), 0, "every case is refused before a provider");
    Ok(())
}
