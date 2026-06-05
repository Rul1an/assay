use super::{ConstraintParam, ConstraintRule};
use serde::Deserialize;
use std::collections::BTreeMap;

// Dual-Shape Deserializer Helper (Legacy)
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ConstraintsCompat {
    List(Vec<ConstraintRule>),
    Map(BTreeMap<String, BTreeMap<String, InputParamConstraint>>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum InputParamConstraint {
    Direct(String),
    Object(ConstraintParam),
}

pub(super) fn deserialize_constraints<'de, D>(d: D) -> Result<Vec<ConstraintRule>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let c = Option::<ConstraintsCompat>::deserialize(d)?;
    let out = match c {
        None => vec![],
        Some(ConstraintsCompat::List(v)) => v,
        Some(ConstraintsCompat::Map(m)) => m
            .into_iter()
            .map(|(tool, params)| {
                let new_params = params
                    .into_iter()
                    .map(|(arg, val)| {
                        let param = match val {
                            InputParamConstraint::Direct(s) => ConstraintParam { matches: Some(s) },
                            InputParamConstraint::Object(o) => o,
                        };
                        (arg, param)
                    })
                    .collect();
                ConstraintRule {
                    tool,
                    params: new_params,
                }
            })
            .collect(),
    };
    Ok(out)
}
