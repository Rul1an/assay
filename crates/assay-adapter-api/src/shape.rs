use serde_json::Value;

use crate::{AdapterError, AdapterErrorKind, AdapterResult};

/// Enforce shared JSON shape limits for adapter inputs.
pub fn validate_json_shape(
    value: &Value,
    max_json_depth: Option<u64>,
    max_array_length: Option<u64>,
) -> AdapterResult<()> {
    visit(value, 1, max_json_depth, max_array_length)
}

fn visit(
    value: &Value,
    depth: u64,
    max_json_depth: Option<u64>,
    max_array_length: Option<u64>,
) -> AdapterResult<()> {
    if let Some(limit) = max_json_depth {
        if depth > limit {
            return Err(AdapterError::new(
                AdapterErrorKind::Measurement,
                format!("payload exceeds max_json_depth ({limit})"),
            ));
        }
    }

    match value {
        Value::Array(values) => {
            if let Some(limit) = max_array_length {
                if values.len() as u64 > limit {
                    return Err(AdapterError::new(
                        AdapterErrorKind::Measurement,
                        format!("payload exceeds max_array_length ({limit})"),
                    ));
                }
            }
            for item in values {
                visit(item, depth + 1, max_json_depth, max_array_length)?;
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                visit(item, depth + 1, max_json_depth, max_array_length)?;
            }
        }
        _ => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::validate_json_shape;
    use crate::AdapterErrorKind;

    #[test]
    fn json_shape_rejects_excessive_depth() {
        let payload = json!({
            "a": {
                "b": {
                    "c": "too-deep"
                }
            }
        });

        let err = validate_json_shape(&payload, Some(3), None).unwrap_err();
        assert_eq!(err.kind, AdapterErrorKind::Measurement);
        assert!(err.message.contains("max_json_depth"));
    }

    #[test]
    fn json_shape_rejects_excessive_array_length() {
        let payload = json!({
            "items": [1, 2, 3]
        });

        let err = validate_json_shape(&payload, None, Some(2)).unwrap_err();
        assert_eq!(err.kind, AdapterErrorKind::Measurement);
        assert!(err.message.contains("max_array_length"));
    }
}
