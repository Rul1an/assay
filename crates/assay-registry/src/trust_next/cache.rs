use std::collections::{HashMap, HashSet};

use chrono::Utc;

use crate::trust::TrustStoreInner;

pub(in crate::trust) fn needs_refresh(inner: &TrustStoreInner) -> bool {
    match inner.manifest_expires_at {
        Some(expires_at) => Utc::now() >= expires_at,
        None => inner.manifest_fetched_at.is_none(),
    }
}

pub(in crate::trust) fn clear_cached_keys(inner: &mut TrustStoreInner) {
    let pinned_roots: HashSet<_> = inner.pinned_roots.iter().cloned().collect();

    inner.keys.retain(|k, _| pinned_roots.contains(k));
    inner.metadata.retain(|k, _| pinned_roots.contains(k));
    inner.manifest_fetched_at = None;
    inner.manifest_expires_at = None;
}

pub(in crate::trust) fn empty_inner() -> TrustStoreInner {
    TrustStoreInner {
        keys: HashMap::new(),
        metadata: HashMap::new(),
        pinned_roots: Vec::new(),
        manifest_fetched_at: None,
        manifest_expires_at: None,
    }
}
