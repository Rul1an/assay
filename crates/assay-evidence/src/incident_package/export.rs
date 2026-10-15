//! Admission for caller-supplied package documents; no assessment generation.

use super::{
    admit_metadata, check_inventory_budget, check_source_bytes, incident_refusal_for_resolution,
    is_valid_hex64, validate_context_input, verify_incident_package, ContextInput, IncidentOutcome,
    IncidentReason, IncidentVerifyReport,
};
use crate::incident_package::container_writer::{
    write_incident_container_with_limits, ContainerBudget,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// One physical object required by admitted metadata. Occurrence budgets have
/// already been charged, including every occurrence sharing this digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentExportObject {
    pub sha256: String,
    pub bytes: u64,
}

/// Admit metadata and declared source budgets before opening any object file.
/// This is preparation only, not package verification or permission to publish.
pub fn prepare_incident_export(
    inventory: &[u8],
    assessment: &[u8],
    context: &ContextInput,
) -> Result<Vec<IncidentExportObject>, IncidentReason> {
    validate_context_input(context)?;
    let mut container_budget = ContainerBudget::new(
        inventory.len() as u64,
        assessment.len() as u64,
        &context.limits,
    )?;
    let (inventory, _, _) = admit_metadata(inventory, assessment, context)?;
    if inventory.assessment_sha256 != hex::encode(Sha256::digest(assessment)) {
        return Err(IncidentReason::Digest);
    }
    check_inventory_budget(&inventory, context)?;
    let mut objects = BTreeMap::new();
    for input in &inventory.inputs {
        // Check every format before deduplication: one digest may be declared
        // under multiple formats, in which case every ceiling must hold.
        check_source_bytes(&input.format, input.bytes, context)
            .map_err(incident_refusal_for_resolution)?;
        if !is_valid_hex64(&input.sha256) {
            return Err(IncidentReason::InputShape);
        }
        if objects
            .insert(input.sha256.clone(), input.bytes)
            .is_some_and(|n| n != input.bytes)
        {
            return Err(IncidentReason::Digest);
        }
    }
    for size in objects.values() {
        let padded = container_budget.check_object(*size)?;
        container_budget.add_unique(padded)?;
    }
    Ok(objects
        .into_iter()
        .map(|(sha256, bytes)| IncidentExportObject { sha256, bytes })
        .collect())
}

/// Build and verify the exact staged bytes. The caller may publish only Ok.
/// This function does not read sources or write an output path.
pub fn build_verified_incident_package(
    inventory: &[u8],
    assessment: &[u8],
    objects: &[&[u8]],
    context: &ContextInput,
) -> Result<(Vec<u8>, IncidentVerifyReport), Box<IncidentVerifyReport>> {
    prepare_incident_export(inventory, assessment, context)
        .map_err(|reason| Box::new(IncidentVerifyReport::refusal(reason)))?;
    let bytes =
        write_incident_container_with_limits(inventory, assessment, objects, &context.limits)
            .map_err(|reason| Box::new(IncidentVerifyReport::refusal(reason)))?;
    let report = verify_incident_package(&bytes, context);
    if report.outcome != IncidentOutcome::PackageVerified {
        return Err(Box::new(report));
    }
    let report_len = report
        .to_canonical_bytes()
        .map_err(|_| Box::new(IncidentVerifyReport::refusal(IncidentReason::InputShape)))?
        .len() as u64;
    let reason = if report_len > super::ContextLimits::HARD.fresh_result_bytes {
        Some(IncidentReason::ResourceHard)
    } else if report_len > context.limits.fresh_result_bytes {
        Some(IncidentReason::ResourceSelected)
    } else {
        None
    };
    if let Some(reason) = reason {
        return Err(Box::new(IncidentVerifyReport::refusal(reason)));
    }
    Ok((bytes, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::incident_package::{read_incident_container, write_incident_container};
    use serde_json::Value;

    const FIXTURE: &[u8] =
        include_bytes!("../../tests/fixtures/incident_package/valid_present_empty.tar");

    fn canonical(value: &Value) -> Vec<u8> {
        let mut bytes = assay_canonical::jcs::to_vec(value).unwrap();
        bytes.push(b'\n');
        bytes
    }

    #[test]
    fn metadata_admission_preserves_multi_fault_first_refusal() {
        let container = read_incident_container(FIXTURE).unwrap();
        let mut inventory: Value = serde_json::from_slice(&container.inventory.bytes).unwrap();
        inventory["context"]["actor"]["reason"] = "invalid".into();
        let inventory = canonical(&inventory);
        // The assessment's preflight budget precedes typed inventory validation.
        let mut assessment = serde_json::json!(null);
        for _ in 0..65 {
            assessment = serde_json::json!([assessment]);
        }
        let assessment = canonical(&assessment);
        let context = ContextInput::default();
        assert_eq!(
            prepare_incident_export(&inventory, &assessment, &context),
            Err(IncidentReason::ResourceHard)
        );
        let package = write_incident_container(&inventory, &assessment, &[b"[]\n"]).unwrap();
        assert_eq!(
            verify_incident_package(&package, &context).reason,
            Some(IncidentReason::ResourceHard)
        );
        // Without that budget fault, the inventory grammar fault is observable.
        assert_eq!(
            prepare_incident_export(&inventory, &container.assessment.bytes, &context),
            Err(IncidentReason::InputShape)
        );
    }

    #[test]
    fn format_caps_have_inclusive_boundaries_for_every_admitted_format() {
        use super::super::{check_source_bytes, incident_refusal_for_resolution};
        let mut context = ContextInput::default();
        context.limits.policy_bytes = 2;
        context.limits.key_bytes = 2;
        context.limits.transcript_bytes = 2;
        context.limits.health_bytes = 2;
        context.limits.retained_report_bytes = 2;
        context.limits.envelope_bytes = 2;
        context.limits.inner_compressed_bytes = 2;
        for (format, hard) in [
            ("policy-bytes", 1_048_576),
            ("ed25519-public-key-pem", 16_384),
            ("proxy-decision-ndjson", 16_777_216),
            ("inspector-protocol-793d103", 16_777_216),
            ("assay-health-json", 1_048_576),
            ("assay-coverage-json", 1_048_576),
            ("incident-context-v1", 1_048_576),
            ("incident-results-v1", 16_777_216),
            ("attestation-report", 16_777_216),
            ("dsse-attestation", 33_554_432),
            ("assay-bundle-v1", 104_857_600),
        ] {
            assert!(check_source_bytes(format, 2, &context).is_ok(), "{format}");
            assert_eq!(
                check_source_bytes(format, 3, &context).map_err(incident_refusal_for_resolution),
                Err(IncidentReason::ResourceSelected),
                "{format}"
            );
            assert!(
                check_source_bytes(format, hard, &ContextInput::default()).is_ok(),
                "{format}"
            );
            assert_eq!(
                check_source_bytes(format, hard + 1, &ContextInput::default())
                    .map_err(incident_refusal_for_resolution),
                Err(IncidentReason::ResourceHard),
                "{format}"
            );
        }
    }

    #[test]
    fn staged_source_and_duplicate_input_order_do_not_change_archive() {
        let container = read_incident_container(FIXTURE).unwrap();
        let context = ContextInput::default();
        let objects = prepare_incident_export(
            &container.inventory.bytes,
            &container.assessment.bytes,
            &context,
        )
        .unwrap();
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].bytes, 3);
        let staged = b"[]\n".to_vec();
        let (bytes, report) = build_verified_incident_package(
            &container.inventory.bytes,
            &container.assessment.bytes,
            &[&staged, &staged],
            &context,
        )
        .unwrap();
        assert_eq!(bytes, FIXTURE);
        assert_eq!(report.outcome, IncidentOutcome::PackageVerified);
        assert!(build_verified_incident_package(
            &container.inventory.bytes,
            &container.assessment.bytes,
            &[b"{}\n"],
            &context
        )
        .is_err());
    }
}
