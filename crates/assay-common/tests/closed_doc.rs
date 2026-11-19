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
