use super::*;

#[test]
fn extract_reads_declared_boolean_hints() {
    let declared = extract_declared_annotations(&json!({
        "readOnlyHint": true,
        "destructiveHint": false,
        "idempotentHint": true,
        "openWorldHint": false,
    }));
    assert_eq!(declared.read_only, Some(true));
    assert_eq!(declared.destructive, Some(false));
    assert_eq!(declared.idempotent, Some(true));
    assert_eq!(declared.open_world, Some(false));
}

#[test]
fn extract_absent_hints_are_none_not_default() {
    // MCP defines schema defaults (e.g. readOnlyHint false), but the carrier records what the
    // server actually declared: an absent hint is undeclared, never the default.
    let declared = extract_declared_annotations(&json!({"title": "Add deploy key"}));
    assert_eq!(declared.read_only, None);
    assert_eq!(declared.destructive, None);
    assert_eq!(declared.idempotent, None);
    assert_eq!(declared.open_world, None);
}

#[test]
fn extract_null_or_non_object_annotations_are_all_none() {
    for value in [
        json!(null),
        json!("readOnlyHint"),
        json!(["readOnlyHint"]),
        json!(42),
    ] {
        let declared = extract_declared_annotations(&value);
        assert_eq!(
            (
                declared.read_only,
                declared.destructive,
                declared.idempotent,
                declared.open_world
            ),
            (None, None, None, None),
            "non-object annotations must not declare any hint: {value}"
        );
    }
}

#[test]
fn extract_non_boolean_hint_values_are_none() {
    // A hint declared with a non-boolean value is not a trustworthy boolean; record None.
    let declared = extract_declared_annotations(&json!({
        "readOnlyHint": "true",
        "destructiveHint": 1,
        "idempotentHint": null,
        "openWorldHint": {},
    }));
    assert_eq!(
        (
            declared.read_only,
            declared.destructive,
            declared.idempotent,
            declared.open_world
        ),
        (None, None, None, None)
    );
}

#[test]
fn extract_partial_hints_keep_declared_and_drop_absent() {
    let declared = extract_declared_annotations(&json!({"readOnlyHint": false}));
    assert_eq!(declared.read_only, Some(false));
    assert_eq!(declared.destructive, None);
    assert_eq!(declared.idempotent, None);
    assert_eq!(declared.open_world, None);
}
