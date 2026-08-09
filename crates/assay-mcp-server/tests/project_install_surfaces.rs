//! Install surfaces must launch the release MCP server they describe.
//!
//! The JSON locations differ by surface, while Codex uses TOML. This test keeps
//! those representations on one invocation and drives the built server so
//! a syntactically correct manifest cannot advertise the wrong binary or tools.

use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

const MAX_PROJECT_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy)]
enum InstallFile {
    ClaudeManifest,
    CursorManifest,
    PluginManifest,
    EditorGuide,
}

impl InstallFile {
    fn relative_path(self) -> &'static Path {
        Path::new(match self {
            Self::ClaudeManifest => ".mcp.json",
            Self::CursorManifest => ".cursor/mcp.json",
            Self::PluginManifest => "packaging/claude-plugin/.mcp.json",
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

fn read_install_file(file: InstallFile) -> String {
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

fn manifest_entry(file: InstallFile) -> Value {
    let manifest: Value =
        serde_json::from_str(&read_install_file(file)).expect("valid MCP manifest JSON");
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

fn assert_release_server_surface(cwd: &Path, args: &[String], context: &str) {
    let child = clean_server_command()
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap_or_else(|error| panic!("spawn assay-mcp-server from {context}: {error}"));

    let mut conn = Conn::attach(child);
    conn.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "install-surface-contract", "version": "1.0"}
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
        "{context} invocation failed with status {status}"
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

#[test]
fn project_surfaces_launch_the_five_production_tools() {
    let claude = manifest_entry(InstallFile::ClaudeManifest);
    let cursor = manifest_entry(InstallFile::CursorManifest);
    assert_eq!(claude, cursor, "Claude and Cursor entries drifted");
    assert_eq!(claude["command"], "assay-mcp-server");
    assert_eq!(claude["args"], serde_json::json!(["--policy-root", "."]));

    let codex_entry = r#"[mcp_servers.assay]
command = "assay-mcp-server"
args = ["--policy-root", "."]"#;
    let guide = read_install_file(InstallFile::EditorGuide).replace("\r\n", "\n");
    assert!(
        guide.contains("cargo install --path crates/assay-mcp-server --locked"),
        "editor guide install must use the reviewed checkout"
    );
    assert!(
        guide.contains(codex_entry),
        "Codex guide does not carry the manifest invocation"
    );

    let args: Vec<String> = claude["args"]
        .as_array()
        .expect("manifest args array")
        .iter()
        .map(|arg| arg.as_str().expect("manifest string arg").to_string())
        .collect();
    assert_release_server_surface(&workspace_root(), &args, "project manifest");
}

#[test]
fn plugin_manifest_drives_the_release_server_surface() {
    let plugin = manifest_entry(InstallFile::PluginManifest);
    assert_eq!(plugin["command"], "assay-mcp-server");
    assert_eq!(
        plugin["args"],
        serde_json::json!(["--policy-root", "${CLAUDE_PROJECT_DIR}"])
    );

    let project = tempfile::tempdir().expect("temporary Claude project");
    let project_root = project
        .path()
        .to_str()
        .expect("temporary Claude project path must be UTF-8");
    let args: Vec<String> = plugin["args"]
        .as_array()
        .expect("plugin manifest args array")
        .iter()
        .map(
            |arg| match arg.as_str().expect("plugin manifest string arg") {
                "${CLAUDE_PROJECT_DIR}" => project_root.to_string(),
                value => {
                    assert!(
                        !value.contains("${"),
                        "plugin manifest contains unsupported placeholder: {value}"
                    );
                    value.to_string()
                }
            },
        )
        .collect();
    assert_release_server_surface(project.path(), &args, "plugin manifest");
}
