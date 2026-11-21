//! The shared reader: what it loads, what it refuses, and how it names a key.
//!
//! Every type here is declared through `closed!`, as a reader's document type would be.

use std::collections::{BTreeMap, BTreeSet};

use assay_common::doc::{load, NameOr, Opaque, Refusal, SeqOrMap};
use assay_common::{closed, closed_scalar};

closed! {
    /// A document with one field of every kind the grammar allows.
    root pub struct Document {
        version: String,
        #[default]
        name: String,
        tools: Option<Tools>,
        #[key = "match"]
        matcher: Option<Vec<Rule>>,
        limits: Option<BTreeMap<String, Limit>>,
        mode: Option<Mode>,
        schema: Option<Opaque<serde_json::Value>>,
        target: Option<NameOr<Target>>,
        steps: Option<SeqOrMap<Vec<Step>, StepTable>>,
        check: Option<Check>,
        chain: Option<Box<Link>>,
        tags: Option<BTreeSet<String>>,
        owner: Option<Handle>,
    }
}

closed! {
    pub struct Tools {
        #[default]
        allow: Vec<String>,
        #[default]
        deny: Vec<String>,
    }
}

closed! {
    pub struct Rule {
        id: String,
        weight: Option<f64>,
    }
}

closed! {
    pub struct Limit {
        max: u64,
    }
}

closed! {
    pub enum Mode {
        "audit" => Audit,
        "enforce" => Enforce,
    }
}

closed! {
    pub struct Target {
        host: String,
        port: Option<u32>,
    }
}

closed! {
    pub struct Step {
        run: String,
    }
}

closed! {
    pub struct StepTable {
        first: Option<Step>,
        last: Option<Step>,
    }
}

closed! {
    pub enum Check tagged "type" {
        "require" => Require(Require),
        "forbid" => Forbid(Forbid),
    }
}

closed! {
    pub struct Require {
        tool: String,
        within: Option<i64>,
    }
}

closed! {
    pub struct Forbid {
        tool: String,
        strict: Option<bool>,
    }
}

closed! {
    pub struct Link {
        label: String,
        next: Option<Box<Link>>,
    }
}

closed_scalar!(
    /// A handle starts with `@`.
    pub Handle,
    "a handle that starts with `@`",
    |text| text.starts_with('@')
);

const FULL: &str = r#"version: "2.0"
name: demo
tools:
  allow: [read]
  deny: [exec]
match:
  - id: first
    weight: 0.5
  - id: second
limits:
  cpu:
    max: 4
mode: enforce
schema:
  type: object
  anything: [goes, {here: true}]
target:
  host: example.test
  port: 443
steps:
  - run: build
  - run: test
check:
  type: require
  tool: read
  within: 3
chain:
  label: a
  next:
    label: b
tags: [x, y]
owner: "@ops"
"#;

fn unknown(text: &str) -> Vec<String> {
    match load::<Document>(text) {
        Err(Refusal::UnknownKeys(keys)) => keys,
        other => panic!("expected unknown keys, got {other:?}"),
    }
}

#[test]
fn a_complete_document_loads_with_every_value_in_place() {
    let doc = load::<Document>(FULL).expect("the full document loads");
    assert_eq!(doc.version, "2.0");
    assert_eq!(doc.name, "demo");
    let tools = doc.tools.expect("tools");
    assert_eq!(tools.allow, ["read"]);
    assert_eq!(tools.deny, ["exec"]);
    let rules = doc.matcher.expect("match");
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].weight, Some(0.5));
    assert_eq!(rules[1].weight, None);
    assert_eq!(doc.limits.expect("limits")["cpu"].max, 4);
    assert_eq!(doc.mode, Some(Mode::Enforce));
    assert_eq!(doc.schema.expect("schema").0["anything"][1]["here"], true);
    assert_eq!(
        doc.target,
        Some(NameOr::Detailed(Target {
            host: "example.test".into(),
            port: Some(443)
        }))
    );
    match doc.steps.expect("steps") {
        SeqOrMap::Seq(steps) => assert_eq!(steps[1].run, "test"),
        SeqOrMap::Map(_) => panic!("a list was written"),
    }
    assert_eq!(
        doc.check,
        Some(Check::Require(Require {
            tool: "read".into(),
            within: Some(3)
        }))
    );
    assert_eq!(doc.chain.expect("chain").next.expect("next").label, "b");
    assert_eq!(doc.tags.expect("tags").len(), 2);
    assert_eq!(doc.owner.expect("owner").as_str(), "@ops");
}

#[test]
fn the_shortest_document_loads_and_absent_parts_are_absent() {
    let doc = load::<Document>("version: \"1\"\n").expect("loads");
    assert_eq!(doc.name, "");
    assert!(doc.tools.is_none());
    assert!(doc.check.is_none());
}

#[test]
fn an_unknown_key_at_the_root_is_refused_by_name() {
    assert_eq!(unknown("version: \"1\"\nzeta: 1\n"), ["zeta"]);
}

#[test]
fn a_misspelled_key_is_refused_and_not_read_as_absent() {
    // `denny` for `deny`: serde's default would drop it and load `deny` as empty.
    assert_eq!(
        unknown("version: \"1\"\ntools:\n  allow: [read]\n  denny: [exec]\n"),
        ["tools.denny"]
    );
}

#[test]
fn every_unknown_key_is_named_with_its_path_in_file_order() {
    let text = r#"version: "2.0"
first_extra: 1
tools:
  allow: [read]
  in_struct: 1
match:
  - id: first
    in_list_item: 1
  - id: second
limits:
  cpu:
    max: 4
    in_map_value: 1
target:
  host: example.test
  in_name_or: 1
steps:
  - run: build
  - run: test
    in_seq_arm: 1
check:
  type: forbid
  tool: read
  in_tagged: 1
chain:
  label: a
  next:
    label: b
    in_box: 1
last_extra: 1
"#;
    assert_eq!(
        unknown(text),
        [
            "first_extra",
            "tools.in_struct",
            "match.0.in_list_item",
            "limits.cpu.in_map_value",
            "target.in_name_or",
            "steps.1.in_seq_arm",
            "check.in_tagged",
            "chain.next.in_box",
            "last_extra",
        ]
    );
}

#[test]
fn the_map_arm_of_a_list_or_map_is_checked_too() {
    assert_eq!(
        unknown("version: \"1\"\nsteps:\n  first:\n    run: a\n    extra: 1\n  middle: 1\n"),
        ["steps.first.extra", "steps.middle"]
    );
}

#[test]
fn a_tagged_part_takes_the_keys_of_its_own_variant_only() {
    // `within` belongs to `require`. Under `forbid` nothing reads it.
    assert_eq!(
        unknown("version: \"1\"\ncheck:\n  type: forbid\n  tool: read\n  within: 3\n"),
        ["check.within"]
    );
    // The tag may come last: the reader does not depend on key order.
    let doc =
        load::<Document>("version: \"1\"\ncheck:\n  tool: read\n  strict: true\n  type: forbid\n")
            .expect("loads");
    assert_eq!(
        doc.check,
        Some(Check::Forbid(Forbid {
            tool: "read".into(),
            strict: Some(true)
        }))
    );
}

#[test]
fn a_key_is_taken_as_the_file_spells_it_not_as_the_field_is_named() {
    assert_eq!(
        unknown("version: \"1\"\nmatcher:\n  - id: a\n"),
        ["matcher"]
    );
}

#[test]
fn an_open_payload_is_not_looked_into() {
    let doc =
        load::<Document>("version: \"1\"\nschema:\n  whatever: {nested: [1, 2]}\n").expect("loads");
    assert_eq!(doc.schema.expect("schema").0["whatever"]["nested"][1], 2);
}

#[test]
fn a_name_stays_a_name() {
    let doc = load::<Document>("version: \"1\"\ntarget: localhost\n").expect("loads");
    assert_eq!(doc.target, Some(NameOr::Name("localhost".into())));
}

#[test]
fn a_merge_key_is_named_because_nothing_applies_it() {
    let text = "version: \"1\"\nbase: &base\n  allow: [read]\ntools:\n  <<: *base\n";
    assert_eq!(unknown(text), ["base", "tools.<<"]);
}

#[test]
fn a_key_that_is_not_text_is_named_by_what_it_says() {
    assert_eq!(unknown("version: \"1\"\n7: seven\n"), ["7"]);
}

#[test]
fn a_string_keeps_the_text_the_file_wrote() {
    // Read through a value tree, `1.0` is a number and no longer a string, and `1.00` loses a
    // digit. The typed read works from the text, as the readers before this one did.
    assert_eq!(
        load::<Document>("version: 1.0\n").expect("loads").version,
        "1.0"
    );
    assert_eq!(
        load::<Document>("version: 1.00\n").expect("loads").version,
        "1.00"
    );
    assert_eq!(
        load::<Document>("version: true\n").expect("loads").version,
        "true"
    );
}

#[test]
fn a_list_where_a_mapping_belongs_is_refused_not_read_by_position() {
    // serde reads a sequence into a struct by position: `[[read], [exec]]` would load as
    // `allow: [read], deny: [exec]` with neither key written.
    match load::<Document>("version: \"1\"\ntools: [[read], [exec]]\n") {
        Err(Refusal::NotAMapping { path, .. }) => assert_eq!(path, "tools"),
        other => panic!("expected a refusal that names `tools`, got {other:?}"),
    }
    match load::<Document>("- \"1\"\n- demo\n") {
        Err(Refusal::NotAMapping { path, .. }) => assert_eq!(path, ""),
        other => panic!("expected a refusal at the root, got {other:?}"),
    }
}

#[test]
fn text_that_is_not_yaml_is_refused_with_a_line() {
    match load::<Document>("version: \"1\"\ntools: [unclosed\n") {
        Err(Refusal::Syntax { line, .. }) => assert!(line.is_some()),
        other => panic!("expected a syntax refusal, got {other:?}"),
    }
}

#[test]
fn a_key_written_twice_is_refused() {
    match load::<Document>("version: \"1\"\nname: a\nname: b\n") {
        Err(Refusal::Syntax { message, .. }) => {
            assert!(message.contains("duplicate"), "{message}")
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_value_of_the_wrong_kind_is_refused_with_its_place() {
    match load::<Document>("version: \"1\"\ntools:\n  allow: read\n") {
        Err(Refusal::Shape { line, message, .. }) => {
            assert_eq!(line, Some(3));
            assert!(message.contains("tools.allow"), "{message}");
        }
        other => panic!("expected a shape refusal, got {other:?}"),
    }
}

#[test]
fn a_missing_key_is_refused() {
    match load::<Document>("name: demo\n") {
        Err(Refusal::Shape { message, .. }) => {
            assert!(message.contains("missing field `version`"), "{message}")
        }
        other => panic!("expected a shape refusal, got {other:?}"),
    }
}

#[test]
fn an_empty_file_is_refused() {
    assert!(matches!(load::<Document>(""), Err(Refusal::Shape { .. })));
    assert!(matches!(
        load::<Document>("# only a comment\n"),
        Err(Refusal::Shape { .. })
    ));
}

#[test]
fn an_enum_takes_the_spelling_in_its_declaration_and_no_other() {
    assert_eq!(
        load::<Document>("version: \"1\"\nmode: audit\n")
            .expect("loads")
            .mode,
        Some(Mode::Audit)
    );
    assert!(matches!(
        load::<Document>("version: \"1\"\nmode: Audit\n"),
        Err(Refusal::Shape { .. })
    ));
    assert!(matches!(
        load::<Document>("version: \"1\"\ncheck:\n  type: allow\n  tool: read\n"),
        Err(Refusal::Shape { .. })
    ));
}

#[test]
fn a_scalar_with_a_rule_is_checked_when_the_file_loads() {
    match load::<Document>("version: \"1\"\nowner: ops\n") {
        Err(Refusal::Shape { message, .. }) => {
            assert!(
                message.contains("a handle that starts with `@`"),
                "{message}"
            )
        }
        other => panic!("expected a shape refusal, got {other:?}"),
    }
}

#[test]
fn unknown_keys_are_reported_before_a_value_of_the_wrong_kind() {
    // Both are wrong. The key is the finding a reader must not lose behind the other.
    assert_eq!(
        unknown("version: \"1\"\nzeta: 1\ntools:\n  allow: read\n"),
        ["zeta"]
    );
}

#[test]
fn a_list_where_a_mapping_belongs_is_reported_before_an_unknown_key() {
    assert!(matches!(
        load::<Document>("version: \"1\"\nzeta: 1\ntools: [[read], [exec]]\n"),
        Err(Refusal::NotAMapping { .. })
    ));
}

#[test]
fn a_tag_key_is_not_an_unknown_key_and_a_yaml_tag_does_not_hide_one() {
    assert!(load::<Document>("version: \"1\"\ncheck:\n  type: require\n  tool: read\n").is_ok());
    assert_eq!(
        unknown("version: \"1\"\ntools: !custom\n  allow: [read]\n  extra: 1\n"),
        ["tools.extra"]
    );
}

#[test]
fn the_first_list_where_a_mapping_belongs_is_the_one_reported() {
    match load::<Document>("version: \"1\"\ntools: [[read], [exec]]\nchain: [a, b]\n") {
        Err(Refusal::NotAMapping { path, expected }) => {
            assert_eq!(path, "tools");
            assert_eq!(expected, "Tools");
        }
        other => panic!("expected a refusal that names `tools`, got {other:?}"),
    }
}

#[test]
fn a_key_that_differs_only_in_case_is_unknown() {
    // serde matches a key exactly. A walk that matched `Allow` to the field `allow` would call the
    // key known while the typed read drops it, and `allow` would load as empty.
    assert_eq!(
        unknown("version: \"1\"\ntools:\n  Allow: [read]\n"),
        ["tools.Allow"]
    );
    assert_eq!(unknown("Version: \"1\"\nversion: \"1\"\n"), ["Version"]);
    // Nor is a key trimmed: a quoted key with a space in front is another key.
    assert_eq!(
        unknown("version: \"1\"\ntools:\n  \" allow\": [read]\n"),
        ["tools. allow"]
    );
    // The same for the tag: `Type` is not the tag key, so the part has no tag and is refused.
    assert!(matches!(
        load::<Document>("version: \"1\"\ncheck:\n  Type: require\n  tool: read\n"),
        Err(Refusal::Shape { .. })
    ));
    // Beside the real tag key, `Type` is one more key nothing reads.
    assert_eq!(
        unknown("version: \"1\"\ncheck:\n  type: require\n  Type: forbid\n  tool: read\n"),
        ["check.Type"]
    );
    // And for the tag's value: `Require` names no variant, so the walk selects none. The typed
    // read refuses the part, and `strict`, which only `forbid` takes, is not what gets reported.
    assert!(matches!(
        load::<Document>("version: \"1\"\ncheck:\n  type: Require\n  tool: read\n  strict: true\n"),
        Err(Refusal::Shape { .. })
    ));
}

#[test]
fn a_field_default_is_used_only_for_an_absent_key() {
    fn default_limit() -> u32 {
        2048
    }

    closed! {
        root struct Limits {
            #[key = "pids_max"]
            #[default = "default_limit"]
            limit: u32,
        }
    }

    assert_eq!(load::<Limits>("{}").unwrap().limit, 2048);
    assert_eq!(load::<Limits>("pids_max: 0").unwrap().limit, 0);
    assert_eq!(load::<Limits>("pids_max: 64").unwrap().limit, 64);
    assert!(matches!(
        load::<Limits>("pids_max: null"),
        Err(Refusal::Shape { .. })
    ));
    assert!(
        matches!(load::<Limits>("limit: 64"), Err(Refusal::UnknownKeys(keys)) if keys == ["limit"])
    );
}

#[test]
fn policy_strings_preserve_yaml_types_in_fields_lists_and_map_keys() {
    closed! {
        root(yaml_types) struct PolicyStrings {
            version: String,
            tools: Vec<String>,
            schemas: BTreeMap<String, Opaque<serde_json::Value>>,
        }
    }
    let valid = "version: '2.0'\ntools: ['true']\nschemas: {'7': {minimum: 3}}";
    let doc = load::<PolicyStrings>(valid).unwrap();
    assert_eq!(doc.version, "2.0");
    assert_eq!(doc.tools, ["true"]);
    assert_eq!(doc.schemas["7"].0["minimum"], 3);
    for quoted in ["'2.0'", "'true'", "'7'"] {
        let invalid = valid.replace(quoted, quoted.trim_matches('\''));
        assert!(
            matches!(load::<PolicyStrings>(&invalid), Err(Refusal::Shape { .. })),
            "{invalid}"
        );
    }
}

#[test]
fn only_an_opted_in_root_accepts_extension_keys_for_yaml_anchors() {
    closed! { root(yaml_types, x_extensions) struct WithExtensions { limit: Limit } }
    closed! { root(yaml_types) struct Plain { limit: Limit } }
    let file = "x-template: &value 3\nlimit: {max: *value}";
    assert_eq!(load::<WithExtensions>(file).unwrap().limit.max, 3);
    assert_eq!(
        load::<WithExtensions>(&format!("!policy\n{file}"))
            .unwrap()
            .limit
            .max,
        3
    );
    assert_eq!(
        load::<Plain>(file).unwrap_err(),
        Refusal::UnknownKeys(vec!["x-template".into()])
    );
    for (yaml, path) in [
        ("limit: {max: 3, x-template: 4}", "limit.x-template"),
        ("x_template: 4\nlimit: {max: 3}", "x_template"),
    ] {
        assert_eq!(
            load::<WithExtensions>(yaml).unwrap_err(),
            Refusal::UnknownKeys(vec![path.into()])
        );
    }
}

#[test]
fn args_schema_keys_keep_their_spelling_and_refuse_logical_duplicates() {
    use assay_common::doc::JsonInput;
    let doc = JsonInput::parse("version: '2'\nschema: {0x10: true, 16: false}")
        .unwrap()
        .read::<Document>()
        .unwrap();
    assert_eq!(
        doc.schema.unwrap().0,
        serde_json::json!({"0x10": true, "16": false})
    );
    let refused =
        JsonInput::parse("version: '2'\nschema: {allOf: [{path: true, !key path: false}]}");
    assert!(refused.is_err(), "duplicate schema property was accepted");
}

#[test]
fn projected_document_reports_unknown_fields_in_authored_order() {
    let input =
        assay_common::doc::JsonInput::parse("version: '2'\ntools: {z: true, a: false}").unwrap();
    assert_eq!(
        input.read::<Document>(),
        Err(Refusal::UnknownKeys(vec![
            "tools.z".into(),
            "tools.a".into()
        ]))
    );
}

#[test]
fn yaml_string_key_checks_decode_typed_strings_from_the_source() {
    closed! { root(yaml_types) struct Text { value: String } }
    for spelling in [
        "012",
        "18446744073709551616",
        "-9223372036854775809",
        "!custom 001",
    ] {
        let input = format!("value: {spelling}");
        let doc = assay_common::doc::load_with_string_keys::<Text>(&input).unwrap();
        assert_eq!(doc.value, spelling.trim_start_matches("!custom "));
    }
}

#[test]
fn json_projection_stays_strict_about_yaml_tags_and_out_of_range_integers() {
    for value in [
        "!custom 001",
        "18446744073709551616",
        "-9223372036854775809",
    ] {
        assert!(assay_common::doc::JsonInput::parse(&format!("value: {value}")).is_err());
    }
}

#[test]
fn boxed_positional_details_report_the_nested_field_path() {
    use assay_common::doc::LegacyNameOr;
    closed! { legacy_positional struct Positional { limit: Limit } }
    closed! { root struct BoxedDetail { detail: LegacyNameOr<Box<Positional>> } }

    assert!(load::<BoxedDetail>("detail: [{max: 3}]").is_ok());
    assert!(matches!(
        load::<BoxedDetail>("detail: [{max: 3, typo: 4}]"),
        Err(Refusal::UnknownKeys(keys)) if keys == ["detail.0.typo"]
    ));
}

#[test]
fn strict_part_refuses_unread_fields_during_typed_yaml_decode() {
    closed! { strict struct Part { minimum: String } }
    let value: Part = serde_yaml::from_str("minimum: 18446744073709551616").unwrap();
    assert_eq!(value.minimum, "18446744073709551616");
    let error = serde_yaml::from_str::<Part>("minimum: 1\nthreshold: 2").unwrap_err();
    assert!(error.to_string().contains("unknown field `threshold`"));
}

#[test]
fn positional_tagged_variants_check_nested_fields_at_their_source_indices() {
    closed! { #[derive(serde::Serialize)] struct Part { value: String } }
    closed! { enum Mode tagged "type" { "known" => Known { label: String, part: Part } } }
    closed! { root struct Record { mode: Mode } }

    let read = |members: &str| load::<Record>(&format!("mode: [known, note, {members}]"));
    assert_eq!(
        read("{value: example}").unwrap(),
        read("[example]").unwrap()
    );
    assert_eq!(read("{0: example}").unwrap(), read("[example]").unwrap());
    assert!(read("{'0': example}").is_err());
    assert!(matches!(
        read("{value: example, extra: 1}"),
        Err(Refusal::UnknownKeys(keys)) if keys == ["mode.2.extra"]
    ));
}

#[test]
fn positional_variant_indices_select_the_nested_declaration() {
    closed! { #[derive(serde::Serialize)] struct Part { value: String } }
    closed! { enum Inner tagged "type" { "known" => Known { part: Part } } }
    closed! { enum Outer tagged "type" { "outer" => Outer { inner: Inner } } }
    closed! { root struct Doc { mode: Outer } }

    for inner in [
        "[0, {value: x, extra: y}]",
        "{type: 0, part: {value: x, extra: y}}",
    ] {
        let source = format!("mode: [outer, {inner}]");
        let expected = if inner.starts_with('[') {
            "mode.1.1.extra"
        } else {
            "mode.1.part.extra"
        };
        assert!(
            matches!(load::<Doc>(&source), Err(Refusal::UnknownKeys(keys)) if keys == [expected])
        );
    }
}

#[test]
fn positional_fields_keep_their_identity_across_reader_projections() {
    use assay_common::doc::{load_with_string_keys, Input};
    closed! { #[derive(serde::Serialize)] struct Named { #[key = "7"] value: String } }
    closed! { enum Mode tagged "type" { "known" => Known { part: Named } } }
    closed! { root struct Record { mode: Mode } }

    let json = Input::parse("mode: [known, {7: example}]")
        .unwrap()
        .read_json_projection::<Record>()
        .unwrap();
    let yaml = load_with_string_keys::<Record>("mode: [known, {0: example}]").unwrap();
    assert_eq!(json, yaml);
}

#[test]
fn nested_variant_index_in_a_mapping_checks_the_selected_payload() {
    closed! { #[derive(serde::Serialize)] struct Part { value: String } }
    closed! { enum Inner tagged "type" { "first" => First, "known" => Known { part: Part } } }
    closed! { enum Outer tagged "type" { "outer" => Outer { inner: Inner } } }
    closed! { root struct Doc { mode: Outer } }

    let read = |tag: &str, part: &str| {
        load::<Doc>(&format!(
            "mode: {{type: outer, inner: {{type: {tag}, part: {part}}}}}"
        ))
    };
    assert_eq!(
        read("1", "{value: x}").unwrap(),
        read("known", "{value: x}").unwrap()
    );
    assert!(read("'1'", "{value: x}").is_err());
    for tag in ["known", "1"] {
        let error = read(tag, "{value: x, extra: y}").unwrap_err();
        assert_eq!(
            error,
            Refusal::UnknownKeys(vec!["mode.inner.part.extra".into()])
        );
    }
    assert!(matches!(
        read("known", "[x]"),
        Err(Refusal::NotAMapping { .. })
    ));
}

#[test]
fn tagged_future_payload_is_open_only_for_unknown_variants() {
    closed! {
        strict enum CheckKind tagged "type" {
            "count" => Count { min: usize };
            future "unsupported" => Unsupported
        }
    }
    closed! { strict root struct Document { check: CheckKind } }

    for kind in ["new_kind", "unsupported"] {
        let yaml = format!("check: {{type: {kind}, payload: {{[a,b]: yes}}}}");
        assert_eq!(
            load::<Document>(&yaml).unwrap().check,
            CheckKind::Unsupported
        );
    }
    // The typed decoder sees the original discriminator spelling before any tree projection.
    for yaml in [
        "{type: 18446744073709551616, payload: {[a,b]: yes}}",
        "{[a,b]: value, name: &kind future, type: *kind}",
    ] {
        let document = format!("check: {yaml}");
        assert_eq!(
            assay_common::doc::load_strict::<Document>(&document)
                .unwrap()
                .check,
            CheckKind::Unsupported
        );
        assert_eq!(
            serde_yaml::from_str::<CheckKind>(yaml).unwrap(),
            CheckKind::Unsupported
        );
    }
    let known = "check: {type: count, min: 1, unused: true}";
    assert!(matches!(
        load::<Document>(known),
        Err(Refusal::UnknownKeys(_))
    ));
    assert!(matches!(
        assay_common::doc::load_strict::<Document>(known),
        Err(Refusal::Shape { message, .. }) if message.contains("unknown field `unused`")
    ));
    let duplicate = "check: {type: future, payload: {[a,b]: 1, [a,b]: 2}}";
    assert!(
        matches!(assay_common::doc::load_strict::<Document>(duplicate),
        Err(Refusal::Shape { message, .. }) if message.contains("duplicate"))
    );
    closed! { strict root struct Header { author: String } }
    for number in ["18446744073709551616", "-9223372036854775809"] {
        let yaml = format!("author: {number}");
        assert_eq!(
            assay_common::doc::load_strict::<Header>(&yaml)
                .unwrap()
                .author,
            number
        );
        assert!(assay_common::doc::Input::parse(&yaml).is_err());
    }
    let repeated = r#"{"type":"new_kind","config":{"x":1,"x":2}}"#;
    assert!(serde_json::from_str::<CheckKind>(repeated)
        .unwrap_err()
        .to_string()
        .contains("duplicate"));
    assert_eq!(
        serde_json::to_value(CheckKind::Unsupported).unwrap(),
        serde_json::json!({"type":"unsupported"})
    );
}

#[test]
fn numeric_variant_name_keeps_its_nested_fields_closed() {
    closed! { #[derive(serde::Serialize)] strict part struct Inner { kept: String } }
    closed! { strict enum Mode tagged "type" { "42" => Known { nested: Inner }; future "unsupported" => Unsupported } }
    closed! { strict root struct Envelope { mode: Mode } }
    let read = assay_common::doc::load_strict::<Envelope>;
    let valid = read("mode: {type: 42, nested: {kept: yes}}").unwrap();
    assert!(matches!(valid.mode, Mode::Known { nested: Inner { kept } } if kept == "yes"));
    assert!(matches!(
        read("mode: {type: 42, nested: {kept: yes, extra: wrong}}"),
        Err(Refusal::Shape { message, .. }) if message.contains("unknown field `extra`")
    ));
}

#[test]
fn strict_field_checks_remain_local_without_a_strict_root() {
    closed! { struct Inner { label: String } }
    closed! { strict struct Outer { inner: Inner } }
    let outer: Outer = serde_yaml::from_str("inner: {label: hello, extra: 1}").unwrap();
    assert_eq!(outer.inner.label, "hello");
    assert!(serde_yaml::from_str::<Outer>("inner: {label: hello}\nextra: 1").is_err());
}

#[test]
fn tagged_enum_accepts_recursive_self_fields() {
    closed! { enum Node tagged "type" {
        "group" => Group { children: Vec<Self> },
    } }
    closed! { root struct Tree { node: Node } }

    let tree = load::<Tree>("node: {type: group, children: []}").unwrap();
    assert_eq!(tree.node, Node::Group { children: vec![] });
}

#[test]
fn renamed_fields_keep_one_slot_and_report_the_authored_alias() {
    closed! { #[derive(serde::Serialize)] struct Detail { amount: u32 } }
    closed! {
        #[derive(serde::Serialize)]
        root struct Header {
            #[key = "configVersion", alias = "version"]
            version: Detail,
            tail: String,
        }
    }
    let canonical = load::<Header>("configVersion: {amount: 1}\ntail: ok").unwrap();
    let aliased = load::<Header>("version: {amount: 1}\ntail: ok").unwrap();
    assert_eq!(aliased, canonical);
    assert_eq!(
        serde_json::to_value(aliased).unwrap(),
        serde_json::json!({"configVersion":{"amount":1},"tail":"ok"})
    );
    for spelling in ["configVersion", "version"] {
        let text = format!("{spelling}: {{amount: 1, extra: 2}}\ntail: ok");
        assert!(
            matches!(load::<Header>(&text), Err(Refusal::UnknownKeys(keys)) if keys == [format!("{spelling}.extra")])
        );
    }
    let duplicate = "version: {amount: 1}\nconfigVersion: {amount: 2}\ntail: ok";
    assert!(
        matches!(load::<Header>(duplicate), Err(Refusal::Shape { message, .. }) if message.contains("duplicate field"))
    );
    let positional: Header = serde_json::from_str(r#"[{"amount":1},"ok"]"#).unwrap();
    assert_eq!(positional, canonical);
}

#[test]
fn buffered_numeric_field_indices_are_not_alias_names() {
    closed! { #[derive(serde::Serialize)] struct Detail { value: String } }
    closed! { #[derive(serde::Serialize)] struct Part { #[key = "detail", alias = "7"] detail: Detail } }
    closed! { enum Mode tagged "type" { "known" => Known { part: Part } } }
    closed! { root struct Record { mode: Mode } }
    let read = |key| load::<Record>(&format!("mode: [known, {{{key}: {{value: x}}}}]"));
    assert_eq!(read("0").unwrap(), read("'7'").unwrap());
    assert!(matches!(read("7"), Err(Refusal::UnknownKeys(keys)) if keys == ["mode.1.7"]));
}

#[test]
#[allow(unreachable_patterns)]
fn overlapping_field_names_follow_the_typed_readers_declaration_order() {
    closed! { struct First { amount: u32 } }
    closed! { struct Second { name: String } }
    closed! { root struct Record {
        #[key = "first", alias = "second"]
        first: Option<First>,
        second: Option<Second>,
    } }
    let text = "second: {amount: 3}";
    let direct = serde_yaml::from_str::<Record>(text).unwrap();
    assert_eq!(direct.first.as_ref().unwrap().amount, 3);
    assert_eq!(load::<Record>(text).unwrap(), direct);
}

#[test]
fn tagged_variant_fields_take_the_struct_alias_rule() {
    closed! { #[derive(serde::Serialize)] struct Detail { amount: u32 } }
    closed! {
        enum Check tagged "type" {
            "score" => Score {
                #[key = "detail", alias = "text"]
                detail: Detail,
                #[default]
                #[key = "min_score", alias = "threshold"]
                min_score: u32,
            },
        }
    }
    closed! { root struct Suite { checks: Vec<Check> } }
    let read = |check: &str| load::<Suite>(&format!("checks: [{check}]")).map(|suite| suite.checks);

    let canonical = read("{type: score, detail: {amount: 1}, min_score: 2}").unwrap();
    assert_eq!(
        read("{type: score, text: {amount: 1}, threshold: 2}").unwrap(),
        canonical
    );
    assert_eq!(
        read("{type: score, text: {amount: 1}, min_score: 2}").unwrap(),
        canonical
    );
    assert_eq!(
        serde_yaml::from_str::<Check>("{type: score, text: {amount: 1}, threshold: 2}").unwrap(),
        canonical[0]
    );
    // Either spelling writes the canonical key.
    assert_eq!(
        serde_json::to_string(&read("{type: score, text: {amount: 1}, threshold: 2}").unwrap())
            .unwrap(),
        r#"[{"type":"score","detail":{"amount":1},"min_score":2}]"#
    );
    assert_eq!(
        read("{type: score, text: {amount: 1}}").unwrap(),
        [Check::Score {
            detail: Detail { amount: 1 },
            min_score: 0
        }]
    );

    // A key below an aliased field is named under the spelling the file used.
    for spelling in ["detail", "text"] {
        assert!(
            matches!(read(&format!("{{type: score, {spelling}: {{amount: 1, extra: 2}}}}")), Err(Refusal::UnknownKeys(keys)) if keys == [format!("checks.0.{spelling}.extra")])
        );
    }
    assert!(
        matches!(read("{type: score, text: {amount: 1}, extra: 2}"), Err(Refusal::UnknownKeys(keys)) if keys == ["checks.0.extra"])
    );
    for duplicate in [
        "{type: score, text: {amount: 1}, detail: {amount: 2}}",
        "{type: score, detail: {amount: 1}, threshold: 1, min_score: 2}",
    ] {
        assert!(
            matches!(read(duplicate), Err(Refusal::Shape { message, .. }) if message.contains("duplicate field")),
            "{duplicate}"
        );
        assert!(
            serde_yaml::from_str::<Check>(duplicate).is_err(),
            "{duplicate}"
        );
    }

    closed! {
        strict enum Strict tagged "type" {
            "label" => Label {
                #[key = "label", alias = "text"]
                label: String,
            },
        }
    }
    closed! { root struct Labels { label: Strict } }
    assert_eq!(
        load::<Labels>("label: {type: label, text: hi}")
            .unwrap()
            .label,
        Strict::Label {
            label: "hi".to_string()
        }
    );
    assert!(
        matches!(load::<Labels>("label: {type: label, text: hi, label: ho}"), Err(Refusal::Shape { message, .. }) if message.contains("duplicate field"))
    );
}

#[test]
fn required_json_on_a_variant_distinguishes_missing_from_null() {
    closed! { enum Check tagged "type" {
        "judge" => Judge { #[native_json = "criteria"] criteria: serde_json::Value }
    } }
    closed! { root struct Suite { check: Check } }
    for value in [
        serde_json::Value::Null,
        serde_json::json!({"unknown": [1, false]}),
    ] {
        let input = serde_json::json!({"check": {"type":"judge", "criteria": value}});
        let check = load::<Suite>(&input.to_string()).unwrap().check;
        assert_eq!(serde_json::to_value(check).unwrap(), input["check"]);
    }
    let missing = r#"{"type":"judge"}"#;
    assert!(serde_json::from_str::<Check>(missing)
        .unwrap_err()
        .to_string()
        .contains("missing field"));
    assert!(load::<Suite>(r#"{"check":{"type":"judge"}}"#).is_err());
    // Direct Serde preserves Value's decoding; the document reader checks the authored source.
    let duplicate = r#"{"type":"judge","criteria":{"x":1,"x":2}}"#;
    let direct: Check = serde_json::from_str(duplicate).unwrap();
    assert_eq!(
        serde_json::to_value(direct).unwrap()["criteria"],
        serde_json::json!({"x":2})
    );
    assert!(load::<Suite>(&format!(r#"{{"check":{duplicate}}}"#)).is_err());
}
