use assay_core::cache::key::cache_key;
use assay_core::providers::llm::LlmClient;
use assay_core::providers::trace::TraceClient;
use std::io::Write;
use tempfile::NamedTempFile;

#[tokio::test]
async fn test_cache_key_trace_sensitivity() -> anyhow::Result<()> {
    // 1. Setup two trace files with SAME prompt but DIFFERENT response/content.
    let mut t1 = NamedTempFile::new()?;
    writeln!(
        t1,
        r#"{{"type":"episode_start","episode_id":"1","timestamp":0,"input":{{"prompt":"test"}},"meta":{{}}}}"#
    )?;
    writeln!(
        t1,
        r#"{{"type":"step","episode_id":"1","step_id":"s1","kind":"llm_completion","timestamp":10,"content":"A"}}"#
    )?;
    writeln!(
        t1,
        r#"{{"type":"episode_end","episode_id":"1","timestamp":20}}"#
    )?;

    let mut t2 = NamedTempFile::new()?;
    writeln!(
        t2,
        r#"{{"type":"episode_start","episode_id":"1","timestamp":0,"input":{{"prompt":"test"}},"meta":{{}}}}"#
    )?;
    writeln!(
        t2,
        r#"{{"type":"step","episode_id":"1","step_id":"s1","kind":"llm_completion","timestamp":10,"content":"B"}}"#
    )?;
    writeln!(
        t2,
        r#"{{"type":"episode_end","episode_id":"1","timestamp":20}}"#
    )?;

    // 2. Load Clients
    let c1 = TraceClient::from_path(t1.path())?;
    let c2 = TraceClient::from_path(t2.path())?;

    // 3. Compute Cache Keys
    // Model, Prompt, Fingerprint(Config) are identical.
    // Client Trace Fingerprint differs.
    let fp_config = "config_hash_constant";
    let key1 = cache_key("trace", "test", fp_config, c1.fingerprint().as_deref());
    let key2 = cache_key("trace", "test", fp_config, c2.fingerprint().as_deref());

    assert_ne!(
        key1, key2,
        "Cache keys must differ if trace content differs"
    );

    // 4. Verify same file yields same key
    let c1_dup = TraceClient::from_path(t1.path())?;
    let key1_dup = cache_key("trace", "test", fp_config, c1_dup.fingerprint().as_deref());
    assert_eq!(key1, key1_dup, "Cache keys must be stable for same content");

    Ok(())
}
