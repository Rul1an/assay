use serde_json::Value;

struct LocalOnlyRetriever;

impl jsonschema::Retrieve for LocalOnlyRetriever {
    fn retrieve(
        &self,
        _uri: &jsonschema::Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        Err("external JSON Schema retrieval is disabled".into())
    }
}

pub(crate) fn compile(schema: &Value) -> Result<jsonschema::Validator, String> {
    jsonschema::options()
        .with_retriever(LocalOnlyRetriever)
        .build(schema)
        .map_err(|error| error.to_string())
}
