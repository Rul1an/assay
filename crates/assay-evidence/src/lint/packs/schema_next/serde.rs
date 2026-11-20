use crate::lint::Severity;
use serde::de::Error as _;
use serde::Deserialize;

/// Check duplicate members while reading opaque JSON, before collecting a map loses them.
pub(crate) fn deserialize_check_json<'de, D>(
    deserializer: D,
) -> Result<Option<serde_json::Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<assay_common::unique_value::UniqueValue>::deserialize(deserializer)?
        .map(assay_common::unique_value::UniqueValue::into_native_value)
        .transpose()
        .map_err(D::Error::custom)
}

/// Future checks may use arbitrary YAML keys. Validate the mapping before discarding it.
pub(crate) fn deserialize_future_payload<'de, D>(deserializer: D) -> Result<(), D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct FuturePayload;
    impl<'de> serde::de::Visitor<'de> for FuturePayload {
        type Value = ();
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a mapping or an empty positional payload")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<(), A::Error> {
            <serde_yaml::Value as serde::Deserialize>::deserialize(
                serde::de::value::MapAccessDeserializer::new(map),
            )
            .map(|_| ())
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, _seq: A) -> Result<(), A::Error> {
            // The internally tagged decoder checks that the visitor consumed the
            // whole sequence. Consume zero arguments, as the unit variant did.
            Ok(())
        }
    }
    deserializer.deserialize_any(FuturePayload)
}

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
