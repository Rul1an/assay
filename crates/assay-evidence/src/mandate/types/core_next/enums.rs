use serde::{Deserialize, Serialize};

/// Mandate kind - determines what operations are authorized.
///
/// | Kind | Purpose | Allowed Operation Classes |
/// |------|---------|---------------------------|
/// | `Intent` | Standing authority for discovery | `read` |
/// | `Transaction` | Final authorization for commits | `read`, `write`, `commit` |
///
/// Note (v1.0.2): `Revocation` was removed as a kind. Revocation is handled
/// via `assay.mandate.revoked.v1` events instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MandateKind {
    /// Standing authority for discovery/browsing
    #[default]
    Intent,
    /// Final authorization for commits/purchases
    Transaction,
}

/// Operation class with normative ordering: read(0) < write(1) < commit(2)
///
/// When a mandate specifies `operation_class`, it authorizes that class
/// **and all lower classes**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OperationClass {
    /// Discovery, browsing, read-only (ordinal 0)
    #[default]
    Read = 0,
    /// Modifications, non-financial (ordinal 1)
    Write = 1,
    /// Financial transactions, irreversible (ordinal 2)
    Commit = 2,
}

impl OperationClass {
    /// Check if this class allows the given operation.
    ///
    /// A mandate authorizes its class and all lower classes.
    pub fn allows(&self, other: OperationClass) -> bool {
        other <= *self
    }
}

/// Authentication method used to verify the principal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    /// OpenID Connect (OAuth 2.0)
    #[default]
    Oidc,
    /// Decentralized Identifier
    Did,
    /// SPIFFE/SPIRE workload identity
    Spiffe,
    /// Local system user
    LocalUser,
    /// Service-to-service
    ServiceAccount,
    /// API key authentication
    ApiKey,
}
