use super::*;
#[test]
fn g4_n1_non_allowlisted_attributes_only_yields_unknown_kind() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_happy_agent_capabilities.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2.0"),
    };
    let batch = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("convert");
    assert_discovery_v1_defaults(&batch.events[0].payload);
}

#[test]
fn g4_n2_assay_g4_wrong_shape_does_not_promote() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let mut packet: Value =
        serde_json::from_slice(&fixture("a2a_happy_agent_capabilities.json")).unwrap();
    packet
        .as_object_mut()
        .unwrap()
        .get_mut("attributes")
        .unwrap()
        .as_object_mut()
        .unwrap()
        .insert("assay_g4".to_string(), Value::String("bad".to_string()));
    let payload = serde_json::to_vec(&packet).unwrap();
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2.0"),
    };
    let batch = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("convert");
    assert_discovery_v1_defaults(&batch.events[0].payload);
}

#[test]
fn g4_n3_unmapped_top_level_fields_alone_do_not_affect_discovery() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let mut packet: Value =
        serde_json::from_slice(&fixture("a2a_happy_agent_capabilities.json")).unwrap();
    packet
        .as_object_mut()
        .unwrap()
        .insert("extra_top_level".to_string(), Value::Number(1.into()));
    let payload = serde_json::to_vec(&packet).unwrap();
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2.0"),
    };
    let batch = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("convert");
    assert!(batch.lossiness.unmapped_fields_count >= 1);
    assert_discovery_v1_defaults(&batch.events[0].payload);
}

#[test]
fn g4_attributes_driven_agent_card_sets_kind_attributes() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_happy_agent_capabilities_g4_agent_card.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2.0"),
    };
    let batch = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("convert");
    let d = &batch.events[0].payload["discovery"];
    assert_eq!(d["agent_card_visible"], Value::Bool(true));
    assert_eq!(
        d["agent_card_source_kind"],
        Value::String("attributes".to_string())
    );
    assert_eq!(d["extended_card_access_visible"], Value::Bool(false));
    assert_eq!(d["signature_material_visible"], Value::Bool(false));
    assert_eq!(
        digest_canonical_json(d),
        G4_DISCOVERY_DIGEST_AGENT_CARD_ATTR
    );
}

/// When `discovery` is non-default, full `AdapterBatch` digests must still be byte-stable across
/// repeated conversion (not only `payload["discovery"]` golden hashes).
#[test]
fn g4_non_default_discovery_full_batch_digest_is_deterministic() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_happy_agent_capabilities_g4_agent_card.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2.0"),
    };
    let first = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("convert");
    let second = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("convert");
    assert_eq!(
        digest_canonical_json(&first),
        digest_canonical_json(&second)
    );
}

#[test]
fn g4_both_visibility_flags_true_fixture_shows_independence() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_happy_agent_capabilities_g4_both_visible.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2.0"),
    };
    let batch = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("convert");
    let d = &batch.events[0].payload["discovery"];
    assert_eq!(d["agent_card_visible"], Value::Bool(true));
    assert_eq!(
        d["agent_card_source_kind"],
        Value::String("attributes".to_string())
    );
    assert_eq!(d["extended_card_access_visible"], Value::Bool(true));
    assert_eq!(d["signature_material_visible"], Value::Bool(false));
    assert_eq!(digest_canonical_json(d), G4_DISCOVERY_DIGEST_BOTH_FLAGS);
}

#[test]
fn g4_extended_access_visible_positive_fixture() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_happy_agent_capabilities_g4_extended.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2.0"),
    };
    let batch = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("convert");
    let d = &batch.events[0].payload["discovery"];
    assert_eq!(d["agent_card_visible"], Value::Bool(false));
    assert_eq!(
        d["agent_card_source_kind"],
        Value::String("unknown".to_string())
    );
    assert_eq!(d["extended_card_access_visible"], Value::Bool(true));
    assert_eq!(d["signature_material_visible"], Value::Bool(false));
    assert_eq!(digest_canonical_json(d), G4_DISCOVERY_DIGEST_EXTENDED_ONLY);
}

#[test]
fn g4_n5_strict_and_lenient_same_discovery_without_assay_g4() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let payload = fixture("a2a_happy_task_requested.json");
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2"),
    };
    let strict = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("strict");
    let lenient = adapter
        .convert(
            input,
            &ConvertOptions {
                mode: ConvertMode::Lenient,
                max_payload_bytes: Some(8_192),
                max_json_depth: None,
                max_array_length: None,
            },
            &writer,
        )
        .expect("lenient");
    assert_eq!(
        strict.events[0].payload["discovery"],
        lenient.events[0].payload["discovery"]
    );
    assert_discovery_v1_defaults(&strict.events[0].payload);
}

#[test]
fn g4_missing_agent_card_object_does_not_promote() {
    let adapter = A2aAdapter;
    let writer = TestWriter;
    let mut packet: Value =
        serde_json::from_slice(&fixture("a2a_happy_agent_capabilities.json")).unwrap();
    packet
        .as_object_mut()
        .unwrap()
        .get_mut("attributes")
        .unwrap()
        .as_object_mut()
        .unwrap()
        .insert(
            "assay_g4".to_string(),
            serde_json::json!({ "priority": "nested" }),
        );
    let payload = serde_json::to_vec(&packet).unwrap();
    let input = AdapterInput {
        payload: &payload,
        media_type: "application/json",
        protocol_version: Some("0.2.0"),
    };
    let batch = adapter
        .convert(input, &ConvertOptions::default(), &writer)
        .expect("convert");
    assert_eq!(
        batch.events[0].payload["discovery"]["agent_card_visible"],
        false
    );
    assert_eq!(
        batch.events[0].payload["discovery"]["agent_card_source_kind"],
        Value::String("unknown".to_string())
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    #[test]
    fn strict_unknown_top_level_fields_account_for_lossiness(
        extras in proptest::collection::btree_map("[a-z_]{1,12}", "[a-z0-9_-]{0,12}", 1..5)
    ) {
        let mut packet: Value = serde_json::from_slice(&fixture("a2a_happy_task_requested.json")).unwrap();
        let object = packet.as_object_mut().unwrap();
        let mut inserted = 0u32;

        for (key, value) in extras {
            prop_assume!(!reserved_key(&key));
            object.insert(key, Value::String(value));
            inserted += 1;
        }

        let payload = serde_json::to_vec(&packet).unwrap();
        let adapter = A2aAdapter;
        let writer = TestWriter;
        let batch = adapter.convert(
            AdapterInput {
                payload: &payload,
                media_type: "application/json",
                protocol_version: Some("0.2.0"),
            },
            &ConvertOptions::default(),
            &writer,
        ).unwrap();

        prop_assert!(batch.lossiness.unmapped_fields_count >= inserted);
    }
}
