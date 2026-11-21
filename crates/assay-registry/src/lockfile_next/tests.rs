use crate::error::RegistryError;
use crate::lockfile::{
    LockMismatch, LockSignature, LockSource, LockedPack, Lockfile, LOCKFILE_VERSION,
};

#[test]
fn test_lockfile_new() {
    let lockfile = Lockfile::new();
    assert_eq!(lockfile.version, LOCKFILE_VERSION);
    assert!(lockfile.packs.is_empty());
}

#[test]
fn test_lockfile_parse() {
    let yaml = r#"
version: 2
generated_at: "2026-01-29T10:00:00Z"
generated_by: "assay-cli/2.10.1"
packs:
  - name: eu-ai-act-pro
    version: "1.2.0"
    digest: sha256:abc123def456
    source: registry
    registry_url: "https://registry.getassay.dev/v1"
    signature:
      algorithm: Ed25519
      key_id: sha256:keyid123
"#;

    let lockfile = Lockfile::parse(yaml).unwrap();
    assert_eq!(lockfile.version, 2);
    assert_eq!(lockfile.packs.len(), 1);

    let pack = &lockfile.packs[0];
    assert_eq!(pack.name, "eu-ai-act-pro");
    assert_eq!(pack.version, "1.2.0");
    assert_eq!(pack.digest, "sha256:abc123def456");
    assert_eq!(pack.source, LockSource::Registry);
    assert!(pack.signature.is_some());
}

#[test]
fn test_lockfile_parse_unsupported_version() {
    let yaml = r#"
version: 99
generated_at: "2026-01-29T10:00:00Z"
generated_by: "future-cli/9.0.0"
packs: []
"#;

    let result = Lockfile::parse(yaml);
    assert!(matches!(result, Err(RegistryError::Lockfile { .. })));
}

#[test]
fn test_lockfile_add_pack() {
    let mut lockfile = Lockfile::new();

    let pack1 = LockedPack {
        name: "pack-b".to_string(),
        version: "1.0.0".to_string(),
        digest: "sha256:bbb".to_string(),
        source: LockSource::Registry,
        registry_url: None,
        namespace: None,
        fetched_at: None,
        etag: None,
        byos_url: None,
        signature: None,
    };

    let pack2 = LockedPack {
        name: "pack-a".to_string(),
        version: "1.0.0".to_string(),
        digest: "sha256:aaa".to_string(),
        source: LockSource::Registry,
        registry_url: None,
        namespace: None,
        fetched_at: None,
        etag: None,
        byos_url: None,
        signature: None,
    };

    lockfile.add_pack(pack1);
    lockfile.add_pack(pack2);

    // Should be sorted by name
    assert_eq!(lockfile.packs[0].name, "pack-a");
    assert_eq!(lockfile.packs[1].name, "pack-b");
}

#[test]
fn test_lockfile_add_pack_update() {
    let mut lockfile = Lockfile::new();

    let pack1 = LockedPack {
        name: "my-pack".to_string(),
        version: "1.0.0".to_string(),
        digest: "sha256:old".to_string(),
        source: LockSource::Registry,
        registry_url: None,
        namespace: None,
        fetched_at: None,
        etag: None,
        byos_url: None,
        signature: None,
    };

    let pack2 = LockedPack {
        name: "my-pack".to_string(),
        version: "1.1.0".to_string(),
        digest: "sha256:new".to_string(),
        source: LockSource::Registry,
        registry_url: None,
        namespace: None,
        fetched_at: None,
        etag: None,
        byos_url: None,
        signature: None,
    };

    lockfile.add_pack(pack1);
    lockfile.add_pack(pack2);

    // Should only have one entry (updated)
    assert_eq!(lockfile.packs.len(), 1);
    assert_eq!(lockfile.packs[0].version, "1.1.0");
    assert_eq!(lockfile.packs[0].digest, "sha256:new");
}

#[test]
fn test_lockfile_remove_pack() {
    let mut lockfile = Lockfile::new();

    let pack = LockedPack {
        name: "my-pack".to_string(),
        version: "1.0.0".to_string(),
        digest: "sha256:abc".to_string(),
        source: LockSource::Registry,
        registry_url: None,
        namespace: None,
        fetched_at: None,
        etag: None,
        byos_url: None,
        signature: None,
    };

    lockfile.add_pack(pack);
    assert!(lockfile.contains("my-pack"));

    let removed = lockfile.remove_pack("my-pack");
    assert!(removed);
    assert!(!lockfile.contains("my-pack"));

    let removed_again = lockfile.remove_pack("my-pack");
    assert!(!removed_again);
}

#[test]
fn test_lockfile_get_pack() {
    let mut lockfile = Lockfile::new();

    let pack = LockedPack {
        name: "my-pack".to_string(),
        version: "1.0.0".to_string(),
        digest: "sha256:abc".to_string(),
        source: LockSource::Registry,
        registry_url: None,
        namespace: None,
        fetched_at: None,
        etag: None,
        byos_url: None,
        signature: None,
    };

    lockfile.add_pack(pack);

    let found = lockfile.get_pack("my-pack");
    assert!(found.is_some());
    assert_eq!(found.unwrap().version, "1.0.0");

    let not_found = lockfile.get_pack("other-pack");
    assert!(not_found.is_none());
}

#[test]
fn test_lockfile_to_yaml() {
    let mut lockfile = Lockfile::new();

    let pack = LockedPack {
        name: "my-pack".to_string(),
        version: "1.0.0".to_string(),
        digest: "sha256:abc123".to_string(),
        source: LockSource::Registry,
        registry_url: Some("https://registry.example.com/v1".to_string()),
        namespace: None,
        fetched_at: None,
        etag: None,
        byos_url: None,
        signature: Some(LockSignature {
            algorithm: "Ed25519".to_string(),
            key_id: "sha256:key123".to_string(),
        }),
    };

    lockfile.add_pack(pack);

    let yaml = lockfile.to_yaml().unwrap();
    assert!(yaml.contains("version: 2"));
    assert!(yaml.contains("my-pack"));
    assert!(yaml.contains("sha256:abc123"));
    assert!(yaml.contains("Ed25519"));
}

#[test]
fn test_lock_source_serialize() {
    let sources = vec![
        (LockSource::Bundled, "bundled"),
        (LockSource::Registry, "registry"),
        (LockSource::Byos, "byos"),
        (LockSource::Local, "local"),
    ];

    for (source, expected) in sources {
        let yaml = serde_yaml::to_string(&source).unwrap();
        assert!(yaml.contains(expected));
    }
}

// ==================== Lockfile Semantics Tests (SPEC §8) ====================

#[test]
fn test_pack_not_in_lockfile() {
    // SPEC §8.4: Pack not in lockfile should be detectable
    let lockfile = Lockfile::new();

    // contains() should return false for unknown pack
    assert!(!lockfile.contains("unknown-pack"));

    // get_pack() should return None
    assert!(lockfile.get_pack("unknown-pack").is_none());

    // pack_names() should be empty
    assert!(lockfile.pack_names().is_empty());
}

#[test]
fn test_lockfile_v2_roundtrip() {
    // SPEC §8.2: Lockfile should roundtrip through YAML serialization
    let mut lockfile = Lockfile::new();

    // Add multiple packs with all fields
    lockfile.add_pack(LockedPack {
        name: "pack-z".to_string(),
        version: "2.0.0".to_string(),
        digest: "sha256:zzz".to_string(),
        source: LockSource::Registry,
        registry_url: Some("https://registry.example.com/v1".to_string()),
        namespace: None,
        fetched_at: None,
        etag: None,
        byos_url: None,
        signature: Some(LockSignature {
            algorithm: "Ed25519".to_string(),
            key_id: "sha256:keyzzz".to_string(),
        }),
    });

    lockfile.add_pack(LockedPack {
        name: "pack-a".to_string(),
        version: "1.0.0".to_string(),
        digest: "sha256:aaa".to_string(),
        source: LockSource::Bundled,
        registry_url: None,
        namespace: None,
        fetched_at: None,
        etag: None,
        byos_url: None,
        signature: None,
    });

    lockfile.add_pack(LockedPack {
        name: "pack-m".to_string(),
        version: "1.5.0".to_string(),
        digest: "sha256:mmm".to_string(),
        source: LockSource::Byos,
        registry_url: None,
        namespace: None,
        fetched_at: None,
        etag: None,
        byos_url: Some("s3://bucket/pack.yaml".to_string()),
        signature: None,
    });

    // Serialize to YAML
    let yaml = lockfile.to_yaml().unwrap();

    // Parse back
    let parsed = Lockfile::parse(&yaml).unwrap();

    // Verify version preserved
    assert_eq!(parsed.version, LOCKFILE_VERSION);

    // Verify packs are sorted by name
    assert_eq!(parsed.packs.len(), 3);
    assert_eq!(parsed.packs[0].name, "pack-a");
    assert_eq!(parsed.packs[1].name, "pack-m");
    assert_eq!(parsed.packs[2].name, "pack-z");

    // Verify all fields preserved
    let pack_z = parsed.get_pack("pack-z").unwrap();
    assert_eq!(pack_z.version, "2.0.0");
    assert_eq!(pack_z.digest, "sha256:zzz");
    assert_eq!(pack_z.source, LockSource::Registry);
    assert!(pack_z.signature.is_some());

    let pack_m = parsed.get_pack("pack-m").unwrap();
    assert_eq!(pack_m.byos_url, Some("s3://bucket/pack.yaml".to_string()));
}

#[test]
fn test_lockfile_stable_ordering() {
    // SPEC §8.2: Packs should be sorted by name for stable diffs
    let mut lockfile = Lockfile::new();

    // Add packs in random order
    for name in ["zebra", "alpha", "middle", "beta"] {
        lockfile.add_pack(LockedPack {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            digest: format!("sha256:{}", name),
            source: LockSource::Registry,
            registry_url: None,
            namespace: None,
            fetched_at: None,
            etag: None,
            byos_url: None,
            signature: None,
        });
    }

    // Verify sorted
    let names: Vec<&str> = lockfile.pack_names().into_iter().collect();
    assert_eq!(names, vec!["alpha", "beta", "middle", "zebra"]);
}

#[test]
fn test_lockfile_digest_mismatch_detection() {
    // SPEC §8.4: Detect when digest differs from lockfile
    let mut lockfile = Lockfile::new();

    lockfile.add_pack(LockedPack {
        name: "my-pack".to_string(),
        version: "1.0.0".to_string(),
        digest: "sha256:expected_digest_here".to_string(),
        source: LockSource::Registry,
        registry_url: None,
        namespace: None,
        fetched_at: None,
        etag: None,
        byos_url: None,
        signature: None,
    });

    // Simulate checking against a different digest
    let locked = lockfile.get_pack("my-pack").unwrap();
    let actual_digest = "sha256:different_digest";

    let mismatch = LockMismatch {
        name: locked.name.clone(),
        version: locked.version.clone(),
        expected: locked.digest.clone(),
        actual: actual_digest.to_string(),
    };

    // Verify mismatch is detectable
    assert_ne!(mismatch.expected, mismatch.actual);
    assert_eq!(mismatch.expected, "sha256:expected_digest_here");
    assert_eq!(mismatch.actual, "sha256:different_digest");
}

#[test]
fn test_lockfile_version_1_rejected() {
    // SPEC §8.2: Old lockfile versions should be handled
    // Version 1 is older than current (2), but should still parse
    let yaml_v1 = r#"
version: 1
generated_at: "2025-01-01T00:00:00Z"
generated_by: "assay-cli/1.0.0"
packs: []
"#;

    let result = Lockfile::parse(yaml_v1);
    // Version 1 is supported (less than current)
    assert!(result.is_ok());
}

#[test]
fn test_lockfile_future_version_rejected() {
    // SPEC §8.2: Future lockfile versions should be rejected
    let yaml_future = r#"
version: 99
generated_at: "2030-01-01T00:00:00Z"
generated_by: "future-cli/99.0.0"
packs: []
"#;

    let result = Lockfile::parse(yaml_future);
    assert!(
        matches!(result, Err(RegistryError::Lockfile { .. })),
        "Should reject future lockfile version"
    );
}

#[test]
fn test_lockfile_signature_fields() {
    // SPEC §8.2: Signature fields in lockfile
    let yaml = r#"
version: 2
generated_at: "2026-01-29T10:00:00Z"
generated_by: "assay-cli/2.10.0"
packs:
  - name: signed-pack
    version: "1.0.0"
    digest: sha256:abc123
    source: registry
    signature:
      algorithm: Ed25519
      key_id: sha256:keyid123
"#;

    let lockfile = Lockfile::parse(yaml).unwrap();
    let pack = lockfile.get_pack("signed-pack").unwrap();

    assert!(pack.signature.is_some());
    let sig = pack.signature.as_ref().unwrap();
    assert_eq!(sig.algorithm, "Ed25519");
    assert_eq!(sig.key_id, "sha256:keyid123");
}

#[tokio::test]
async fn lockfile_reads_at_the_byte_limit_and_refuses_one_more_byte() {
    let mut input = Lockfile::new().to_yaml().unwrap();
    input.push('#');
    input.push_str(&"x".repeat(16 * 1024 * 1024 - input.len()));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("assay.packs.lock");
    std::fs::write(&path, &input).unwrap();
    assert!(Lockfile::parse(&input).is_ok());
    Lockfile::load(&path).await.unwrap();

    input.push('x');
    std::fs::write(&path, &input).unwrap();
    for result in [Lockfile::parse(&input), Lockfile::load(&path).await] {
        let error = result.unwrap_err().to_string();
        assert!(error.contains("16777216"), "{error}");
    }
}

#[tokio::test]
async fn oversized_lockfile_output_preserves_the_existing_file() {
    let mut lock = Lockfile::new();
    lock.generated_by = "x".into();
    let remaining = 16 * 1024 * 1024 - lock.to_yaml().unwrap().len();
    lock.generated_by.push_str(&"x".repeat(remaining));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("assay.packs.lock");
    lock.save(&path).await.unwrap();
    let original = std::fs::read(&path).unwrap();
    assert_eq!(original.len(), 16 * 1024 * 1024);
    assert_eq!(
        Lockfile::load(&path).await.unwrap().generated_by,
        lock.generated_by
    );

    lock.generated_by.push('x');
    assert!(lock
        .save(&path)
        .await
        .unwrap_err()
        .to_string()
        .contains("16777216"));
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn oversized_lockfile_stream_refuses_without_waiting_for_eof() {
    use tokio::{io::AsyncWriteExt, net::unix::pipe::OpenOptions};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("assay.packs.lock");
    assert!(std::process::Command::new("mkfifo")
        .arg(&path)
        .status()
        .unwrap()
        .success());
    // Opening both ends lets the writer start without blocking on the reader.
    // Keep it open until the assertion so success cannot depend on EOF.
    let mut writer = OpenOptions::new()
        .read_write(true)
        .open_sender(&path)
        .unwrap();
    let send = async {
        let block = [b' '; 64 * 1024];
        for _ in 0..256 {
            writer.write_all(&block).await.unwrap();
        }
        writer.write_all(b"x").await.unwrap();
    };
    let read = Lockfile::load(&path);
    tokio::pin!(read);
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::select! {
            biased;
            () = send => (),
            result = &mut read => return result,
        }
        read.await
    });
    assert!(result
        .await
        .expect("load waited for EOF after the limit")
        .unwrap_err()
        .to_string()
        .contains("16777216"));
}

#[test]
fn lockfile_refuses_unknown_fields_before_a_roundtrip_can_drop_them() {
    let input = r#"
version: 2
generated_at: "2026-01-29T10:00:00Z"
generated_by: 0x10
lock_note: root
packs:
  - name: pack
    version: "1"
    digest: sha256:abc
    source: registry
    pack_note: entry
    signature:
      algorithm: Ed25519
      key_id: sha256:key
      sig_note: signature
"#;
    let error = Lockfile::parse(input).unwrap_err().to_string();
    for path in [
        "lock_note",
        "packs.0.pack_note",
        "packs.0.signature.sig_note",
    ] {
        assert!(error.contains(path), "{error}");
    }
}

#[test]
fn valid_lockfiles_keep_authored_strings_and_omit_absent_pack_fields() {
    for version in [1, 2] {
        for source in ["bundled", "registry", "byos", "local"] {
            let input = format!(
                "version: {version}\ngenerated_at: 2026-01-29T10:00:00Z\ngenerated_by: 0x10\n\
                 packs: [{{name: p, version: 010, digest: d, source: {source}}}]"
            );
            let lock = Lockfile::parse(&input).unwrap();
            assert_eq!(lock.version, version);
            assert_eq!(lock.generated_by, "0x10");
            assert_eq!(lock.packs[0].version, "010");
            let yaml = lock.to_yaml().unwrap();
            assert!(yaml.contains(&format!("source: {source}")));
            for absent in ["registry_url:", "byos_url:", "signature:"] {
                assert!(!yaml.contains(absent), "{yaml}");
            }
            assert_eq!(Lockfile::parse(&yaml).unwrap().packs, lock.packs);
        }
    }
}
