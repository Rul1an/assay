//! What the public YAML patch API (`assay_core::fix::apply_ops_to_text` and `apply_ops_to_file`)
//! promises about the document it reads.
//!
//! The target is read without a document grammar, through the shared syntax layer. A patch applies
//! to YAML the policy loader refuses and to any root. The tree it patches is the tree the shared
//! reader parses. A text that is not YAML fails with the YAML crate's own error and location, and
//! the file keeps its bytes. JSON targets and the CLI routes that plan patches are not covered here.

use assay_common::doc::{Input, Refusal};
use assay_core::agentic::JsonPatchOp;
use assay_core::fix::{apply_ops_in_place, apply_ops_to_file, apply_ops_to_text};
use assay_core::mcp::policy::McpPolicy;
use serde_json::{json, Value};

#[test]
fn a_patch_applies_to_yaml_the_policy_loader_refuses() {
    for (source, pointer) in [
        (
            "version: '2.0'\ntools: {allow: [read], typo: true}",
            "/tools/typo",
        ),
        (
            "suite: trial\nmetadata: {obsolete: alpha, keep: beta}",
            "/metadata/obsolete",
        ),
        ("- {obsolete: true, keep: 7}", "/0/obsolete"),
    ] {
        assert!(
            McpPolicy::from_slice_with_source(source.as_bytes()).is_err(),
            "{source}"
        );
        let ops = [JsonPatchOp::Remove {
            path: pointer.into(),
        }];
        let patched = apply_ops_to_text(source, &ops, false).expect(source);
        let after: Value = serde_yaml::from_str(&patched).unwrap();
        let mut expected: Value = serde_yaml::from_str(source).unwrap();
        apply_ops_in_place(&mut expected, &ops).unwrap();
        assert!(after.pointer(pointer).is_none(), "{source}");
        assert_eq!(after, expected, "{source}");
    }
    // The repaired policy loads.
    let repaired = apply_ops_to_text(
        "version: '2.0'\ntools: {allow: [read], typo: true}",
        &[JsonPatchOp::Remove {
            path: "/tools/typo".into(),
        }],
        false,
    )
    .unwrap();
    McpPolicy::from_slice_with_source(repaired.as_bytes()).expect("the repaired policy loads");
}

#[test]
fn a_text_that_is_not_yaml_fails_with_the_parsers_own_error_and_the_file_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("broken.yaml");
    for source in ["tools: [", "tools:\n  allow: []\n  allow: [read]"] {
        let native = serde_yaml::from_str::<serde_yaml::Value>(source).unwrap_err();
        let Err(Refusal::Syntax { message, .. }) = Input::parse(source) else {
            panic!("the shared reader must refuse {source:?} as syntax");
        };
        assert_eq!(
            message,
            native.to_string(),
            "the shared reader's text is the parser's"
        );
        std::fs::write(&file, source).unwrap();
        for error in [
            apply_ops_to_text(source, &[], false).unwrap_err(),
            apply_ops_to_file(&file, &[]).unwrap_err(),
        ] {
            let cause = error
                .downcast_ref::<serde_yaml::Error>()
                .unwrap_or_else(|| panic!("the YAML crate's error must survive: {error:#}"));
            assert_eq!(cause.to_string(), native.to_string());
            let at = |e: &serde_yaml::Error| e.location().map(|l| (l.line(), l.column()));
            assert_eq!(at(cause), at(&native), "{source:?}");
        }
        assert_eq!(
            std::fs::read(&file).unwrap(),
            source.as_bytes(),
            "{source:?}"
        );
    }
}

#[test]
fn the_patched_tree_is_the_shared_readers_tree() {
    for source in [
        "root: &r [yes, true, 0x10]\nsecond: *r",
        "a: !tag example",
        "[~, 'null', 1.0, 18446744073709551615]",
        "null",
    ] {
        let tree = serde_yaml::from_str::<serde_yaml::Value>(source).unwrap();
        assert_eq!(Input::parse(source).unwrap().source(), &tree, "{source}");
        let unchanged = apply_ops_to_text(source, &[], false).expect(source);
        let written: Value = serde_yaml::from_str(&unchanged).unwrap();
        assert_eq!(written, serde_json::to_value(&tree).unwrap(), "{source}");
    }
    let replaced = apply_ops_to_text(
        "[one, two]",
        &[JsonPatchOp::Replace {
            path: "/1".into(),
            value: json!(3),
        }],
        false,
    )
    .unwrap();
    assert_eq!(
        serde_yaml::from_str::<Value>(&replaced).unwrap(),
        json!(["one", 3])
    );
}
