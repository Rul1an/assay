//! YAML parsing and normalization for canonical pack format.

use std::{
    borrow::ToOwned,
    format,
    string::{String, ToString},
    vec::Vec,
};

use serde_json::Value as JsonValue;

pub mod errors;

use self::errors::{
    ReadError, ReadResult, MAX_DEPTH, MAX_KEYS_PER_MAPPING, MAX_SAFE_INTEGER, MAX_STRING_LENGTH,
    MAX_TOTAL_SIZE, MIN_SAFE_INTEGER,
};

/// A document that passed the registry's strict YAML and integer-only value checks.
///
/// ```compile_fail
/// assay_common::closed! { canonical_registry struct Doc }
/// let invalid = Doc(serde_json::json!(0.5));
/// ```
pub struct RegistryDocument(JsonValue);

impl RegistryDocument {
    pub fn read(content: &str) -> ReadResult<Self> {
        read(content).map(Self)
    }

    pub fn into_value(self) -> JsonValue {
        self.0
    }
}

fn read(content: &str) -> ReadResult<JsonValue> {
    // Pre-check: input size
    if content.len() > MAX_TOTAL_SIZE {
        return Err(ReadError::InputTooLarge {
            size: content.len(),
        });
    }

    // Pre-scan for forbidden patterns
    pre_scan_yaml(content)?;

    // Parse YAML to intermediate value
    let yaml_value: serde_yaml::Value =
        crate::yaml_input::parse_tree(content.as_bytes()).map_err(|e| ReadError::ParseError {
            message: e.to_string(),
        })?;

    // Convert to JSON and validate
    let json_value = yaml_to_json(&yaml_value, 0)?;

    Ok(json_value)
}

#[path = "yaml_tokens.rs"]
mod yaml_tokens;

fn pre_scan_yaml(content: &str) -> ReadResult<()> {
    use libyaml_rs::yaml_token_type_t::*;

    // Block duplicates have a dedicated error. Flow duplicates retain the value
    // decoder's located ParseError.
    let mut scopes: Vec<Option<std::collections::HashSet<String>>> = Vec::new();
    let mut after_key = false;
    yaml_tokens::scan(content, |token| {
        match token.kind() {
            YAML_DOCUMENT_START_TOKEN | YAML_DOCUMENT_END_TOKEN => {
                return Err(ReadError::MultiDocumentFound);
            }
            YAML_ANCHOR_TOKEN => {
                return Err(ReadError::AnchorFound {
                    position: token.position(),
                })
            }
            YAML_ALIAS_TOKEN => {
                return Err(ReadError::AliasFound {
                    position: token.position(),
                })
            }
            YAML_TAG_TOKEN => return Err(ReadError::TagFound { tag: token.tag() }),
            YAML_BLOCK_MAPPING_START_TOKEN => scopes.push(Some(Default::default())),
            YAML_BLOCK_SEQUENCE_START_TOKEN
            | YAML_FLOW_MAPPING_START_TOKEN
            | YAML_FLOW_SEQUENCE_START_TOKEN => scopes.push(None),
            YAML_BLOCK_END_TOKEN | YAML_FLOW_MAPPING_END_TOKEN | YAML_FLOW_SEQUENCE_END_TOKEN => {
                scopes.pop();
            }
            YAML_SCALAR_TOKEN if after_key => {
                if let Some(Some(keys)) = scopes.last_mut() {
                    let key = token.scalar();
                    if !keys.insert(key.to_owned()) {
                        return Err(ReadError::DuplicateKey {
                            key: key.to_owned(),
                        });
                    }
                }
            }
            _ => {}
        }
        after_key = token.kind() == YAML_KEY_TOKEN;
        Ok(())
    })
}

/// Convert YAML value to JSON value with validation.
fn yaml_to_json(yaml: &serde_yaml::Value, depth: usize) -> ReadResult<JsonValue> {
    // Check depth limit
    if depth > MAX_DEPTH {
        return Err(ReadError::MaxDepthExceeded { depth });
    }

    match yaml {
        serde_yaml::Value::Null => Ok(JsonValue::Null),

        serde_yaml::Value::Bool(b) => Ok(JsonValue::Bool(*b)),

        serde_yaml::Value::Number(n) => {
            // Check for float
            if n.is_f64() {
                return Err(ReadError::FloatNotAllowed {
                    value: n.to_string(),
                });
            }

            // Check integer range
            if let Some(i) = n.as_i64() {
                if !(MIN_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&i) {
                    return Err(ReadError::IntegerOutOfRange { value: i });
                }
                Ok(JsonValue::Number(serde_json::Number::from(i)))
            } else if let Some(u) = n.as_u64() {
                if u > MAX_SAFE_INTEGER as u64 {
                    return Err(ReadError::IntegerOutOfRange { value: u as i64 });
                }
                Ok(JsonValue::Number(serde_json::Number::from(u)))
            } else {
                Err(ReadError::FloatNotAllowed {
                    value: n.to_string(),
                })
            }
        }

        serde_yaml::Value::String(s) => {
            // Check string length
            if s.len() > MAX_STRING_LENGTH {
                return Err(ReadError::StringTooLong { length: s.len() });
            }
            Ok(JsonValue::String(s.clone()))
        }

        serde_yaml::Value::Sequence(seq) => {
            let items: ReadResult<Vec<JsonValue>> = seq
                .iter()
                .map(|item| yaml_to_json(item, depth + 1))
                .collect();
            Ok(JsonValue::Array(items?))
        }

        serde_yaml::Value::Mapping(map) => {
            // Check key count
            if map.len() > MAX_KEYS_PER_MAPPING {
                return Err(ReadError::MaxKeysExceeded { count: map.len() });
            }

            let mut json_map = serde_json::Map::new();
            let mut seen_keys = std::collections::HashSet::new();

            for (key, value) in map {
                // Keys must be strings
                let key_str = match key {
                    serde_yaml::Value::String(s) => s.clone(),
                    _ => {
                        return Err(ReadError::ParseError {
                            message: format!("non-string key: {:?}", key),
                        })
                    }
                };

                // Reject YAML merge keys (<<) — can cause duplicate-key-by-construction
                if key_str == "<<" {
                    return Err(ReadError::ParseError {
                        message: "reason=merge_key_not_allowed: merge keys (<<) not allowed"
                            .to_string(),
                    });
                }

                // Check for duplicate keys
                if !seen_keys.insert(key_str.clone()) {
                    return Err(ReadError::DuplicateKey { key: key_str });
                }

                let json_value = yaml_to_json(value, depth + 1)?;
                json_map.insert(key_str, json_value);
            }

            Ok(JsonValue::Object(json_map))
        }

        // Tagged values are not allowed
        serde_yaml::Value::Tagged(tagged) => Err(ReadError::TagFound {
            tag: format!("{:?}", tagged.tag),
        }),
    }
}
