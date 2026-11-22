use assay_core::mcp::policy::{McpPolicy, PolicyDecision, PolicyState};
use serde_json::json;
use std::path::Path;

fn allows(policy: &McpPolicy, tool: &str, argument: &str) -> bool {
    let args = json!({ "path": argument });
    match policy.evaluate(tool, &args, &mut PolicyState::default(), None) {
        PolicyDecision::Allow => true,
        PolicyDecision::Deny { code, .. } if code == "E_ARG_SCHEMA" => false,
        other => panic!("unexpected decision for {tool} {argument:?}: {other:?}"),
    }
}

fn check_paths(file: &str, tool: &str, root: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(file);
    let policy = McpPolicy::from_file(&path).unwrap();
    for (suffix, expected) in [
        ("/notes/.draft v1.txt", true),
        ("/notes/..archive", true),
        ("/notes/../draft", false),
        ("/..", false),
        ("/./draft", false),
        ("/notes//draft", false),
        ("-other/draft", false),
        ("/notes\\draft", false),
        ("/draft\n", false),
    ] {
        let argument = format!("{root}{suffix}");
        assert_eq!(
            allows(&policy, tool, &argument),
            expected,
            "{file} {tool} {argument:?}"
        );
    }
}

#[test]
fn preset_paths() {
    for preset in ["default", "dev", "hardened"] {
        let file = format!("packs/{preset}-policy.yaml");
        for root in ["/app", "/data"] {
            check_paths(&file, "read_file", root);
            if preset == "hardened" {
                check_paths(&file, "list_directory", root);
                check_paths(&file, "search_files", root);
            }
        }
        if preset != "hardened" {
            check_paths(&file, "read_file", "/workspace");
        }
    }
}

#[test]
fn quickstart_paths() {
    for tool in ["read_file", "list_dir"] {
        check_paths(
            "../../examples/mcp-quickstart/policy.yaml",
            tool,
            "/tmp/assay-demo",
        );
    }
}

#[test]
fn fixed_demo_paths() {
    let file = "../../examples/demo/filesystem-mcp-hardened-fixed.yaml";
    for root in [
        "/workspace",
        "/home/example/projects",
        "/Users/example/Developer",
    ] {
        for tool in ["read_file", "list_directory"] {
            check_paths(file, tool, root);
        }
    }
}

#[test]
fn fixed_demo_directory_roots_and_temporary_reads() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/demo/filesystem-mcp-hardened-fixed.yaml");
    let policy = McpPolicy::from_file(&path).unwrap();
    for (tool, argument, expected) in [
        ("list_directory", "/workspace", true),
        ("list_directory", "/home/../projects", false),
        ("read_file", "/tmp/assay-example/note", true),
        ("read_file", "/tmp/assay-example/../note", false),
        ("read_file", "/tmp/other/note", false),
        ("list_directory", "/tmp/assay-example", false),
    ] {
        assert_eq!(
            allows(&policy, tool, argument),
            expected,
            "{tool} {argument}"
        );
    }
}
