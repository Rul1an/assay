//! Compliance pack system for evidence bundle linting.
//!
//! Packs are YAML-defined rule sets that can be composed and applied to evidence bundles.
//! See SPEC-Pack-Engine-v1 for the complete specification.

pub mod checks;
pub mod executor;
pub mod loader;
pub mod schema;

pub use executor::PackExecutor;
pub use loader::{load_pack, load_packs, LoadedPack, PackSource};
pub use schema::{CheckDefinition, PackDefinition, PackKind, PackRequirements, PackRule, Severity};

/// Built-in packs embedded at compile time.
///
/// Format: (pack_name, pack_yaml_content)
pub static BUILTIN_PACKS: &[(&str, &str)] = &[
    (
        "eu-ai-act-baseline",
        include_str!("../../../../../packs/eu-ai-act-baseline.yaml"),
    ),
    // Future: ("soc2-baseline", include_str!("../../../../../packs/soc2-baseline.yaml")),
];

/// Look up a built-in pack by name.
pub fn get_builtin_pack(name: &str) -> Option<&'static str> {
    BUILTIN_PACKS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, content)| *content)
}
