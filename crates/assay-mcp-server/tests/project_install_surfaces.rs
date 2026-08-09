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
    read_bounded_install_path(&path)
}

fn read_bounded_install_path(path: &Path) -> String {
    let metadata = std::fs::symlink_metadata(path)
        .unwrap_or_else(|error| panic!("stat checked project file {}: {error}", path.display()));
    assert!(
        metadata.file_type().is_file() && !metadata.file_type().is_symlink(),
        "checked project path must be a regular non-symlink file: {}",
        path.display()
    );
    let bytes = metadata.len();
    assert!(
        bytes <= MAX_PROJECT_FILE_BYTES,
        "checked project file exceeds {MAX_PROJECT_FILE_BYTES} bytes: {} ({bytes} bytes)",
        path.display()
    );
    let source = std::fs::File::open(path)
        .unwrap_or_else(|error| panic!("open checked project file {}: {error}", path.display()));
    let mut bounded = source.take(MAX_PROJECT_FILE_BYTES + 1);
    let mut contents = Vec::with_capacity(bytes as usize);
    bounded
        .read_to_end(&mut contents)
        .unwrap_or_else(|error| panic!("read checked project file {}: {error}", path.display()));
    assert!(
        contents.len() as u64 <= MAX_PROJECT_FILE_BYTES,
        "checked project file grew beyond {MAX_PROJECT_FILE_BYTES} bytes while reading: {}",
        path.display()
    );
    String::from_utf8(contents).unwrap_or_else(|error| {
        panic!(
            "checked project file is not UTF-8 {}: {error}",
            path.display()
        )
    })
}

#[test]
fn install_file_reader_rejects_oversized_regular_file() {
    let directory = tempfile::tempdir().expect("temporary install-file probe");
    let path = directory.path().join("oversized.json");
    let file = std::fs::File::create(&path).expect("create sparse oversized probe");
    file.set_len(MAX_PROJECT_FILE_BYTES + 1)
        .expect("size oversized probe");
    let result = std::panic::catch_unwind(|| read_bounded_install_path(&path));
    assert!(result.is_err(), "oversized install file was accepted");
}

#[cfg(unix)]
#[test]
fn install_file_reader_rejects_symlink_to_regular_file() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().expect("temporary install-file probe");
    let target = directory.path().join("manifest.json");
    let link = directory.path().join("manifest-link.json");
    std::fs::write(&target, "{}\n").expect("write symlink target");
    symlink(&target, &link).expect("create install-file symlink probe");
    let result = std::panic::catch_unwind(|| read_bounded_install_path(&link));
    assert!(result.is_err(), "symlinked install file was accepted");
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
    std::fs::write(
        cwd.join("install-surface-policy.yaml"),
        "blocklist:\n  - install_surface_probe\n",
    )
    .expect("write consumer-project policy probe");
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
    conn.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "assay_policy_decide",
            "arguments": {
                "tool": "install_surface_probe",
                "policy": "install-surface-policy.yaml"
            }
        }
    }));
    let policy_response = conn.read_response_for_id(3);
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

    let policy_text = policy_response["result"]["content"][0]["text"]
        .as_str()
        .expect("policy decision text");
    assert_eq!(
        policy_response["result"]["isError"].as_bool(),
        Some(true),
        "{context} did not preserve the server contract that a denied tool result sets isError"
    );
    let policy_result: Value = serde_json::from_str(policy_text).expect("policy decision JSON");
    assert_eq!(
        policy_result["allowed"], false,
        "{context} did not resolve policy from its consuming-project cwd"
    );
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
    let project = tempfile::tempdir().expect("temporary project-manifest consumer");
    assert_release_server_surface(project.path(), &args, "project manifest");
}

#[test]
fn plugin_manifest_drives_the_release_server_surface() {
    let plugin = manifest_entry(InstallFile::PluginManifest);
    let project_manifest = manifest_entry(InstallFile::ClaudeManifest);
    assert_eq!(
        plugin, project_manifest,
        "project and plugin manifests must intentionally share cwd-relative policy-root semantics"
    );
    assert_eq!(plugin["command"], "assay-mcp-server");
    assert_eq!(plugin["args"], serde_json::json!(["--policy-root", "."]));

    let project = tempfile::tempdir().expect("temporary Claude project");
    let args: Vec<String> = plugin["args"]
        .as_array()
        .expect("plugin manifest args array")
        .iter()
        .map(|arg| {
            arg.as_str()
                .expect("plugin manifest string arg")
                .to_string()
        })
        .collect();
    assert_release_server_surface(project.path(), &args, "plugin manifest");
}
