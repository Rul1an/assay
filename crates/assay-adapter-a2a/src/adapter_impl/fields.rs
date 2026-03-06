use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;

use super::DEFAULT_TIME_SECS;

pub(super) fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(ToOwned::to_owned)
}

pub(super) fn nested_string_field(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str().map(ToOwned::to_owned)
}

pub(super) fn nested_string_array_field(value: &Value, path: &[&str]) -> Option<Vec<String>> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }

    let arr = current.as_array()?;
    let mut values: Vec<String> = arr
        .iter()
        .filter_map(|item| item.as_str().map(ToOwned::to_owned))
        .collect();
    values.sort();
    Some(values)
}

pub(super) fn timestamp_field(value: &Value, key: &str) -> Option<DateTime<Utc>> {
    let raw = value.get(key)?.as_str()?;
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

pub(super) fn default_time() -> DateTime<Utc> {
    Utc.timestamp_opt(DEFAULT_TIME_SECS, 0)
        .single()
        .expect("default timestamp must be valid")
}
