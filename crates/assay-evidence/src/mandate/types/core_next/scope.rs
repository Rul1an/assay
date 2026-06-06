use serde::{Deserialize, Serialize};

use super::enums::OperationClass;

/// Maximum transaction value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaxValue {
    /// Decimal amount as string (MUST NOT use float)
    pub amount: String,

    /// ISO 4217 currency code
    pub currency: String,
}

impl MaxValue {
    pub fn new(amount: impl Into<String>, currency: impl Into<String>) -> Self {
        Self {
            amount: amount.into(),
            currency: currency.into(),
        }
    }
}

/// Scope - what the mandate authorizes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    /// Tool name patterns (glob syntax)
    pub tools: Vec<String>,

    /// Resource path patterns (glob syntax)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<Vec<String>>,

    /// Highest operation class allowed (default: read)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_class: Option<OperationClass>,

    /// Maximum transaction value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_value: Option<MaxValue>,

    /// Hash of cart/order intent object (for commit mandates)
    /// Prevents mandate reuse for different transactions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_ref: Option<String>,
}

impl Scope {
    /// Create a new scope with required tools.
    pub fn new(tools: Vec<String>) -> Self {
        Self {
            tools,
            resources: None,
            operation_class: None,
            max_value: None,
            transaction_ref: None,
        }
    }

    /// Get operation class (defaults to Read if not specified).
    pub fn operation_class(&self) -> OperationClass {
        self.operation_class.unwrap_or_default()
    }

    /// Set operation class.
    pub fn with_operation_class(mut self, class: OperationClass) -> Self {
        self.operation_class = Some(class);
        self
    }

    /// Set resources.
    pub fn with_resources(mut self, resources: Vec<String>) -> Self {
        self.resources = Some(resources);
        self
    }

    /// Set max value.
    pub fn with_max_value(mut self, max_value: MaxValue) -> Self {
        self.max_value = Some(max_value);
        self
    }

    /// Set transaction ref (for commit mandates).
    pub fn with_transaction_ref(mut self, transaction_ref: impl Into<String>) -> Self {
        self.transaction_ref = Some(transaction_ref.into());
        self
    }
}
