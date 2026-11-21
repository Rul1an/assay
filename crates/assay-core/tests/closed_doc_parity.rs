//! The shared reader and serde_ignored around the typed read are two readers of one rule: a key no
//! part of the type takes is named by its path as the file spells it. They are built differently
//! (a walk along a description of the type; `assay_common::ignored_keys::decode` around the typed
//! read), so they are compared here over a generated corpus instead of being assumed to agree.
//!
//! The corpus is one full document with an unknown key planted at every mapping, one at a time
//! and then two at a time.

use std::collections::BTreeMap;

use assay_common::closed;
use assay_common::doc::{load, ClosedDoc, NameOr, Opaque, Refusal, SeqOrMap};
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

/// The second reader, built from the same primitive the removed `unread_keys::from_yaml_str`
/// wrapped. Test code, which the reader counter does not count.
fn through_serde_ignored<T: serde::de::DeserializeOwned>(text: &str) -> Outcome {
    match assay_common::ignored_keys::decode::<_, T>(serde_yaml::Deserializer::from_str(text)) {
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
fn positional_legacy_constraints_are_read_only_by_migration() {
    use assay_core::mcp::policy::McpPolicy;
    for constraints in [
        serde_json::json!({"run": {"command": ["^x$"]}}),
        serde_json::json!([["run", {"command": {"matches": "^x$"}}]]),
    ] {
        let input = serde_json::json!({"constraints": constraints});
        assert!(serde_json::from_value::<McpPolicy>(input.clone()).is_err());
        let output = McpPolicy::migrate_yaml(&serde_json::to_vec(&input).unwrap()).unwrap();
        let policy = McpPolicy::from_slice(output.as_bytes()).unwrap();
        assert_eq!(
            policy.schemas["run"]["properties"]["command"]["pattern"],
            "^x$"
        );
    }
}

#[test]
fn converted_coverage_preserves_tool_fields_and_text_scalars() {
    use assay_core::mcp::policy::McpPolicy;
    use assay_core::on_error::ErrorPolicy;
    for (label, version) in [
        ("", "2.0"),
        ("version: '1.0'\n", "2.0"),
        ("version: 2\n", "2"),
    ] {
        let yaml = format!("{label}name: 2.50\nconstraints: []\nallow: [root_tool]\ntools:\n  allow: [inline_tool]\n  arg_constraints: {{probe: {{limit: 3}}}}\nmetadata: {{owner: docs}}\non_error: allow\n");
        assert!(McpPolicy::load_for_coverage(&yaml).is_err());
        assert!(McpPolicy::migrate_yaml(yaml.as_bytes()).is_err());
        let migrated = McpPolicy::migrate_coverage_yaml(yaml.as_bytes()).unwrap();
        let (enforcement, coverage) = McpPolicy::load_for_coverage(&migrated).unwrap();
        assert_eq!(coverage.name, "2.50");
        assert_eq!(enforcement.version, version);
        assert_eq!(coverage.version, version);
        assert_eq!(coverage.tools.allow, enforcement.tools.allow);
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

#[test]
fn eval_thresholds_keep_their_wire_forms_and_report_unknown_fields() {
    use assay_common::doc::JsonInput;
    use assay_core::model::{ThresholdingConfig, ThresholdingSettings};

    closed! { root struct Suite { thresholding: ThresholdingSettings } }
    closed! { root struct Metric { thresholding: ThresholdingConfig } }

    let suite = load::<Suite>("thresholding: {mode: relative, max_drop: 0.5}").unwrap();
    assert_eq!(
        serde_json::to_value(suite.thresholding).unwrap(),
        serde_json::json!({"mode": "relative", "max_drop": 0.5, "min_floor": null})
    );
    assert!(matches!(
        load::<Suite>("thresholding: [relative, 0.5, null]"),
        Err(Refusal::NotAMapping { path, .. }) if path == "thresholding"
    ));
    for source in ["thresholding: {max_drop: 0.25}", "thresholding: [0.25]"] {
        let metric = JsonInput::parse(source).unwrap().read::<Metric>().unwrap();
        assert_eq!(
            serde_json::to_value(metric.thresholding).unwrap(),
            serde_json::json!({"max_drop": 0.25})
        );
    }
    assert_eq!(
        serde_json::to_value(ThresholdingConfig::default()).unwrap(),
        serde_json::json!({"max_drop": null})
    );
    let typo = "thresholding: {extra_limit: 0.5}";
    for result in [
        load::<Suite>(typo).map(|_| ()),
        JsonInput::parse(typo).unwrap().read::<Metric>().map(|_| ()),
    ] {
        assert!(
            matches!(result, Err(Refusal::UnknownKeys(keys)) if keys == ["thresholding.extra_limit"])
        );
    }
}

#[test]
fn metric_threshold_typos_stop_execution_and_migration() {
    use assay_core::config::{load_config, load_config_for_migration};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("eval.yaml");
    for metric in [
        "type: semantic_similarity_to, semantic_similarity_to: hello",
        "type: faithfulness",
        "type: relevance",
    ] {
        let input = format!(
            "configVersion: 1\nsuite: thresholds\nmodel: trace\nsettings: {{thresholding: {{mode: relative}}}}\ntests:\n  - id: check\n    input: {{prompt: hello}}\n    expected: {{{metric}, thresholding: {{extra_limit: 0.25}}}}\n"
        );
        std::fs::write(&path, input).unwrap();
        for result in [
            load_config(&path, false, true)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            load_config_for_migration(&path)
                .map(|_| ())
                .map_err(|e| e.to_string()),
        ] {
            let error = result.expect_err(metric);
            assert!(
                error.contains("expected.thresholding") && error.contains("extra_limit"),
                "{metric}: {error}"
            );
        }
    }
}

#[test]
fn eval_telemetry_and_suite_thresholds_keep_defaults_and_name_nested_typos() {
    use assay_core::config::otel::OtelConfig;
    use assay_core::thresholds::ThresholdConfig;

    closed! { root struct Sections { #[default] otel: OtelConfig, #[default] thresholds: ThresholdConfig } }

    for input in ["{}", "otel: {}\nthresholds: {}"] {
        let doc = load::<Sections>(input).unwrap();
        assert_eq!(doc.otel, OtelConfig::default());
        assert!(doc.otel.capture_requires_sampled_span);
        assert!(!doc.otel.capture_acknowledged);
        assert_eq!(
            serde_json::to_value(&doc.otel).unwrap(),
            serde_json::json!({
                "genai_semconv_version": assay_core::otel::pin::GENAI_SEMCONV_PIN,
                "semconv_stability": "stable_only", "capture_mode": "off",
                "redaction": {"policies": []}, "exporter": {"allowlist": null, "allow_localhost": false},
                "capture_acknowledged": false, "capture_requires_sampled_span": true
            })
        );
        assert_eq!(
            serde_json::to_value(doc.thresholds).unwrap(),
            serde_json::json!({})
        );
    }

    let input = "otel:\n  semconv_stability: experimental_opt_in\n  capture_mode: blob_ref\n  capture_acknowledged: true\n  capture_requires_sampled_span: false\n  exporter: {allowlist: [example.org]}\n  redaction: {policies: [secret]}\nthresholds: {min_score: 0.5, block_on_warn: false}";
    let doc = load::<Sections>(input).unwrap();
    let original = serde_yaml::from_str::<serde_yaml::Value>(input).unwrap();
    assert_eq!(
        doc.otel,
        serde_yaml::from_value(original["otel"].clone()).unwrap()
    );
    let wire = serde_json::to_value(doc.otel).unwrap();
    assert_eq!(
        wire["exporter"]["allowlist"],
        serde_json::json!(["example.org"])
    );
    assert_eq!(wire["redaction"]["policies"], serde_json::json!(["secret"]));
    assert_eq!(wire["capture_mode"], "blob_ref");
    assert_eq!(wire["semconv_stability"], "experimental_opt_in");
    assert_eq!(wire["capture_requires_sampled_span"], false);
    assert_eq!(
        serde_json::to_value(doc.thresholds).unwrap(),
        serde_json::json!({"min_score":0.5,"block_on_warn":false})
    );

    let typo = "otel: {exporter: {allow_localhosts: true}, redaction: {policy_list: []}}\nthresholds: {min_scores: 0.2}";
    assert!(
        matches!(load::<Sections>(typo), Err(Refusal::UnknownKeys(keys)) if keys == ["otel.exporter.allow_localhosts", "otel.redaction.policy_list", "thresholds.min_scores"])
    );
}

#[test]
fn eval_settings_keep_sparse_output_and_required_reliability_fields() {
    use assay_core::model::Settings;
    closed! { root struct Suite { #[default] settings: Settings } }

    let read = |yaml: &str| load::<Suite>(yaml).map(|doc| doc.settings);
    assert_eq!(
        serde_json::to_value(read("{}").unwrap()).unwrap(),
        serde_json::json!({})
    );
    let settings =
        read("settings: {judge: {}, on_error: allow, bail_on_first_failure: true}").unwrap();
    let wire = serde_json::to_value(settings).unwrap();
    assert_eq!(wire["on_error"], "allow");
    assert_eq!(wire["bail_on_first_failure"], true);
    assert_eq!(
        wire["judge"]["reliability"]["rerun_strategy"],
        "sequential_sprt"
    );
    assert_eq!(wire["judge"]["reliability"]["tie_break"], "fail_closed");
    assert!(wire["judge"]
        .as_object()
        .unwrap()
        .contains_key("rubric_version"));
    assert!(read("settings: {judge: {reliability: {}}}").is_err());
    assert!(read("settings: {on_error: null}").is_err());
    let typo = "settings: {judge: {reliability: {call_budget: 9}}, thresholding: {floor_value: 0}}";
    assert!(
        matches!(read(typo), Err(Refusal::UnknownKeys(keys)) if keys == ["settings.judge.reliability.call_budget","settings.thresholding.floor_value"])
    );
}

#[test]
fn trace_assertions_name_unknown_fields_and_preserve_public_raw_rows() {
    use assay_core::agent_assertions::model::TraceAssertion;
    closed! { root struct Suite { assertions: Vec<TraceAssertion> } }
    let assertion = r#"{type: sequence_valid, test_trace_raw: [{tool: Read, extra: {"$serde_json::private::Number": "7"}}], policy: {regex: "^Read$"}}"#;
    let direct: TraceAssertion = serde_yaml::from_str(assertion).unwrap();
    let doc = load::<Suite>(&format!("assertions: [{assertion}]")).unwrap();
    assert_eq!(doc.assertions, [direct]);
    let TraceAssertion::SequenceValid {
        test_trace_raw: Some(rows),
        ..
    } = &doc.assertions[0]
    else {
        panic!("expected raw trace rows")
    };
    assert_eq!(rows[0]["tool"], "Read");
    assert!(rows[0].get("extra").is_some());

    let typed = "{type: sequence_valid, test_trace: [{id: 1, step_id: step, episode_id: episode, call_index: -1}]}";
    let loaded = load::<Suite>(&format!("assertions: [{typed}]")).unwrap();
    assert_eq!(
        loaded.assertions,
        [serde_yaml::from_str::<TraceAssertion>(typed).unwrap()]
    );
    let extra = typed.replace("call_index:", "unexpected_index:");
    assert!(serde_yaml::from_str::<TraceAssertion>(&extra).is_ok());
    assert!(
        matches!(load::<Suite>(&format!("assertions: [{extra}]")), Err(Refusal::UnknownKeys(keys)) if keys == ["assertions.0.test_trace.0.unexpected_index"])
    );
    assert!(load::<Suite>("assertions: [{type: sequence_valid, test_trace_raw: 7}]").is_err());
    assert!(
        matches!(load::<Suite>("assertions: [{type: sequence_valid, test_trace_row: []}]"), Err(Refusal::UnknownKeys(keys)) if keys == ["assertions.0.test_trace_row"])
    );
}

#[test]
fn a_test_input_is_a_prompt_or_a_mapping_in_the_shared_grammar() {
    use assay_core::model::TestInput;
    closed! { root struct Case { input: TestInput } }

    let read = |yaml: &str| load::<Case>(yaml).map(|doc| doc.input);
    let input = read("input: hello").unwrap();
    assert_eq!((input.prompt.as_str(), input.context), ("hello", None));
    let input = read("input: {prompt: hello, context: [a]}").unwrap();
    assert_eq!(input.context, Some(vec!["a".to_string()]));
    assert_eq!(read("input: {prompt: 123}").unwrap().prompt, "123");
    assert!(
        matches!(read("input: {prompt: hello, extra: 1}"), Err(Refusal::UnknownKeys(keys)) if keys == ["input.extra"])
    );
    for refused in [
        "input: 123",
        "input: [a]",
        "input: null",
        "input: {context: [a]}",
    ] {
        assert!(read(refused).is_err(), "{refused}");
    }
}

#[test]
fn a_pinned_tool_is_named_by_the_shared_reader_and_left_open_to_public_serde() {
    use assay_core::model::PinnedTool;
    closed! { root struct Suite { pinned_tools: Vec<PinnedTool> } }

    let pinned = |name: &str, description: Option<&str>, schema_sha256: Option<&str>| PinnedTool {
        name: name.to_string(),
        description: description.map(str::to_string),
        schema_sha256: schema_sha256.map(str::to_string),
    };
    let tools =
        "[{name: t, description: d}, {name: u, schema_sha256: abc}, {name: v, description: null}]";
    let direct: Vec<PinnedTool> = serde_yaml::from_str(tools).unwrap();
    assert_eq!(
        direct,
        [
            pinned("t", Some("d"), None),
            pinned("u", None, Some("abc")),
            pinned("v", None, None)
        ]
    );
    assert_eq!(
        load::<Suite>(&format!("pinned_tools: {tools}"))
            .unwrap()
            .pinned_tools,
        direct
    );

    // Public Serde reads a key the type has no field for and leaves it out; the shared reader
    // names it. A closed Expected would turn the second answer on in the eval loader.
    let extra = "{name: t, extra: 1}";
    assert_eq!(
        serde_yaml::from_str::<PinnedTool>(extra).unwrap(),
        pinned("t", None, None)
    );
    assert!(
        matches!(load::<Suite>(&format!("pinned_tools: [{extra}]")), Err(Refusal::UnknownKeys(keys)) if keys == ["pinned_tools.0.extra"])
    );
    // A scalar where a string is expected is read as its text, as public Serde reads a pinned
    // tool on its own; inside `Expected`, which serde buffers for its tag, it is refused.
    assert_eq!(
        serde_yaml::from_str::<PinnedTool>("{name: 123}").unwrap(),
        pinned("123", None, None)
    );
    assert_eq!(
        load::<Suite>("pinned_tools: [{name: 123}]")
            .unwrap()
            .pinned_tools,
        [pinned("123", None, None)]
    );
    // Every other form gets the same answer from both readers; `expected_wire_forms` pins what
    // public Serde answers.
    for tools in [
        "[{name: null}]",
        "[{name: t, description: ~}]",
        "[{description: d}]",
        "[t]",
        "[{name: t, description: [d]}]",
    ] {
        let direct = serde_yaml::from_str::<Vec<PinnedTool>>(tools).ok();
        let shared = load::<Suite>(&format!("pinned_tools: {tools}"))
            .ok()
            .map(|suite| suite.pinned_tools);
        assert_eq!(shared, direct, "{tools}");
    }
    for refused in ["[{description: d}]", "[t]", "[{name: t, description: [d]}]"] {
        assert!(
            load::<Suite>(&format!("pinned_tools: {refused}")).is_err(),
            "{refused}"
        );
    }
}

#[test]
fn the_case_grammar_walks_expected_and_keeps_metadata_open() {
    use assay_core::model::EvalConfig;

    let source = "configVersion: 1\nsuite: suite\nmodel: trace\ntests:\n  - id: first\n    input: hello\n    expected: {type: relevance, thresholding: {extra_limit: 1}}\n    metadata: {owner: {anything: 1}}";
    assert!(
        matches!(load::<EvalConfig>(source), Err(Refusal::UnknownKeys(keys)) if keys == ["tests.0.expected.thresholding.extra_limit"])
    );
    let valid = source.replace("extra_limit", "max_drop");
    assert!(load::<EvalConfig>(&valid).is_ok());
}

#[test]
fn the_eval_root_keeps_version_presence_specific_to_each_reader() {
    use assay_core::config::load_config_for_migration;
    use assay_core::model::EvalConfig;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("eval.yaml");
    let body = "suite: suite\nmodel: trace\ntests: [{id: example, input: hello, expected: {type: must_contain, must_contain: [hello]}}]";
    for (prefix, runtime, migration) in [
        ("", None, Some(0)),
        ("version: 0\n", Some(0), Some(0)),
        ("configVersion: 1\n", Some(1), Some(1)),
        ("version: null\n", None, None),
    ] {
        let source = format!("{prefix}{body}");
        assert_eq!(load::<EvalConfig>(&source).ok().map(|c| c.version), runtime);
        std::fs::write(&file, &source).unwrap();
        assert_eq!(
            load_config_for_migration(&file)
                .ok()
                .map(|m| m.config.version),
            migration,
            "{source}"
        );
    }
}

#[test]
fn the_eval_root_reads_short_positional_pinned_tools() {
    use assay_core::model::EvalConfig;
    let source = "version: 1\nsuite: suite\nmodel: trace\ntests:\n- id: tool\n  input: hello\n  expected: {type: tool_description_integrity, pinned_tools: [[first], [second, description]]}";
    let config = load::<EvalConfig>(source).unwrap();
    let expected = serde_json::to_value(&config.tests[0].expected).unwrap();
    assert_eq!(
        expected["pinned_tools"],
        serde_json::json!([{"name":"first"}, {"name":"second","description":"description"}])
    );
}

#[test]
fn eval_loader_reports_fields_buffered_by_serde() {
    use assay_core::config::{load_config, load_config_for_migration};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("eval.yaml");
    let source = r#"version: 1
suite: suite
model: trace
tests:
- id: first
  input: hello
  expected: {type: tool_description_integrity, pinned_tools: [{name: first, unknown_pin: true}]}
  assertions:
  - type: sequence_valid
    test_trace: [{id: 1, step_id: step, episode_id: episode, call_index: 0, unknown_row: true, unknown_row: false}]
    test_trace_raw: [{tool: Read}]
    policy: {regex: '^Read$'}
"#;
    std::fs::write(&path, source).unwrap();
    assert!(load_config(&path, false, false).is_ok());
    for err in [
        load_config(&path, false, true).err().map(|e| e.to_string()),
        load_config_for_migration(&path)
            .err()
            .map(|e| e.to_string()),
    ] {
        let message = err.expect("strict reader must report the two ignored keys");
        assert!(message.contains(r#"["tests.0.expected.pinned_tools.0.unknown_pin", "tests.0.assertions.0.test_trace.0.unknown_row"]"#), "{message}");
    }
}

#[test]
fn eval_unknown_fields_are_named_without_decoding_their_contents() {
    use assay_core::config::load_config;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("eval.yaml");
    let base = "version: 1\nsuite: suite\nmodel: trace\ntests: [{id: first, input: hello}]\n";
    for (fields, paths) in [
        ("typo: 1, typo: 2", r#"["settings.typo"]"#),
        ("typo: {? [a, b]: v}", r#"["settings.typo"]"#),
        ("0x10: 1, '16': 2", r#"["settings.0x10", "settings.16"]"#),
    ] {
        std::fs::write(&path, format!("{base}settings: {{{fields}}}")).unwrap();
        assert!(load_config(&path, false, false).is_ok(), "{fields}");
        let err = load_config(&path, false, true).unwrap_err().to_string();
        assert!(err.contains(paths), "{err}");
    }
}

#[test]
fn converting_a_single_legacy_assertion_names_ignored_fields_inside_it() {
    use assay_core::config::load_config_for_migration;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("eval.yaml");
    std::fs::write(&path, "version: 1\nsuite: suite\nmodel: trace\ntests: [{id: first, input: hello, expected: [{type: tool_description_integrity, pinned_tools: [{name: first, extra: 1}]}]}]").unwrap();
    let err = load_config_for_migration(&path)
        .err()
        .expect("migration would discard extra")
        .to_string();
    assert!(
        err.contains("tests.0.expected.0.pinned_tools.0.extra"),
        "{err}"
    );
}

#[test]
fn eval_typed_yaml_errors_keep_precedence_over_reporting() {
    use assay_core::config::load_config;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("eval.yaml");
    std::fs::write(&path, "version: 1\nsuite: suite\nmodel: trace\ntests:\n- id: first\n  input: hello\n  assertions: [{type: sequence_valid, test_trace: [{id: 1, step_id: step, episode_id: episode, unknown_row: {? [a, b]: v}}]}]").unwrap();
    let message = load_config(&path, false, true).unwrap_err().to_string();
    assert!(
        message
            .contains("tests[0].assertions[0].test_trace[0].unknown_row: non-string key at line 7"),
        "{message}"
    );
}

#[test]
fn eval_buffered_assertion_row_keeps_serde_field_indices() {
    use assay_core::config::load_config_for_migration;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("eval.yaml");
    for row in [
        "{0: 1, 1: step, 2: episode, 4: 0}",
        "{id: 1, step_id: step, episode_id: episode, call_index: 0}",
    ] {
        for assertion in [
            format!("{{type: sequence_valid, test_trace: [{row}], test_trace_raw: [{{tool: Read}}], policy: {{regex: '^Read$'}}}}"),
            format!("[sequence_valid, [{row}], [{{tool: Read}}], {{regex: '^Read$'}}, null]"),
        ] {
            std::fs::write(&path, format!("version: 1\nsuite: s\nmodel: trace\ntests: [{{id: a, input: hi, assertions: [{assertion}]}}]")).unwrap();
            load_config_for_migration(&path).unwrap();
        }
    }
}

#[test]
fn numeric_keys_in_expected_stay_json_names_in_the_field_walk() {
    use assay_core::model::EvalConfig;
    let source = "version: 1\nsuite: suite\nmodel: trace\ntests: [{id: a, input: hi, expected: {type: tool_description_integrity, pinned_tools: [{name: Read, 0: unused}]}}]";
    assert!(
        matches!(load::<EvalConfig>(source), Err(Refusal::UnknownKeys(keys)) if keys == ["tests.0.expected.pinned_tools.0.0"])
    );
}

#[test]
fn buffered_trace_row_reports_each_authored_key_spelling() {
    use assay_core::config::load_config_for_migration;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("eval.yaml");
    let text = "version: 1\nsuite: s\nmodel: trace\ntests: [{id: a, input: hi, assertions: [{type: sequence_valid, test_trace: [{id: 1, step_id: step, episode_id: episode, 0x4: 0, 0x10: typo, 16: another}], test_trace_raw: [{tool: Read}], policy: {regex: '^Read$'}}]}]";
    std::fs::write(&path, text).unwrap();
    let error = load_config_for_migration(&path)
        .err()
        .expect("unknown fields")
        .to_string();
    assert!(
        error.contains(
            r#"["tests.0.assertions.0.test_trace.0.0x10", "tests.0.assertions.0.test_trace.0.16"]"#
        ),
        "{error}"
    );
}

#[test]
fn reporting_reader_respects_declared_root_extensions() {
    closed! { root(x_extensions) struct WithExtensions { label: String } }
    let (doc, unread) = assay_common::doc::read_reporting_unknowns::<WithExtensions>(
        "x-template: &label hello\nunknown: true\nlabel: *label",
    )
    .unwrap();
    assert_eq!(doc.label, "hello");
    assert_eq!(unread, ["unknown"]);
}
