//! Derive inventory and assessment from admitted sources, then verify-before-publish.
//!
//! Surface grades, joins and claims are taken from the verifier's own functions.
//! This module does not invent a second assessment engine.

use super::{
    admitted_action_records, build_verified_incident_package, derive_surface_result,
    fresh_examination_result, incident_refusal_for_resolution, incident_surface_sources,
    is_valid_id, parse_bundle_inputs, retained_context_inventory, validate_context_input,
    AdmittedActionRecord, AssessmentDocument, AssessmentUnit, ContextInput, IncidentReason,
    IncidentVerifyReport, InventoryDisclosure, InventoryDocument, InventoryInput, PackageRef,
    SourceBudgetState, SurfaceResult, UnitDisclosure, ADMITTED_FORMATS, CONTEXT_KINDS,
    NON_CLAIMS_DEFAULT, VALID_SURFACES,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// One caller-supplied original-byte source for [`prepare_verified_incident_package`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentPrepareSource {
    /// Inventory occurrence id (`[a-z0-9][a-z0-9._-]{0,63}`).
    pub id: String,
    /// Exact SPEC §3 format token.
    pub format: String,
    /// Original source bytes; never rewritten.
    pub bytes: Vec<u8>,
}

const ENVELOPE_KEYS: [&str; 6] = ["specversion", "id", "type", "source", "time", "data"];

fn refuse(reason: IncidentReason) -> Box<IncidentVerifyReport> {
    Box::new(IncidentVerifyReport::refusal(reason))
}

fn canonical_json(value: &impl serde::Serialize) -> Result<Vec<u8>, IncidentReason> {
    let mut bytes = assay_canonical::jcs::to_vec(value).map_err(|_| IncidentReason::InputShape)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn unit_id(reference: &PackageRef) -> Result<String, IncidentReason> {
    let bytes = assay_canonical::jcs::to_vec(&serde_json::json!([
        reference.input_id,
        reference.sha256,
        reference.locator
    ]))
    .map_err(|_| IncidentReason::InputShape)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn empty_inventory_context() -> BTreeMap<String, InventoryDisclosure> {
    CONTEXT_KINDS
        .iter()
        .map(|kind| {
            (
                (*kind).to_string(),
                InventoryDisclosure {
                    value: serde_json::Value::Null,
                    reason: "not_available".to_string(),
                    refs: Vec::new(),
                },
            )
        })
        .collect()
}

fn declared_surfaces(format: &str) -> Vec<String> {
    match format {
        "proxy-decision-ndjson" => vec!["tool_call".to_string()],
        "inspector-protocol-793d103" => vec!["transcript_join".to_string()],
        "assay-bundle-v1" => ["tool_call", "filesystem", "network", "process"]
            .into_iter()
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn wrap_log_coherence(records: &[AdmittedActionRecord]) -> Result<(), IncidentReason> {
    let mut source = None;
    for record in records {
        if record.operation != "proxy-decision-ndjson" {
            continue;
        }
        let Some(object) = record.value.as_object() else {
            return Err(IncidentReason::InputShape);
        };
        if object
            .keys()
            .any(|key| !ENVELOPE_KEYS.contains(&key.as_str()))
        {
            return Err(IncidentReason::InputShape);
        }
        let line_source = object
            .get("source")
            .and_then(serde_json::Value::as_str)
            .ok_or(IncidentReason::InputShape)?;
        match source {
            None => source = Some(line_source.to_string()),
            Some(ref expected) if expected == line_source => {}
            Some(_) => return Err(IncidentReason::InputShape),
        }
    }
    Ok(())
}

fn derived_cap1(units: &[AssessmentUnit], surfaces: &[SurfaceResult]) -> Option<serde_json::Value> {
    let present: Vec<&SurfaceResult> = surfaces
        .iter()
        .filter(|surface| !surface.source_inputs.is_empty())
        .collect();
    if present.is_empty() {
        return None;
    }
    let incomplete = units.iter().any(|unit| {
        matches!(
            unit.disposition.as_str(),
            "failed" | "resource_exhausted" | "unavailable"
        )
    });
    Some(serde_json::json!({
        "profile": "cap/1",
        "subject": {"kind": "collection", "ref": "incident-selected-inputs"},
        "integrity": {
            "complete": !incomplete,
            "statement": "accounting of selected supplied records",
            "capped_to": if incomplete { serde_json::Value::from("incomplete") } else { serde_json::Value::Null }
        },
        "strata": present.iter().map(|surface| {
            let eligible = units.iter().filter(|unit| unit.surface == surface.name).count() as u64;
            let examined = units.iter().filter(|unit| {
                unit.surface == surface.name && unit.disposition == "examined"
            }).count() as u64;
            serde_json::json!({
                "id": surface.name,
                "population": "retained supplied records",
                "basis": {
                    "kind": "enumeration",
                    "enumeration_method": "incident-v1-digest-locator-enumeration"
                },
                "eligible": eligible,
                "examined": examined,
                "unexamined": [],
                "supports": ["positive_presence", "bounded_negative", "exhaustive_set"]
            })
        }).collect::<Vec<_>>()
    }))
}

/// Build a verified incident package from admitted original-byte sources.
///
/// Inventory labels and assessment accounting are derived here. Surface
/// observation grades come from the verifier's `derive_surface_result`. Joins
/// and claims stay empty because the verifier does not invent them. The recorded
/// `verification_context_sha256` is the digest of `context` under the same
/// JCS-plus-LF encoding the verifier recomputes.
pub fn prepare_verified_incident_package(
    sources: &[IncidentPrepareSource],
    context: &ContextInput,
) -> Result<(Vec<u8>, IncidentVerifyReport), Box<IncidentVerifyReport>> {
    validate_context_input(context).map_err(refuse)?;
    if sources.is_empty() {
        return Err(refuse(IncidentReason::InputShape));
    }
    let mut seen = BTreeSet::new();
    let mut inputs = Vec::new();
    let mut owned_objects = BTreeMap::new();
    for source in sources {
        if !ADMITTED_FORMATS.contains(&source.format.as_str()) {
            return Err(refuse(IncidentReason::UnknownFormat));
        }
        if !is_valid_id(&source.id) || !seen.insert(source.id.clone()) {
            return Err(refuse(IncidentReason::InputShape));
        }
        let sha256 = hex::encode(Sha256::digest(&source.bytes));
        inputs.push(InventoryInput {
            id: source.id.clone(),
            sha256: sha256.clone(),
            bytes: source.bytes.len() as u64,
            format: source.format.clone(),
            surfaces: declared_surfaces(&source.format),
            binding: None,
        });
        owned_objects.insert(format!("objects/{sha256}"), source.bytes.as_slice());
    }
    inputs.sort_by(|left, right| left.id.cmp(&right.id));
    let inventory = InventoryDocument {
        schema: "assay.incident.inventory.v1".to_string(),
        assessment_sha256: "0".repeat(64),
        inputs,
        context: empty_inventory_context(),
        non_claims: NON_CLAIMS_DEFAULT
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
    };
    let object_map: BTreeMap<&str, &[u8]> = owned_objects
        .iter()
        .map(|(path, bytes)| (path.as_str(), *bytes))
        .collect();
    let mut source_budget = SourceBudgetState::default();
    let parsed_bundles = parse_bundle_inputs(&inventory, &object_map, context, &mut source_budget)
        .map_err(|error| refuse(incident_refusal_for_resolution(error)))?;
    let records = admitted_action_records(&inventory, &object_map, &parsed_bundles)
        .map_err(|error| refuse(incident_refusal_for_resolution(error)))?;
    wrap_log_coherence(&records).map_err(refuse)?;
    let retained = retained_context_inventory(&inventory, &object_map, &parsed_bundles, context)
        .map_err(|error| refuse(incident_refusal_for_resolution(error)))?;
    let mut units = Vec::new();
    for record in &records {
        units.push(AssessmentUnit {
            id: unit_id(&record.reference).map_err(refuse)?,
            source: record.reference.clone(),
            surface: record.surface.to_string(),
            disposition: "examined".to_string(),
            result: Some(UnitDisclosure::Inline {
                value: fresh_examination_result(record),
            }),
            withheld_digest: None,
        });
    }
    units.sort_by(|left, right| left.id.cmp(&right.id));
    let mut surfaces = Vec::new();
    for name in VALID_SURFACES {
        let source_inputs = incident_surface_sources(&inventory, name);
        let applicability = context
            .applicability
            .iter()
            .find(|applicability| applicability.surface == *name);
        let derived = derive_surface_result(&source_inputs, applicability, &retained, name);
        surfaces.push(SurfaceResult {
            name: (*name).to_string(),
            source_inputs,
            observation: derived.observation,
            basis_refs: derived.basis_refs,
            window: derived.window,
        });
    }
    let mut ctx_jcs =
        assay_canonical::jcs::to_vec(context).map_err(|_| refuse(IncidentReason::InputShape))?;
    ctx_jcs.push(b'\n');
    let assessment = AssessmentDocument {
        schema: "assay.incident.assessment.v1".to_string(),
        verification_context_sha256: hex::encode(Sha256::digest(&ctx_jcs)),
        cap1: derived_cap1(&units, &surfaces),
        units,
        surfaces,
        joins: Vec::new(),
        claims: Vec::new(),
        resolved_results: Vec::new(),
    };
    let assessment_bytes = canonical_json(&assessment).map_err(refuse)?;
    let mut inventory = inventory;
    inventory.assessment_sha256 = hex::encode(Sha256::digest(&assessment_bytes));
    let inventory_bytes = canonical_json(&inventory).map_err(refuse)?;
    let object_slices: Vec<&[u8]> = sources
        .iter()
        .map(|source| source.bytes.as_slice())
        .collect();
    build_verified_incident_package(&inventory_bytes, &assessment_bytes, &object_slices, context)
}
