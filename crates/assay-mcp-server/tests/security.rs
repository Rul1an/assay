use anyhow::Result;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

#[tokio::test]
async fn test_path_traversal_prevention() -> Result<()> {
    // 1. Setup Server
    let status = Command::new("cargo")
        .args(&["build", "-p", "assay-mcp-server"])
        .status()
        .await?;
    assert!(status.success());
    // Resolve binary path
    let mut bin_path = std::env::current_dir()?.join("target/debug/assay-mcp-server");
    if !bin_path.exists() {
        // Try workspace root from crate dir
        bin_path = std::env::current_dir()?.join("../../target/debug/assay-mcp-server");
    }
    if !bin_path.exists() {
        panic!(
            "Could not find assay-mcp-server binary at {:?} or in ../../target",
            std::env::current_dir()?.join("target/debug/assay-mcp-server")
        );
    }

    // Create a temp policy root
    let temp_root = std::env::temp_dir().join(format!(
        "assay_sec_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    tokio::fs::create_dir_all(&temp_root).await?;

    // Create a valid policy
    tokio::fs::write(temp_root.join("valid.yaml"), "ls:\n  type: object").await?;

    // Create a file outside the root (but effectively reachable via ..)
    let secret_file = temp_root.parent().unwrap().join("secret_config.yaml");
    tokio::fs::write(&secret_file, "secret: true").await?;
    let secret_rel_path = "../secret_config.yaml";

    let mut cmd = Command::new(bin_path)
        .arg("--policy-root")
        .arg(&temp_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut stdin = cmd.stdin.take().unwrap();
    let stdout = cmd.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    // 2. Test: Access Valid Policy
    let req_valid = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "assay_check_args",
            "arguments": {
                "tool": "ls",
                "arguments": {},
                "policy": "valid.yaml"
            }
        }
    });
    stdin.write_all(req_valid.to_string().as_bytes()).await?;
    stdin.write_all(b"\n").await?;

    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let resp: serde_json::Value = serde_json::from_str(&line)?;
    assert!(
        resp["result"]["allowed"].as_bool().unwrap_or(false),
        "Valid policy should be allowed"
    );

    // 3. Test: Access Invalid Path (Traversal)
    let req_hack = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "assay_check_args",
            "arguments": {
                "tool": "ls",
                "arguments": {},
                "policy": secret_rel_path
            }
        }
    });

    stdin.write_all(req_hack.to_string().as_bytes()).await?;
    stdin.write_all(b"\n").await?;

    line.clear();
    reader.read_line(&mut line).await?;
    let resp: serde_json::Value = serde_json::from_str(&line)?;

    // Expect failure
    let result = &resp["result"];
    assert_eq!(
        result["allowed"].as_bool(),
        Some(false),
        "Should deny traversal"
    );
    assert_eq!(
        result["error"]["code"].as_str(),
        Some("E_PERMISSION_DENIED"),
        "Should return E_PERMISSION_DENIED"
    );

    // 4. Test: Symlink Escape (Unix only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        // Create a symlink in root pointing to outside secret
        let link_path = temp_root.join("evil.yaml");
        symlink(&secret_file, &link_path)?;

        let req_sym = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "assay_check_args",
                "arguments": {
                    "tool": "ls",
                    "arguments": {},
                    "policy": "evil.yaml"
                }
            }
        });

        stdin.write_all(req_sym.to_string().as_bytes()).await?;
        stdin.write_all(b"\n").await?;

        line.clear();
        reader.read_line(&mut line).await?;
        let resp: serde_json::Value = serde_json::from_str(&line)?;

        // Expect failure due to canonicalization check
        let result = &resp["result"];
        assert_eq!(
            result["allowed"].as_bool(),
            Some(false),
            "Should deny symlink escape"
        );
        assert_eq!(
            result["error"]["code"].as_str(),
            Some("E_PERMISSION_DENIED"),
            "Should return E_PERMISSION_DENIED for symlink"
        );
    }

    // Cleanup
    let _ = cmd.kill().await;
    let _ = tokio::fs::remove_dir_all(&temp_root).await;
    // secret_file is outside temp_root, clean it up manually
    let _ = tokio::fs::remove_file(secret_file).await;

    Ok(())
}
