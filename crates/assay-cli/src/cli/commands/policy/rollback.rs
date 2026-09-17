//! `assay policy rollback` — roll back an active policy to its previously activated version.

use crate::cli::args::PolicyRollbackArgs;
use crate::exit_codes;

pub async fn run(args: PolicyRollbackArgs) -> anyhow::Result<i32> {
    let name = &args.name;
    super::activate::validate_target_name(name)?;

    let root = &args.root;
    let assay_dir = root.join(".assay");
    let store_dir = assay_dir.join("policy-store");
    let activations_dir = assay_dir.join("activations");

    if !activations_dir.exists() {
        anyhow::bail!("no activation history found for root {}", root.display());
    }

    let latest = super::activate::find_latest_activation_record(&activations_dir, name)?;
    let (_latest_seq, latest_record_file, latest_rec) =
        latest.ok_or_else(|| anyhow::anyhow!("no activation records found for policy '{name}'"))?;

    let prev_sha = latest_rec
        .previous_input_sha256
        .as_deref()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "cannot rollback policy '{name}': no previous version recorded in latest activation record {latest_record_file}"
            )
        })?;

    let stored_bytes_path = store_dir.join(prev_sha);
    let bytes = std::fs::read(&stored_bytes_path).map_err(|err| {
        anyhow::anyhow!(
            "stored policy content '{prev_sha}' missing from store {}: {err}",
            store_dir.display()
        )
    })?;

    // Validate stored bytes before restoring
    let resolved = super::resolved::load_resolved(&bytes)
        .map_err(|err| anyhow::anyhow!("stored policy for rollback failed to validate: {err}"))?;

    // Replace pointer
    super::activate::replace_pointer_atomic(root, name, &bytes)?;

    // Write rollback activation record
    let source_str = format!("rollback:{}", latest_rec.input_sha256);
    let rollback_of = Some(latest_record_file);
    let (record, record_path) = super::activate::write_activation_record(
        &activations_dir,
        name,
        &resolved.input_sha256,
        &resolved.policy_digest,
        Some(latest_rec.input_sha256.clone()),
        Some(latest_rec.policy_digest.clone()),
        &source_str,
        rollback_of,
    )?;

    eprintln!(
        "✔ Policy rolled back: {} (input: {}, digest: {}, record: {})",
        name,
        record.input_sha256,
        record.policy_digest,
        record_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
    );

    Ok(exit_codes::OK)
}
