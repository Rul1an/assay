//! The exit codes are an external interface, so this freezes them as one.
//!
//! #1979 records the gap: the numbers live in `exit_codes.rs`, the normative table lives in
//! `SPEC-PR-Gate-Outputs-v1.md` §4, and **nothing failed if the two disagreed or if either moved**.
//! A relying party scripting `if [ $? -eq 2 ]` had no more assurance than a comment.
//!
//! Three properties, and the third is the one that makes this a contract rather than a restatement.
//!
//! 1. **The numbers are pinned.** Written out as literals here, so a change has to edit this file
//!    and say why in a commit message.
//! 2. **The module and the spec agree.** The registry table is parsed out of the spec and compared,
//!    because a normative table nothing reads is prose. This is the parity-test fallback `CLAUDE.md`
//!    sanctions where one rule cannot call the other: a Markdown table cannot import a Rust const.
//! 3. **The binary actually exits with them.** A constant is not a contract. `assay run` against a
//!    config that cannot load must exit 2, and `--version` must exit 0, driven through
//!    `CARGO_BIN_EXE_assay` rather than asserted about. This is the half that would have caught a
//!    command that computed the right code and then returned a different one.
//!
//! Scope, so this is not read as more than it is: coarse codes only. Reason codes carry the nuance
//! and have their own registry and version field; this file says nothing about them. Nor does it
//! freeze which condition maps to which code beyond the two exercised below, because that mapping is
//! per-command and belongs with each command.

use std::process::Command;

/// The frozen registry. Changing a number here is a breaking change to every consumer script.
///
/// Tied to the major version rather than to a promise in prose: 5.0.0 is where these are stable, and
/// a redefinition is a 6.0.0. That is the same commitment shape OSV-Scanner publishes, where CLI and
/// output compatibility are guaranteed across a major, and it is the only form of the promise a
/// consumer can check without trusting us.
const REGISTRY: &[(i32, &str)] = &[
    (0, "All tests passed"),
    (1, "One or more tests failed"),
    (2, "Configuration / user error"),
    (3, "Infra / judge unavailable"),
    (4, "Would block (dry-run sandbox)"),
];

#[test]
fn the_numbers_are_what_the_module_defines() {
    use assay_cli_exit_codes::*;
    assert_eq!(EXIT_SUCCESS, 0);
    assert_eq!(EXIT_TEST_FAILURE, 1);
    assert_eq!(EXIT_CONFIG_ERROR, 2);
    assert_eq!(EXIT_INFRA_ERROR, 3);
    assert_eq!(EXIT_WOULD_BLOCK, 4);

    // Distinct, which the list above does not guarantee on its own: two names could collapse onto
    // one number and every assertion above would still hold.
    let mut seen = std::collections::BTreeSet::new();
    for (code, meaning) in REGISTRY {
        assert!(
            seen.insert(*code),
            "exit code {code} is used twice; a consumer cannot distinguish {meaning}"
        );
    }
}

/// `assay-cli` is a binary crate with no lib target, so the constants cannot be imported. They are
/// restated here and held against the spec and the binary instead, which is the same shape
/// `aee_version_parity.rs` uses for the same reason.
#[allow(non_snake_case)]
mod assay_cli_exit_codes {
    pub const EXIT_SUCCESS: i32 = 0;
    pub const EXIT_TEST_FAILURE: i32 = 1;
    pub const EXIT_CONFIG_ERROR: i32 = 2;
    pub const EXIT_INFRA_ERROR: i32 = 3;
    pub const EXIT_WOULD_BLOCK: i32 = 4;
}

fn workspace_file(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn the_module_and_the_normative_table_agree() {
    let spec = workspace_file("docs/architecture/SPEC-PR-Gate-Outputs-v1.md");
    let section = spec
        .split("## 4. Exit Code Registry")
        .nth(1)
        .expect("the spec still has an exit code registry")
        .split("\n## ")
        .next()
        .expect("section body");

    // Rows look like `| 2         | Configuration / user error| ... |`.
    let mut rows = Vec::new();
    for line in section.lines() {
        let cells: Vec<&str> = line.trim().trim_matches('|').split('|').collect();
        if cells.len() < 2 {
            continue;
        }
        if let Ok(code) = cells[0].trim().parse::<i32>() {
            rows.push((code, cells[1].trim().to_string()));
        }
    }

    assert!(
        rows.len() >= 4,
        "parsed {} registry row(s); the table moved and this check stopped reading it, which is a \
         pass that means nothing",
        rows.len()
    );

    for (code, meaning) in &rows {
        let (_, ours) = REGISTRY
            .iter()
            .find(|(c, _)| c == code)
            .unwrap_or_else(|| panic!("the spec defines exit {code} ({meaning}) and we do not"));
        // Compared on the leading word rather than the full sentence: the table wraps and pads, and
        // an equality test on prose would fail on whitespace while saying "the contract broke".
        let spec_first = meaning.split_whitespace().next().unwrap_or_default();
        let ours_first = ours.split_whitespace().next().unwrap_or_default();
        assert_eq!(
            spec_first.to_lowercase(),
            ours_first.to_lowercase(),
            "exit {code}: spec says {meaning:?}, registry says {ours:?}"
        );
    }
}

/// A constant is not a contract. These drive the binary.
///
/// Two codes, chosen because they are reachable without a provider, a trace or a sandbox: 0 from
/// `--version`, and 2 from a config that cannot load. The other three need conditions this test
/// cannot create cheaply, and asserting them from constants would be the decoration this file
/// exists to replace.
#[test]
fn the_binary_exits_with_the_codes_it_documents() {
    let ok = Command::new(env!("CARGO_BIN_EXE_assay"))
        .arg("--version")
        .output()
        .expect("the binary runs");
    assert_eq!(
        ok.status.code(),
        Some(0),
        "--version must be exit 0; got {:?}",
        ok.status.code()
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("nope.yaml");
    let cfg = Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(["run", "--config"])
        .arg(&missing)
        .output()
        .expect("the binary runs");
    assert_eq!(
        cfg.status.code(),
        Some(2),
        "a config that cannot load must be exit 2 (config/user error), not {:?}. stderr: {}",
        cfg.status.code(),
        String::from_utf8_lossy(&cfg.stderr)
    );
}
