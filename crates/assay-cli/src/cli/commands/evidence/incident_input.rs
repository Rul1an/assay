//! Shared bounded file admission for the incident-package CLI commands.

use std::io::Read;
use std::path::Path;

use assay_common::limits::{LimitExceeded, LimitKind, LimitReader};
use assay_evidence::{
    ContextInput, ContextLimits, IncidentOutcome, IncidentReason, IncidentVerifyReport,
};
use sha2::{Digest, Sha256};

pub(super) fn failure(reason: IncidentReason) -> IncidentVerifyReport {
    let mut report = IncidentVerifyReport::refusal(reason);
    if reason == IncidentReason::IoUnavailable {
        report.outcome = IncidentOutcome::VerificationUnavailable;
    }
    report
}

pub(super) fn read_file(
    path: &Path,
    limit: u64,
    exceeded: IncidentReason,
    no_follow: bool,
) -> Result<(Vec<u8>, String), IncidentReason> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | if no_follow { libc::O_NOFOLLOW } else { 0 });
    }
    #[cfg(not(unix))]
    let _ = no_follow;
    let file = options.open(path).map_err(|_error| {
        #[cfg(unix)]
        if _error.raw_os_error() == Some(libc::ELOOP) {
            return IncidentReason::InputShape;
        }
        IncidentReason::IoUnavailable
    })?;
    if !file
        .metadata()
        .map_err(|_| IncidentReason::IoUnavailable)?
        .is_file()
    {
        return Err(IncidentReason::InputShape);
    }
    read_bounded(file, limit, exceeded)
}

fn read_bounded(
    reader: impl Read,
    limit: u64,
    exceeded: IncidentReason,
) -> Result<(Vec<u8>, String), IncidentReason> {
    let mut reader = LimitReader::new(reader, limit, LimitKind::SourceBytes);
    let mut bytes = Vec::new();
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                return Err(if LimitExceeded::from_io(&error).is_some() {
                    exceeded
                } else {
                    IncidentReason::IoUnavailable
                })
            }
        };
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
        bytes.extend_from_slice(&buffer[..count]);
    }
    Ok((bytes, hex::encode(hash.finalize())))
}

pub(super) fn read_context(path: Option<&Path>) -> Result<ContextInput, IncidentReason> {
    let Some(path) = path else {
        return Ok(ContextInput::default());
    };
    // A CLI envelope, not a new ContextLimits wire field. The context is echoed
    // in the fresh report, whose immutable byte ceiling also bounds this input.
    let (bytes, _) = read_file(
        path,
        ContextLimits::HARD.fresh_result_bytes,
        IncidentReason::ResourceHard,
        false,
    )?;
    let text = std::str::from_utf8(&bytes).map_err(|_| IncidentReason::TrustInput)?;
    assay_evidence::json_strict::validate_json_strict(text)
        .map_err(|_| IncidentReason::TrustInput)?;
    let context: ContextInput =
        serde_json::from_slice(&bytes).map_err(|_| IncidentReason::TrustInput)?;
    assay_evidence::incident_package::verify::validate_context_input(&context)?;
    Ok(context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incident_source_stops_at_limit_plus_one_even_with_short_reads() {
        struct Short {
            consumed: usize,
        }
        impl Read for Short {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                let count = out.len().min(2).min(100 - self.consumed);
                out[..count].fill(b'x');
                self.consumed += count;
                Ok(count)
            }
        }
        let mut input = Short { consumed: 0 };
        assert_eq!(
            read_bounded(&mut input, 7, IncidentReason::ResourceSelected),
            Err(IncidentReason::ResourceSelected)
        );
        assert_eq!(input.consumed, 8);
        let (bytes, digest) = read_bounded(&b"[]\n"[..], 3, IncidentReason::Digest).unwrap();
        assert_eq!(bytes, b"[]\n");
        assert_eq!(
            digest,
            "37517e5f3dc66819f61f5a7bb8ace1921282415f10551d2defa5c3eb0985b570"
        );
    }

    #[test]
    fn incident_source_io_failure_never_becomes_a_short_success() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("injected read failure"))
            }
        }
        assert_eq!(
            read_bounded(Broken, 7, IncidentReason::Digest),
            Err(IncidentReason::IoUnavailable)
        );
    }
}
