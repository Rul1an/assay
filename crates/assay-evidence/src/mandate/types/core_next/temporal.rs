use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Validity - when the mandate is valid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Validity {
    /// When mandate was created (ISO 8601 UTC)
    pub issued_at: DateTime<Utc>,

    /// Mandate valid after this time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_before: Option<DateTime<Utc>>,

    /// Mandate expires at this time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

impl Validity {
    /// Create validity with issued_at set to now.
    pub fn now() -> Self {
        Self {
            issued_at: Utc::now(),
            not_before: None,
            expires_at: None,
        }
    }

    /// Create validity with explicit issued_at.
    pub fn at(issued_at: DateTime<Utc>) -> Self {
        Self {
            issued_at,
            not_before: None,
            expires_at: None,
        }
    }

    /// Set not_before.
    pub fn with_not_before(mut self, not_before: DateTime<Utc>) -> Self {
        self.not_before = Some(not_before);
        self
    }

    /// Set expires_at.
    pub fn with_expires_at(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Check if the mandate is valid at the given time.
    ///
    /// - `not_before`: mandate valid if `now >= not_before`
    /// - `expires_at`: mandate valid if `now < expires_at`
    pub fn is_valid_at(&self, now: DateTime<Utc>) -> bool {
        if let Some(nb) = self.not_before {
            if now < nb {
                return false;
            }
        }
        if let Some(exp) = self.expires_at {
            if now >= exp {
                return false;
            }
        }
        true
    }

    /// Check validity with clock skew tolerance.
    pub fn is_valid_at_with_skew(&self, now: DateTime<Utc>, skew_seconds: i64) -> bool {
        let skew = chrono::Duration::seconds(skew_seconds);

        if let Some(nb) = self.not_before {
            if now + skew < nb {
                return false;
            }
        }
        if let Some(exp) = self.expires_at {
            if now - skew >= exp {
                return false;
            }
        }
        true
    }
}
