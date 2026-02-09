use std::collections::{HashSet, VecDeque};
use std::sync::{Mutex, MutexGuard, OnceLock};

type WarningKey = (String, String);
const MAX_WARNING_KEYS: usize = 4_096;

#[derive(Default)]
struct WarningCache {
    seen: HashSet<WarningKey>,
    order: VecDeque<WarningKey>,
}

impl WarningCache {
    fn insert(&mut self, key: WarningKey) -> bool {
        if !self.seen.insert(key.clone()) {
            return false;
        }

        self.order.push_back(key);
        if self.order.len() > MAX_WARNING_KEYS {
            if let Some(oldest) = self.order.pop_front() {
                self.seen.remove(&oldest);
            }
        }
        true
    }
}

fn warning_cache() -> &'static Mutex<WarningCache> {
    static CACHE: OnceLock<Mutex<WarningCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(WarningCache::default()))
}

fn lock_warning_cache() -> MutexGuard<'static, WarningCache> {
    // Recover poisoned mutex state rather than panicking in warning-only path.
    warning_cache().lock().unwrap_or_else(|p| p.into_inner())
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
    let mut cache = lock_warning_cache();
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

    #[test]
    fn cache_is_bounded_and_old_entries_evict() {
        let stable_metric = "bounded_metric".to_string();
        let stable_path = "/tmp/bounded_policy.yaml".to_string();
        assert!(should_emit_deprecated_policy_warning_impl(
            &stable_metric,
            &stable_path,
            false
        ));
        assert!(!should_emit_deprecated_policy_warning_impl(
            &stable_metric,
            &stable_path,
            false
        ));

        for i in 0..(MAX_WARNING_KEYS + 32) {
            let metric = format!("m_{}", i);
            let path = format!("/tmp/p_{}.yaml", i);
            let _ = should_emit_deprecated_policy_warning_impl(&metric, &path, false);
        }

        // After bounded eviction, older keys can be emitted again.
        assert!(should_emit_deprecated_policy_warning_impl(
            &stable_metric,
            &stable_path,
            false
        ));
    }
}
