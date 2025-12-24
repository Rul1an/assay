use crate::model::LlmResponse;
use crate::storage::Store;

#[derive(Clone)]
pub struct VcrCache {
    store: Store,
}

impl VcrCache {
    pub fn new(store: Store) -> Self {
        Self { store }
    }
    pub fn get(&self, key: &str) -> anyhow::Result<Option<LlmResponse>> {
        self.store.cache_get(key)
    }
    pub fn put(&self, key: &str, resp: &LlmResponse) -> anyhow::Result<()> {
        self.store.cache_put(key, resp)
    }
}
