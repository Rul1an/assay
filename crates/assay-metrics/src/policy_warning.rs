use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

type WarningKey = (String, String);

fn warning_cache() -> &'static Mutex<HashSet<WarningKey>> {
    static CACHE: OnceLock<Mutex<HashSet<WarningKey>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashSet::new()))
}

fn should_emit_deprecated_policy_warning_impl(
    metric_name: &str,
    policy_path: &str,
    suppressed: bool,
) -> bool {
    if suppressed {
        return false;
    }

    let key = (metric_name.to_string(), policy_path.to_string());
    let mut cache = warning_cache()
        .lock()
        .expect("policy warning cache mutex must not be poisoned");
    cache.insert(key)
}

pub(crate) fn should_emit_deprecated_policy_warning(metric_name: &str, policy_path: &str) -> bool {
    let suppressed = std::env::var("MCP_CONFIG_LEGACY").is_ok();
    should_emit_deprecated_policy_warning_impl(metric_name, policy_path, suppressed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_key(label: &str) -> (String, String) {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        (
            format!("{}_metric_{}", label, id),
            format!("/tmp/{}_policy_{}.yaml", label, id),
        )
    }

    #[test]
    fn emits_once_per_metric_and_path() {
        let (metric, path) = unique_key("once");

        assert!(should_emit_deprecated_policy_warning_impl(
            &metric, &path, false
        ));
        assert!(!should_emit_deprecated_policy_warning_impl(
            &metric, &path, false
        ));
    }

    #[test]
    fn emits_for_different_metrics_and_paths() {
        let (metric_a, path_a) = unique_key("metric_a");
        let (metric_b, path_b) = unique_key("metric_b");
        let (_, path_c) = unique_key("metric_a_second_path");

        assert!(should_emit_deprecated_policy_warning_impl(
            &metric_a, &path_a, false
        ));
        assert!(should_emit_deprecated_policy_warning_impl(
            &metric_b, &path_b, false
        ));
        assert!(should_emit_deprecated_policy_warning_impl(
            &metric_a, &path_c, false
        ));
    }

    #[test]
    fn suppressed_mode_does_not_consume_key() {
        let (metric, path) = unique_key("suppressed");

        assert!(!should_emit_deprecated_policy_warning_impl(
            &metric, &path, true
        ));
        assert!(should_emit_deprecated_policy_warning_impl(
            &metric, &path, false
        ));
    }
}
