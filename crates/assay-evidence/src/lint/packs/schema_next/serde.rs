use crate::lint::Severity;
use serde::Deserialize;

impl assay_common::doc::TextScalar for Severity {
    fn spelling(&self) -> &str {
        match self {
            Severity::Error => "error",
            Severity::Warn => "warning",
            Severity::Info => "info",
        }
    }

    fn parse_spelling(raw: &str) -> Result<Self, String> {
        Severity::deserialize(
            serde::de::value::StrDeserializer::<serde::de::value::Error>::new(raw),
        )
        .map_err(|_| {
            format!(
                "invalid severity '{}'; expected error|warning|warn|info",
                raw
            )
        })
    }
}
