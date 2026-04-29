use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use super::jcs;

/// Digest algorithm used by P56b tool-definition bindings.
pub const TOOL_DEFINITION_DIGEST_ALG_SHA256: &str = "sha256";
/// Canonicalization applied before computing `tool_definition_digest`.
pub const TOOL_DEFINITION_CANONICALIZATION_JCS_MCP_TOOL_DEFINITION_V1: &str =
    "jcs:mcp_tool_definition.v1";
/// Bounded schema tag for the supported MCP tool-definition snapshot projection.
pub const TOOL_DEFINITION_SCHEMA_V1: &str = "assay.mcp.tool-definition.snapshot.v1";
/// Supported source surface for P56b v1 tool-definition bindings.
pub const TOOL_DEFINITION_SOURCE_MCP_TOOLS_LIST: &str = "mcp.tools/list";

/// Self-describing digest over a bounded MCP tool-definition projection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolDefinitionBinding {
    pub digest: String,
    pub digest_alg: String,
    pub canonicalization: String,
    pub schema: String,
    pub source: String,
}

impl ToolDefinitionBinding {
    fn from_digest(digest: String) -> Self {
        Self {
            digest,
            digest_alg: TOOL_DEFINITION_DIGEST_ALG_SHA256.to_string(),
            canonicalization: TOOL_DEFINITION_CANONICALIZATION_JCS_MCP_TOOL_DEFINITION_V1
                .to_string(),
            schema: TOOL_DEFINITION_SCHEMA_V1.to_string(),
            source: TOOL_DEFINITION_SOURCE_MCP_TOOLS_LIST.to_string(),
        }
    }
}

/// Compute a P56b binding from an observed `tools/list` tool definition.
///
/// Returns `Ok(None)` when the observed value is well-formed but is not a
/// supported bounded v1 tool definition. Malformed or otherwise invalid tool
/// definitions may return `Err` during normalization or projection. This
/// function never reconstructs a definition from `tools/call` data and never
/// imports unsupported top-level fields.
pub fn binding_from_tools_list_tool(
    tool: &Value,
    server_id: Option<&str>,
) -> Result<Option<ToolDefinitionBinding>> {
    let Some(projection) = canonical_tool_definition_projection(tool, server_id)? else {
        return Ok(None);
    };
    let canonical = jcs::to_vec(&projection)?;
    let hash = Sha256::digest(&canonical);
    Ok(Some(ToolDefinitionBinding::from_digest(format!(
        "sha256:{}",
        hex::encode(hash)
    ))))
}

/// Build the bounded canonical P56b v1 projection.
pub fn canonical_tool_definition_projection(
    tool: &Value,
    server_id: Option<&str>,
) -> Result<Option<Value>> {
    let Some(tool_object) = tool.as_object() else {
        return Ok(None);
    };

    let Some(name) = tool_object.get("name").and_then(Value::as_str) else {
        return Ok(None);
    };
    if name.trim().is_empty() {
        return Ok(None);
    }

    let mut projection = Map::new();
    projection.insert("name".to_string(), Value::String(name.to_string()));

    if let Some(description) = tool_object.get("description").and_then(Value::as_str) {
        let trimmed = description.trim();
        if !trimmed.is_empty() {
            projection.insert(
                "description".to_string(),
                Value::String(trimmed.to_string()),
            );
        }
    }

    if let Some(input_schema) = normalized_input_schema(tool_object)? {
        projection.insert("input_schema".to_string(), input_schema);
    }

    if let Some(server_id) = normalized_server_id(server_id) {
        projection.insert(
            "server_id".to_string(),
            Value::String(server_id.to_string()),
        );
    }

    Ok(Some(Value::Object(projection)))
}

fn normalized_input_schema(tool_object: &Map<String, Value>) -> Result<Option<Value>> {
    let schema = tool_object
        .get("inputSchema")
        .or_else(|| tool_object.get("input_schema"));

    match schema {
        Some(value) if value.is_object() => Ok(Some(value.clone())),
        Some(_) => bail!("unsupported MCP tool definition: input schema must be a JSON object"),
        None => Ok(None),
    }
}

fn normalized_server_id(server_id: Option<&str>) -> Option<&str> {
    server_id.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::super::signing::SIG_FIELD;
    use super::*;
    use serde_json::json;

    fn binding(tool: Value) -> ToolDefinitionBinding {
        binding_from_tools_list_tool(&tool, Some("server-a"))
            .expect("binding should compute")
            .expect("tool should be supported")
    }

    #[test]
    fn projection_allowlist_excludes_unknown_top_level_fields() {
        let tool = json!({
            "name": "read_file",
            "description": "Read a file",
            "inputSchema": {"type": "object"},
            "annotations": {"title": "Read"},
            "display_hint": "danger",
            "provider_metadata": {"opaque": true}
        });

        let projection = canonical_tool_definition_projection(&tool, Some("server-a"))
            .unwrap()
            .unwrap();

        assert_eq!(
            projection,
            json!({
                "name": "read_file",
                "description": "Read a file",
                "input_schema": {"type": "object"},
                "server_id": "server-a"
            })
        );
    }

    #[test]
    fn digest_is_stable_across_top_level_key_order() {
        let first = binding(json!({
            "name": "read_file",
            "description": "Read a file",
            "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}}
        }));
        let second = binding(json!({
            "inputSchema": {"properties": {"path": {"type": "string"}}, "type": "object"},
            "description": "Read a file",
            "name": "read_file"
        }));

        assert_eq!(first.digest, second.digest);
    }

    #[test]
    fn input_schema_spelling_normalizes_to_same_digest() {
        let camel = binding(json!({
            "name": "read_file",
            "inputSchema": {"type": "object"}
        }));
        let snake = binding(json!({
            "name": "read_file",
            "input_schema": {"type": "object"}
        }));

        assert_eq!(camel.digest, snake.digest);
    }

    #[test]
    fn description_is_trimmed_and_whitespace_only_is_absent() {
        let trimmed = binding(json!({
            "name": "read_file",
            "description": "  Read a file  ",
            "inputSchema": {"type": "object"}
        }));
        let clean = binding(json!({
            "name": "read_file",
            "description": "Read a file",
            "inputSchema": {"type": "object"}
        }));
        let whitespace = canonical_tool_definition_projection(
            &json!({
                "name": "read_file",
                "description": "   \t\n ",
                "inputSchema": {"type": "object"}
            }),
            None,
        )
        .unwrap()
        .unwrap();

        assert_eq!(trimmed.digest, clean.digest);
        assert!(whitespace.get("description").is_none());
    }

    #[test]
    fn server_id_is_only_included_when_supplied() {
        let tool = json!({
            "name": "read_file",
            "inputSchema": {"type": "object"}
        });

        let unscoped = canonical_tool_definition_projection(&tool, None)
            .unwrap()
            .unwrap();
        let empty_scoped = canonical_tool_definition_projection(&tool, Some("   "))
            .unwrap()
            .unwrap();
        let scoped = canonical_tool_definition_projection(&tool, Some("server-a"))
            .unwrap()
            .unwrap();

        assert!(unscoped.get("server_id").is_none());
        assert!(empty_scoped.get("server_id").is_none());
        assert_eq!(scoped["server_id"], "server-a");
    }

    #[test]
    fn signature_field_does_not_affect_digest() {
        let unsigned = binding(json!({
            "name": "read_file",
            "description": "Read",
            "inputSchema": {"type": "object"}
        }));
        let signed = binding(json!({
            "name": "read_file",
            "description": "Read",
            "inputSchema": {"type": "object"},
            SIG_FIELD: {"signature": "opaque"}
        }));

        assert_eq!(unsigned.digest, signed.digest);
    }

    #[test]
    fn vendor_schema_keywords_inside_input_schema_are_preserved() {
        let projection = canonical_tool_definition_projection(
            &json!({
                "name": "read_file",
                "inputSchema": {
                    "type": "object",
                    "x-vendor-keyword": {"opaque": true}
                },
                "x-vendor-top-level": "excluded"
            }),
            None,
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            projection["input_schema"]["x-vendor-keyword"],
            json!({"opaque": true})
        );
        assert!(projection.get("x-vendor-top-level").is_none());
    }

    #[test]
    fn invalid_or_missing_name_does_not_invent_binding() {
        assert!(
            binding_from_tools_list_tool(&json!({"inputSchema": {"type": "object"}}), None)
                .unwrap()
                .is_none()
        );
        assert!(binding_from_tools_list_tool(&json!({"name": "   "}), None)
            .unwrap()
            .is_none());
    }

    #[test]
    fn non_object_input_schema_is_not_supported() {
        assert!(binding_from_tools_list_tool(
            &json!({"name": "read_file", "inputSchema": true}),
            None
        )
        .is_err());
    }
}
