//! Compliance pack system for evidence bundle linting.
//!
//! Packs are YAML-defined rule sets that can be composed and applied to evidence bundles.
//! See SPEC-Pack-Engine-v1 for the complete specification.

pub mod checks;
pub mod executor;
pub mod loader;
pub mod schema;

pub use executor::PackExecutor;
pub use loader::{load_pack, load_packs, LoadedPack, PackError, PackSource};
pub use schema::{CheckDefinition, PackDefinition, PackKind, PackRequirements, PackRule, Severity};

/// Built-in packs embedded at compile time.
///
/// Format: (pack_name, pack_yaml_content)
///
/// Note: Pack files are stored in crates/assay-evidence/packs/ to ensure they're
/// included in the published crate. The root /packs/ directory is the source of
/// truth; sync changes manually or via build script.
pub static BUILTIN_PACKS: &[(&str, &str)] = &[
    (
        "eu-ai-act-baseline",
        include_str!("../../../packs/eu-ai-act-baseline.yaml"),
    ),
    (
        "mandate-baseline",
        include_str!("../../../packs/mandate-baseline.yaml"),
    ),
];

/// Look up a built-in pack by name.
pub fn get_builtin_pack(name: &str) -> Option<&'static str> {
    BUILTIN_PACKS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, content)| *content)
}
