use super::classifier::{classify_path_pattern, fnv1a_hash, PathClass};
use super::{compile, FilePolicy, NetworkPolicy, Policy, ProcessPolicy};

#[test]
fn test_classify_exact() {
    assert!(matches!(
        classify_path_pattern("/etc/shadow"),
        PathClass::Exact
    ));
    assert!(matches!(
        classify_path_pattern("/home/user/.ssh/id_rsa"),
        PathClass::Exact
    ));
}

#[test]
fn test_classify_prefix() {
    match classify_path_pattern("/home/user/*") {
        PathClass::Prefix(p) => assert_eq!(p, "/home/user/"),
        _ => panic!("Expected Prefix"),
    }
}

#[test]
fn test_classify_glob() {
    assert!(matches!(
        classify_path_pattern("**/.ssh/*"),
        PathClass::Glob
    ));
    assert!(matches!(
        classify_path_pattern("/etc/*.conf"),
        PathClass::Glob
    ));
    assert!(matches!(
        classify_path_pattern("/tmp/file?.txt"),
        PathClass::Glob
    ));
}

#[test]
fn test_compile_splits_tiers() {
    let policy = Policy {
        files: FilePolicy {
            deny: vec![
                "/etc/shadow".to_string(),
                "/home/user/*".to_string(),
                "**/.ssh/id_*".to_string(),
            ],
            allow: vec![],
        },
        network: NetworkPolicy {
            allow_cidrs: vec!["10.0.0.0/8".to_string()],
            deny_ports: vec![22, 23],
            ..Default::default()
        },
        processes: ProcessPolicy::default(),
    };

    let compiled = compile(&policy);

    // File rules.
    assert_eq!(compiled.tier1.file_deny_exact.len(), 1);
    assert_eq!(compiled.tier1.file_deny_prefix.len(), 1);
    assert_eq!(compiled.tier2.file_deny_globs.len(), 1);

    // Network rules.
    assert_eq!(compiled.tier1.network_allow_cidrs.len(), 1);
    assert_eq!(compiled.tier1.network_deny_ports.len(), 2);

    // Stats.
    assert_eq!(compiled.stats.tier1_rules, 5);
    assert_eq!(compiled.stats.tier2_rules, 1);
}

#[test]
fn test_hash_consistency() {
    // Ensure hash is consistent (for kernel matching).
    let hash1 = fnv1a_hash(b"/etc/shadow");
    let hash2 = fnv1a_hash(b"/etc/shadow");
    assert_eq!(hash1, hash2);

    let hash3 = fnv1a_hash(b"/etc/passwd");
    assert_ne!(hash1, hash3);
}
