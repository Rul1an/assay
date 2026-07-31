//! Era-parity vectors, driven from an external fixture corpus.
//!
//! Crate-internal on purpose. `McpEvent`, `parse_mcp_transcript_detailed` and the era sidecar stay
//! `pub(crate)`: publishing them so a test can reach them would establish a semver contract for a
//! shape slice 2 is still designing, and a test is not a reason to publish an API. The corpus lives
//! outside the source tree and is read as bytes, so slice 2 can point a production caller at the
//! same files without moving them.
//!
//! Every assertion is exact: a decoded value from the manifest compared against what the parser
//! and the conclusion layer actually produced. Nothing is asserted by exclusion, and no assertion
//! is satisfied by merely avoiding one wrong answer.
//!
//! Four layers, never collapsed. The schema says whether the pinned artifact accepts one message
//! under one era. The observation says what the transcript carried. The conclusion says what that
//! licenses. The profile is not restated here at all: every vector references the frozen row, so the
//! two cannot drift.

use super::era::*;
use super::parser::parse_mcp_transcript_detailed;
use super::types::McpInputFormat;
use serde_json::Value;
use std::path::PathBuf;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn corpus_dir() -> PathBuf {
    crate_root().join("tests/fixtures/mcp-era-parity-v0")
}

fn read_json(path: PathBuf) -> Value {
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn manifest() -> Value {
    read_json(corpus_dir().join("MANIFEST.json"))
}

fn pin() -> Value {
    read_json(corpus_dir().join("PIN.json"))
}

fn vectors() -> Vec<Value> {
    manifest()["vectors"].as_array().expect("vectors").clone()
}

fn vector(id: &str) -> Value {
    vectors()
        .into_iter()
        .find(|v| v["id"] == id)
        .unwrap_or_else(|| panic!("no vector {id}"))
}

fn transcript_text(id: &str) -> String {
    let file = vector(id)["file"].as_str().expect("file").to_string();
    std::fs::read_to_string(corpus_dir().join(file)).expect("read vector")
}

fn transcript(id: &str) -> Value {
    serde_json::from_str(&transcript_text(id)).expect("vector parses")
}

/// Every *result-bearing* response context in a transcript, in source order.
///
/// Total rather than selective: `no_vector_carries_a_response_without_a_result` refuses any
/// corpus response that has no result, so nothing is being filtered out here.
fn contexts(id: &str) -> Vec<McpEraContext> {
    parse_mcp_transcript_detailed(&transcript_text(id), McpInputFormat::StreamableHttp)
        .unwrap_or_else(|e| panic!("{id} must parse: {e:?}"))
        .into_iter()
        .filter(|p| p.context.result_observation.is_some())
        .map(|p| p.context)
        .collect()
}

/// Every *result-bearing* response context paired with the correlation id it belongs to.
///
/// Source order is deliberately not the attribution: a transcript may answer out of order, and
/// reading position is exactly the wrong key.
fn response_contexts_by_id(id: &str) -> Vec<(CorrelationId, McpEraContext)> {
    parse_mcp_transcript_detailed(&transcript_text(id), McpInputFormat::StreamableHttp)
        .unwrap_or_else(|e| panic!("{id} must parse: {e:?}"))
        .into_iter()
        .filter(|p| p.context.result_observation.is_some())
        .map(|p| {
            // The typed key, never `McpEvent.jsonrpc_id`: that renders JSON `1` and `"1"` alike, so
            // a harness reading it cannot tell two calls apart and would score one twice.
            let key = p
                .context
                .correlation
                .clone()
                .unwrap_or_else(|| panic!("{id}: a response with no correlation key"));
            (key, p.context)
        })
        .collect()
}

/// A manifest `jsonrpc_id` decoded into the key the parser correlates on.
///
/// A JSON number becomes `Num`, a JSON string becomes `Str`, and nothing else is an id. Written as
/// native JSON rather than as text so the manifest states the type instead of describing it.
fn manifest_correlation_id(vector_id: &str, raw: &Value) -> CorrelationId {
    match raw {
        Value::String(s) => CorrelationId::Str(s.clone()),
        Value::Number(n) => correlation_key(&Value::Number(n.clone()))
            .unwrap_or_else(|| panic!("{vector_id}: {n} is not an id this build keys on")),
        other => panic!("{vector_id}: jsonrpc_id must be a JSON string or number, got {other}"),
    }
}

fn context_for_call(vector_id: &str, key: &CorrelationId) -> McpEraContext {
    let mut found: Vec<McpEraContext> = response_contexts_by_id(vector_id)
        .into_iter()
        .filter(|(k, _)| k == key)
        .map(|(_, ctx)| ctx)
        .collect();
    assert_eq!(
        found.len(),
        1,
        "{vector_id}: expected exactly one response on {key:?}"
    );
    found.remove(0)
}

fn sole_context(id: &str) -> McpEraContext {
    let mut all = contexts(id);
    assert_eq!(all.len(), 1, "{id} was expected to carry one response");
    all.remove(0)
}

/// A vendored schema, digest-checked against the pin before it is used for anything.
///
/// Vendored so the corpus runs offline, and checked so a silent edit to the vendored copy cannot
/// quietly change what every vector means.
fn pinned_schema(era: &str) -> Value {
    let p = pin();
    let entry = p["schemas"]
        .as_array()
        .expect("schemas")
        .iter()
        .find(|s| s["era"] == era)
        .unwrap_or_else(|| panic!("no pinned schema for era {era}"))
        .clone();
    let file = entry["vendored_as"].as_str().expect("vendored_as");
    let bytes = std::fs::read(corpus_dir().join(file)).expect("read vendored schema");
    let actual = {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(&bytes))
    };
    assert_eq!(
        actual,
        entry["sha256"].as_str().expect("sha256"),
        "vendored {file} does not match PIN.json; the corpus means something other than it says"
    );
    serde_json::from_slice(&bytes).expect("schema parses")
}

/// Where a pinned artifact keeps its definitions.
///
/// The two artifacts are written against different drafts and this is not cosmetic: `2026-07-28` is
/// 2020-12 and uses `$defs`, `2025-06-18` is draft-07 and uses `definitions`. Hardcoding either one
/// makes the other resolve to nowhere, and a `$ref` into nothing is not a schema verdict.
/// An artifact carrying both containers is ambiguous, so it is refused rather than resolved: which
/// one the `$ref` should reach is exactly what is unclear, and preferring either silently makes the
/// verdict depend on a choice nobody recorded.
fn definitions_root(schema: &Value) -> &'static str {
    match (schema.get("$defs"), schema.get("definitions")) {
        (Some(_), None) => "$defs",
        (None, Some(_)) => "definitions",
        (Some(_), Some(_)) => {
            panic!("pinned artifact carries both $defs and definitions; the pin must carry exactly one")
        }
        (None, None) => panic!("pinned artifact has neither $defs nor definitions"),
    }
}

fn schema_accepts(era: &str, def: &str, doc: &Value) -> bool {
    let schema = pinned_schema(era);
    let root = definitions_root(&schema);
    let defs = schema[root].clone();
    assert!(
        defs.get(def).is_some(),
        "{era}: no definition {def} under {root}; the manifest names a definition the artifact \
         does not have"
    );

    let mut subschema = serde_json::Map::new();
    subschema.insert("$schema".to_string(), schema["$schema"].clone());
    subschema.insert(root.to_string(), defs);
    subschema.insert("$ref".to_string(), Value::String(format!("#/{root}/{def}")));

    jsonschema::validator_for(&Value::Object(subschema))
        .unwrap_or_else(|e| panic!("{era}/{def}: compile schema: {e}"))
        .is_valid(doc)
}

/// Every request message in a transcript, in source order.
///
/// Every hop, not the first one: a multi-hop vector could otherwise hide a second message behind a
/// passing first, which is the shape a fixture is most likely to get wrong.
fn request_messages(id: &str) -> Vec<Value> {
    transcript(id)["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter_map(|e| e.get("request").cloned())
        .collect()
}

/// Every `result` payload, in source order. The payload only, not the enclosing response message:
/// the definitions being validated against describe the result object.
fn result_payloads(id: &str) -> Vec<Value> {
    transcript(id)["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter_map(|e| e.get("response").and_then(|r| r.get("result")).cloned())
        .collect()
}

// --- The corpus keeps its own promises ----------------------------------------------------------

#[test]
fn every_vector_file_exists_and_parses() {
    for v in vectors() {
        let doc = read_json(corpus_dir().join(v["file"].as_str().expect("file")));
        assert!(
            doc.get("transport").is_some(),
            "{} not a transcript",
            v["id"]
        );
    }
}

/// Whether every response in a transcript carries a `result`.
///
/// `classify_message` calls a JSON-RPC *error* response a `Response` too, and `correlate_calls`
/// consumes it as one — but this corpus reads responses through `result_observation`, which an
/// error response does not have. So an extra or duplicate error response would be invisible to the
/// schema layer, to `per_call` coverage and to every conclusion, while the corpus reported exact
/// coverage. That is the silent exclusion this refuses.
///
/// The corpus has result-conclusion vocabulary and nothing else. Including error responses in
/// coverage would mean giving them a row, and a row needs a conclusion label — inventing one for a
/// case no vector exercises would add untested vocabulary to fix an absence. Refusing the shape
/// outright is the honest alternative: the filter downstream is then provably total rather than
/// selective, and the day an error vector is wanted, this guard is the thing that has to change.
fn every_response_bears_a_result(entries: &[Value]) -> Result<(), String> {
    for (i, e) in entries.iter().enumerate() {
        let Some(response) = e.get("response") else {
            continue;
        };
        // The canonical classifier decides what a message is, not the fixture wrapper and not a
        // structural rule of our own. `classify_message` answers `Request` whenever `method` is
        // present, even beside a `result`, so a hybrid sitting in a response slot parses as a
        // request, never gets a `result_observation`, and drops out of conclusions and coverage —
        // while a wrapper-shaped guard waves it through because it does carry a `result`. A guard
        // that classifies differently from the parser guards nothing.
        //
        // Value-free: `MessageKind::Request` carries the method the input chose, so the kind is
        // never rendered into the fault.
        match classify_message(response) {
            Ok(MessageKind::Response) => {}
            Ok(_) => {
                return Err(format!(
                    "entry {i} sits in a response slot but the parser classifies it as another \
                     kind of message, so it never reaches a result conclusion"
                ))
            }
            Err(_) => {
                return Err(format!(
                    "entry {i} sits in a response slot and is not a classifiable message"
                ))
            }
        }
        if response.get("result").is_none() {
            return Err(format!(
                "entry {i} is a response with no result; this corpus reads responses through \
                 result_observation, so such a response is invisible to coverage"
            ));
        }
    }
    Ok(())
}

/// The shape rule, on the transcripts it exists to refuse.
#[test]
fn a_response_without_a_result_is_refused() {
    let ok = serde_json::json!([
        {"response": {"jsonrpc": "2.0", "id": 1, "result": {"content": []}}},
        {"request": {"jsonrpc": "2.0", "id": 2, "method": "tools/call"}},
    ]);
    assert!(every_response_bears_a_result(ok.as_array().expect("array")).is_ok());

    let one_error = serde_json::json!([
        {"response": {"jsonrpc": "2.0", "id": 1, "error": {"code": -32000, "message": "no"}}},
    ]);
    let two_errors = serde_json::json!([
        {"response": {"jsonrpc": "2.0", "id": 1, "error": {"code": -32000, "message": "no"}}},
        {"response": {"jsonrpc": "2.0", "id": 1, "error": {"code": -32000, "message": "no"}}},
    ]);
    for (why, entries) in [
        ("a single error response", one_error),
        (
            "two error responses on one id, the duplicate coverage cannot see",
            two_errors,
        ),
    ] {
        let got = every_response_bears_a_result(entries.as_array().expect("array"))
            .expect_err(&format!("{why} must be refused"));
        assert!(
            got.contains("response with no result"),
            "{why}: got {got:?}"
        );
    }

    // The hybrid. It carries a `result`, so a wrapper-shaped guard accepts it, and `method` makes
    // the parser call it a request — so it would parse, correlate, and then be absent from every
    // result conclusion and from coverage, with nothing reporting the gap.
    let hybrid = serde_json::json!([
        {"response": {"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                      "result": {"content": []}}},
    ]);
    let got = every_response_bears_a_result(hybrid.as_array().expect("array"))
        .expect_err("a method+result hybrid in a response slot must be refused");
    assert!(
        got.contains("classifies it as another kind of message"),
        "the hybrid must be refused on classification, not on its body: got {got:?}"
    );
    assert!(
        !got.contains("tools/call"),
        "the fault must not echo the method the input chose: {got:?}"
    );
}

/// Every vector held to that shape, so the result-bearing filter used everywhere else is total.
#[test]
fn no_vector_carries_a_response_without_a_result() {
    for v in vectors() {
        let id = v["id"].as_str().expect("id");
        let doc = transcript(id);
        let entries = doc["entries"].as_array().expect("entries");
        if let Err(fault) = every_response_bears_a_result(entries) {
            panic!("{id}: {fault}");
        }
    }
}

/// Exactly one of something, as a rule rather than a `remove(0)`.
///
/// `remove(0)` on a vector of unknown length silently reads the first of many, so a fixture that
/// grew a second message would be scored on its first and the rest would go unexamined.
fn exactly_one<T>(what: &str, mut found: Vec<T>) -> Result<T, String> {
    if found.len() != 1 {
        return Err(format!(
            "expected exactly one {what}, found {}",
            found.len()
        ));
    }
    Ok(found.remove(0))
}

#[test]
fn exactly_one_refuses_none_and_many() {
    assert!(exactly_one("thing", vec![1]).is_ok());
    for (why, v) in [
        ("none", vec![]),
        ("two", vec![1, 2]),
        ("three", vec![1, 2, 3]),
    ] {
        let got = exactly_one("thing", v).expect_err(&format!("{why} must be refused"));
        assert!(got.contains("expected exactly one thing"), "{why}: {got:?}");
    }
}

/// Whether the manifest and the vector directory describe the same set, exactly once each.
///
/// `vector(id)` returns the first match, so a duplicate id silently makes every assertion exercise
/// the first row while the second file sits present and unread. A duplicate path does the same one
/// level down, and a path with no file — or a file with no row — means the corpus and the directory
/// have drifted apart. Returned as a fault so each rule is testable without breaking the corpus.
fn structural_faults(ids: &[String], paths: &[String], on_disk: &[String]) -> Result<(), String> {
    let mut seen_ids = std::collections::HashSet::new();
    for i in ids {
        if !seen_ids.insert(i) {
            return Err(format!("duplicate vector id {i}"));
        }
    }
    let mut seen_paths = std::collections::HashSet::new();
    for p in paths {
        if !seen_paths.insert(p) {
            return Err(format!("duplicate vector path {p}"));
        }
    }
    let declared: std::collections::HashSet<&String> = paths.iter().collect();
    let present: std::collections::HashSet<&String> = on_disk.iter().collect();
    let missing: Vec<&&String> = declared.difference(&present).collect();
    if !missing.is_empty() {
        return Err(format!(
            "manifest names files that are not there: {missing:?}"
        ));
    }
    let unlisted: Vec<&&String> = present.difference(&declared).collect();
    if !unlisted.is_empty() {
        return Err(format!("vector files no manifest row names: {unlisted:?}"));
    }
    Ok(())
}

/// The structural rules, exercised on the shapes a corpus could drift into. Each case isolates one
/// rule: a case tripping two proves nothing about either, since the first to fire masks the second.
#[test]
fn structural_faults_reject_duplicates_and_drift() {
    let v = |xs: &[&str]| xs.iter().map(|x| x.to_string()).collect::<Vec<_>>();

    assert!(structural_faults(&v(&["a", "b"]), &v(&["x", "y"]), &v(&["y", "x"])).is_ok());

    for (why, ids, paths, disk, fault) in [
        (
            "two rows sharing an id, everything else exact",
            v(&["a", "a"]),
            v(&["x", "y"]),
            v(&["x", "y"]),
            "duplicate vector id a",
        ),
        (
            "two rows naming one file, ids distinct",
            v(&["a", "b"]),
            v(&["x", "x"]),
            v(&["x"]),
            "duplicate vector path x",
        ),
        (
            "a row naming a file that is not there",
            v(&["a", "b"]),
            v(&["x", "y"]),
            v(&["x"]),
            "files that are not there",
        ),
        (
            "a file no row names",
            v(&["a"]),
            v(&["x"]),
            v(&["x", "z"]),
            "no manifest row names",
        ),
    ] {
        let got =
            structural_faults(&ids, &paths, &disk).expect_err(&format!("{why} must be refused"));
        assert!(
            got.contains(fault),
            "{why}: expected {fault:?}, got {got:?}"
        );
    }
}

/// The corpus on disk, held to those rules.
#[test]
fn the_corpus_and_the_directory_describe_one_set() {
    let ids: Vec<String> = vectors()
        .iter()
        .map(|v| v["id"].as_str().expect("id").to_string())
        .collect();
    let paths: Vec<String> = vectors()
        .iter()
        .map(|v| v["file"].as_str().expect("file").to_string())
        .collect();
    let mut on_disk: Vec<String> = std::fs::read_dir(corpus_dir().join("vectors"))
        .expect("vectors dir")
        .map(|e| {
            format!(
                "vectors/{}",
                e.expect("entry").file_name().to_string_lossy()
            )
        })
        .collect();
    on_disk.sort();

    if let Err(fault) = structural_faults(&ids, &paths, &on_disk) {
        panic!("corpus structure: {fault}");
    }
}

/// No two vectors may be byte-identical, and the capability axis must actually vary.
///
/// Two files with the same bytes cannot be exercising two different things, however differently
/// their manifest rows are worded — the rows would be asserting about one input while claiming to
/// cover two. This once happened: `modern-unrecognized-token` and
/// `capabilities-core-only-noncore-token` were the same bytes, and only the second declared a
/// capability observation, so the corpus looked wider than it was.
///
/// The second half is the one that matters for the capability arm: the vectors that name a
/// capability state must not all name the same one, or the axis is a label rather than a variable.
#[test]
fn no_two_vectors_are_the_same_bytes_and_the_capability_axis_varies() {
    use sha2::{Digest, Sha256};

    let mut seen: Vec<(String, String)> = Vec::new();
    for v in vectors() {
        let id = v["id"].as_str().expect("id").to_string();
        let file = v["file"].as_str().expect("file");
        let digest = hex::encode(Sha256::digest(
            std::fs::read(corpus_dir().join(file)).expect("read vector"),
        ));
        if let Some((other, _)) = seen.iter().find(|(_, d)| *d == digest) {
            panic!(
                "{id} and {other} are byte-identical; two rows cannot exercise two things on one \
                 input"
            );
        }
        seen.push((id, digest));
    }

    let declared: Vec<&str> = vectors()
        .iter()
        .filter_map(|v| {
            v["observation"]
                .get("capabilities")
                .and_then(|c| c.as_str())
        })
        .map(|s| match s {
            "core_only" => "core_only",
            "absent" => "absent",
            "extension_not_understood" => "extension_not_understood",
            other => panic!("unrecognised capability observation {other:?}"),
        })
        .collect();
    for state in ["core_only", "absent", "extension_not_understood"] {
        assert!(
            declared.contains(&state),
            "no vector declares capabilities {state}; the axis must vary, not just be labelled"
        );
    }
}

/// The maturity line is load-bearing: two corpora side by side without one let a reader borrow the
/// frozen corpus's standing.
#[test]
fn the_corpus_declares_its_lower_maturity() {
    let m = manifest();
    assert_eq!(m["maturity"], "exploratory");
    assert!(m["maturity_note"]
        .as_str()
        .expect("maturity_note")
        .contains("privileged-mcp-action-v0"));
}

/// The profile is referenced, never restated, and the row it names must exist in the frozen
/// manifest with the matrix the corpus relies on: policy confirmed, the other three incomplete.
#[test]
fn every_vector_references_the_frozen_profile_row() {
    let row_id = pin()["profile_baseline"]["row"]
        .as_str()
        .expect("row")
        .to_string();
    let frozen =
        read_json(crate_root().join("../../conformance/privileged-mcp-action-v0/MANIFEST.json"));
    let row = frozen["vectors"]
        .as_array()
        .expect("frozen vectors")
        .iter()
        .find(|r| r["id"] == row_id.as_str())
        .unwrap_or_else(|| panic!("frozen row {row_id} is gone"))
        .clone();
    let claims = &row["expected"]["claims"];
    assert_eq!(claims["policy_decision_recorded"]["status"], "confirmed");
    for cell in [
        "caller_visible_denial",
        "upstream_delivery",
        "external_side_effect",
    ] {
        assert_eq!(
            claims[cell]["status"], "incomplete",
            "{cell} must stay incomplete: a wire transcript carries no vantage that could upgrade it"
        );
    }
    for v in vectors() {
        assert_eq!(
            v["profile_baseline"],
            row_id.as_str(),
            "{} must reference the frozen row rather than restate a matrix",
            v["id"]
        );
    }
}

// --- Schema is per message and per era ------------------------------------------------------------

/// Result-schema acceptance, stated as exactly that. `ResultType` is `{"type":"string"}` with no
/// enum and `CallToolResult` does not set `additionalProperties:false`, so the definition accepts a
/// result claiming completion while carrying input requests.
///
/// This is a statement about one definition accepting one object. It is not whole-transcript
/// validity, and it is not a semantic verdict. If it ever fails, upstream has tightened the schema
/// and the contradiction has stopped being expressible: a finding, not a fix.
#[test]
fn the_result_definition_accepts_complete_alongside_input_requests() {
    let contradiction = result_payloads("complete-with-input-requests").remove(0);
    assert_eq!(contradiction["resultType"], "complete");
    assert!(contradiction.get("inputRequests").is_some());
    assert!(
        schema_accepts("2026-07-28", "CallToolResult", &contradiction),
        "the CallToolResult definition was expected to accept this object"
    );
}

/// `expect` is a closed vocabulary, decoded rather than compared.
///
/// A bare `expect == "valid"` makes every misspelling mean `invalid`, so a slip in that one word
/// asserts the opposite of what the vector says and still passes. The only two accepted values
/// are the two that mean something; anything else is a corpus defect and says so.
fn expect_valid(id: &str, slot: &str, spec: &Value) -> bool {
    match spec["expect"].as_str() {
        Some("valid") => true,
        Some("invalid") => false,
        other => panic!(
            "{id}/{slot}: expect must be \"valid\" or \"invalid\", got {other:?}; an unrecognised \
             value must never fall through to a verdict"
        ),
    }
}

/// Each vector's per-message schema expectation, checked against the pinned artifact for its own
/// era, every hop.
#[test]
fn each_vectors_schema_expectations_hold() {
    for v in vectors() {
        let id = v["id"].as_str().expect("id");
        let s = &v["schema"];
        let era = s["era"].as_str().expect("era");

        for (slot, docs) in [
            ("request_message", request_messages(id)),
            ("result_payload", result_payloads(id)),
        ] {
            let spec = &s[slot];

            // An undetermined era licenses no verdict, and the vocabulary is closed here too: the
            // pair must be exactly `not_applicable` with no definition, never a stray third value.
            if era == "ambiguous" {
                assert_eq!(spec["expect"], "not_applicable", "{id}/{slot}");
                assert!(spec["definition"].is_null(), "{id}/{slot}");
                continue;
            }

            let want_valid = expect_valid(id, slot, spec);
            let def = spec["definition"]
                .as_str()
                .unwrap_or_else(|| panic!("{id}/{slot}: a resolved era must name a definition"));
            assert!(!docs.is_empty(), "{id}/{slot}: nothing to validate");
            for (hop, doc) in docs.iter().enumerate() {
                assert_eq!(
                    schema_accepts(era, def, doc),
                    want_valid,
                    "{id}/{slot} hop {hop}: {def} under {era}"
                );
            }
        }
    }
}

/// Why the conflicting vector is ambiguous, pinned counterfactually rather than described.
///
/// The two artifacts do not disagree about the request. `2025-06-18` does not forbid the extra
/// `_meta` members, and the request carries exactly what `2026-07-28` requires, so it validates
/// under both. Only the result flips: `{"content": []}` satisfies the legacy `CallToolResult`,
/// which requires only `content`, and fails the modern one, which also requires `resultType`.
///
/// That is what makes reporting one verdict a choice rather than a reading, and it is asserted here
/// because the corpus previously said something else — that the legacy artifact rejected the
/// request — which was never true of these bytes.
#[test]
fn the_conflicting_vector_flips_only_on_the_result() {
    // Cardinality first: `remove(0)` would read the first of however many there are, so a fixture
    // that grew a second message would be scored on its first and the rest never looked at.
    let request = exactly_one("request", request_messages("conflicting-era-signals"))
        .unwrap_or_else(|e| panic!("conflicting-era-signals: {e}"));
    let result = exactly_one("result", result_payloads("conflicting-era-signals"))
        .unwrap_or_else(|e| panic!("conflicting-era-signals: {e}"));

    for (era, want) in [("2025-06-18", true), ("2026-07-28", true)] {
        assert_eq!(
            schema_accepts(era, "CallToolRequest", &request),
            want,
            "request under {era}"
        );
    }
    for (era, want) in [("2025-06-18", true), ("2026-07-28", false)] {
        assert_eq!(
            schema_accepts(era, "CallToolResult", &result),
            want,
            "result under {era}"
        );
    }
}

/// The two vectors whose era is undetermined state no schema verdict, and that is the honest
/// record. Reporting one would mean choosing an artifact the transcript does not license, which is
/// the same fault those vectors exist to catch.
#[test]
fn an_undetermined_era_states_no_schema_verdict() {
    for id in ["unknown-era", "conflicting-era-signals"] {
        let s = vector(id)["schema"].clone();
        assert_eq!(s["era"], "ambiguous", "{id}");
        for slot in ["request_message", "result_payload"] {
            assert_eq!(s[slot]["expect"], "not_applicable", "{id}/{slot}");
            assert!(s[slot]["definition"].is_null(), "{id}/{slot}");
        }
    }
}

// --- The manifest table, executable ---------------------------------------------------------------

/// What the manifest says a vector concludes, decoded into a value rather than left as prose.
enum Expected {
    Exact(ResultConclusion),
}

/// `unknown:<reason>`, decoded so that swapping the reason in the manifest changes what is asserted.
fn unknown_reason(id: &str, observation: &str) -> UnknownReason {
    let rest = observation
        .strip_prefix("unknown:")
        .unwrap_or_else(|| panic!("{id}: {observation:?} is not an unknown-era observation"));
    match rest {
        "no_signal" => UnknownReason::NoSignal,
        "malformed_signal" => UnknownReason::MalformedSignal,
        _ => match rest.strip_prefix("unsupported_version:") {
            Some(v) => UnknownReason::UnsupportedVersion(v.to_string()),
            None => panic!("{id}: unrecognised unknown reason {rest:?}"),
        },
    }
}

/// `conflicting:<header>/<body>`. The order is header-then-body and is not a convention this test
/// invents: the vector's `MCP-Protocol-Version` header carries the first, its `_meta` the second.
fn conflicting_versions(id: &str, observation: &str) -> (String, String) {
    let rest = observation
        .strip_prefix("conflicting:")
        .unwrap_or_else(|| panic!("{id}: {observation:?} is not a conflicting-era observation"));
    let (header, body) = rest
        .split_once('/')
        .unwrap_or_else(|| panic!("{id}: {rest:?} must name both versions"));
    (header.to_string(), body.to_string())
}

/// The coupling between the manifest's label and the enum. An unrecognised label is a defect, not a
/// skip: a table that silently ignores rows it does not understand is prose again.
fn expected_conclusion(v: &Value) -> Expected {
    let id = v["id"].as_str().expect("id");
    let era_observation = v["observation"]["era"].as_str().expect("observation.era");
    let label = v["conclusion"]
        .as_str()
        .unwrap_or_else(|| panic!("{id}: conclusion must be a string"));
    decode_conclusion(id, label, era_observation)
}

fn decode_conclusion(id: &str, label: &str, era_observation: &str) -> Expected {
    match label {
        "terminal" => Expected::Exact(ResultConclusion::Terminal),
        "non_terminal" => Expected::Exact(ResultConclusion::NonTerminal),
        "invalid:missing_result_type" => {
            Expected::Exact(ResultConclusion::Invalid(InvalidReason::MissingResultType))
        }
        "incomplete:unrecognized_result_type" => Expected::Exact(ResultConclusion::Incomplete(
            IncompleteReason::UnrecognizedResultType,
        )),
        "incomplete:era_unknown" => Expected::Exact(ResultConclusion::Incomplete(
            IncompleteReason::EraUnknown(unknown_reason(id, era_observation)),
        )),
        "invalid:era_conflicting" => {
            let (header, body) = conflicting_versions(id, era_observation);
            Expected::Exact(ResultConclusion::Invalid(InvalidReason::EraConflicting {
                header,
                body,
            }))
        }
        "incomplete:contradictory_result" => Expected::Exact(ResultConclusion::Incomplete(
            IncompleteReason::ContradictoryResult,
        )),
        "invalid:uncontinuable_input_required" => Expected::Exact(ResultConclusion::Invalid(
            InvalidReason::UncontinuableInputRequired,
        )),
        "incomplete:recognition_undeterminable" => Expected::Exact(ResultConclusion::Incomplete(
            IncompleteReason::RecognitionUndeterminable,
        )),
        other => panic!(
            "{id}: unrecognised conclusion {other:?}; the manifest and the enum have drifted apart"
        ),
    }
}

/// `observation.era`, decoded into the value the parser is expected to have resolved.
fn expected_era(id: &str, observation: &str) -> EraResolution {
    if let Some(v) = observation.strip_prefix("known:") {
        return EraResolution::Known(v.to_string());
    }
    if observation.starts_with("unknown:") {
        return EraResolution::Unknown(unknown_reason(id, observation));
    }
    if observation.starts_with("conflicting:") {
        let (header, body) = conflicting_versions(id, observation);
        return EraResolution::Conflicting { header, body };
    }
    panic!("{id}: unrecognised era observation {observation:?}")
}

/// What the manifest says the parser observed in the result.
fn expected_result(id: &str, observation: &str) -> ResultObservation {
    match observation {
        "missing" => ResultObservation::Missing,
        "complete" => ResultObservation::Complete,
        "input_required" => ResultObservation::InputRequired,
        "unrecognized" => ResultObservation::Unrecognized,
        "malformed" => ResultObservation::Malformed,
        "complete_with_continuation" => ResultObservation::CompleteWithContinuation,
        "input_required_without_continuation" => {
            ResultObservation::InputRequiredWithoutContinuation
        }
        other => panic!("{id}: unrecognised result observation {other:?}"),
    }
}

/// The conclusion for one call, judged against that call's own capability set.
fn actual_conclusion(ctx: &McpEraContext) -> ResultConclusion {
    conclude(
        &ctx.era,
        &ctx.result_observation.clone().expect("a result"),
        ctx.capability_observation.as_ref(),
    )
}

/// Whether `per_call` rows account for exactly the responses the transcript carries.
///
/// Iterating the rows alone proves only that each named id resolves. It does not prove the rows
/// *cover* the transcript: two rows naming one id score that response twice while a second response
/// goes unread, and the table still reports a full pass. Three ways to be wrong, all rejected here.
///
/// Returns the fault rather than panicking, so the rule itself is testable without a corpus that
/// breaks it.
fn per_call_coverage(
    declared: &[CorrelationId],
    responses: &[CorrelationId],
) -> Result<(), String> {
    let mut named = std::collections::HashSet::new();
    for d in declared {
        if !named.insert(d.clone()) {
            return Err(format!("per_call names id {d:?} more than once"));
        }
    }
    let mut answered = std::collections::HashSet::new();
    for r in responses {
        if !answered.insert(r.clone()) {
            return Err(format!("the transcript answers id {r:?} more than once"));
        }
    }
    let uncovered: Vec<&CorrelationId> = answered.difference(&named).collect();
    if !uncovered.is_empty() {
        return Err(format!("responses with no per_call row: {uncovered:?}"));
    }
    let unanswered: Vec<&CorrelationId> = named.difference(&answered).collect();
    if !unanswered.is_empty() {
        return Err(format!("per_call rows with no response: {unanswered:?}"));
    }
    Ok(())
}

/// The coverage rule, exercised directly on the shapes a corpus could drift into.
#[test]
fn per_call_coverage_rejects_duplicate_missing_and_extra_rows() {
    // Numeric keys here, because the shapes under test are about coverage rather than type; the
    // type distinction has its own vector and its own mutations.
    let s = |v: &[&str]| {
        v.iter()
            .map(|x| CorrelationId::Num(x.to_string()))
            .collect::<Vec<_>>()
    };

    assert!(per_call_coverage(&s(&["1", "2"]), &s(&["2", "1"])).is_ok());
    assert!(per_call_coverage(&s(&[]), &s(&[])).is_ok());

    // Each case isolates one guard and is asserted on the fault it must report. A case that trips
    // several guards at once proves nothing about any of them: the first to fire masks the rest, so
    // removing a later guard leaves the case still failing and the mutation still alive. Both the
    // duplicate-row and extra-row cases were that shape and survived their mutations.
    for (why, declared, responses, fault) in [
        (
            "a row naming an id twice, with coverage otherwise exact",
            s(&["1", "1", "2"]),
            s(&["1", "2"]),
            "names id Num(\"1\") more than once",
        ),
        (
            "a response no row accounts for",
            s(&["1"]),
            s(&["1", "2"]),
            "responses with no per_call row",
        ),
        (
            "a row naming a response the transcript does not carry, nothing else wrong",
            s(&["1", "2", "3"]),
            s(&["1", "2"]),
            "per_call rows with no response",
        ),
        (
            "a transcript answering one id twice",
            s(&["1", "2"]),
            s(&["1", "1", "2"]),
            "answers id Num(\"1\") more than once",
        ),
    ] {
        let got =
            per_call_coverage(&declared, &responses).expect_err(&format!("{why} must be refused"));
        assert!(
            got.contains(fault),
            "{why}: expected the fault {fault:?}, got {got:?}"
        );
    }
}

/// The control. Every vector is run through the parser and both its observed state and its
/// conclusion are compared to the decoded manifest row, every hop.
///
/// Observation is pinned as well as conclusion, and that is the load-bearing part: a right
/// conclusion reached from a wrongly observed input is a passing test that proves nothing. Pinning
/// only the outcome would let the parser misread a transcript and still score green because the
/// misreading happened to land on the same verdict.
///
/// Every settled row pins an exact value, so no single blanket answer can satisfy them: whichever
/// one it picks, it contradicts the rows that pin something else.
#[test]
fn every_vector_observes_and_concludes_exactly_what_the_manifest_says() {
    for v in vectors() {
        let id = v["id"].as_str().expect("id");
        let obs = &v["observation"];
        let want_era = expected_era(id, obs["era"].as_str().expect("observation.era"));
        let want_result = obs["result"].as_str().expect("observation.result");

        // A row states its conclusion once: either one label for every hop, or one per call. Both
        // at once would be two sources of truth that can disagree silently.
        let per_call = v.get("per_call").and_then(|p| p.as_array()).cloned();
        assert!(
            per_call.is_some() != v.get("conclusion").is_some(),
            "{id}: a vector carries exactly one of conclusion or per_call"
        );

        // Where calls are named, they are addressed by correlation id rather than by reading
        // position, so an out-of-order transcript is scored the same way it is parsed.
        let labelled: Vec<(String, McpEraContext, String)> = match &per_call {
            Some(rows) => {
                // Coverage before evaluation. Scoring the rows proves each named id resolves; it
                // does not prove the rows account for every response, and a table that can miss one
                // is not a control.
                let declared: Vec<CorrelationId> = rows
                    .iter()
                    .map(|r| manifest_correlation_id(id, &r["jsonrpc_id"]))
                    .collect();
                let answered: Vec<CorrelationId> = response_contexts_by_id(id)
                    .into_iter()
                    .map(|(key, _)| key)
                    .collect();
                if let Err(fault) = per_call_coverage(&declared, &answered) {
                    panic!("{id}: {fault}");
                }
                rows.iter()
                    .map(|r| {
                        let key = manifest_correlation_id(id, &r["jsonrpc_id"]);
                        let label = r["conclusion"].as_str().expect("conclusion").to_string();
                        let ctx = context_for_call(id, &key);
                        (format!("call {key:?}"), ctx, label)
                    })
                    .collect()
            }
            None => {
                let label = v["conclusion"].as_str().expect("conclusion").to_string();
                contexts(id)
                    .into_iter()
                    .enumerate()
                    .map(|(hop, ctx)| (format!("hop {hop}"), ctx, label.clone()))
                    .collect()
            }
        };
        assert!(!labelled.is_empty(), "{id}: no response to conclude on");

        for (where_, ctx, label) in labelled {
            assert_eq!(ctx.era, want_era, "{id} {where_}: era observation");

            let observed = ctx.result_observation.clone().expect("a result");
            assert_eq!(
                observed,
                expected_result(id, want_result),
                "{id} {where_}: result observation"
            );

            let Expected::Exact(want) =
                decode_conclusion(id, &label, obs["era"].as_str().expect("observation.era"));
            assert_eq!(actual_conclusion(&ctx), want, "{id} {where_}: conclusion");
        }
    }
}

/// The capability observation is a state of its own, and the three vectors are otherwise identical:
/// same envelope, same era, same request metadata, same result token. Everything but the capability
/// set is asserted equal, so the state is the only thing that can be carrying the difference.
///
/// A present-and-empty set, an absent one, and an unread extension are three findings, not two and
/// a fold.
#[test]
fn the_capability_observation_is_a_state_of_its_own() {
    let core_only = sole_context("capabilities-core-only-noncore-token");
    let absent = sole_context("capabilities-absent-noncore-token");
    let extension = sole_context("capabilities-unknown-extension");

    for (id, other, want) in [
        (
            "capabilities-absent-noncore-token",
            &absent,
            CapabilityObservation::Absent,
        ),
        (
            "capabilities-unknown-extension",
            &extension,
            CapabilityObservation::ExtensionNotUnderstood,
        ),
    ] {
        assert_eq!(core_only.envelope, other.envelope, "{id}: envelope");
        assert_eq!(core_only.era, other.era, "{id}: era");
        assert_eq!(
            core_only.request_metadata, other.request_metadata,
            "{id}: all three carry the same protocolVersion, so this must not be the field that \
             distinguishes them"
        );
        assert_eq!(
            core_only.result_observation, other.result_observation,
            "{id}: the result token is the same in all three; the difference is upstream of it"
        );
        assert_eq!(other.capability_observation, Some(want), "{id}");
    }

    assert_eq!(
        core_only.capability_observation,
        Some(CapabilityObservation::CoreOnly),
        "a present and empty set is a complete statement, not silence"
    );
}

/// An orphan response under a revision that defines the capability set cannot reach the closed
/// answer.
///
/// Composite rather than a unit read, because the point is what the *parser* leaves behind: a
/// response with no correlated request keeps `capability_observation: None`, and borrowing a
/// neighbouring call's set is the inference the revision forbids. From there, `None` under 2026
/// must not become "nothing advertised covers this token" — no set was ever seen, so that claim has
/// no ground. Absence of evidence is not evidence.
///
/// Not a corpus vector: every row there pairs a request with its response, and the missing request
/// is this case's whole content.
#[test]
fn an_orphan_response_under_2026_cannot_close_the_recognition_question() {
    let text = r#"{"transport":"streamable-http",
      "transport_context":{"headers":{"MCP-Protocol-Version":"2026-07-28"}},
      "entries":[{"timestamp_ms":1000,"response":{"jsonrpc":"2.0","id":9,
        "result":{"resultType":"io.vendor/unknown","content":[]}}}]}"#;
    let mut all: Vec<McpEraContext> =
        parse_mcp_transcript_detailed(text, McpInputFormat::StreamableHttp)
            .expect("parses")
            .into_iter()
            .filter(|p| p.context.result_observation.is_some())
            .map(|p| p.context)
            .collect();
    assert_eq!(all.len(), 1, "one orphan response");
    let ctx = all.remove(0);

    assert_eq!(ctx.era, EraResolution::Known("2026-07-28".to_string()));
    assert_eq!(
        ctx.result_observation,
        Some(ResultObservation::Unrecognized)
    );
    assert_eq!(
        ctx.capability_observation, None,
        "no request was correlated, so nothing was observed; this is not `Some(Absent)`"
    );
    assert_eq!(
        actual_conclusion(&ctx),
        ResultConclusion::Incomplete(IncompleteReason::RecognitionUndeterminable),
        "an unseen capability set cannot establish that nothing advertised covers the token"
    );
}

/// Under a revision with no capability member there was nothing that could have been advertised, so
/// the question was never open and the closed answer still holds. Kept explicit so the branch above
/// cannot be widened into revisions it was never about.
#[test]
fn a_legacy_era_keeps_the_closed_answer_without_a_capability_set() {
    assert_eq!(
        conclude(
            &EraResolution::Known("2025-06-18".to_string()),
            &ResultObservation::Unrecognized,
            None,
        ),
        ResultConclusion::Incomplete(IncompleteReason::UnrecognizedResultType)
    );
}

/// A legacy transcript, parsed end to end, keeps the pre-slice answer.
///
/// The capability axis reads a `_meta` key namespaced for a later revision. An ordinary 2025 call
/// has no reason to carry it, and the observer cannot tell "this revision does not define the
/// member" from "this request omitted it", so it reports `Absent` either way. The gate is what
/// makes that harmless: applied to a legacy call, `Absent` would turn an ordinary conforming
/// request into an open question and manufacture a finding out of conformance.
///
/// The second row is the sharper one. A `clientCapabilities` value that is malformed *by 2026
/// rules* says nothing about a 2025 call, so it must not reach the fault arm and must not change
/// the verdict — a future field cannot invalidate a legacy result.
#[test]
fn a_legacy_call_is_not_judged_against_the_capability_contract() {
    for (why, caps_member) in [
        ("ordinary legacy _meta, no capability member at all", ""),
        (
            "a future capability key that is malformed by 2026 rules",
            r#","io.modelcontextprotocol/clientCapabilities":"not-an-object""#,
        ),
    ] {
        let text = format!(
            r#"{{"transport":"streamable-http",
              "transport_context":{{"headers":{{"MCP-Protocol-Version":"2025-06-18"}}}},
              "entries":[
                {{"timestamp_ms":1000,"request":{{"jsonrpc":"2.0","id":1,"method":"tools/call",
                  "params":{{"name":"Calc","arguments":{{}},"_meta":{{
                    "io.modelcontextprotocol/protocolVersion":"2025-06-18"{caps_member}}}}}}}}},
                {{"timestamp_ms":1001,"response":{{"jsonrpc":"2.0","id":1,
                  "result":{{"resultType":"io.vendor/unknown","content":[]}}}}}}]}}"#
        );
        let mut all: Vec<McpEraContext> =
            parse_mcp_transcript_detailed(&text, McpInputFormat::StreamableHttp)
                .unwrap_or_else(|e| panic!("{why}: must parse: {e:?}"))
                .into_iter()
                .filter(|p| p.context.result_observation.is_some())
                .map(|p| p.context)
                .collect();
        assert_eq!(all.len(), 1, "{why}");
        let ctx = all.remove(0);

        assert_eq!(
            ctx.era,
            EraResolution::Known("2025-06-18".to_string()),
            "{why}"
        );
        assert_eq!(
            ctx.result_observation,
            Some(ResultObservation::Unrecognized),
            "{why}"
        );
        assert_eq!(
            actual_conclusion(&ctx),
            ResultConclusion::Incomplete(IncompleteReason::UnrecognizedResultType),
            "{why}: a revision that does not define the capability contract is not judged by it"
        );
    }
}

/// A request is assessed on its own terms, and a modern one must state its capability set.
///
/// `RequestMetaObject.required` names `protocolVersion` and `clientCapabilities` both, so a version
/// alone leaves the metadata incomplete. This runs parser to era to request assessment, and the
/// third row is why it cannot wait for the response side: a refused or abandoned call never gets a
/// response, so the request is the only record that will ever exist for it.
#[test]
fn a_modern_request_must_state_its_capability_set() {
    fn assess(text: &str) -> Vec<RequestAssessment> {
        parse_mcp_transcript_detailed(text, McpInputFormat::StreamableHttp)
            .unwrap_or_else(|e| panic!("must parse: {e:?}"))
            .into_iter()
            .filter(|p| p.context.request_metadata.is_some())
            .map(|p| {
                conclude_request(
                    &p.context.era,
                    p.context.request_metadata.as_ref().expect("metadata"),
                    p.context.capability_observation.as_ref(),
                )
            })
            .collect()
    }

    for (why, meta, want) in [
        (
            "a stated core-only set is a complete statement",
            r#""io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}"#,
            RequestAssessment::Valid,
        ),
        (
            "the version alone leaves a required member unstated",
            r#""io.modelcontextprotocol/protocolVersion":"2026-07-28""#,
            RequestAssessment::Invalid(InvalidReason::MissingCapabilities),
        ),
        (
            "a set that arrived and could not be read is a fault, not an absence",
            r#""io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":7"#,
            RequestAssessment::Invalid(InvalidReason::MalformedCapabilities),
        ),
    ] {
        // No response at all: the request is the whole transcript.
        let text = format!(
            r#"{{"transport":"streamable-http",
              "transport_context":{{"headers":{{"MCP-Protocol-Version":"2026-07-28"}}}},
              "entries":[{{"timestamp_ms":1000,"request":{{"jsonrpc":"2.0","id":1,
                "method":"tools/call","params":{{"name":"Calc","arguments":{{}},
                "_meta":{{{meta}}}}}}}}}]}}"#
        );
        assert_eq!(assess(&text), vec![want], "{why}");
    }
}

/// The same contract does not reach back to a revision that never defined it.
#[test]
fn a_legacy_request_is_not_held_to_the_capability_contract() {
    for (why, capability) in [
        (
            "no capability member at all",
            Some(CapabilityObservation::Absent),
        ),
        ("nothing observed", None),
        (
            "a future key that is malformed by 2026 rules",
            Some(CapabilityObservation::Malformed),
        ),
    ] {
        assert_eq!(
            conclude_request(
                &EraResolution::Known("2025-06-18".to_string()),
                &RequestMetadata::Present("2025-06-18".to_string()),
                capability.as_ref(),
            ),
            RequestAssessment::Valid,
            "{why}"
        );
    }
}

/// A request carrying one capability set, for the reads below.
fn request_advertising(caps: Value) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": "Calc", "arguments": {}, "_meta": {
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientCapabilities": caps,
        }},
    })
}

/// `ClientCapabilities` defines five members and does not set `additionalProperties: false`, so
/// "beyond core" is wider than the extensions map alone.
///
/// The rows that must not read as core are the whole point: a non-empty `experimental` map, and a
/// member this build has never heard of. Reporting either as `CoreOnly` would turn unknown
/// vocabulary into the closed answer that nothing advertised covers the token, which is a claim
/// about a set this build could not read.
#[test]
fn beyond_core_is_wider_than_the_extensions_map() {
    for (why, caps, want) in [
        (
            "an empty set advertises nothing",
            serde_json::json!({}),
            CapabilityObservation::CoreOnly,
        ),
        (
            "core members this build has rules for",
            serde_json::json!({"roots": {}, "sampling": {}, "elicitation": {}}),
            CapabilityObservation::CoreOnly,
        ),
        (
            "both open maps present and empty is a complete statement that none is offered",
            serde_json::json!({"experimental": {}, "extensions": {}}),
            CapabilityObservation::CoreOnly,
        ),
        (
            "a non-empty experimental map is a capability with no rule here",
            serde_json::json!({"experimental": {"io.vendor/try": {}}}),
            CapabilityObservation::ExtensionNotUnderstood,
        ),
        (
            "a non-empty extensions map, the same read one member over",
            serde_json::json!({"extensions": {"io.vendor/partial-results": {}}}),
            CapabilityObservation::ExtensionNotUnderstood,
        ),
        (
            "an unrecognised top-level member is legal and unevaluable, never core",
            serde_json::json!({"roots": {}, "io.vendor/whatever": {}}),
            CapabilityObservation::ExtensionNotUnderstood,
        ),
        (
            "a known member that is not an object is a broken statement, not silence",
            serde_json::json!({"roots": "yes"}),
            CapabilityObservation::Malformed,
        ),
        (
            "an open map that is not a map is unreadable for the same reason",
            serde_json::json!({"extensions": []}),
            CapabilityObservation::Malformed,
        ),
    ] {
        assert_eq!(
            observe_client_capabilities(&request_advertising(caps)),
            Some(want),
            "{why}"
        );
    }
}

/// No observation and an absent member are different facts, and the observer keeps them apart.
///
/// `None` is reached wherever there was nothing to look in: no `params`, or no `_meta` inside it.
/// `Some(Absent)` is reached only when the container was readable and the capability member was not
/// in it. They agree on today's conclusion under both eras, so this discriminates them where the
/// difference actually lives — in what was observed. Carrying it is what stops a later fold from
/// reporting silence about the question as an answer to it.
#[test]
fn no_capability_observation_is_not_the_same_as_an_absent_member() {
    for (why, raw) in [
        (
            "no params at all",
            serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call"}),
        ),
        (
            "params present, no _meta to look in",
            serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                               "params": {"name": "Calc", "arguments": {}}}),
        ),
    ] {
        assert_eq!(
            observe_client_capabilities(&raw),
            None,
            "{why}: nothing was observed, which is not a statement that nothing was advertised"
        );
    }

    let readable = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "Calc", "arguments": {}, "_meta": {
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        }},
    });
    assert_eq!(
        observe_client_capabilities(&readable),
        Some(CapabilityObservation::Absent),
        "the container was readable and the member was not in it"
    );
}

/// A capability set that arrived and could not be read is a fault, not an absence. Asserted
/// directly rather than through a vector: the corpus describes transcripts a conforming client
/// could send, and this shape is one no schema accepts.
#[test]
fn an_unreadable_capability_set_is_invalid_rather_than_absent() {
    let raw = request_advertising(serde_json::json!("not-an-object"));
    assert_eq!(
        observe_client_capabilities(&raw),
        Some(CapabilityObservation::Malformed)
    );
    assert_eq!(
        conclude(
            &EraResolution::Known("2026-07-28".to_string()),
            &ResultObservation::Unrecognized,
            Some(&CapabilityObservation::Malformed),
        ),
        ResultConclusion::Invalid(InvalidReason::MalformedCapabilities),
        "more evidence does not make an unreadable value readable"
    );
}

/// Capabilities are stated per request and MUST NOT be inferred from a prior one, so the binding
/// has to survive a transcript that answers out of order.
///
/// Two calls on distinct ids advertise different capability sets; their responses arrive reversed
/// and carry the *same* unrecognized token. The token being identical is what makes this
/// load-bearing: nothing about the response bytes can separate the two conclusions, so only the
/// binding to each response's own request can.
///
/// The two exact expectations together rule out both cheap implementations. Transcript-global
/// capability state gives one answer to both responses and fails whichever row disagrees with it.
/// Last-seen state binds request 2's capabilities to both responses, because request 2 is the most
/// recent request when either response is read, and fails the row for id 1.
#[test]
fn capabilities_bind_to_their_own_call_across_interleaved_responses() {
    let vector_id = "per-request-capabilities-interleaved";
    let rows = vector(vector_id)["per_call"]
        .as_array()
        .expect("per_call")
        .clone();
    assert_eq!(rows.len(), 2, "the vector is a pair by construction");

    // Source order is the reverse of call order, so passing by reading position would be wrong.
    let arrival: Vec<CorrelationId> = response_contexts_by_id(vector_id)
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    assert_eq!(
        arrival,
        [
            CorrelationId::Num("2".to_string()),
            CorrelationId::Num("1".to_string())
        ],
        "the responses must arrive out of order or the test proves nothing"
    );

    let era_observation = vector(vector_id)["observation"]["era"]
        .as_str()
        .expect("observation.era")
        .to_string();

    for row in rows {
        let key = manifest_correlation_id(vector_id, &row["jsonrpc_id"]);
        let label = row["conclusion"].as_str().expect("conclusion");
        let ctx = context_for_call(vector_id, &key);

        // Both responses carry the same token, so the observation cannot be what separates them.
        assert_eq!(
            ctx.result_observation,
            Some(ResultObservation::Unrecognized),
            "{vector_id}/{key:?}: both responses carry the same unrecognized token"
        );

        let expected_capability = match row["capabilities"].as_str().expect("capabilities") {
            "core_only" => CapabilityObservation::CoreOnly,
            "extension_not_understood" => CapabilityObservation::ExtensionNotUnderstood,
            "absent" => CapabilityObservation::Absent,
            other => panic!("{vector_id}/{key:?}: unrecognised capability {other:?}"),
        };
        assert_eq!(
            ctx.capability_observation,
            Some(expected_capability),
            "{vector_id}/{key:?}: this response carries its own request's capability set"
        );

        let Expected::Exact(want) = decode_conclusion(vector_id, label, &era_observation);
        assert_eq!(
            actual_conclusion(&ctx),
            want,
            "{vector_id}/{key:?}: this response must be judged against its own request's \
             capabilities, not the transcript's or the most recent one's"
        );
    }
}

/// The advertised extension name equals the result token in that vector, which is the shape most
/// likely to tempt an invented mapping. Neither may be retained.
#[test]
fn the_interleaved_vector_retains_no_capability_or_token_bytes() {
    for (key, ctx) in response_contexts_by_id("per-request-capabilities-interleaved") {
        let rendered = format!("{ctx:?}");
        assert!(
            !rendered.contains("io.vendor/partial-results"),
            "call {key:?}: the token and the extension name are attacker-chosen and must not be \
             retained: {rendered}"
        );
    }
}

/// The capability label is a closed vocabulary too, and it is carried by exactly the vectors whose
/// test above depends on it. Without this the label could be quietly renamed and the pending
/// coupling would go looking for a state nobody described.
#[test]
fn the_capability_observation_vocabulary_is_closed() {
    fn check(id: &str, c: &Value) {
        match c.as_str() {
            Some("core_only" | "absent" | "extension_not_understood") => {}
            other => panic!("{id}: unrecognised capability observation {other:?}"),
        }
    }

    let mut carriers: Vec<String> = Vec::new();
    for v in vectors() {
        let id = v["id"].as_str().expect("id").to_string();

        // Per-call rows are the same vocabulary, checked at the same closure: a label that only
        // appears inside per_call must not escape the check by living one level deeper.
        if let Some(rows) = v.get("per_call").and_then(|p| p.as_array()) {
            for r in rows {
                check(&id, &r["capabilities"]);
            }
            carriers.push(id);
            continue;
        }

        if let Some(c) = v["observation"].get("capabilities") {
            check(&id, c);
            carriers.push(id);
        }
    }
    carriers.sort();
    assert_eq!(
        carriers,
        [
            "capabilities-absent-noncore-token",
            "capabilities-core-only-noncore-token",
            "capabilities-unknown-extension",
            "correlation-id-type-collision",
            "modern-unrecognized-token",
            "per-request-capabilities-interleaved",
        ],
        "the capability arm is exactly these vectors"
    );
}

/// The advertised extension name is attacker-chosen and must never travel into a finding, so no
/// capability state may retain it.
#[test]
fn no_capability_state_echoes_an_advertised_extension_name() {
    let rendered = format!("{:?}", sole_context("capabilities-unknown-extension"));
    assert!(
        !rendered.contains("io.vendor/partial-results"),
        "the extension name is attacker-chosen and value-free by design: {rendered}"
    );
}

// --- The semantic layer, where the schema cannot reach ---------------------------------------------

/// The load-bearing edge, on the conclusion side. The result definition accepts these bytes, and
/// terminal is the one conclusion they must never license.
///
/// Both continuation forms, not one. `InputRequiredResult` requires at least one of `inputRequests`
/// or `requestState`, so a rule reading only the first leaves a transcript able to state the
/// contradiction in a shape this build cannot see — and the two are asserted separately so a
/// mutation to either member has somewhere to fail.
#[test]
fn a_schema_valid_contradiction_is_not_terminal() {
    for (id, member) in [
        ("complete-with-input-requests", "inputRequests"),
        ("complete-with-request-state", "requestState"),
    ] {
        let ctx = sole_context(id);
        assert_eq!(
            ctx.result_observation,
            Some(ResultObservation::CompleteWithContinuation),
            "{id}: a completion claim beside {member} is observed as its own state, not as plain \
             completion"
        );
        assert_eq!(
            actual_conclusion(&ctx),
            ResultConclusion::Incomplete(IncompleteReason::ContradictoryResult),
            "{id}: it concludes with its own reason"
        );
    }
}

/// The interim arm of the same gap, and the sharper half of it.
///
/// `InputRequiredResult` requires at least one of `inputRequests` or `requestState` in prose, and
/// its JSON Schema encodes nothing of the sort: `required` lists only `resultType`. So the pinned
/// definition accepts a result that asks for input while offering no way to supply it, and that is
/// asserted here rather than assumed — the schema layer is shown saying yes on the same bytes the
/// conclusion layer refuses.
///
/// The conclusion is `Invalid`, not `NonTerminal` and not `Incomplete`. `NonTerminal` would call a
/// dead exchange validly unfinished; `Incomplete` would suggest more evidence could settle it, and
/// nothing arriving later makes this continuable.
///
/// The explicit-null twin is here for the same reason it exists on the completion side: an absent
/// member and a null one are the same silence, so a rule counting presence rather than content
/// would pass one and fail the other.
#[test]
fn an_input_required_result_with_no_continuation_is_invalid() {
    for (id, why) in [
        (
            "input-required-without-continuation",
            "neither member present",
        ),
        (
            "input-required-null-continuation",
            "both members explicitly null",
        ),
    ] {
        let payload = result_payloads(id).remove(0);
        assert_eq!(payload["resultType"], "input_required", "{id}");

        let ctx = sole_context(id);
        assert_eq!(
            ctx.result_observation,
            Some(ResultObservation::InputRequiredWithoutContinuation),
            "{id}: {why}"
        );
        assert_eq!(
            actual_conclusion(&ctx),
            ResultConclusion::Invalid(InvalidReason::UncontinuableInputRequired),
            "{id}: a request for input that cannot be answered is not a valid interim result"
        );
    }

    // The gap itself: the definition carrying the prose requirement accepts the bytes that
    // violate it. If this ever fails, upstream has encoded the MUST and the finding is closed.
    assert!(
        schema_accepts(
            "2026-07-28",
            "InputRequiredResult",
            &result_payloads("input-required-without-continuation").remove(0)
        ),
        "InputRequiredResult was expected to accept a result with no continuation member"
    );
}

/// A continuation member that arrived in a shape that cannot carry a call forward.
///
/// Parser to observation to conclusion, through a real transcript, because this is exactly where
/// schema parity does not help: the runtime conclusion path never validates against the vendored
/// artifact, so a `requestState` that is a number or an `inputRequests` that is an array reaches
/// the conclusion layer unchallenged. Presence alone used to license a valid interim result, which
/// would have travelled a call that cannot be continued as one that can.
///
/// Only the top-level type is judged. Re-deriving `InputRequest` here would copy a published
/// definition into the test and go stale against it; whether the member could carry a call forward
/// at all is answered without reading any deeper.
#[test]
fn a_continuation_member_of_the_wrong_type_is_invalid() {
    for (why, member) in [
        ("requestState must be a string", r#""requestState":7"#),
        ("inputRequests must be an object", r#""inputRequests":[]"#),
        (
            "a broken member is a fault even beside a well-formed sibling",
            r#""requestState":"s1","inputRequests":[]"#,
        ),
    ] {
        let ctx = sole_context_from(&interim_transcript(member));
        assert_eq!(
            ctx.result_observation,
            Some(ResultObservation::InputRequiredWithMalformedContinuation),
            "{why}"
        );
        assert_eq!(
            actual_conclusion(&ctx),
            ResultConclusion::Invalid(InvalidReason::MalformedContinuation),
            "{why}: a member that cannot be read is not a way to continue"
        );
    }

    // The completion arm folds the same shape into the contradiction instead. That is a different
    // answer for a reason: folding is fail-closed there and would be fail-open here.
    let complete = sole_context_from(
        &interim_transcript(r#""requestState":7"#).replace("input_required", "complete"),
    );
    assert_eq!(
        complete.result_observation,
        Some(ResultObservation::CompleteWithContinuation)
    );
    assert_eq!(
        actual_conclusion(&complete),
        ResultConclusion::Incomplete(IncompleteReason::ContradictoryResult),
        "a completion claim carrying an unreadable continuation must not reach terminal"
    );
}

/// A 2026 call whose result carries the given raw continuation members.
fn interim_transcript(members: &str) -> String {
    format!(
        r#"{{"transport":"streamable-http",
          "transport_context":{{"headers":{{"MCP-Protocol-Version":"2026-07-28"}}}},
          "entries":[
            {{"timestamp_ms":1000,"request":{{"jsonrpc":"2.0","id":1,"method":"tools/call",
              "params":{{"name":"Calc","arguments":{{}},"_meta":{{
                "io.modelcontextprotocol/protocolVersion":"2026-07-28",
                "io.modelcontextprotocol/clientCapabilities":{{}}}}}}}}}},
            {{"timestamp_ms":1001,"response":{{"jsonrpc":"2.0","id":1,
              "result":{{"resultType":"input_required","content":[],{members}}}}}}}]}}"#
    )
}

fn sole_context_from(text: &str) -> McpEraContext {
    let mut all: Vec<McpEraContext> =
        parse_mcp_transcript_detailed(text, McpInputFormat::StreamableHttp)
            .unwrap_or_else(|e| panic!("must parse: {e:?}"))
            .into_iter()
            .filter(|p| p.context.result_observation.is_some())
            .map(|p| p.context)
            .collect();
    assert_eq!(all.len(), 1, "one response");
    all.remove(0)
}

/// The continuation members are read independently, so neither can be carried by the other.
///
/// A result claiming completion beside exactly one of them is still the contradiction; a plain
/// completion beside neither is not. Asserted directly because the corpus vectors each carry one
/// member, and this pins that reading one is not standing in for reading both.
#[test]
fn either_continuation_member_alone_is_the_contradiction() {
    let observe = |result: Value| {
        observe_result(&serde_json::json!({"jsonrpc": "2.0", "id": 1, "result": result}))
    };
    for (why, result) in [
        (
            "inputRequests alone",
            serde_json::json!({"resultType": "complete", "content": [],
                               "inputRequests": {"elicitation": {"method": "elicitation/create",
                                                                 "params": {}}}}),
        ),
        (
            "requestState alone",
            serde_json::json!({"resultType": "complete", "content": [], "requestState": "s1"}),
        ),
        (
            "an empty continuation value still disagrees with the completion claim",
            serde_json::json!({"resultType": "complete", "content": [], "requestState": ""}),
        ),
    ] {
        assert_eq!(
            observe(result),
            Some(ResultObservation::CompleteWithContinuation),
            "{why}"
        );
    }
    for (why, result) in [
        (
            "neither member is plain completion",
            serde_json::json!({"resultType": "complete", "content": []}),
        ),
        (
            "an explicit null is silence, not a continuation",
            serde_json::json!({"resultType": "complete", "content": [],
                               "inputRequests": null, "requestState": null}),
        ),
    ] {
        assert_eq!(observe(result), Some(ResultObservation::Complete), "{why}");
    }
}

/// Recognition is capability-relative, so the same unreadable token is a different finding
/// depending on whether the capability set was there to read.
#[test]
fn recognition_is_capability_relative() {
    let known = sole_context("capabilities-core-only-noncore-token");
    let undeterminable = sole_context("capabilities-absent-noncore-token");
    let unknown_ext = sole_context("capabilities-unknown-extension");

    // The present-capabilities arm is pinned positively, not merely as "different from the other".
    assert_eq!(
        actual_conclusion(&known),
        ResultConclusion::Incomplete(IncompleteReason::UnrecognizedResultType),
        "a present core-only capability set makes the token known-unrecognized"
    );

    for (id, ctx) in [
        ("capabilities-absent-noncore-token", &undeterminable),
        ("capabilities-unknown-extension", &unknown_ext),
    ] {
        assert_eq!(
            actual_conclusion(ctx),
            ResultConclusion::Incomplete(IncompleteReason::RecognitionUndeterminable),
            "{id}: recognition is undeterminable here and carries its own reason"
        );
    }

    assert_eq!(
        actual_conclusion(&undeterminable),
        actual_conclusion(&unknown_ext),
        "an advertised extension this build does not read is undeterminable for the same reason: \
         no mapping from extension name to result token may be invented"
    );
}

#[test]
fn an_interim_result_never_lifts_delivery_out_of_incomplete() {
    let ctx = sole_context("modern-input-required");
    let observed = ctx.result_observation.clone().expect("a result");
    assert_eq!(observed, ResultObservation::InputRequired);
    assert_eq!(actual_conclusion(&ctx), ResultConclusion::NonTerminal);
}

/// Every hop is its own record, and nothing joins them. `requestState` is an opaque continuation
/// token the client echoes back, not an identity Assay may pair on.
#[test]
fn multi_hop_keeps_two_records_and_infers_no_pairing() {
    let all = contexts("multi-hop-input-required");
    assert_eq!(all.len(), 2, "each hop is its own response record");
    for (i, ctx) in all.iter().enumerate() {
        let observed = ctx.result_observation.clone().expect("a result");
        assert_eq!(observed, ResultObservation::InputRequired, "hop {i}");
        assert_eq!(
            actual_conclusion(ctx),
            ResultConclusion::NonTerminal,
            "hop {i}"
        );
    }
    let doc = transcript("multi-hop-input-required");
    let ids: Vec<&Value> = doc["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter_map(|e| e.get("response").and_then(|r| r.get("id")))
        .collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(
        ids[0], ids[1],
        "the hops are distinct calls, not one call twice"
    );
    assert!(
        vector("multi-hop-input-required")["observation"]["inferred_pairing"] == false,
        "the corpus must not claim a pairing it does not have"
    );
}

/// Correlation metadata changes correlation and nothing else. Both transcripts are parsed and their
/// actual conclusions compared, so this fails the moment traceparent starts moving behaviour.
#[test]
fn traceparent_changes_no_conclusion() {
    let plain = sole_context("modern-complete");
    let traced = sole_context("traceparent-present");
    assert_eq!(plain.era, traced.era);
    assert_eq!(plain.result_observation, traced.result_observation);
    assert_eq!(
        actual_conclusion(&plain),
        actual_conclusion(&traced),
        "a traceparent is a correlation hint and never evidence"
    );
    assert_eq!(
        vector("modern-complete")["profile_baseline"],
        vector("traceparent-present")["profile_baseline"]
    );
}

#[test]
fn unknown_and_conflicting_eras_stay_distinguishable() {
    let unknown = sole_context("unknown-era");
    let conflicting = sole_context("conflicting-era-signals");
    assert!(matches!(unknown.era, EraResolution::Unknown(_)));
    assert!(matches!(conflicting.era, EraResolution::Conflicting { .. }));
    assert_ne!(
        conclude(&unknown.era, &ResultObservation::Missing, None),
        conclude(&conflicting.era, &ResultObservation::Missing, None),
        "unknown may become conclusive with more evidence; contradicted will not"
    );
}

/// The parity claim itself: equivalent calls reach the same conclusion and reference the same row.
#[test]
fn equivalent_calls_conclude_alike() {
    for pair in manifest()["equivalence_pairs"].as_array().expect("pairs") {
        let ids = pair["pair"].as_array().expect("pair");
        let (a, b) = (ids[0].as_str().expect("a"), ids[1].as_str().expect("b"));
        let (ca, cb) = (
            actual_conclusion(&sole_context(a)),
            actual_conclusion(&sole_context(b)),
        );
        assert_eq!(
            ca, cb,
            "{a} and {b} are declared equivalent and must conclude alike"
        );

        // Equality alone is satisfied by two identical wrong answers, so the pair is also pinned to
        // the value the manifest states. Both sides carry the same label, and that is checked too:
        // an equivalence pair whose members disagree on paper is a corpus defect.
        assert_eq!(
            vector(a)["conclusion"],
            vector(b)["conclusion"],
            "{a} and {b} are declared equivalent but the manifest gives them different conclusions"
        );
        let Expected::Exact(want) = expected_conclusion(&vector(a));
        assert_eq!(ca, want, "{a}/{b}");
        assert_eq!(vector(a)["profile_baseline"], vector(b)["profile_baseline"]);
    }
}
