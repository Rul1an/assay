use crate::json_strict::{
    check_next, validate_json_strict_bounded, BoundedJsonError, BoundedJsonHooks,
    BoundedJsonLimits, BudgetDimension, BudgetHit, BudgetLimit, BudgetTier, PathSegment,
    StringClass,
};
use crate::json_strict::{
    from_str_strict, validate_json_strict, StrictJsonError, MAX_KEYS_PER_OBJECT, MAX_STRING_LENGTH,
};
use serde::Deserialize;

struct TestHooks {
    items: u64,
    item_limit: BudgetLimit,
    entry_limit: Option<BudgetLimit>,
}

impl BoundedJsonHooks for TestHooks {
    fn item_start(
        &mut self,
        _path: &[PathSegment],
        _index: usize,
    ) -> Result<Option<BudgetLimit>, BudgetHit> {
        self.items = check_next(self.items, 1, self.item_limit, BudgetDimension::Items)?;
        Ok(self.entry_limit)
    }

    fn string_class(&self, path: &[PathSegment]) -> StringClass {
        if path
            .iter()
            .any(|part| matches!(part, PathSegment::Key(key) if key == "payload"))
        {
            StringClass::Payload
        } else {
            StringClass::NonPayload
        }
    }
}

fn bounded_limits() -> BoundedJsonLimits {
    BoundedJsonLimits {
        depth: BudgetLimit {
            hard: 64,
            selected: 64,
        },
        key_bytes: BudgetLimit {
            hard: 16,
            selected: 16,
        },
        non_payload_string_bytes: BudgetLimit {
            hard: 16,
            selected: 16,
        },
        payload_string_bytes: BudgetLimit {
            hard: 16,
            selected: 16,
        },
    }
}

fn hooks() -> TestHooks {
    TestHooks {
        items: 0,
        item_limit: BudgetLimit {
            hard: 16,
            selected: 16,
        },
        entry_limit: None,
    }
}

#[test]
fn bounded_route_charges_decoded_utf8_key_and_string_bytes() {
    let mut limits = bounded_limits();
    limits.key_bytes.selected = 1;
    let error = validate_json_strict_bounded(r#"{"é":"ok"}"#, limits, &mut hooks())
        .expect_err("two UTF-8 key bytes exceed one");
    assert!(matches!(
        error,
        BoundedJsonError::Budget(BudgetHit {
            dimension: BudgetDimension::KeyBytes,
            ..
        })
    ));

    let mut limits = bounded_limits();
    limits.payload_string_bytes.selected = 1;
    let error = validate_json_strict_bounded(r#"{"payload":"é"}"#, limits, &mut hooks())
        .expect_err("two decoded payload bytes exceed one");
    assert!(matches!(
        error,
        BoundedJsonError::Budget(BudgetHit {
            dimension: BudgetDimension::PayloadStringBytes,
            ..
        })
    ));
}

#[test]
fn bounded_route_charges_items_before_the_later_value() {
    let mut hooks = hooks();
    hooks.item_limit.selected = 1;
    let error = validate_json_strict_bounded(
        r#"[0,{"payload":"a very large later value"}]"#,
        bounded_limits(),
        &mut hooks,
    )
    .expect_err("the second item is outside the selected item budget");
    assert!(matches!(
        error,
        BoundedJsonError::Budget(BudgetHit {
            dimension: BudgetDimension::Items,
            ..
        })
    ));
}

#[test]
fn bounded_route_charges_raw_entry_bytes_during_traversal() {
    let mut hooks = hooks();
    hooks.entry_limit = Some(BudgetLimit {
        hard: 11,
        selected: 10,
    });
    let error = validate_json_strict_bounded(r#"[{"x":"123"}]"#, bounded_limits(), &mut hooks)
        .expect_err("the raw object is eleven bytes");
    assert!(matches!(
        error,
        BoundedJsonError::Budget(BudgetHit {
            dimension: BudgetDimension::EntryBytes,
            ..
        })
    ));
}

#[test]
fn bounded_route_charges_keyword_bytes_before_token_validity() {
    let mut hooks = hooks();
    hooks.entry_limit = Some(BudgetLimit {
        hard: 4,
        selected: 2,
    });
    for input in ["[false]", "[truX]"] {
        let error = validate_json_strict_bounded(input, bounded_limits(), &mut hooks)
            .expect_err("the selected entry ceiling is crossed before token validity is known");
        assert!(matches!(
            error,
            BoundedJsonError::Budget(BudgetHit {
                dimension: BudgetDimension::EntryBytes,
                tier: BudgetTier::Selected,
            })
        ));
    }
}

#[test]
fn bounded_route_charges_invalid_unicode_escape_bytes_before_syntax() {
    let mut hooks = hooks();
    hooks.entry_limit = Some(BudgetLimit {
        hard: 6,
        selected: 4,
    });
    let error = validate_json_strict_bounded(r#"["\u0X00"]"#, bounded_limits(), &mut hooks)
        .expect_err("the selected entry ceiling is crossed by the invalid escape byte");
    assert!(matches!(
        error,
        BoundedJsonError::Budget(BudgetHit {
            dimension: BudgetDimension::EntryBytes,
            tier: BudgetTier::Selected,
        })
    ));
}

#[test]
fn bounded_route_charges_decoded_string_before_legacy_ceiling() {
    let mut limits = bounded_limits();
    limits.payload_string_bytes = BudgetLimit {
        hard: MAX_STRING_LENGTH as u64,
        selected: MAX_STRING_LENGTH as u64,
    };
    let value = "a".repeat(MAX_STRING_LENGTH + 1);
    let input = format!(r#"{{"payload":"{value}"}}"#);
    let error = validate_json_strict_bounded(&input, limits, &mut hooks())
        .expect_err("the bounded hard ceiling is decided before the legacy string ceiling");
    assert!(matches!(
        error,
        BoundedJsonError::Budget(BudgetHit {
            dimension: BudgetDimension::PayloadStringBytes,
            tier: BudgetTier::Hard,
        })
    ));
}

#[test]
fn bounded_route_is_hard_first_and_has_exact_utf8_boundaries() {
    let mut limits = bounded_limits();
    limits.payload_string_bytes = BudgetLimit {
        hard: 3,
        selected: 1,
    };
    let error = validate_json_strict_bounded(r#"{"payload":"😀"}"#, limits, &mut hooks())
        .expect_err("four decoded UTF-8 bytes cross both ceilings");
    assert!(matches!(
        error,
        BoundedJsonError::Budget(BudgetHit {
            dimension: BudgetDimension::PayloadStringBytes,
            tier: BudgetTier::Hard,
        })
    ));

    let mut limits = bounded_limits();
    limits.payload_string_bytes = BudgetLimit {
        hard: 4,
        selected: 4,
    };
    validate_json_strict_bounded(r#"{"payload":"\uD83D\uDE00"}"#, limits, &mut hooks())
        .expect("escaped astral scalar is four decoded UTF-8 bytes");
}

#[test]
fn bounded_route_does_not_charge_phantom_items_or_flatten_paths() {
    let mut empty_hooks = hooks();
    empty_hooks.item_limit = BudgetLimit {
        hard: 0,
        selected: 0,
    };
    validate_json_strict_bounded("[]", bounded_limits(), &mut empty_hooks)
        .expect("empty arrays contain no item");

    struct PathHooks {
        seen: Vec<Vec<PathSegment>>,
    }
    impl BoundedJsonHooks for PathHooks {
        fn item_start(
            &mut self,
            path: &[PathSegment],
            _index: usize,
        ) -> Result<Option<BudgetLimit>, BudgetHit> {
            self.seen.push(path.to_vec());
            Ok(None)
        }

        fn string_class(&self, _path: &[PathSegment]) -> StringClass {
            StringClass::NonPayload
        }
    }
    let mut path_hooks = PathHooks { seen: Vec::new() };
    validate_json_strict_bounded(
        r#"{"a/b":[0],"a":{"b":[1]}}"#,
        bounded_limits(),
        &mut path_hooks,
    )
    .expect("structured paths are accepted");
    assert_eq!(
        path_hooks.seen,
        vec![
            vec![PathSegment::Key("a/b".into())],
            vec![PathSegment::Key("a".into()), PathSegment::Key("b".into())]
        ]
    );
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestStruct {
    key: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct NestedStruct {
    outer: Inner,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Inner {
    key: String,
}

// === B1: Duplicate Key Tests ===

#[test]
fn test_rejects_top_level_duplicate() {
    let json = r#"{"key": "first", "key": "second"}"#;
    let result = from_str_strict::<TestStruct>(json);

    assert!(matches!(
        result,
        Err(StrictJsonError::DuplicateKey { key, path })
        if key == "key" && path == "/"
    ));
}

#[test]
fn test_rejects_nested_duplicate() {
    let json = r#"{"outer": {"key": "a", "key": "b"}}"#;
    let result = from_str_strict::<NestedStruct>(json);

    assert!(matches!(
        result,
        Err(StrictJsonError::DuplicateKey { key, path })
        if key == "key" && path == "/outer"
    ));
}

#[test]
fn test_rejects_deeply_nested_duplicate() {
    let json = r#"{"data": {"scope": {"tools": ["a"], "tools": ["b"]}}}"#;
    let result = validate_json_strict(json);

    assert!(matches!(
        result,
        Err(StrictJsonError::DuplicateKey { key, path })
        if key == "tools" && path == "/data/scope"
    ));
}

#[test]
fn test_accepts_same_key_different_objects() {
    let json = r#"{"a": {"key": "1"}, "b": {"key": "2"}}"#;
    let result = validate_json_strict(json);
    assert!(result.is_ok());
}

#[test]
fn test_rejects_unicode_escape_duplicate() {
    let json = r#"{"a": 1, "\u0061": 2}"#;
    let result = validate_json_strict(json);

    assert!(
        matches!(&result, Err(StrictJsonError::DuplicateKey { key, .. }) if key == "a"),
        "Unicode escaped key must be detected as duplicate: {:?}",
        result
    );
}

#[test]
fn test_rejects_mixed_escape_duplicate() {
    let json = r#"{"Hello": 1, "\u0048\u0065\u006c\u006c\u006f": 2}"#;
    let result = validate_json_strict(json);

    assert!(
        matches!(&result, Err(StrictJsonError::DuplicateKey { key, .. }) if key == "Hello"),
        "Fully escaped key must be detected as duplicate: {:?}",
        result
    );
}

#[test]
fn test_rejects_partial_escape_duplicate() {
    let json = r#"{"key": 1, "k\u0065y": 2}"#;
    let result = validate_json_strict(json);

    assert!(
        matches!(&result, Err(StrictJsonError::DuplicateKey { key, .. }) if key == "key"),
        "Partially escaped key must be detected as duplicate: {:?}",
        result
    );
}

#[test]
fn test_rejects_surrogate_pair_duplicate() {
    let json = r#"{"😀": 1, "\uD83D\uDE00": 2}"#;
    let result = validate_json_strict(json);

    assert!(
        matches!(&result, Err(StrictJsonError::DuplicateKey { key, .. }) if key == "😀"),
        "Surrogate pair key must be detected as duplicate of direct UTF-8: {:?}",
        result
    );
}

#[test]
fn test_rejects_escaped_solidus_duplicate() {
    let json = r#"{"a/b": 1, "a\/b": 2}"#;
    let result = validate_json_strict(json);

    assert!(
        matches!(&result, Err(StrictJsonError::DuplicateKey { key, .. }) if key == "a/b"),
        "Escaped solidus key must be detected as duplicate: {:?}",
        result
    );
}

#[test]
fn test_rejects_escaped_quote_duplicate() {
    let json = r#"{"a\\b": 1, "a\u005Cb": 2}"#;
    let result = validate_json_strict(json);

    assert!(
        matches!(&result, Err(StrictJsonError::DuplicateKey { key, .. }) if key == "a\\b"),
        "Escaped backslash key must be detected as duplicate: {:?}",
        result
    );
}

#[test]
fn test_accepts_valid_json() {
    let json = r#"{"key": "value"}"#;
    let result: TestStruct = from_str_strict(json).unwrap();
    assert_eq!(result.key, "value");
}

// === B2: Lone Surrogate Tests ===

#[test]
fn test_rejects_lone_high_surrogate() {
    let json = r#"{"key": "\uD800"}"#;
    let result = validate_json_strict(json);
    assert!(matches!(result, Err(StrictJsonError::LoneSurrogate { .. })));
}

#[test]
fn test_rejects_lone_low_surrogate() {
    let json = r#"{"key": "\uDC00"}"#;
    let result = validate_json_strict(json);
    assert!(matches!(result, Err(StrictJsonError::LoneSurrogate { .. })));
}

#[test]
fn test_accepts_valid_surrogate_pair() {
    let json = r#"{"key": "\uD83D\uDE00"}"#;
    let result = validate_json_strict(json);
    assert!(result.is_ok());
}

#[test]
fn test_rejects_reversed_surrogate_pair() {
    let json = r#"{"key": "\uDC00\uD800"}"#;
    let result = validate_json_strict(json);
    assert!(matches!(result, Err(StrictJsonError::LoneSurrogate { .. })));
}

#[test]
fn test_accepts_non_surrogate_unicode() {
    let json = r#"{"key": "\u0041"}"#;
    let result = validate_json_strict(json);
    assert!(result.is_ok());
}

// === Edge Cases ===

#[test]
fn test_array_with_objects() {
    let json = r#"[{"key": "a", "key": "b"}]"#;
    let result = validate_json_strict(json);

    assert!(matches!(
        result,
        Err(StrictJsonError::DuplicateKey { key, .. })
        if key == "key"
    ));
}

#[test]
fn test_complex_nested_structure() {
    let json = r#"{
        "manifest": {"version": "1.0"},
        "events": [
            {"type": "test", "data": {"a": 1}},
            {"type": "test", "data": {"b": 2}}
        ]
    }"#;
    let result = validate_json_strict(json);
    assert!(result.is_ok());
}

#[test]
fn test_empty_objects_and_arrays() {
    let json = r#"{"empty_obj": {}, "empty_arr": []}"#;
    let result = validate_json_strict(json);
    assert!(result.is_ok());
}

#[test]
fn test_signature_duplicate_key_attack() {
    let json = r#"{"signature": {"key_id": "legit", "key_id": "evil"}}"#;
    let result = validate_json_strict(json);

    assert!(matches!(
        result,
        Err(StrictJsonError::DuplicateKey { key, path })
        if key == "key_id" && path == "/signature"
    ));
}

// === DoS Protection Tests ===

#[test]
fn test_dos_nesting_depth_limit() {
    let deep_open = "{\"a\":".repeat(65);
    let deep_close = "}".repeat(65);
    let json = format!("{}1{}", deep_open, deep_close);

    let result = validate_json_strict(&json);
    assert!(
        matches!(result, Err(StrictJsonError::NestingTooDeep { depth: 65 })),
        "Expected NestingTooDeep error, got: {:?}",
        result
    );
}

#[test]
fn test_dos_nesting_at_limit_ok() {
    let deep_open = "{\"a\":".repeat(64);
    let deep_close = "}".repeat(64);
    let json = format!("{}1{}", deep_open, deep_close);

    let result = validate_json_strict(&json);
    assert!(result.is_ok(), "64 levels of nesting should be allowed");
}

#[test]
fn test_dos_array_nesting_counts() {
    let deep_open = "[".repeat(65);
    let deep_close = "]".repeat(65);
    let json = format!("{}1{}", deep_open, deep_close);

    let result = validate_json_strict(&json);
    assert!(
        matches!(result, Err(StrictJsonError::NestingTooDeep { .. })),
        "Array nesting should count towards depth limit"
    );
}

// === Edge Cases: Whitespace & Escapes ===

#[test]
fn test_crlf_in_string_accepted() {
    let json = r#"{"key": "line1\r\nline2"}"#;
    let result = validate_json_strict(json);
    assert!(result.is_ok(), "Escaped CRLF in string should be accepted");
}

#[test]
fn test_whitespace_between_tokens() {
    let json = "{ \t\n\r\"key\" \t:\r\n \"value\" \t}";
    let result = validate_json_strict(json);
    assert!(
        result.is_ok(),
        "Whitespace between tokens should be accepted"
    );
}

#[test]
fn test_many_unicode_escapes_in_string() {
    let escapes = "\\u0061".repeat(1000);
    let json = format!(r#"{{"key": "{}"}}"#, escapes);
    let result = validate_json_strict(&json);
    assert!(result.is_ok(), "Many unicode escapes should be handled");
}

#[test]
fn test_string_length_limit_on_decoded_content() {
    let escapes = "\\u0061".repeat(10000);
    let json = format!(r#"{{"key": "{}"}}"#, escapes);
    let result = validate_json_strict(&json);
    assert!(result.is_ok(), "10k escaped chars should be under limit");
}

#[test]
fn test_surrogate_pair_counts_as_one_decoded_char() {
    // \uD83D\uDE00 = 😀; each pair produces one decoded char for limit purposes.
    let pairs = "\\uD83D\\uDE00".repeat(1000);
    let json = format!(r#"{{"key": "{}"}}"#, pairs);
    let result = validate_json_strict(&json);
    assert!(
        result.is_ok(),
        "1000 surrogate pairs (1000 decoded chars) should be under limit"
    );
}

#[test]
fn test_mixed_escapes_in_key() {
    let json = r#"{"a\tb\nc\\d\"e": "value"}"#;
    let result = validate_json_strict(json);
    assert!(result.is_ok(), "Mixed escapes in key should be accepted");
}

#[test]
fn test_all_standard_escapes() {
    let json = r#"{"key": "\"\\/\b\f\n\r\t"}"#;
    let result = validate_json_strict(json);
    assert!(
        result.is_ok(),
        "All standard JSON escapes should be accepted"
    );
}

// === Limit boundary tests ===

#[test]
fn test_string_length_at_limit_ok() {
    // Exactly MAX_STRING_LENGTH decoded chars at boundary (closing quote does not count).
    let value = "a".repeat(MAX_STRING_LENGTH);
    let json = format!(r#"{{"k": "{}"}}"#, value);
    let result = validate_json_strict(&json);
    assert!(
        result.is_ok(),
        "String of exactly MAX_STRING_LENGTH decoded chars should be accepted"
    );
}

#[test]
fn test_string_length_over_limit_rejected() {
    let value = "a".repeat(MAX_STRING_LENGTH + 1);
    let json = format!(r#"{{"k": "{}"}}"#, value);
    let result = validate_json_strict(&json);
    assert!(
        matches!(result, Err(StrictJsonError::StringTooLong { .. })),
        "String exceeding MAX_STRING_LENGTH decoded chars must be rejected, got: {:?}",
        result
    );
}

#[test]
fn test_string_length_at_limit_ok_via_escapes() {
    // Decode path: \u0061 = 'a'; exactly at limit must be accepted.
    let escapes = "\\u0061".repeat(MAX_STRING_LENGTH);
    let json = format!(r#"{{"k": "{}"}}"#, escapes);
    let result = validate_json_strict(&json);
    assert!(
        result.is_ok(),
        "String of exactly MAX_STRING_LENGTH decoded chars (via escapes) should be accepted"
    );
}

#[test]
fn test_string_length_over_limit_rejected_via_escapes() {
    // Decode path: \u0061 = 'a'; over limit must fail. Catches regressions where
    // char_count is incremented per input iteration instead of per output char.
    let escapes = "\\u0061".repeat(MAX_STRING_LENGTH + 1);
    let json = format!(r#"{{"k": "{}"}}"#, escapes);
    let result = validate_json_strict(&json);
    assert!(
        matches!(result, Err(StrictJsonError::StringTooLong { .. })),
        "String exceeding MAX_STRING_LENGTH decoded chars (via escapes) must be rejected, got: {:?}",
        result
    );
}

#[test]
fn test_keys_at_limit_ok() {
    let mut pairs: Vec<String> = Vec::with_capacity(MAX_KEYS_PER_OBJECT);
    for i in 0..MAX_KEYS_PER_OBJECT {
        pairs.push(format!(r#""k{}": {}"#, i, i));
    }
    let json = format!("{{{}}}", pairs.join(","));
    let result = validate_json_strict(&json);
    assert!(
        result.is_ok(),
        "Object with exactly MAX_KEYS_PER_OBJECT keys should be accepted"
    );
}

#[test]
fn test_keys_over_limit_rejected() {
    let count = MAX_KEYS_PER_OBJECT + 1;
    let mut pairs: Vec<String> = Vec::with_capacity(count);
    for i in 0..count {
        pairs.push(format!(r#""k{}": {}"#, i, i));
    }
    let json = format!("{{{}}}", pairs.join(","));
    let result = validate_json_strict(&json);
    assert!(
        matches!(result, Err(StrictJsonError::TooManyKeys { .. })),
        "Object exceeding MAX_KEYS_PER_OBJECT must be rejected, got: {:?}",
        result
    );
}
