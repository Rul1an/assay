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

#[test]
fn test_explicit_null_expected_is_rejected_instead_of_treated_as_omitted() {
    let yaml = r#"
            id: explicit_null
            input: "test"
            expected: null
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("an explicit null expected block must not become the omitted sentinel");
    assert!(err.to_string().contains("expected"), "{err}");
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

#[test]
fn test_tagged_args_valid_without_policy_or_schema_is_hard_error() {
    let yaml = r#"
            id: vacuous_args
            input: "test"
            expected:
              type: args_valid
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("args_valid without policy or schema must not parse");
    assert!(err.to_string().contains("asserts nothing"), "{}", err);
}

#[test]
fn test_tagged_sequence_valid_without_constraint_is_hard_error() {
    let yaml = r#"
            id: vacuous_sequence
            input: "test"
            expected:
              type: sequence_valid
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("sequence_valid without a constraint must not parse");
    assert!(err.to_string().contains("asserts nothing"), "{}", err);
}

#[test]
fn test_tagged_sequence_valid_with_empty_sequence_is_an_exact_constraint() {
    let yaml = r#"
            id: vacuous_tagged_sequence
            input: "test"
            expected:
              type: sequence_valid
              sequence: []
        "#;
    let tc: TestCase = serde_yaml::from_str(yaml)
        .expect("an empty exact sequence requires the trace to contain no tool calls");
    assert!(matches!(
        tc.expected,
        Expected::SequenceValid {
            sequence: Some(ref sequence),
            ..
        } if sequence.is_empty()
    ));
}

#[test]
fn test_legacy_empty_sequence_is_an_exact_constraint() {
    let yaml = r#"
            id: vacuous_legacy_sequence
            input: "test"
            expected:
              sequence: []
        "#;
    serde_yaml::from_str::<TestCase>(yaml)
        .expect("legacy empty sequence still requires a trace with no tool calls");
}

#[test]
fn test_empty_inline_rules_cannot_erase_a_referenced_policy() {
    let yaml = r#"
            id: erased_policy
            input: "test"
            expected:
              type: sequence_valid
              policy: checks.yaml
              rules: []
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("empty inline rules override the referenced policy and assert nothing");
    assert!(err.to_string().contains("asserts nothing"), "{err}");
}

#[test]
fn test_empty_sequence_with_nonempty_rules_still_parses() {
    let yaml = r#"
            id: rule_constrained_sequence
            input: "test"
            expected:
              type: sequence_valid
              sequence: []
              rules:
                - type: require
                  tool: Search
        "#;
    let tc: TestCase = serde_yaml::from_str(yaml).expect("nonempty rules assert a constraint");
    match tc.expected {
        Expected::SequenceValid { rules, .. } => {
            assert_eq!(rules.expect("rules").len(), 1);
        }
        other => panic!("Expected SequenceValid, got {:?}", other),
    }
}

#[test]
fn test_tagged_must_contain_with_only_empty_strings_is_hard_error() {
    let yaml = r#"
            id: vacuous_tagged_must_contain
            input: "test"
            expected:
              type: must_contain
              must_contain: [""]
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("must_contain with only empty strings must not parse");
    assert!(err.to_string().contains("asserts nothing"), "{}", err);
}

#[test]
fn test_legacy_scalar_empty_must_contain_is_hard_error() {
    let yaml = r#"
            id: vacuous_legacy_must_contain
            input: "test"
            expected:
              must_contain: ""
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("legacy empty must_contain must not parse");
    assert!(err.to_string().contains("asserts nothing"), "{}", err);
}

#[test]
fn test_tagged_empty_regex_match_is_hard_error() {
    let yaml = r#"
            id: vacuous_regex
            input: "test"
            expected:
              type: regex_match
              pattern: ""
        "#;
    let err =
        serde_yaml::from_str::<TestCase>(yaml).expect_err("an empty positive regex must not parse");
    assert!(err.to_string().contains("asserts nothing"), "{}", err);
}

#[test]
fn test_tagged_empty_tool_blocklist_is_hard_error() {
    let yaml = r#"
            id: vacuous_blocklist
            input: "test"
            expected:
              type: tool_blocklist
              blocked: []
        "#;
    let err =
        serde_yaml::from_str::<TestCase>(yaml).expect_err("an empty tool blocklist must not parse");
    assert!(err.to_string().contains("asserts nothing"), "{}", err);
}

#[test]
fn test_nonempty_semantic_constraints_still_parse() {
    let cases = [
        r#"
            id: constrained_must_contain
            input: "test"
            expected:
              type: must_contain
              must_contain: ["needle", ""]
        "#,
        r#"
            id: constrained_regex
            input: "test"
            expected:
              type: regex_match
              pattern: "needle"
        "#,
        r#"
            id: constrained_blocklist
            input: "test"
            expected:
              type: tool_blocklist
              blocked: ["exec"]
        "#,
    ];

    for yaml in cases {
        serde_yaml::from_str::<TestCase>(yaml).expect("a nonempty constraint must parse");
    }
}

#[test]
fn test_semantic_similarity_at_cosine_floor_is_hard_error() {
    let yaml = r#"
            id: vacuous_similarity
            input: "test"
            expected:
              type: semantic_similarity_to
              semantic_similarity_to: "reference"
              min_score: -1.0
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("the cosine floor cannot reject a valid similarity score");
    assert!(err.to_string().contains("asserts nothing"), "{}", err);

    let epsilon_floor = yaml.replace("-1.0", "-0.9999995");
    let err = serde_yaml::from_str::<TestCase>(&epsilon_floor)
        .expect_err("the evaluator epsilon makes this threshold universally passing");
    assert!(err.to_string().contains("asserts nothing"), "{err}");

    let constrained = yaml.replace("-1.0", "-0.99");
    serde_yaml::from_str::<TestCase>(&constrained)
        .expect("a threshold above the cosine floor must parse");
}

#[test]
fn test_judge_criteria_without_an_evaluator_is_hard_error() {
    let yaml = r#"
            id: unsupported_judge
            input: "test"
            expected:
              type: judge_criteria
              judge_criteria:
                rubric: "be concise"
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("an Expected variant with no evaluator must not parse");
    assert!(err.to_string().contains("not executable"), "{}", err);
}

#[test]
fn test_unimplemented_sequence_rules_are_hard_errors() {
    let rules = [
        "eventually\n                  tool: Search\n                  within: 2",
        "max_calls\n                  tool: Search\n                  max: 2",
        "after\n                  trigger: Search\n                  then: Create\n                  within: 2",
        "never_after\n                  trigger: Delete\n                  forbidden: Export",
        "sequence\n                  tools: [Search, Create]\n                  strict: true",
    ];

    for rule in rules {
        let yaml = format!(
            r#"
            id: unsupported_sequence_rule
            input: "test"
            expected:
              type: sequence_valid
              rules:
                - type: {rule}
        "#
        );
        let err = serde_yaml::from_str::<TestCase>(&yaml)
            .expect_err("a sequence rule ignored by the evaluator must not parse");
        assert!(err.to_string().contains("not executable"), "{}", err);
    }
}

#[test]
fn test_tautological_before_rule_is_hard_error() {
    let yaml = r#"
            id: tautological_before
            input: "test"
            expected:
              type: sequence_valid
              rules:
                - type: before
                  first: Search
                  then: Search
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("before with identical operands passes every trace");
    assert!(err.to_string().contains("cannot constrain"), "{}", err);
}

#[test]
fn test_supported_sequence_rules_still_parse() {
    let yaml = r#"
            id: supported_sequence_rules
            input: "test"
            expected:
              type: sequence_valid
              rules:
                - type: require
                  tool: Search
                - type: before
                  first: Search
                  then: Create
                - type: blocklist
                  pattern: Delete
        "#;
    serde_yaml::from_str::<TestCase>(yaml).expect("implemented sequence rules must parse");
}

#[test]
fn test_obviously_universal_args_schemas_are_hard_errors() {
    let schemas = ["{}", "{Search: {}}", "{Search: true, Create: {}}"];

    for schema in schemas {
        let yaml = format!(
            r#"
            id: vacuous_args_schema
            input: "test"
            expected:
              type: args_valid
              policy: ignored-by-inline-schema.yaml
              schema: {schema}
        "#
        );
        let err = serde_yaml::from_str::<TestCase>(&yaml)
            .expect_err("an inline schema map that accepts everything must not parse");
        assert!(err.to_string().contains("asserts nothing"), "{}", err);
    }
}

#[test]
fn test_obviously_universal_output_schemas_are_hard_errors() {
    let schemas = ["{}", "{Search: {}}", "{Search: true, Create: {}}"];

    for schema in schemas {
        let yaml = format!(
            r#"
            id: vacuous_output_schema
            input: "test"
            expected:
              type: tool_output_valid
              schemas: {schema}
        "#
        );
        let err = serde_yaml::from_str::<TestCase>(&yaml)
            .expect_err("an output schema map that accepts everything must not parse");
        assert!(err.to_string().contains("asserts nothing"), "{}", err);
    }
}

#[test]
fn test_constraining_schema_maps_still_parse() {
    let cases = [
        r#"
            id: constrained_args_schema
            input: "test"
            expected:
              type: args_valid
              schema:
                Search:
                  type: object
                  required: [query]
        "#,
        r#"
            id: constrained_output_schema
            input: "test"
            expected:
              type: tool_output_valid
              schemas:
                Search:
                  type: object
                  required: [results]
        "#,
    ];

    for yaml in cases {
        serde_yaml::from_str::<TestCase>(yaml).expect("a constraining schema map must parse");
    }
}

#[test]
fn test_structured_policy_combines_trivial_schemas_with_effective_enforcement() {
    let yaml = r#"
            id: structured_allowlist
            input: "test"
            expected:
              type: args_valid
              schema:
                version: "2.0"
                enforcement:
                  unconstrained_tools: deny
                schemas:
                  Search: true
        "#;

    let test = serde_yaml::from_str::<TestCase>(yaml)
        .expect("trivial schemas participate in an effective structured allowlist");
    crate::model::validate_test_case_for_execution(&test)
        .expect("combined structured constraints must be validated in context");
}

#[test]
fn test_explicit_schema_containers_preserve_keyword_tool_names() {
    let cases = [
        r#"
            id: structured_keyword_tool
            input: "test"
            expected:
              type: args_valid
              schema:
                version: "2.0"
                schemas:
                  properties:
                    type: object
                    required: [query]
        "#,
        r#"
            id: output_keyword_tool
            input: "test"
            expected:
              type: tool_output_valid
              schemas:
                type:
                  type: object
                  required: [result]
        "#,
        r#"
            id: bare_metadata_named_tool
            input: "test"
            expected:
              type: args_valid
              schema:
                allow:
                  type: object
                  required: [query]
        "#,
    ];

    for yaml in cases {
        let test = serde_yaml::from_str::<TestCase>(yaml)
            .expect("an explicit schema container must not reserve valid tool names");
        crate::model::validate_test_case_for_execution(&test)
            .expect("explicit containers remove root-schema ambiguity");
    }
}

#[test]
fn schemas_only_policy_shape_is_rejected_as_ambiguous() {
    let ambiguous = serde_json::json!({
        "schemas": {
            "properties": {
                "query": {"type": "string"}
            }
        }
    });
    let err = crate::model::validate_args_policy_value(&ambiguous)
        .expect_err("schemas-only input has two valid interpretations");
    assert!(err.to_string().contains("ambiguous"), "{err:#}");

    let explicit = serde_json::json!({
        "version": "2.0",
        "schemas": {
            "schemas": {
                "properties": {
                    "query": {"type": "string"}
                }
            }
        }
    });
    crate::model::validate_args_policy_value(&explicit)
        .expect("a versioned container disambiguates the tool named schemas");
}

#[test]
fn structured_args_policy_rejects_unenforced_nested_tool_controls() {
    let policy = serde_json::json!({
        "version": "2.0",
        "tools": {
            "deny": ["never_called"],
            "restrict_scope": ["exec"]
        }
    });

    let err = crate::model::validate_args_policy_value(&policy)
        .expect_err("nested controls outside the args_valid evaluator must not be ignored");
    assert!(err.to_string().contains("tools.restrict_scope"), "{err:#}");
}

#[test]
fn structured_args_policy_supports_shared_defs_but_defs_alone_are_vacuous() {
    let shared_defs = serde_json::json!({
        "safe_path": {
            "type": "string",
            "pattern": "^/workspace/.*"
        }
    });
    let policy = serde_json::json!({
        "version": "2.0",
        "schemas": {
            "$defs": shared_defs.clone(),
            "read_file": {
                "type": "object",
                "properties": {
                    "path": {"$ref": "#/$defs/safe_path"}
                },
                "required": ["path"]
            }
        }
    });
    crate::model::validate_args_policy_value(&policy)
        .expect("documented shared definitions must compile for each tool schema");

    let defs_only = serde_json::json!({
        "version": "2.0",
        "schemas": {"$defs": shared_defs}
    });
    let err = crate::model::validate_args_policy_value(&defs_only)
        .expect_err("shared definitions without a tool schema assert nothing");
    assert!(err.to_string().contains("asserts nothing"), "{err:#}");
}

#[test]
fn malformed_structured_version_never_falls_back_to_a_legacy_tool_map() {
    let malformed = serde_json::json!({
        "version": {},
        "schemas": {
            "read_file": {"type": "object"}
        }
    });
    let err = crate::model::validate_args_policy_value(&malformed)
        .expect_err("a present structured discriminator must be validated");
    assert!(
        err.to_string().contains("version must be a string"),
        "{err:#}"
    );

    let explicit_tool = serde_json::json!({
        "version": "2.0",
        "schemas": {
            "version": {"type": "object"}
        }
    });
    crate::model::validate_args_policy_value(&explicit_tool)
        .expect("the structured container still permits a tool named version");
}

#[test]
fn shared_defs_must_be_a_mapping_even_for_boolean_tool_schemas() {
    let malformed = serde_json::json!({
        "version": "2.0",
        "schemas": {
            "$defs": ["not", "a", "mapping"],
            "deny_all": false
        }
    });
    let err = crate::model::validate_args_policy_value(&malformed)
        .expect_err("malformed shared definitions must not disappear beside a boolean schema");
    assert!(
        err.to_string().contains("$defs must be a mapping"),
        "{err:#}"
    );
}

#[test]
fn shared_defs_are_validated_even_when_all_tool_schemas_are_boolean() {
    let malformed = serde_json::json!({
        "version": "2.0",
        "schemas": {
            "$defs": {
                "invalid": {"type": "not-a-json-schema-type"}
            },
            "deny_all": false
        }
    });
    let err = crate::model::validate_args_policy_value(&malformed)
        .expect_err("shared definitions are policy input even when no object schema uses them");
    assert!(
        err.to_string().contains("shared $defs failed to compile"),
        "{err:#}"
    );
}

#[test]
fn tool_local_defs_without_assertions_are_vacuous() {
    let policy = serde_json::json!({
        "Search": {
            "$defs": {
                "query": {"type": "string"}
            }
        }
    });
    let err = crate::model::validate_args_policy_value(&policy)
        .expect_err("definitions do not constrain a tool call unless a schema references them");
    assert!(err.to_string().contains("asserts nothing"), "{err:#}");
}

#[test]
fn tool_schema_annotations_do_not_make_shared_defs_effective() {
    let annotations = [
        ("$comment", serde_json::json!("implementation note")),
        ("$id", serde_json::json!("urn:assay:test-schema")),
        (
            "$schema",
            serde_json::json!("https://json-schema.org/draft/2020-12/schema"),
        ),
        ("title", serde_json::json!("Search arguments")),
        ("description", serde_json::json!("Shared query definitions")),
        ("default", serde_json::json!({})),
        ("deprecated", serde_json::json!(false)),
        ("readOnly", serde_json::json!(true)),
        ("writeOnly", serde_json::json!(true)),
        ("examples", serde_json::json!([])),
        ("id", serde_json::json!("legacy-schema-id")),
        (
            "definitions",
            serde_json::json!({"unused": {"type": "string"}}),
        ),
        ("$recursiveAnchor", serde_json::json!(true)),
        ("x-note", serde_json::json!("extension annotation")),
    ];

    for (keyword, annotation) in annotations {
        let mut schema = serde_json::json!({
            "$defs": {"query": {"type": "string"}}
        });
        schema
            .as_object_mut()
            .expect("schema object")
            .insert(keyword.to_string(), annotation);
        let policy = serde_json::json!({"Search": schema});
        let err = crate::model::validate_args_policy_value(&policy)
            .expect_err("annotations do not constrain any instance");
        assert!(
            err.to_string().contains("asserts nothing"),
            "{keyword}: {err:#}"
        );
    }
}

#[test]
fn empty_required_does_not_make_a_schema_effective() {
    let no_ops = [
        serde_json::json!({"required": []}),
        serde_json::json!({"dependentRequired": {"kind": []}}),
        serde_json::json!({"properties": {"query": true}}),
        serde_json::json!({"allOf": [true]}),
        serde_json::json!({"anyOf": [true]}),
        serde_json::json!({"oneOf": [true]}),
        serde_json::json!({"if": {"type": "string"}}),
        serde_json::json!({"minLength": 0}),
        serde_json::json!({"uniqueItems": false}),
        serde_json::json!({"$defs": {"noop": true}, "$ref": "#/$defs/noop"}),
        serde_json::json!({"pattern": ""}),
        serde_json::json!({"oneOf": [true, false]}),
        serde_json::json!({"contains": false, "minContains": 0}),
        serde_json::json!({"if": false, "then": false}),
        serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "additionalItems": false
        }),
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-04/schema#",
            "const": 5
        }),
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "dependentSchemas": {"credit_card": false}
        }),
    ];
    for schema in no_ops {
        let policy = serde_json::json!({"Search": schema});
        let err = crate::model::validate_args_policy_value(&policy)
            .expect_err("a no-op JSON Schema must constrain no instance");
        assert!(err.to_string().contains("asserts nothing"), "{err:#}");
    }

    for schema in [
        serde_json::json!({"required": ["query"]}),
        serde_json::json!({"properties": {"query": false}}),
        serde_json::json!({"oneOf": [true, true]}),
        serde_json::json!({"if": true, "then": false}),
        serde_json::json!({"minLength": 1}),
        serde_json::json!({"uniqueItems": true}),
        serde_json::json!({"$defs": {"deny": false}, "$ref": "#/$defs/deny"}),
        serde_json::json!({"contains": true}),
        serde_json::json!({"if": false, "else": false}),
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "dependencies": {"credit_card": ["billing_address"]}
        }),
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "format": "email"
        }),
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "items": [true],
            "additionalItems": false
        }),
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "contains": false,
            "minContains": 0
        }),
    ] {
        let policy = serde_json::json!({"Search": schema});
        crate::model::validate_args_policy_value(&policy)
            .expect("an asserting JSON Schema must remain executable");
    }
}

#[test]
fn ref_shaped_instance_data_is_not_a_schema_reference() {
    let policy = serde_json::json!({
        "version": "2.0",
        "schemas": {
            "record": {"const": {"$ref": "https://example.invalid/instance-data"}}
        }
    });
    crate::model::validate_args_policy_value(&policy)
        .expect("a $ref key inside const is ordinary instance data");
}

#[test]
fn json_schema_expected_does_not_retrieve_file_refs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let schema_path = dir.path().join("external.json");
    std::fs::write(&schema_path, r#"{"type":"string"}"#).expect("write external schema");
    let external_ref = url::Url::from_file_path(&schema_path)
        .expect("absolute path becomes file URL")
        .to_string();
    let schema = serde_json::json!({"$ref": external_ref}).to_string();
    let yaml = format!(
        "id: local_only_schema\ninput: test\nexpected:\n  type: json_schema\n  json_schema: '{}'\n",
        schema.replace('\'', "''")
    );
    let test = serde_yaml::from_str::<TestCase>(&yaml).expect("test case parses");

    let err = crate::model::validate_test_case_for_execution(&test)
        .expect_err("preflight must not retrieve an external schema");
    assert!(err.to_string().contains("schema compile failed"), "{err:#}");
}

#[test]
fn shared_defs_cannot_overwrite_tool_local_definitions() {
    let collision = serde_json::json!({
        "version": "2.0",
        "schemas": {
            "$defs": {"identifier": {"type": "string"}},
            "lookup": {
                "$defs": {"identifier": {"type": "integer"}},
                "$ref": "#/$defs/identifier"
            }
        }
    });
    let err = crate::model::validate_args_policy_value(&collision)
        .expect_err("shared and local definitions need explicit collision semantics");
    assert!(
        err.to_string().contains("$defs entries must not overlap"),
        "{err:#}"
    );
}

#[test]
fn external_schema_refs_are_rejected_before_retrieval() {
    let dir = tempfile::tempdir().expect("tempdir");
    let schema_path = dir.path().join("external.json");
    std::fs::write(&schema_path, r#"{"type":"string"}"#).expect("write external schema");
    let external_ref = url::Url::from_file_path(&schema_path)
        .expect("absolute path becomes file URL")
        .to_string();
    let policy = serde_json::json!({
        "version": "2.0",
        "schemas": {"lookup": {"$ref": external_ref}}
    });

    let err = crate::model::validate_args_policy_value(&policy)
        .expect_err("policy validation must never retrieve an external schema");
    assert!(
        err.to_string()
            .contains("external JSON Schema retrieval is disabled"),
        "{err:#}"
    );
}

#[test]
fn test_tagged_tool_output_valid_without_schemas_is_hard_error() {
    let yaml = r#"
            id: vacuous_output
            input: "test"
            expected:
              type: tool_output_valid
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("tool_output_valid without schemas must not parse");
    assert!(err.to_string().contains("asserts nothing"), "{}", err);
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

/// A failed tagged parse must not change metric type through an unrelated legacy
/// key. This block asks for `regex_match`; accepting it as `must_contain` would
/// silently enforce a different assertion than the author selected.
#[test]
fn test_tagged_parse_failure_cannot_fallback_to_different_legacy_metric() {
    let yaml = r#"
            id: mismatched_tag
            input: "test"
            expected:
              type: regex_match
              must_contain: "not-the-dummy-output"
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("failed tagged parse must not change metric type");
    let msg = err.to_string();
    assert!(
        msg.contains("pattern"),
        "message must preserve the tagged parse failure: {}",
        msg
    );
}

#[test]
fn test_unrelated_tagged_failure_is_not_replaced_by_legacy_value_error() {
    let yaml = r#"
            id: mismatched_malformed_legacy
            input: "test"
            expected:
              type: regex_match
              must_contain: {not: a-list}
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("an unrelated legacy decoder must not replace the tagged error");
    let msg = err.to_string();
    assert!(
        msg.contains("invalid `expected:` block") && !msg.contains("must be a string or a list"),
        "message must preserve the tagged parse failure: {}",
        msg
    );
}

#[test]
fn test_valid_tagged_metric_rejects_additional_legacy_assertion() {
    let yaml = r#"
            id: tagged_extra
            input: "test"
            expected:
              type: regex_match
              pattern: "^hello$"
              must_contain: "ignored"
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("a tagged assertion must not ignore another assertion");
    assert!(err.to_string().contains("must_contain"), "{}", err);
}

#[test]
fn test_valid_tagged_metric_rejects_unknown_field() {
    let yaml = r#"
            id: tagged_typo
            input: "test"
            expected:
              type: regex_match
              pattern: "^hello$"
              pattten: "ignored"
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("a tagged assertion must not ignore a misspelled field");
    assert!(err.to_string().contains("pattten"), "{}", err);
}

#[test]
fn test_legacy_metric_rejects_unknown_field() {
    let yaml = r#"
            id: legacy_typo
            input: "test"
            expected:
              must_contain: "hello"
              extra_check: "ignored"
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("a legacy assertion must not ignore a misspelled field");
    assert!(err.to_string().contains("extra_check"), "{}", err);
}

#[test]
fn test_tagged_legacy_compatibility_rejects_second_assertion() {
    let yaml = r#"
            id: tagged_ambiguous
            input: "test"
            expected:
              type: must_contain
              must_contain: "hello"
              sequence: ["Search"]
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("tagged compatibility must not hide a second assertion");
    assert!(err.to_string().contains("ambiguous"), "{}", err);
}

#[test]
fn test_scalar_expected_value_is_rejected() {
    let yaml = r#"
            id: scalar_expected
            input: "test"
            expected: "hello"
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml).expect_err("scalar expected must not parse");
    assert!(err.to_string().contains("must be a mapping"), "{}", err);
}

#[test]
fn test_legacy_ref_still_parses_and_requires_a_string() {
    let valid = r#"
            id: ref_test
            input: "test"
            expected:
              $ref: "shared/checks.yaml"
        "#;
    let tc: TestCase = serde_yaml::from_str(valid).expect("legacy $ref must parse");
    match tc.expected {
        Expected::Reference { path } => assert_eq!(path, "shared/checks.yaml"),
        other => panic!("Expected Reference, got {:?}", other),
    }

    let invalid = r#"
            id: bad_ref
            input: "test"
            expected:
              $ref: 42
        "#;
    let err =
        serde_yaml::from_str::<TestCase>(invalid).expect_err("non-string $ref must not parse");
    assert!(
        err.to_string().contains("`$ref` must be a string"),
        "{}",
        err
    );
}

#[test]
fn test_legacy_schema_parses_but_cannot_be_combined() {
    let valid = r#"
            id: schema_test
            input: "test"
            expected:
              schema:
                Search: {type: object}
        "#;
    let tc: TestCase = serde_yaml::from_str(valid).expect("legacy schema must parse");
    assert!(matches!(tc.expected, Expected::ArgsValid { .. }));

    let ambiguous = r#"
            id: schema_ambiguous
            input: "test"
            expected:
              schema: {Search: {type: object}}
              must_contain: "hello"
        "#;
    let err = serde_yaml::from_str::<TestCase>(ambiguous)
        .expect_err("schema plus another legacy assertion must not parse");
    assert!(err.to_string().contains("ambiguous"), "{}", err);
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

/// Multiple legacy keys are multiple assertions. Choosing one would silently
/// discard the rest, so the single-assertion model must reject the block.
#[test]
fn test_ambiguous_legacy_expected_is_rejected() {
    let yaml = r#"
            id: ambiguous_legacy
            input: "test"
            expected:
              must_contain: "passed"
              sequence: ["Search"]
        "#;
    let err = serde_yaml::from_str::<TestCase>(yaml)
        .expect_err("ambiguous legacy assertions must not be truncated");
    let msg = err.to_string();
    assert!(msg.contains("ambiguous"), "{}", msg);
    assert!(
        msg.contains("must_contain") && msg.contains("sequence"),
        "{}",
        msg
    );
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

#[test]
fn test_explicit_expected_variant_is_not_erased_during_serialization() {
    let tc = TestCase {
        id: "explicit-regex".into(),
        input: TestInput {
            prompt: "test".into(),
            context: None,
        },
        expected: Expected::RegexMatch {
            pattern: String::new(),
            flags: Vec::new(),
        },
        assertions: None,
        on_error: None,
        tags: Vec::new(),
        metadata: None,
    };

    let written = serde_yaml::to_string(&tc).expect("serialize");
    assert!(written.contains("regex_match"), "{written}");
    assert!(written.contains("pattern"), "{written}");
}

#[test]
fn test_impossible_negative_assertions_are_rejected() {
    for yaml in [
        r#"
            id: impossible_substring
            input: "test"
            expected:
              type: must_not_contain
              must_not_contain: [""]
        "#,
        r#"
            id: impossible_regex
            input: "test"
            expected:
              type: regex_not_match
              pattern: ""
        "#,
    ] {
        let err = serde_yaml::from_str::<TestCase>(yaml)
            .expect_err("an assertion that no response can satisfy must be rejected");
        assert!(err.to_string().contains("pass"), "{err}");
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

#[test]
fn f1_structured_policy_with_only_object_roots_is_not_vacuous() {
    // Regression: a structured args_valid policy whose root values are all
    // objects (no scalar discriminator) must not be rejected as vacuous.
    for schema in [
        serde_json::json!({
            "tools": {"allow": ["read_*"]},
            "schemas": {"read_file": {"type": "object", "required": ["path"]}}
        }),
        serde_json::json!({
            "enforcement": {"unconstrained_tools": "deny"},
            "schemas": {"read_file": {"type": "object", "required": ["path"]}}
        }),
        // Maximally strict and carrying no schemas at all.
        serde_json::json!({"tools": {"deny": ["*"]}}),
    ] {
        let expected = Expected::ArgsValid {
            schema: Some(schema.clone()),
            policy: None,
        };
        assert!(
            crate::model::validation::vacuous_expected_field(&expected).is_none(),
            "structured policy wrongly rejected as vacuous: {schema}"
        );
    }
}

#[test]
fn dependencies_asserts_under_every_dialect() {
    // jsonschema compiles `dependencies` whenever the applicator vocabulary is
    // declared, which 2020-12 does — so a policy using it is enforced at
    // runtime and must not be rejected at parse as vacuous.
    for schema in [
        serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "dependencies": {"credit_card": ["billing_address"]}
        }),
        serde_json::json!({"dependencies": {"credit_card": ["billing_address"]}}),
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "dependencies": {"credit_card": ["billing_address"]}
        }),
    ] {
        let policy = serde_json::json!({"pay": schema.clone()});
        crate::model::validate_args_policy_value(&policy)
            .unwrap_or_else(|e| panic!("`dependencies` must assert: {schema} -> {e:#}"));
    }
}
