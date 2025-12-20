use crate::storage::Store;

#[derive(Debug, Clone, Copy)]
pub enum QuarantineMode {
    Off,
    Warn,
    Strict,
}

impl QuarantineMode {
    pub fn parse(s: &str) -> Self {
        match s {
            "off" => Self::Off,
            "strict" => Self::Strict,
            _ => Self::Warn,
        }
    }
}

#[derive(Clone)]
pub struct QuarantineService {
    store: Store,
}

impl QuarantineService {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    pub fn is_quarantined(&self, suite: &str, test_id: &str) -> anyhow::Result<Option<String>> {
        self.store.quarantine_get_reason(suite, test_id)
    }

    pub fn add(&self, suite: &str, test_id: &str, reason: &str) -> anyhow::Result<()> {
        self.store.quarantine_add(suite, test_id, reason)
    }

    pub fn remove(&self, suite: &str, test_id: &str) -> anyhow::Result<()> {
        self.store.quarantine_remove(suite, test_id)
    }
}
