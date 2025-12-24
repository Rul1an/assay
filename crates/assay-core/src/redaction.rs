use std::borrow::Cow;

#[derive(Debug, Clone, Copy, Default)]
pub struct RedactionPolicy {
    pub redact_prompts: bool,
}

impl RedactionPolicy {
    pub fn new(redact_prompts: bool) -> Self {
        Self { redact_prompts }
    }

    pub fn redact_prompt<'a>(&self, s: &'a str) -> Cow<'a, str> {
        if self.redact_prompts {
            "[REDACTED]".into()
        } else {
            s.into()
        }
    }

    pub fn redact_judge_metadata(&self, meta: &mut serde_json::Value) {
        if !self.redact_prompts {
            return;
        }

        if let Some(obj) = meta
            .pointer_mut("/assay/judge")
            .and_then(|v| v.as_object_mut())
        {
            for (_, v) in obj.iter_mut() {
                if let Some(inner) = v.as_object_mut() {
                    if inner.contains_key("rationale") {
                        inner.insert("rationale".to_string(), serde_json::json!("[REDACTED]"));
                    }
                }
            }
        }

        // 2. Redact from TestResultRow.details style (metrics output)
        if let Some(metrics) = meta.pointer_mut("/metrics").and_then(|v| v.as_object_mut()) {
            for (_, metric_res) in metrics.iter_mut() {
                if let Some(details) = metric_res
                    .get_mut("details")
                    .and_then(|v| v.as_object_mut())
                {
                    if details.contains_key("rationale") {
                        details.insert("rationale".to_string(), serde_json::json!("[REDACTED]"));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redaction_on() {
        let policy = RedactionPolicy::new(true);
        assert_eq!(policy.redact_prompt("my secret prompt"), "[REDACTED]");
    }

    #[test]
    fn test_redaction_off() {
        let policy = RedactionPolicy::new(false);
        assert_eq!(policy.redact_prompt("safe prompt"), "safe prompt");
    }
}
