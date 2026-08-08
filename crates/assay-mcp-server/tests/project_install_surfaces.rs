//! Project install surfaces must launch the release MCP server they describe.
//!
//! The JSON locations differ by client, while Codex uses TOML. This test keeps
//! those three representations on one invocation and drives the built server so
//! a syntactically correct manifest cannot advertise the wrong binary or tools.

use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

const MAX_PROJECT_FILE_BYTES: u64 = 1024 * 1024;

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
    let mut source = std::fs::File::open(&path)
        .unwrap_or_else(|error| panic!("open checked project file {}: {error}", path.display()));
    let bytes = source
        .metadata()
        .unwrap_or_else(|error| panic!("stat checked project file {}: {error}", path.display()))
        .len();
    assert!(
        bytes <= MAX_PROJECT_FILE_BYTES,
        "checked project file exceeds {MAX_PROJECT_FILE_BYTES} bytes: {} ({bytes} bytes)",
        path.display()
    );
    let mut contents = String::with_capacity(bytes as usize);
    source
        .read_to_string(&mut contents)
        .unwrap_or_else(|error| panic!("read checked project file {}: {error}", path.display()));
    contents
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
    let guide = read_project_file(ProjectFile::EditorGuide).replace("\r\n", "\n");
    assert!(
        guide.contains("cargo install --path crates/assay-mcp-server --locked"),
        "editor guide install must use the reviewed checkout"
    );
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
    let child = clean_server_command()
        .args(args)
        .current_dir(workspace_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn assay-mcp-server from project manifest");

    let mut conn = Conn::attach(child);
    conn.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "project-install-contract", "version": "1.0"}
        }
    }));
    let initialize = conn.read_response_for_id(0);
    assert_eq!(initialize["result"]["protocolVersion"], "2024-11-05");
    conn.send(serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    }));
    conn.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list"
    }));
    conn.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list"
    }));
    let response = conn.read_response_for_id(1);
    let status = conn.shutdown();
    assert!(
        status.success(),
        "manifest invocation failed with status {status}"
    );
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
