//! Capture-side redaction finalization for the runner-spike command (ADR-034).
//!
//! Order is load-bearing: `capture -> redact -> serialize -> assert clean -> hash/sign`. This runs
//! after the archive is assembled and BEFORE it is written (which is where the manifest hashes are
//! computed), so a raw secret never reaches the serialized bytes, the hash input, or the signature.
//!
//! The assertion sweep only asserts: if a secret-shaped value survives, bundle creation fails hard
//! (the caller propagates the error to a non-zero exit). It never rewrites bytes, so a missed capture
//! funnel is surfaced as a bug rather than silently patched. No raw value is ever placed in an error,
//! a log line, or the health block.

use std::path::PathBuf;

use assay_runner_core::{RedactMode, RedactionKey, Redactor, RunnerSpikeArchive, ENV_KEY_FILE};
use assay_runner_schema::Redaction;

use super::args::{RedactArg, RedactionKeyArg, RunnerSpikeRunArgs};

/// Redact the assembled archive in place, record the value-free `observation_health.redaction`
/// summary, and run the fail-closed assertion sweep. Must be called before the archive is written.
pub(super) fn finalize_redaction(
    args: &RunnerSpikeRunArgs,
    archive: &mut RunnerSpikeArchive,
) -> anyhow::Result<()> {
    if args.unsafe_disable_redaction {
        eprintln!(
            "WARNING: --unsafe-disable-redaction is set. This bundle may contain raw credentials in \
             argv, filesystem paths, or tool names. Do not share or retain it."
        );
        archive.observation_health.redaction = Some(Redaction {
            mode: "disabled_unsafe".to_string(),
            redacted_count: 0,
            by_rule: Default::default(),
            by_field: Default::default(),
            key_scope: "ephemeral".to_string(),
            key_id: "none".to_string(),
        });
        return Ok(());
    }

    let mode = match args.redact {
        RedactArg::ShapeAndFlag => RedactMode::ShapeAndFlag,
        RedactArg::ShapeOnly => RedactMode::ShapeOnly,
    };
    let key = resolve_key(args)?;
    let redactor = Redactor::new(mode, key.salt(), Vec::new());

    let tally = archive.redact_in_place(&redactor);
    archive.observation_health.redaction = Some(Redaction {
        mode: mode.as_health_str().to_string(),
        redacted_count: tally.total,
        by_rule: tally.by_rule,
        by_field: tally.by_field,
        key_scope: key.scope().as_str().to_string(),
        key_id: key.key_id().to_string(),
    });

    // Hard fail-closed: a surviving secret-shaped value aborts bundle creation.
    archive.assert_no_unredacted(&redactor)?;
    Ok(())
}

fn resolve_key(args: &RunnerSpikeRunArgs) -> anyhow::Result<RedactionKey> {
    match args.redaction_key {
        RedactionKeyArg::Ephemeral => {
            eprintln!(
                "WARNING: --redaction-key ephemeral. Redaction tokens in this bundle do not correlate \
                 with any other run."
            );
            Ok(RedactionKey::ephemeral())
        }
        RedactionKeyArg::HostLocal => {
            let env = std::env::var_os(ENV_KEY_FILE).map(PathBuf::from);
            match RedactionKey::resolve_host_local(env.as_deref(), &default_key_path()) {
                Ok(key) => Ok(key),
                // System path not writable and no explicit env override: fall back to a user-mode path.
                Err(_) if env.is_none() => {
                    Ok(RedactionKey::resolve_host_local(None, &user_key_path())?)
                }
                Err(err) => Err(err.into()),
            }
        }
    }
}

fn default_key_path() -> PathBuf {
    PathBuf::from("/var/lib/assay/redaction.key")
}

fn user_key_path() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        PathBuf::from(xdg).join("assay/redaction.key")
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".local/share/assay/redaction.key")
    } else {
        std::env::temp_dir().join("assay/redaction.key")
    }
}
