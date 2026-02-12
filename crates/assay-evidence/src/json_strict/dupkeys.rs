//! Duplicate key detection for strict JSON object validation.

use std::collections::HashSet;

use super::errors::{StrictJsonError, MAX_KEYS_PER_OBJECT};

/// Tracks keys per object scope for duplicate detection.
pub(crate) struct ObjectKeyTracker {
    /// Stack of (path, keys_at_this_level)
    stack: Vec<(String, HashSet<String>)>,
}

impl ObjectKeyTracker {
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    pub fn enter_object(&mut self, path: String) {
        self.stack.push((path, HashSet::new()));
    }

    pub fn push_key(&mut self, key: String) -> Result<(), StrictJsonError> {
        if let Some((path, keys)) = self.stack.last_mut() {
            if keys.len() >= MAX_KEYS_PER_OBJECT {
                return Err(StrictJsonError::TooManyKeys {
                    count: keys.len() + 1,
                });
            }
            if !keys.insert(key.clone()) {
                return Err(StrictJsonError::DuplicateKey {
                    key,
                    path: if path.is_empty() {
                        "/".to_string()
                    } else {
                        path.clone()
                    },
                });
            }
        }
        Ok(())
    }

    pub fn exit_object(&mut self) {
        self.stack.pop();
    }
}
