use super::model::{StepVerdict, TraceExplanation};

impl TraceExplanation {
    /// Format as terminal output with colors
    pub fn to_terminal(&self) -> String {
        let mut lines = Vec::new();

        lines.push(format!(
            "Policy: {} (v{})",
            self.policy_name, self.policy_version
        ));
        lines.push(format!(
            "Trace: {} steps ({} allowed, {} blocked)\n",
            self.total_steps, self.allowed_steps, self.blocked_steps
        ));

        lines.push("Timeline:".to_string());

        for step in &self.steps {
            let icon = match step.verdict {
                StepVerdict::Allowed => "✅",
                StepVerdict::Blocked => "❌",
                StepVerdict::Warning => "⚠️",
            };

            let args_str = step
                .args
                .as_ref()
                .map(|a| format!("({})", summarize_args(a)))
                .unwrap_or_default();

            let status = match step.verdict {
                StepVerdict::Allowed => "allowed".to_string(),
                StepVerdict::Blocked => "BLOCKED".to_string(),
                StepVerdict::Warning => "warning".to_string(),
            };

            lines.push(format!(
                "  [{}] {}{:<40} {} {}",
                step.index, step.tool, args_str, icon, status
            ));

            // Show blocking rule details
            if step.verdict == StepVerdict::Blocked {
                for eval in &step.rules_evaluated {
                    if !eval.passed {
                        lines.push(format!("      └── Rule: {}", eval.rule_id));
                        lines.push(format!("      └── Reason: {}", eval.explanation));
                    }
                }
            }
        }

        if !self.blocking_rules.is_empty() {
            lines.push(String::new());
            lines.push("Blocking Rules:".to_string());
            for rule in &self.blocking_rules {
                lines.push(format!("  - {}", rule));
            }
        }

        lines.join("\n")
    }

    /// Format as markdown
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        let status = if self.blocked_steps == 0 {
            "✅ PASS"
        } else {
            "❌ BLOCKED"
        };

        md.push_str(&format!("## Trace Explanation {}\n\n", status));
        md.push_str(&format!(
            "**Policy:** {} (v{})\n\n",
            self.policy_name, self.policy_version
        ));
        md.push_str("| Steps | Allowed | Blocked |\n");
        md.push_str("|-------|---------|----------|\n");
        md.push_str(&format!(
            "| {} | {} | {} |\n\n",
            self.total_steps, self.allowed_steps, self.blocked_steps
        ));

        md.push_str("### Timeline\n\n");
        md.push_str("| # | Tool | Verdict | Details |\n");
        md.push_str("|---|------|---------|----------|\n");

        for step in &self.steps {
            let icon = match step.verdict {
                StepVerdict::Allowed => "✅",
                StepVerdict::Blocked => "❌",
                StepVerdict::Warning => "⚠️",
            };

            let details = if step.verdict == StepVerdict::Blocked {
                step.rules_evaluated
                    .iter()
                    .filter(|e| !e.passed)
                    .map(|e| e.explanation.clone())
                    .collect::<Vec<_>>()
                    .join("; ")
            } else {
                String::new()
            };

            md.push_str(&format!(
                "| {} | `{}` | {} | {} |\n",
                step.index, step.tool, icon, details
            ));
        }

        if !self.blocking_rules.is_empty() {
            md.push_str("\n### Blocking Rules\n\n");
            for rule in &self.blocking_rules {
                md.push_str(&format!("- `{}`\n", rule));
            }
        }

        md
    }

    /// Format as HTML
    pub fn to_html(&self) -> String {
        let mut html = String::new();

        html.push_str("<!DOCTYPE html>\n<html><head>\n");
        html.push_str("<meta charset=\"utf-8\">\n");
        html.push_str("<title>Trace Explanation</title>\n");
        html.push_str("<style>\n");
        html.push_str("body { font-family: system-ui, sans-serif; max-width: 900px; margin: 2rem auto; padding: 0 1rem; }\n");
        html.push_str(".step { padding: 0.5rem; margin: 0.25rem 0; border-radius: 4px; }\n");
        html.push_str(".allowed { background: #d4edda; }\n");
        html.push_str(".blocked { background: #f8d7da; }\n");
        html.push_str(".warning { background: #fff3cd; }\n");
        html.push_str(".rule-detail { margin-left: 2rem; color: #666; font-size: 0.9em; }\n");
        html.push_str(
            "code { background: #f4f4f4; padding: 0.2rem 0.4rem; border-radius: 3px; }\n",
        );
        html.push_str("</style>\n</head><body>\n");

        let status = if self.blocked_steps == 0 {
            "✅ PASS"
        } else {
            "❌ BLOCKED"
        };
        html.push_str(&format!("<h1>Trace Explanation {}</h1>\n", status));
        html.push_str(&format!(
            "<p><strong>Policy:</strong> {} (v{})</p>\n",
            self.policy_name, self.policy_version
        ));
        html.push_str(&format!(
            "<p><strong>Summary:</strong> {} steps ({} allowed, {} blocked)</p>\n",
            self.total_steps, self.allowed_steps, self.blocked_steps
        ));

        html.push_str("<h2>Timeline</h2>\n");

        for step in &self.steps {
            let class = match step.verdict {
                StepVerdict::Allowed => "allowed",
                StepVerdict::Blocked => "blocked",
                StepVerdict::Warning => "warning",
            };

            let icon = match step.verdict {
                StepVerdict::Allowed => "✅",
                StepVerdict::Blocked => "❌",
                StepVerdict::Warning => "⚠️",
            };

            html.push_str(&format!("<div class=\"step {}\">\n", class));
            html.push_str(&format!(
                "  <strong>[{}]</strong> <code>{}</code> {}\n",
                step.index, step.tool, icon
            ));

            if step.verdict == StepVerdict::Blocked {
                for eval in &step.rules_evaluated {
                    if !eval.passed {
                        html.push_str(&format!(
                            "  <div class=\"rule-detail\">Rule: <code>{}</code> — {}</div>\n",
                            eval.rule_id, eval.explanation
                        ));
                    }
                }
            }

            html.push_str("</div>\n");
        }

        html.push_str("</body></html>");
        html
    }
}

fn summarize_args(args: &serde_json::Value) -> String {
    match args {
        serde_json::Value::Object(map) => map
            .iter()
            .take(2)
            .map(|(k, v)| {
                let v_str = match v {
                    serde_json::Value::String(s) => {
                        if s.len() > 20 {
                            format!("\"{}...\"", &s[..20])
                        } else {
                            format!("\"{}\"", s)
                        }
                    }
                    _ => v.to_string(),
                };
                format!("{}: {}", k, v_str)
            })
            .collect::<Vec<_>>()
            .join(", "),
        _ => args.to_string(),
    }
}
