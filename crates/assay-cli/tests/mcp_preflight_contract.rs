//! `assay mcp preflight` publishes one `assay.mcp_preflight.v0` document (#2195).

#[path = "../../../tests/support/bounded_process.rs"]
#[allow(dead_code)]
mod bounded_process;

use bounded_process::{run_bounded, GOLDEN_PATH_LIMITS};
use serde_json::Value;
use std::process::{Command, Output};

#[cfg(windows)]
use std::ffi::OsString;
#[cfg(windows)]
use std::fs;
#[cfg(windows)]
use std::path::{Path, PathBuf};

const PREFLIGHT_SCHEMA: &str = "assay.mcp_preflight.v0";

fn preflight(args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_assay"));
    command.env("NO_COLOR", "1").env("PATH", "");
    command.args(["mcp", "preflight"]).args(args);
    run_bounded(command, b"", GOLDEN_PATH_LIMITS, "assay mcp preflight").expect("preflight ran")
}

fn exit_code(output: &Output) -> i32 {
    output
        .status
        .code()
        .expect("preflight exited by code rather than by signal")
}

#[test]
fn policy_root_dot_format_json_is_one_preflight_document() {
    let output = preflight(&["--policy-root", ".", "--format", "json"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        exit_code(&output),
        2,
        "empty PATH must not be ready; stderr={stderr}"
    );
    let document: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!("stdout is not one JSON document: {error}\nstdout={stdout}\nstderr={stderr}")
    });
    assert_eq!(document["schema"], PREFLIGHT_SCHEMA);
    assert_eq!(document["phase"], "missing");
    assert_eq!(document["expected_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(document["policy_root"], ".");
    assert!(
        document.get("actual_version").is_none(),
        "missing must omit actual_version: {document}"
    );
    let mut keys: Vec<_> = document
        .as_object()
        .expect("object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "expected_version",
            "message",
            "next_step",
            "phase",
            "policy_root",
            "schema",
        ]
    );
    assert_eq!(
        document["message"],
        "assay-mcp-server was not found on PATH"
    );
    assert_eq!(
        document["next_step"],
        format!(
            "Install assay-mcp-server on PATH (cargo install assay-mcp-server --version {} --locked), then re-run assay mcp preflight.",
            env!("CARGO_PKG_VERSION")
        )
    );
}

#[test]
fn format_terminal_is_not_json() {
    let output = preflight(&["--format", "terminal"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(exit_code(&output), 2);
    assert!(
        stdout.starts_with("missing:"),
        "terminal format must stay human; stdout={stdout}"
    );
    assert!(serde_json::from_slice::<Value>(&output.stdout).is_err());
}

#[cfg(windows)]
fn compile_windows_server(executable: &Path) {
    let source = executable.with_extension("rs");
    let helper = format!(
        r#"use std::env;

fn main() {{
    let mut args = env::args();
    let _program = args.next();
    match args.next().as_deref() {{
        Some("--version") if args.next().is_none() => {{
            println!("assay-mcp-server {}");
        }}
        Some("--policy-root") if args.next().is_some() && args.next().is_none() => {{}}
        _ => std::process::exit(64),
    }}
}}
"#,
        env!("CARGO_PKG_VERSION")
    );
    fs::write(&source, helper).expect("write Windows server helper source");

    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
    let output = Command::new(rustc)
        .arg(&source)
        .arg("-o")
        .arg(executable)
        .output()
        .expect("run rustc for Windows server helper");
    assert!(
        output.status.success(),
        "Windows server helper failed to compile: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(windows)]
fn native_windows_preflight(
    assay: &Path,
    path: &Path,
    pathext: &str,
    policy_root: &Path,
) -> Output {
    let mut command = Command::new(assay);
    command
        .env("NO_COLOR", "1")
        .env("PATH", path)
        .env("PATHEXT", pathext)
        .args(["mcp", "preflight", "--policy-root"])
        .arg(policy_root)
        .args(["--format", "json"]);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("ASSAY_AUTH_") {
            command.env_remove(key);
        }
    }
    run_bounded(command, b"", GOLDEN_PATH_LIMITS, "native Windows preflight")
        .expect("native Windows preflight ran")
}

#[cfg(windows)]
fn preflight_document(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout is not one JSON document: {error}\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[cfg(windows)]
#[test]
fn native_windows_bare_command_uses_exe_not_pathext_scripts() {
    let root = tempfile::tempdir().expect("tempdir");
    let launcher_dir = root.path().join("launcher");
    let exe_dir = root.path().join("exe-only");
    fs::create_dir_all(&launcher_dir).expect("create isolated launcher directory");
    fs::create_dir_all(&exe_dir).expect("create exe fixture directory");

    let assay = launcher_dir.join("assay.exe");
    fs::copy(env!("CARGO_BIN_EXE_assay"), &assay).expect("copy assay into isolated directory");

    let server = exe_dir.join("assay-mcp-server.exe");
    compile_windows_server(&server);
    let exe_output = native_windows_preflight(&assay, &exe_dir, ".CMD;.BAT", root.path());
    let exe_document = preflight_document(&exe_output);
    assert_eq!(
        exit_code(&exe_output),
        0,
        "bare command must resolve .exe without .EXE in PATHEXT: {exe_document}"
    );
    assert_eq!(exe_document["schema"], PREFLIGHT_SCHEMA);
    assert_eq!(exe_document["phase"], "ready");
    assert_eq!(exe_document["expected_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(exe_document["actual_version"], env!("CARGO_PKG_VERSION"));

    let system_root = std::env::var_os("SystemRoot").expect("SystemRoot is set on Windows");
    let where_exe = PathBuf::from(system_root).join("System32/where.exe");
    for extension in ["cmd", "bat"] {
        let script_dir = root.path().join(format!("{extension}-only"));
        fs::create_dir_all(&script_dir).expect("create PATHEXT fixture directory");
        let script = script_dir.join(format!("assay-mcp-server.{extension}"));
        let marker = script_dir.join("invoked.txt");
        fs::write(
            &script,
            format!(
                "@echo off\r\necho invoked>\"%~dp0invoked.txt\"\r\nif \"%~1\"==\"--version\" echo assay-mcp-server {}\r\nexit /b 0\r\n",
                env!("CARGO_PKG_VERSION")
            ),
        )
        .expect("write PATHEXT script fixture");

        let pathext = format!(".{}", extension.to_ascii_uppercase());
        let where_output = Command::new(&where_exe)
            .arg("assay-mcp-server")
            .current_dir(&launcher_dir)
            .env("PATH", &script_dir)
            .env("PATHEXT", &pathext)
            .output()
            .expect("run where.exe");
        assert!(
            where_output.status.success(),
            "where.exe must resolve the .{extension} fixture through PATHEXT: stdout={} stderr={}",
            String::from_utf8_lossy(&where_output.stdout),
            String::from_utf8_lossy(&where_output.stderr)
        );
        let where_stdout = String::from_utf8_lossy(&where_output.stdout)
            .replace('\\', "/")
            .to_ascii_lowercase();
        let expected_suffix = format!("/{extension}-only/assay-mcp-server.{extension}");
        assert!(
            where_stdout
                .lines()
                .any(|line| line.ends_with(&expected_suffix)),
            "where.exe output must name the .{extension} fixture: {}",
            String::from_utf8_lossy(&where_output.stdout)
        );

        let output = native_windows_preflight(&assay, &script_dir, &pathext, root.path());
        let document = preflight_document(&output);
        assert_eq!(
            exit_code(&output),
            2,
            ".{extension} alone must not satisfy the bare Command lookup: {document}"
        );
        assert_eq!(document["phase"], "missing");
        assert!(
            document.get("actual_version").is_none(),
            ".{extension} missing phase must omit actual_version: {document}"
        );
        assert!(
            !marker.exists(),
            ".{extension} fixture must not be executed by the bare Command lookup"
        );
    }
}
