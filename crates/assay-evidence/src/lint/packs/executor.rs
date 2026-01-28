//! Pack executor with collision handling.
//!
//! Executes pack rules against evidence bundles and handles rule collisions
//! according to SPEC-Pack-Engine-v1 collision policy.

use super::checks::{execute_check, CheckContext};
use super::loader::{LoadedPack, PackError};
use super::schema::PackKind;
use crate::bundle::writer::Manifest;
use crate::lint::LintFinding;
use crate::types::EvidenceEvent;
use std::collections::{HashMap, HashSet};

/// Pack executor that runs checks from multiple packs.
pub struct PackExecutor {
    /// Loaded packs.
    packs: Vec<LoadedPack>,
    /// Deduplicated rule IDs (canonical format).
    rule_ids: HashSet<String>,
}

impl PackExecutor {
    /// Create a new pack executor, validating collision policy.
    pub fn new(packs: Vec<LoadedPack>) -> Result<Self, PackError> {
        let mut rule_ids = HashSet::new();
        let mut canonical_to_pack: HashMap<String, (&str, PackKind)> = HashMap::new();

        for pack in &packs {
            for rule in &pack.definition.rules {
                let canonical_id = pack.canonical_rule_id(&rule.id);

                // Check for collision
                if let Some((existing_pack, existing_kind)) = canonical_to_pack.get(&canonical_id) {
                    // For compliance packs, collision is a hard fail
                    if pack.definition.kind == PackKind::Compliance
                        || *existing_kind == PackKind::Compliance
                    {
                        return Err(PackError::ComplianceCollision {
                            rule_id: canonical_id,
                            pack_a: existing_pack.to_string(),
                            pack_b: pack.definition.name.clone(),
                        });
                    }

                    // For non-compliance packs, warn and use last definition
                    eprintln!(
                        "Warning: Rule '{}' defined in both '{}' and '{}', using definition from '{}'",
                        canonical_id, existing_pack, pack.definition.name, pack.definition.name
                    );
                }

                canonical_to_pack.insert(
                    canonical_id.clone(),
                    (&pack.definition.name, pack.definition.kind),
                );
                rule_ids.insert(canonical_id);
            }
        }

        Ok(Self { packs, rule_ids })
    }

    /// Get the number of unique rules across all packs.
    pub fn rule_count(&self) -> usize {
        self.rule_ids.len()
    }

    /// Get all loaded packs.
    pub fn packs(&self) -> &[LoadedPack] {
        &self.packs
    }

    /// Check if any pack is a compliance pack.
    pub fn has_compliance_pack(&self) -> bool {
        self.packs
            .iter()
            .any(|p| p.definition.kind == PackKind::Compliance)
    }

    /// Get combined disclaimer for all compliance packs.
    pub fn combined_disclaimer(&self) -> Option<String> {
        let disclaimers: Vec<&str> = self
            .packs
            .iter()
            .filter(|p| p.definition.kind == PackKind::Compliance)
            .filter_map(|p| p.definition.disclaimer.as_deref())
            .collect();

        if disclaimers.is_empty() {
            None
        } else {
            Some(disclaimers.join("\n\n---\n\n"))
        }
    }

    /// Execute all pack rules against the bundle.
    pub fn execute(
        &self,
        events: &[EvidenceEvent],
        manifest: &Manifest,
        bundle_path: &str,
    ) -> Vec<LintFinding> {
        let mut findings = Vec::new();
        let mut seen_canonical_ids = HashSet::new();

        for pack in &self.packs {
            let ctx = CheckContext {
                events,
                manifest,
                bundle_path,
                pack_name: &pack.definition.name,
                pack_version: &pack.definition.version,
                pack_digest: &pack.digest,
            };

            for rule in &pack.definition.rules {
                let canonical_id = pack.canonical_rule_id(&rule.id);

                // Dedupe within same execution (same canonical ID = run once)
                if seen_canonical_ids.contains(&canonical_id) {
                    continue;
                }
                seen_canonical_ids.insert(canonical_id);

                let result = execute_check(rule, &ctx);
                if let Some(finding) = result.finding {
                    findings.push(finding);
                }
            }
        }

        findings
    }

    /// Execute and truncate results to max_results (lowest severity first).
    pub fn execute_with_limit(
        &self,
        events: &[EvidenceEvent],
        manifest: &Manifest,
        bundle_path: &str,
        max_results: usize,
    ) -> (Vec<LintFinding>, bool, usize) {
        let mut findings = self.execute(events, manifest, bundle_path);

        if findings.len() <= max_results {
            return (findings, false, 0);
        }

        // Sort by severity (lowest first for truncation)
        findings.sort_by(|a, b| {
            let a_priority = severity_priority(&a.severity);
            let b_priority = severity_priority(&b.severity);
            a_priority.cmp(&b_priority)
        });

        // Truncate lowest severity first
        let truncated_count = findings.len() - max_results;
        findings.truncate(max_results);

        // Re-sort for display (highest severity first)
        findings.sort_by(|a, b| {
            let a_priority = severity_priority(&a.severity);
            let b_priority = severity_priority(&b.severity);
            b_priority.cmp(&a_priority)
        });

        (findings, true, truncated_count)
    }
}

/// Get severity priority for sorting.
fn severity_priority(severity: &crate::lint::Severity) -> u8 {
    match severity {
        crate::lint::Severity::Info => 0,
        crate::lint::Severity::Warn => 1,
        crate::lint::Severity::Error => 2,
    }
}

/// Metadata about pack execution for SARIF output.
#[derive(Debug, Clone)]
pub struct PackExecutionMeta {
    /// Packs that were executed.
    pub packs: Vec<PackInfo>,
    /// Combined disclaimer (if any compliance packs).
    pub disclaimer: Option<String>,
    /// Whether results were truncated.
    pub truncated: bool,
    /// Number of truncated findings.
    pub truncated_count: usize,
}

/// Information about a single pack.
#[derive(Debug, Clone)]
pub struct PackInfo {
    pub name: String,
    pub version: String,
    pub digest: String,
    pub source_url: Option<String>,
    pub kind: PackKind,
}

impl From<&LoadedPack> for PackInfo {
    fn from(pack: &LoadedPack) -> Self {
        Self {
            name: pack.definition.name.clone(),
            version: pack.definition.version.clone(),
            digest: pack.digest.clone(),
            source_url: pack.definition.source_url.clone(),
            kind: pack.definition.kind,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::packs::schema::{
        CheckDefinition, PackDefinition, PackRequirements, PackRule, Severity,
    };

    fn make_test_pack(name: &str, kind: PackKind, rules: Vec<PackRule>) -> LoadedPack {
        LoadedPack {
            definition: PackDefinition {
                name: name.to_string(),
                version: "1.0.0".to_string(),
                kind,
                description: "Test pack".to_string(),
                author: "Test".to_string(),
                license: "Apache-2.0".to_string(),
                source_url: None,
                disclaimer: if kind == PackKind::Compliance {
                    Some("Test disclaimer".to_string())
                } else {
                    None
                },
                requires: PackRequirements {
                    assay_min_version: ">=0.0.0".to_string(),
                    evidence_schema_version: None,
                },
                rules,
            },
            digest: "sha256:test".to_string(),
            source: super::super::loader::PackSource::BuiltIn("test"),
        }
    }

    fn make_test_rule(id: &str) -> PackRule {
        PackRule {
            id: id.to_string(),
            severity: Severity::Error,
            description: "Test rule".to_string(),
            article_ref: None,
            help_markdown: None,
            check: CheckDefinition::EventCount { min: 1 },
        }
    }

    #[test]
    fn test_compliance_collision_fails() {
        let pack_a = make_test_pack(
            "pack-a",
            PackKind::Compliance,
            vec![make_test_rule("RULE-001")],
        );
        let pack_b = make_test_pack(
            "pack-a", // Same name = same canonical ID
            PackKind::Compliance,
            vec![make_test_rule("RULE-001")],
        );

        let result = PackExecutor::new(vec![pack_a, pack_b]);
        assert!(result.is_err());
    }

    #[test]
    fn test_security_collision_warns() {
        let pack_a = make_test_pack(
            "pack-a",
            PackKind::Security,
            vec![make_test_rule("RULE-001")],
        );
        let pack_b = make_test_pack(
            "pack-a", // Same name
            PackKind::Security,
            vec![make_test_rule("RULE-001")],
        );

        let result = PackExecutor::new(vec![pack_a, pack_b]);
        assert!(result.is_ok()); // Should succeed with warning
    }

    #[test]
    fn test_different_packs_same_rule_id_allowed() {
        let pack_a = make_test_pack(
            "pack-a",
            PackKind::Compliance,
            vec![make_test_rule("RULE-001")],
        );
        let pack_b = make_test_pack(
            "pack-b", // Different name = different canonical ID
            PackKind::Compliance,
            vec![make_test_rule("RULE-001")],
        );

        let result = PackExecutor::new(vec![pack_a, pack_b]);
        assert!(result.is_ok());
    }
}
