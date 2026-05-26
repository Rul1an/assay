use super::*;
use crate::A2aAdapter;

use assay_adapter_api::{
    digest_canonical_json, AdapterErrorKind, AdapterInput, AdapterResult, AttachmentWriter,
    ConvertMode, ConvertOptions, LossinessLevel, ProtocolAdapter, RawPayloadRef,
};
use proptest::prelude::*;
use serde_json::Value;
use sha2::Digest;
use std::{fs, path::PathBuf};

struct TestWriter;

impl AttachmentWriter for TestWriter {
    fn write_raw_payload(&self, payload: &[u8], media_type: &str) -> AdapterResult<RawPayloadRef> {
        Ok(RawPayloadRef {
            sha256: hex::encode(sha2::Sha256::digest(payload)),
            size_bytes: payload.len() as u64,
            media_type: media_type.to_string(),
        })
    }
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/ci/fixtures/adr026/a2a/v0.2")
}

fn fixture(name: &str) -> Vec<u8> {
    fs::read(fixture_dir().join(name)).expect("fixture must exist")
}

fn reserved_key(key: &str) -> bool {
    matches!(
        key,
        "protocol"
            | "version"
            | "event_type"
            | "timestamp"
            | "agent"
            | "task"
            | "artifact"
            | "message"
            | "attributes"
    )
}

fn assert_discovery_v1_defaults(payload: &Value) {
    let d = &payload["discovery"];
    assert_eq!(d["agent_card_visible"], Value::Bool(false));
    assert_eq!(
        d["agent_card_source_kind"],
        Value::String("unknown".to_string())
    );
    assert_eq!(d["extended_card_access_visible"], Value::Bool(false));
    assert_eq!(d["signature_material_visible"], Value::Bool(false));
}

fn assert_handoff_v1_defaults(payload: &Value) {
    let h = &payload["handoff"];
    assert_eq!(h["visible"], Value::Bool(false));
    assert_eq!(h["source_kind"], Value::String("unknown".to_string()));
    assert_eq!(h["task_ref_visible"], Value::Bool(false));
    assert_eq!(h["message_ref_visible"], Value::Bool(false));
}

/// Golden digests over `payload.discovery` via `digest_canonical_json` (sorted keys).
/// If `discovery` shape or types change intentionally, update hashes and this comment.
const G4_DISCOVERY_DIGEST_DEFAULT: &str =
    "26b4d9c0105f4cc26d4b413e7b6b27effe5829f9f319a60b91ca490fd7776a13";
const G4_DISCOVERY_DIGEST_AGENT_CARD_ATTR: &str =
    "93f5c26d149e7400d38104c4479f332df4df23df0d1f4d25aef252aac87b9769";
const G4_DISCOVERY_DIGEST_BOTH_FLAGS: &str =
    "9d0f24e430e00ee3ec1bc595cb59e6e7d7d5b12c0c90e102ea4d26ad3890e665";
/// Extended visibility only (`agent_card_source_kind` stays `unknown`).
const G4_DISCOVERY_DIGEST_EXTENDED_ONLY: &str =
    "13e23c6783de838b52ca92d787569bccd3cadc0f8900f1bf76b42262959f77ba";
const K1_HANDOFF_DIGEST_DEFAULT: &str =
    "60e992b4881c03d816cd94929856d8c8cade113f62273d42a8a75412533a294a";
const K1_HANDOFF_DIGEST_TYPED_POSITIVE: &str =
    "e478af7359a254678c90b5eb2737d63f79c6d667a2b5c4bc323442f07d09d33b";
const K1_HANDOFF_DIGEST_LENIENT_PARTIAL: &str =
    "0be260743587b9594018a4ab7809560157be088be0372a8ae7c7faa6a744effe";

mod discovery_g4;
mod measurement_and_lenient;
mod protocol_and_strict;
