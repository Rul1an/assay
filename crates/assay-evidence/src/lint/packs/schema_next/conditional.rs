use super::types::{CheckDefinition, SupportedConditionalCheck, SupportedConditionalClause};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConditionalCondition {
    all: Vec<RawConditionalClause>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConditionalClause {
    path: String,
    equals: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConditionalThen {
    #[serde(rename = "type")]
    type_name: String,
    paths: Vec<String>,
}

impl CheckDefinition {
    /// Check if this is an unsupported/future check type.
    pub fn is_unsupported(&self) -> bool {
        matches!(self, CheckDefinition::Unsupported)
            || matches!(self, CheckDefinition::Conditional { .. } if self.supported_conditional().is_err())
    }

    /// Parse the narrow conditional subset supported in engine v1.1.
    pub fn supported_conditional(&self) -> Result<SupportedConditionalCheck, String> {
        let (condition, then_check) = match self {
            CheckDefinition::Conditional {
                condition,
                then_check,
            } => (condition.as_ref(), then_check.as_ref()),
            _ => return Err("check is not conditional".to_string()),
        };

        let raw_condition: RawConditionalCondition = serde_json::from_value(
            condition
                .cloned()
                .ok_or_else(|| "missing condition".to_string())?,
        )
        .map_err(|err| format!("unsupported condition shape: {err}"))?;

        if raw_condition.all.is_empty() {
            return Err("condition.all must contain at least one clause".to_string());
        }

        let mut clauses = Vec::with_capacity(raw_condition.all.len());
        for clause in raw_condition.all {
            if clause.path.is_empty() {
                return Err("conditional clause path cannot be empty".to_string());
            }
            if clause.equals.is_array() || clause.equals.is_object() {
                return Err("conditional clause equals must be a JSON scalar or null".to_string());
            }
            clauses.push(SupportedConditionalClause {
                path: clause.path,
                equals: clause.equals,
            });
        }

        let raw_then: RawConditionalThen = serde_json::from_value(
            then_check
                .cloned()
                .ok_or_else(|| "missing then check".to_string())?,
        )
        .map_err(|err| format!("unsupported then shape: {err}"))?;

        if raw_then.type_name != "json_path_exists" {
            return Err("conditional then must use json_path_exists".to_string());
        }
        if raw_then.paths.len() != 1 {
            return Err(
                "conditional then json_path_exists must contain exactly one required path"
                    .to_string(),
            );
        }

        let required_path = raw_then.paths.into_iter().next().unwrap_or_default();
        if required_path.is_empty() {
            return Err("conditional then path cannot be empty".to_string());
        }

        Ok(SupportedConditionalCheck {
            clauses,
            required_path,
        })
    }
}
