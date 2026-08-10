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
    /// binding already applies to the same object, not introduced by routing identity through it,
    /// and it is not a widening: `jsonschema` enforces over the same `f64`, so a pair JCS calls
    /// equal is a pair the validator also cannot distinguish.
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
        ] {
            assert_eq!(
                identity_for(a).schema_hash,
                identity_for(b).schema_hash,
                "{a} and {b} should share one identity"
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
