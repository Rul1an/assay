use super::CoverageReport;

impl CoverageReport {
    /// Format as GitHub Actions annotation
    pub fn to_github_annotation(&self) -> String {
        let mut lines = Vec::new();

        if !self.meets_threshold {
            lines.push(format!(
                "::error::Coverage {:.1}% is below threshold {:.1}%",
                self.overall_coverage_pct, self.threshold
            ));
        }

        for gap in &self.high_risk_gaps {
            lines.push(format!(
                "::warning::High-risk tool '{}' never tested: {}",
                gap.tool, gap.reason
            ));
        }

        for tool in &self.tool_coverage.unseen_tools {
            lines.push(format!(
                "::notice::Tool '{}' in policy but not covered by tests",
                tool
            ));
        }

        lines.join("\n")
    }

    /// Format as markdown summary
    pub fn to_markdown(&self) -> String {
        let status = if self.meets_threshold { "✅" } else { "❌" };

        let mut md = format!(
            "## Coverage Report {}\n\n\
            | Metric | Value |\n\
            |--------|-------|\n\
            | Overall Coverage | {:.1}% |\n\
            | Tool Coverage | {:.1}% ({}/{}) |\n\
            | Rule Coverage | {:.1}% ({}/{}) |\n\
            | Threshold | {:.1}% |\n\n",
            status,
            self.overall_coverage_pct,
            self.tool_coverage.coverage_pct,
            self.tool_coverage.tools_seen_in_traces,
            self.tool_coverage.total_tools_in_policy,
            self.rule_coverage.coverage_pct,
            self.rule_coverage.rules_triggered,
            self.rule_coverage.total_rules,
            self.threshold,
        );

        if !self.high_risk_gaps.is_empty() {
            md.push_str("### ⚠️ High-Risk Gaps\n\n");
            for gap in &self.high_risk_gaps {
                md.push_str(&format!("- **{}**: {}\n", gap.tool, gap.reason));
            }
            md.push('\n');
        }

        if !self.tool_coverage.unseen_tools.is_empty() {
            md.push_str("### Uncovered Tools\n\n");
            for tool in &self.tool_coverage.unseen_tools {
                md.push_str(&format!("- `{}`\n", tool));
            }
            md.push('\n');
        }

        if !self.rule_coverage.untriggered_rules.is_empty() {
            md.push_str("### Untriggered Rules\n\n");
            for rule in &self.rule_coverage.untriggered_rules {
                md.push_str(&format!("- `{}`\n", rule));
            }
            md.push('\n');
        }

        md
    }
}
