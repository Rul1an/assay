//! Consumer evaluation sidecar (ADR-025 E2 Phase 3).
//!
//! Evaluation attestations written by lint/soak; NOT in manifest (tamper-evidence).
//! Schema: evaluation-v1.

use crate::bundle::manifest::Manifest;
use crate::crypto::jcs;
use crate::lint::{LintReport, LintSummary, Severity};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Schema version for evaluation-v1.
pub const EVALUATION_V1: &str = "evaluation-v1";

/// Subject digest (attestation-shaped).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubjectDigest {
    pub sha256: String,
}

/// Subject entry for attestation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubjectEntry {
    pub name: String,
    pub digest: SubjectDigest,
}

/// Pack applied in evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PackApplied {
    pub name: String,
    pub version: String,
    pub kind: String,
    pub digest: String,
    pub source: String,
}

/// Decision policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionPolicy {
    pub pass_on_severity_at_or_above: String,
}

/// Evaluation inputs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvaluationInputs {
    pub bundle_digest: String,
    pub manifest_digest: String,
    pub packs_applied: Vec<PackApplied>,
    pub decision_policy: DecisionPolicy,
}

/// Canonical report payload for results_digest verification (JCS digest).
/// Embedded inline in evaluation for portable/audit-proof verification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReportInline {
    pub schema_version: String,
    pub report: serde_json::Value,
}

/// Evaluation outputs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvaluationOutputs {
    pub status: String,
    pub summary: LintSummary,
    pub results_digest: String,
    /// Inline report for offline results_digest verification (ADR-025 E2 Phase 3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_inline: Option<ReportInline>,
}

/// Consumer evaluation (sidecar JSON).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Evaluation {
    pub schema_version: String,
    pub evaluation_id: String,
    pub created_at: String,
    pub subject: Vec<SubjectEntry>,
    pub command: EvaluationCommand,
    pub inputs: EvaluationInputs,
    pub outputs: EvaluationOutputs,
}

/// Command that produced the evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvaluationCommand {
    pub name: String,
    pub version: String,
    pub args: Vec<String>,
}

/// Compute logical bundle digest from manifest (ADR-025 E2).
/// sha256(JCS({run_root, algorithms, files})).
pub fn compute_bundle_digest(manifest: &Manifest) -> Result<String> {
    let input = serde_json::json!({
        "run_root": manifest.run_root,
        "algorithms": &manifest.algorithms,
        "files": &manifest.files,
    });
    let bytes = jcs::to_vec(&input)?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(&bytes))))
}

/// Compute manifest digest: sha256(JCS(manifest)).
pub fn compute_manifest_digest(manifest: &Manifest) -> Result<String> {
    let bytes = jcs::to_vec(manifest)?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(&bytes))))
}

/// Canonical report structure for digest computation (lint-report-v1).
/// Must match exactly for results_digest verification.
fn canonical_report_json(report: &LintReport) -> serde_json::Value {
    serde_json::json!({
        "bundle_meta": {
            "bundle_id": report.bundle_meta.bundle_id,
            "run_root": report.bundle_meta.run_root,
            "event_count": report.bundle_meta.event_count,
        },
        "verified": report.verified,
        "findings": report.findings.iter().map(|f| serde_json::json!({
            "rule_id": f.rule_id,
            "severity": f.severity.to_string(),
            "message": f.message,
            "location": f.location,
            "fingerprint": f.fingerprint,
        })).collect::<Vec<_>>(),
        "summary": &report.summary,
    })
}

/// Compute results digest from lint report: sha256(JCS(canonical_report)).
pub fn compute_results_digest(report: &LintReport) -> Result<String> {
    let canonical = canonical_report_json(report);
    let bytes = jcs::to_vec(&canonical)?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(&bytes))))
}

/// Compute results digest from inline report payload.
pub fn compute_results_digest_from_inline(report_json: &serde_json::Value) -> Result<String> {
    let bytes = jcs::to_vec(report_json)?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(&bytes))))
}

/// Build evaluation from lint result.
pub fn build_evaluation_from_lint(
    report: &LintReport,
    packs_applied: Vec<PackApplied>,
    command_name: &str,
    command_version: &str,
    command_args: Vec<String>,
    fail_on: Severity,
    evaluation_id: String,
) -> Result<Evaluation> {
    let bundle_digest = compute_bundle_digest(&report.bundle_meta)?;
    let manifest_digest = compute_manifest_digest(&report.bundle_meta)?;
    let results_digest = compute_results_digest(report)?;

    let status = if report.has_findings_at_or_above(&fail_on) {
        "fail"
    } else {
        "pass"
    };

    let subject = vec![
        SubjectEntry {
            name: "bundle.tar.gz".into(),
            digest: SubjectDigest {
                sha256: bundle_digest
                    .strip_prefix("sha256:")
                    .unwrap_or(&bundle_digest)
                    .to_string(),
            },
        },
        SubjectEntry {
            name: "manifest.json".into(),
            digest: SubjectDigest {
                sha256: manifest_digest
                    .strip_prefix("sha256:")
                    .unwrap_or(&manifest_digest)
                    .to_string(),
            },
        },
    ];

    Ok(Evaluation {
        schema_version: EVALUATION_V1.to_string(),
        evaluation_id,
        created_at: chrono::Utc::now().to_rfc3339(),
        subject,
        command: EvaluationCommand {
            name: command_name.into(),
            version: command_version.into(),
            args: command_args,
        },
        inputs: EvaluationInputs {
            bundle_digest: bundle_digest.clone(),
            manifest_digest: manifest_digest.clone(),
            packs_applied,
            decision_policy: DecisionPolicy {
                pass_on_severity_at_or_above: fail_on.to_string(),
            },
        },
        outputs: EvaluationOutputs {
            status: status.into(),
            summary: report.summary.clone(),
            results_digest: results_digest.clone(),
            report_inline: Some(ReportInline {
                schema_version: "lint-report-v1".into(),
                report: canonical_report_json(report),
            }),
        },
    })
}

/// Result of evaluation verification.
#[derive(Debug, Clone, Serialize)]
pub struct VerifyEvalResult {
    pub ok: bool,
    pub bundle_digest_match: bool,
    pub manifest_digest_match: bool,
    pub results_digest_verified: bool,
    pub results_digest_verifiable: bool,
    pub packs_verified: usize,
    pub packs_unverifiable: usize,
    pub packs_mismatched: usize,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Verify evaluation sidecar against bundle (ADR-025 E2 Phase 3).
///
/// Checks: bundle_digest, manifest_digest, results_digest (if report_inline), optionally pack digests.
/// Exit codes: 0=OK, 1=verification failed, 2=infra error.
pub fn verify_evaluation(
    evaluation: &Evaluation,
    manifest: &Manifest,
    pack_digests: Option<Vec<(String, String)>>,
    strict: bool,
) -> Result<VerifyEvalResult> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Schema checks
    if evaluation.schema_version != EVALUATION_V1 {
        errors.push(format!(
            "schema_version must be \"{}\", got \"{}\"",
            EVALUATION_V1, evaluation.schema_version
        ));
    }

    if uuid::Uuid::parse_str(&evaluation.evaluation_id).is_err() {
        errors.push(format!(
            "evaluation_id must be UUID, got \"{}\"",
            evaluation.evaluation_id
        ));
    }

    if chrono::DateTime::parse_from_rfc3339(&evaluation.created_at).is_err() {
        errors.push(format!(
            "created_at must be RFC3339, got \"{}\"",
            evaluation.created_at
        ));
    }

    let digest_ok = |s: &str| -> bool {
        s.starts_with("sha256:") && s.len() == 71 && s[7..].chars().all(|c| c.is_ascii_hexdigit())
    };

    if !digest_ok(&evaluation.inputs.bundle_digest) {
        errors.push(format!(
            "inputs.bundle_digest invalid format (expected sha256:<64 hex>): \"{}\"",
            evaluation.inputs.bundle_digest
        ));
    }
    if !digest_ok(&evaluation.inputs.manifest_digest) {
        errors.push(format!(
            "inputs.manifest_digest invalid format (expected sha256:<64 hex>): \"{}\"",
            evaluation.inputs.manifest_digest
        ));
    }
    if !digest_ok(&evaluation.outputs.results_digest) {
        errors.push(format!(
            "outputs.results_digest invalid format (expected sha256:<64 hex>): \"{}\"",
            evaluation.outputs.results_digest
        ));
    }

    if !errors.is_empty() {
        return Ok(VerifyEvalResult {
            ok: false,
            bundle_digest_match: false,
            manifest_digest_match: false,
            results_digest_verified: false,
            results_digest_verifiable: false,
            packs_verified: 0,
            packs_unverifiable: 0,
            packs_mismatched: 0,
            errors,
            warnings,
        });
    }

    // 1) Bundle bindings
    let bundle_digest_expected = compute_bundle_digest(manifest)?;
    let manifest_digest_expected = compute_manifest_digest(manifest)?;

    let bundle_digest_match = normalize_digest(&evaluation.inputs.bundle_digest)
        == normalize_digest(&bundle_digest_expected);
    let manifest_digest_match = normalize_digest(&evaluation.inputs.manifest_digest)
        == normalize_digest(&manifest_digest_expected);

    if !bundle_digest_match {
        errors.push(format!(
            "bundle_digest mismatch: eval={} expected={}",
            evaluation.inputs.bundle_digest, bundle_digest_expected
        ));
    }
    if !manifest_digest_match {
        errors.push(format!(
            "manifest_digest mismatch: eval={} expected={}",
            evaluation.inputs.manifest_digest, manifest_digest_expected
        ));
    }

    // 2) Results digest
    let (results_digest_verified, results_digest_verifiable) =
        if let Some(ref ri) = evaluation.outputs.report_inline {
            let computed = compute_results_digest_from_inline(&ri.report)?;
            (
                normalize_digest(&evaluation.outputs.results_digest) == normalize_digest(&computed),
                true,
            )
        } else {
            warnings.push("no report_inline: results_digest not verifiable".into());
            (false, false)
        };

    if results_digest_verifiable && !results_digest_verified {
        errors.push(format!(
            "results_digest mismatch: eval={} recomputed differs",
            evaluation.outputs.results_digest
        ));
    }

    // 3) Pack digests
    let (packs_verified, packs_unverifiable, packs_mismatched) =
        if let Some(ref expected) = pack_digests {
            let mut verified = 0;
            let mut unverifiable = 0;
            let mut mismatched = 0;
            let expected_map: std::collections::HashMap<_, _> = expected
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();

            for pa in &evaluation.inputs.packs_applied {
                let key = format!("{}@{}", pa.name, pa.version);
                match expected_map.get(key.as_str()) {
                    Some(exp) => {
                        if normalize_digest(&pa.digest) == normalize_digest(exp) {
                            verified += 1;
                        } else {
                            mismatched += 1;
                            errors.push(format!(
                                "packs_applied digest mismatch: {} eval={} expected={}",
                                key, pa.digest, exp
                            ));
                        }
                    }
                    None => {
                        unverifiable += 1;
                        warnings.push(format!("pack {} not resolvable for digest check", key));
                        if strict {
                            errors.push(format!("--strict: pack {} not resolvable", key));
                        }
                    }
                }
            }
            (verified, unverifiable, mismatched)
        } else {
            let n = evaluation.inputs.packs_applied.len();
            if strict && n > 0 {
                errors.push(format!(
                    "--strict: {} pack(s) unverifiable (pass --pack to resolve)",
                    n
                ));
            }
            (0, n, 0)
        };

    let ok = errors.is_empty();

    Ok(VerifyEvalResult {
        ok,
        bundle_digest_match,
        manifest_digest_match,
        results_digest_verified,
        results_digest_verifiable,
        packs_verified,
        packs_unverifiable,
        packs_mismatched,
        errors,
        warnings,
    })
}

fn normalize_digest(d: &str) -> String {
    if d.starts_with("sha256:") {
        d.to_lowercase()
    } else {
        format!("sha256:{}", d.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::manifest::{AlgorithmMeta, FileMeta};
    use std::collections::BTreeMap;

    #[test]
    fn test_compute_bundle_digest_deterministic() {
        use crate::types::ProducerMeta;

        let manifest = Manifest {
            schema_version: 1,
            bundle_id: "sha256:abc".into(),
            producer: ProducerMeta::default(),
            run_id: "run1".into(),
            event_count: 1,
            run_root: "sha256:def".into(),
            algorithms: AlgorithmMeta::default(),
            files: {
                let mut m = BTreeMap::new();
                m.insert(
                    "events.ndjson".into(),
                    FileMeta {
                        path: "events.ndjson".into(),
                        sha256: "sha256:123".into(),
                        bytes: 100,
                    },
                );
                m
            },
            x_assay: None,
        };

        let d1 = compute_bundle_digest(&manifest).unwrap();
        let d2 = compute_bundle_digest(&manifest).unwrap();
        assert_eq!(d1, d2);
        assert!(d1.starts_with("sha256:"));
    }

    #[test]
    fn test_build_evaluation_from_lint() {
        use crate::bundle::manifest::AlgorithmMeta;
        use crate::lint::{LintReport, LintSummary};
        use crate::types::ProducerMeta;

        let manifest = Manifest {
            schema_version: 1,
            bundle_id: "sha256:abc".into(),
            producer: ProducerMeta::default(),
            run_id: "run1".into(),
            event_count: 0,
            run_root: "sha256:def".into(),
            algorithms: AlgorithmMeta::default(),
            files: BTreeMap::new(),
            x_assay: None,
        };

        let report = LintReport {
            tool_version: "1.0".into(),
            bundle_meta: manifest,
            verified: true,
            findings: vec![],
            summary: LintSummary {
                total: 0,
                errors: 0,
                warnings: 0,
                infos: 0,
            },
        };

        let eval = build_evaluation_from_lint(
            &report,
            vec![],
            "assay evidence lint",
            "1.0",
            vec!["bundle.tar.gz".into()],
            Severity::Error,
            "test-id-123".to_string(),
        )
        .unwrap();

        assert_eq!(eval.schema_version, EVALUATION_V1);
        assert_eq!(eval.evaluation_id, "test-id-123");
        assert_eq!(eval.outputs.status, "pass");
        assert_eq!(eval.subject.len(), 2);
        assert!(eval.inputs.bundle_digest.starts_with("sha256:"));
        assert!(eval.outputs.results_digest.starts_with("sha256:"));
    }

    #[test]
    fn test_verify_evaluation_ok() {
        use crate::bundle::manifest::AlgorithmMeta;
        use crate::lint::{LintReport, LintSummary};
        use crate::types::ProducerMeta;

        let manifest = Manifest {
            schema_version: 1,
            bundle_id: "sha256:abc".into(),
            producer: ProducerMeta::default(),
            run_id: "run1".into(),
            event_count: 0,
            run_root: "sha256:def".into(),
            algorithms: AlgorithmMeta::default(),
            files: BTreeMap::new(),
            x_assay: None,
        };

        let report = LintReport {
            tool_version: "1.0".into(),
            bundle_meta: manifest.clone(),
            verified: true,
            findings: vec![],
            summary: LintSummary {
                total: 0,
                errors: 0,
                warnings: 0,
                infos: 0,
            },
        };

        let eval = build_evaluation_from_lint(
            &report,
            vec![],
            "assay evidence lint",
            "1.0",
            vec!["bundle.tar.gz".into()],
            Severity::Error,
            uuid::Uuid::nil().to_string(),
        )
        .unwrap();

        let result = verify_evaluation(&eval, &manifest, None, false).unwrap();
        assert!(result.ok);
        assert!(result.bundle_digest_match);
        assert!(result.manifest_digest_match);
        assert!(result.results_digest_verified);
        assert!(result.results_digest_verifiable);
    }

    #[test]
    fn test_verify_evaluation_bundle_digest_mismatch() {
        use crate::bundle::manifest::AlgorithmMeta;
        use crate::lint::LintSummary;
        use crate::types::ProducerMeta;

        let manifest = Manifest {
            schema_version: 1,
            bundle_id: "sha256:abc".into(),
            producer: ProducerMeta::default(),
            run_id: "run1".into(),
            event_count: 0,
            run_root: "sha256:def".into(),
            algorithms: AlgorithmMeta::default(),
            files: BTreeMap::new(),
            x_assay: None,
        };

        let eval = Evaluation {
            schema_version: EVALUATION_V1.to_string(),
            evaluation_id: "550e8400-e29b-41d4-a716-446655440000".into(),
            created_at: chrono::Utc::now().to_rfc3339(),
            subject: vec![],
            command: EvaluationCommand {
                name: "assay evidence lint".into(),
                version: "1.0".into(),
                args: vec![],
            },
            inputs: EvaluationInputs {
                bundle_digest:
                    "sha256:wrong123456789012345678901234567890123456789012345678901234567890"
                        .into(),
                manifest_digest: compute_manifest_digest(&manifest).unwrap(),
                packs_applied: vec![],
                decision_policy: DecisionPolicy {
                    pass_on_severity_at_or_above: "error".into(),
                },
            },
            outputs: EvaluationOutputs {
                status: "pass".into(),
                summary: LintSummary {
                    total: 0,
                    errors: 0,
                    warnings: 0,
                    infos: 0,
                },
                results_digest:
                    "sha256:0000000000000000000000000000000000000000000000000000000000000000".into(),
                report_inline: None,
            },
        };

        let result = verify_evaluation(&eval, &manifest, None, false).unwrap();
        assert!(!result.ok);
        assert!(!result.bundle_digest_match);
    }
}
