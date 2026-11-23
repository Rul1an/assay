//! The live wrapper accepts static object properties and homogeneous array items.
//! JSON Schema validation still belongs to the policy engine. This positional index
//! identifies names the upstream must receive with exactly their declared spelling.

use super::McpPolicy;
use crate::mcp::unique_json::wire_spelling as spelling;
use serde_json::Value;
use std::{collections::HashMap, io, sync::Arc};

#[derive(Default)]
pub(super) struct WireSchemas(HashMap<String, Shape>);

#[derive(Default)]
struct Shape {
    properties: HashMap<String, (String, Arc<Shape>)>,
    items: Option<Box<Shape>>,
    additional: Option<Arc<Shape>>,
}

fn refuse(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, reason)
}

impl WireSchemas {
    pub(super) fn compile(policy: &McpPolicy) -> io::Result<Self> {
        let mut tools = HashMap::new();
        for (tool, schema) in &policy.schemas {
            if tool != "$defs" {
                let shape = Shape::compile(schema)
                    .map_err(|e| refuse(&format!("wrap schema for {tool}: {e}")))?;
                tools.insert(tool.clone(), shape);
            }
        }
        Ok(Self(tools))
    }

    pub(super) fn accepts(&self, tool: &str, args: &Value) -> bool {
        self.0.get(tool).is_none_or(|shape| shape.accepts(args))
    }
}

fn scalar_test(keyword: &str) -> bool {
    matches!(
        keyword,
        "pattern"
            | "minLength"
            | "maxLength"
            | "format"
            | "minimum"
            | "maximum"
            | "exclusiveMinimum"
            | "exclusiveMaximum"
            | "multipleOf"
    )
}

impl Shape {
    fn compile(schema: &Value) -> io::Result<Self> {
        let mut shape = Self::default();
        if schema.is_boolean() {
            return Ok(shape);
        }
        let object = schema
            .as_object()
            .ok_or_else(|| refuse("expected object or boolean schema"))?;
        for (keyword, rule) in object {
            match keyword.as_str() {
                "properties" => {
                    for (name, member) in rule
                        .as_object()
                        .ok_or_else(|| refuse("expected properties map"))?
                    {
                        shape.insert_property(name, Arc::new(Self::compile(member)?))?;
                    }
                }
                "items" => shape.items = Some(Box::new(Self::compile(rule)?)),
                "additionalProperties" => shape.additional = Some(Arc::new(Self::compile(rule)?)),
                "required" => {}
                "const" if !rule.is_object() && !rule.is_array() => {}
                "enum"
                    if rule.as_array().is_some_and(|values| {
                        values.iter().all(|v| !v.is_object() && !v.is_array())
                    }) => {}
                "not"
                    if rule
                        .as_object()
                        .is_some_and(|fields| fields.keys().all(|k| scalar_test(k))) => {}
                "type" | "title" | "description" | "examples" | "default" | "$comment"
                | "$defs" | "minItems" | "maxItems" | "minProperties" | "maxProperties" => {}
                k if scalar_test(k) => {}
                _ => {
                    return Err(refuse(&format!(
                        "unsupported live schema keyword {keyword}"
                    )))
                }
            }
        }
        if let Some(required) = schema.get("required") {
            for item in required
                .as_array()
                .ok_or_else(|| refuse("expected required array"))?
            {
                let name = item
                    .as_str()
                    .ok_or_else(|| refuse("expected required member name"))?;
                if shape
                    .properties
                    .get(&spelling(name))
                    .is_none_or(|(exact, _)| exact != name)
                {
                    shape.insert_property(name, shape.additional.clone().unwrap_or_default())?;
                }
            }
        }
        Ok(shape)
    }

    fn insert_property(&mut self, name: &str, member: Arc<Shape>) -> io::Result<()> {
        let folded = spelling(name);
        if !name.is_ascii()
            || folded.is_empty()
            || name.contains(char::is_control)
            || self
                .properties
                .insert(folded, (name.to_owned(), member))
                .is_some()
        {
            return Err(refuse("ambiguous or unsupported live schema property name"));
        }
        Ok(())
    }

    fn accepts(&self, value: &Value) -> bool {
        match value {
            Value::Object(fields) => fields.iter().all(|(name, value)| {
                if self.properties.is_empty() {
                    return self.additional.as_ref().is_none_or(|s| s.accepts(value));
                }
                if !name.is_ascii() {
                    return false;
                }
                match self.properties.get(&spelling(name)) {
                    Some((exact, s)) => name == exact && s.accepts(value),
                    None => self.additional.as_ref().is_none_or(|s| s.accepts(value)),
                }
            }),
            Value::Array(items) => self
                .items
                .as_ref()
                .is_none_or(|s| items.iter().all(|v| s.accepts(v))),
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn implicit_required_properties_share_the_additional_shape() {
        let shape = Shape::compile(&json!({
            "required": ["left", "right"],
            "additionalProperties": {"properties": {"label": {"type": "string"}}}
        }))
        .unwrap();
        let left = &shape.properties["left"].1.properties;
        let right = &shape.properties["right"].1.properties;
        assert!(std::ptr::eq(left, right));
    }

    #[test]
    fn wrap_checks_static_names_once_at_their_schema_position() {
        let mut policy = McpPolicy::default();
        policy.schemas.insert("example".into(), json!({"properties":{"path":{"type":"string"}, "rows":{"items":{"properties":{"label":{"type":"string"}}}}}}));
        let schema = WireSchemas::compile(&policy).unwrap();
        assert!(schema.accepts(
            "example",
            &json!({"path":"demo", "env":{"PATH":"a","path":"b"}, "rows":[{"label":"ok"}]})
        ));
        assert!(!schema.accepts("example", &json!({"rows":[{"Label":"demo"}]})));
        policy.schemas.insert(
            "example".into(),
            json!({"type":"object","patternProperties":{"^label":{"type":"string"}}}),
        );
        assert!(WireSchemas::compile(&policy).is_err());
    }
}
