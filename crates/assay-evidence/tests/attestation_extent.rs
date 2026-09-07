//! Artifact-backed extent contracts. Synthetic archives assembled independently of BundleWriter.
use assay_evidence::attestation::{
    sign_statement, statement_for_bundle, verify_attestation_for_bundle,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::SigningKey;
use serde_json::json;
use std::io::Read;

const RAW_LARGE_INTEGER: &str = "H4sIAAAAAAAC/+2V32/aMBDHed5fgfy0asBsx47j/B17mpDQxT9KOnBYHLpWFf/77BSooH3cOm27TyKZ3J3tu0v4eguh9S4Oi7vYhcnvgSZKIcYxcT0yKvjp97M9WWgxmdLJO7CPA/TT6eQ/5YnA5rbr22G9jaR+IgZCF0hN7kyc995UqpJkRtYQ18kY18BlmZ77rhvOzx9NFwwMeRhcGFY5ePppuiTLZViSmxtymJFmH+zGrVp7nlVbx5WAohReWVNwzpqqlFZI4NBI6kF7ywtVKGBUGWkrpbU0kgFTrJLC+ZSGu8/7mW4fUjZsRny7cWMVoyMugs1fdTY0j0P2yKKakR0MuZjLmNmpuHN+BSsFbzj3ttFCcpkSUeA8LUqwnDnKGZiGKZWyF41qGFQlVIwJz0FrpQ05pLp3fWf3xvU5h9s29+z4kacNA2xdzuMhd22e8huS8d71sR3fAF2kK/eu34fnxh0j05pdrj7bL17Er2hpNGu3hdU5DXaYIP8wF/+CP6X/xWv95xz1/530P0Z4PGr3hc7XjGmrjS20bkrTCOCUVcLpkqvC21I2pQJojPOl5VwXmqtKWS0rwYGlYC+SnIyLXwvfaNy1Lak9bKI7Gc5SeSWJF95rfTy6kxa+JZGjLzrTuyFe7hbdd1LTGbEwQBbn8eg4HSWaUsW05lIoQbUukla74UfXfzsF0FHZjYvxZVKyRQi26R5W1t32kFZOiZ69h+e9jo0eHndZ+2G327RmDPx8PIZelVHnImO3702ese9DPRZQXzYp7px56Q0bOzO04wHDKS/nVM+p+kJpPd5fs/eYQl5skXbKHVj4NrRx7Sw5fEBtRBAEQRAEQRAEQRAEQRAEQRAEQRAE+Zv4CY5jM7kAKAAA";
const ORDINARY_PROFILE: &str = "H4sIAAAAAAAC/+2VwY6bMBCGc+5TRD511SS1DcbAc/RURYrM2GzYJoZis93VKu9em5BUZHtst2o7XyI5zAz2zED+OSrb1Mb5zYNr7eL3QANZmo5r4HZlNOWX32d7sNBksaSLN2BwXvXL5eI/5YWow33bN35/dKR8IaBsa0lJHsCt+xpymQuyInvl9sHo9oqLLFz3beuv1++htaB8XLyxfheDlx+WW7Ld2i25uyOnFakGqw9m1+jrXSUHngDVRUVFlcmshlpBkhYZcAGyEjSjaQ5KSm4qWSgptM4qzrkSVZUZk0BIwzzG86AdbMiGrUjdHMxYxehwG6vjWx0N1bOPHsGTFemUj8XMY1aX4q75QSZpImtZSCkEqMIAp7WsBdBUy8RwVVRMpwXLWZYyXVEpOKPMcIBCKpYDOYW6u77VA5g+5nDfxJ5NL3k40KqjiXk8xa6tQ34+GB9N75rxCdBN+MTe9YM9N26KDHu2NTnbZw/iV7TUwd4c1e6aBjstkH+Y2b/gT+k/S17pP2eo/2+k/86p50m7ZzpfUhGUgSlIM4A0iElW66AbhYC8rrQRJs9FrSqZFTwPwqpB5ZJSHcySihDOgpyMm98K32jsmoaUtTo4czFcpfJGEmfeW32c3EELfyaRo88Z6I1389Oc+UpKuiJaeRXFeRwdl1EShoQ1/lvbf7lY6CjlYJz7ERVsTlldtU87be57FbYKmV29p/PmU2f9cxfFXnXdoYEx8OM0d17lXcaqXDv0EO8YeluOGZfzrrjOwI9msLEVvhknCqc8W9NiTeUnSsvx+zl6pxTiZptwUix5Uze2cXujyekdiiGCIAiCIAiCIAiCIAiCIAiCIAiCIMhfzHc8zA2BACgAAA==";

#[test]
fn raw_large_integer_archive_is_ordinary_valid() {
    let bytes = STANDARD.decode(RAW_LARGE_INTEGER).unwrap();
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes.as_slice()));
    let mut found = false;
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        if entry.path().unwrap().as_ref() == std::path::Path::new("events.ndjson") {
            let mut raw = String::new();
            entry.read_to_string(&mut raw).unwrap();
            assert!(raw.contains("9007199254740993"));
            let event: serde_json::Value = serde_json::from_str(raw.trim()).unwrap();
            assert_eq!(
                event["data"]["files_count"].as_u64(),
                Some(9007199254740993)
            );
            found = true;
        }
    }
    assert!(found, "raw NDJSON member must exist");
    let result = assay_evidence::bundle::verify_bundle(bytes.as_slice())
        .expect("independent archive must pass ordinary verification before extent refusal");
    assert_eq!(result.event_count, 1);
}

#[test]
fn signed_false_extent_is_refused_by_existing_consumer() {
    let bytes = STANDARD.decode(ORDINARY_PROFILE).unwrap();
    let mut statement = statement_for_bundle(&bytes).expect("ordinary archive accepted");
    statement.predicate["extent"] = json!({
        "retained_events_by_type": {"assay.profile.finished": 2},
        "observed": {"basis": "producer_reported", "source_type": "assay.profile.finished", "counts": {"files": 3, "network": 0, "processes": 0, "sandbox_degradations": 0}}
    });
    let key = SigningKey::from_bytes(&[7; 32]);
    let envelope = sign_statement(&statement, &key).expect("sign deliberately false histogram");
    let failure = verify_attestation_for_bundle(&envelope, &key.verifying_key(), &bytes)
        .expect_err("existing canonical consumer must refuse signed false extent");
    assert!(
        failure
            .to_string()
            .contains("extent.retained_events_by_type"),
        "named extent comparison: {failure}"
    );
}

use assay_evidence::attestation::{
    statement_for_bundle_with_extent, statement_for_bundle_with_extent_and_limits,
    verify_attestation_for_bundle_with_extent, verify_envelope_signature,
};
use assay_evidence::bundle::VerifyLimits;
use assay_evidence::types::EvidenceEvent;
use serde_json::Value;

// Independent raw NDJSON assembly: content hashes use the published algorithm, while event
// serialization preserves integer tokens instead of passing them through BundleWriter's JCS.
fn pack(rows: Vec<(&str, Value)>) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    use std::io::Write;
    let mut hashes = Vec::new();
    let mut body = Vec::new();
    for (seq, (kind, payload)) in rows.into_iter().enumerate() {
        let mut event = EvidenceEvent::new(
            kind,
            "urn:assay:extent-test",
            "extent-proof",
            seq as u64,
            payload,
        )
        .with_time("2026-09-07T00:00:00Z".parse().unwrap());
        let hash = assay_evidence::crypto::id::compute_content_hash(&event).unwrap();
        event.content_hash = Some(hash.clone());
        hashes.push(hash);
        serde_json::to_writer(&mut body, &event).unwrap();
        body.push(b'\n');
    }
    let root = assay_evidence::crypto::id::compute_run_root(&hashes);
    let manifest = json!({"schema_version":1,"bundle_id":root,"run_id":"extent-proof","run_root":root,"event_count":hashes.len(),"producer":{"name":"extent-test","version":"0.0.0","git":"0000000"},"algorithms":{"canon":"jcs-rfc8785","hash":"sha256","root":"sha256(concat(content_hash + \"\\n\"))"},"files":{"events.ndjson":{"path":"events.ndjson","sha256":format!("sha256:{}",hex::encode(Sha256::digest(&body))),"bytes":body.len()}}});
    let mut tar = tar::Builder::new(Vec::new());
    for (name, bytes) in [
        ("manifest.json", serde_json::to_vec(&manifest).unwrap()),
        ("events.ndjson", body),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_path(name).unwrap();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_cksum();
        tar.append(&header, bytes.as_slice()).unwrap();
    }
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&tar.into_inner().unwrap()).unwrap();
    gz.finish().unwrap()
}
fn profile(n: Value) -> Value {
    json!({"files_count":n,"network_count":2,"processes_count":3,"sandbox_degradation_count":4})
}
fn sandbox() -> Value {
    json!({"fs_count":5,"exec_count":6,"degradation_count":7})
}
fn extent(bytes: &[u8]) -> Value {
    statement_for_bundle_with_extent(bytes).unwrap().predicate["extent"].clone()
}
fn signed_check(
    bytes: &[u8],
    changed: Value,
) -> anyhow::Result<assay_evidence::attestation::AttestationVerifiedWithExtent> {
    let mut statement = statement_for_bundle(bytes).unwrap();
    statement.predicate["extent"] = changed;
    let key = SigningKey::from_bytes(&[7; 32]);
    let envelope = sign_statement(&statement, &key).unwrap();
    verify_attestation_for_bundle_with_extent(&envelope, &key.verifying_key(), bytes)
}
fn refusal(bytes: &[u8], changed: Value, field: &str) {
    let error = signed_check(bytes, changed).expect_err("signed extent must refuse");
    assert!(
        format!("{error:#}").contains(field),
        "expected {field}, got {error:#}"
    );
}
fn legacy_accepts(bytes: &[u8]) {
    assay_evidence::bundle::verify_bundle(bytes).expect("ordinary accepted set preserved");
    let key = SigningKey::from_bytes(&[7; 32]);
    let statement = statement_for_bundle(bytes).expect("legacy producer accepted");
    assert!(statement.predicate.get("extent").is_none());
    let envelope = sign_statement(&statement, &key).unwrap();
    assert!(
        verify_attestation_for_bundle_with_extent(&envelope, &key.verifying_key(), bytes)
            .unwrap()
            .extent()
            .is_none()
    );
    assert!(signed_check(bytes, Value::Null).unwrap().extent().is_none());
}

#[test]
fn every_known_extent_field_is_checked_after_signing() {
    let bytes = pack(vec![("assay.profile.finished", profile(json!(1)))]);
    let correct = extent(&bytes);
    for (pointer, bad, field) in [
        (
            "/retained_events_by_type/assay.profile.finished",
            json!(2),
            "extent.retained_events_by_type",
        ),
        (
            "/observed/counts/files",
            json!(9),
            "extent.observed.counts.files",
        ),
        (
            "/observed/counts/network",
            json!(9),
            "extent.observed.counts.network",
        ),
        (
            "/observed/counts/processes",
            json!(9),
            "extent.observed.counts.processes",
        ),
        (
            "/observed/counts/sandbox_degradations",
            json!(9),
            "extent.observed.counts.sandbox_degradations",
        ),
    ] {
        let mut changed = correct.clone();
        *changed.pointer_mut(pointer).unwrap() = bad;
        refusal(&bytes, changed, field);
    }
    let mut basis = correct.clone();
    basis["observed"] = json!({"basis":"not_stated"});
    refusal(&bytes, basis, "extent.observed.basis");
    let mut source = correct.clone();
    source["observed"]["source_type"] = json!("assay.sandbox.summary");
    source["observed"]["counts"]
        .as_object_mut()
        .unwrap()
        .remove("network");
    refusal(&bytes, source, "extent.observed.source_type");
    let mut surplus = correct.clone();
    surplus["retained_events_by_type"]["unseen-type"] = json!(0);
    refusal(&bytes, surplus, "extent.retained_events_by_type");
    let mut missing = correct.clone();
    missing["retained_events_by_type"] = json!({});
    refusal(&bytes, missing, "extent.retained_events_by_type");
    let mut unknown = correct;
    unknown["future"] = json!(true);
    unknown["observed"]["future"] = json!(true);
    unknown["observed"]["counts"]["future"] = json!(true);
    signed_check(&bytes, unknown).unwrap();
}

#[test]
fn tagged_observed_required_shapes_and_presence_are_checked() {
    let bytes = pack(vec![("assay.profile.finished", profile(json!(1)))]);
    let correct = extent(&bytes);
    for changed in [
        json!({}),
        json!([]),
        json!(false),
        json!({"observed":null}),
        json!({"retained_events_by_type":null,"observed":{"basis":"not_stated"}}),
    ] {
        refusal(&bytes, changed, "extent");
    }
    for value in [
        Value::Null,
        json!({}),
        json!({"basis":"unknown"}),
        json!({"basis":"producer_reported","source_type":"assay.profile.finished","counts":null}),
        json!({"basis":"not_stated","counts":null}),
        json!({"basis":"not_stated","source_type":null}),
    ] {
        let mut changed = correct.clone();
        changed["observed"] = value;
        refusal(&bytes, changed, "extent.observed");
    }
    for field in ["files", "network", "processes", "sandbox_degradations"] {
        let mut changed = correct.clone();
        changed["observed"]["counts"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        refusal(&bytes, changed, &format!("extent.observed.counts.{field}"));
        let mut changed = correct.clone();
        changed["observed"]["counts"][field] = Value::Null;
        refusal(&bytes, changed, &format!("extent.observed.counts.{field}"));
    }
    legacy_accepts(&bytes);
}

#[test]
fn ranked_sources_validate_both_orders_and_omit_sandbox_network() {
    for rows in [
        vec![
            ("assay.profile.finished", profile(json!(1))),
            ("assay.sandbox.summary", sandbox()),
        ],
        vec![
            ("assay.sandbox.summary", sandbox()),
            ("assay.profile.finished", profile(json!(1))),
        ],
    ] {
        let bytes = pack(rows);
        let result = extent(&bytes);
        assert_eq!(result["observed"]["source_type"], "assay.profile.finished");
        assert_eq!(result["observed"]["counts"]["files"], 1);
        signed_check(&bytes, result).unwrap();
    }
    let bytes = pack(vec![
        ("assay.sandbox.summary", sandbox()),
        ("assay.coding_agent.evidence_pack.v0", json!({})),
    ]);
    let result = extent(&bytes);
    assert_eq!(result["observed"]["source_type"], "assay.sandbox.summary");
    assert_eq!(
        result["observed"]["counts"],
        json!({"files":5,"processes":6,"sandbox_degradations":7})
    );
    assert_eq!(
        result["retained_events_by_type"]["assay.coding_agent.evidence_pack.v0"],
        1
    );
    for extra in [json!(0), Value::Null] {
        let mut changed = result.clone();
        changed["observed"]["counts"]["network"] = extra;
        refusal(&bytes, changed, "extent.observed.counts.network");
    }
    assert!(!serde_json::to_string(&result).unwrap().contains("null"));
    let unknown = pack(vec![("arbitrary.event", json!({"files_count":9000}))]);
    assert_eq!(
        extent(&unknown),
        json!({"retained_events_by_type":{"arbitrary.event":1},"observed":{"basis":"not_stated"}})
    );
}

#[test]
fn repeated_and_malformed_unselected_summaries_refuse_only_extent_mode() {
    for kind in ["assay.profile.finished", "assay.sandbox.summary"] {
        let payload = if kind == "assay.profile.finished" {
            profile(json!(1))
        } else {
            sandbox()
        };
        for second in [payload.clone(), json!({})] {
            let bytes = pack(vec![(kind, payload.clone()), (kind, second)]);
            legacy_accepts(&bytes);
            let error = statement_for_bundle_with_extent(&bytes).unwrap_err();
            assert!(format!("{error:#}").contains("repeated recognized summary"));
            refusal(
                &bytes,
                json!({"retained_events_by_type":{kind:2},"observed":{"basis":"not_stated"}}),
                "repeated recognized summary",
            );
        }
    }
    for rows in [
        vec![
            ("assay.profile.finished", profile(json!(1))),
            ("assay.sandbox.summary", json!({})),
        ],
        vec![
            ("assay.sandbox.summary", json!({})),
            ("assay.profile.finished", profile(json!(1))),
        ],
    ] {
        let bytes = pack(rows);
        legacy_accepts(&bytes);
        let error = statement_for_bundle_with_extent(&bytes).unwrap_err();
        assert!(format!("{error:#}").contains("extent.observed.counts.files"));
    }
}

#[test]
fn counts_preserve_exact_signed_max_and_refuse_out_of_domain_raw_inputs() {
    for value in [0, 9_007_199_254_740_991u64] {
        let bytes = pack(vec![("assay.profile.finished", profile(json!(value)))]);
        let statement = statement_for_bundle_with_extent(&bytes).unwrap();
        let key = SigningKey::from_bytes(&[7; 32]);
        let envelope = sign_statement(&statement, &key).unwrap();
        let actual: Value =
            serde_json::from_slice(&STANDARD.decode(&envelope.payload).unwrap()).unwrap();
        assert_eq!(
            actual["predicate"]["extent"]["observed"]["counts"]["files"].as_u64(),
            Some(value)
        );
        verify_envelope_signature(&envelope, &key.verifying_key()).unwrap();
        verify_attestation_for_bundle_with_extent(&envelope, &key.verifying_key(), &bytes).unwrap();
    }
    for bad in [
        json!(9_007_199_254_740_992u64),
        json!(9_007_199_254_740_993u64),
        json!(-1),
        json!(0.5),
        json!("1"),
        Value::Null,
    ] {
        let bytes = pack(vec![("assay.profile.finished", profile(bad))]);
        legacy_accepts(&bytes);
        let error = statement_for_bundle_with_extent(&bytes).unwrap_err();
        assert!(format!("{error:#}").contains("extent.observed.counts.files"));
    }
    let bytes = STANDARD.decode(RAW_LARGE_INTEGER).unwrap();
    legacy_accepts(&bytes);
    let error = statement_for_bundle_with_extent(&bytes).unwrap_err();
    assert!(format!("{error:#}").contains("extent.observed.counts.files"));
}

#[test]
fn retained_detail_changes_histogram_without_upgrading_reported_counts() {
    let summary = pack(vec![("assay.profile.finished", profile(json!(7)))]);
    let detailed = pack(vec![
        ("assay.profile.finished", profile(json!(7))),
        ("assay.fs.access", json!({"hits":999})),
    ]);
    let nothing = pack(vec![("assay.profile.finished", profile(json!(0)))]);
    let a = extent(&summary);
    let b = extent(&detailed);
    let c = extent(&nothing);
    assert_eq!(a["observed"], b["observed"]);
    assert_ne!(a["retained_events_by_type"], b["retained_events_by_type"]);
    assert_ne!(a, c);
    assert_eq!(b["observed"]["counts"]["files"], 7);
    assert_eq!(b["observed"]["counts"]["processes"], 3);
}

#[test]
fn histogram_cardinality_max_and_max_plus_one_are_bounded() {
    let max = assay_evidence::json_strict::MAX_KEYS_PER_OBJECT;
    for n in [max, max + 1] {
        let keys: Vec<_> = (0..n).map(|i| format!("type.{i:05}")).collect();
        let bytes = pack(keys.iter().map(|s| (s.as_str(), json!({}))).collect());
        legacy_accepts(&bytes);
        let result = statement_for_bundle_with_extent(&bytes);
        if n == max {
            let statement = result.unwrap();
            assert_eq!(
                statement.predicate["extent"]["retained_events_by_type"]
                    .as_object()
                    .unwrap()
                    .len(),
                max
            );
            let key = SigningKey::from_bytes(&[7; 32]);
            let env = sign_statement(&statement, &key).unwrap();
            verify_attestation_for_bundle_with_extent(&env, &key.verifying_key(), &bytes).unwrap();
        } else {
            assert!(format!("{:#}", result.unwrap_err()).contains("key count limit"));
        }
    }
}

#[test]
fn histogram_utf8_bytes_max_and_max_plus_one_are_bounded() {
    for extra in [false, true] {
        let keys: Vec<_> = (0..1024)
            .map(|i| {
                format!(
                    "{i:04}{}{}",
                    "é".repeat(510),
                    if extra && i == 0 { "x" } else { "" }
                )
            })
            .collect();
        assert_eq!(
            keys.iter().map(String::len).sum::<usize>(),
            1024 * 1024 + usize::from(extra)
        );
        let bytes = pack(keys.iter().map(|s| (s.as_str(), json!({}))).collect());
        legacy_accepts(&bytes);
        let result = statement_for_bundle_with_extent(&bytes);
        if extra {
            assert!(format!("{:#}", result.unwrap_err()).contains("key bytes limit"));
        } else {
            let statement = result.unwrap();
            let key = SigningKey::from_bytes(&[7; 32]);
            let env = sign_statement(&statement, &key).unwrap();
            verify_attestation_for_bundle_with_extent(&env, &key.verifying_key(), &bytes).unwrap();
        }
    }
}

#[test]
fn extent_producer_keeps_bundle_limit_before_archive_work() {
    let bytes = STANDARD.decode(ORDINARY_PROFILE).unwrap();
    let limits = VerifyLimits {
        max_bundle_bytes: bytes.len() as u64 - 1,
        ..VerifyLimits::default()
    };
    assert!(statement_for_bundle_with_extent_and_limits(&bytes, limits).is_err());
}

#[test]
fn existing_public_struct_literals_and_destructures_remain_usable() {
    use assay_evidence::attestation::{AttestationVerified, EvidenceBundlePredicate};
    use assay_evidence::bundle::VerifyResult;
    let bytes = STANDARD.decode(ORDINARY_PROFILE).unwrap();
    let VerifyResult {
        manifest,
        event_count,
        computed_run_root,
    } = assay_evidence::bundle::verify_bundle(bytes.as_slice()).unwrap();
    let rebuilt = VerifyResult {
        manifest,
        event_count,
        computed_run_root,
    };
    assert_eq!(rebuilt.event_count, 1);
    let key = SigningKey::from_bytes(&[7; 32]);
    let envelope = sign_statement(&statement_for_bundle(&bytes).unwrap(), &key).unwrap();
    let AttestationVerified {
        statement,
        predicate,
        artifact_sha256,
    } = verify_attestation_for_bundle(&envelope, &key.verifying_key(), &bytes).unwrap();
    let EvidenceBundlePredicate {
        schema_version,
        semantic_equivalence,
        run,
    } = predicate;
    let rebuilt = AttestationVerified {
        statement,
        predicate: EvidenceBundlePredicate {
            schema_version,
            semantic_equivalence,
            run,
        },
        artifact_sha256,
    };
    assert_eq!(rebuilt.predicate.schema_version, 1);
    let VerifyLimits {
        max_bundle_bytes,
        max_decode_bytes,
        max_manifest_bytes,
        max_events_bytes,
        max_events,
        max_line_bytes,
        max_path_len,
        max_json_depth,
    } = VerifyLimits::default();
    let rebuilt = VerifyLimits {
        max_bundle_bytes,
        max_decode_bytes,
        max_manifest_bytes,
        max_events_bytes,
        max_events,
        max_line_bytes,
        max_path_len,
        max_json_depth,
    };
    assert_eq!(rebuilt, VerifyLimits::default());
}

#[test]
fn both_summary_families_validate_every_count_before_signing() {
    for (kind, fields, initial) in [
        (
            "assay.profile.finished",
            vec![
                "files_count",
                "network_count",
                "processes_count",
                "sandbox_degradation_count",
            ],
            profile(json!(1)),
        ),
        (
            "assay.sandbox.summary",
            vec!["fs_count", "exec_count", "degradation_count"],
            sandbox(),
        ),
    ] {
        for field in fields {
            let projected = match field {
                "files_count" | "fs_count" => "files",
                "network_count" => "network",
                "processes_count" | "exec_count" => "processes",
                "sandbox_degradation_count" | "degradation_count" => "sandbox_degradations",
                _ => unreachable!(),
            };
            for accepted in [0, 9_007_199_254_740_991u64] {
                let mut payload = initial.clone();
                payload[field] = json!(accepted);
                let bytes = pack(vec![(kind, payload)]);
                let statement = statement_for_bundle_with_extent(&bytes).unwrap();
                let key = SigningKey::from_bytes(&[7; 32]);
                let env = sign_statement(&statement, &key).unwrap();
                let actual: Value =
                    serde_json::from_slice(&STANDARD.decode(&env.payload).unwrap()).unwrap();
                assert_eq!(
                    actual["predicate"]["extent"]["observed"]["counts"][projected].as_u64(),
                    Some(accepted)
                );
                verify_attestation_for_bundle_with_extent(&env, &key.verifying_key(), &bytes)
                    .unwrap();
            }
            for bad in [
                Value::Null,
                json!("1"),
                json!(-1),
                json!(0.5),
                json!(9_007_199_254_740_992u64),
                json!(9_007_199_254_740_993u64),
            ] {
                let mut payload = initial.clone();
                payload[field] = bad;
                let bytes = pack(vec![(kind, payload)]);
                legacy_accepts(&bytes);
                let err = statement_for_bundle_with_extent(&bytes).unwrap_err();
                assert!(format!("{err:#}").contains("expected integer in 0..2^53-1"));
            }
            let mut missing = initial.clone();
            missing.as_object_mut().unwrap().remove(field);
            let bytes = pack(vec![(kind, missing)]);
            assert!(statement_for_bundle_with_extent(&bytes).is_err());
        }
    }
    let bytes = STANDARD.decode(ORDINARY_PROFILE).unwrap();
    let correct = extent(&bytes);
    for bad in [
        Value::Null,
        json!("1"),
        json!(-1),
        json!(0.5),
        json!(9_007_199_254_740_992u64),
    ] {
        let mut changed = correct.clone();
        changed["retained_events_by_type"]["assay.profile.finished"] = bad;
        refusal(&bytes, changed, "extent.retained_events_by_type");
    }
}

#[test]
fn identical_old_reader_signed_bytes_are_checked_by_new_reader() {
    use sha2::{Digest, Sha256};
    // Exact DSSE bytes retained from the frozen d270 old-reader acceptance control.
    // They are not re-signed or regenerated here. The key is the declared synthetic fixture key.
    let envelope_bytes=STANDARD.decode("eyJwYXlsb2FkIjoiZXlKZmRIbHdaU0k2SW1oMGRIQnpPaTh2YVc0dGRHOTBieTVwYnk5VGRHRjBaVzFsYm5RdmRqRWlMQ0p3Y21Wa2FXTmhkR1VpT25zaVpYaDBaVzUwSWpwN0ltOWljMlZ5ZG1Wa0lqcDdJbUpoYzJseklqb2ljSEp2WkhWalpYSmZjbVZ3YjNKMFpXUWlMQ0pqYjNWdWRITWlPbnNpWm1sc1pYTWlPak1zSW01bGRIZHZjbXNpT2pBc0luQnliMk5sYzNObGN5STZNQ3dpYzJGdVpHSnZlRjlrWldkeVlXUmhkR2x2Ym5NaU9qQjlMQ0p6YjNWeVkyVmZkSGx3WlNJNkltRnpjMkY1TG5CeWIyWnBiR1V1Wm1sdWFYTm9aV1FpZlN3aWNtVjBZV2x1WldSZlpYWmxiblJ6WDJKNVgzUjVjR1VpT25zaVlYTnpZWGt1Y0hKdlptbHNaUzVtYVc1cGMyaGxaQ0k2TW4xOUxDSnlkVzRpT25zaVpYWmxiblJmWTI5MWJuUWlPakVzSW5CeWIyUjFZMlZ5SWpwN0ltZHBkQ0k2SWpBd01EQXdNREFpTENKdVlXMWxJam9pWlhoMFpXNTBMWFJsYzNRaUxDSjJaWEp6YVc5dUlqb2lNQzR3TGpBaWZTd2ljblZ1WDJsa0lqb2laWGgwWlc1MExYQnliMjltSWl3aWRHbHRaVjkzYVc1a2IzY2lPbnNpWlc1a0lqb2lNakF5Tmkwd09TMHdOMVF3TURvd01Eb3dNRm9pTENKemRHRnlkQ0k2SWpJd01qWXRNRGt0TURkVU1EQTZNREE2TURCYUluMTlMQ0p6WTJobGJXRmZkbVZ5YzJsdmJpSTZNU3dpYzJWdFlXNTBhV05mWlhGMWFYWmhiR1Z1WTJVaU9uc2lZV3huYjNKcGRHaHRJam9pWVhOellYa3RjblZ1TFhKdmIzUXRkakVpTENKMllXeDFaU0k2SW5Ob1lUSTFOam95WXpJell6QmtPV0l3TldJMk56Wm1ZMlpoWXpNME9UWmpNalZqTjJJMU1EWXdORGhqWVRjM01tVmlOemxoTnpWa1pEWmlNakl5WVRWaVlqWmxaVE5qSW4xOUxDSndjbVZrYVdOaGRHVlVlWEJsSWpvaWFIUjBjSE02THk5a2IyTnpMbWRsZEdGemMyRjVMbVJsZGk5aGRIUmxjM1JoZEdsdmJpOWxkbWxrWlc1alpTMWlkVzVrYkdVdmRqRWlMQ0p6ZFdKcVpXTjBJanBiZXlKa2FXZGxjM1FpT25zaWMyaGhNalUySWpvaVpUWXlNMlJtWkdaaE5UY3laV0ZrTnprNU4yVTJOR1l6TXpWaU56UTBNekpoTnpZNE1qUmxOMlEyTlRVM09URm1aV1JsWWpWaE5HUmtOalk1TVdJMllpSjlMQ0p1WVcxbElqb2ljMmhoTWpVMk9qSmpNak5qTUdRNVlqQTFZalkzTm1aalptRmpNelE1Tm1NeU5XTTNZalV3TmpBME9HTmhOemN5WldJM09XRTNOV1JrTm1JeU1qSmhOV0ppTm1WbE0yTWlmVjE5IiwicGF5bG9hZFR5cGUiOiJhcHBsaWNhdGlvbi92bmQuaW4tdG90bytqc29uIiwic2lnbmF0dXJlcyI6W3sia2V5aWQiOiJzaGEyNTY6MzI0YmUyZGVhOGJjNDQ0NjFiMDIzM2U1MWZhNDg5MDJlZDZiMWNjNjcxZTc3MzlhZjI1NTFlMGJmZTY4ZjU0ZSIsInNpZyI6InhIS1dVUHdNd2YrRStYMUdrMnFuSW1mS0M2S3lkTW0rSVpUV3dpMElycWVycm5kM2lRWGtMMExqRjI4cWRvenRrYmxJY3NrZ25LZjVQSThDUkNoVERnPT0ifV19").unwrap();
    assert_eq!(
        hex::encode(Sha256::digest(&envelope_bytes)),
        "d70c4ac82c15e9b4eb4c67e6f758509a1a160f359520c1a62cafdf0923faa761"
    );

    let envelope: assay_evidence::attestation::DsseEnvelope =
        serde_json::from_slice(&envelope_bytes).unwrap();
    let key = SigningKey::from_bytes(&[7; 32]);
    verify_envelope_signature(&envelope, &key.verifying_key())
        .expect("same old-reader witness has valid signature");
    let bytes = STANDARD.decode(ORDINARY_PROFILE).unwrap();
    let error = verify_attestation_for_bundle(&envelope, &key.verifying_key(), &bytes)
        .expect_err("new reader checks the same extent old reader ignored");
    assert!(
        error.to_string().contains("extent.retained_events_by_type"),
        "{error}"
    );
}
