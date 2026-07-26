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
    assert_eq!(
        err.downcast_ref::<ReplayContractError>(),
        Some(&ReplayContractError::DuplicatePath),
        "duplicates are a typed contract violation, not a rendered string: {err}"
    );
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

    /// The promise of `read_verify_bounded` is that three things describe one snapshot. Nothing
    /// tested that promise: the digest could have been computed over a different read and every
    /// other assertion would still have passed.
    #[test]
    fn the_bounded_helper_binds_digest_parse_and_verdict_to_one_read() {
        use crate::replay::read_verify_bounded;
        use sha2::{Digest, Sha256};

        let bytes = small_bundle();
        let expected = format!("sha256:{}", hex::encode(Sha256::digest(&bytes)));

        let (source, pulled) = counting(bytes.clone());
        let snapshot = match read_verify_bounded(source, ReplayLimits::default()) {
            Ok(s) => s,
            Err(e) => panic!("a well-formed bundle must read, digest and verify: {e}"),
        };

        assert_eq!(
            snapshot.source_digest, expected,
            "the digest must cover exactly the bytes that were read"
        );
        assert_eq!(
            pulled.get(),
            bytes.len() as u64,
            "the source must be consumed once; {} bytes pulled for a {} byte input",
            pulled.get(),
            bytes.len()
        );
        assert!(
            snapshot.verify.errors.is_empty(),
            "the verdict must belong to the same snapshot: {:?}",
            snapshot.verify.errors
        );
        assert!(
            snapshot
                .read
                .entries
                .iter()
                .any(|(p, _)| p == "files/trace.jsonl"),
            "the parsed bundle must come from that same read"
        );
    }

    /// The helper is the recommended entrypoint, so it has to carry the same typed contract as
    /// the reader it wraps.
    ///
    /// The source boundary is exact on both entrypoints now. Both snapshot the source to EOF
    /// under the ceiling before parsing, so every byte counts, including a suffix no parser
    /// reads. An earlier revision bounded only the prefix tar consumed and could not express this
    /// boundary at all. Wrapping the io error in context first buried the cause and returned
    /// a weaker refusal than the lower-level call.
    #[test]
    fn the_bounded_helper_reports_a_typed_source_ceiling() {
        use crate::replay::read_verify_bounded;

        let bytes = small_bundle();
        let ceiling = bytes.len() as u64 - 1;
        let limits = ReplayLimits {
            max_source_bytes: ceiling,
            ..ReplayLimits::default()
        };
        let err = read_verify_bounded(Cursor::new(bytes.clone()), limits)
            .expect_err("one byte over the source ceiling must be refused");
        match err.ingest_refusal() {
            Some(ReplayIngestError::SourceCeiling { kind, limit }) => {
                assert_eq!(*kind, LimitKind::SourceBytes);
                assert_eq!(*limit, ceiling);
            }
            other => panic!("expected a typed SourceCeiling from the helper, got {other:?}"),
        }
        assert!(
            err.source_digest.is_none(),
            "the source itself could not be read, so there is nothing to attest"
        );

        let exact = ReplayLimits {
            max_source_bytes: bytes.len() as u64,
            ..ReplayLimits::default()
        };
        assert!(
            read_verify_bounded(Cursor::new(bytes), exact).is_ok(),
            "a source of exactly the ceiling must be accepted"
        );
    }

    /// The ceiling has to cover the whole source, not the prefix tar chooses to consume. A valid
    /// bundle followed by an arbitrary suffix used to pass a ceiling far below the real input
    /// size, because the trailing bytes were never requested and therefore never counted.
    #[test]
    fn a_valid_bundle_with_an_unread_suffix_is_measured_whole() {
        let valid = small_bundle();
        let mut with_suffix = valid.clone();
        with_suffix.extend(std::iter::repeat_n(b'Z', 10_000));
        let total = with_suffix.len() as u64;

        read_bundle_tar_gz_with_limits(
            Cursor::new(with_suffix.clone()),
            ReplayLimits {
                max_source_bytes: total,
                ..ReplayLimits::default()
            },
        )
        .expect("exactly the real input size must be accepted, suffix included");

        let err = read_bundle_tar_gz_with_limits(
            Cursor::new(with_suffix),
            ReplayLimits {
                max_source_bytes: total - 1,
                ..ReplayLimits::default()
            },
        )
        .expect_err("the suffix must count towards the source ceiling");
        match err.downcast_ref::<ReplayIngestError>() {
            Some(ReplayIngestError::SourceCeiling { kind, limit }) => {
                assert_eq!(*kind, LimitKind::SourceBytes);
                assert_eq!(*limit, total - 1);
            }
            other => panic!("expected a typed SourceCeiling, got {other:?}"),
        }

        read_bundle_tar_gz_with_limits(
            Cursor::new(valid.clone()),
            ReplayLimits {
                max_source_bytes: valid.len() as u64,
                ..ReplayLimits::default()
            },
        )
        .expect("the bundle alone must still read");
    }

    #[test]
    fn the_member_boundary_is_exact() {
        let payload = vec![b'z'; 4096];
        let bytes = bundle_with(vec![BundleEntry {
            path: "files/trace.jsonl".into(),
            data: payload.clone(),
        }]);
        let size = payload.len() as u64;

        read_bundle_tar_gz_with_limits(
            Cursor::new(bytes.clone()),
            ReplayLimits {
                max_member_bytes: size,
                ..ReplayLimits::default()
            },
        )
        .expect("a member of exactly the ceiling must be accepted");

        let err = read_bundle_tar_gz_with_limits(
            Cursor::new(bytes),
            ReplayLimits {
                max_member_bytes: size - 1,
                ..ReplayLimits::default()
            },
        )
        .expect_err("one byte over the member ceiling must be refused");
        match err.downcast_ref::<ReplayIngestError>() {
            Some(ReplayIngestError::MemberCeiling { kind, limit }) => {
                assert_eq!(*kind, LimitKind::MemberBytes);
                assert_eq!(*limit, size - 1);
            }
            other => panic!("expected a typed MemberCeiling, got {other:?}"),
        }
    }

    #[test]
    fn the_manifest_boundary_is_exact() {
        let manifest = ReplayManifest::minimal("2.15.0".into());
        // The writer serializes the manifest with `serde_json::to_vec`, so the same call gives
        // the exact member size the ceiling is compared against.
        let size = serde_json::to_vec(&manifest)
            .expect("serialize manifest")
            .len() as u64;
        let bytes = small_bundle();

        read_bundle_tar_gz_with_limits(
            Cursor::new(bytes.clone()),
            ReplayLimits {
                max_manifest_bytes: size,
                ..ReplayLimits::default()
            },
        )
        .expect("a manifest of exactly the ceiling must be accepted");

        let err = read_bundle_tar_gz_with_limits(
            Cursor::new(bytes),
            ReplayLimits {
                max_manifest_bytes: size - 1,
                ..ReplayLimits::default()
            },
        )
        .expect_err("one byte over the manifest ceiling must be refused");
        match err.downcast_ref::<ReplayIngestError>() {
            Some(ReplayIngestError::MemberCeiling { kind, limit }) => {
                assert_eq!(*kind, LimitKind::MemberBytes);
                assert_eq!(*limit, size - 1);
            }
            other => panic!("expected a typed MemberCeiling on the manifest, got {other:?}"),
        }
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
        let err = read_bundle_tar_gz_with_limits(source, limits)
            .expect_err("a bundle over the source ceiling must be refused");
        match err.downcast_ref::<ReplayIngestError>() {
            Some(ReplayIngestError::SourceCeiling { kind, limit }) => {
                assert_eq!(*kind, LimitKind::SourceBytes);
                assert_eq!(*limit, ceiling);
            }
            other => panic!("expected a typed SourceCeiling, got {other:?}"),
        }
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
            Some(ReplayIngestError::MemberCeiling { kind, limit }) => {
                assert_eq!(*kind, LimitKind::MemberBytes);
                assert_eq!(*limit, 64);
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

    /// `max_manifest_bytes` had no test of its own: the member ceiling covered entry bodies and
    /// the manifest was assumed to travel with them. It does not, it has its own dimension, and
    /// its refusal has to be distinguishable.
    #[test]
    fn a_manifest_over_its_own_ceiling_is_refused() {
        let bytes = small_bundle();
        let tight = ReplayLimits {
            max_manifest_bytes: 8,
            ..ReplayLimits::default()
        };
        let err = read_bundle_tar_gz_with_limits(Cursor::new(bytes.clone()), tight)
            .expect_err("a manifest over its own ceiling must be refused");
        match err.downcast_ref::<ReplayIngestError>() {
            Some(ReplayIngestError::MemberCeiling { kind, limit }) => {
                assert_eq!(*kind, LimitKind::MemberBytes);
                assert_eq!(*limit, 8, "the manifest ceiling must be the one reported");
            }
            other => panic!("expected a typed MemberCeiling on the manifest, got {other:?}"),
        }

        read_bundle_tar_gz_with_limits(Cursor::new(bytes), ReplayLimits::default())
            .expect("the same manifest must read under the default ceiling");
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

    /// Exactly the ceiling is accepted and one deeper is refused. This exercises the depth
    /// counter directly on hand-built documents, because a written manifest has a fixed shape and
    /// cannot express the +/-1 boundary; the end-to-end case above covers the real manifest.
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
        // One expected variant, not "either of two". The decoder sits above the tar walker, so
        // the expansion ceiling trips while the walker is pulling an entry body, which is the
        // member-scoped classification path.
        // The decoder sits under the tar walker, so the expansion ceiling trips through a member
        // read. It is still a decode refusal and must not be reported as a member one: a reader
        // deciding whether to raise `max_member_bytes` or `max_decoded_bytes` needs the right
        // dimension.
        match err.downcast_ref::<ReplayIngestError>() {
            Some(ReplayIngestError::SourceCeiling { kind, limit }) => {
                assert_eq!(*kind, LimitKind::DecodedBytes);
                assert_eq!(*limit, 4096);
            }
            other => panic!("expected a typed DecodedBytes refusal, got {other:?}"),
        }

        read_bundle_tar_gz_with_limits(Cursor::new(bytes), ReplayLimits::default())
            .expect("the same bundle must read under the default decode ceiling");
    }
}

/// Build a tar whose entry names are written straight into the header blocks, bypassing the
/// writer's own relative-path rule.
///
/// `tar::Builder` refuses to emit an absolute name, so the hostile fixture cannot be produced
/// through our own writer at all. That is not a defence: an attacker writes the bytes directly.
/// Each entry is first appended under a same-length placeholder, then the name field is patched
/// in place and the header checksum recomputed, which keeps every block offset stable.
fn raw_tar_gz_with_entry_names(names: &[&str]) -> Vec<u8> {
    let manifest = ReplayManifest::minimal("2.15.0".into());
    let manifest_json = serde_json::to_vec(&manifest).unwrap();

    // A distinct filler byte per entry, so each placeholder is found at its own header.
    let placeholders: Vec<String> = names
        .iter()
        .enumerate()
        .map(|(i, name)| ((b'a' + i as u8) as char).to_string().repeat(name.len()))
        .collect();

    // Uncompressed first, so the header blocks can be patched before they are deflated.
    let mut tar_bytes = Vec::new();
    {
        let mut tar = Builder::new(&mut tar_bytes);
        let mut h = Header::new_gnu();
        h.set_path(paths::MANIFEST).unwrap();
        h.set_size(manifest_json.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        tar.append(&h, &manifest_json[..]).unwrap();

        for placeholder in &placeholders {
            let mut h2 = Header::new_gnu();
            h2.set_path(placeholder).unwrap();
            h2.set_size(1);
            h2.set_mode(0o644);
            h2.set_cksum();
            tar.append(&h2, &b"x"[..]).unwrap();
        }
        tar.finish().unwrap();
    }

    for (name, placeholder) in names.iter().zip(&placeholders) {
        let header_start = tar_bytes
            .windows(placeholder.len())
            .position(|w| w == placeholder.as_bytes())
            .expect("placeholder name present in a header");
        // The name field sits at offset 0 of the 512-byte block, so the match is the block start.
        tar_bytes[header_start..header_start + name.len()].copy_from_slice(name.as_bytes());

        // The checksum covers the whole block with its own field read as spaces.
        let block = &mut tar_bytes[header_start..header_start + 512];
        block[148..156].copy_from_slice(b"        ");
        let sum: u32 = block.iter().map(|b| *b as u32).sum();
        let cksum = format!("{:06o}\0 ", sum);
        block[148..156].copy_from_slice(cksum.as_bytes());
    }

    let mut out = Vec::new();
    {
        let mut gz = GzBuilder::new()
            .mtime(0)
            .write(&mut out, flate2::Compression::default());
        std::io::Write::write_all(&mut gz, &tar_bytes).unwrap();
        gz.finish().unwrap();
    }
    out
}

fn raw_tar_gz_with_entry_name(name: &str, _body: &[u8]) -> Vec<u8> {
    raw_tar_gz_with_entry_names(&[name])
}

fn raw_tar_gz_with_two_entry_names(a: &str, b: &str) -> Vec<u8> {
    raw_tar_gz_with_entry_names(&[a, b])
}

/// An absolute entry name must never survive into the stored key.
///
/// `validate_entry_path` trims leading slashes and returns the normalized path, but the reader
/// discarded that return value and stored the raw name. The CLI then does `workspace.join(rel)`,
/// and joining an absolute path throws the workspace prefix away entirely: a `/files/x` entry
/// validates as `files/x` and materializes at the filesystem root. Validating one string and
/// storing another is the whole defect, so the assertion is on the stored key.
#[test]
fn absolute_entry_name_is_stored_normalized() {
    let buf = raw_tar_gz_with_entry_name("/files/x", b"x");
    let read = read_bundle_tar_gz(std::io::Cursor::new(&buf))
        .expect("an absolute name normalizes rather than refusing");
    let keys: Vec<&str> = read.entries.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        keys,
        vec!["files/x"],
        "stored key must be the normalized path"
    );

    // The property that actually matters: the join stays under the workspace.
    for (rel, _) in &read.entries {
        let joined = std::path::Path::new("/tmp/assay-ws").join(rel);
        assert!(
            joined.starts_with("/tmp/assay-ws"),
            "entry {rel:?} escapes the workspace as {}",
            joined.display()
        );
    }
}

/// Two spellings of one path must collide.
///
/// Duplicate detection keyed on the raw name, so `files/x` and `/files/x` were two entries that a
/// consumer resolves to the same file. Which bytes win then depends on map iteration order rather
/// than on anything the archive declared. Keying on the canonical form is what makes the pair a
/// detectable duplicate at all.
#[test]
fn two_spellings_of_one_path_are_a_duplicate() {
    let buf = raw_tar_gz_with_two_entry_names("files/x", "/files/x");
    let err = read_bundle_tar_gz(std::io::Cursor::new(&buf))
        .expect_err("two spellings of one path must be refused");
    assert_eq!(
        err.downcast_ref::<ReplayContractError>(),
        Some(&ReplayContractError::DuplicatePath),
        "{err}"
    );
}

/// A second manifest is refused when it is met, before it is read and before it replaces the
/// first.
///
/// The reader overwrote `manifest_data` on every `manifest.json` it met, so the last one won
/// silently while non-manifest duplicates were refused. An archive could then present one
/// manifest to a reader that stops at the head of the stream and another to this one.
#[test]
fn a_second_manifest_is_refused() {
    let manifest = ReplayManifest::minimal("2.15.0".into());
    let manifest_json = serde_json::to_vec(&manifest).unwrap();
    let mut buf = Vec::new();
    let gz = GzBuilder::new()
        .mtime(0)
        .write(&mut buf, flate2::Compression::default());
    let mut tar = Builder::new(gz);
    for _ in 0..2 {
        let mut h = Header::new_gnu();
        h.set_path(paths::MANIFEST).unwrap();
        h.set_size(manifest_json.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        tar.append(&h, &manifest_json[..]).unwrap();
    }
    let gz = tar.into_inner().unwrap();
    gz.finish().unwrap();

    let err = read_bundle_tar_gz(std::io::Cursor::new(&buf))
        .expect_err("a second manifest must be refused");
    assert_eq!(
        err.downcast_ref::<ReplayContractError>(),
        Some(&ReplayContractError::DuplicateManifest),
        "{err}"
    );
}

/// A duplicate is a malformed bundle, not a resource refusal. If it were classified as an ingest
/// ceiling the CLI would report `E_REPLAY_LIMIT_EXCEEDED` and tell an operator to raise a budget
/// against an archive that no budget can fix.
#[test]
fn a_duplicate_is_not_classified_as_an_ingest_ceiling() {
    let buf = raw_tar_gz_with_two_entry_names("files/x", "/files/x");
    let err = read_bundle_tar_gz(std::io::Cursor::new(&buf)).unwrap_err();
    assert!(
        err.downcast_ref::<ReplayIngestError>().is_none(),
        "a structural violation must not be typed as a ceiling: {err}"
    );
}

/// Duplicate diagnostics carry nothing the archive chose.
#[test]
fn duplicate_refusals_are_value_free() {
    for (a, b) in [
        ("files/x", "/files/x"),
        ("files/secret-name", "files/secret-name"),
    ] {
        let buf = raw_tar_gz_with_two_entry_names(a, b);
        let err = read_bundle_tar_gz(std::io::Cursor::new(&buf)).unwrap_err();
        let rendered = err.to_string();
        for archive_chosen in ["files/", "secret-name", "/files/x", "x"] {
            assert!(
                !rendered.contains(archive_chosen),
                "archive-chosen text `{archive_chosen}` reached the diagnostic: {rendered}"
            );
        }
    }
}

/// What provenance survives a refusal, measured on the real entrypoint.
///
/// The digest can only exist once the source has been read, so the two refusal classes are not
/// symmetric: a source overflow has nothing to attest, while every later refusal happens on bytes
/// we already hold. A test that models both states as "digest survives" describes a system that
/// does not exist.
mod refusal_provenance {
    use super::*;
    use crate::replay::bundle::ReplayLimits;
    use crate::replay::verify::read_verify_bounded;
    use std::io::Cursor;

    fn valid_bundle() -> Vec<u8> {
        let manifest = ReplayManifest::minimal("2.15.0".into());
        let entries = vec![BundleEntry {
            path: "files/trace.jsonl".into(),
            data: b"[]".to_vec(),
        }];
        let mut buf = Vec::new();
        write_bundle_tar_gz(&mut buf, &manifest, &entries).unwrap();
        buf
    }

    /// A source overflow yields no digest, because the bytes were never fully read. The CLI turns
    /// this into `sha256:unknown`, which is the honest answer rather than a defect.
    #[test]
    fn a_source_overflow_has_no_digest_to_report() {
        let bytes = valid_bundle();
        let limits = ReplayLimits {
            max_source_bytes: (bytes.len() - 1) as u64,
            ..ReplayLimits::default()
        };
        let failure = read_verify_bounded(Cursor::new(&bytes), limits)
            .expect_err("a source ceiling below the input must refuse");
        assert!(
            failure.source_digest.is_none(),
            "nothing was fully read, so there is nothing to attest"
        );
        assert!(
            failure.ingest_refusal().is_some(),
            "it is still a typed ingest refusal"
        );
    }

    /// Every refusal after the snapshot names the exact bytes it happened on. This is the property
    /// the one-snapshot design exists for: the digest, the parse and the verdict describe the same
    /// input, so a refusal is reproducible by whoever receives the summary.
    #[test]
    fn a_post_snapshot_refusal_reports_the_exact_digest() {
        let bytes = valid_bundle();
        let expected = format!("sha256:{}", hex::encode(<Sha256 as Digest>::digest(&bytes)));
        // The source fits; a later ceiling is what refuses.
        let limits = ReplayLimits {
            max_path_len: 4,
            ..ReplayLimits::default()
        };
        let failure = read_verify_bounded(Cursor::new(&bytes), limits)
            .expect_err("a path ceiling must refuse this bundle");
        assert_eq!(
            failure.source_digest.as_deref(),
            Some(expected.as_str()),
            "the digest must be of the exact bytes that failed"
        );
    }

    /// The acceptance twin: the same fixture verifies cleanly under default ceilings, so neither
    /// refusal above is an artefact of a bundle that was broken to begin with.
    #[test]
    fn the_same_bundle_verifies_under_default_limits() {
        let bytes = valid_bundle();
        read_verify_bounded(Cursor::new(&bytes), ReplayLimits::default())
            .expect("the fixture is a valid bundle");
    }
}
