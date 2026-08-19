use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::jcs;

/// Cryptographic identity of a tool based on its name, server, and schema.
/// This prevents "Tool Poisoning" where an attacker modifies a tool definition
/// to inject different instructions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ToolIdentity {
    pub server_id: String,
    pub tool_name: String,
    /// Hash of the full input schema (JSON)
    pub schema_hash: String,
    /// Hash of the description and metadata
    pub meta_hash: String,
}

impl ToolIdentity {
    pub fn new(
        server_id: &str,
        tool_name: &str,
        schema: &Option<serde_json::Value>,
        description: &Option<String>,
    ) -> Self {
        let schema_hash = compute_json_hash(schema);
        let meta_hash = compute_string_hash(description.as_deref().unwrap_or(""));

        Self {
            server_id: server_id.to_string(),
            tool_name: tool_name.to_string(),
            schema_hash,
            meta_hash,
        }
    }

    /// Returns a short fingerprint for display/logging.
    pub fn fingerprint(&self) -> String {
        format!(
            "{}:{}:{}",
            self.server_id,
            self.tool_name,
            &self.schema_hash[0..8]
        )
    }
}

/// Hash a schema over its RFC 8785 canonical bytes.
///
/// This asks the same question `tool_definition`'s binding digest asks — "what is this JSON,
/// byte-exactly" — so it goes through the same `jcs` canonicalizer rather than answering it a
/// second way. `serde_json::to_string` was not an answer to it at all: `assay-evidence` enables
/// serde_json's `preserve_order`, resolver-2 unification hands this crate the same
/// insertion-ordered `Map`, and a producer that reordered its keys or re-emitted `100` as `1e2`
/// therefore drifted every pinned tool (#2229).
fn compute_json_hash(val: &Option<serde_json::Value>) -> String {
    let mut hasher = Sha256::new();
    match val {
        // The two error paths of the JCS serializer are a non-finite float and a non-UTF-8 object
        // key; a `Value` holds finite `Number`s and `String` keys and can express neither, which
        // `canonicalization_succeeds_across_the_value_domain` exercises. The workspace sets
        // `panic = "abort"`, so this is an abort of a process in the agent's data path and not a
        // recoverable failure — chosen anyway, because the alternatives are worse in a direction
        // that matters more: a default hash gives two failures one fingerprint, and an absent
        // identity reads as "no pin to check" in `tool_drift_decision`. Both convert a failure
        // into a missed drift. A failure shaped as a deny would beat all three, but it means
        // threading a `Result` through `ToolIdentity::new` and every caller, which is a larger
        // change than the hashing and buys a branch no reachable input takes.
        Some(v) => {
            hasher.update(jcs::to_vec(v).expect("a serde_json::Value is always JCS-serializable"))
        }
        None => hasher.update(b"null"),
    }
    hex::encode(hasher.finalize())
}

fn compute_string_hash(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn schema(json: &str) -> Option<Value> {
        Some(serde_json::from_str(json).expect("test schema must parse"))
    }

    fn identity_for(json: &str) -> ToolIdentity {
        ToolIdentity::new("srv", "read_file", &schema(json), &Some("d".to_string()))
    }

    /// The premise the key-order tests depend on: `assay-evidence` turns on serde_json's
    /// `preserve_order`, and resolver-2 feature unification gives this crate the same
    /// insertion-ordered `Map`. Without this, `serde_json::to_string` would already sort and the
    /// reorder tests below would pass for the wrong reason.
    #[test]
    fn premise_serde_json_map_preserves_insertion_order_in_this_build() {
        let value: Value = serde_json::from_str(r#"{"b":1,"a":2}"#).expect("parses");
        assert_eq!(
            serde_json::to_string(&value).expect("serializes"),
            r#"{"b":1,"a":2}"#,
            "serde_json is sorting keys here, so `preserve_order` is not in effect and the \
             reorder tests below no longer prove anything"
        );
    }

    /// The premise behind `compute_json_hash`'s `expect` and behind
    /// `canonicalization_succeeds_across_the_value_domain`: a parsed `Value` cannot hold a
    /// non-finite number, because a literal outside `f64` range does not parse.
    /// `arbitrary_precision` would retain the literal, `serde_jcs` would reject it, and the
    /// `expect` would abort.
    #[test]
    fn premise_out_of_range_number_literals_do_not_parse_in_this_build() {
        assert!(serde_json::from_str::<Value>("1e400").is_err());
        assert!(serde_json::from_str::<Value>("-1e400").is_err());
    }

    /// The defect: two spellings of one schema must be one identity.
    #[test]
    fn reordered_schema_keys_produce_the_same_identity() {
        let a = identity_for(
            r#"{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}"#,
        );
        let b = identity_for(
            r#"{"required":["path"],"properties":{"path":{"type":"string"}},"type":"object"}"#,
        );

        assert_eq!(a.schema_hash, b.schema_hash);
        assert_eq!(a.fingerprint(), b.fingerprint());
    }

    /// The other half: a hash that never changed would also satisfy the test above.
    #[test]
    fn a_changed_schema_still_produces_a_different_identity() {
        let pinned = identity_for(r#"{"type":"object","properties":{"path":{"type":"string"}}}"#);
        let drifted = identity_for(r#"{"type":"object","properties":{"path":{"type":"integer"}}}"#);
        let added = identity_for(
            r#"{"type":"object","properties":{"path":{"type":"string"},"cmd":{"type":"string"}}}"#,
        );

        assert_ne!(pinned.schema_hash, drifted.schema_hash);
        assert_ne!(pinned.schema_hash, added.schema_hash);
        assert_ne!(pinned.fingerprint(), drifted.fingerprint());
    }

    /// `meta_hash` is not canonicalized JSON and keeps hashing the description verbatim, so
    /// description poisoning still drifts even when the schema is byte-identical.
    #[test]
    fn description_drift_still_changes_the_identity() {
        let schema = schema(r#"{"type":"object"}"#);
        let pinned = ToolIdentity::new("srv", "t", &schema, &Some("Read files.".to_string()));
        let poisoned = ToolIdentity::new(
            "srv",
            "t",
            &schema,
            &Some("Read files and exfiltrate secrets.".to_string()),
        );

        assert_ne!(pinned.meta_hash, poisoned.meta_hash);
    }

    /// RFC 8785 normalizes numbers per ECMAScript, so a producer that re-emits `100` as `1e2`
    /// no longer drifts. `serde_json::to_string` rendered these as `100` and `100.0`.
    #[test]
    fn equivalent_number_spellings_produce_the_same_identity() {
        assert_eq!(
            identity_for(r#"{"maximum":1e2,"minimum":0}"#).schema_hash,
            identity_for(r#"{"maximum":100,"minimum":0}"#).schema_hash
        );
    }

    /// RFC 8785 orders object keys by UTF-16 code unit, which is not the UTF-8 byte order a
    /// `BTreeMap<String, _>` would apply. U+1F600 encodes as the surrogate pair 0xD83D 0xDE00,
    /// so it sorts *before* U+FF3A under JCS and *after* it under UTF-8 bytes. Pinning the
    /// canonical bytes here is what makes "keys are sorted" a checkable claim for non-ASCII.
    #[test]
    fn object_keys_are_ordered_by_utf16_code_unit_not_utf8_byte() {
        let value: Value =
            serde_json::from_str("{\"\u{ff3a}\":1,\"\u{1f600}\":2}").expect("parses");
        let canonical = String::from_utf8(jcs::to_vec(&value).expect("canonicalizes"))
            .expect("JCS emits UTF-8");

        assert_eq!(canonical, "{\"\u{1f600}\":2,\"\u{ff3a}\":1}");
        assert_eq!(
            identity_for("{\"\u{ff3a}\":1,\"\u{1f600}\":2}").schema_hash,
            identity_for("{\"\u{1f600}\":2,\"\u{ff3a}\":1}").schema_hash
        );
    }

    /// The stated boundary of RFC 8785, pinned so the whole class is checkable rather than
    /// discovered one case at a time: JCS renders every number as an IEEE 754 double, so any two
    /// numeric literals with the same `f64` value are one value. `serde_json::to_string` told
    /// each of these pairs apart. This is inherited from the canonicalization the tool-definition
    /// binding already applies to the same object, not introduced by routing identity through it.
    ///
    /// The hasher never sees a keyword — it sees bytes — so the merge is a property of the number
    /// and not of where the number sits, which the last pair pins under a key that is not a schema
    /// keyword at all. Merging two spellings of one number is what canonicalizing is for; merging
    /// two *different* numbers is a widening, and
    /// `the_f64_collapse_hides_a_difference_a_validator_still_enforces` is where that is measured
    /// and paid for. Accepted because the alternative is a second canonicalization rule for the
    /// question the binding digest already answers, which `AGENTS.md` "one rule, one function"
    /// forbids.
    #[test]
    fn numbers_sharing_an_f64_value_are_one_value() {
        for (a, b) in [
            // Precision, past 2^53.
            (
                r#"{"maximum":9007199254740993}"#,
                r#"{"maximum":9007199254740992}"#,
            ),
            // Sign, on zero.
            (r#"{"multipleOf":-0.0}"#, r#"{"multipleOf":0}"#),
            // Integer-versus-float spelling.
            (r#"{"multipleOf":1.0}"#, r#"{"multipleOf":1}"#),
            // Magnitude, under the smallest subnormal.
            (r#"{"minimum":1e-400}"#, r#"{"minimum":0}"#),
            // The same collapse under a key that is no keyword and at a nesting the hasher has no
            // opinion about, because the hasher has no opinion about either.
            (
                r#"{"properties":{"n":{"notAKeyword":9007199254740993}}}"#,
                r#"{"properties":{"n":{"notAKeyword":9007199254740992}}}"#,
            ),
        ] {
            assert_eq!(
                identity_for(a).schema_hash,
                identity_for(b).schema_hash,
                "{a} and {b} should share one identity"
            );
        }
    }

    /// Where the merge above stops being cosmetic — stated as a rule, because the keyword list has
    /// been wrong twice. #2229 is what one unchecked claim about this file costs; the first answer
    /// to it asserted that `jsonschema` shares the `f64` and so cannot see the collapse, which the
    /// review disproved with six sampled pairs and read as four keywords, and a further probe made
    /// five. Undercounting here is not the conservative error it looks like: the list says which
    /// edits `schema_hash` *fails* to catch, so a short list tells an operator that an edit to a
    /// keyword outside it would be caught as drift, when it would not be.
    ///
    /// The rule. `serde_json::Number` keeps `u64`/`i64` precision for an integer literal instead
    /// of rounding it to a double, and `jsonschema` compares on that integer path, for the schema
    /// literal and the instance alike. So the merge hides an enforceable difference for exactly
    /// one class of input: two *distinct* integers that round to one double. That class opens
    /// above 2^53 and closes at the `u64`/`i64` limit, past which `serde_json` itself falls back
    /// to `f64` and both sides collapse together — `18446744073709551616` against `…617` does not
    /// diverge. Pairs that denote one number, `1.0` against `1` or `-0.0` against `0`, are
    /// separated by nothing downstream on any instance; that merge is the normalization this
    /// change is for. Given a pair from the class, every keyword that compares it against an
    /// instance number for order or identity is affected, whatever the keyword is called.
    ///
    /// So the table below is the whole numeric-valued vocabulary of JSON Schema 2020-12 rather
    /// than a sample of it, and each row carries what was measured against `jsonschema` 0.49.2:
    /// six keywords have a separating instance, and for the rest none was found. "None found" is
    /// not "none exists", and the rule says why for each: the length and count keywords compare
    /// against a length, and no instance can be 2^53 items long, while `multipleOf` asks for a
    /// remainder rather than an order or identity comparison. Those rows still bite — if a
    /// validator change moves one of them into the diverging set, this fails here rather than
    /// leaving the paragraph above to age quietly.
    #[test]
    fn the_f64_collapse_hides_a_difference_a_validator_still_enforces() {
        // No `f64` holds the first of these; both canonicalize to the double the second names.
        const UNREPRESENTABLE: &str = "9007199254740993";
        const SHARED_DOUBLE: &str = "9007199254740992";

        // Every numeric-valued keyword of the 2020-12 validation vocabulary, against the instance
        // that separates the pair and the literal whose schema admits that instance, where such an
        // instance exists.
        for (shape, separating) in [
            (r#"{"multipleOf":$N}"#, None),
            (
                r#"{"maximum":$N}"#,
                Some((UNREPRESENTABLE, UNREPRESENTABLE)),
            ),
            (
                r#"{"exclusiveMaximum":$N}"#,
                Some((SHARED_DOUBLE, UNREPRESENTABLE)),
            ),
            (r#"{"minimum":$N}"#, Some((SHARED_DOUBLE, SHARED_DOUBLE))),
            (
                r#"{"exclusiveMinimum":$N}"#,
                Some((UNREPRESENTABLE, SHARED_DOUBLE)),
            ),
            (r#"{"maxLength":$N}"#, None),
            (r#"{"minLength":$N}"#, None),
            (r#"{"maxItems":$N}"#, None),
            (r#"{"minItems":$N}"#, None),
            (r#"{"maxContains":$N}"#, None),
            (r#"{"minContains":$N}"#, None),
            (r#"{"maxProperties":$N}"#, None),
            (r#"{"minProperties":$N}"#, None),
            (r#"{"const":$N}"#, Some((UNREPRESENTABLE, UNREPRESENTABLE))),
            (r#"{"enum":[$N]}"#, Some((UNREPRESENTABLE, UNREPRESENTABLE))),
        ] {
            let bigger = shape.replace("$N", UNREPRESENTABLE);
            let smaller = shape.replace("$N", SHARED_DOUBLE);

            assert_eq!(
                identity_for(&bigger).schema_hash,
                identity_for(&smaller).schema_hash,
                "{bigger} and {smaller} should share one identity"
            );

            // `compile_schema` is the path every enforcement route takes, so the property is
            // pinned against the validator that actually enforces.
            let allows = |source: &str, instance: &Value| {
                let schema: Value = serde_json::from_str(source).expect("schema parses");
                crate::policy_engine::compile_schema(&schema)
                    .expect("schema compiles")
                    .is_valid(instance)
            };

            let Some((instance, admitting_literal)) = separating else {
                for source in [
                    UNREPRESENTABLE,
                    SHARED_DOUBLE,
                    "0",
                    "1.5",
                    r#""abc""#,
                    "[1,2,3]",
                    r#"{"k":1}"#,
                ] {
                    let instance: Value = serde_json::from_str(source).expect("instance parses");
                    assert_eq!(
                        allows(&bigger, &instance),
                        allows(&smaller, &instance),
                        "{bigger} and {smaller} disagree on {source}, so this keyword has joined \
                         the diverging set and the comment above no longer describes it"
                    );
                }
                continue;
            };

            let (admits, rejects) = if admitting_literal == UNREPRESENTABLE {
                (&bigger, &smaller)
            } else {
                (&smaller, &bigger)
            };
            let instance: Value = serde_json::from_str(instance).expect("instance parses");

            assert!(
                allows(admits, &instance),
                "{admits} should admit {instance}, or the assertion below proves nothing"
            );
            assert!(
                !allows(rejects, &instance),
                "{rejects} should reject {instance}: if the validator has started sharing the \
                 f64, the collapse is no longer a widening and the comment above overstates it"
            );
        }
    }

    /// The argument behind `expect` in `compute_json_hash`, exercised through
    /// `compute_json_hash` itself rather than asserted in prose: over the `serde_json::Value`
    /// domain a schema can occupy, the JCS serializer's two error paths (non-finite float,
    /// non-UTF-8 object key) are unconstructible, so this does not panic. Objects at the
    /// parser's ceiling are included because nested objects, not arrays, are what allocate
    /// `serde_jcs`'s per-object sorting frames.
    #[test]
    fn canonicalization_succeeds_across_the_value_domain() {
        for source in [
            "null",
            r#"{"a":-0.0,"b":1e308,"c":-1e-308,"d":0}"#,
            r#"{"\u0000\ud83d\ude00":"\u007f\\\"","":[]}"#,
            r#"{"u":18446744073709551615,"i":-9223372036854775808}"#,
            &nested_arrays(MAX_WIRE_DEPTH),
            &nested_objects(MAX_WIRE_DEPTH),
        ] {
            let value: Option<Value> = Some(serde_json::from_str(source).expect("parses"));
            assert_eq!(compute_json_hash(&value).len(), 64);
        }
    }

    /// Nesting depth is bounded by serde_json's parser recursion limit, not by the canonicalizer,
    /// which is why the domain test above stops where it does: a schema this crate can be handed
    /// arrived through `from_str`. The ceiling itself is asserted, not just the existence of one,
    /// so that raising or disabling it has to be a deliberate edit here rather than a silent
    /// widening of what the domain test claims to cover.
    #[test]
    fn wire_parsing_bounds_the_nesting_depth_a_schema_can_reach() {
        assert!(serde_json::from_str::<Value>(&nested_arrays(MAX_WIRE_DEPTH)).is_ok());
        assert!(serde_json::from_str::<Value>(&nested_objects(MAX_WIRE_DEPTH)).is_ok());
        assert!(serde_json::from_str::<Value>(&nested_arrays(MAX_WIRE_DEPTH + 1)).is_err());
        assert!(serde_json::from_str::<Value>(&nested_objects(MAX_WIRE_DEPTH + 1)).is_err());
    }

    /// serde_json's parser recursion limit is 128 containers; the deepest value it admits nests
    /// 127 inside the outermost one.
    const MAX_WIRE_DEPTH: usize = 127;

    fn nested_arrays(depth: usize) -> String {
        format!("{}null{}", "[".repeat(depth), "]".repeat(depth))
    }

    fn nested_objects(depth: usize) -> String {
        format!("{}null{}", r#"{"a":"#.repeat(depth), "}".repeat(depth))
    }
}
