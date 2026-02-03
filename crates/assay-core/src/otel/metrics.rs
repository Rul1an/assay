use lazy_static::lazy_static;
use std::collections::HashSet;
use std::sync::Mutex;

lazy_static! {
    static ref FORBIDDEN_LABELS: HashSet<&'static str> = {
        let mut s = HashSet::new();
        s.insert("trace_id");
        s.insert("span_id");
        s.insert("user_id");
        s.insert("prompt_hash");
        s.insert("file_path");
        s
    };
}

pub struct MetricRegistry {
    // In real OTel context, this wraps MeterProvider
    // For MVP, checking logic.
    #[allow(dead_code)]
    registered_instruments: Mutex<HashSet<String>>,
}

impl Default for MetricRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricRegistry {
    pub fn new() -> Self {
        Self {
            registered_instruments: Mutex::new(HashSet::new()),
        }
    }

    /// Register a metric with validated labels.
    pub fn register_counter(&self, _name: &str, labels: &[&str]) {
        if let Ok(_safe_labels) = self.filter_labels(labels) {
            // ... call OTel SDK with safe_labels ...
            // In a real impl, we'd pass safe_labels to the SDK.
        } else {
            // Dropped (Fail-closed or Logged as per config)
        }
    }

    pub fn register_histogram(&self, _name: &str, labels: &[&str]) {
        if let Ok(_safe_labels) = self.filter_labels(labels) {
            // ... call OTel SDK ...
        }
    }

    fn filter_labels<'a>(&self, labels: &'a [&'a str]) -> Result<Vec<&'a str>, String> {
        let mut safe = Vec::with_capacity(labels.len());
        for label in labels {
            if FORBIDDEN_LABELS.contains(label) {
                let msg = format!("Cardinality Violation: Forbidden metric label '{}'", label);
                eprintln!(
                    "ERROR: Dropping forbidden metric label '{}' to prevent hash collision DoS",
                    label
                );
                return Err(msg);
            }
            safe.push(*label);
        }
        Ok(safe)
    }

    /// Check that labels are allowed (E8.2 low-cardinality enforcement). Used by tests and callers that need to validate before registering.
    pub fn check_labels(&self, labels: &[&str]) -> Result<(), String> {
        self.filter_labels(labels).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cardinality_forbidden_labels_rejected() {
        let r = MetricRegistry::new();
        assert!(r.check_labels(&["model", "operation"]).is_ok());
        assert!(r.check_labels(&["user_id"]).is_err());
        assert!(r.check_labels(&["trace_id"]).is_err());
        assert!(r.check_labels(&["prompt_hash"]).is_err());
        assert!(r.check_labels(&["file_path"]).is_err());
        assert!(r.check_labels(&["model", "user_id"]).is_err());
    }
}
