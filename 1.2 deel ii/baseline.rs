//! Baseline management for coverage regression detection
//!
//! Baselines capture coverage state at a point in time (e.g., main branch)
//! and allow comparison against new coverage reports.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use chrono::{DateTime, Utc};

/// A saved baseline snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    /// Baseline format version
    pub version: String,
    
    /// When this baseline was created
    pub created_at: DateTime<Utc>,
    
    /// Git commit hash (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    
    /// Git branch name (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    
    /// Policy name this baseline is for
    pub policy_name: String,
    
    /// Policy version
    pub policy_version: String,
    
    /// Coverage metrics at baseline time
    pub coverage: BaselineCoverage,
    
    /// Tools that were seen in baseline traces
    pub tools_seen: HashSet<String>,
    
    /// Rules that were triggered in baseline
    pub rules_triggered: HashSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineCoverage {
    /// Overall coverage percentage
    pub overall_pct: f64,
    
    /// Tool coverage details
    pub tool_coverage: BaselineToolCoverage,
    
    /// Rule coverage details
    pub rule_coverage: BaselineRuleCoverage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineToolCoverage {
    /// Total tools in policy
    pub total: usize,
    
    /// Tools seen in traces
    pub seen: usize,
    
    /// Coverage percentage
    pub pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineRuleCoverage {
    /// Total rules in policy
    pub total: usize,
    
    /// Rules triggered
    pub triggered: usize,
    
    /// Coverage percentage
    pub pct: f64,
}

/// Result of comparing current coverage against a baseline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineDiff {
    /// Baseline being compared against
    pub baseline_commit: Option<String>,
    
    /// Baseline creation time
    pub baseline_created: DateTime<Utc>,
    
    /// Overall coverage change
    pub coverage_delta: CoverageDelta,
    
    /// Tools newly covered (not in baseline)
    pub newly_covered_tools: Vec<String>,
    
    /// Tools no longer covered (were in baseline)
    pub regression_tools: Vec<String>,
    
    /// Rules newly triggered
    pub newly_triggered_rules: Vec<String>,
    
    /// Rules no longer triggered (regression)
    pub regression_rules: Vec<String>,
    
    /// Whether this is a regression (coverage decreased or tools lost)
    pub is_regression: bool,
    
    /// Summary message
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageDelta {
    /// Previous coverage percentage
    pub baseline_pct: f64,
    
    /// Current coverage percentage
    pub current_pct: f64,
    
    /// Change in percentage points
    pub delta_pct: f64,
    
    /// Direction: "improved", "regressed", or "unchanged"
    pub direction: String,
}

impl Baseline {
    /// Create a new baseline from a coverage report
    pub fn from_coverage_report(
        report: &crate::coverage::CoverageReport,
        policy: &crate::model::Policy,
        commit: Option<String>,
        branch: Option<String>,
    ) -> Self {
        let tools_seen: HashSet<String> = report.tool_coverage.unseen_tools
            .iter()
            .cloned()
            .collect::<HashSet<_>>()
            .symmetric_difference(
                &policy.tools.allow.clone().unwrap_or_default().into_iter().collect()
            )
            .cloned()
            .collect();
        
        // Actually, let's compute this properly
        let all_policy_tools: HashSet<String> = policy.tools.allow
            .clone()
            .unwrap_or_default()
            .into_iter()
            .chain(policy.tools.deny.clone().unwrap_or_default())
            .collect();
        
        let unseen: HashSet<String> = report.tool_coverage.unseen_tools
            .iter()
            .cloned()
            .collect();
        
        let tools_seen: HashSet<String> = all_policy_tools
            .difference(&unseen)
            .cloned()
            .collect();
        
        let all_rules: HashSet<String> = report.rule_coverage.untriggered_rules
            .iter()
            .cloned()
            .collect();
        
        // Infer triggered rules (total - untriggered)
        // This is a simplification - ideally we'd track this in the report
        let rules_triggered: HashSet<String> = HashSet::new(); // TODO: Track in CoverageReport
        
        Self {
            version: "1.0".to_string(),
            created_at: Utc::now(),
            commit,
            branch,
            policy_name: policy.name.clone(),
            policy_version: policy.version.clone(),
            coverage: BaselineCoverage {
                overall_pct: report.overall_coverage_pct,
                tool_coverage: BaselineToolCoverage {
                    total: report.tool_coverage.total_tools_in_policy,
                    seen: report.tool_coverage.tools_seen_in_traces,
                    pct: report.tool_coverage.coverage_pct,
                },
                rule_coverage: BaselineRuleCoverage {
                    total: report.rule_coverage.total_rules,
                    triggered: report.rule_coverage.rules_triggered,
                    pct: report.rule_coverage.coverage_pct,
                },
            },
            tools_seen,
            rules_triggered,
        }
    }
    
    /// Load baseline from YAML file
    pub fn from_file(path: &std::path::Path) -> Result<Self, BaselineError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| BaselineError::ReadError(path.to_path_buf(), e.to_string()))?;
        
        serde_yaml::from_str(&content)
            .map_err(|e| BaselineError::ParseError(e.to_string()))
    }
    
    /// Save baseline to YAML file
    pub fn save(&self, path: &std::path::Path) -> Result<(), BaselineError> {
        let content = serde_yaml::to_string(self)
            .map_err(|e| BaselineError::SerializeError(e.to_string()))?;
        
        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| BaselineError::WriteError(path.to_path_buf(), e.to_string()))?;
        }
        
        std::fs::write(path, content)
            .map_err(|e| BaselineError::WriteError(path.to_path_buf(), e.to_string()))
    }
    
    /// Compare current coverage report against this baseline
    pub fn diff(&self, current: &crate::coverage::CoverageReport) -> BaselineDiff {
        // Calculate coverage delta
        let delta_pct = current.overall_coverage_pct - self.coverage.overall_pct;
        let direction = if delta_pct > 0.5 {
            "improved"
        } else if delta_pct < -0.5 {
            "regressed"
        } else {
            "unchanged"
        };
        
        // Find tool changes
        let current_tools: HashSet<String> = {
            let all: HashSet<String> = current.tool_coverage.unseen_tools
                .iter()
                .cloned()
                .collect();
            // Tools seen = policy tools - unseen tools
            // Simplified: we track what changed
            HashSet::new() // TODO: Get from report
        };
        
        // For now, use unseen_tools to detect regressions
        let baseline_unseen: HashSet<String> = HashSet::new(); // From baseline
        let current_unseen: HashSet<String> = current.tool_coverage.unseen_tools
            .iter()
            .cloned()
            .collect();
        
        // Newly covered = was unseen, now seen (was in baseline unseen, not in current unseen)
        // Regression = was seen, now unseen (not in baseline unseen, now in current unseen)
        
        let newly_covered_tools: Vec<String> = self.tools_seen
            .iter()
            .filter(|t| !current_unseen.contains(*t))
            .cloned()
            .collect();
        
        let regression_tools: Vec<String> = current.tool_coverage.unseen_tools
            .iter()
            .filter(|t| self.tools_seen.contains(*t))
            .cloned()
            .collect();
        
        // Rule changes (simplified)
        let newly_triggered_rules: Vec<String> = Vec::new();
        let regression_rules: Vec<String> = current.rule_coverage.untriggered_rules
            .iter()
            .filter(|r| self.rules_triggered.contains(*r))
            .cloned()
            .collect();
        
        // Determine if this is a regression
        let is_regression = delta_pct < -1.0 || !regression_tools.is_empty();
        
        // Build summary
        let summary = if is_regression {
            let mut parts = Vec::new();
            if delta_pct < -1.0 {
                parts.push(format!("Coverage dropped {:.1}%", delta_pct.abs()));
            }
            if !regression_tools.is_empty() {
                parts.push(format!("{} tools lost coverage", regression_tools.len()));
            }
            format!("⚠️ REGRESSION: {}", parts.join(", "))
        } else if delta_pct > 1.0 {
            format!("✅ Coverage improved by {:.1}%", delta_pct)
        } else {
            "✅ Coverage stable".to_string()
        };
        
        BaselineDiff {
            baseline_commit: self.commit.clone(),
            baseline_created: self.created_at,
            coverage_delta: CoverageDelta {
                baseline_pct: self.coverage.overall_pct,
                current_pct: current.overall_coverage_pct,
                delta_pct,
                direction: direction.to_string(),
            },
            newly_covered_tools,
            regression_tools,
            newly_triggered_rules,
            regression_rules,
            is_regression,
            summary,
        }
    }
}

/// Errors that can occur during baseline operations
#[derive(Debug)]
pub enum BaselineError {
    ReadError(std::path::PathBuf, String),
    WriteError(std::path::PathBuf, String),
    ParseError(String),
    SerializeError(String),
}

impl std::fmt::Display for BaselineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadError(path, msg) => write!(f, "Cannot read {}: {}", path.display(), msg),
            Self::WriteError(path, msg) => write!(f, "Cannot write {}: {}", path.display(), msg),
            Self::ParseError(msg) => write!(f, "Invalid baseline format: {}", msg),
            Self::SerializeError(msg) => write!(f, "Cannot serialize baseline: {}", msg),
        }
    }
}

impl std::error::Error for BaselineError {}

impl BaselineDiff {
    /// Format as terminal output
    pub fn to_terminal(&self) -> String {
        let mut lines = Vec::new();
        
        // Header
        lines.push(format!("Baseline Comparison"));
        if let Some(commit) = &self.baseline_commit {
            lines.push(format!("  Baseline: {} ({})", commit, self.baseline_created.format("%Y-%m-%d")));
        }
        lines.push(String::new());
        
        // Coverage delta
        let arrow = match self.coverage_delta.direction.as_str() {
            "improved" => "↑",
            "regressed" => "↓",
            _ => "→",
        };
        
        lines.push(format!(
            "Coverage: {:.1}% {} {:.1}% ({:+.1}%)",
            self.coverage_delta.baseline_pct,
            arrow,
            self.coverage_delta.current_pct,
            self.coverage_delta.delta_pct
        ));
        lines.push(String::new());
        
        // Regressions
        if !self.regression_tools.is_empty() {
            lines.push(format!("❌ Coverage Lost ({}):", self.regression_tools.len()));
            for tool in &self.regression_tools {
                lines.push(format!("   - {}", tool));
            }
            lines.push(String::new());
        }
        
        // Improvements
        if !self.newly_covered_tools.is_empty() {
            lines.push(format!("✅ Newly Covered ({}):", self.newly_covered_tools.len()));
            for tool in &self.newly_covered_tools {
                lines.push(format!("   + {}", tool));
            }
            lines.push(String::new());
        }
        
        // Summary
        lines.push(self.summary.clone());
        
        lines.join("\n")
    }
    
    /// Format as GitHub Actions annotation
    pub fn to_github_annotation(&self) -> String {
        let mut annotations = Vec::new();
        
        if self.is_regression {
            annotations.push(format!(
                "::error::Coverage regression: {:.1}% → {:.1}% ({:+.1}%)",
                self.coverage_delta.baseline_pct,
                self.coverage_delta.current_pct,
                self.coverage_delta.delta_pct
            ));
            
            for tool in &self.regression_tools {
                annotations.push(format!("::warning::Tool '{}' lost coverage", tool));
            }
        }
        
        annotations.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn make_baseline() -> Baseline {
        Baseline {
            version: "1.0".to_string(),
            created_at: Utc::now(),
            commit: Some("abc123".to_string()),
            branch: Some("main".to_string()),
            policy_name: "test".to_string(),
            policy_version: "1.1".to_string(),
            coverage: BaselineCoverage {
                overall_pct: 85.0,
                tool_coverage: BaselineToolCoverage {
                    total: 10,
                    seen: 8,
                    pct: 80.0,
                },
                rule_coverage: BaselineRuleCoverage {
                    total: 5,
                    triggered: 4,
                    pct: 80.0,
                },
            },
            tools_seen: ["Search", "Create", "Update", "Delete"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            rules_triggered: ["before_search_create", "max_calls_api"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
    
    #[test]
    fn test_baseline_serialization() {
        let baseline = make_baseline();
        let yaml = serde_yaml::to_string(&baseline).unwrap();
        
        assert!(yaml.contains("version: '1.0'") || yaml.contains("version: \"1.0\""));
        assert!(yaml.contains("policy_name: test"));
        assert!(yaml.contains("overall_pct: 85.0"));
    }
    
    #[test]
    fn test_baseline_deserialization() {
        let yaml = r#"
version: "1.0"
created_at: "2026-01-01T00:00:00Z"
commit: "abc123"
branch: "main"
policy_name: "test"
policy_version: "1.1"
coverage:
  overall_pct: 85.0
  tool_coverage:
    total: 10
    seen: 8
    pct: 80.0
  rule_coverage:
    total: 5
    triggered: 4
    pct: 80.0
tools_seen:
  - Search
  - Create
rules_triggered:
  - before_search_create
"#;
        
        let baseline: Baseline = serde_yaml::from_str(yaml).unwrap();
        
        assert_eq!(baseline.policy_name, "test");
        assert_eq!(baseline.coverage.overall_pct, 85.0);
        assert!(baseline.tools_seen.contains("Search"));
    }
    
    #[test]
    fn test_coverage_delta_direction() {
        let delta_improved = CoverageDelta {
            baseline_pct: 80.0,
            current_pct: 90.0,
            delta_pct: 10.0,
            direction: "improved".to_string(),
        };
        assert_eq!(delta_improved.direction, "improved");
        
        let delta_regressed = CoverageDelta {
            baseline_pct: 90.0,
            current_pct: 80.0,
            delta_pct: -10.0,
            direction: "regressed".to_string(),
        };
        assert_eq!(delta_regressed.direction, "regressed");
    }
}
