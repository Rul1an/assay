//! A prepared package declares exactly the surfaces its runner-kernel lines map to.
use assay_evidence::{
    prepare_verified_incident_package, verify_incident_package, ContextInput, IncidentOutcome,
    IncidentPrepareSource,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

const V0: &str = "runner-kernel-ndjson-v0";
const V1: &str = "runner-kernel-ndjson-v1";
const V0_OPENAT: &str = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"run_kernel_01","seq":0,"pid":101,"event_type":1,"kind":"openat","value":"/tmp/out.txt","flags":577,"mode":420,"return_value":3,"access_mode":"write","operation_flags":["create","truncate"],"status":"success"}"#;
const V0_CONNECT: &str = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"run_kernel_01","seq":1,"pid":101,"event_type":2,"kind":"connect","value":null}"#;
const V0_EXEC: &str = r#"{"schema":"assay.runner.kernel_event.v0","run_id":"run_kernel_01","seq":2,"pid":102,"event_type":4,"kind":"exec","value":"/usr/bin/curl"}"#;
const V1_OPENAT: &str = r#"{"schema":"assay.runner.kernel_event.v1","run_id":"r1","seq":0,"pid":1,"event_type":1,"kind":"openat","value":"/tmp/a","flags":655616,"mode":0,"return_value":3,"access_mode":"read","status":"success"}"#;

fn kernel(format: &str, lines: &[&str]) -> IncidentPrepareSource {
    IncidentPrepareSource {
        id: "kernel".into(),
        format: format.into(),
        bytes: lines
            .iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>()
            .into_bytes(),
    }
}

fn member(package: &[u8], wanted: &str) -> Vec<u8> {
    let mut archive = tar::Archive::new(std::io::Cursor::new(package));
    for item in archive.entries().unwrap() {
        let mut item = item.unwrap();
        if item.path().unwrap().to_string_lossy() == wanted {
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut item, &mut bytes).unwrap();
            return bytes;
        }
    }
    panic!("package has no member {wanted}");
}

fn prepare_and_verify(src: &IncidentPrepareSource) -> Vec<u8> {
    let (package, report) =
        prepare_verified_incident_package(std::slice::from_ref(src), &ContextInput::default())
            .unwrap_or_else(|report| panic!("prepare refused: {:?}", report.reason));
    assert_eq!(report.outcome, IncidentOutcome::PackageVerified);
    assert_eq!(
        verify_incident_package(&package, &ContextInput::default()).outcome,
        IncidentOutcome::PackageVerified
    );
    let object = format!("objects/{}", hex::encode(Sha256::digest(&src.bytes)));
    assert_eq!(member(&package, &object), src.bytes);
    package
}

fn declared_surfaces(package: &[u8]) -> Value {
    let inventory: Value = serde_json::from_slice(&member(package, "inventory.json")).unwrap();
    inventory["inputs"][0]["surfaces"].clone()
}

fn unit_kinds(package: &[u8]) -> Vec<String> {
    let assessment: Value = serde_json::from_slice(&member(package, "assessment.json")).unwrap();
    let mut kinds: Vec<String> = assessment["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|unit| {
            unit["result"]["value"]["value"]["kind"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    kinds.sort();
    kinds
}

#[test]
fn prepare_declares_the_canonical_surfaces_a_kernel_source_maps_to() {
    for (lines, expected) in [
        (vec![V0_EXEC, V0_OPENAT], vec!["filesystem", "process"]),
        (vec![V0_CONNECT], vec!["network"]),
        (
            vec![V0_EXEC, V0_CONNECT, V0_OPENAT, V0_EXEC],
            vec!["filesystem", "network", "process"],
        ),
    ] {
        let package = prepare_and_verify(&kernel(V0, &lines));
        assert_eq!(declared_surfaces(&package), serde_json::json!(expected));
        let mut kinds: Vec<String> = lines
            .iter()
            .map(|line| {
                serde_json::from_str::<Value>(line).unwrap()["kind"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        kinds.sort();
        assert_eq!(unit_kinds(&package), kinds);
    }
}

#[test]
fn prepare_declares_kernel_v1_surfaces_under_the_same_rule() {
    let package = prepare_and_verify(&kernel(V1, &[V1_OPENAT]));
    assert_eq!(
        declared_surfaces(&package),
        serde_json::json!(["filesystem"])
    );
}

#[test]
fn prepare_declares_each_kernel_input_from_its_own_lines() {
    let sources = [
        IncidentPrepareSource {
            id: "kernel-a".into(),
            ..kernel(V0, &[V0_EXEC])
        },
        IncidentPrepareSource {
            id: "kernel-b".into(),
            ..kernel(V1, &[V1_OPENAT])
        },
    ];
    let (package, report) = prepare_verified_incident_package(&sources, &ContextInput::default())
        .unwrap_or_else(|report| panic!("prepare refused: {:?}", report.reason));
    assert_eq!(report.outcome, IncidentOutcome::PackageVerified);
    let inventory: Value = serde_json::from_slice(&member(&package, "inventory.json")).unwrap();
    let declared: Vec<(String, Value)> = inventory["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|input| {
            (
                input["id"].as_str().unwrap().to_string(),
                input["surfaces"].clone(),
            )
        })
        .collect();
    assert_eq!(
        declared,
        vec![
            ("kernel-a".to_string(), serde_json::json!(["process"])),
            ("kernel-b".to_string(), serde_json::json!(["filesystem"])),
        ]
    );
}

#[test]
fn present_empty_kernel_source_still_declares_no_surface() {
    let package = prepare_and_verify(&kernel(V0, &[]));
    assert_eq!(declared_surfaces(&package), serde_json::json!([]));
}
