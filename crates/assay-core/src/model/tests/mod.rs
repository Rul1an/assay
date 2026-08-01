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

/// A tagged block whose VALUE shape is legacy must still parse. The strict parse
/// fails (a scalar is not a list), but the legacy heuristics understand it, and
/// rejecting it would turn working suites into config errors.
#[test]
fn test_tagged_block_with_legacy_scalar_value_still_parses() {
    let yaml = r#"
            id: tagged_scalar
            input: "test"
            expected:
              - type: must_contain
                must_contain: "hello"
        "#;
    let tc: TestCase = serde_yaml::from_str(yaml).expect("tagged block with scalar must parse");
    match tc.expected {
        Expected::MustContain { must_contain } => assert_eq!(must_contain, vec!["hello"]),
        other => panic!("Expected MustContain, got {:?}", other),
    }
}

/// `type: sequence` is not an `Expected` variant (the variant is `sequence_valid`),
/// but it is the shape documented in the migration guide, and the legacy `sequence`
/// key resolves it. It must keep working.
#[test]
fn test_legacy_type_sequence_still_parses() {
    let yaml = r#"
            id: legacy_seq
            input: "test"
            expected:
              - type: sequence
                sequence: ["Search", "Create"]
        "#;
    let tc: TestCase = serde_yaml::from_str(yaml).expect("legacy type: sequence must parse");
    match tc.expected {
        Expected::SequenceValid { sequence, .. } => {
            assert_eq!(
                sequence,
                Some(vec!["Search".to_string(), "Create".to_string()])
            );
        }
        other => panic!("Expected SequenceValid, got {:?}", other),
    }
}

/// An unparsable `sequence` value used to become `sequence: None` via `.ok()`, and
/// `sequence_valid` passes unconditionally with neither sequence nor rules — an
/// always-green test that no validate rule caught.
#[test]
fn test_unparsable_sequence_value_is_hard_error() {
    let yaml = r#"
            id: bad_seq
            input: "test"
            expected:
              sequence: 42
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml).expect_err("bad sequence must not parse");
    assert!(
        err.to_string().contains("`sequence` must be a list"),
        "{}",
        err
    );
}

/// An unparsable `must_contain` value used to collapse to an empty vec via
/// `unwrap_or_default()`, which passes for any response.
#[test]
fn test_unparsable_must_contain_value_is_hard_error() {
    let yaml = r#"
            id: bad_mc
            input: "test"
            expected:
              must_contain: {oops: 1}
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml).expect_err("bad must_contain must not parse");
    assert!(
        err.to_string().contains("`must_contain` must be a string"),
        "{}",
        err
    );
}

/// An assertion written out as empty passes for any response. Rejecting it at parse
/// time means every command that loads a config catches it, including `run` and `ci`.
#[test]
fn test_explicit_empty_must_contain_is_hard_error() {
    let yaml = r#"
            id: vacuous
            input: "test"
            expected:
              type: must_contain
              must_contain: []
        "#;
    let err =
        serde_yaml::from_str::<TestCase>(yaml).expect_err("empty must_contain must not parse");
    assert!(
        err.to_string().contains("would pass for any response"),
        "{}",
        err
    );
}

#[test]
fn test_explicit_empty_must_not_contain_is_hard_error() {
    let yaml = r#"
            id: vacuous
            input: "test"
            expected:
              type: must_not_contain
              must_not_contain: []
        "#;
    let err =
        serde_yaml::from_str::<TestCase>(yaml).expect_err("empty must_not_contain must not parse");
    assert!(
        err.to_string().contains("would pass for any response"),
        "{}",
        err
    );
}

/// A block that opts into the tagged form and matches NO legacy key gets the
/// underlying serde error, not the generic "unrecognized keys" message.
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

/// Writers must not emit a config the parser rejects.
///
/// A test that omits `expected:` holds the vacuous default. Serializing it verbatim
/// would write `must_contain: []`, which is now a hard parse error — so `assay migrate`
/// would produce files that no longer load. `skip_serializing_if` prevents that; this
/// test pins the round-trip.
#[test]
fn test_omitted_expected_round_trips_through_serialization() {
    let yaml = r#"
            id: assertions_only
            input: "test"
            assertions:
              - type: trace_must_call_tool
                tool: Search
        "#;
    let tc: TestCase = serde_yaml::from_str(yaml).expect("failed to parse");

    let written = serde_yaml::to_string(&tc).expect("serialize");
    assert!(
        !written.contains("must_contain"),
        "vacuous default must not be materialised into config: {}",
        written
    );

    let reparsed: TestCase = serde_yaml::from_str(&written).expect("writer output must load again");
    assert_eq!(reparsed.id, "assertions_only");
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
