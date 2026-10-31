//! Optional startup context. The decision-only stream and its readers are unchanged.
use anyhow::{Context, Result};
use assay_core::mcp::jcs;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

/// The one-run outputs of a wrap, and the other outputs they must not alias.
pub(super) struct OneRunOutputs {
    pub(super) stop: Option<PathBuf>,
    pub(super) decision_log: Option<PathBuf>,
    pub(super) receipts: Option<PathBuf>,
    pub(super) context: Option<PathBuf>,
    pub(super) effective_decision_log: Option<PathBuf>,
    pub(super) audit_log: Option<PathBuf>,
    pub(super) coverage: Option<PathBuf>,
    pub(super) state_window: Option<PathBuf>,
    pub(super) tool_decision_truth: Option<PathBuf>,
}

pub(super) struct ReservedOutputs {
    pub(super) stop: Option<File>,
    pub(super) decision_log: Option<File>,
    pub(super) receipts: Option<File>,
    pub(super) context: Option<File>,
}

impl OneRunOutputs {
    /// Prove before spawn that every output can be created, then give the paths back: a start that
    /// is refused, or that never becomes a session, consumes none of them. This is [`Self::create`]
    /// itself, so the check and the later creation cannot disagree. It still removes files, so it
    /// is only for use before the server runs.
    pub(super) fn preflight(&self) -> Result<()> {
        let mut created = Vec::new();
        let outcome = self.create_recording(&mut created).map(drop);
        for path in created {
            std::fs::remove_file(path).context("cannot give back a checked output")?;
        }
        outcome
    }

    /// Create every output, refusing any that exists. Nothing already there is ever removed.
    pub(super) fn create(&self) -> Result<ReservedOutputs> {
        self.create_recording(&mut Vec::new())
    }

    fn create_recording(&self, created: &mut Vec<PathBuf>) -> Result<ReservedOutputs> {
        let stop = self
            .stop
            .as_deref()
            .map(|path| {
                reserve(
                    path,
                    "session stop",
                    [
                        self.effective_decision_log.as_deref(),
                        self.audit_log.as_deref(),
                        self.coverage.as_deref(),
                        self.state_window.as_deref(),
                        self.tool_decision_truth.as_deref(),
                        self.context.as_deref(),
                        self.receipts.as_deref(),
                    ],
                    created,
                )
            })
            .transpose()?;
        let decision_log = self
            .decision_log
            .as_deref()
            .map(|path| {
                reserve(
                    path,
                    "decision log",
                    [
                        self.audit_log.as_deref(),
                        self.coverage.as_deref(),
                        self.state_window.as_deref(),
                        self.tool_decision_truth.as_deref(),
                        self.context.as_deref(),
                        self.receipts.as_deref(),
                    ],
                    created,
                )
                .context("cannot reserve --decision-log output")
            })
            .transpose()?;
        let receipts = self
            .receipts
            .as_deref()
            .map(|path| {
                reserve(
                    path,
                    "request receipts",
                    [
                        self.effective_decision_log.as_deref(),
                        self.audit_log.as_deref(),
                        self.coverage.as_deref(),
                        self.state_window.as_deref(),
                        self.tool_decision_truth.as_deref(),
                        self.context.as_deref(),
                    ],
                    created,
                )
                .context("cannot reserve --request-receipts-out output")
            })
            .transpose()?;
        let context = self
            .context
            .as_deref()
            .map(|path| {
                reserve(
                    path,
                    "session context",
                    [
                        self.effective_decision_log.as_deref(),
                        self.audit_log.as_deref(),
                        self.coverage.as_deref(),
                        self.state_window.as_deref(),
                        self.tool_decision_truth.as_deref(),
                    ],
                    created,
                )
            })
            .transpose()?;
        Ok(ReservedOutputs {
            stop,
            decision_log,
            receipts,
            context,
        })
    }
}

fn reserve<const N: usize>(
    path: &Path,
    label: &str,
    other_outputs: [Option<&Path>; N],
    created: &mut Vec<PathBuf>,
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
        .with_context(|| format!("cannot create new {label} file"))?;
    created.push(path.to_path_buf());
    // The target now exists: canonicalize also detects relative, parent-symlink,
    // and filesystem case aliases to this previously absent target.
    let target = path
        .canonicalize()
        .with_context(|| format!("cannot resolve {label} file"))?;
    for other in other_outputs.into_iter().flatten() {
        match other.canonicalize() {
            Ok(resolved) if resolved == target => {
                anyhow::bail!("{label} and other artifacts must use different output paths")
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("cannot resolve other output path"),
        }
    }
    Ok(file)
}

pub(super) fn event(
    argv: &[String],
    source: &str,
    server_id: &str,
    dry_run: bool,
    session_id: &str,
) -> Result<Value> {
    let digest = hex::encode(Sha256::digest(jcs::to_vec(&argv)?));
    Ok(json!({
        "specversion": "1.0",
        "id": format!("evt_context_{session_id}"),
        "type": "assay.mcp.session.context.v1",
        "source": source,
        "time": chrono::Utc::now().to_rfc3339(),
        "datacontenttype": "application/json",
        "data": {
            "server_id": server_id,
            "enforcement": if dry_run { "dry_run" } else { "enforce" },
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
            let value = event(&argv, "assay://test", "test", false, "test-id").unwrap();
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
