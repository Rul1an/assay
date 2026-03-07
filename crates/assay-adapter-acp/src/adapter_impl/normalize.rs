use serde_json::{Map, Value};

pub(crate) fn normalize_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            let mut normalized = Map::new();
            for key in keys {
                normalized.insert(key.clone(), normalize_json(&map[key]));
            }
            Value::Object(normalized)
        }
        Value::Array(values) => Value::Array(values.iter().map(normalize_json).collect()),
        _ => value.clone(),
    }
}
