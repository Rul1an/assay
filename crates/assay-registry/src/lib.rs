//! Pack registry client for remote pack distribution.
//!
//! This crate implements the client side of SPEC-Pack-Registry-v1, providing:
//!
//! - HTTP client for registry API with token auth
//! - Digest and signature verification
//! - Local caching with integrity verification
//! - Pack resolution (local → bundled → registry → BYOS)
//! - Lockfile support for reproducible builds
//! - OIDC token exchange for CI environments
//!
//! # Quick Start
//!
//! ```no_run
//! use assay_registry::{RegistryClient, RegistryConfig};
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create client from environment
//! let client = RegistryClient::from_env()?;
//!
//! // Fetch a pack
//! let result = client.fetch_pack("eu-ai-act-baseline", "1.2.0", None).await?;
//! if let Some(pack) = result {
//!     println!("Fetched pack with digest: {}", pack.computed_digest);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Authentication
//!
//! The client supports token-based authentication via:
//!
//! - `ASSAY_REGISTRY_TOKEN` environment variable
//! - Explicit token in `RegistryConfig`
//! - OIDC token exchange (with `oidc` feature)
//!
//! # Configuration
//!
//! | Environment Variable | Description |
//! |---------------------|-------------|
//! | `ASSAY_REGISTRY_URL` | Registry base URL (default: `https://registry.getassay.dev/v1`) |
//! | `ASSAY_REGISTRY_TOKEN` | Authentication token |
//! | `ASSAY_ALLOW_UNSIGNED_PACKS` | Allow unsigned packs (dev only) |
//! | `ASSAY_REGISTRY_TIMEOUT` | Request timeout in seconds (default: 30) |
//! | `ASSAY_REGISTRY_MAX_RETRIES` | Max retries for transient failures (default: 3) |

pub mod auth;
pub mod cache;
pub mod canonicalize;
pub mod client;
mod digest;
pub mod error;
pub mod lockfile;
pub mod reference;
pub mod resolver;
pub mod trust;
pub mod types;
pub mod verify;

// Re-export main types
pub use auth::TokenProvider;
pub use cache::{CacheEntry, CacheMeta, PackCache};
pub use client::RegistryClient;
pub use error::{RegistryError, RegistryResult};
pub use lockfile::{
    generate_lockfile, verify_lockfile, LockMismatch, LockSignature, LockSource, LockedPack,
    Lockfile, VerifyLockResult, LOCKFILE_NAME, LOCKFILE_VERSION,
};
pub use reference::PackRef;
pub use resolver::{PackResolver, ResolveSource, ResolvedPack, ResolverConfig};
pub use trust::{KeyMetadata, TrustStore};
pub use types::{
    DsseEnvelope, DsseSignature, FetchResult, KeysManifest, PackHeaders, PackMeta, RegistryConfig,
    TrustedKey, VersionInfo, VersionsResponse,
};
pub use verify::{compute_digest, verify_digest, verify_pack, VerifyOptions, VerifyResult};

// Canonical digest (SPEC §6.2)
pub use canonicalize::{
    compute_canonical_digest, compute_canonical_digest_result, parse_yaml_strict,
    to_canonical_jcs_bytes, CanonicalizeError, MAX_DEPTH, MAX_KEYS_PER_MAPPING, MAX_SAFE_INTEGER,
    MAX_STRING_LENGTH, MAX_TOTAL_SIZE, MIN_SAFE_INTEGER,
};
