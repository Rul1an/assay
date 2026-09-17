//! `assay policy status` — check active policy status and verify synchronization with activation history.

use serde::Serialize;

use crate::cli::args::{OutputFormat, PolicyStatusArgs};
use crate::exit_codes::{self, EXIT_CONFIG_ERROR};
use crate::output_write::write_stdout_json;

pub const SCHEMA_STATUS_V0: &str = "assay.policy.status.v0";

#[derive(Serialize)]
struct StatusDocument {
    schema: &'static str,
    name: String,
    in_sync: bool,
    input_sha256: String,
    policy_digest: String,
    latest_record: Option<String>,
    recorded_input_sha256: Option<String>,
    recorded_policy_digest: Option<String>,
}

pub async fn run(args: PolicyStatusArgs) -> anyhow::Result<i32> {
    let name = &args.name;
    super::activate::validate_target_name(name)?;

    let root = &args.root;
    let dirs = match super::activate::ensure_policy_root_dirs(root, false) {
        Ok(d) => d,
        Err(err) => {
            eprintln!("error: {err}");
            return Ok(EXIT_CONFIG_ERROR);
        }
    };

    let active_path = root.join(name);
    if let Ok(meta) = std::fs::symlink_metadata(&active_path) {
        if meta.file_type().is_symlink() {
            eprintln!(
                "error: active policy target '{}' in root {} is a symlink; refusing to operate on symlinks",
                name,
                root.display()
            );
            return Ok(EXIT_CONFIG_ERROR);
        }
        if meta.is_dir() {
            eprintln!(
                "error: active policy target '{}' in root {} is a directory; expected a regular file",
                name,
                root.display()
            );
            return Ok(EXIT_CONFIG_ERROR);
        }
    }

    if !active_path.exists() {
        eprintln!(
            "error: active policy file does not exist: {}",
            active_path.display()
        );
        return Ok(EXIT_CONFIG_ERROR);
    }

    let bytes = match super::resolved::read_bounded(&active_path) {
        Ok(b) => b,
        Err(err) => {
            eprintln!(
                "error: failed to read active policy {}: {err}",
                active_path.display()
            );
            return Ok(EXIT_CONFIG_ERROR);
        }
    };

    let resolved = match super::resolved::load_resolved(&bytes) {
        Ok(r) => r,
        Err(err) => {
            eprintln!(
                "error: active policy {} failed to validate: {err}",
                active_path.display()
            );
            return Ok(EXIT_CONFIG_ERROR);
        }
    };

    let latest = super::activate::find_latest_activation_record(&dirs.activations_dir, name)?;
    let (_latest_seq, latest_record_file, latest_rec) = match latest {
        Some(r) => r,
        None => {
            eprintln!(
                "error: active policy file '{}' is unrecorded (no activation record found in {})",
                name,
                dirs.activations_dir.display()
            );
            if args.format == OutputFormat::Json {
                let doc = StatusDocument {
                    schema: SCHEMA_STATUS_V0,
                    name: name.clone(),
                    in_sync: false,
                    input_sha256: resolved.input_sha256,
                    policy_digest: resolved.policy_digest,
                    latest_record: None,
                    recorded_input_sha256: None,
                    recorded_policy_digest: None,
                };
                let json = serde_json::to_string(&doc)?;
                write_stdout_json(&json);
            }
            return Ok(EXIT_CONFIG_ERROR);
        }
    };

    // Check hash and digest match latest record
    let sha_matches = resolved.input_sha256 == latest_rec.input_sha256;
    let digest_matches = resolved.policy_digest == latest_rec.policy_digest;

    if !sha_matches || !digest_matches {
        eprintln!(
            "error: active policy bytes differ from latest activation record {}\n  active:   input={}, digest={}\n  recorded: input={}, digest={}",
            latest_record_file,
            resolved.input_sha256,
            resolved.policy_digest,
            latest_rec.input_sha256,
            latest_rec.policy_digest,
        );
        if args.format == OutputFormat::Json {
            let doc = StatusDocument {
                schema: SCHEMA_STATUS_V0,
                name: name.clone(),
                in_sync: false,
                input_sha256: resolved.input_sha256,
                policy_digest: resolved.policy_digest,
                latest_record: Some(latest_record_file),
                recorded_input_sha256: Some(latest_rec.input_sha256),
                recorded_policy_digest: Some(latest_rec.policy_digest),
            };
            let json = serde_json::to_string(&doc)?;
            write_stdout_json(&json);
        }
        return Ok(EXIT_CONFIG_ERROR);
    }

    // Check store
    let in_store = dirs.store_dir.join(&resolved.input_sha256).exists();
    if !in_store {
        eprintln!(
            "error: active policy content '{}' is not present in policy store {}",
            resolved.input_sha256,
            dirs.store_dir.display()
        );
        return Ok(EXIT_CONFIG_ERROR);
    }

    if args.format == OutputFormat::Json {
        let doc = StatusDocument {
            schema: SCHEMA_STATUS_V0,
            name: name.clone(),
            in_sync: true,
            input_sha256: resolved.input_sha256,
            policy_digest: resolved.policy_digest,
            latest_record: Some(latest_record_file),
            recorded_input_sha256: Some(latest_rec.input_sha256),
            recorded_policy_digest: Some(latest_rec.policy_digest),
        };
        let json = serde_json::to_string(&doc)?;
        return Ok(write_stdout_json(&json));
    }

    eprintln!(
        "✔ Policy active and in sync: {} (input: {}, digest: {}, record: {})",
        name, resolved.input_sha256, resolved.policy_digest, latest_record_file
    );

    Ok(exit_codes::OK)
}
