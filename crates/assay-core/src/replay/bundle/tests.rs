use super::paths::validate_entry_path;
use super::*;
use crate::replay::manifest::{
    ReplayCoverage, ReplayManifest, ReplayOutputs, ReplaySeeds, ScrubPolicy,
};
use flate2::GzBuilder;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use tar::{Builder, Header};

#[test]
fn write_bundle_minimal_roundtrip() {
    let manifest = ReplayManifest::minimal("2.15.0".into());
    let entries = vec![BundleEntry {
        path: "outputs/summary.json".into(),
        data: br#"{"schema_version":1}"#.to_vec(),
    }];
    let mut buf = Vec::new();
    write_bundle_tar_gz(&mut buf, &manifest, &entries).unwrap();
    assert!(!buf.is_empty());
    let digest = bundle_digest(&manifest, &entries).unwrap();
    assert_eq!(digest.len(), 64);
}

#[test]
fn read_bundle_roundtrip() {
    let manifest = ReplayManifest::minimal("2.15.0".into());
    let entries = vec![
        BundleEntry {
            path: "files/trace.jsonl".into(),
            data: b"[]".to_vec(),
        },
        BundleEntry {
            path: "outputs/summary.json".into(),
            data: br#"{"schema_version":1}"#.to_vec(),
        },
    ];
    let mut buf = Vec::new();
    write_bundle_tar_gz(&mut buf, &manifest, &entries).unwrap();
    let read = read_bundle_tar_gz(std::io::Cursor::new(&buf)).unwrap();
    assert_eq!(read.manifest.schema_version, manifest.schema_version);
    assert_eq!(read.manifest.assay_version, manifest.assay_version);
    let paths: std::collections::BTreeSet<_> =
        read.entries.iter().map(|(p, _)| p.as_str()).collect();
    assert!(paths.contains("files/trace.jsonl"));
    assert!(paths.contains("outputs/summary.json"));
    let data: std::collections::BTreeMap<_, _> = read.entries.into_iter().collect();
    assert_eq!(data.get("files/trace.jsonl").unwrap(), &b"[]"[..]);
}

/// Reader fails when manifest.json is absent (same policy: bundle must be valid).
#[test]
fn read_bundle_fails_manifest_missing() {
    let mut buf = Vec::new();
    let gz = GzBuilder::new()
        .mtime(0)
        .write(&mut buf, flate2::Compression::default());
    let mut tar = Builder::new(gz);
    let mut header = Header::new_gnu();
    header.set_path("files/x").unwrap();
    header.set_size(0);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append(&header, &[] as &[u8]).unwrap();
    let gz = tar.into_inner().unwrap();
    gz.finish().unwrap();
    let err = read_bundle_tar_gz(std::io::Cursor::new(&buf)).unwrap_err();
    assert!(err.to_string().contains("manifest.json missing"), "{}", err);
}

/// Duplicate path in tar → Error (avoids zip-slip style confusion; last-wins undefined).
#[test]
fn read_bundle_fails_duplicate_path() {
    let manifest = ReplayManifest::minimal("2.15.0".into());
    let manifest_json = serde_json::to_vec(&manifest).unwrap();
    let mut buf = Vec::new();
    let gz = GzBuilder::new()
        .mtime(0)
        .write(&mut buf, flate2::Compression::default());
    let mut tar = Builder::new(gz);
    tar.mode(tar::HeaderMode::Deterministic);
    let mut h = Header::new_gnu();
    h.set_path(paths::MANIFEST).unwrap();
    h.set_size(manifest_json.len() as u64);
    h.set_mode(0o644);
    h.set_cksum();
    tar.append(&h, &manifest_json[..]).unwrap();
    for _ in 0..2 {
        let mut h2 = Header::new_gnu();
        h2.set_path("files/x").unwrap();
        h2.set_size(1);
        h2.set_mode(0o644);
        h2.set_cksum();
        tar.append(&h2, &b"x"[..]).unwrap();
    }
    let gz = tar.into_inner().unwrap();
    gz.finish().unwrap();
    let err = read_bundle_tar_gz(std::io::Cursor::new(&buf)).unwrap_err();
    assert!(err.to_string().contains("duplicate path"), "{}", err);
}

#[test]
fn build_file_manifest_normalizes_paths() {
    let entries = vec![BundleEntry {
        path: "files/trace.jsonl".into(),
        data: vec![1, 2, 3],
    }];
    let manifest_map = build_file_manifest(&entries).unwrap();
    assert_eq!(manifest_map.len(), 1);
    let entry = manifest_map.get("files/trace.jsonl").unwrap();
    assert_eq!(entry.size, 3);
    assert!(entry.sha256.starts_with("sha256:"));
}

/// Legitimate filename with ".." in segment (not traversal) is allowed.
#[test]
fn path_segment_dotdot_allows_literal_dotdot_in_filename() {
    let manifest = ReplayManifest::minimal("2.15.0".into());
    let entries = vec![BundleEntry {
        path: "files/a..b.txt".into(),
        data: b"ok".to_vec(),
    }];
    let mut buf = Vec::new();
    write_bundle_tar_gz(&mut buf, &manifest, &entries).unwrap();
    let names = list_tar_gz_paths(&buf);
    assert!(names.contains(&"files/a..b.txt".to_string()));
}

/// Non-canonical prefix (evil.txt, x/y) rejected.
#[test]
fn path_must_have_canonical_prefix() {
    let manifest = ReplayManifest::minimal("2.15.0".into());
    for bad in ["evil.txt", "x/y/z", "output/run.json"] {
        let entries = vec![BundleEntry {
            path: bad.to_string(),
            data: vec![],
        }];
        let err = write_bundle_tar_gz(&mut Vec::new(), &manifest, &entries).unwrap_err();
        assert!(
            err.to_string().contains("invalid bundle path prefix"),
            "{}",
            bad
        );
    }
}

/// Empty segment (duplicate slash) rejected.
#[test]
fn path_rejects_empty_segment() {
    let manifest = ReplayManifest::minimal("2.15.0".into());
    let entries = vec![BundleEntry {
        path: "files//x.json".into(),
        data: vec![],
    }];
    let err = write_bundle_tar_gz(&mut Vec::new(), &manifest, &entries).unwrap_err();
    assert!(err.to_string().contains("empty segment"), "files//x");
}

/// Windows drive-letter-like path rejected.
#[test]
fn path_rejects_drive_letter() {
    let manifest = ReplayManifest::minimal("2.15.0".into());
    for bad in ["C:/foo", "C:\\foo", "D:bar"] {
        let entries = vec![BundleEntry {
            path: bad.to_string(),
            data: vec![],
        }];
        let err = write_bundle_tar_gz(&mut Vec::new(), &manifest, &entries).unwrap_err();
        assert!(
            err.to_string().contains("drive-letter") || err.to_string().contains("first segment"),
            "{}",
            bad
        );
    }
}

/// build_file_manifest fail-closed: invalid path returns Err (same policy as writer).
#[test]
fn build_file_manifest_fail_closed_on_invalid_path() {
    let entries = vec![
        BundleEntry {
            path: "files/ok.json".into(),
            data: vec![],
        },
        BundleEntry {
            path: "../secrets.txt".into(),
            data: vec![],
        },
    ];
    let err = build_file_manifest(&entries).unwrap_err();
    assert!(err.to_string().contains("invalid bundle path"));
}

/// Audit: digest of written bytes equals bundle_digest(manifest, entries).
#[test]
fn bundle_digest_equals_sha256_of_written_bytes() {
    let manifest = ReplayManifest::minimal("2.15.0".into());
    let entries = vec![
        BundleEntry {
            path: "files/trace.jsonl".into(),
            data: b"[]".to_vec(),
        },
        BundleEntry {
            path: "outputs/summary.json".into(),
            data: b"{}".to_vec(),
        },
    ];
    let mut buf = Vec::new();
    write_bundle_tar_gz(&mut buf, &manifest, &entries).unwrap();
    let digest_from_fn = bundle_digest(&manifest, &entries).unwrap();
    let hash_of_bytes = hex::encode(Sha256::digest(&buf));
    assert_eq!(
        digest_from_fn, hash_of_bytes,
        "bundle_digest must equal sha256(written bytes)"
    );
}

/// Audit: path traversal (..) and empty path rejected; no .. or absolute in output.
#[test]
fn path_traversal_rejected_and_output_has_no_traversal() {
    let manifest = ReplayManifest::minimal("2.15.0".into());
    for bad_path in [
        "../secrets.txt",
        "files/../../etc/passwd",
        "outputs/../leak",
        "",
    ] {
        let entries = vec![BundleEntry {
            path: bad_path.to_string(),
            data: vec![],
        }];
        let mut buf = Vec::new();
        let err = write_bundle_tar_gz(&mut buf, &manifest, &entries).unwrap_err();
        assert!(
            err.to_string().contains("invalid bundle path"),
            "{}",
            bad_path
        );
    }
    // Leading slash and backslash are normalized; result must not be in archive as absolute/traversal
    let entries = vec![
        BundleEntry {
            path: "files/trace.jsonl".into(),
            data: b"[]".to_vec(),
        },
        BundleEntry {
            path: "outputs/run.json".into(),
            data: b"{}".to_vec(),
        },
    ];
    let mut buf = Vec::new();
    write_bundle_tar_gz(&mut buf, &manifest, &entries).unwrap();
    let names = list_tar_gz_paths(&buf);
    for name in &names {
        assert!(!name.contains(".."), "no .. in archive path: {}", name);
        assert!(
            !name.starts_with('/'),
            "no leading / in archive path: {}",
            name
        );
    }
    assert!(names.iter().any(|s| s == "manifest.json"));
    assert!(names.iter().any(|s| s.starts_with("files/")));
    assert!(names.iter().any(|s| s.starts_with("outputs/")));
}

/// Audit: full manifest (replay_coverage, seeds, scrub_policy) and canonical layout.
#[test]
fn audit_full_manifest_and_canonical_layout() {
    let mut reason = BTreeMap::new();
    reason.insert(
        "test_b".to_string(),
        "judge response not cached".to_string(),
    );
    let manifest = ReplayManifest {
        schema_version: 1,
        assay_version: "2.15.0".to_string(),
        created_at: Some("2025-01-27T12:00:00Z".to_string()),
        source_run_path: Some(".assay/run_abc123".to_string()),
        selection_method: Some("run-id".to_string()),
        git_sha: Some("a1b2c3d4e5f6".to_string()),
        git_dirty: Some(false),
        workflow_run_id: None,
        config_digest: None,
        policy_digest: None,
        baseline_digest: None,
        trace_digest: None,
        trace_path: Some("files/trace.jsonl".to_string()),
        outputs: Some(ReplayOutputs {
            run: Some("outputs/run.json".to_string()),
            summary: Some("outputs/summary.json".to_string()),
            junit: None,
            sarif: None,
        }),
        toolchain: None,
        seeds: Some(ReplaySeeds {
            seed_version: Some(1),
            order_seed: Some("42".to_string()),
            judge_seed: None,
        }),
        replay_coverage: Some(ReplayCoverage {
            complete_tests: vec!["test_a".to_string()],
            incomplete_tests: vec!["test_b".to_string()],
            reason: Some(reason),
        }),
        scrub_policy: Some(ScrubPolicy::default()),
        files: None,
        env: None,
    };
    let entries = vec![
        BundleEntry {
            path: "files/trace.jsonl".into(),
            data: b"[]".to_vec(),
        },
        BundleEntry {
            path: "outputs/run.json".into(),
            data: b"{}".to_vec(),
        },
        BundleEntry {
            path: "outputs/summary.json".into(),
            data: b"{}".to_vec(),
        },
        BundleEntry {
            path: "cassettes/.gitkeep".into(),
            data: vec![],
        },
    ];
    let mut buf = Vec::new();
    write_bundle_tar_gz(&mut buf, &manifest, &entries).unwrap();
    let names = list_tar_gz_paths(&buf);
    assert!(
        names.contains(&"manifest.json".to_string()),
        "canonical: manifest at root"
    );
    assert!(names
        .iter()
        .all(|p| !p.contains("..") && !p.starts_with('/')));
    assert!(names.contains(&"manifest.json".to_string()));
    assert!(names.iter().any(|p| p.starts_with("files/")));
    assert!(names.iter().any(|p| p.starts_with("outputs/")));
    assert!(names.iter().any(|p| p.starts_with("cassettes/")));
}

// --- Gap 1: Golden-value snapshot test ---

/// Pinned digest: catches silent reproducibility regressions (serde field order,
/// flate2 compression defaults, tar header changes).
#[test]
fn golden_digest_snapshot() {
    let manifest = ReplayManifest::minimal("2.15.0".into());
    let entries = vec![BundleEntry {
        path: "files/trace.jsonl".into(),
        data: b"[]".to_vec(),
    }];
    let digest = bundle_digest(&manifest, &entries).unwrap();
    assert_eq!(
        digest, "e982d2dd1d7cf56df6b417c7af1bc3f7f334ecfc47298bf5d240f4485f3b7a7c",
        "Golden digest changed — if intentional, update this value after verifying \
             that the new output is still deterministic across platforms"
    );
}

// --- Gap 2: Fix helper + sort-order test ---

/// Returns tar entry paths in **archive order** (no sorting).
fn list_tar_gz_paths(gz: &[u8]) -> Vec<String> {
    let dec = flate2::read::GzDecoder::new(gz);
    let mut ar = tar::Archive::new(dec);
    let mut names = Vec::new();
    for e in ar.entries().unwrap() {
        let e = e.unwrap();
        let path = e.path().unwrap();
        names.push(path.to_string_lossy().replace('\\', "/"));
    }
    names
}

/// Writer must emit entries in sorted order (after manifest). Entries given
/// out-of-order must appear sorted in the archive.
#[test]
fn entries_written_in_sorted_order() {
    let manifest = ReplayManifest::minimal("2.15.0".into());
    // Provide entries deliberately out of sorted order.
    let entries = vec![
        BundleEntry {
            path: "outputs/z.json".into(),
            data: b"{}".to_vec(),
        },
        BundleEntry {
            path: "files/a.jsonl".into(),
            data: b"[]".to_vec(),
        },
        BundleEntry {
            path: "cassettes/m.json".into(),
            data: b"{}".to_vec(),
        },
    ];
    let mut buf = Vec::new();
    write_bundle_tar_gz(&mut buf, &manifest, &entries).unwrap();
    let names = list_tar_gz_paths(&buf);
    assert_eq!(names[0], "manifest.json", "manifest must be first");
    let data_entries: Vec<_> = names[1..].to_vec();
    let mut expected = data_entries.clone();
    expected.sort();
    assert_eq!(
        data_entries, expected,
        "entries after manifest must be in sorted order"
    );
}

// --- Gap 3: Direct unit tests for validate_entry_path ---

#[test]
fn validate_entry_path_accepts_valid_paths() {
    for good in [
        "files/trace.jsonl",
        "outputs/run.json",
        "cassettes/openai/embed.json",
        "files/a..b.txt",
        "files/deep/nested/dir/file.json",
    ] {
        let result = validate_entry_path(good);
        assert!(result.is_ok(), "should accept: {}", good);
        assert_eq!(result.unwrap(), good, "valid path returned unchanged");
    }
}

#[test]
fn validate_entry_path_normalizes_backslash_and_leading_slash() {
    assert_eq!(
        validate_entry_path("files\\trace.jsonl").unwrap(),
        "files/trace.jsonl"
    );
    assert_eq!(
        validate_entry_path("/files/trace.jsonl").unwrap(),
        "files/trace.jsonl"
    );
    assert_eq!(
        validate_entry_path("\\files\\trace.jsonl").unwrap(),
        "files/trace.jsonl"
    );
}

#[test]
fn validate_entry_path_rejects_empty() {
    let err = validate_entry_path("").unwrap_err();
    assert!(err.to_string().contains("empty path"));
}

#[test]
fn validate_entry_path_rejects_empty_segment() {
    let err = validate_entry_path("files//x.json").unwrap_err();
    assert!(err.to_string().contains("empty segment"));
}

#[test]
fn validate_entry_path_rejects_dot_segments() {
    for bad in ["files/./x.json", "files/../x.json", "outputs/.."] {
        let err = validate_entry_path(bad).unwrap_err();
        assert!(
            err.to_string().contains("traversal segment"),
            "should reject: {}",
            bad
        );
    }
}

#[test]
fn validate_entry_path_rejects_drive_letter() {
    for bad in ["C:/foo", "D:bar"] {
        let err = validate_entry_path(bad).unwrap_err();
        assert!(
            err.to_string().contains("drive-letter"),
            "should reject: {}",
            bad
        );
    }
}

#[test]
fn validate_entry_path_rejects_non_canonical_prefix() {
    for bad in ["evil.txt", "x/y/z", "output/run.json", "file/x.json"] {
        let err = validate_entry_path(bad).unwrap_err();
        assert!(
            err.to_string().contains("invalid bundle path prefix"),
            "should reject: {}",
            bad
        );
    }
}
