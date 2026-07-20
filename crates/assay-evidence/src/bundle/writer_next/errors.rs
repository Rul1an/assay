use serde::Serialize;

/// Verification error classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// Integrity violation (hash mismatch, corrupted gzip/tar).
    Integrity,
    /// Contract violation (missing fields, wrong source format, disallowed files).
    Contract,
    /// Security violation (path traversal, malicious payloads).
    Security,
    /// Resource limit exceeded (DoS prevention).
    Limits,
}

impl std::fmt::Display for ErrorClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Stable error codes for verification failures.
///
/// The wire form of a code is its variant name, so renaming one is a wire
/// change for every consumer, not a refactor.
///
/// Codes marked deprecated below are never emitted by the verifier. They are
/// kept until the next major release rather than removed, because removing a
/// variant from a published enum is a breaking change; see the crate CHANGELOG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ErrorCode {
    // Integrity
    IntegrityGzip,
    IntegrityTar,
    IntegrityManifestHash,
    IntegrityEventHash,
    IntegrityFileSizeMismatch,
    IntegrityRunRootMismatch,
    /// Never emitted: a decompression bomb trips `LimitDecodeBytes` first.
    #[deprecated(
        since = "3.35.0",
        note = "never emitted; oversized decompression reports LimitDecodeBytes. Scheduled for removal in 4.0.0"
    )]
    IntegrityZipBomb,
    IntegrityIo,
    // Contract
    /// Never emitted: a bundle whose first entry is not the manifest reports
    /// `ContractFileOrder`, and one with no entries at all reports
    /// `ContractMissingFile`.
    #[deprecated(
        since = "3.35.0",
        note = "never emitted; use ContractFileOrder or ContractMissingFile. Scheduled for removal in 4.0.0"
    )]
    ContractMissingManifest,
    ContractSchemaVersion,
    ContractFileOrder,
    ContractMissingFile,
    ContractDuplicateFile,
    ContractUnexpectedFile,
    ContractRunIdMismatch,
    ContractSequenceGap,
    ContractSequenceStart,
    /// Never emitted: the verifier performs no timestamp monotonicity check.
    /// The code predates the check it was named for.
    #[deprecated(
        since = "3.35.0",
        note = "never emitted; no timestamp monotonicity check exists. Scheduled for removal in 4.0.0"
    )]
    ContractTimestampRegression,
    ContractInvalidJson,
    /// Never emitted: malformed events report `ContractInvalidJson`.
    #[deprecated(
        since = "3.35.0",
        note = "never emitted; malformed events report ContractInvalidJson. Scheduled for removal in 4.0.0"
    )]
    ContractInvalidEvent,
    // Limits
    LimitPathLength,
    LimitFileSize,
    LimitTotalEvents,
    LimitLineBytes,
    LimitJsonDepth,
    LimitBundleBytes,
    LimitDecodeBytes,
    // Security
    SecurityPathTraversal,
    SecurityAbsolutePath,
}

/// A resource limit tripped inside a reader.
///
/// Carried as the source of an [`std::io::Error`] so the verifier classifies it
/// by type. The previous design formatted the code name into the error message
/// and recovered it with a substring match, which turned any rename into a
/// silent runtime misclassification rather than a compile error.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LimitExceeded {
    pub(crate) code: ErrorCode,
    pub(crate) limit: u64,
}

impl std::fmt::Display for LimitExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: exceeded limit of {} bytes", self.code, self.limit)
    }
}

impl std::error::Error for LimitExceeded {}

impl LimitExceeded {
    /// Wrap this marker in an `io::Error` so it survives the `Read` boundary.
    pub(crate) fn into_io(self) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::InvalidData, self)
    }

    /// Recover the marker from an `io::Error`, if it carries one.
    pub(crate) fn from_io(err: &std::io::Error) -> Option<Self> {
        err.get_ref()
            .and_then(|inner| inner.downcast_ref::<Self>())
            .copied()
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Typed verification error with stable code.
#[derive(Debug, thiserror::Error)]
#[error("{class}: {message} ({code})")]
pub struct VerifyError {
    pub class: ErrorClass,
    pub code: ErrorCode,
    pub message: String,
    #[source]
    pub source: Option<anyhow::Error>,
}

impl VerifyError {
    pub fn new(class: ErrorClass, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            class,
            code,
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(mut self, source: impl Into<anyhow::Error>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.message = format!("{}: {}", context.into(), self.message);
        self
    }

    pub fn class(&self) -> ErrorClass {
        self.class
    }
}

// Helper for IO errors (defaults to Integrity/IntegrityIo).
//
// A limit tripped inside a reader arrives here as an io::Error carrying a
// `LimitExceeded` marker. Classifying it at this single conversion point is
// what lets every call site use `VerifyError::from` and still get the specific
// limit code, instead of each site re-deriving it from the message text.
impl From<std::io::Error> for VerifyError {
    fn from(err: std::io::Error) -> Self {
        if let Some(limit) = LimitExceeded::from_io(&err) {
            return Self {
                class: ErrorClass::Limits,
                code: limit.code,
                message: err.to_string(),
                source: Some(err.into()),
            };
        }
        Self {
            class: ErrorClass::Integrity,
            code: ErrorCode::IntegrityIo,
            message: err.to_string(),
            source: Some(err.into()),
        }
    }
}

// Helper for JSON errors (defaults to Contract/ContractSchemaVersion - simplified)
impl From<serde_json::Error> for VerifyError {
    fn from(err: serde_json::Error) -> Self {
        Self {
            class: ErrorClass::Contract,
            code: ErrorCode::ContractSchemaVersion,
            message: err.to_string(),
            source: Some(err.into()),
        }
    }
}
