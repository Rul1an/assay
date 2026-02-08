pub mod judge_cache;
pub mod rows;
pub mod schema;
pub mod store;

pub use store::Store;

pub(crate) fn now_rfc3339ish() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format!("unix:{}", secs)
}
