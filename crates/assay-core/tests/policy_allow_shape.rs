use assay_core::mcp::policy::McpPolicy;

#[test]
fn allow_configuration_shape_is_consistent_across_readers_and_migration() {
    for (body, valid) in [
        ("{}", true),
        ("tools: {}", true),
        ("tools: {allow: [], allow_classes: []}", true),
        (
            "tools: {allow: [Read], allow_classes: [documentation]}",
            true,
        ),
        ("tools:", false),
        ("tools: null", false),
        ("tools: !example null", false),
        ("tools: !example null\nallow: [Read]", false),
        ("tools: !example\nallow: [Read]", false),
        ("tools:\n  allow:", false),
        ("tools: {allow_classes: ~}", false),
    ] {
        let bytes = body.as_bytes();
        for (route, accepted) in [
            ("parse", McpPolicy::from_slice(bytes).is_ok()),
            ("source", McpPolicy::from_slice_with_source(bytes).is_ok()),
            ("coverage", McpPolicy::load_for_coverage(body).is_ok()),
            ("format", McpPolicy::format_yaml(bytes).is_ok()),
            ("migrate", McpPolicy::migrate_yaml(bytes).is_ok()),
            (
                "migrate coverage",
                McpPolicy::migrate_coverage_yaml(bytes).is_ok(),
            ),
        ] {
            assert_eq!(accepted, valid, "{route}: {body}");
        }
    }
}

#[test]
fn legacy_root_allow_requires_an_explicit_list() {
    for migrate in [McpPolicy::migrate_yaml, McpPolicy::migrate_coverage_yaml] {
        for input in ["allow:", "allow: null"] {
            assert!(migrate(input.as_bytes()).is_err(), "{input}");
        }
        for (input, expected) in [
            ("allow: []", vec![]),
            ("allow: [Read]", vec!["Read".to_owned()]),
        ] {
            let migrated = migrate(input.as_bytes()).unwrap();
            assert_eq!(
                McpPolicy::from_slice(migrated.as_bytes())
                    .unwrap()
                    .tools
                    .allow,
                Some(expected)
            );
        }
    }
}

#[test]
fn omission_and_explicit_empty_lists_survive_migration() {
    for input in ["{}", "tools: {}", "tools: {allow: [], allow_classes: []}"] {
        let original = McpPolicy::from_slice(input.as_bytes()).unwrap();
        let expected = input.contains("allow").then(Vec::<String>::new);
        assert_eq!(original.tools.allow, expected);
        assert_eq!(original.tools.allow_classes, expected);
        for migrate in [McpPolicy::migrate_yaml, McpPolicy::migrate_coverage_yaml] {
            let converted = migrate(input.as_bytes()).unwrap();
            let loaded = McpPolicy::from_slice(converted.as_bytes()).unwrap();
            assert_eq!(loaded.tools, original.tools);
        }
        let encoded = serde_json::to_value(&original).unwrap();
        let reloaded: McpPolicy = serde_json::from_value(encoded).unwrap();
        assert_eq!(reloaded.policy_digest(), original.policy_digest());
    }
}

#[test]
fn historical_digest_keeps_the_original_null_semantics() {
    let digest = McpPolicy::from_slice(b"{}")
        .unwrap()
        .policy_digest()
        .unwrap();
    assert_eq!(
        McpPolicy::retained_policy_digest(b"tools: {allow: null}").unwrap(),
        digest
    );
}
