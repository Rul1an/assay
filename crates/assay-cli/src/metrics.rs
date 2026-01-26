// ============================================================================
// metrics.rs — Privacy-Safe Local Metrics (PR6)
// ============================================================================

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

/// Metrics store (counters only, no PII)
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MetricsStore {
    pub counters: HashMap<String, u64>,
    #[serde(default)]
    pub version: u32,
}

static METRICS: Lazy<Mutex<MetricsStore>> =
    Lazy::new(|| Mutex::new(load_metrics().unwrap_or_default()));

/// Get metrics file path
fn metrics_path() -> PathBuf {
    // Prefer XDG_DATA_HOME, fallback to ~/.local/share/assay
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join(".local/share")
        });

    base.join("assay").join("metrics.json")
}

/// Load metrics from disk
fn load_metrics() -> Option<MetricsStore> {
    let path = metrics_path();
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Save metrics to disk (atomic write)
fn save_metrics(store: &MetricsStore) {
    let path = metrics_path();

    // Create parent directory
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    // Atomic write: temp file (same dir) + fsync + rename
    let temp_path = path.with_extension("json.tmp");

    if let Ok(content) = serde_json::to_string_pretty(store) {
        // Use File for explicit sync
        if let Ok(mut file) = fs::File::create(&temp_path) {
            use std::io::Write;
            if file.write_all(content.as_bytes()).is_ok() {
                // SOTA: fsync to ensure data hits disk before rename
                let _ = file.sync_all();
                drop(file); // Close before rename (Windows compat)
                let _ = fs::rename(&temp_path, &path);
            }
        }
    }
}

/// Increment a counter by 1
pub fn increment(name: &str) {
    add(name, 1);
}

/// Add value to a counter
pub fn add(name: &str, value: u64) {
    if let Ok(mut store) = METRICS.lock() {
        *store.counters.entry(name.to_string()).or_default() += value;
        save_metrics(&store);
    }
}

/// Get current metrics (for doctor output)
pub fn get_all() -> HashMap<String, u64> {
    METRICS
        .lock()
        .map(|s| s.counters.clone())
        .unwrap_or_default()
}

/// Reset all metrics (for testing)
#[cfg(test)]
pub fn reset() {
    if let Ok(mut store) = METRICS.lock() {
        store.counters.clear();
        save_metrics(&store);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_increment() {
        // Use a temporary path for testing to avoid messing with real metrics
        // Note: In real usage, this changes the global static, so we can't fully isolate path
        // without more complex injection. For now, rely on `reset()`.
        reset();
        increment("test_counter");
        increment("test_counter");
        let metrics = get_all();
        assert_eq!(metrics.get("test_counter"), Some(&2));
    }

    #[test]
    #[serial]
    fn test_add() {
        reset();
        add("test_add", 5);
        add("test_add", 3);
        let metrics = get_all();
        assert_eq!(metrics.get("test_add"), Some(&8));
    }
}
