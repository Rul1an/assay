use super::*;

#[test]
fn test_string_input_deserialize() {
    let yaml = r#"
            id: test1
            input: "simple string"
            expected:
              type: must_contain
              must_contain: ["foo"]
        "#;
    let tc: TestCase = serde_yaml::from_str(yaml).expect("failed to parse");
    assert_eq!(tc.input.prompt, "simple string");
}

#[test]
fn test_legacy_list_expected_single_entry() {
    let yaml = r#"
            id: test1
            input: "test"
            expected:
              - must_contain: "Paris"
        "#;
    let tc: TestCase = serde_yaml::from_str(yaml).expect("failed to parse");
    if let Expected::MustContain { must_contain } = tc.expected {
        assert_eq!(must_contain, vec!["Paris"]);
    } else {
        panic!("Expected MustContain, got {:?}", tc.expected);
    }
}

/// A multi-element `expected:` list used to keep element 0 and drop the rest in
/// silence, so a two-assertion block enforced half of what it claimed.
#[test]
fn test_multi_element_expected_list_is_rejected() {
    let yaml = r#"
            id: test1
            input: "test"
            expected:
              - must_contain: "Paris"
              - must_not_contain: "London"
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("multi-element expected list must not parse");
    let msg = err.to_string();
    assert!(msg.contains("test1"), "message must name the test: {}", msg);
    assert!(
        msg.contains("2 entries"),
        "message must name the entry count: {}",
        msg
    );
}

#[test]
fn test_empty_expected_list_is_rejected() {
    let yaml = r#"
            id: test1
            input: "test"
            expected: []
        "#;
    let err =
        serde_yaml::from_str::<TestCase>(yaml).expect_err("empty expected list must not parse");
    assert!(err.to_string().contains("empty list"), "{}", err);
}

/// The headline regression: a typo in a key used to fall back to
/// `Expected::default()` (an empty `must_contain`), which passes unconditionally.
#[test]
fn test_unparsable_expected_object_is_hard_error() {
    let yaml = r#"
            id: typo_test
            input: "test"
            expected:
              must_contains: ["Paris"]
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("unrecognized expected block must not parse");
    let msg = err.to_string();
    assert!(
        msg.contains("typo_test"),
        "message must name the test: {}",
        msg
    );
    assert!(
        msg.contains("must_contains"),
        "message must name the offending key: {}",
        msg
    );
}

/// Same typo, but inside a list entry: the other silent path to the default.
#[test]
fn test_unrecognized_expected_list_entry_is_hard_error() {
    let yaml = r#"
            id: typo_list
            input: "test"
            expected:
              - must_contains: ["Paris"]
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("unrecognized expected list entry must not parse");
    let msg = err.to_string();
    assert!(
        msg.contains("typo_list") && msg.contains("must_contains"),
        "message must name test and key: {}",
        msg
    );
}

/// A block that opts into the tagged form gets the underlying serde error, not the
/// generic "unrecognized keys" message. Here `pattern` is misspelled, so the field
/// is missing as far as serde is concerned.
#[test]
fn test_tagged_expected_reports_underlying_error() {
    let yaml = r#"
            id: bad_tagged
            input: "test"
            expected:
              type: regex_match
              pattten: "^hi"
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("tagged block with a missing field must not parse");
    let msg = err.to_string();
    assert!(
        msg.contains("bad_tagged") && msg.contains("pattern"),
        "message must name the test and the missing field: {}",
        msg
    );
}

/// Untagged single mappings are read with the same legacy heuristics as list
/// entries. Before the fix, this shape silently became an empty `must_contain`.
#[test]
fn test_untagged_single_object_uses_legacy_heuristics() {
    let yaml = r#"
            id: test1
            input: "test"
            expected:
              must_contain: ["Paris"]
        "#;
    let tc: TestCase = serde_yaml::from_str(yaml).expect("failed to parse");
    match tc.expected {
        Expected::MustContain { must_contain } => assert_eq!(must_contain, vec!["Paris"]),
        other => panic!("Expected MustContain, got {:?}", other),
    }
}

/// (d) A missing `expected:` key stays permissive: `assertions:` may carry the
/// checks. `assay validate` reports the case where neither is present.
#[test]
fn test_missing_expected_key_still_parses() {
    let yaml = r#"
            id: test1
            input: "test"
        "#;
    let tc: TestCase = serde_yaml::from_str(yaml).expect("missing expected must stay permissive");
    match tc.expected {
        Expected::MustContain { must_contain } => assert!(must_contain.is_empty()),
        other => panic!("Expected default MustContain, got {:?}", other),
    }
}

#[test]
fn test_scalar_must_contain_promotion() {
    let yaml = r#"
            id: test1
            input: "test"
            expected:
              - must_contain: "single value"
        "#;
    let tc: TestCase = serde_yaml::from_str(yaml).unwrap();
    if let Expected::MustContain { must_contain } = tc.expected {
        assert_eq!(must_contain, vec!["single value"]);
    } else {
        panic!("Expected MustContain");
    }
}

#[test]
fn test_validate_ref_in_v1() {
    let config = EvalConfig {
        version: 1,
        suite: "test".into(),
        model: "test".into(),
        settings: Settings::default(),
        thresholds: Default::default(),
        tests: vec![TestCase {
            id: "t1".into(),
            input: TestInput {
                prompt: "hi".into(),
                context: None,
            },
            expected: Expected::Reference {
                path: "foo.yaml".into(),
            },
            assertions: None,
            tags: vec![],
            metadata: None,
            on_error: None,
        }],
        otel: Default::default(),
    };
    assert!(config.validate().is_err());
}

#[test]
fn test_thresholding_for_metric() {
    // No thresholding
    let exp = Expected::SemanticSimilarityTo {
        semantic_similarity_to: "ref".into(),
        min_score: 0.8,
        thresholding: None,
    };
    assert!(exp
        .thresholding_for_metric("semantic_similarity_to")
        .is_none());
    // With thresholding
    let exp = Expected::SemanticSimilarityTo {
        semantic_similarity_to: "ref".into(),
        min_score: 0.8,
        thresholding: Some(ThresholdingConfig {
            max_drop: Some(0.05),
        }),
    };
    let t = exp
        .thresholding_for_metric("semantic_similarity_to")
        .unwrap();
    assert_eq!(t.max_drop, Some(0.05));
    // Wrong metric name
    assert!(exp.thresholding_for_metric("faithfulness").is_none());
    // Faithfulness variant
    let exp = Expected::Faithfulness {
        min_score: 0.7,
        rubric_version: None,
        thresholding: Some(ThresholdingConfig {
            max_drop: Some(0.1),
        }),
    };
    let t = exp.thresholding_for_metric("faithfulness").unwrap();
    assert_eq!(t.max_drop, Some(0.1));
}
