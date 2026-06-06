use serde::{Deserialize, Serialize};

use super::{
    Constraints, Context, MandateKind, OperationClass, Principal, Scope, Signature, Validity,
};

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
