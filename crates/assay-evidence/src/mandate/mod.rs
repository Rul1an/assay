//! Mandate Evidence Module (SPEC-Mandate-v1)
//!
//! Cryptographically-signed user authorization evidence for AI agent tool calls.
//!
//! # Overview
//!
//! Mandates are tamper-proof records that link tool decisions to explicit user intent.
//! They provide:
//!
//! - **Proof of authorization** - Cryptographic evidence that a user approved an action
//! - **Scope limitation** - What tools and resources are authorized
//! - **Time bounds** - When the authorization is valid
//! - **Usage constraints** - Single-use, max uses, confirmation requirements
//!
//! # Mandate Kinds
//!
//! | Kind | Purpose | Typical Use |
//! |------|---------|-------------|
//! | `Intent` | Standing authority | Browse products, search |
//! | `Transaction` | Final authorization | Purchase, transfer funds |
//! | `Revocation` | Cancel mandate | User revokes permission |
//!
//! # Example
//!
//! ```rust
//! use assay_evidence::mandate::{
//!     Mandate, MandateKind, Principal, Scope, Validity, Constraints, Context,
//!     AuthMethod, OperationClass, compute_mandate_id,
//! };
//!
//! // Build mandate content
//! let content = Mandate::builder()
//!     .kind(MandateKind::Intent)
//!     .principal(Principal::new("user-123", AuthMethod::Oidc))
//!     .scope(Scope::new(vec!["search_*".to_string()]))
//!     .validity(Validity::now())
//!     .context(Context::new("myorg/app", "auth.myorg.com"))
//!     .build()
//!     .unwrap();
//!
//! // Compute content-addressed ID
//! let mandate_id = compute_mandate_id(&content).unwrap();
//! assert!(mandate_id.starts_with("sha256:"));
//!
//! // Convert to full mandate
//! let mandate = content.into_mandate(mandate_id);
//! ```

pub mod events;
pub mod glob;
pub mod id;
pub mod policy;
pub mod signing;
pub mod types;

// Re-export main types
pub use events::{
    mandate_event, mandate_revoked_event, mandate_used_event, MandateEvent, MandateRevokedPayload,
    MandateUsedPayload, RevocationReason, ToolDecisionWithMandate, EVENT_TYPE_MANDATE,
    EVENT_TYPE_MANDATE_REVOKED, EVENT_TYPE_MANDATE_USED,
};
pub use glob::{GlobPattern, GlobSet};
pub use id::{compute_mandate_id, compute_transaction_ref, verify_mandate_id};
pub use policy::{
    validate_mandate_policy, MandateTrustPolicy, PolicyValidationError, PolicyValidationResult,
    ToolClassification,
};
pub use signing::{
    compute_key_id, compute_key_id_from_verifying_key, is_signed, sign_mandate, verify_mandate,
    VerifyError, VerifyResult,
};
pub use types::{
    AuthMethod, Constraints, Context, Mandate, MandateBuilder, MandateContent, MandateKind,
    MaxValue, OperationClass, Principal, Scope, Signature, Validity, MANDATE_PAYLOAD_TYPE,
    MANDATE_REVOKED_PAYLOAD_TYPE, MANDATE_USED_PAYLOAD_TYPE,
};
