//! Mandate Evidence Types (SPEC-Mandate-v1)
//!
//! Core data structures for cryptographically-signed user authorization
//! evidence for AI agent tool calls.
//!
//! # Design Principles
//!
//! - **AP2-aligned** - Compatible with Agent Payments Protocol
//! - **Deterministic** - Same content always produces same `mandate_id`
//! - **Offline-verifiable** - Verification requires only trusted keys
//! - **Privacy-preserving** - Opaque principal identifiers, no PII

use chrono::{DateTime, Utc};
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

/// Principal - who granted the mandate.
///
/// # Privacy Requirements
///
/// - `subject` MUST be opaque; MUST NOT contain email, name, or other PII
/// - `display` is for UX only; verifiers MUST NOT use it for trust decisions
/// - `display` SHOULD be absent in exported audit bundles
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    /// Opaque identifier (MUST NOT contain PII)
    pub subject: String,

    /// Authentication method
    pub method: AuthMethod,

    /// Human-readable name (UX only, MUST NOT use for verification)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,

    /// Hash reference to verifiable credential
    /// Format: "sha256:" + lowercase_hex(SHA256(credential_bytes))
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<String>,
}

impl Principal {
    /// Create a new principal with required fields.
    pub fn new(subject: impl Into<String>, method: AuthMethod) -> Self {
        Self {
            subject: subject.into(),
            method,
            display: None,
            credential_ref: None,
        }
    }

    /// Set display name (UX only).
    pub fn with_display(mut self, display: impl Into<String>) -> Self {
        self.display = Some(display.into());
        self
    }

    /// Set credential reference.
    pub fn with_credential_ref(mut self, credential_ref: impl Into<String>) -> Self {
        self.credential_ref = Some(credential_ref.into());
        self
    }
}

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

/// Constraints - usage limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Constraints {
    /// Syntactic sugar for `max_uses: 1`
    #[serde(default, skip_serializing_if = "is_false")]
    pub single_use: bool,

    /// Maximum uses (null = unlimited)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u32>,

    /// Require interactive confirmation
    #[serde(default, skip_serializing_if = "is_false")]
    pub require_confirmation: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
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

/// Context - binding context for replay prevention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    /// Target application/org identifier
    /// Format: {org}/{app} or {org}/{app}/{env}
    pub audience: String,

    /// Signing authority identifier
    pub issuer: String,

    /// Session binding (for interactive flows)
    /// Minimum 128 bits entropy for transaction mandates
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,

    /// W3C Trace Context for correlation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traceparent: Option<String>,
}

impl Context {
    /// Create context with required fields.
    pub fn new(audience: impl Into<String>, issuer: impl Into<String>) -> Self {
        Self {
            audience: audience.into(),
            issuer: issuer.into(),
            nonce: None,
            traceparent: None,
        }
    }

    /// Set nonce.
    pub fn with_nonce(mut self, nonce: impl Into<String>) -> Self {
        self.nonce = Some(nonce.into());
        self
    }

    /// Set traceparent.
    pub fn with_traceparent(mut self, traceparent: impl Into<String>) -> Self {
        self.traceparent = Some(traceparent.into());
        self
    }
}

/// Signature object (DSSE-compatible, v1.0.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    /// Schema version. MUST be 1
    pub version: u32,

    /// Algorithm. MUST be "ed25519" for v1
    pub algorithm: String,

    /// Payload type for type confusion prevention
    pub payload_type: String,

    /// Content-addressed identifier = mandate_id
    pub content_id: String,

    /// SHA256 of signed payload bytes (DSSE standard)
    pub signed_payload_digest: String,

    /// SHA-256 of SPKI public key
    pub key_id: String,

    /// Base64-encoded Ed25519 signature (with padding)
    pub signature: String,

    /// Signing timestamp (metadata only)
    pub signed_at: DateTime<Utc>,
}

/// Mandate payload type for type confusion prevention.
pub const MANDATE_PAYLOAD_TYPE: &str = "application/vnd.assay.mandate+json;v=1";

/// Mandate used event payload type.
pub const MANDATE_USED_PAYLOAD_TYPE: &str = "application/vnd.assay.mandate.used+json;v=1";

/// Mandate revoked event payload type.
pub const MANDATE_REVOKED_PAYLOAD_TYPE: &str = "application/vnd.assay.mandate.revoked+json;v=1";

/// Complete mandate data structure.
///
/// This is the `data` object in the CloudEvents envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mandate {
    /// Content-addressed identifier (see mandate_id computation)
    pub mandate_id: String,

    /// Kind of mandate
    pub mandate_kind: MandateKind,

    /// Who granted the mandate
    pub principal: Principal,

    /// What the mandate authorizes
    pub scope: Scope,

    /// When the mandate is valid
    pub validity: Validity,

    /// Usage limits
    pub constraints: Constraints,

    /// Binding context for replay prevention
    pub context: Context,

    /// Cryptographic signature (optional for unsigned mandates)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<Signature>,
}

impl Mandate {
    /// Create a new mandate builder.
    pub fn builder() -> MandateBuilder {
        MandateBuilder::default()
    }

    /// Check if this mandate allows the given operation class.
    pub fn allows_operation(&self, op: OperationClass) -> bool {
        // Transaction mandates allow all operations up to their operation_class
        // Intent mandates only allow read
        match self.mandate_kind {
            MandateKind::Intent => op == OperationClass::Read,
            MandateKind::Transaction => self.scope.operation_class().allows(op),
        }
    }
}

/// Builder for creating mandates.
#[derive(Default)]
pub struct MandateBuilder {
    mandate_kind: Option<MandateKind>,
    principal: Option<Principal>,
    scope: Option<Scope>,
    validity: Option<Validity>,
    constraints: Option<Constraints>,
    context: Option<Context>,
}

impl MandateBuilder {
    /// Set mandate kind.
    pub fn kind(mut self, kind: MandateKind) -> Self {
        self.mandate_kind = Some(kind);
        self
    }

    /// Set principal.
    pub fn principal(mut self, principal: Principal) -> Self {
        self.principal = Some(principal);
        self
    }

    /// Set scope.
    pub fn scope(mut self, scope: Scope) -> Self {
        self.scope = Some(scope);
        self
    }

    /// Set validity.
    pub fn validity(mut self, validity: Validity) -> Self {
        self.validity = Some(validity);
        self
    }

    /// Set constraints.
    pub fn constraints(mut self, constraints: Constraints) -> Self {
        self.constraints = Some(constraints);
        self
    }

    /// Set context.
    pub fn context(mut self, context: Context) -> Self {
        self.context = Some(context);
        self
    }

    /// Build the mandate (without mandate_id - must be computed separately).
    ///
    /// Returns an error if required fields are missing.
    pub fn build(self) -> Result<MandateContent, &'static str> {
        Ok(MandateContent {
            mandate_kind: self.mandate_kind.ok_or("mandate_kind is required")?,
            principal: self.principal.ok_or("principal is required")?,
            scope: self.scope.ok_or("scope is required")?,
            validity: self.validity.ok_or("validity is required")?,
            constraints: self.constraints.unwrap_or_default(),
            context: self.context.ok_or("context is required")?,
        })
    }
}

/// Mandate content without mandate_id (for hashing).
///
/// This is the hashable content used to compute mandate_id.
/// The mandate_id is computed from this struct WITHOUT the mandate_id field
/// to avoid circularity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MandateContent {
    /// Kind of mandate
    pub mandate_kind: MandateKind,

    /// Who granted the mandate
    pub principal: Principal,

    /// What the mandate authorizes
    pub scope: Scope,

    /// When the mandate is valid
    pub validity: Validity,

    /// Usage limits
    pub constraints: Constraints,

    /// Binding context for replay prevention
    pub context: Context,
}

impl MandateContent {
    /// Convert to full Mandate with computed mandate_id (unsigned).
    ///
    /// The mandate_id is computed from this content using JCS + SHA256.
    pub fn into_mandate(self, mandate_id: String) -> Mandate {
        Mandate {
            mandate_id,
            mandate_kind: self.mandate_kind,
            principal: self.principal,
            scope: self.scope,
            validity: self.validity,
            constraints: self.constraints,
            context: self.context,
            signature: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_class_ordering() {
        assert!(OperationClass::Read < OperationClass::Write);
        assert!(OperationClass::Write < OperationClass::Commit);
        assert!(OperationClass::Read < OperationClass::Commit);
    }

    #[test]
    fn test_operation_class_allows() {
        assert!(OperationClass::Commit.allows(OperationClass::Read));
        assert!(OperationClass::Commit.allows(OperationClass::Write));
        assert!(OperationClass::Commit.allows(OperationClass::Commit));

        assert!(OperationClass::Write.allows(OperationClass::Read));
        assert!(OperationClass::Write.allows(OperationClass::Write));
        assert!(!OperationClass::Write.allows(OperationClass::Commit));

        assert!(OperationClass::Read.allows(OperationClass::Read));
        assert!(!OperationClass::Read.allows(OperationClass::Write));
        assert!(!OperationClass::Read.allows(OperationClass::Commit));
    }

    #[test]
    fn test_validity_check() {
        use chrono::TimeZone;

        let now = Utc.with_ymd_and_hms(2026, 1, 28, 12, 0, 0).unwrap();
        let before = Utc.with_ymd_and_hms(2026, 1, 28, 11, 0, 0).unwrap();
        let after = Utc.with_ymd_and_hms(2026, 1, 28, 13, 0, 0).unwrap();

        // No constraints
        let validity = Validity::at(before);
        assert!(validity.is_valid_at(now));

        // Valid window
        let validity = Validity::at(before)
            .with_not_before(before)
            .with_expires_at(after);
        assert!(validity.is_valid_at(now));

        // Not yet valid
        let validity = Validity::at(now).with_not_before(after);
        assert!(!validity.is_valid_at(now));

        // Expired
        let validity = Validity::at(before).with_expires_at(before);
        assert!(!validity.is_valid_at(now));
    }

    #[test]
    fn test_constraints_single_use() {
        let constraints = Constraints::single_use();
        assert!(constraints.single_use);
        assert_eq!(constraints.effective_max_uses(), Some(1));

        assert!(constraints.is_use_allowed(0));
        assert!(!constraints.is_use_allowed(1));
    }

    #[test]
    fn test_constraints_max_uses() {
        let constraints = Constraints::unlimited().with_max_uses(3);
        assert_eq!(constraints.effective_max_uses(), Some(3));

        assert!(constraints.is_use_allowed(0));
        assert!(constraints.is_use_allowed(1));
        assert!(constraints.is_use_allowed(2));
        assert!(!constraints.is_use_allowed(3));
    }

    #[test]
    fn test_mandate_kind_serialization() {
        assert_eq!(
            serde_json::to_string(&MandateKind::Intent).unwrap(),
            "\"intent\""
        );
        assert_eq!(
            serde_json::to_string(&MandateKind::Transaction).unwrap(),
            "\"transaction\""
        );
    }

    #[test]
    fn test_operation_class_serialization() {
        assert_eq!(
            serde_json::to_string(&OperationClass::Read).unwrap(),
            "\"read\""
        );
        assert_eq!(
            serde_json::to_string(&OperationClass::Write).unwrap(),
            "\"write\""
        );
        assert_eq!(
            serde_json::to_string(&OperationClass::Commit).unwrap(),
            "\"commit\""
        );
    }

    #[test]
    fn test_mandate_builder() {
        let content = Mandate::builder()
            .kind(MandateKind::Intent)
            .principal(Principal::new("user-123", AuthMethod::Oidc))
            .scope(Scope::new(vec!["search_*".to_string()]))
            .validity(Validity::now())
            .context(Context::new("myorg/app", "auth.myorg.com"))
            .build()
            .unwrap();

        assert_eq!(content.mandate_kind, MandateKind::Intent);
        assert_eq!(content.principal.subject, "user-123");
    }
}
