//! A configured server has no PID, so these file-reader tests never signal a process.
use std::{fs, process::Command};

const YAML: &str = r#"
generated_at: 2026-11-19T00:00:00Z
host: {hostname: fixture, os: linux, arch: aarch64}
servers:
  - id: fixture
    source: {type: config_file, path: fixture, client: generic}
    transport: {type: unknown}
    status: configured
    policy_status: !managed {policy_file: fixture.yaml}
    auth: none
summary: {total: 1, configured: 1, running: 0, managed: 1, unmanaged: 0, with_auth: 0, without_auth: 1}
"#;

const JSON: &str = r#"{
    "generated_at":"2026-11-19T00:00:00Z",
    "host":{"hostname":"fixture","os":"linux","arch":"aarch64"},
    "servers":[{"id":"fixture","source":{"type":"config_file","path":"fixture","client":"generic"},
        "transport":{"type":"unknown"},"status":"configured",
        "policy_status":{"managed":{"policy_file":"fixture.yaml"}},"auth":"none"}],
    "summary":{"total":1,"configured":1,"running":0,"managed":1,"unmanaged":0,"with_auth":0,"without_auth":1}
}"#;

fn check_managed_inventory(extension: &str, good: &str, field: &str, replacement: &str) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(format!("inventory.{extension}"));
    let invoke = |contents: &str| {
        fs::write(&path, contents).unwrap();
        Command::new(env!("CARGO_BIN_EXE_assay"))
            .args(["mcp", "kill", "--all", "--yes", "--inventory"])
            .arg(&path)
            .output()
            .unwrap()
    };
    let output = invoke(good);
    assert!(output.status.success(), "{:?}", output);
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("No running process-based servers"));
    let invalid = good.replace(field, replacement);
    assert_ne!(good, invalid);
    let output = invoke(&invalid);
    assert!(
        !output.status.success(),
        "unknown managed-policy field accepted"
    );
    let error = String::from_utf8(output.stderr).unwrap();
    let expected_path = match extension {
        "json" => "servers.0.policy_status.managed.extra_setting",
        "yaml" => "servers.0.policy_status.extra_setting",
        _ => unreachable!(),
    };
    assert!(error.contains(expected_path), "{error}");
}

#[test]
fn yaml_managed_inventory_refuses_an_unknown_policy_field() {
    check_managed_inventory(
        "yaml",
        YAML,
        "policy_file: fixture.yaml",
        "policy_file: fixture.yaml, extra_setting: true",
    );
}

#[test]
fn json_managed_inventory_refuses_an_unknown_policy_field() {
    check_managed_inventory(
        "json",
        JSON,
        r#""policy_file":"fixture.yaml""#,
        r#""policy_file":"fixture.yaml","extra_setting":true"#,
    );
}

#[test]
fn inventory_size_limit_applies_before_decoding() {
    let dir = tempfile::tempdir().unwrap();
    for (format, good) in [("yaml", YAML), ("json", JSON)] {
        let path = dir.path().join(format!("inventory.{format}"));
        let mut bytes = good.as_bytes().to_vec();
        bytes.resize(16 * 1024 * 1024, b' ');
        fs::write(&path, &bytes).unwrap();
        let invoke = || {
            Command::new(env!("CARGO_BIN_EXE_assay"))
                .args(["mcp", "kill", "--all", "--yes", "--inventory"])
                .arg(&path)
                .output()
                .unwrap()
        };
        assert!(invoke().status.success());
        bytes.push(0xff);
        fs::write(&path, bytes).unwrap();
        let output = invoke();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("16777216"));
    }
}

#[test]
fn discovery_output_roundtrips_through_both_file_readers() {
    use assay_core::discovery::types::{
        AuthStatus, DiscoverySource, Inventory, PolicyStatus, Transport,
    };

    let mut inventory: Inventory = serde_json::from_str(JSON).unwrap();
    let example = inventory.servers[0].clone();
    for (i, (transport, auth)) in [
        (
            Transport::Stdio {
                command: "tool".into(),
                args: vec![],
            },
            AuthStatus::None,
        ),
        (
            Transport::Http {
                url: "http://fixture".into(),
            },
            AuthStatus::ApiKey,
        ),
        (
            Transport::Sse {
                url: "https://fixture".into(),
            },
            AuthStatus::OAuth,
        ),
        (
            Transport::WebSocket {
                url: "wss://fixture".into(),
            },
            AuthStatus::Mtls,
        ),
        (Transport::Unknown, AuthStatus::Unknown),
    ]
    .into_iter()
    .enumerate()
    {
        let mut server = example.clone();
        server.id = format!("fixture-{i}");
        server.source = DiscoverySource::NetworkScan {
            address: "localhost".into(),
            port: 443,
        };
        server.policy_status = if i == 0 {
            PolicyStatus::Unmanaged
        } else {
            PolicyStatus::Unknown
        };
        server.transport = transport;
        server.auth = auth;
        inventory.servers.push(server);
    }
    let dir = tempfile::tempdir().unwrap();
    for (format, bytes) in [
        ("yaml", serde_yaml::to_string(&inventory).unwrap()),
        ("json", serde_json::to_string_pretty(&inventory).unwrap()),
    ] {
        assert!(bytes.contains("web_socket") && bytes.contains("o_auth"));
        let path = dir.path().join(format!("inventory.{format}"));
        fs::write(&path, bytes).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_assay"))
            .args(["mcp", "kill", "--all", "--yes", "--inventory"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("No running process-based servers")
        );
    }
}

#[test]
fn file_only_transport_and_source_keys_do_not_change_direct_serde() {
    use assay_core::discovery::types::{DiscoverySource, Transport};
    let source = r#"{"type":"config_file","path":"fixture","client":"generic","future":true}"#;
    let transport = r#"{"type":"http","url":"https://fixture","future":true}"#;
    assert!(matches!(
        serde_json::from_str::<DiscoverySource>(source).unwrap(),
        DiscoverySource::ConfigFile { .. }
    ));
    assert!(matches!(
        serde_json::from_str::<Transport>(transport).unwrap(),
        Transport::Http { .. }
    ));
    let mut inventory: serde_json::Value = serde_json::from_str(JSON).unwrap();
    inventory["servers"][0]["source"] = serde_json::from_str(source).unwrap();
    inventory["servers"][0]["transport"] = serde_json::from_str(transport).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inventory.json");
    fs::write(&path, serde_json::to_vec(&inventory).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(["mcp", "kill", "--all", "--yes", "--inventory"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8(output.stderr).unwrap();
    for path in ["servers.0.source.future", "servers.0.transport.future"] {
        assert!(error.contains(path), "{error}");
    }
}
