//! VCR cassette recording test.
//!
//! Run with:
//! ```bash
//! ASSAY_VCR_MODE=record OPENAI_API_KEY=sk-... cargo test -p assay-core --test vcr_record -- --ignored --nocapture
//! ```
//!
//! This test makes real API calls and records responses to cassettes.
//! Cassettes are saved to `tests/fixtures/perf/semantic_vcr/cassettes/<provider>/<kind>/`.

use assay_core::vcr::{VcrClient, VcrMode};
use serde_json::json;
use std::path::PathBuf;

const CASSETTE_DIR: &str = "tests/fixtures/perf/semantic_vcr/cassettes";

/// Record embedding cassettes for semantic_vcr fixture.
///
/// Requires OPENAI_API_KEY environment variable.
#[tokio::test]
#[ignore] // Run manually with --ignored
async fn record_embedding_cassettes() {
    let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY required for recording");

    let cassette_dir = PathBuf::from("tests/fixtures/perf/semantic_vcr/cassettes");
    let mut vcr = VcrClient::new(VcrMode::Record, cassette_dir);

    // Record embedding for "Reference text for similarity."
    let url = "https://api.openai.com/v1/embeddings";
    let body = json!({
        "input": "Reference text for similarity.",
        "model": "text-embedding-3-small",
        "encoding_format": "float"
    });

    let auth = format!("Bearer {}", api_key);
    let resp = vcr.post_json(url, &body, Some(&auth)).await;

    match resp {
        Ok(r) => {
            println!("Embedding recorded: status={}", r.status);
            assert!(r.is_success(), "API call failed: {:?}", r.body);
        }
        Err(e) => panic!("Failed to record embedding: {}", e),
    }

    println!("Cassettes saved to tests/fixtures/perf/semantic_vcr/cassettes/embeddings/");
}

/// Record judge/LLM cassettes for semantic_vcr fixture.
///
/// Requires OPENAI_API_KEY environment variable.
#[tokio::test]
#[ignore] // Run manually with --ignored
async fn record_judge_cassettes() {
    let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY required for recording");

    let cassette_dir = PathBuf::from("tests/fixtures/perf/semantic_vcr/cassettes");
    let mut vcr = VcrClient::new(VcrMode::Record, cassette_dir);

    // Record judge call for faithfulness evaluation
    let url = "https://api.openai.com/v1/chat/completions";
    let body = json!({
        "model": "gpt-4o-mini",
        "messages": [
            {
                "role": "user",
                "content": "Evaluate the faithfulness of this response. Score from 0.0 to 1.0.\n\nResponse: The answer is faithful to the source.\n\nReturn only a JSON object with 'score' and 'reason' fields."
            }
        ],
        "temperature": 0.0,
        "max_tokens": 256
    });

    let auth = format!("Bearer {}", api_key);
    let resp = vcr.post_json(url, &body, Some(&auth)).await;

    match resp {
        Ok(r) => {
            println!("Judge response recorded: status={}", r.status);
            assert!(r.is_success(), "API call failed: {:?}", r.body);
        }
        Err(e) => panic!("Failed to record judge response: {}", e),
    }

    println!("Cassettes saved to tests/fixtures/perf/semantic_vcr/cassettes/judge/");
}

/// Record all cassettes in one test.
///
/// This creates cassettes for the semantic_vcr fixture:
/// - embeddings: text-embedding-3-small for "Reference text for similarity."
/// - judge: gpt-4o-mini faithfulness evaluation
#[tokio::test]
#[ignore]
async fn record_all_cassettes() {
    let api_key = std::env::var("OPENAI_API_KEY").expect(
        "OPENAI_API_KEY required for recording.\n\
         Set it with: export OPENAI_API_KEY=sk-...",
    );

    // Use workspace-relative path
    let cassette_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(CASSETTE_DIR);

    println!("Recording cassettes to: {}", cassette_dir.display());

    let mut vcr = VcrClient::new(VcrMode::Record, cassette_dir.clone());
    let auth = format!("Bearer {}", api_key);

    // 1. Record embedding for semantic_similarity_to test
    println!("\n1. Recording embedding cassette...");
    let url = "https://api.openai.com/v1/embeddings";
    let body = json!({
        "input": "Reference text for similarity.",
        "model": "text-embedding-3-small",
        "encoding_format": "float"
    });
    let resp = vcr
        .post_json(url, &body, Some(&auth))
        .await
        .expect("Embedding request failed");
    assert!(resp.is_success(), "Embedding API failed: {:?}", resp.body);
    println!(
        "   ✅ Embedding recorded (status={}, from_cache={})",
        resp.status, resp.from_cache
    );

    // 2. Record judge/LLM for faithfulness test
    println!("\n2. Recording judge cassette...");
    let url = "https://api.openai.com/v1/chat/completions";
    let body = json!({
        "model": "gpt-4o-mini",
        "messages": [{
            "role": "user",
            "content": "Evaluate the faithfulness of this response. Score from 0.0 to 1.0.\n\nResponse: The answer is faithful to the source.\n\nReturn only a JSON object with 'score' and 'reason' fields."
        }],
        "temperature": 0.0,
        "max_tokens": 256
    });
    let resp = vcr
        .post_json(url, &body, Some(&auth))
        .await
        .expect("Judge request failed");
    assert!(resp.is_success(), "Judge API failed: {:?}", resp.body);
    println!(
        "   ✅ Judge recorded (status={}, from_cache={})",
        resp.status, resp.from_cache
    );

    // Verify cassettes exist
    println!("\n3. Verifying cassettes...");
    let embeddings_dir = cassette_dir.join("openai").join("embeddings");
    let judge_dir = cassette_dir.join("openai").join("judge");

    let embedding_files: Vec<_> = std::fs::read_dir(&embeddings_dir)
        .map(|rd| rd.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();
    let judge_files: Vec<_> = std::fs::read_dir(&judge_dir)
        .map(|rd| rd.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();

    println!(
        "   Embeddings: {} cassettes in {}",
        embedding_files.len(),
        embeddings_dir.display()
    );
    println!(
        "   Judge: {} cassettes in {}",
        judge_files.len(),
        judge_dir.display()
    );

    println!("\n✅ All cassettes recorded successfully!");
    println!("\nNext steps:");
    println!("  1. Review cassettes for secrets (should be scrubbed)");
    println!("  2. git add {}", CASSETTE_DIR);
    println!("  3. Commit with: git commit -m 'chore(vcr): add semantic_vcr cassettes'");
}
