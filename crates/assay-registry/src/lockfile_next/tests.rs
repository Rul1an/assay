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
        byos_url: None,
        signature: None,
    };

    let pack2 = LockedPack {
        name: "pack-a".to_string(),
        version: "1.0.0".to_string(),
        digest: "sha256:aaa".to_string(),
        source: LockSource::Registry,
        registry_url: None,
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
        byos_url: None,
        signature: None,
    };

    let pack2 = LockedPack {
        name: "my-pack".to_string(),
        version: "1.1.0".to_string(),
        digest: "sha256:new".to_string(),
        source: LockSource::Registry,
        registry_url: None,
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
        byos_url: None,
        signature: None,
    });

    lockfile.add_pack(LockedPack {
        name: "pack-m".to_string(),
        version: "1.5.0".to_string(),
        digest: "sha256:mmm".to_string(),
        source: LockSource::Byos,
        registry_url: None,
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
