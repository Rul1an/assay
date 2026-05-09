use crate::mcp::identity::ToolIdentity;
use crate::mcp::tool_definition::{binding_from_tools_list_tool, ToolDefinitionBinding};

pub(super) struct ToolDefinitionObservation {
    pub(super) name: String,
    pub(super) identity: ToolIdentity,
    pub(super) binding: Option<ToolDefinitionBinding>,
}

pub(super) fn observe_tool_definition(
    tool: &mut serde_json::Value,
    server_id: &str,
) -> Option<ToolDefinitionObservation> {
    let name = tool.get("name").and_then(|n| n.as_str())?;
    if name.trim().is_empty() {
        return None;
    }
    let name = name.to_string();
    let description = tool
        .get("description")
        .and_then(|d| d.as_str())
        .map(|s| s.to_string());
    let input_schema = tool
        .get("inputSchema")
        .or_else(|| tool.get("input_schema"))
        .cloned();

    let identity = ToolIdentity::new(server_id, &name, &input_schema, &description);
    let binding = binding_from_tools_list_tool(tool, Some(server_id))
        .ok()
        .flatten();

    // Augment the response with the computed identity for downstream/logging.
    tool.as_object_mut().and_then(|m| {
        m.insert(
            "tool_identity".to_string(),
            serde_json::to_value(&identity).unwrap(),
        )
    });

    Some(ToolDefinitionObservation {
        name,
        identity,
        binding,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observe_tool_definition_computes_identity_and_binding() {
        let mut tool = serde_json::json!({
            "name": "read_file",
            "description": " Read files ",
            "inputSchema": {"type": "object"},
            "annotations": {"title": "Read"},
            "x-assay-sig": {"signature": "opaque"}
        });

        let observation = observe_tool_definition(&mut tool, "server-a")
            .expect("supported tool definition should be observed");

        assert_eq!(observation.name, "read_file");
        assert_eq!(observation.identity.server_id, "server-a");
        assert!(observation.binding.is_some());
        assert!(tool.get("tool_identity").is_some());
    }

    #[test]
    fn proxy_contract_observe_tool_definition_rejects_empty_names() {
        let mut tool = serde_json::json!({
            "name": "   ",
            "description": "invalid",
            "inputSchema": {"type": "object"}
        });

        let observation = observe_tool_definition(&mut tool, "server-a");

        assert!(observation.is_none());
        assert!(tool.get("tool_identity").is_none());
    }
}
