use serde::{Deserialize, Serialize};
use std::collections::HashSet;

impl ToolCoverage {
    /// Whether the tool dimension was declared by the policy.
    ///
    /// An empty dimension is not applicable: it never reads 100 and never
    /// enters the overall mean. See [`CoverageReport::dimension_is_applicable`].
    pub fn is_applicable(&self) -> bool {
        CoverageReport::dimension_is_applicable(self.total_tools_in_policy)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCoverage {
    /// Total unique tools referenced in policy
    pub total_tools_in_policy: usize,

    /// Tools that appeared in at least one trace
    pub tools_seen_in_traces: usize,

    /// Coverage percentage
    pub coverage_pct: f64,

    /// Tools in policy but never seen
    pub unseen_tools: Vec<String>,

    /// Tools seen in traces but not in policy (potential gaps)
    pub unexpected_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyViolation {
    pub trace_id: String,
    pub tool: String,
    pub error_code: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyWarning {
    pub trace_id: String,
    pub tool: String,
    pub warning_code: String,
    pub reason: String,
}

/// Coverage analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageReport {
    /// Tool coverage metrics
    pub tool_coverage: ToolCoverage,

    /// Rule coverage metrics
    pub rule_coverage: RuleCoverage,

    /// High-risk gaps (blocklisted tools never seen)
    pub high_risk_gaps: Vec<HighRiskGap>,

    /// Policy violations found during analysis
    #[serde(default)]
    pub policy_violations: Vec<PolicyViolation>,

    /// Policy warnings (e.g. unconstrained tools)
    #[serde(default)]
    pub policy_warnings: Vec<PolicyWarning>,

    /// Overall coverage percentage
    pub overall_coverage_pct: f64,

    /// Whether coverage meets threshold
    pub meets_threshold: bool,

    /// Threshold that was checked
    pub threshold: f64,
}

impl CoverageReport {
    /// The single applicability predicate: a dimension applies iff the policy
    /// declares anything in it. The analyzer, the renderers, and the baseline
    /// writer all funnel through this function, so the rule cannot drift.
    pub fn dimension_is_applicable(declared_total: usize) -> bool {
        declared_total > 0
    }

    /// Number of applicable dimensions (0, 1, or 2).
    pub fn applicable_dimensions(&self) -> usize {
        usize::from(self.tool_coverage.is_applicable())
            + usize::from(self.rule_coverage.is_applicable())
    }

    /// Why no threshold can be met, when no dimension is applicable.
    pub fn not_applicable_reason(&self) -> Option<&'static str> {
        if self.applicable_dimensions() == 0 {
            Some("Coverage not applicable: policy declares no tools and no rules")
        } else {
            None
        }
    }
}

impl RuleCoverage {
    /// Whether the rule dimension was declared by the policy.
    ///
    /// An empty dimension is not applicable: it never reads 100 and never
    /// enters the overall mean. See [`CoverageReport::dimension_is_applicable`].
    pub fn is_applicable(&self) -> bool {
        CoverageReport::dimension_is_applicable(self.total_rules)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleCoverage {
    /// Total rules in policy
    pub total_rules: usize,

    /// Rules that were triggered (evaluated to allow or deny)
    pub rules_triggered: usize,

    /// Coverage percentage
    pub coverage_pct: f64,

    /// Rules that were never triggered
    pub untriggered_rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighRiskGap {
    /// Tool name
    pub tool: String,

    /// Why it's high risk
    pub reason: String,

    /// Severity: "critical", "high", "medium"
    pub severity: String,
}

/// Trace data for coverage analysis
#[derive(Debug, Clone)]
pub struct TraceRecord {
    pub trace_id: String,
    pub tools_called: Vec<String>,
    pub rules_triggered: HashSet<String>,
}
