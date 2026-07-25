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

mod bounded_ingest {
    //! ADR-043 §1 for the replay reader.
    //!
    //! `read_bundle_tar_gz` had no ceilings at all: an unbounded compressed source, an unbounded
    //! gzip decoder on top of it, and `read_to_end` per member on top of that. The evidence
    //! verifier had a limit set and replay had none, so the same archive met two different
    //! contracts depending on which reader opened it.
    //!
    //! Each test pins one dimension and pairs it with an acceptance case built from the same
    //! bundle, so a ceiling that refused everything would not pass.

    use super::super::{
        read_bundle_tar_gz_with_limits, write_bundle_tar_gz, BundleEntry, ReplayIngestError,
        ReplayLimits,
    };
    use crate::replay::manifest::ReplayManifest;
    use assay_common::limits::LimitKind;
    use std::cell::Cell;
    use std::io::{Cursor, Read};
    use std::rc::Rc;

    fn bundle_with(entries: Vec<BundleEntry>) -> Vec<u8> {
        let manifest = ReplayManifest::minimal("2.15.0".into());
        let mut buf = Vec::new();
        write_bundle_tar_gz(&mut buf, &manifest, &entries).expect("write bundle");
        buf
    }

    fn small_bundle() -> Vec<u8> {
        // Deliberately incompressible: a run of identical bytes gzips to almost nothing, which
        // would leave the compressed fixture smaller than the ceiling under test and make the
        // assertion vacuous.
        let data: Vec<u8> = (0..4096u32)
            .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
            .collect();
        bundle_with(vec![BundleEntry {
            path: "files/trace.jsonl".into(),
            data,
        }])
    }

    /// Counts what the reader actually pulled, so a test can tell bounded ingest from a late
    /// rejection. Asserting only that the call failed would pass against an unbounded reader
    /// that drained the whole archive first.
    struct Counting<R> {
        inner: R,
        pulled: Rc<Cell<u64>>,
    }

    impl<R: Read> Read for Counting<R> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.inner.read(buf)?;
            self.pulled.set(self.pulled.get() + n as u64);
            Ok(n)
        }
    }

    fn counting(bytes: Vec<u8>) -> (Counting<Cursor<Vec<u8>>>, Rc<Cell<u64>>) {
        let pulled = Rc::new(Cell::new(0));
        (
            Counting {
                inner: Cursor::new(bytes),
                pulled: Rc::clone(&pulled),
            },
            pulled,
        )
    }

    #[test]
    fn the_compressed_source_stops_being_read_at_the_ceiling() {
        let bytes = small_bundle();
        let ceiling = 64u64;
        assert!((bytes.len() as u64) > ceiling * 4);

        let (source, pulled) = counting(bytes.clone());
        let limits = ReplayLimits {
            max_source_bytes: ceiling,
            ..ReplayLimits::default()
        };
        assert!(read_bundle_tar_gz_with_limits(source, limits).is_err());
        assert!(
            pulled.get() <= ceiling + 1,
            "ingest must stop at the ceiling; {} bytes were pulled",
            pulled.get()
        );

        read_bundle_tar_gz_with_limits(Cursor::new(bytes), ReplayLimits::default())
            .expect("the same bundle must read under the default limits");
    }

    #[test]
    fn a_member_over_its_ceiling_is_refused_with_a_typed_cause() {
        let bytes = small_bundle();
        let limits = ReplayLimits {
            max_member_bytes: 64,
            ..ReplayLimits::default()
        };
        let err = read_bundle_tar_gz_with_limits(Cursor::new(bytes.clone()), limits)
            .expect_err("a 4 KiB member must be refused under a 64 byte ceiling");
        match err.downcast_ref::<ReplayIngestError>() {
            Some(ReplayIngestError::MemberCeiling { member, kind, .. }) => {
                assert_eq!(member, "files/trace.jsonl");
                assert_eq!(*kind, LimitKind::MemberBytes);
            }
            other => panic!("expected a typed MemberCeiling, got {other:?}"),
        }

        read_bundle_tar_gz_with_limits(Cursor::new(bytes), ReplayLimits::default())
            .expect("the same member must read under the default ceiling");
    }

    #[test]
    fn an_entry_path_over_the_ceiling_is_refused_before_it_is_read() {
        // The tar writer refuses names past the ustar limit without PAX extensions, so the
        // fixture cannot carry a path long enough to cross the default 256 byte ceiling. Lower
        // the ceiling under an ordinary path instead: the behaviour under test is the check, not
        // the size of the number.
        let path = "files/trace.jsonl";
        let bytes = bundle_with(vec![BundleEntry {
            path: path.into(),
            data: b"[]".to_vec(),
        }]);
        let tight = ReplayLimits {
            max_path_len: path.len() - 1,
            ..ReplayLimits::default()
        };
        let err = read_bundle_tar_gz_with_limits(Cursor::new(bytes.clone()), tight)
            .expect_err("a path one byte over the ceiling must be refused");
        match err.downcast_ref::<ReplayIngestError>() {
            Some(ReplayIngestError::PathTooLong { limit }) => {
                assert_eq!(*limit, path.len() - 1)
            }
            other => panic!("expected a typed PathTooLong, got {other:?}"),
        }

        let exact = ReplayLimits {
            max_path_len: path.len(),
            ..ReplayLimits::default()
        };
        read_bundle_tar_gz_with_limits(Cursor::new(bytes), exact)
            .expect("a path of exactly the ceiling must be accepted");
    }

    #[test]
    fn too_many_entries_is_refused_with_a_typed_cause() {
        let entries: Vec<BundleEntry> = (0..8)
            .map(|i| BundleEntry {
                path: format!("files/e{i}.jsonl"),
                data: b"[]".to_vec(),
            })
            .collect();
        let bytes = bundle_with(entries);

        let limits = ReplayLimits {
            max_entries: 3,
            ..ReplayLimits::default()
        };
        let err = read_bundle_tar_gz_with_limits(Cursor::new(bytes.clone()), limits)
            .expect_err("nine members must exceed a ceiling of three");
        match err.downcast_ref::<ReplayIngestError>() {
            Some(ReplayIngestError::TooManyEntries { limit }) => assert_eq!(*limit, 3),
            other => panic!("expected a typed TooManyEntries, got {other:?}"),
        }

        read_bundle_tar_gz_with_limits(Cursor::new(bytes), ReplayLimits::default())
            .expect("the same bundle must read under the default entry ceiling");
    }

    /// The manifest ceiling is about shape, not size: a small document can nest deeply enough to
    /// exhaust the parser, and `max_manifest_bytes` says nothing about that.
    #[test]
    fn a_manifest_nested_past_the_ceiling_is_refused() {
        let bytes = small_bundle();
        let tight = ReplayLimits {
            max_manifest_json_depth: 1,
            ..ReplayLimits::default()
        };
        let err = read_bundle_tar_gz_with_limits(Cursor::new(bytes.clone()), tight)
            .expect_err("a manifest deeper than one level must be refused");
        match err.downcast_ref::<ReplayIngestError>() {
            Some(ReplayIngestError::ManifestTooDeep { limit }) => assert_eq!(*limit, 1),
            other => panic!("expected a typed ManifestTooDeep, got {other:?}"),
        }

        read_bundle_tar_gz_with_limits(Cursor::new(bytes), ReplayLimits::default())
            .expect("the same manifest must read under the default depth");
    }

    /// Exactly the ceiling is accepted and one deeper is refused, pinned against the real
    /// manifest rather than a hand-built document, so the boundary is the one callers meet.
    #[test]
    fn the_manifest_depth_boundary_is_exact() {
        use super::super::limits::check_manifest_json_depth;

        // `{"a":{"b":1}}` is two levels of nesting.
        let doc = br#"{"a":{"b":1}}"#;
        assert!(
            check_manifest_json_depth(doc, 2).is_ok(),
            "exactly the ceiling must pass"
        );
        assert!(
            check_manifest_json_depth(doc, 1).is_err(),
            "one level over the ceiling must be refused"
        );

        // Braces inside a string are not structure and must not count towards depth.
        let stringy = br#"{"a":"{{{{{{{{"}"#;
        assert!(
            check_manifest_json_depth(stringy, 1).is_ok(),
            "braces inside a string literal must not be counted as nesting"
        );
    }

    /// A bundle that is small compressed and large expanded. The source ceiling says nothing
    /// about what the input decompresses to, which is the whole reason the decode ceiling has to
    /// exist separately.
    #[test]
    fn expansion_past_the_decode_ceiling_is_refused() {
        let bytes = bundle_with(vec![BundleEntry {
            path: "files/pad.jsonl".into(),
            data: vec![b'A'; 512 * 1024],
        }]);
        let limits = ReplayLimits {
            max_source_bytes: bytes.len() as u64 * 8,
            max_decoded_bytes: 4096,
            ..ReplayLimits::default()
        };
        let err = read_bundle_tar_gz_with_limits(Cursor::new(bytes.clone()), limits)
            .expect_err("expansion past the decode ceiling must be refused");
        match err.downcast_ref::<ReplayIngestError>() {
            Some(ReplayIngestError::SourceCeiling { kind, .. }) => {
                assert_eq!(*kind, LimitKind::DecodedBytes)
            }
            Some(ReplayIngestError::MemberCeiling { kind, .. }) => {
                assert_eq!(*kind, LimitKind::DecodedBytes)
            }
            other => panic!("expected a decode ceiling refusal, got {other:?}"),
        }

        read_bundle_tar_gz_with_limits(Cursor::new(bytes), ReplayLimits::default())
            .expect("the same bundle must read under the default decode ceiling");
    }
}
