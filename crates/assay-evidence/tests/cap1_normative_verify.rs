//! The CAP-1 normative verifier: bounded admission, the pinned JSON Schema, then R0–R8 in
//! numeric order with the first failure exposed (SPEC-Incident-Package-v1 §6, §10 phase 8).
//!
//! Fixtures under `tests/fixtures/cap1/normative/` are the fifteen upstream vectors of
//! `Certisyn-Inc/certisyn-drafts@0980d3201aa2caab3cbad5c6e9bc99b422370b43`, vendored
//! byte-for-byte and pinned by `SHA256SUMS`. Their expected FIRST failure is stated here from
//! the schema-first ordering, not copied from the upstream run record, which lists every rule
//! that fires (NC-05 fires R1 and R5 there; here the first failure is R1, and R5 is reached
//! only when R1 is silenced).
//!
//! Conformance is internal consistency of a document. It says nothing about producer truth,
//! capture completeness, or what any agent did.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use assay_evidence::{
    cap1_claim_decision, verify_cap1_document, verify_cap1_rules, verify_cap1_rules_with,
    Cap1AdmissionLimits, Cap1Document, Cap1NormativeRule, Cap1Refusal, Cap1RelyingPartyContext,
    Cap1Stage, Cap1SyntaxFault, CodingAgentClaimKind, CAP1_SCHEMA_JSON, CAP1_SCHEMA_SHA256,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

fn normative_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cap1/normative")
}

fn vector(id: &str) -> Vec<u8> {
    fs::read(normative_dir().join(format!("{id}.json"))).expect("vector readable")
}

fn verify(bytes: &[u8]) -> Result<Cap1Document, Cap1Refusal> {
    verify_cap1_document(bytes, &Cap1AdmissionLimits::default())
}

#[derive(Debug, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
enum ExpectedOutcome {
    Pass,
    Refusal {
        stage: String,
        #[serde(default)]
        rule: Option<String>,
        #[serde(default)]
        schema_ground: Option<String>,
        #[serde(default)]
        admission_ground: Option<String>,
    },
}

#[derive(Debug, PartialEq, Eq)]
enum ActualOutcome {
    Pass,
    Refusal {
        stage: String,
        rule: Option<String>,
        schema_ground: Option<String>,
        admission_ground: Option<String>,
    },
}

fn rule_of(r: &Cap1Refusal) -> Cap1NormativeRule {
    match r {
        Cap1Refusal::Rule { rule, .. } => *rule,
        other => panic!("expected a rules-stage refusal, got {other:?}"),
    }
}

#[test]
fn pinned_schema_bytes_match_the_spec_digest() {
    let digest = hex::encode(Sha256::digest(CAP1_SCHEMA_JSON.as_bytes()));
    assert_eq!(
        digest,
        "4453f216089543780bfecc4295cc4a61462fdc585b88d1e35b7d1aba79716b4a"
    );
    assert_eq!(CAP1_SCHEMA_SHA256, digest);
    // No retrieval can happen if there is nothing to retrieve.
    assert!(!CAP1_SCHEMA_JSON.contains("$ref"));
}

#[test]
fn vendored_vectors_match_their_recorded_digests() {
    let sums = fs::read_to_string(normative_dir().join("SHA256SUMS")).expect("SHA256SUMS");
    let mut seen = 0;
    for line in sums.lines() {
        let (digest, name) = line.split_once("  ").expect("two-space shasum format");
        let bytes = fs::read(normative_dir().join(name)).expect("listed vector exists");
        assert_eq!(hex::encode(Sha256::digest(&bytes)), digest, "{name}");
        seen += 1;
    }
    assert_eq!(seen, 17);
}

#[test]
fn shared_expected_first_failure_parity_fixture() {
    let expected: BTreeMap<String, ExpectedOutcome> = serde_json::from_slice(
        &fs::read(normative_dir().join("expected-first-failure.json")).expect("expected fixture"),
    )
    .expect("expected fixture parses");
    for (vector_file, expected_outcome) in expected {
        let bytes = fs::read(normative_dir().join(&vector_file)).expect("vector readable");
        let actual = match verify(&bytes) {
            Ok(_) => ActualOutcome::Pass,
            Err(err) => refusal_to_actual(&err),
        };
        assert_eq!(
            actual,
            expected_to_actual(expected_outcome),
            "{vector_file}"
        );
    }
}

fn expected_to_actual(expected: ExpectedOutcome) -> ActualOutcome {
    match expected {
        ExpectedOutcome::Pass => ActualOutcome::Pass,
        ExpectedOutcome::Refusal {
            stage,
            rule,
            schema_ground,
            admission_ground,
        } => ActualOutcome::Refusal {
            stage,
            rule,
            schema_ground,
            admission_ground,
        },
    }
}

fn refusal_to_actual(err: &Cap1Refusal) -> ActualOutcome {
    let stage = match err.stage() {
        Cap1Stage::Admission => "admission",
        Cap1Stage::Schema => "schema",
        Cap1Stage::Rules => "rules",
    }
    .to_string();
    let rule = err.rule().map(|r| r.as_str().to_string());
    let schema_ground = match err {
        Cap1Refusal::Schema { instance_path, .. } => Some(instance_path.clone()),
        _ => None,
    };
    let admission_ground = match err {
        Cap1Refusal::Oversized { .. } => Some("oversized".to_string()),
        Cap1Refusal::Syntax(Cap1SyntaxFault::NotUtf8) => Some("not-utf8".to_string()),
        Cap1Refusal::Syntax(Cap1SyntaxFault::DuplicateKey) => Some("duplicate-key".to_string()),
        Cap1Refusal::Syntax(Cap1SyntaxFault::InvalidEscape) => Some("invalid-escape".to_string()),
        Cap1Refusal::Syntax(Cap1SyntaxFault::NestingTooDeep) => {
            Some("nesting-too-deep".to_string())
        }
        Cap1Refusal::Syntax(Cap1SyntaxFault::TooManyKeys) => Some("too-many-keys".to_string()),
        Cap1Refusal::Syntax(Cap1SyntaxFault::StringTooLong) => Some("string-too-long".to_string()),
        Cap1Refusal::Syntax(Cap1SyntaxFault::Malformed) => Some("malformed-json".to_string()),
        _ => None,
    };
    ActualOutcome::Refusal {
        stage,
        rule,
        schema_ground,
        admission_ground,
    }
}

#[test]
fn silencing_a_rule_moves_the_first_failure_later() {
    use Cap1NormativeRule as R;
    // With R1 silenced NC-05 reaches R5: examined 11 exceeds eligible 9.
    let doc = typed("NC-05");
    let err = verify_cap1_rules_with(&doc, &[R::R1NoSilentRemainder]).expect_err("R5");
    assert_eq!(rule_of(&err), R::R5CountsWellFormed);
    // Each single-rule vector conforms once its rule is silenced: no rule is decoration.
    for (id, rule) in [
        ("NC-01", R::R1NoSilentRemainder),
        ("NC-03", R::R3WithholdingDigestBound),
        ("NC-04", R::R4DenominatorBasis),
        ("NC-06", R::R6AbsenceIsScoped),
        ("NC-07", R::R7IncompleteNotClean),
        ("NC-08", R::R8SupportsBoundsCitation),
        ("NC-09", R::R1NoSilentRemainder),
        ("NC-10", R::R7IncompleteNotClean),
    ] {
        verify_cap1_rules_with(&typed(id), &[rule]).unwrap_or_else(|e| panic!("{id}: {e}"));
    }
}

/// Typed parse only, bypassing the verifier: for tests that need a document the verifier
/// refuses. Never a production path.
fn typed(id: &str) -> Cap1Document {
    serde_json::from_slice(&vector(id)).expect("typed parse")
}

#[test]
fn schema_stage_is_load_bearing_beyond_the_typed_shape() {
    // Uppercase violates the schema's stratum id pattern; the typed shape and R0–R8 accept it.
    // The id and the assertion that cites it are renamed together so R6 still resolves.
    let text = String::from_utf8(vector("PV-01")).unwrap();
    assert!(text.contains("\"id\": \"detectors\"") && text.contains("\"stratum\": \"detectors\""));
    let mutant = text.replace("\"detectors\"", "\"Detectors\"");
    let err = verify(mutant.as_bytes()).expect_err("schema");
    assert_eq!(err.stage(), Cap1Stage::Schema, "{err}");
    match &err {
        Cap1Refusal::Schema { instance_path, .. } => assert_eq!(instance_path, "/strata/0/id"),
        other => panic!("{other:?}"),
    }
    let doc: Cap1Document = serde_json::from_str(&mutant).expect("typed shape accepts it");
    verify_cap1_rules(&doc).expect("R0–R8 alone do not catch it");
}

#[test]
fn r1_bites_on_a_hand_written_schema_valid_document() {
    // Written by hand, not regenerated: eligible 3, examined 1, one accounted unexamined unit.
    let doc = br#"{
      "profile": "cap/1",
      "subject": {"kind": "artefact", "ref": "s"},
      "strata": [{
        "id": "a", "population": "p", "basis": {"kind": "declared"},
        "eligible": 3, "examined": 1,
        "unexamined": [{"unit": "u1", "disposition": "failed"}]
      }],
      "integrity": {"complete": false, "statement": "s", "capped_to": "incomplete"}
    }"#;
    let err = verify(doc).expect_err("R1");
    match &err {
        Cap1Refusal::Rule { rule, at, .. } => {
            assert_eq!(*rule, Cap1NormativeRule::R1NoSilentRemainder);
            assert_eq!(at.as_deref(), Some("/strata/0"));
        }
        other => panic!("{other:?}"),
    }
    // The balanced control passes.
    let balanced = String::from_utf8_lossy(doc).replace("\"eligible\": 3", "\"eligible\": 2");
    verify(balanced.as_bytes()).expect("balanced control conforms");
}

#[test]
fn r0_duplicate_stratum_ids_refuse_before_later_rules() {
    let doc = br#"{
      "profile": "cap/1",
      "subject": {"kind": "artefact", "ref": "s"},
      "strata": [
        {"id": "a", "population": "p", "basis": {"kind": "declared"},
         "eligible": 0, "examined": 0, "unexamined": []},
        {"id": "a", "population": "p", "basis": {"kind": "declared"},
         "eligible": 5, "examined": 0, "unexamined": []}
      ],
      "integrity": {"complete": true, "statement": "s"}
    }"#;
    let err = verify(doc).expect_err("R0");
    assert_eq!(rule_of(&err), Cap1NormativeRule::R0Shape);
}

#[test]
fn byte_ceiling_is_charged_before_parsing_at_limit_and_limit_plus_one() {
    let bytes = vector("PV-01");
    let exact = Cap1AdmissionLimits {
        max_bytes: bytes.len(),
    };
    verify_cap1_document(&bytes, &exact).expect("exactly at the selected limit passes");
    let one_under = Cap1AdmissionLimits {
        max_bytes: bytes.len() - 1,
    };
    let err = verify_cap1_document(&bytes, &one_under).expect_err("limit + 1 refuses");
    assert_eq!(err.stage(), Cap1Stage::Admission);
    assert!(matches!(err, Cap1Refusal::Oversized { .. }), "{err:?}");
}

#[test]
fn selected_budget_cannot_raise_the_hard_ceiling() {
    let mut padded = vector("PV-01");
    padded.resize(Cap1AdmissionLimits::HARD_MAX_BYTES + 1, b' ');
    let err = verify_cap1_document(
        &padded,
        &Cap1AdmissionLimits {
            max_bytes: usize::MAX,
        },
    )
    .expect_err("hard ceiling holds");
    assert!(matches!(err, Cap1Refusal::Oversized { .. }), "{err:?}");
    // The same document one byte shorter is admitted and conforms.
    padded.truncate(Cap1AdmissionLimits::HARD_MAX_BYTES);
    verify_cap1_document(
        &padded,
        &Cap1AdmissionLimits {
            max_bytes: usize::MAX,
        },
    )
    .expect("at the hard ceiling");
}

#[test]
fn strict_syntax_faults_refuse_before_schema() {
    let text = String::from_utf8(vector("PV-01")).unwrap();
    let dup = text.replacen(
        "\"profile\": \"cap/1\",",
        "\"profile\": \"cap/1\", \"profile\": \"cap/1\",",
        1,
    );
    assert_ne!(dup, text);
    let err = verify(dup.as_bytes()).expect_err("duplicate key");
    assert_eq!(err, Cap1Refusal::Syntax(Cap1SyntaxFault::DuplicateKey));

    let deep = format!("{}{}", "[".repeat(65), "]".repeat(65));
    let err = verify(deep.as_bytes()).expect_err("depth");
    assert_eq!(err, Cap1Refusal::Syntax(Cap1SyntaxFault::NestingTooDeep));

    let err = verify(&[0xff, b'{', b'}']).expect_err("utf-8");
    assert_eq!(err, Cap1Refusal::Syntax(Cap1SyntaxFault::NotUtf8));

    let err = verify(b"{\"profile\": ").expect_err("malformed");
    assert_eq!(err, Cap1Refusal::Syntax(Cap1SyntaxFault::Malformed));
}

#[test]
fn a_shape_the_typed_parse_refuses_after_schema_acceptance_is_still_shape() {
    // JSON Schema treats 1.0 as an integer; the typed count domain does not.
    let text = String::from_utf8(vector("PV-01")).unwrap();
    assert!(text.contains("\"eligible\": 6"));
    let mutant = text.replacen("\"eligible\": 6", "\"eligible\": 6.0", 1);
    let err = verify(mutant.as_bytes()).expect_err("shape");
    assert_eq!(err.stage(), Cap1Stage::Schema, "{err}");
}

#[test]
fn refusal_text_carries_no_instance_values() {
    let err = verify(&vector("NC-02")).expect_err("NC-02");
    assert!(!err.to_string().contains("other"), "{err}");
    let err = verify(&vector("NC-06")).expect_err("NC-06");
    assert!(!err.to_string().contains("does-not-exist"), "{err}");
    let text = String::from_utf8(vector("PV-01")).unwrap();
    let dup = text.replacen(
        "\"population\":",
        "\"secret-key-name\": 1, \"secret-key-name\": 2, \"population\":",
        1,
    );
    let err = verify(dup.as_bytes()).expect_err("dup");
    assert!(!err.to_string().contains("secret-key-name"), "{err}");
}

#[test]
fn verified_document_feeds_the_existing_relying_party_gate() {
    let doc = verify(&vector("PV-03")).expect("PV-03 conforms");
    let decision = cap1_claim_decision(
        &doc,
        CodingAgentClaimKind::PositiveExistence,
        &Cap1RelyingPartyContext::default(),
    );
    assert_eq!(decision.claim_kind, CodingAgentClaimKind::PositiveExistence);
}
