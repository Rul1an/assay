// Downstream witness for Rul1an/assay#3176. This file is byte-identical in every variant;
// only the manifest (which assay-core, which jsonschema) differs.
use std::collections::HashMap;
use std::sync::Arc;

fn main() {
    let schema = serde_json::json!({"type": "object", "required": ["path"]});
    let args = serde_json::json!({"path": "/tmp/witness"});

    // 1. A validator built from the consumer's own direct jsonschema dependency,
    //    passed into the public assay-core API.
    let own: jsonschema::Validator = jsonschema::validator_for(&schema).expect("schema compiles");
    let verdict = assay_core::policy_engine::evaluate_schema(&own, &args);
    println!("evaluate_schema status: {:?}", verdict.status);

    // 2. Validators returned by the public assay-core API, bound to the consumer's type.
    let policy = assay_core::mcp::policy::McpPolicy::new();
    let compiled: HashMap<String, Arc<jsonschema::Validator>> =
        policy.try_compile_all_schemas().expect("empty policy compiles");
    for validator in compiled.values() {
        let _ = validator.is_valid(&args);
    }
    println!("try_compile_all_schemas: {} validators", compiled.len());
}
