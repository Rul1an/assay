use serde::{Deserialize, Serialize};

/// Constraints - usage limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Constraints {
    /// Syntactic sugar for `max_uses: 1`
    #[serde(
        default,
        skip_serializing_if = "crate::mandate::types::serde::is_false"
    )]
    pub single_use: bool,

    /// Maximum uses (null = unlimited)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u32>,

    /// Require interactive confirmation
    #[serde(
        default,
        skip_serializing_if = "crate::mandate::types::serde::is_false"
    )]
    pub require_confirmation: bool,
}

impl Constraints {
    /// Create unlimited constraints.
    pub fn unlimited() -> Self {
        Self::default()
    }

    /// Create single-use constraint.
    pub fn single_use() -> Self {
        Self {
            single_use: true,
            max_uses: Some(1),
            require_confirmation: false,
        }
    }

    /// Set max uses.
    pub fn with_max_uses(mut self, max_uses: u32) -> Self {
        self.max_uses = Some(max_uses);
        if max_uses == 1 {
            self.single_use = true;
        }
        self
    }

    /// Set require confirmation.
    pub fn with_require_confirmation(mut self) -> Self {
        self.require_confirmation = true;
        self
    }

    /// Get effective max uses (None = unlimited).
    pub fn effective_max_uses(&self) -> Option<u32> {
        if self.single_use {
            Some(1)
        } else {
            self.max_uses
        }
    }

    /// Check if use count is within limits.
    pub fn is_use_allowed(&self, current_use_count: u32) -> bool {
        match self.effective_max_uses() {
            Some(max) => current_use_count < max,
            None => true,
        }
    }
}
