//! The wire of every current `Expected` variant, pinned byte for byte.
//!
//! `serde_json::to_string(&expected)` is the `expected` part of a test's fingerprint
//! (`engine/runner_next/single.rs`), and `serde_yaml` writes the same value when `assay migrate`
//! rewrites a config. A declaration change that moves these bytes changes cache keys and converter
//! output, so each case records what public Serde reads from YAML and from JSON and the exact JSON
//! and YAML it writes, or the exact refusal on each route.
//!
//! The cases cover every variant, its defaults, omitted and null fields, both field aliases
//! (`text`, `threshold`), the positional thresholding form, unknown fields at the variant, inside a
//! pinned tool (read and dropped) and inside a sequence rule (refused), and missing and null
//! required fields, and a scalar where a string is expected: `Expected` is internally tagged, so
//! serde buffers the mapping and refuses `name: 123` here, while a pinned tool read on its own, or
//! through the shared reader, takes the scalar's text. They were recorded on the unchanged
//! declaration (49243d8de).

use assay_core::model::{Expected, PinnedTool};

enum Want {
    Writes {
        json: &'static str,
        yaml: &'static str,
    },
    Refused {
        yaml: &'static str,
        json: &'static str,
    },
}
use Want::{Refused, Writes};

struct Case {
    name: &'static str,
    input: &'static str,
    want: Want,
}

#[rustfmt::skip]
const CASES: &[Case] = &[
    Case { name: r#"must_contain"#, input: r#"{type: must_contain, must_contain: [a]}"#, want: Writes { json: r#"{"type":"must_contain","must_contain":["a"]}"#, yaml: r#"type: must_contain
must_contain:
- a
"# } },
    Case { name: r#"must_contain default list"#, input: r#"{type: must_contain}"#, want: Writes { json: r#"{"type":"must_contain","must_contain":[]}"#, yaml: r#"type: must_contain
must_contain: []
"# } },
    Case { name: r#"must_not_contain"#, input: r#"{type: must_not_contain, must_not_contain: [a]}"#, want: Writes { json: r#"{"type":"must_not_contain","must_not_contain":["a"]}"#, yaml: r#"type: must_not_contain
must_not_contain:
- a
"# } },
    Case { name: r#"regex_match"#, input: r#"{type: regex_match, pattern: 'a+'}"#, want: Writes { json: r#"{"type":"regex_match","pattern":"a+","flags":[]}"#, yaml: r#"type: regex_match
pattern: a+
flags: []
"# } },
    Case { name: r#"regex_match flags"#, input: r#"{type: regex_match, pattern: 'a+', flags: [i]}"#, want: Writes { json: r#"{"type":"regex_match","pattern":"a+","flags":["i"]}"#, yaml: r#"type: regex_match
pattern: a+
flags:
- i
"# } },
    Case { name: r#"regex_not_match"#, input: r#"{type: regex_not_match, pattern: b}"#, want: Writes { json: r#"{"type":"regex_not_match","pattern":"b","flags":[]}"#, yaml: r#"type: regex_not_match
pattern: b
flags: []
"# } },
    Case { name: r#"json_schema"#, input: r#"{type: json_schema, json_schema: '{}'}"#, want: Writes { json: r#"{"type":"json_schema","json_schema":"{}","schema_file":null}"#, yaml: r#"type: json_schema
json_schema: '{}'
schema_file: null
"# } },
    Case { name: r#"json_schema file"#, input: r#"{type: json_schema, json_schema: '', schema_file: s.json}"#, want: Writes { json: r#"{"type":"json_schema","json_schema":"","schema_file":"s.json"}"#, yaml: r#"type: json_schema
json_schema: ''
schema_file: s.json
"# } },
    Case { name: r#"semantic_similarity_to"#, input: r#"{type: semantic_similarity_to, semantic_similarity_to: hi}"#, want: Writes { json: r#"{"type":"semantic_similarity_to","semantic_similarity_to":"hi","min_score":0.8,"thresholding":null}"#, yaml: r#"type: semantic_similarity_to
semantic_similarity_to: hi
min_score: 0.8
thresholding: null
"# } },
    Case { name: r#"semantic_similarity_to score"#, input: r#"{type: semantic_similarity_to, semantic_similarity_to: hi, min_score: 0.9}"#, want: Writes { json: r#"{"type":"semantic_similarity_to","semantic_similarity_to":"hi","min_score":0.9,"thresholding":null}"#, yaml: r#"type: semantic_similarity_to
semantic_similarity_to: hi
min_score: 0.9
thresholding: null
"# } },
    Case { name: r#"semantic_similarity_to aliases"#, input: r#"{type: semantic_similarity_to, text: hi, threshold: 0.7}"#, want: Writes { json: r#"{"type":"semantic_similarity_to","semantic_similarity_to":"hi","min_score":0.7,"thresholding":null}"#, yaml: r#"type: semantic_similarity_to
semantic_similarity_to: hi
min_score: 0.7
thresholding: null
"# } },
    Case { name: r#"semantic_similarity_to thresholding"#, input: r#"{type: semantic_similarity_to, semantic_similarity_to: hi, thresholding: {max_drop: 0.1}}"#, want: Writes { json: r#"{"type":"semantic_similarity_to","semantic_similarity_to":"hi","min_score":0.8,"thresholding":{"max_drop":0.1}}"#, yaml: r#"type: semantic_similarity_to
semantic_similarity_to: hi
min_score: 0.8
thresholding:
  max_drop: 0.1
"# } },
    Case { name: r#"semantic_similarity_to positional thresholding"#, input: r#"{type: semantic_similarity_to, semantic_similarity_to: hi, thresholding: [0.1]}"#, want: Writes { json: r#"{"type":"semantic_similarity_to","semantic_similarity_to":"hi","min_score":0.8,"thresholding":{"max_drop":0.1}}"#, yaml: r#"type: semantic_similarity_to
semantic_similarity_to: hi
min_score: 0.8
thresholding:
  max_drop: 0.1
"# } },
    Case { name: r#"semantic_similarity_to null thresholding"#, input: r#"{type: semantic_similarity_to, semantic_similarity_to: hi, thresholding: null}"#, want: Writes { json: r#"{"type":"semantic_similarity_to","semantic_similarity_to":"hi","min_score":0.8,"thresholding":null}"#, yaml: r#"type: semantic_similarity_to
semantic_similarity_to: hi
min_score: 0.8
thresholding: null
"# } },
    Case { name: r#"judge_criteria"#, input: r#"{type: judge_criteria, judge_criteria: {z: 1, a: [true, null]}}"#, want: Writes { json: r#"{"type":"judge_criteria","judge_criteria":{"z":1,"a":[true,null]}}"#, yaml: r#"type: judge_criteria
judge_criteria:
  z: 1
  a:
  - true
  - null
"# } },
    Case { name: r#"faithfulness"#, input: r#"{type: faithfulness}"#, want: Writes { json: r#"{"type":"faithfulness","min_score":0.8,"rubric_version":null,"thresholding":null}"#, yaml: r#"type: faithfulness
min_score: 0.8
rubric_version: null
thresholding: null
"# } },
    Case { name: r#"faithfulness full"#, input: r#"{type: faithfulness, min_score: 0.8, rubric_version: v2, thresholding: {max_drop: 0.2}}"#, want: Writes { json: r#"{"type":"faithfulness","min_score":0.8,"rubric_version":"v2","thresholding":{"max_drop":0.2}}"#, yaml: r#"type: faithfulness
min_score: 0.8
rubric_version: v2
thresholding:
  max_drop: 0.2
"# } },
    Case { name: r#"relevance"#, input: r#"{type: relevance, rubric_version: v1}"#, want: Writes { json: r#"{"type":"relevance","min_score":0.8,"rubric_version":"v1","thresholding":null}"#, yaml: r#"type: relevance
min_score: 0.8
rubric_version: v1
thresholding: null
"# } },
    Case { name: r#"args_valid policy"#, input: r#"{type: args_valid, policy: p.yaml}"#, want: Writes { json: r#"{"type":"args_valid","policy":"p.yaml"}"#, yaml: r#"type: args_valid
policy: p.yaml
"# } },
    Case { name: r#"args_valid schema"#, input: r#"{type: args_valid, schema: {version: '2', schemas: {Search: {type: object}}}}"#, want: Writes { json: r#"{"type":"args_valid","schema":{"version":"2","schemas":{"Search":{"type":"object"}}}}"#, yaml: r#"type: args_valid
schema:
  version: '2'
  schemas:
    Search:
      type: object
"# } },
    Case { name: r#"args_valid null policy"#, input: r#"{type: args_valid, policy: null, schema: {version: '2'}}"#, want: Writes { json: r#"{"type":"args_valid","schema":{"version":"2"}}"#, yaml: r#"type: args_valid
schema:
  version: '2'
"# } },
    Case { name: r#"args_valid empty"#, input: r#"{type: args_valid}"#, want: Writes { json: r#"{"type":"args_valid"}"#, yaml: r#"type: args_valid
"# } },
    Case { name: r#"sequence_valid sequence"#, input: r#"{type: sequence_valid, sequence: [a, b]}"#, want: Writes { json: r#"{"type":"sequence_valid","sequence":["a","b"]}"#, yaml: r#"type: sequence_valid
sequence:
- a
- b
"# } },
    Case { name: r#"sequence_valid rules"#, input: r#"{type: sequence_valid, rules: [{type: before, first: A, then: B}, {type: require, tool: {tool: C, args_match: {q: x}}}]}"#, want: Writes { json: r#"{"type":"sequence_valid","rules":[{"type":"before","first":"A","then":"B"},{"type":"require","tool":{"tool":"C","args_match":{"q":"x"}}}]}"#, yaml: r#"type: sequence_valid
rules:
- type: before
  first: A
  then: B
- type: require
  tool:
    tool: C
    args_match:
      q: x
"# } },
    Case { name: r#"sequence_valid policy"#, input: r#"{type: sequence_valid, policy: s.yaml}"#, want: Writes { json: r#"{"type":"sequence_valid","policy":"s.yaml"}"#, yaml: r#"type: sequence_valid
policy: s.yaml
"# } },
    Case { name: r#"tool_blocklist"#, input: r#"{type: tool_blocklist, blocked: [x]}"#, want: Writes { json: r#"{"type":"tool_blocklist","blocked":["x"]}"#, yaml: r#"type: tool_blocklist
blocked:
- x
"# } },
    Case { name: r#"tool_description_integrity empty"#, input: r#"{type: tool_description_integrity}"#, want: Writes { json: r#"{"type":"tool_description_integrity"}"#, yaml: r#"type: tool_description_integrity
"# } },
    Case { name: r#"tool_description_integrity pinned"#, input: r#"{type: tool_description_integrity, pinned_tools: [{name: t, description: d}, {name: u, schema_sha256: abc}]}"#, want: Writes { json: r#"{"type":"tool_description_integrity","pinned_tools":[{"name":"t","description":"d"},{"name":"u","schema_sha256":"abc"}]}"#, yaml: r#"type: tool_description_integrity
pinned_tools:
- name: t
  description: d
- name: u
  schema_sha256: abc
"# } },
    Case { name: r#"tool_output_valid empty"#, input: r#"{type: tool_output_valid}"#, want: Writes { json: r#"{"type":"tool_output_valid"}"#, yaml: r#"type: tool_output_valid
"# } },
    Case { name: r#"tool_output_valid schemas"#, input: r#"{type: tool_output_valid, schemas: {Search: {type: object}}}"#, want: Writes { json: r#"{"type":"tool_output_valid","schemas":{"Search":{"type":"object"}}}"#, yaml: r#"type: tool_output_valid
schemas:
  Search:
    type: object
"# } },
    Case { name: r#"tool_collision_detect empty"#, input: r#"{type: tool_collision_detect}"#, want: Writes { json: r#"{"type":"tool_collision_detect"}"#, yaml: r#"type: tool_collision_detect
"# } },
    Case { name: r#"tool_collision_detect trusted"#, input: r#"{type: tool_collision_detect, trusted_servers: [s]}"#, want: Writes { json: r#"{"type":"tool_collision_detect","trusted_servers":["s"]}"#, yaml: r#"type: tool_collision_detect
trusted_servers:
- s
"# } },
    Case { name: r#"reference"#, input: r#"{type: $ref, path: p.yaml}"#, want: Writes { json: r#"{"type":"$ref","path":"p.yaml"}"#, yaml: r#"type: $ref
path: p.yaml
"# } },
    Case { name: r#"unknown variant field"#, input: r#"{type: must_contain, must_contain: [a], extra: 1}"#, want: Refused { yaml: r#"unknown field `extra`, expected `must_contain`"#, json: r#"unknown field `extra`, expected `must_contain`"# } },
    Case { name: r#"unknown pinned tool field"#, input: r#"{type: tool_description_integrity, pinned_tools: [{name: t, extra: 1}]}"#, want: Writes { json: r#"{"type":"tool_description_integrity","pinned_tools":[{"name":"t"}]}"#, yaml: r#"type: tool_description_integrity
pinned_tools:
- name: t
"# } },
    Case { name: r#"unknown sequence rule field"#, input: r#"{type: sequence_valid, rules: [{type: before, first: A, then: B, extra: 1}]}"#, want: Refused { yaml: r#"unknown field `extra`, expected `first` or `then`"#, json: r#"unknown field `extra`, expected `first` or `then`"# } },
    Case { name: r#"alias and canonical"#, input: r#"{type: semantic_similarity_to, semantic_similarity_to: a, text: b}"#, want: Refused { yaml: r#"duplicate field `semantic_similarity_to`"#, json: r#"duplicate field `semantic_similarity_to`"# } },
    Case { name: r#"both score spellings"#, input: r#"{type: semantic_similarity_to, semantic_similarity_to: a, min_score: 0.5, threshold: 0.6}"#, want: Refused { yaml: r#"duplicate field `min_score`"#, json: r#"duplicate field `min_score`"# } },
    Case { name: r#"null required list"#, input: r#"{type: tool_blocklist, blocked: null}"#, want: Refused { yaml: r#"invalid type: unit value, expected a sequence"#, json: r#"invalid type: null, expected a sequence"# } },
    Case { name: r#"null score"#, input: r#"{type: faithfulness, min_score: null}"#, want: Refused { yaml: r#"invalid type: unit value, expected f64"#, json: r#"invalid type: null, expected f64"# } },
    Case { name: r#"missing required"#, input: r#"{type: regex_match}"#, want: Refused { yaml: r#"missing field `pattern`"#, json: r#"missing field `pattern`"# } },
    Case { name: r#"unknown type"#, input: r#"{type: nope}"#, want: Refused { yaml: r#"type: unknown variant `nope`, expected one of `must_contain`, `must_not_contain`, `regex_match`, `regex_not_match`, `json_schema`, `semantic_similarity_to`, `judge_criteria`, `faithfulness`, `relevance`, `args_valid`, `sequence_valid`, `tool_blocklist`, `tool_description_integrity`, `tool_output_valid`, `tool_collision_detect`, `$ref` at line 1 column 8"#, json: r#"unknown variant `nope`, expected one of `must_contain`, `must_not_contain`, `regex_match`, `regex_not_match`, `json_schema`, `semantic_similarity_to`, `judge_criteria`, `faithfulness`, `relevance`, `args_valid`, `sequence_valid`, `tool_blocklist`, `tool_description_integrity`, `tool_output_valid`, `tool_collision_detect`, `$ref`"# } },
    Case { name: r#"no type"#, input: r#"{must_contain: [a]}"#, want: Refused { yaml: r#"missing field `type`"#, json: r#"missing field `type`"# } },
    Case { name: r#"pinned tool without name"#, input: r#"{type: tool_description_integrity, pinned_tools: [{description: d}]}"#, want: Refused { yaml: r#"missing field `name`"#, json: r#"missing field `name`"# } },
    Case { name: r#"pinned tool scalar name"#, input: r#"{type: tool_description_integrity, pinned_tools: [{name: 123}]}"#, want: Refused { yaml: r#"invalid type: integer `123`, expected a string"#, json: r#"invalid type: integer `123`, expected a string"# } },
    Case { name: r#"must_contain scalar item"#, input: r#"{type: must_contain, must_contain: [1]}"#, want: Refused { yaml: r#"invalid type: integer `1`, expected a string"#, json: r#"invalid type: integer `1`, expected a string"# } },
    Case { name: r#"regex_match scalar pattern"#, input: r#"{type: regex_match, pattern: 123}"#, want: Refused { yaml: r#"invalid type: integer `123`, expected a string"#, json: r#"invalid type: integer `123`, expected a string"# } },
];

#[test]
fn every_expected_variant_keeps_its_wire() {
    let mut differences = Vec::new();
    for case in CASES {
        let from_yaml = serde_yaml::from_str::<Expected>(case.input).map_err(|e| e.to_string());
        let as_json: serde_json::Value =
            serde_yaml::from_str(case.input).expect("the input is YAML");
        let from_json = serde_json::from_value::<Expected>(as_json).map_err(|e| e.to_string());
        let seen = match (&from_yaml, &from_json) {
            (Ok(yaml), Ok(json)) => {
                let written = serde_json::to_string(yaml).expect("json");
                if serde_json::to_string(json).expect("json") != written {
                    differences.push(format!(
                        "{}: the YAML and JSON reads write different bytes",
                        case.name
                    ));
                }
                let reread: Expected =
                    serde_json::from_str(&written).expect("its own output reads back");
                if serde_json::to_string(&reread).expect("json") != written {
                    differences.push(format!(
                        "{}: its output does not read back to the same bytes",
                        case.name
                    ));
                }
                Some((written, serde_yaml::to_string(yaml).expect("yaml")))
            }
            _ => None,
        };
        match (&case.want, seen, &from_yaml, &from_json) {
            (Writes { json, yaml }, Some((written_json, written_yaml)), _, _) => {
                if written_json != *json || written_yaml != *yaml {
                    differences.push(format!(
                        "{}: writes {written_json} / {written_yaml:?}",
                        case.name
                    ));
                }
            }
            (Refused { yaml, json }, None, Err(yaml_error), Err(json_error)) => {
                if yaml_error != yaml || json_error != json {
                    differences.push(format!(
                        "{}: refused with {yaml_error:?} / {json_error:?}",
                        case.name
                    ));
                }
            }
            (_, _, yaml_read, json_read) => differences.push(format!(
                "{}: {:?} / {:?}",
                case.name,
                yaml_read.as_ref().map(|_| "read"),
                json_read.as_ref().map(|_| "read")
            )),
        }
    }
    assert!(differences.is_empty(), "{differences:#?}");
}

/// A pinned tool read on its own: what YAML and JSON give, as the JSON written or the error.
/// Read straight from YAML a scalar gives its text, so `name: 123` and `name: null` are names here
/// while JSON, and `Expected` for its buffered tag, refuse them.
/// The JSON a read writes, or its error.
type Read = Result<&'static str, &'static str>;

#[rustfmt::skip]
const PINNED: &[(&str, Read, Read)] = &[
    ("{name: t}", Ok(r#"{"name":"t"}"#), Ok(r#"{"name":"t"}"#)),
    ("{name: t, description: d, schema_sha256: abc}", Ok(r#"{"name":"t","description":"d","schema_sha256":"abc"}"#), Ok(r#"{"name":"t","description":"d","schema_sha256":"abc"}"#)),
    ("{name: t, description: null}", Ok(r#"{"name":"t"}"#), Ok(r#"{"name":"t"}"#)),
    ("{name: t, extra: 1}", Ok(r#"{"name":"t"}"#), Ok(r#"{"name":"t"}"#)),
    ("{name: 123}", Ok(r#"{"name":"123"}"#), Err("invalid type: integer `123`, expected a string")),
    ("{name: null}", Ok(r#"{"name":"null"}"#), Err("invalid type: null, expected a string")),
    ("{description: d}", Err("missing field `name`"), Err("missing field `name`")),
    ("{name: t, description: [d]}", Err("description: invalid type: sequence, expected a string at line 1 column 24"), Err("invalid type: sequence, expected a string")),
];

#[test]
fn a_pinned_tool_read_on_its_own_keeps_its_wire() {
    let written = |read: Result<PinnedTool, String>| {
        read.map(|tool| serde_json::to_string(&tool).expect("json"))
    };
    let mut differences = Vec::new();
    for (input, yaml, json) in PINNED {
        let from_yaml =
            written(serde_yaml::from_str::<PinnedTool>(input).map_err(|e| e.to_string()));
        let as_json: serde_json::Value = serde_yaml::from_str(input).expect("the input is YAML");
        let from_json =
            written(serde_json::from_value::<PinnedTool>(as_json).map_err(|e| e.to_string()));
        let want = (
            yaml.map(str::to_string).map_err(str::to_string),
            json.map(str::to_string).map_err(str::to_string),
        );
        if (from_yaml.clone(), from_json.clone()) != want {
            differences.push(format!("{input}: yaml {from_yaml:?} json {from_json:?}"));
        }
    }
    assert!(differences.is_empty(), "{differences:#?}");
}

/// The similarity variant declared in the shared grammar, with its two aliases as tagged-variant
/// field aliases. It is compared with `Expected` itself, which the table above pins.
mod similarity_mirror {
    use assay_common::closed;
    use assay_core::model::ThresholdingConfig;

    /// A copy of the crate-private default; the comparison below catches a drift.
    pub fn default_min_score() -> f64 {
        0.8
    }

    closed! {
        pub enum Mirror tagged "type" {
            "semantic_similarity_to" => SemanticSimilarityTo {
                #[key = "semantic_similarity_to", alias = "text"]
                semantic_similarity_to: String,
                #[default = "default_min_score"]
                #[key = "min_score", alias = "threshold"]
                min_score: f64,
                thresholding: Option<ThresholdingConfig>,
            },
        }
    }
    closed! { root pub struct Document { check: Mirror } }
}

#[test]
fn the_similarity_aliases_read_and_write_like_expected_in_the_shared_grammar() {
    use assay_common::doc::{load, Refusal};
    use similarity_mirror::{Document, Mirror};

    let written = |json: Result<String, String>| json;
    let mut differences = Vec::new();
    for input in [
        "{type: semantic_similarity_to, semantic_similarity_to: hi}",
        "{type: semantic_similarity_to, semantic_similarity_to: hi, min_score: 0.9}",
        "{type: semantic_similarity_to, text: hi, threshold: 0.7}",
        "{type: semantic_similarity_to, text: hi}",
        "{type: semantic_similarity_to, semantic_similarity_to: hi, threshold: 0.7}",
        "{type: semantic_similarity_to, text: hi, thresholding: {max_drop: 0.1}}",
        "{type: semantic_similarity_to, text: hi, thresholding: [0.1]}",
        "{type: semantic_similarity_to, text: hi, thresholding: null}",
        "{type: semantic_similarity_to, text: hi, semantic_similarity_to: ho}",
        "{type: semantic_similarity_to, text: hi, threshold: 0.7, min_score: 0.9}",
        "{type: semantic_similarity_to, threshold: 0.7}",
        "{type: semantic_similarity_to, text: hi, extra: 1}",
    ] {
        let expected = written(
            serde_yaml::from_str::<Expected>(input)
                .map(|value| serde_json::to_string(&value).expect("json"))
                .map_err(|e| e.to_string()),
        );
        let direct = written(
            serde_yaml::from_str::<Mirror>(input)
                .map(|value| serde_json::to_string(&value).expect("json"))
                .map_err(|e| e.to_string()),
        );
        let shared = load::<Document>(&format!("check: {input}"));
        let agrees = match (&expected, &shared) {
            (Ok(json), Ok(document)) => {
                serde_json::to_string(&document.check).expect("json") == *json
            }
            (Err(error), Err(Refusal::Shape { message, .. })) => message.contains(error.as_str()),
            (Err(error), Err(Refusal::UnknownKeys(keys))) => {
                error.starts_with("unknown field `extra`") && keys == &["check.extra"]
            }
            _ => false,
        };
        if direct != expected || !agrees {
            differences.push(format!(
                "{input}: expected {expected:?} mirror {direct:?} shared {shared:?}"
            ));
        }
    }
    assert!(differences.is_empty(), "{differences:#?}");
    assert_eq!(
        serde_json::to_string(
            &load::<Document>("check: {type: semantic_similarity_to, text: hi, threshold: 0.7}")
                .unwrap()
                .check
        )
        .unwrap(),
        r#"{"type":"semantic_similarity_to","semantic_similarity_to":"hi","min_score":0.7,"thresholding":null}"#
    );
}

#[test]
fn positional_pinned_tool_defaults_only_the_trailing_fields() {
    for (input, expected) in [
        (r#"["t"]"#, r#"{"name":"t"}"#),
        (r#"["t","d"]"#, r#"{"name":"t","description":"d"}"#),
    ] {
        let tool: PinnedTool = serde_json::from_str(input).unwrap();
        assert_eq!(serde_json::to_string(&tool).unwrap(), expected);
    }
    assert!(serde_json::from_str::<PinnedTool>("[]").is_err());
}

#[test]
fn production_expected_exposes_the_authored_unknown_field_path() {
    use assay_common::doc::{load, Refusal};
    assay_common::closed! { root struct Suite { expected: Expected } }
    let bad = "expected: {type: sequence_valid, rules: [{type: require, tool: {tool: t, args_match: {}, typo: true}}]}";
    assert!(matches!(load::<Suite>(bad), Err(Refusal::UnknownKeys(keys))
        if keys == ["expected.rules.0.tool.typo"]));
    let valid = "expected: {type: semantic_similarity_to, text: x, threshold: 0.5}";
    let config = load::<Suite>(valid).unwrap();
    assert_eq!(
        serde_json::to_value(config.expected).unwrap(),
        serde_json::json!({
            "type": "semantic_similarity_to", "semantic_similarity_to":"x", "min_score":0.5, "thresholding":null
        })
    );
}

#[test]
fn only_the_metric_field_adds_context_to_a_threshold_error() {
    use assay_core::model::ThresholdingConfig;
    let bad = serde_json::json!({"extra_limit": 1});
    let message = serde_json::from_value::<ThresholdingConfig>(bad.clone())
        .unwrap_err()
        .to_string();
    assert_eq!(message, "unknown field `extra_limit`, expected `max_drop`");
    for variant in ["semantic_similarity_to", "faithfulness", "relevance"] {
        let mut expected = serde_json::json!({"type": variant, "thresholding": bad});
        if variant == "semantic_similarity_to" {
            expected["text"] = "sample".into();
        }
        assert_eq!(
            serde_json::from_value::<Expected>(expected)
                .unwrap_err()
                .to_string(),
            format!("expected.thresholding: {message}")
        );
    }
}

#[test]
fn optional_schema_bodies_keep_native_json_decoding() {
    for (variant, field) in [("args_valid", "schema"), ("tool_output_valid", "schemas")] {
        let input = format!(r#"{{"type":"{variant}","{field}":{{"key":1,"key":2}}}}"#);
        let value: Expected = serde_json::from_str(&input).unwrap();
        assert_eq!(
            serde_json::to_value(value).unwrap()[field],
            serde_json::json!({"key":2})
        );
    }
}
