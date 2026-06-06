//! Pack resolution.
//!
//! Resolves pack references to content with the following priority:
//! 1. Local file (if path exists)
//! 2. Bundled pack (compiled into binary)
//! 3. Cache (if valid and not expired)
//! 4. Registry (remote fetch)
//! 5. BYOS (Bring Your Own Storage)

#[path = "resolver_next/mod.rs"]
mod resolver_next;

pub use resolver_next::{PackResolver, ResolveSource, ResolvedPack, ResolverConfig};
