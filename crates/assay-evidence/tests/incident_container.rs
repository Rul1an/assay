use assay_evidence::incident_package::{
    read_incident_container, IncidentOutcome, IncidentReason, IncidentVerifyReport,
};
use hex::ToHex;
use sha2::{Digest, Sha256};

fn compute_sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    result.encode_hex::<String>()
}

fn make_test_header(name: &str, size: usize, typeflag: u8) -> [u8; 512] {
    let mut header = [0u8; 512];
    header[..name.len()].copy_from_slice(name.as_bytes());
    header[100..108].copy_from_slice(b"0000644\0");
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    let size_octal = format!("{:011o}\0", size);
    header[124..136].copy_from_slice(size_octal.as_bytes());
    header[136..148].copy_from_slice(b"00000000000\0");
    header[148..156].copy_from_slice(b"        "); // 8 spaces for chksum calculation
    header[156] = typeflag;
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");

    let sum: u32 = header.iter().map(|&b| b as u32).sum();
    let chksum_str = format!("{:06o}\0 ", sum);
    header[148..156].copy_from_slice(chksum_str.as_bytes());
    header
}

fn pad_to_512(data: &[u8]) -> Vec<u8> {
    let mut v = data.to_vec();
    let rem = v.len() % 512;
    if rem != 0 {
        v.resize(v.len() + (512 - rem), 0);
    }
    v
}

fn build_minimal_valid_archive(
    inv_data: &[u8],
    ass_data: &[u8],
    obj_data: &[u8],
) -> (Vec<u8>, String) {
    let obj_digest = compute_sha256_hex(obj_data);
    let obj_path = format!("objects/{obj_digest}");

    let mut archive = Vec::new();

    // 1. inventory.json
    archive.extend_from_slice(&make_test_header("inventory.json", inv_data.len(), b'0'));
    archive.extend_from_slice(&pad_to_512(inv_data));

    // 2. assessment.json
    archive.extend_from_slice(&make_test_header("assessment.json", ass_data.len(), b'0'));
    archive.extend_from_slice(&pad_to_512(ass_data));

    // 3. objects/<sha256>
    archive.extend_from_slice(&make_test_header(&obj_path, obj_data.len(), b'0'));
    archive.extend_from_slice(&pad_to_512(obj_data));

    // Exactly 2 trailing zero blocks
    archive.extend_from_slice(&[0u8; 1024]);

    (archive, obj_path)
}

/// Test 1: A valid minimal archive - inventory.json, assessment.json, one object - parses,
/// and its member names and bytes come back exactly.
#[test]
fn test_1_valid_minimal_archive() {
    let inv_data = br#"{"schema":"assay.incident.inventory.v1"}"#;
    let ass_data = br#"{"schema":"assay.incident.assessment.v1"}"#;
    let obj_data = b"synthetic object payload bytes\n";

    let (archive, obj_path) = build_minimal_valid_archive(inv_data, ass_data, obj_data);

    let container =
        read_incident_container(&archive).expect("valid archive must parse successfully");

    assert_eq!(container.inventory.name, "inventory.json");
    assert_eq!(container.inventory.bytes, inv_data);

    assert_eq!(container.assessment.name, "assessment.json");
    assert_eq!(container.assessment.bytes, ass_data);

    assert_eq!(container.objects.len(), 1);
    assert_eq!(container.objects[0].name, obj_path);
    assert_eq!(container.objects[0].bytes, obj_data);
}

/// Test 2: Reordering assessment before inventory refuses input_shape, while every byte else is unchanged.
#[test]
fn test_2_reordered_assessment_before_inventory_refuses() {
    let inv_data = br#"{"schema":"assay.incident.inventory.v1"}"#;
    let ass_data = br#"{"schema":"assay.incident.assessment.v1"}"#;
    let obj_data = b"synthetic object payload bytes\n";
    let obj_digest = compute_sha256_hex(obj_data);
    let obj_path = format!("objects/{obj_digest}");

    let mut archive = Vec::new();
    // Swapped order: assessment first, then inventory
    archive.extend_from_slice(&make_test_header("assessment.json", ass_data.len(), b'0'));
    archive.extend_from_slice(&pad_to_512(ass_data));
    archive.extend_from_slice(&make_test_header("inventory.json", inv_data.len(), b'0'));
    archive.extend_from_slice(&pad_to_512(inv_data));
    archive.extend_from_slice(&make_test_header(&obj_path, obj_data.len(), b'0'));
    archive.extend_from_slice(&pad_to_512(obj_data));
    archive.extend_from_slice(&[0u8; 1024]);

    let err = read_incident_container(&archive).expect_err("swapped order must be refused");
    assert_eq!(err.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(err.reason, Some(IncidentReason::InputShape));
}

/// Test 3: A fourth path (README) refuses input_shape.
#[test]
fn test_3_fourth_path_readme_refuses() {
    let inv_data = br#"{"schema":"assay.incident.inventory.v1"}"#;
    let ass_data = br#"{"schema":"assay.incident.assessment.v1"}"#;
    let obj_data = b"synthetic object payload bytes\n";
    let readme_data = b"README content\n";

    let (mut archive_prefix, _obj_path) = build_minimal_valid_archive(inv_data, ass_data, obj_data);
    // Remove the 1024 zero blocks at the end
    archive_prefix.truncate(archive_prefix.len() - 1024);

    // Append README
    archive_prefix.extend_from_slice(&make_test_header("README", readme_data.len(), b'0'));
    archive_prefix.extend_from_slice(&pad_to_512(readme_data));
    archive_prefix.extend_from_slice(&[0u8; 1024]);

    let err = read_incident_container(&archive_prefix).expect_err("README member must be refused");
    assert_eq!(err.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(err.reason, Some(IncidentReason::InputShape));
}

/// Test 4: An object path whose name is not the sha256 of its bytes refuses input_shape.
#[test]
fn test_4_object_path_digest_mismatch_refuses() {
    let inv_data = br#"{"schema":"assay.incident.inventory.v1"}"#;
    let ass_data = br#"{"schema":"assay.incident.assessment.v1"}"#;
    let obj_data = b"synthetic object payload bytes\n";
    // Mismatched digest: all zeros
    let wrong_path = "objects/0000000000000000000000000000000000000000000000000000000000000000";

    let mut archive = Vec::new();
    archive.extend_from_slice(&make_test_header("inventory.json", inv_data.len(), b'0'));
    archive.extend_from_slice(&pad_to_512(inv_data));
    archive.extend_from_slice(&make_test_header("assessment.json", ass_data.len(), b'0'));
    archive.extend_from_slice(&pad_to_512(ass_data));
    archive.extend_from_slice(&make_test_header(wrong_path, obj_data.len(), b'0'));
    archive.extend_from_slice(&pad_to_512(obj_data));
    archive.extend_from_slice(&[0u8; 1024]);

    let err = read_incident_container(&archive).expect_err("digest mismatch must be refused");
    assert_eq!(err.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(err.reason, Some(IncidentReason::InputShape));
}

/// Test 5: A GNU long-name header (typeflag 'L') refuses input_shape rather than being followed.
#[test]
fn test_5_gnu_long_name_header_refuses() {
    let inv_data = br#"{"schema":"assay.incident.inventory.v1"}"#;
    let ass_data = br#"{"schema":"assay.incident.assessment.v1"}"#;
    let obj_data = b"synthetic object payload bytes\n";

    let mut archive = Vec::new();
    // Prepend a GNU long name header before inventory.json
    let long_name_data = b"././@LongLink\0";
    archive.extend_from_slice(&make_test_header(
        "././@LongLink",
        long_name_data.len(),
        b'L',
    ));
    archive.extend_from_slice(&pad_to_512(long_name_data));
    archive.extend_from_slice(&make_test_header("inventory.json", inv_data.len(), b'0'));
    archive.extend_from_slice(&pad_to_512(inv_data));
    archive.extend_from_slice(&make_test_header("assessment.json", ass_data.len(), b'0'));
    archive.extend_from_slice(&pad_to_512(ass_data));
    let obj_digest = compute_sha256_hex(obj_data);
    let obj_path = format!("objects/{obj_digest}");
    archive.extend_from_slice(&make_test_header(&obj_path, obj_data.len(), b'0'));
    archive.extend_from_slice(&pad_to_512(obj_data));
    archive.extend_from_slice(&[0u8; 1024]);

    let err =
        read_incident_container(&archive).expect_err("GNU long-name typeflag L must be refused");
    assert_eq!(err.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(err.reason, Some(IncidentReason::InputShape));
}

/// Test 6: One trailing zero block refuses; three trailing zero blocks refuse; two plus a non-zero suffix refuses.
#[test]
fn test_6_trailing_zero_blocks_refusals() {
    let inv_data = br#"{"schema":"assay.incident.inventory.v1"}"#;
    let ass_data = br#"{"schema":"assay.incident.assessment.v1"}"#;
    let obj_data = b"synthetic object payload bytes\n";

    let (base_archive, _obj_path) = build_minimal_valid_archive(inv_data, ass_data, obj_data);
    // Base archive has exactly 1024 trailing zeros
    let payload_only = &base_archive[..base_archive.len() - 1024];

    // Case 6a: Exactly 1 zero block (512 bytes)
    let mut one_block = payload_only.to_vec();
    one_block.extend_from_slice(&[0u8; 512]);
    let err = read_incident_container(&one_block).expect_err("1 zero block must be refused");
    assert_eq!(err.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(err.reason, Some(IncidentReason::InputShape));

    // Case 6b: Exactly 3 zero blocks (1536 bytes)
    let mut three_blocks = payload_only.to_vec();
    three_blocks.extend_from_slice(&[0u8; 1536]);
    let err = read_incident_container(&three_blocks).expect_err("3 zero blocks must be refused");
    assert_eq!(err.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(err.reason, Some(IncidentReason::InputShape));

    // Case 6c: Exactly 2 zero blocks + trailing non-zero suffix (e.g. 1 non-zero byte)
    let mut suffix = base_archive.clone();
    suffix.push(0x42);
    let err =
        read_incident_container(&suffix).expect_err("trailing non-zero suffix must be refused");
    assert_eq!(err.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(err.reason, Some(IncidentReason::InputShape));
}

/// Test 7: A header whose checksum is right but whose device fields are non-zero refuses.
#[test]
fn test_7_non_zero_device_fields_refuses() {
    let inv_data = br#"{"schema":"assay.incident.inventory.v1"}"#;
    let ass_data = br#"{"schema":"assay.incident.assessment.v1"}"#;
    let obj_data = b"synthetic object payload bytes\n";
    let obj_digest = compute_sha256_hex(obj_data);
    let obj_path = format!("objects/{obj_digest}");

    // Create an inventory header with non-zero devmajor (bytes 329..337)
    let mut bad_header = make_test_header("inventory.json", inv_data.len(), b'0');
    // Set devmajor to non-zero
    bad_header[329..337].copy_from_slice(b"0000001\0");
    // Recompute valid checksum with spaces at 148..156
    bad_header[148..156].copy_from_slice(b"        ");
    let sum: u32 = bad_header.iter().map(|&b| b as u32).sum();
    let chksum_str = format!("{:06o}\0 ", sum);
    bad_header[148..156].copy_from_slice(chksum_str.as_bytes());

    let mut archive = Vec::new();
    archive.extend_from_slice(&bad_header);
    archive.extend_from_slice(&pad_to_512(inv_data));
    archive.extend_from_slice(&make_test_header("assessment.json", ass_data.len(), b'0'));
    archive.extend_from_slice(&pad_to_512(ass_data));
    archive.extend_from_slice(&make_test_header(&obj_path, obj_data.len(), b'0'));
    archive.extend_from_slice(&pad_to_512(obj_data));
    archive.extend_from_slice(&[0u8; 1024]);

    let err =
        read_incident_container(&archive).expect_err("non-zero device fields must be refused");
    assert_eq!(err.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(err.reason, Some(IncidentReason::InputShape));
}

/// Test 8: The refusal report for case 3 serializes byte-identically to Example C with reason "input_shape".
#[test]
fn test_8_refusal_report_byte_identical_to_example_c() {
    let inv_data = br#"{"schema":"assay.incident.inventory.v1"}"#;
    let ass_data = br#"{"schema":"assay.incident.assessment.v1"}"#;
    let obj_data = b"synthetic object payload bytes\n";
    let readme_data = b"README content\n";

    let (mut archive_prefix, _obj_path) = build_minimal_valid_archive(inv_data, ass_data, obj_data);
    archive_prefix.truncate(archive_prefix.len() - 1024);
    archive_prefix.extend_from_slice(&make_test_header("README", readme_data.len(), b'0'));
    archive_prefix.extend_from_slice(&pad_to_512(readme_data));
    archive_prefix.extend_from_slice(&[0u8; 1024]);

    let report = read_incident_container(&archive_prefix).expect_err("must be refused");

    // Canonical JCS representation of Example C with reason "input_shape"
    let expected_jcs_bytes =
        br#"{"artifact_sha256":null,"assessment_sha256":null,"attestations":[],"counts":null,"expectation":"not_evaluated","inventory_sha256":null,"non_claims":["no_activity_completeness","no_provider_outcome","no_automatic_trust"],"outcome":"package_refused","reason":"input_shape","resolved_results":[],"schema":"assay.incident.verify.v1","verification_context":null,"verification_context_sha256":null}"#;
    let mut expected_bytes_with_lf = expected_jcs_bytes.to_vec();
    expected_bytes_with_lf.push(b'\n');

    let serialized = report
        .to_canonical_bytes()
        .expect("serialization to canonical bytes must succeed");

    assert_eq!(serialized, expected_bytes_with_lf);

    // Also assert report constructed via constructor matches
    let direct_report = IncidentVerifyReport::refusal(IncidentReason::InputShape);
    assert_eq!(report, direct_report);
    assert_eq!(
        direct_report.to_canonical_bytes().unwrap(),
        expected_bytes_with_lf
    );
}

/// Test mutant M1: typeflag '5' (directory) refuses input_shape.
#[test]
fn test_mutant_m1_directory_typeflag_refuses() {
    let inv_data = br#"{"schema":"assay.incident.inventory.v1"}"#;
    let ass_data = br#"{"schema":"assay.incident.assessment.v1"}"#;
    let obj_data = b"synthetic object payload bytes\n";
    let obj_digest = compute_sha256_hex(obj_data);
    let obj_path = format!("objects/{obj_digest}");

    let mut archive = Vec::new();
    // Set typeflag to '5' (directory) for inventory
    archive.extend_from_slice(&make_test_header("inventory.json", inv_data.len(), b'5'));
    archive.extend_from_slice(&pad_to_512(inv_data));
    archive.extend_from_slice(&make_test_header("assessment.json", ass_data.len(), b'0'));
    archive.extend_from_slice(&pad_to_512(ass_data));
    archive.extend_from_slice(&make_test_header(&obj_path, obj_data.len(), b'0'));
    archive.extend_from_slice(&pad_to_512(obj_data));
    archive.extend_from_slice(&[0u8; 1024]);

    let err = read_incident_container(&archive).expect_err("typeflag '5' must be refused");
    assert_eq!(err.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(err.reason, Some(IncidentReason::InputShape));
}

/// Test mutant M2: case-sensitive paths required (uppercase refuses).
#[test]
fn test_mutant_m2_uppercase_path_refuses() {
    let inv_data = br#"{"schema":"assay.incident.inventory.v1"}"#;
    let ass_data = br#"{"schema":"assay.incident.assessment.v1"}"#;
    let obj_data = b"synthetic object payload bytes\n";
    let obj_digest = compute_sha256_hex(obj_data);
    let obj_path = format!("objects/{obj_digest}");

    // Test uppercase INVENTORY.JSON
    let mut archive1 = Vec::new();
    archive1.extend_from_slice(&make_test_header("INVENTORY.JSON", inv_data.len(), b'0'));
    archive1.extend_from_slice(&pad_to_512(inv_data));
    archive1.extend_from_slice(&make_test_header("assessment.json", ass_data.len(), b'0'));
    archive1.extend_from_slice(&pad_to_512(ass_data));
    archive1.extend_from_slice(&make_test_header(&obj_path, obj_data.len(), b'0'));
    archive1.extend_from_slice(&pad_to_512(obj_data));
    archive1.extend_from_slice(&[0u8; 1024]);

    let err1 =
        read_incident_container(&archive1).expect_err("uppercase INVENTORY.JSON must refuse");
    assert_eq!(err1.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(err1.reason, Some(IncidentReason::InputShape));

    // Test uppercase hex in object path
    let uppercase_digest = obj_digest.to_uppercase();
    let uppercase_obj_path = format!("objects/{uppercase_digest}");

    let mut archive2 = Vec::new();
    archive2.extend_from_slice(&make_test_header("inventory.json", inv_data.len(), b'0'));
    archive2.extend_from_slice(&pad_to_512(inv_data));
    archive2.extend_from_slice(&make_test_header("assessment.json", ass_data.len(), b'0'));
    archive2.extend_from_slice(&pad_to_512(ass_data));
    archive2.extend_from_slice(&make_test_header(&uppercase_obj_path, obj_data.len(), b'0'));
    archive2.extend_from_slice(&pad_to_512(obj_data));
    archive2.extend_from_slice(&[0u8; 1024]);

    let err2 =
        read_incident_container(&archive2).expect_err("uppercase hex in object path must refuse");
    assert_eq!(err2.outcome, IncidentOutcome::PackageRefused);
    assert_eq!(err2.reason, Some(IncidentReason::InputShape));
}
