//! Project install surfaces must launch the release MCP server they describe.
//!
//! The JSON locations differ by client, while Codex uses TOML. This test keeps
//! those three representations on one invocation and drives the built server so
//! a syntactically correct manifest cannot advertise the wrong binary or tools.

use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Copy)]
enum ProjectFile {
    ClaudeManifest,
    CursorManifest,
    EditorGuide,
}

impl ProjectFile {
    fn relative_path(self) -> &'static Path {
        Path::new(match self {
            Self::ClaudeManifest => ".mcp.json",
            Self::CursorManifest => ".cursor/mcp.json",
            Self::EditorGuide => "docs/guides/editor-mcp-recipe.md",
        })
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn read_project_file(file: ProjectFile) -> String {
    let path = workspace_root().join(file.relative_path());
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read checked project file {}: {error}", path.display()))
}

fn manifest_entry(file: ProjectFile) -> Value {
    let manifest: Value =
        serde_json::from_str(&read_project_file(file)).expect("valid project MCP manifest JSON");
    manifest["mcpServers"]["assay"].clone()
}

fn clean_server_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"));
    for (name, _) in std::env::vars_os() {
        if name
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("ASSAY_AUTH_")
        {
            command.env_remove(name);
        }
    }
    command
}

#[test]
fn project_surfaces_launch_the_five_production_tools() {
    let claude = manifest_entry(ProjectFile::ClaudeManifest);
    let cursor = manifest_entry(ProjectFile::CursorManifest);
    assert_eq!(claude, cursor, "Claude and Cursor entries drifted");
    assert_eq!(claude["command"], "assay-mcp-server");
    assert_eq!(claude["args"], serde_json::json!(["--policy-root", "."]));

    let codex_entry = r#"[mcp_servers.assay]
command = "assay-mcp-server"
args = ["--policy-root", "."]"#;
    let guide = read_project_file(ProjectFile::EditorGuide);
    assert!(
        guide.contains(codex_entry),
        "Codex guide does not carry the manifest invocation"
    );

    let args: Vec<&str> = claude["args"]
        .as_array()
        .expect("manifest args array")
        .iter()
        .map(|arg| arg.as_str().expect("manifest string arg"))
        .collect();
    let mut child = clean_server_command()
        .args(args)
        .current_dir(workspace_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn assay-mcp-server from project manifest");

    {
        let mut stdin = child.stdin.take().expect("server stdin");
        writeln!(
            stdin,
            "{}",
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/list"
            })
        )
        .expect("write tools/list request");
    }

    let output = child.wait_with_output().expect("wait for server");
    assert!(
        output.status.success(),
        "manifest invocation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "parse tools/list response: {error}; stdout={}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    let mut actual: Vec<&str> = response["result"]["tools"]
        .as_array()
        .expect("tools/list result array")
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name"))
        .collect();
    #[cfg(feature = "test-outbound")]
    {
        assert!(
            actual.contains(&"assay_test_outbound"),
            "test-outbound feature did not expose its test-only tool"
        );
        actual.retain(|name| *name != "assay_test_outbound");
    }
    #[cfg(not(feature = "test-outbound"))]
    assert!(
        !actual.contains(&"assay_test_outbound"),
        "default build advertised the test-only outbound tool"
    );
    actual.sort_unstable();

    let expected = [
        "assay_check_args",
        "assay_check_coverage",
        "assay_check_sequence",
        "assay_explain_trace",
        "assay_policy_decide",
    ];
    assert_eq!(actual, expected, "release tool surface changed");
}
