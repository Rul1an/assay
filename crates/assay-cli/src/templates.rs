pub const CI_EVAL_YAML: &str = r#"version: 1
suite: "ci_smoke"
model: "trace"
tests:
  - id: "ci_smoke_regex"
    input:
      prompt: "ci_regex"
    expected:
      type: regex_match
      pattern: "Hello\\s+CI"
      flags: ["i"]
  - id: "ci_smoke_schema"
    input:
      prompt: "ci_schema"
    expected:
      type: json_schema
      json_schema: "{}"
      schema_file: "schemas/ci_answer.schema.json"
  - id: "ci_smoke_semantic"
    input:
      prompt: "ci_semantic"
    expected:
      type: semantic_similarity_to
      text: "Hello Semantic"
      threshold: 0.99
"#;

pub const CI_SCHEMA_JSON: &str = r#"{
  "type": "object",
  "required": ["answer"],
  "properties": {
    "answer": { "type": "string" }
  },
  "additionalProperties": false
}"#;

pub const CI_TRACES_JSONL: &str = r#"{"schema_version": 1, "type": "assay.trace", "request_id": "ci_1", "prompt": "ci_regex", "response": "hello   ci", "model": "trace", "provider": "trace"}
{"schema_version": 1, "type": "assay.trace", "request_id": "ci_2", "prompt": "ci_schema", "response": "{\"answer\":\"ok\"}", "model": "trace", "provider": "trace"}
{"schema_version": 1, "type": "assay.trace", "request_id": "ci_3", "prompt": "ci_semantic", "response": "Hello Semantic", "model": "trace", "provider": "trace", "meta": {"assay": {"embeddings": {"model":"trace-embed","response":[1.0,0.0,0.0],"reference":[1.0,0.0,0.0],"source_response":"trace","source_reference":"trace"}}}}
"#;

pub const CI_WORKFLOW_YML: &str = r#"name: Assay Gate
on: [push, pull_request]
jobs:
  assay:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Run Assay Smoke Test
        uses: Rul1an/assay-action@v1.0.0
        with:
          assay_version: "v0.1.0" # Update to latest
          config: ci-eval.yaml
          trace_file: traces/ci.jsonl
          strict: "true"
"#;

pub const GITIGNORE: &str = "/.eval/\n/out/\n*.db\n*.db-shm\n*.db-wal\n/assay\n";
