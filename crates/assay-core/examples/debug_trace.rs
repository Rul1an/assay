use std::io::Write;

use assay_core::providers::llm::LlmClient;
use assay_core::providers::trace::TraceClient;

fn main() -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        // Create temp file manually
        let path = std::path::Path::new("debug_trace.jsonl");
        let mut file = std::fs::File::create(path)?;
        let json = r#"{"schema_version":1,"type":"assay.trace","request_id":"test-1","prompt":"Say hello","response":"Hello world","meta":{"assay":{"embeddings":{"model":"text-embedding-3-small","response":[0.1],"reference":[0.1]}}}}"#;
        writeln!(file, "{}", json)?;

        println!("Created trace file: {}", path.display());

        let client = TraceClient::from_path(path)?;
        let resp = client.complete("Say hello", None).await?;

        println!("---------------------------------------------------");
        println!("Response Text: {}", resp.text);
        println!("Response Meta: {}", resp.meta);
        println!("---------------------------------------------------");

        if resp.meta.pointer("/assay/embeddings/response").is_some() {
            println!("SUCCESS: Embeddings found in meta!");
        } else {
            println!("FAILURE: Embeddings MISSING in meta!");
        }

        std::fs::remove_file(path)?;
        Ok(())
    })
}
