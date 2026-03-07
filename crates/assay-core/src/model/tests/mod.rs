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
fn test_legacy_list_expected() {
    let yaml = r#"
            id: test1
            input: "test"
            expected:
              - must_contain: "Paris"
              - must_not_contain: "London"
        "#;
    let tc: TestCase = serde_yaml::from_str(yaml).expect("failed to parse");
    if let Expected::MustContain { must_contain } = tc.expected {
        assert_eq!(must_contain, vec!["Paris"]);
    } else {
        panic!("Expected MustContain, got {:?}", tc.expected);
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
