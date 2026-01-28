//! Error types for bundle storage operations.

use thiserror::Error;

/// Result type for store operations.
pub type StoreResult<T> = Result<T, StoreError>;

/// Errors that can occur during bundle storage operations.
#[derive(Debug, Error)]
pub enum StoreError {
    /// Bundle already exists (conditional write failed).
    /// This is not necessarily an error—it means the bundle was already uploaded.
    #[error("bundle already exists: {bundle_id}")]
    AlreadyExists { bundle_id: String },

    /// Bundle not found.
    #[error("bundle not found: {bundle_id}")]
    NotFound { bundle_id: String },

    /// Access denied to the storage backend.
    #[error("access denied: {message}")]
    AccessDenied { message: String },

    /// Invalid store specification (URL parsing failed).
    #[error("invalid store spec '{spec}': {reason}")]
    InvalidSpec { spec: String, reason: String },

    /// Storage backend is not configured.
    #[error("store not configured: {message}")]
    NotConfigured { message: String },

    /// Network or I/O error.
    #[error("I/O error: {message}")]
    Io { message: String },

    /// The storage backend doesn't support a required operation.
    #[error("operation not supported: {operation}")]
    Unsupported { operation: String },

    /// Generic error from the underlying object store.
    #[error("object store error: {0}")]
    ObjectStore(object_store::Error),

    /// Other errors.
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl StoreError {
    /// Returns true if this error indicates the bundle already exists.
    /// Useful for idempotent upload handling.
    pub fn is_already_exists(&self) -> bool {
        matches!(self, Self::AlreadyExists { .. })
    }

    /// Returns true if this error indicates the bundle was not found.
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound { .. })
    }

    /// Returns true if this is an access/permission error.
    pub fn is_access_denied(&self) -> bool {
        matches!(self, Self::AccessDenied { .. })
    }

    /// Suggested exit code for CLI.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::NotFound { .. } => 2,
            Self::AlreadyExists { .. } => 0, // Idempotent success
            Self::AccessDenied { .. } => 3,
            Self::NotConfigured { .. } => 4,
            _ => 1,
        }
    }

    /// Create from object_store error with context about the bundle.
    pub fn from_object_store(err: object_store::Error, bundle_id: &str) -> Self {
        match &err {
            object_store::Error::NotFound { .. } => StoreError::NotFound {
                bundle_id: bundle_id.to_string(),
            },
            object_store::Error::AlreadyExists { .. } => StoreError::AlreadyExists {
                bundle_id: bundle_id.to_string(),
            },
            object_store::Error::Precondition { .. } => StoreError::AlreadyExists {
                bundle_id: bundle_id.to_string(),
            },
            _ => StoreError::ObjectStore(err),
        }
    }
}

impl From<object_store::Error> for StoreError {
    fn from(err: object_store::Error) -> Self {
        StoreError::from_object_store(err, "unknown")
    }
}
