//! Optional startup context. The decision-only stream and its readers are unchanged.
use anyhow::{Context, Result};
use assay_core::mcp::{jcs, proxy::ProxyConfig};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::Path,
};

pub(super) fn reserve<const N: usize>(
    path: &Path,
    other_outputs: [Option<&Path>; N],
) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(path)
        .context("cannot create new session context file")?;
    // The target now exists: canonicalize also detects relative, parent-symlink,
    // and filesystem case aliases to this previously absent target.
    let target = path
        .canonicalize()
        .context("cannot resolve session context file")?;
    for other in other_outputs.into_iter().flatten() {
        match other.canonicalize() {
            Ok(resolved) if resolved == target => {
                anyhow::bail!("session context and other artifacts must use different output paths")
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("cannot resolve other output path"),
        }
    }
    Ok(file)
}

pub(super) fn event(argv: &[String], config: &ProxyConfig, session_id: &str) -> Result<Value> {
    let digest = hex::encode(Sha256::digest(jcs::to_vec(&argv)?));
    Ok(json!({
        "specversion": "1.0",
        "id": format!("evt_context_{session_id}"),
        "type": "assay.mcp.session.context.v1",
        "source": config.event_source.as_deref().context("session context requires event source")?,
        "time": chrono::Utc::now().to_rfc3339(),
        "datacontenttype": "application/json",
        "data": {
            "server_id": config.server_id,
            "enforcement": if config.dry_run { "dry_run" } else { "enforce" },
            "server_binding_basis": "spawned_command",
            "server_binding_digest": format!("sha256:{digest}"),
            "server_binding_digest_alg": "sha256",
            "server_binding_canonicalization": "jcs:argv.v1"
        }
    }))
}

pub(super) fn write(mut output: impl Write, event: &Value) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(event)?;
    bytes.push(b'\n');
    output.write_all(&bytes)?;
    output.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_context_digest_preserves_empty_arguments_unicode_and_order() {
        let config = ProxyConfig {
            dry_run: false,
            verbose: false,
            audit_log_path: None,
            server_id: "test".into(),
            decision_log_path: None,
            event_source: Some("assay://test".into()),
            tdt_producer: None,
        };
        // Independently calculated with Python JSON (UTF-8, compact) + hashlib;
        // these string-only vectors have no object-key/numeric canonicalization.
        for (argv, expected) in [
            (
                vec!["tool"],
                "f3a60c587a13313f1669991d6dbf560c8deb18ac2b5602f05c121d3ed630c964",
            ),
            (
                vec!["tool", "", "雪", "\""],
                "c0a74910074d7b5932456e4dee3c6c9806d2a75a59b49935aca80efac43f5688",
            ),
            (
                vec!["tool", "b", "a"],
                "7e6a14baef28e9ab6cd4c41fcd073642f0cdd5de3fb17cd12acc690b8f74f579",
            ),
            (
                vec!["tool", "a", "b"],
                "078ce8d349df971bc71b5d145f56093ab857e06b6e67e9f91576e8a5d41eb3a5",
            ),
        ] {
            let argv: Vec<_> = argv.into_iter().map(str::to_owned).collect();
            let value = event(&argv, &config, "test-id").unwrap();
            assert_eq!(
                value["data"]["server_binding_digest"],
                format!("sha256:{expected}")
            );
        }
    }

    struct FailingWriter {
        fail_flush: bool,
    }
    impl Write for FailingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail_flush {
                Ok(bytes.len())
            } else {
                Err(io::Error::other("injected write failure"))
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("injected flush failure"))
        }
    }
    #[test]
    fn session_context_propagates_write_and_flush_errors() {
        for fail_flush in [false, true] {
            let error = write(FailingWriter { fail_flush }, &json!({"type":"test"})).unwrap_err();
            assert_eq!(
                error.to_string(),
                if fail_flush {
                    "injected flush failure"
                } else {
                    "injected write failure"
                }
            );
        }
    }
}
