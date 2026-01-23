use std::process::Command;
use tempfile::TempDir;

const FULL_LIFECYCLE_POLICY: &str = r#"
version: "2.0"

discovery:
  enabled: true
  methods: [config_files, processes]
  on_findings:
    unmanaged_server: fail

runtime_monitor:
  enabled: true
  rules:
    - id: block-ssh-read
      type: file_open
      match:
        path_globs: ["**/.ssh/*"]
      severity: critical
      action: trigger_kill

kill_switch:
  enabled: true
  mode: immediate
  capture_state: true
  output_dir: ".assay/incidents"
  triggers:
    - on_rule: block-ssh-read
      mode: immediate
"#;

#[test]
fn test_policy_parses_all_blocks() {
    let policy: assay_core::mcp::policy::McpPolicy =
        serde_yaml::from_str(FULL_LIFECYCLE_POLICY).unwrap();

    assert!(policy.discovery.is_some());
    assert!(policy.runtime_monitor.is_some());
    assert!(policy.kill_switch.is_some());
}

fn setup_fake_unmanaged_config(root: &std::path::Path) {
    // Unix-style path (Linux/macOS + sometimes used cross-platform)
    let claude_unix = root.join(".config/claude");
    std::fs::create_dir_all(&claude_unix).unwrap();
    std::fs::write(
        claude_unix.join("claude_desktop_config.json"),
        r#"{
          "mcpServers": {
            "unmanaged-server": { "command": "echo", "args": ["hello"] }
          }
        }"#,
    )
    .unwrap();

    // Windows-style paths (AppData/Roaming and AppData/Local)
    let appdata_roaming = root.join("AppData").join("Roaming");
    let claude_roaming = appdata_roaming.join("Claude");
    std::fs::create_dir_all(&claude_roaming).unwrap();
    std::fs::write(
        claude_roaming.join("claude_desktop_config.json"),
        r#"{
          "mcpServers": {
            "unmanaged-server": { "command": "echo", "args": ["hello"] }
          }
        }"#,
    )
    .unwrap();

    let appdata_local = root.join("AppData").join("Local");
    let claude_local = appdata_local.join("Claude");
    std::fs::create_dir_all(&claude_local).unwrap();
    std::fs::write(
        claude_local.join("claude_desktop_config.json"),
        r#"{
          "mcpServers": {
            "unmanaged-server": { "command": "echo", "args": ["hello"] }
          }
        }"#,
    )
    .unwrap();
}

#[test]
fn test_discovery_respects_policy_fail_on_unmanaged() {
    let temp = TempDir::new().unwrap();
    let policy_path = temp.path().join("policy.yaml");
    std::fs::write(&policy_path, FULL_LIFECYCLE_POLICY).unwrap();

    setup_fake_unmanaged_config(temp.path());

    let appdata = temp.path().join("AppData").join("Roaming");
    let xdg_config_home = temp.path().join(".config");

    let out = Command::new(env!("CARGO_BIN_EXE_assay"))
        // Make discovery resolve “home” into the temp dir on Windows
        .env("HOME", temp.path())
        .env("USERPROFILE", temp.path())
        .env("HOMEDRIVE", "C:")
        .env("HOMEPATH", r"\") // not strictly required if USERPROFILE is set, but harmless
        // Make config_dir() resolve into the temp dir on Windows
        .env("APPDATA", &appdata)
        // Make XDG lookups deterministic too
        .env("XDG_CONFIG_HOME", &xdg_config_home)
        .args(["discover", "--policy", policy_path.to_str().unwrap()])
        .output()
        .unwrap();

    if out.status.success() {
        panic!(
            "expected discovery to fail; stdout={}\nstderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
    }
}

#[test]
fn test_kill_switch_config_parses_mode() {
    let policy: assay_core::mcp::policy::McpPolicy = serde_yaml::from_str(
        r#"
version: "2.0"
kill_switch:
  enabled: true
  mode: graceful
  grace_period_ms: 1000
  kill_children: true
"#,
    )
    .unwrap();

    let ks = policy.kill_switch.unwrap();
    assert!(ks.enabled);
    assert!(ks.kill_children);
    assert_eq!(ks.grace_period_ms, 1000);
}

#[test]
#[cfg(target_os = "linux")]
#[ignore = "requires CAP_BPF/sudo + ebpf artifact"]
fn test_monitor_triggers_kill_on_ssh_read() {
    let temp = TempDir::new().unwrap();
    let policy_path = temp.path().join("policy.yaml");
    std::fs::write(&policy_path, FULL_LIFECYCLE_POLICY).unwrap();

    let mut child = Command::new("bash")
        .args([
            "-c",
            "mkdir -p ~/.ssh; echo 'secret' > ~/.ssh/id_rsa; sleep 1; cat ~/.ssh/id_rsa; sleep 30",
        ])
        .spawn()
        .unwrap();

    let pid = child.id().to_string();

    let _ = Command::new(env!("CARGO_BIN_EXE_assay"))
        .args([
            "monitor",
            "--pid",
            &pid,
            "--policy",
            policy_path.to_str().unwrap(),
            "--duration",
            "10s",
            "--print",
        ])
        .status()
        .unwrap();

    let status = child.wait().unwrap();
    assert!(!status.success(), "expected child to be killed by policy");
}
