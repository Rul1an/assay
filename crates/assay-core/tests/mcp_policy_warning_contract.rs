//! The 6.x reader refuses unknown keys before warning about legacy policy syntax.
//! Each child has a fresh process-wide deprecation warning.
use assay_core::mcp::policy::McpPolicy;
use std::process::Command;

#[test]
#[ignore] // Invoked by warning_ordering_and_single_fire_contract in a fresh process.
fn read_policy_fixture() {
    let Ok(case) = std::env::var("ASSAY_MCP_POLICY_WARNING_CHILD") else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let fixture = dir.path().join("policy.yaml");
    let mut yaml = String::from("version: \"1.0\"\ntools:\n  allow: [read_file]\n");
    match case.as_str() {
        "unknown" => {
            yaml.push_str("  unknown_field_xyz: true\n");
            std::fs::write(&fixture, yaml).unwrap();
            let error = McpPolicy::from_file(&fixture).unwrap_err();
            assert!(
                error.to_string().contains("tools.unknown_field_xyz"),
                "{error:#}"
            );
        }
        "valid" => {
            std::fs::write(&fixture, yaml).unwrap();
            McpPolicy::from_file(&fixture).unwrap();
            McpPolicy::from_file(&fixture).unwrap();
        }
        _ => panic!("unexpected fixture {case}"),
    }
}

#[test]
fn warning_ordering_and_single_fire_contract() {
    for (case, expected_count) in [("unknown", 0), ("valid", 1)] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "read_policy_fixture", "--nocapture"])
            .env("ASSAY_MCP_POLICY_WARNING_CHILD", case)
            .env_remove("ASSAY_STRICT_DEPRECATIONS")
            .output()
            .unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(output.status.success(), "{case}: {stderr}");
        assert_eq!(
            stderr.matches("DEPRECATED").count(),
            expected_count,
            "{case}: {stderr}"
        );
        if case == "valid" {
            assert!(stderr.contains("assay policy migrate"), "{stderr}");
        }
    }
}
