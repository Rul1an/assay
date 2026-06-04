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

        if descriptor.schema != COVERAGE_DESCRIPTOR_SCHEMA {
            return CoverageClaimDecision {
                decision: ClaimGateDecision::Blocked,
                rule: "coverage_descriptor_schema_mismatch".to_string(),
                reason: format!(
                    "coverage descriptor schema must be {}, found {}",
                    COVERAGE_DESCRIPTOR_SCHEMA, descriptor.schema
                ),
            };
        }

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
            CoverageClaimKind::ExhaustiveSet if descriptor.supports_complete_claims() => {
                CoverageClaimDecision {
                    decision: ClaimGateDecision::Allowed,
                    rule: "coverage_descriptor_allows_exhaustive_claim".to_string(),
                    reason: "descriptor completeness is full and declares no blind spots"
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
                    blind_spot_summary(descriptor)
                ),
            },
            CoverageClaimKind::BoundedNegative if descriptor.supports_complete_claims() => {
                CoverageClaimDecision {
                    decision: ClaimGateDecision::Allowed,
                    rule: "coverage_descriptor_allows_absence_claim".to_string(),
                    reason: "descriptor completeness is full and declares no blind spots"
                        .to_string(),
                }
            }
            CoverageClaimKind::BoundedNegative => CoverageClaimDecision {
                decision: ClaimGateDecision::Blocked,
                rule: "coverage_descriptor_blocks_absence_claim".to_string(),
                reason: format!(
                    "{} completeness is {}; blind spots can hide the requested absence: {}",
                    dimension_label(descriptor.dimension),
                    completeness_label(descriptor.completeness),
                    blind_spot_summary(descriptor)
                ),
            },
        }
    }

    /// Effect-class-aware variant of [`Self::claim_decision_for`].
    ///
    /// This refines only `PositiveExistence`: it first applies the same
    /// descriptor-presence, schema, and claim-kind gate, and then, when that
    /// gate would allow the positive claim, additionally checks that the
    /// caller's `effect_class` is one this descriptor actually observes. A
    /// positive claim for a class outside `observes` is downgraded to
    /// `Degraded` rather than blanket-allowed, so callers cannot read a
    /// present descriptor as permission for an effect class it never captured.
    ///
    /// `ExhaustiveSet` and `BoundedNegative` are unaffected: they already gate
    /// on completeness and blind spots, which are class-independent. The
    /// existing [`Self::claim_decision_for`] is left unchanged for callers that
    /// scope the effect class themselves.
    #[must_use]
    pub fn claim_decision_for_effect(
        descriptor: Option<&Self>,
        claim_kind: CoverageClaimKind,
        effect_class: &str,
    ) -> CoverageClaimDecision {
        let base = Self::claim_decision_for(descriptor, claim_kind);
        if claim_kind != CoverageClaimKind::PositiveExistence {
            return base;
        }
        // Only refine a positive claim the base gate already allowed; missing
        // descriptor / schema mismatch are already blocked by `base`.
        let Some(descriptor) = descriptor else {
            return base;
        };
        if base.decision != ClaimGateDecision::Allowed {
            return base;
        }
        if descriptor.observes_effect_class(effect_class) {
            return base;
        }
        CoverageClaimDecision {
            decision: ClaimGateDecision::Degraded,
            rule: "coverage_descriptor_positive_class_not_observed".to_string(),
            reason: format!(
                "{} does not list the claimed effect class \"{}\" in observes: {}",
                dimension_label(descriptor.dimension),
                effect_class.trim(),
                observes_summary(descriptor)
            ),
        }
    }

    /// Conservative containment check: does any `observes` entry mention this
    /// effect class? `observes` carries free-text capture descriptions, so the
    /// match is a case-insensitive substring rather than an enum equality. An
    /// empty class never matches.
    #[must_use]
    pub fn observes_effect_class(&self, effect_class: &str) -> bool {
        let needle = effect_class.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return false;
        }
        self.observes
            .iter()
            .any(|observed| observed.to_ascii_lowercase().contains(&needle))
    }

    fn supports_complete_claims(&self) -> bool {
        self.completeness == CoverageCompleteness::Full && self.known_blind_spots.is_empty()
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

fn blind_spot_summary(descriptor: &CoverageDescriptor) -> String {
    if descriptor.known_blind_spots.is_empty() {
        "none declared".to_string()
    } else {
        descriptor.known_blind_spots.join("; ")
    }
}

fn observes_summary(descriptor: &CoverageDescriptor) -> String {
    if descriptor.observes.is_empty() {
        "none declared".to_string()
    } else {
        descriptor.observes.join("; ")
    }
}
