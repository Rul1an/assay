use serde::{Deserialize, Serialize};

use crate::ClaimGateDecision;

pub const COVERAGE_DESCRIPTOR_SCHEMA: &str = "assay.runner.coverage_descriptor.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectDimension {
    Filesystem,
    Network,
    Process,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageCompleteness {
    Full,
    OpenSyscallOnly,
    ConnectOnly,
    ExecOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageClaimKind {
    PositiveExistence,
    ExhaustiveSet,
    BoundedNegative,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageDescriptor {
    pub schema: String,
    pub dimension: EffectDimension,
    pub method: String,
    pub observes: Vec<String>,
    pub known_blind_spots: Vec<String>,
    pub completeness: CoverageCompleteness,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageClaimDecision {
    pub decision: ClaimGateDecision,
    pub rule: String,
    pub reason: String,
}

impl CoverageDescriptor {
    #[must_use]
    pub fn filesystem_open_syscall_only() -> Self {
        Self {
            schema: COVERAGE_DESCRIPTOR_SCHEMA.to_string(),
            dimension: EffectDimension::Filesystem,
            method: "open/openat/openat2 tracepoints".to_string(),
            observes: vec!["path opens through syscall tracepoints".to_string()],
            known_blind_spots: vec![
                "io_uring file operations may bypass syscall tracepoints".to_string(),
                "mmap-backed writes are not path-open observations".to_string(),
            ],
            completeness: CoverageCompleteness::OpenSyscallOnly,
        }
    }

    #[must_use]
    pub fn network_connect_only() -> Self {
        Self {
            schema: COVERAGE_DESCRIPTOR_SCHEMA.to_string(),
            dimension: EffectDimension::Network,
            method: "connect tracepoint".to_string(),
            observes: vec!["connect-time peer endpoints".to_string()],
            known_blind_spots: vec![
                "QUIC/datagram peer changes after connect are not an exhaustive peer set"
                    .to_string(),
                "io_uring network operations may bypass syscall tracepoints".to_string(),
            ],
            completeness: CoverageCompleteness::ConnectOnly,
        }
    }

    #[must_use]
    pub fn process_exec_only() -> Self {
        Self {
            schema: COVERAGE_DESCRIPTOR_SCHEMA.to_string(),
            dimension: EffectDimension::Process,
            method: "exec tracepoint".to_string(),
            observes: vec!["process exec targets".to_string()],
            known_blind_spots: vec![
                "fork/clone gaps can make process-tree exhaustiveness kernel-dependent".to_string(),
            ],
            completeness: CoverageCompleteness::ExecOnly,
        }
    }

    #[must_use]
    pub fn claim_decision(&self, claim_kind: CoverageClaimKind) -> CoverageClaimDecision {
        Self::claim_decision_for(Some(self), claim_kind)
    }

    #[must_use]
    pub fn claim_decision_for(
        descriptor: Option<&Self>,
        claim_kind: CoverageClaimKind,
    ) -> CoverageClaimDecision {
        let Some(descriptor) = descriptor else {
            return CoverageClaimDecision {
                decision: ClaimGateDecision::Blocked,
                rule: "coverage_descriptor_required_for_claim".to_string(),
                reason: "missing coverage descriptor blocks coverage-aware side-effect claims"
                    .to_string(),
            };
        };

        match claim_kind {
            CoverageClaimKind::PositiveExistence => CoverageClaimDecision {
                decision: ClaimGateDecision::Allowed,
                rule: "coverage_descriptor_allows_observed_positive_claim".to_string(),
                reason: format!(
                    "{} observes positive {} effects",
                    descriptor.method,
                    dimension_label(descriptor.dimension)
                ),
            },
            CoverageClaimKind::ExhaustiveSet if descriptor.known_blind_spots.is_empty() => {
                CoverageClaimDecision {
                    decision: ClaimGateDecision::Allowed,
                    rule: "coverage_descriptor_allows_exhaustive_claim".to_string(),
                    reason: "descriptor declares no relevant blind spots for this dimension"
                        .to_string(),
                }
            }
            CoverageClaimKind::ExhaustiveSet => CoverageClaimDecision {
                decision: ClaimGateDecision::Degraded,
                rule: "coverage_descriptor_degrades_exhaustive_claim".to_string(),
                reason: format!(
                    "{} completeness is {}; blind spots: {}",
                    dimension_label(descriptor.dimension),
                    completeness_label(descriptor.completeness),
                    descriptor.known_blind_spots.join("; ")
                ),
            },
            CoverageClaimKind::BoundedNegative if descriptor.known_blind_spots.is_empty() => {
                CoverageClaimDecision {
                    decision: ClaimGateDecision::Allowed,
                    rule: "coverage_descriptor_allows_absence_claim".to_string(),
                    reason: "descriptor declares no relevant blind spots for this dimension"
                        .to_string(),
                }
            }
            CoverageClaimKind::BoundedNegative => CoverageClaimDecision {
                decision: ClaimGateDecision::Blocked,
                rule: "coverage_descriptor_blocks_absence_claim".to_string(),
                reason: format!(
                    "{} blind spots can hide the requested absence: {}",
                    dimension_label(descriptor.dimension),
                    descriptor.known_blind_spots.join("; ")
                ),
            },
        }
    }
}

fn dimension_label(dimension: EffectDimension) -> &'static str {
    match dimension {
        EffectDimension::Filesystem => "filesystem",
        EffectDimension::Network => "network",
        EffectDimension::Process => "process",
    }
}

fn completeness_label(completeness: CoverageCompleteness) -> &'static str {
    match completeness {
        CoverageCompleteness::Full => "full",
        CoverageCompleteness::OpenSyscallOnly => "open_syscall_only",
        CoverageCompleteness::ConnectOnly => "connect_only",
        CoverageCompleteness::ExecOnly => "exec_only",
    }
}
