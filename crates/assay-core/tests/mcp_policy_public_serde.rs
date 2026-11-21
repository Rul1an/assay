use assay_core::mcp::policy::McpPolicy;
use assay_core::mcp::tool_decision_truth::{decision_verdict, DecisionEvidence};
use serde_json::{json, Value};

#[test]
fn public_serde_refuses_removed_shapes_accepted_by_the_converter() {
    let mut accepted = Vec::new();
    for (name, input) in [
        ("version", json!({"version":"1.0"})),
        ("allow", json!({"allow":["Read"]})),
        ("deny", json!({"deny":null})),
        ("constraints", json!({"constraints":[]})),
    ] {
        let bytes = serde_json::to_vec(&input).unwrap();
        let migrated = McpPolicy::migrate_yaml(&bytes).unwrap();
        McpPolicy::from_slice(migrated.as_bytes()).unwrap();
        if serde_json::from_value::<McpPolicy>(input).is_ok() {
            accepted.push(name);
        }
    }
    assert!(accepted.is_empty(), "accepted removed fields: {accepted:?}");
}

#[test]
fn current_public_serde_roundtrips_and_keeps_the_released_digest() {
    let policy: McpPolicy =
        serde_json::from_value(json!({"version":"2.0","tools":{"allow":["Read"]}})).unwrap();
    let encoded = serde_json::to_value(&policy).unwrap();
    let roundtrip: McpPolicy = serde_json::from_value::<McpPolicy>(encoded.clone()).unwrap();
    let digest = "sha256:3f10a20df4fb9e7d2f45f61ab722a7fbbfed163810142959bdae192d98753c55";
    assert_eq!(roundtrip.policy_digest().as_deref(), Some(digest));
    let object = Value::as_object(&encoded).unwrap();
    for removed in ["allow", "deny", "constraints"] {
        assert!(
            !object.contains_key(removed),
            "public serialization writes removed field {removed}"
        );
    }
}

#[test]
fn overlapping_legacy_declarations_have_distinct_migration_routes() {
    let mixed = json!({
        "version": "2.0",
        "tools": {"allow": ["deploy"]},
        "schemas": {"deploy": {"type":"object","required":["env"],
            "properties":{"env":{"type":"string","enum":["staging"]}}}},
        "constraints": [{"tool":"deploy","params":{"env":{"matches":"^prod$"}}}]
    });
    let verdicts = |policy: &McpPolicy| {
        ["staging", "prod"].map(|env| {
            decision_verdict(
                policy,
                "deploy",
                Some(&json!({"env": env})),
                "present",
                &DecisionEvidence::default(),
            )
        })
    };
    assert!(serde_json::from_value::<McpPolicy>(mixed.clone()).is_err());
    // Port the old direct Rust verdict: this fixture has just one legacy rule,
    // for a tool with an explicit schema. The file converter has different precedence.
    let mut explicit = mixed.clone();
    explicit.as_object_mut().unwrap().remove("constraints");
    let direct = serde_json::from_value::<McpPolicy>(explicit).unwrap();
    let converted = McpPolicy::migrate_yaml(&serde_json::to_vec(&mixed).unwrap()).unwrap();
    let file = McpPolicy::from_slice(converted.as_bytes()).unwrap();
    assert_eq!(verdicts(&direct), ["match", "mismatch"]);
    assert_eq!(verdicts(&file), ["mismatch", "match"]);
}

#[test]
fn public_serde_and_file_readers_agree_on_declared_policy_shapes() {
    for (text, accepted) in [
        ("version: '2.0'\ntools: {allow: [Read]}", true),
        ("!policy {version: '2.0', tools: {deny: [Write]}}", true),
        ("x-label: &label Read\ntools: {allow: [*label]}", true),
        ("version: 2", false),
        ("deny: [Write]", false),
        ("tools: {alen: [Read]}", false),
        ("tools: {deny: [Write], deny: []}", false),
    ] {
        let file = McpPolicy::from_slice(text.as_bytes());
        let direct = serde_yaml::from_str::<McpPolicy>(text);
        assert_eq!(file.is_ok(), accepted, "{text}: {file:?}");
        assert_eq!(direct.is_ok(), accepted, "{text}: {direct:?}");
        if let (Ok(file), Ok(direct)) = (file, direct) {
            assert_eq!(file.policy_digest(), direct.policy_digest(), "{text}");
        }
    }
}
