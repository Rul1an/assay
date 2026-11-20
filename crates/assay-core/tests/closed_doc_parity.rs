//! The shared reader and `unread_keys::from_yaml_str` are two readers of one rule: a key no part
//! of the type takes is named by its path as the file spells it. They are built differently
//! (a walk along a description of the type; serde_ignored around the typed read), so they are
//! compared here over a generated corpus instead of being assumed to agree.
//!
//! The corpus is one full document with an unknown key planted at every mapping, one at a time
//! and then two at a time.

use std::collections::BTreeMap;

use assay_common::closed;
use assay_common::doc::{load, ClosedDoc, NameOr, Opaque, Refusal, SeqOrMap};
use assay_core::unread_keys::from_yaml_str;
use serde_yaml::{Mapping, Value};

closed! {
    root pub struct Document {
        version: String,
        tools: Option<Tools>,
        #[key = "match"]
        matcher: Option<Vec<Rule>>,
        limits: Option<BTreeMap<String, Limit>>,
        schema: Option<Opaque<serde_json::Value>>,
        target: Option<NameOr<Target>>,
        steps: Option<SeqOrMap<Vec<Step>, StepTable>>,
        table: Option<SeqOrMap<Vec<Step>, StepTable>>,
        chain: Option<Box<Link>>,
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
        when: Option<Target>,
    }
}
closed! {
    pub struct Limit {
        max: u64,
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
    pub struct Link {
        label: String,
        next: Option<Box<Link>>,
    }
}

const FULL: &str = r#"version: "2.0"
tools:
  allow: [read]
  deny: [exec]
match:
  - id: first
    when:
      host: a.test
  - id: second
limits:
  cpu:
    max: 4
  memory:
    max: 8
schema:
  type: object
  properties:
    nested: {deep: true}
target:
  host: example.test
  port: 443
steps:
  - run: build
  - run: test
table:
  first:
    run: a
  last:
    run: z
chain:
  label: a
  next:
    label: b
    next:
      label: c
"#;

/// What a reader said about one text.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Loaded,
    Unknown(Vec<String>),
    Refused,
}

fn shared<T: ClosedDoc>(text: &str) -> Outcome {
    match load::<T>(text) {
        Ok(_) => Outcome::Loaded,
        Err(Refusal::UnknownKeys(keys)) => Outcome::Unknown(keys),
        Err(_) => Outcome::Refused,
    }
}

fn through_serde_ignored<T: serde::de::DeserializeOwned>(text: &str) -> Outcome {
    match from_yaml_str::<T>(text) {
        Ok((_, unread)) if unread.is_empty() => Outcome::Loaded,
        Ok((_, unread)) => Outcome::Unknown(unread),
        Err(_) => Outcome::Refused,
    }
}

/// The address of every mapping in the tree, root first, in file order.
fn mappings(node: &Value, at: &mut Vec<Value>, found: &mut Vec<Vec<Value>>) {
    match node {
        Value::Mapping(entries) => {
            found.push(at.clone());
            for (key, child) in entries {
                at.push(key.clone());
                mappings(child, at, found);
                at.pop();
            }
        }
        Value::Sequence(items) => {
            for (index, child) in items.iter().enumerate() {
                at.push(Value::Number(index.into()));
                mappings(child, at, found);
                at.pop();
            }
        }
        _ => {}
    }
}

fn mapping_at<'a>(root: &'a mut Value, address: &[Value]) -> &'a mut Mapping {
    let mut node = root;
    for step in address {
        node = match node {
            Value::Mapping(entries) => entries.get_mut(step).expect("the key is there"),
            Value::Sequence(items) => {
                let index = step.as_u64().expect("an index") as usize;
                &mut items[index]
            }
            _ => panic!("the address leads through a scalar"),
        };
    }
    node.as_mapping_mut().expect("the address names a mapping")
}

fn planted(base: &Value, plants: &[(&[Value], &str)]) -> String {
    let mut tree = base.clone();
    for (address, key) in plants {
        mapping_at(&mut tree, address).insert(Value::String((*key).to_string()), Value::from(1));
    }
    serde_yaml::to_string(&tree).expect("the tree serializes")
}

fn addresses() -> (Value, Vec<Vec<Value>>) {
    let base: Value = serde_yaml::from_str(FULL).expect("the base is YAML");
    let mut found = Vec::new();
    mappings(&base, &mut Vec::new(), &mut found);
    (base, found)
}

#[test]
fn the_base_document_loads_in_both_readers() {
    assert_eq!(shared::<Document>(FULL), Outcome::Loaded);
    assert_eq!(through_serde_ignored::<Document>(FULL), Outcome::Loaded);
}

#[test]
fn one_unknown_key_at_every_mapping_gets_the_same_answer_from_both_readers() {
    let (base, addresses) = addresses();
    let (mut named, mut open, mut refused) = (0, 0, 0);
    for address in &addresses {
        let text = planted(&base, &[(address, "planted")]);
        let ours = shared::<Document>(&text);
        let theirs = through_serde_ignored::<Document>(&text);
        assert_eq!(ours, theirs, "the readers disagree on:\n{text}");
        match ours {
            Outcome::Unknown(keys) => {
                assert_eq!(keys.len(), 1, "one key was planted in:\n{text}");
                named += 1;
            }
            Outcome::Loaded => open += 1,
            Outcome::Refused => refused += 1,
        }
    }
    // Not vacuous: the corpus has this many mappings, and this is what becomes of each.
    // 16 are read by a struct, so the planted key is named. 3 lie inside the open `schema`
    // payload, where nothing is checked. 1 is `limits`, whose keys are data: the planted key
    // becomes an entry whose value is not a limit, and the typed read refuses it.
    assert_eq!(addresses.len(), 20);
    assert_eq!((named, open, refused), (16, 3, 1));
}

#[test]
fn two_unknown_keys_are_both_named_in_file_order_by_both_readers() {
    let (base, addresses) = addresses();
    let mut compared = 0;
    for (i, first) in addresses.iter().enumerate() {
        for second in &addresses[i + 1..] {
            let text = planted(&base, &[(first, "planted_a"), (second, "planted_b")]);
            let ours = shared::<Document>(&text);
            let theirs = through_serde_ignored::<Document>(&text);
            // Where one planted key makes the typed read fail and the other is unknown, the
            // shared reader names the key first and serde_ignored's reader stops at the failure.
            // That order is the shared reader's contract and is pinned in its own tests; here
            // the comparison is over the texts where both got as far as naming keys.
            if ours == Outcome::Refused || theirs == Outcome::Refused {
                continue;
            }
            assert_eq!(ours, theirs, "the readers disagree on:\n{text}");
            compared += 1;
        }
    }
    // 19 mappings other than `limits`, two at a time.
    assert_eq!(compared, 19 * 18 / 2);
}

closed! {
    root pub struct WithTag {
        check: Check,
    }
}
closed! {
    pub enum Check tagged "type" {
        "require" => Require(Require),
    }
}
closed! {
    pub struct Require {
        tool: String,
    }
}

/// The reason the shared reader does not use serde_ignored: a part selected by a tag key is
/// buffered before it is read, and serde_ignored does not see a key dropped there. If this test
/// starts failing on its second assertion, serde_ignored has learned to, and the two readers can
/// be compared over tagged parts as well.
#[test]
fn a_key_inside_a_tagged_part_is_named_by_the_shared_reader_and_missed_by_serde_ignored() {
    let text = "check:\n  type: require\n  tool: read\n  tol: write\n";
    assert_eq!(
        shared::<WithTag>(text),
        Outcome::Unknown(vec!["check.tol".to_string()])
    );
    assert_eq!(through_serde_ignored::<WithTag>(text), Outcome::Loaded);
}

#[test]
fn the_policy_document_uses_the_tool_identity_wire_fields() {
    use assay_core::mcp::identity::ToolIdentity;

    closed! {
        root(yaml_types) struct Pins {
            tool_pins: BTreeMap<String, ToolIdentity>,
        }
    }
    let pin = ToolIdentity::new("demo", "example", &None, &None);
    let wire = serde_json::json!({
        "server_id": "demo", "tool_name": "example",
        "schema_hash": pin.schema_hash, "meta_hash": pin.meta_hash
    });
    assert_eq!(serde_json::to_value(&pin).unwrap(), wire);
    let mut input = serde_json::json!({"tool_pins": {"example": wire}});
    let parsed = load::<Pins>(&input.to_string()).unwrap();
    assert_eq!(parsed.tool_pins["example"], pin);
    input["tool_pins"]["example"]["description_hash"] = "unused".into();
    assert_eq!(
        load::<Pins>(&input.to_string()).unwrap_err(),
        Refusal::UnknownKeys(vec!["tool_pins.example.description_hash".into()])
    );
}

#[test]
fn public_tool_policy_keeps_its_positional_serde_defaults() {
    use assay_core::mcp::policy::ToolPolicy;

    let policy: ToolPolicy = serde_json::from_str("[null,null]").unwrap();
    assert!(policy.allow.is_none() && policy.deny.is_none());
    assert!(policy.allow_classes.is_none() && policy.redact_args_contract.is_none());
    assert!(serde_json::from_str::<ToolPolicy>("[null]").is_err());
}

#[test]
fn direct_serde_preserves_legacy_constraint_positions() {
    use assay_core::mcp::policy::McpPolicy;

    for constraints in [
        serde_json::json!({"run": {"command": ["^x$"]}}),
        serde_json::json!([["run", {"command": {"matches": "^x$"}}]]),
    ] {
        let policy: McpPolicy =
            serde_json::from_value(serde_json::json!({"constraints": constraints})).unwrap();
        assert_eq!(policy.constraints.len(), 1);
        assert_eq!(policy.constraints[0].tool, "run");
        assert_eq!(
            policy.constraints[0].params["command"].matches.as_deref(),
            Some("^x$")
        );
    }
}

#[test]
fn coverage_preserves_legacy_tool_fields_and_the_version_without_constraints() {
    use assay_core::mcp::policy::McpPolicy;
    use assay_core::on_error::ErrorPolicy;
    for (label, version) in [("", ""), ("version: '1.0'\n", "1.0"), ("version: 2\n", "2")] {
        let yaml = format!("{label}constraints: []\nallow: [root_tool]\ntools:\n  allow: [inline_tool]\n  arg_constraints: {{probe: {{limit: 3}}}}\nmetadata: {{owner: docs}}\non_error: allow\n");
        let (enforcement, coverage) = McpPolicy::load_for_coverage(&yaml).unwrap();
        assert_eq!(enforcement.version, version);
        assert_eq!(coverage.version, version);
        assert_eq!(coverage.tools.allow, enforcement.tools.allow);
        let converted = McpPolicy::migrate_coverage_yaml(yaml.as_bytes()).unwrap();
        let (_, migrated) = McpPolicy::load_for_coverage(&converted).unwrap();
        let mut expected = serde_json::to_value(&coverage).unwrap();
        expected["version"] = if version == "2" { "2" } else { "2.0" }.into();
        assert_eq!(serde_json::to_value(&migrated).unwrap(), expected);
        assert_eq!(coverage.tools.allow.unwrap(), ["inline_tool", "root_tool"]);
        assert_eq!(coverage.metadata.unwrap()["owner"], "docs");
        assert_eq!(coverage.on_error, ErrorPolicy::Allow);
        assert_eq!(coverage.tools.arg_constraints.unwrap()["probe"]["limit"], 3);
    }
}

#[test]
fn coverage_resolves_root_anchors_and_enforces_their_schema() {
    use assay_core::mcp::policy::McpPolicy;
    let yaml = "version: 2\nx-rules: &rules [{type: require, tool: read_file}]\nsequences: *rules\nschemas: {}\n";
    let (_, coverage) = McpPolicy::load_for_coverage(yaml).unwrap();
    assert_eq!(coverage.sequences.len(), 1);
    let wrong = yaml.replace("tool: read_file", "tool: read_file, typoo: true");
    let error = McpPolicy::load_for_coverage(&wrong).unwrap_err();
    assert!(error.to_string().contains("sequences.0.typoo"), "{error}");
}

#[test]
fn coverage_migration_preserves_argument_and_alias_names() {
    use assay_core::mcp::policy::McpPolicy;
    let yaml = "version: '1.0'\ntools: {require_args: {Read: [0x10]}}\naliases: {Group: [2.50]}\n";
    let converted = McpPolicy::migrate_coverage_yaml(yaml.as_bytes()).unwrap();
    let (_, policy) = McpPolicy::load_for_coverage(&converted).unwrap();
    assert_eq!(policy.tools.require_args.unwrap()["Read"], ["0x10"]);
    assert_eq!(policy.aliases["Group"], ["2.50"]);
}

#[test]
fn coverage_migration_rewrites_tagged_field_names() {
    use assay_core::mcp::policy::McpPolicy;
    let yaml = "version: '1.0'\n!field allow: [Read]\ntools: {deny: [Write]}\nx-origin: foo\n";
    let converted = McpPolicy::migrate_coverage_yaml(yaml.as_bytes()).unwrap();
    let (_, policy) = McpPolicy::load_for_coverage(&converted).unwrap();
    assert_eq!(policy.tools.allow.unwrap(), ["Read"]);
    assert_eq!(policy.tools.deny.unwrap(), ["Write"]);
    assert!(converted.contains("x-origin: foo"));
}

#[test]
fn coverage_migration_keeps_metadata_key_spelling() {
    use assay_core::mcp::policy::McpPolicy;
    let fields = "metadata: {0x10: owner}";
    let (_, before) = McpPolicy::load_for_coverage(&format!("version: '2.0'\n{fields}")).unwrap();
    let original = before.metadata.unwrap();
    assert_eq!(original["0x10"], "owner");
    let migrated =
        McpPolicy::migrate_coverage_yaml(format!("version: '1.0'\n{fields}").as_bytes()).unwrap();
    let (_, after) = McpPolicy::load_for_coverage(&migrated).unwrap();
    assert_eq!(after.metadata.unwrap(), original);
}
