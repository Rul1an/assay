//! `ASSAY_ALLOW_UNSIGNED_PACKS` was documented as allowing unsigned packs. `RegistryConfig::from_env`
//! read it into `RegistryConfig::allow_unsigned`, and nothing read that field: the resolver decides
//! with `ResolverConfig::allow_unsigned`, and verification with `VerifyOptions::allow_unsigned`. So
//! the variable switched nothing, and the crate's configuration table said it did.
//!
//! This file is its own test process: it sets an environment variable, which tests in one process
//! would race on.

#[test]
fn the_unsigned_packs_variable_is_not_read_and_not_documented() {
    std::env::set_var("ASSAY_ALLOW_UNSIGNED_PACKS", "1");
    #[allow(deprecated)]
    let read = assay_registry::RegistryConfig::from_env().allow_unsigned;
    std::env::remove_var("ASSAY_ALLOW_UNSIGNED_PACKS");
    assert!(
        !read,
        "from_env must not turn the variable into a field nothing reads"
    );

    let crate_docs = include_str!("../src/lib.rs");
    assert!(
        !crate_docs.contains("ASSAY_ALLOW_UNSIGNED_PACKS"),
        "the configuration table must not list a variable that switches nothing"
    );
    let config_docs = include_str!("../src/types.rs");
    assert!(
        !config_docs.contains("ASSAY_ALLOW_UNSIGNED_PACKS"),
        "RegistryConfig::from_env must not document the variable"
    );
}
