use super::{CoverageReport, HighRiskGap, RuleCoverage, ToolCoverage, TraceRecord};
use std::collections::{HashMap, HashSet};

/// Coverage analyzer
pub struct CoverageAnalyzer {
    /// Tools referenced in policy (from allow, deny, sequences)
    policy_tools: HashSet<String>,

    /// High-risk tools (from deny list, blocklist patterns)
    high_risk_tools: HashSet<String>,

    /// Rule IDs in policy
    rule_ids: Vec<String>,

    /// Resolved aliases (alias -> members)
    aliases: HashMap<String, Vec<String>>,
}

impl CoverageAnalyzer {
    /// Create analyzer from a v1.1 policy
    pub fn from_policy(policy: &crate::model::Policy) -> Self {
        let mut policy_tools = HashSet::new();
        let mut high_risk_tools = HashSet::new();
        let mut rule_ids = Vec::new();

        // Extract tools from policy.tools
        if let Some(allow) = &policy.tools.allow {
            for tool in allow {
                policy_tools.insert(tool.clone());
            }
        }

        if let Some(deny) = &policy.tools.deny {
            for tool in deny {
                policy_tools.insert(tool.clone());
                high_risk_tools.insert(tool.clone()); // Denied = high risk
            }
        }

        if let Some(require_args) = &policy.tools.require_args {
            for tool in require_args.keys() {
                policy_tools.insert(tool.clone());
            }
        }

        // Extract tools from sequences
        for (idx, rule) in policy.sequences.iter().enumerate() {
            let rule_id = Self::rule_id(rule, idx);
            rule_ids.push(rule_id);

            match rule {
                crate::model::SequenceRule::Require { tool } => {
                    policy_tools.insert(tool.clone());
                }
                crate::model::SequenceRule::Eventually { tool, .. } => {
                    policy_tools.insert(tool.clone());
                }
                crate::model::SequenceRule::MaxCalls { tool, .. } => {
                    policy_tools.insert(tool.clone());
                }
                crate::model::SequenceRule::Before { first, then } => {
                    policy_tools.insert(first.clone());
                    policy_tools.insert(then.clone());
                }
                crate::model::SequenceRule::After { trigger, then, .. } => {
                    policy_tools.insert(trigger.clone());
                    policy_tools.insert(then.clone());
                }
                crate::model::SequenceRule::NeverAfter { trigger, forbidden } => {
                    policy_tools.insert(trigger.clone());
                    policy_tools.insert(forbidden.clone());
                    high_risk_tools.insert(forbidden.clone()); // Forbidden = high risk
                }
                crate::model::SequenceRule::Sequence { tools, .. } => {
                    for tool in tools {
                        policy_tools.insert(tool.clone());
                    }
                }
                crate::model::SequenceRule::Blocklist { pattern } => {
                    // Pattern-based, mark as high risk indicator
                    high_risk_tools.insert(format!("*{}*", pattern));
                }
            }
        }

        // Resolve aliases - add alias members to policy_tools
        for (alias, members) in &policy.aliases {
            policy_tools.insert(alias.clone());
            for member in members {
                policy_tools.insert(member.clone());
            }
        }

        Self {
            policy_tools,
            high_risk_tools,
            rule_ids,
            aliases: policy.aliases.clone(),
        }
    }

    /// Generate a rule ID from rule type and index
    fn rule_id(rule: &crate::model::SequenceRule, _idx: usize) -> String {
        match rule {
            crate::model::SequenceRule::Require { tool } => {
                format!("require_{}", tool.to_lowercase())
            }
            crate::model::SequenceRule::Eventually { tool, within } => {
                format!("eventually_{}_{}", tool.to_lowercase(), within)
            }
            crate::model::SequenceRule::MaxCalls { tool, max } => {
                format!("max_calls_{}_{}", tool.to_lowercase(), max)
            }
            crate::model::SequenceRule::Before { first, then } => {
                format!(
                    "before_{}_then_{}",
                    first.to_lowercase(),
                    then.to_lowercase()
                )
            }
            crate::model::SequenceRule::After { trigger, then, .. } => {
                format!(
                    "after_{}_then_{}",
                    trigger.to_lowercase(),
                    then.to_lowercase()
                )
            }
            crate::model::SequenceRule::NeverAfter { trigger, forbidden } => {
                format!(
                    "never_after_{}_forbidden_{}",
                    trigger.to_lowercase(),
                    forbidden.to_lowercase()
                )
            }
            crate::model::SequenceRule::Sequence { tools, strict } => {
                let mode = if *strict { "strict" } else { "seq" };
                format!("{}_{}", mode, tools.join("_").to_lowercase())
            }
            crate::model::SequenceRule::Blocklist { pattern } => {
                format!("blocklist_{}", pattern.to_lowercase())
            }
        }
    }

    /// Analyze coverage from a set of traces
    pub fn analyze(&self, traces: &[TraceRecord], threshold: f64) -> CoverageReport {
        let mut tools_seen: HashSet<String> = HashSet::new();
        let mut rules_triggered: HashSet<String> = HashSet::new();
        let mut unexpected_tools: HashSet<String> = HashSet::new();

        // Collect all tools and triggered rules from traces
        for trace in traces {
            for tool in &trace.tools_called {
                tools_seen.insert(tool.clone());

                // Check if tool is in policy (including alias resolution)
                if !self.is_policy_tool(tool) {
                    unexpected_tools.insert(tool.clone());
                }
            }

            for rule_id in &trace.rules_triggered {
                rules_triggered.insert(rule_id.clone());
            }
        }

        // Calculate tool coverage
        let policy_tool_count = self.policy_tools.len();
        let seen_policy_tools: HashSet<_> = tools_seen
            .iter()
            .filter(|t| self.is_policy_tool(t))
            .cloned()
            .collect();
        let tools_seen_count = seen_policy_tools.len();

        let unseen_tools: Vec<String> = self
            .policy_tools
            .iter()
            .filter(|t| !self.is_tool_seen(t, &tools_seen))
            .cloned()
            .collect();

        let tool_coverage_pct = if policy_tool_count > 0 {
            (tools_seen_count as f64 / policy_tool_count as f64) * 100.0
        } else {
            100.0
        };

        // Calculate rule coverage
        let total_rules = self.rule_ids.len();
        let triggered_count = rules_triggered.len();

        let untriggered_rules: Vec<String> = self
            .rule_ids
            .iter()
            .filter(|r| !rules_triggered.contains(*r))
            .cloned()
            .collect();

        let rule_coverage_pct = if total_rules > 0 {
            (triggered_count as f64 / total_rules as f64) * 100.0
        } else {
            100.0
        };

        // Identify high-risk gaps
        let high_risk_gaps: Vec<HighRiskGap> = self
            .high_risk_tools
            .iter()
            .filter(|t| !t.starts_with('*')) // Skip patterns
            .filter(|t| !self.is_tool_seen(t, &tools_seen))
            .map(|t| HighRiskGap {
                tool: t.clone(),
                reason: "Tool is in deny list but never appeared in test traces".to_string(),
                severity: "high".to_string(),
            })
            .collect();

        // Overall coverage (average of tool and rule coverage)
        let overall_coverage_pct = (tool_coverage_pct + rule_coverage_pct) / 2.0;
        let meets_threshold = overall_coverage_pct >= threshold;

        CoverageReport {
            tool_coverage: ToolCoverage {
                total_tools_in_policy: policy_tool_count,
                tools_seen_in_traces: tools_seen_count,
                coverage_pct: tool_coverage_pct,
                unseen_tools,
                unexpected_tools: unexpected_tools.into_iter().collect(),
            },
            rule_coverage: RuleCoverage {
                total_rules,
                rules_triggered: triggered_count,
                coverage_pct: rule_coverage_pct,
                untriggered_rules,
            },
            high_risk_gaps,
            policy_violations: Vec::new(),
            policy_warnings: Vec::new(),
            overall_coverage_pct,
            meets_threshold,
            threshold,
        }
    }

    /// Check if a tool is in the policy (including alias resolution)
    fn is_policy_tool(&self, tool: &str) -> bool {
        if self.policy_tools.contains(tool) {
            return true;
        }

        // Check if tool is a member of any alias
        for members in self.aliases.values() {
            if members.contains(&tool.to_string()) {
                return true;
            }
        }

        false
    }

    /// Check if a tool (or any of its alias members) was seen
    fn is_tool_seen(&self, tool: &str, seen: &HashSet<String>) -> bool {
        if seen.contains(tool) {
            return true;
        }

        // Check if this tool is an alias and any member was seen
        if let Some(members) = self.aliases.get(tool) {
            return members.iter().any(|m| seen.contains(m));
        }

        // Check if tool is a member of an alias that was seen
        for (alias, members) in &self.aliases {
            if members.contains(&tool.to_string()) && seen.contains(alias) {
                return true;
            }
        }

        false
    }
}
