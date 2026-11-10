use crate::lint::Severity;
use serde::de::Error as _;
use serde::Deserialize;

pub(crate) fn serialize_pack_severity<S>(
    severity: &Severity,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let value = match severity {
        Severity::Error => "error",
        Severity::Warn => "warning",
        Severity::Info => "info",
    };
    serializer.serialize_str(value)
}

/// Refuse any field beside `type` for a check that takes no parameters.
///
/// Serde reads the rest of an internally tagged unit variant and ignores it, even under
/// `deny_unknown_fields`, so a parameterless check would otherwise accept a misplaced key such as
/// `event_types` written under `check:`.
pub(crate) fn deserialize_no_fields<'de, D>(deserializer: D) -> Result<(), D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct NoFields {}

    NoFields::deserialize(deserializer).map(|NoFields {}| ())
}

pub(crate) fn deserialize_pack_severity<'de, D>(deserializer: D) -> Result<Severity, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    match raw.as_str() {
        "error" | "Error" => Ok(Severity::Error),
        "warning" | "Warning" | "warn" | "Warn" => Ok(Severity::Warn),
        "info" | "Info" => Ok(Severity::Info),
        _ => Err(D::Error::custom(format!(
            "invalid severity '{}'; expected error|warning|warn|info",
            raw
        ))),
    }
}
