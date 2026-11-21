use super::{ConstraintParam, ConstraintRule};
use assay_common::doc::{LegacyNameOr, LegacySeqOrMap};
use std::collections::BTreeMap;

pub(super) type LegacyConstraints = LegacySeqOrMap<
    Vec<ConstraintRule>,
    BTreeMap<String, BTreeMap<String, LegacyNameOr<ConstraintParam>>>,
>;

pub(super) fn normalize_constraints(constraints: Option<LegacyConstraints>) -> Vec<ConstraintRule> {
    match constraints {
        None => Vec::new(),
        Some(LegacySeqOrMap::Seq(list)) => list,
        Some(LegacySeqOrMap::Map(map)) => map
            .into_iter()
            .map(|(tool, params)| {
                let params = params
                    .into_iter()
                    .map(|(arg, constraint)| {
                        let param = match constraint {
                            LegacyNameOr::Name(pattern) => ConstraintParam {
                                matches: Some(pattern),
                            },
                            LegacyNameOr::Detailed(param) => param,
                        };
                        (arg, param)
                    })
                    .collect();
                ConstraintRule { tool, params }
            })
            .collect(),
    }
}
