use super::errors::{from_reader, CanonicalizeResult};

assay_common::closed! { canonical_registry struct RegistryDocument }

/// Parse the strict YAML subset defined by SPEC-Pack-Registry-v1 §6.1.
pub fn parse_yaml_strict(content: &str) -> CanonicalizeResult<serde_json::Value> {
    RegistryDocument::read(content)
        .map(RegistryDocument::into_value)
        .map_err(from_reader)
}
