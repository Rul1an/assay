//! A crate that depends on assay-registry keeps serde_json's own map order.
//!
//! serde_json keeps an object's members sorted by key unless a crate in the build turns on its
//! `preserve_order` feature, and Cargo then turns it on for the whole build. A crate cannot turn
//! off a feature that a dependency turns on. On 6.x a crate depending on assay-registry got sorted
//! maps, so the registry must not ask for `preserve_order` on that crate's behalf.
//!
//! The test asks `cargo tree` which serde_json features the registry's normal dependency graph
//! turns on, which is what a crate depending on the registry gets from it. Selecting the package
//! keeps this workspace's other members out of the feature resolution, and leaving out dev edges
//! keeps the dev-dependencies out, as a dependent's build does. Nothing is compiled or written.
//!
//! Only this case is checked here. `--frozen` keeps the query offline, which holds because the
//! registry's normal graph for this host is part of what this test's own build fetched. A query
//! that turns on more features, such as `preserve_order`, can need packages that build did not
//! fetch. Readers that keep the authored order are tested in assay-common's `closed_doc` target.

use std::path::Path;
use std::process::Command;

#[test]
fn a_crate_that_depends_on_the_registry_keeps_sorted_maps() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/assay-registry sits two levels below the workspace root");
    let output = Command::new(option_env!("CARGO").unwrap_or("cargo"))
        .current_dir(root)
        .args([
            "tree",
            "--frozen",
            "--package",
            "assay-registry",
            "--edges",
            "normal",
            "--invert",
            "serde_json",
            "--depth",
            "0",
            "--prefix",
            "none",
            "--format",
            "{f}",
        ])
        .output()
        .expect("run cargo tree");
    assert!(
        output.status.success(),
        "cargo tree failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert_eq!(stdout.lines().count(), 1, "one serde_json node: {stdout:?}");
    let features: Vec<&str> = stdout.trim().split(',').collect();
    assert!(
        !features.contains(&"preserve_order"),
        "assay-registry turns on serde_json/preserve_order for its dependents: {features:?}"
    );
}
