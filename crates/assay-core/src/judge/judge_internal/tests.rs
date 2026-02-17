use crate::judge::reliability::{ReliabilityConfig, RerunStrategy, VerdictStatus};
use crate::judge::{JudgeRuntimeConfig, JudgeService};
use crate::model::{LlmResponse, TestInput};
use crate::providers::llm::LlmClient;
use crate::storage::judge_cache::JudgeCache;
use crate::storage::Store;
use async_trait::async_trait;
use std::sync::Arc;
use tempfile::tempdir;

struct MockLlmClient {
    responses: std::sync::Mutex<Vec<String>>,
}

#[async_trait]
impl LlmClient for MockLlmClient {
    async fn complete(
        &self,
        _prompt: &str,
        _system: Option<&[String]>,
    ) -> anyhow::Result<LlmResponse> {
        let mut resps = self.responses.lock().unwrap();
        if resps.is_empty() {
            anyhow::bail!("No more mock responses");
        }
        let text = resps.remove(0);
        Ok(LlmResponse {
            text,
            provider: "mock".to_string(),
            model: "mock".to_string(),
            cached: false,
            meta: serde_json::Value::Null,
        })
    }
    fn provider_name(&self) -> &'static str {
        "mock"
    }
}

#[tokio::test]
async fn contract_two_of_three_majority() {
    let tmp = tempdir().unwrap();
    let store = Store::open(&tmp.path().join("test.db")).unwrap();
    store.init_schema().unwrap();
    let cache = JudgeCache::new(store);

    // Mock client: Fail, Pass, Pass -> Should result in Pass (2 of 3)
    let mock_client = Arc::new(MockLlmClient {
        responses: std::sync::Mutex::new(vec![
            r#"{"passed": false, "rationale": "bad"}"#.to_string(),
            r#"{"passed": true, "rationale": "good"}"#.to_string(),
            r#"{"passed": true, "rationale": "better"}"#.to_string(),
        ]),
    });

    let config = JudgeRuntimeConfig {
        enabled: true,
        provider: "mock".to_string(),
        model: Some("mock".to_string()),
        samples: 1,
        temperature: 0.0,
        max_tokens: 100,
        refresh: false,
        reliability: ReliabilityConfig {
            borderline_min: 0.4,
            borderline_max: 0.6,
            rerun_strategy: RerunStrategy::AlwaysThree,
            max_extra_calls_per_test: 2,
            ..Default::default()
        },
        system_prompt_version: "v1".to_string(),
    };

    let svc = JudgeService::new(config, cache, Some(mock_client));
    let mut meta = serde_json::json!({});
    let data = TestInput {
        prompt: "test".to_string(),
        context: None,
    };

    svc.evaluate(
        "test_id",
        "rubric_id",
        &data,
        "resp",
        None,
        &mut meta,
        Some(42),
    )
    .await
    .unwrap();

    let result = meta["assay"]["judge"]["rubric_id"].as_object().unwrap();
    assert_eq!(result["passed"], true);
    assert_eq!(result["verdict"], "Pass");
    assert_eq!(result["extra_calls_used"], 2);
    assert_eq!(result["agreement"], 2.0 / 3.0);
}

#[tokio::test]
async fn contract_sprt_early_stop() {
    let tmp = tempdir().unwrap();
    let store = Store::open(&tmp.path().join("test.db")).unwrap();
    store.init_schema().unwrap();
    let cache = JudgeCache::new(store);

    // Mock client: Fail, Fail -> Should stop early with FAIL (score 0.0 is not [0.4, 0.6])
    let mock_client = Arc::new(MockLlmClient {
        responses: std::sync::Mutex::new(vec![
            r#"{"passed": false, "rationale": "bad"}"#.to_string(),
            r#"{"passed": false, "rationale": "very bad"}"#.to_string(),
        ]),
    });

    let config = JudgeRuntimeConfig {
        enabled: true,
        provider: "mock".to_string(),
        model: Some("mock".to_string()),
        samples: 1,
        temperature: 0.0,
        max_tokens: 100,
        refresh: false,
        reliability: ReliabilityConfig {
            borderline_min: 0.4,
            borderline_max: 0.6,
            rerun_strategy: RerunStrategy::SequentialSprt,
            max_extra_calls_per_test: 2,
            ..Default::default()
        },
        system_prompt_version: "v1".to_string(),
    };

    let svc = JudgeService::new(config, cache, Some(mock_client));
    let mut meta = serde_json::json!({});
    let data = TestInput {
        prompt: "test".to_string(),
        context: None,
    };

    svc.evaluate(
        "test_id",
        "rubric_id",
        &data,
        "resp",
        None,
        &mut meta,
        Some(123),
    )
    .await
    .unwrap();

    let result = meta["assay"]["judge"]["rubric_id"].as_object().unwrap();
    assert_eq!(result["passed"], false);
    assert_eq!(result["verdict"], "Fail");
    assert_eq!(result["extra_calls_used"], 1); // Stops after second Fail (Vote Confirmation)
}

#[tokio::test]
async fn contract_abstain_mapping() {
    let config = ReliabilityConfig {
        borderline_min: 0.4,
        borderline_max: 0.6,
        ..Default::default()
    };
    assert_eq!(config.assess(0.5), VerdictStatus::Abstain);
    assert_eq!(config.assess(0.3), VerdictStatus::Fail);
    assert_eq!(config.assess(0.7), VerdictStatus::Pass);
}

#[tokio::test]
async fn contract_determinism_parallel_replay_legacy() {
    let tmp = tempdir().unwrap();
    let store = Store::open(&tmp.path().join("test.db")).unwrap();
    store.init_schema().unwrap();
    let cache = JudgeCache::new(store);

    async fn run_eval_inner(
        cache: JudgeCache,
        seed: u64,
        inflate_counter: bool,
    ) -> serde_json::Value {
        let mock_client = Arc::new(MockLlmClient {
            responses: std::sync::Mutex::new(vec![
                r#"{"passed": false, "rationale": "bad"}"#.to_string(),
                r#"{"passed": true, "rationale": "good"}"#.to_string(),
                r#"{"passed": true, "rationale": "better"}"#.to_string(),
            ]),
        });

        let config = JudgeRuntimeConfig {
            enabled: true,
            provider: "mock".to_string(),
            model: Some("mock".to_string()),
            samples: 1,
            temperature: 0.0,
            max_tokens: 100,
            refresh: true,
            reliability: ReliabilityConfig {
                rerun_strategy: RerunStrategy::AlwaysThree,
                max_extra_calls_per_test: 2,
                max_extra_calls_per_run: 100,
                ..Default::default()
            },
            system_prompt_version: "v1".to_string(),
        };

        let svc = JudgeService::new(config, cache, Some(mock_client));
        if inflate_counter {
            svc.global_extra_calls
                .fetch_add(50, std::sync::atomic::Ordering::Relaxed);
        }

        let mut meta = serde_json::json!({});
        let data = TestInput {
            prompt: "test".to_string(),
            context: None,
        };
        svc.evaluate(
            "test_id",
            "rubric_id",
            &data,
            "resp",
            None,
            &mut meta,
            Some(seed),
        )
        .await
        .unwrap();
        meta["assay"]["judge"]["rubric_id"].clone()
    }

    let seed = 999;
    // Simulate real parallelism
    let (res1, res2) = tokio::join!(
        run_eval_inner(cache.clone(), seed, false),
        run_eval_inner(cache.clone(), seed, true)
    );

    assert_eq!(
        res1["verdict"], res2["verdict"],
        "Determinism failed: verdicts differed"
    );
    assert_eq!(res1["extra_calls_used"], res2["extra_calls_used"]);
    assert_eq!(res1["score"], res2["score"]);
}

#[tokio::test]
async fn contract_determinism_parallel_replay() {
    let tmp = tempdir().unwrap();
    let store = Store::open(&tmp.path().join("test.db")).unwrap();
    store.init_schema().unwrap();
    let cache = JudgeCache::new(store);

    // 1. Setup Common Config
    let config = JudgeRuntimeConfig {
        enabled: true,
        provider: "mock".to_string(),
        model: Some("mock".to_string()),
        samples: 1,
        temperature: 0.0,
        max_tokens: 100,
        refresh: true,
        reliability: ReliabilityConfig {
            rerun_strategy: RerunStrategy::AlwaysThree,
            max_extra_calls_per_test: 2,
            max_extra_calls_per_run: 50, // Limit is 50
            ..Default::default()
        },
        system_prompt_version: "v1".to_string(),
    };

    // 2. Setup SHARED global counter (Inflated)
    let shared_counter = Arc::new(std::sync::atomic::AtomicU32::new(100)); // Start above limit (50)

    // 3. Setup Independent Mocks (Identical Responses)
    // Each service gets its own sequence: Fail -> Pass -> Pass.
    // This ensures we test "shared counter contention" without "scheduling interleaving noise".
    let make_mock = || {
        Arc::new(MockLlmClient {
            responses: std::sync::Mutex::new(vec![
                r#"{"passed": false, "rationale": "bad"}"#.to_string(),
                r#"{"passed": true, "rationale": "good"}"#.to_string(),
                r#"{"passed": true, "rationale": "better"}"#.to_string(),
            ]),
        })
    };

    // 4. Create Two Service Instances sharing the Atomic
    let mut svc1_struct = JudgeService::new(config.clone(), cache.clone(), Some(make_mock()));
    svc1_struct.global_extra_calls = shared_counter.clone();
    let svc1 = Arc::new(svc1_struct);

    let mut svc2_struct = JudgeService::new(config.clone(), cache.clone(), Some(make_mock()));
    svc2_struct.global_extra_calls = shared_counter.clone();
    let svc2 = Arc::new(svc2_struct);

    let run_eval = |svc: Arc<JudgeService>, seed: u64| async move {
        let mut meta = serde_json::json!({});
        let data = TestInput {
            prompt: "test".to_string(),
            context: None,
        };
        svc.evaluate(
            "test_id",
            "rubric_id",
            &data,
            "resp",
            None,
            &mut meta,
            Some(seed),
        )
        .await
        .unwrap();
        meta["assay"]["judge"]["rubric_id"].clone()
    };

    let seed = 999;
    // 5. Run Parallel
    let (mut res1, mut res2) = tokio::join!(run_eval(svc1, seed), run_eval(svc2, seed));

    // 6. Normalize Metadata (Remove non-deterministic timestamps)
    res1.as_object_mut().unwrap().remove("cached_at");
    res2.as_object_mut().unwrap().remove("cached_at");

    // 7. Verify Exact Identity
    // - Soft budget meant both completed (Status Pass)
    // - Determinism meant both got same score/metadata despite sharing saturated atomic.
    assert_eq!(
        res1["verdict"], "Pass",
        "Soft budget failed: Execution stopped early"
    );
    assert_eq!(
        res1, res2,
        "Determinism failed: Full metadata identity mismatch"
    );
}
