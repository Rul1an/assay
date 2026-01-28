//! `assay evidence list` - List evidence bundles in storage.

use anyhow::{Context, Result};
use assay_evidence::store::BundleStore;
use assay_evidence::{ObjectStoreBundleStore, StoreSpec};
use clap::{Args, ValueEnum};

#[derive(Debug, Args, Clone)]
pub struct ListArgs {
    /// List bundles for a specific run ID
    #[arg(long)]
    pub run_id: Option<String>,

    /// Filter by bundle ID prefix (e.g., sha256:abc)
    #[arg(long)]
    pub prefix: Option<String>,

    /// Maximum number of results
    #[arg(long, default_value = "100")]
    pub limit: usize,

    /// Store URL (e.g., s3://bucket/prefix, file:///path)
    #[arg(long, env = "ASSAY_STORE_URL")]
    pub store: String,

    /// Output format
    #[arg(long, value_enum, default_value = "plain")]
    pub format: ListFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ListFormat {
    /// One bundle ID per line (machine-friendly)
    Plain,
    /// JSON array with metadata
    Json,
    /// Human-readable table
    Table,
}

pub async fn cmd_list(args: ListArgs) -> Result<i32> {
    // Connect to store
    let spec = StoreSpec::parse(&args.store)
        .with_context(|| format!("invalid store URL: {}", args.store))?;

    let store = ObjectStoreBundleStore::from_spec(&spec)
        .await
        .with_context(|| "failed to connect to store")?;

    if let Some(run_id) = &args.run_id {
        // List bundles for a specific run
        list_for_run(&store, run_id, args.format).await
    } else {
        // List all bundles
        list_all(&store, args.prefix.as_deref(), args.limit, args.format).await
    }
}

async fn list_for_run(
    store: &ObjectStoreBundleStore,
    run_id: &str,
    format: ListFormat,
) -> Result<i32> {
    let bundle_ids = store
        .list_bundles_for_run(run_id)
        .await
        .with_context(|| format!("failed to list bundles for run: {}", run_id))?;

    match format {
        ListFormat::Plain => {
            for id in &bundle_ids {
                println!("{}", id);
            }
        }
        ListFormat::Json => {
            let json = serde_json::json!({
                "run_id": run_id,
                "bundles": bundle_ids,
                "count": bundle_ids.len()
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        ListFormat::Table => {
            eprintln!("Run: {}", run_id);
            eprintln!("Bundles: {}", bundle_ids.len());
            eprintln!();
            println!("{:<60} ", "BUNDLE_ID");
            println!("{:-<60}", "");
            for id in &bundle_ids {
                println!("{:<60}", id);
            }
        }
    }

    if bundle_ids.is_empty() && !matches!(format, ListFormat::Json) {
        eprintln!("(no bundles found)");
    }

    Ok(0)
}

async fn list_all(
    store: &ObjectStoreBundleStore,
    prefix: Option<&str>,
    limit: usize,
    format: ListFormat,
) -> Result<i32> {
    let metas = store
        .list_bundles(prefix, Some(limit))
        .await
        .context("failed to list bundles")?;

    match format {
        ListFormat::Plain => {
            for meta in &metas {
                println!("{}", meta.bundle_id);
            }
        }
        ListFormat::Json => {
            let json = serde_json::json!({
                "bundles": metas.iter().map(|m| {
                    serde_json::json!({
                        "bundle_id": m.bundle_id,
                        "size": m.size,
                        "modified": m.modified.map(|t| t.to_rfc3339())
                    })
                }).collect::<Vec<_>>(),
                "count": metas.len(),
                "prefix": prefix,
                "limit": limit
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        ListFormat::Table => {
            if let Some(p) = prefix {
                eprintln!("Prefix: {}", p);
            }
            eprintln!("Showing up to {} bundle(s)", limit);
            eprintln!();
            println!("{:<50} {:>12} MODIFIED", "BUNDLE_ID", "SIZE");
            println!("{:-<50} {:->12} {:-<20}", "", "", "");
            for meta in &metas {
                let size_str = meta
                    .size
                    .map(format_size)
                    .unwrap_or_else(|| "-".to_string());
                let modified_str = meta
                    .modified
                    .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "-".to_string());

                // Truncate bundle_id if too long
                let id_display = if meta.bundle_id.len() > 47 {
                    format!("{}...", &meta.bundle_id[..47])
                } else {
                    meta.bundle_id.clone()
                };

                println!("{:<50} {:>12} {}", id_display, size_str, modified_str);
            }
        }
    }

    if metas.is_empty() && !matches!(format, ListFormat::Json) {
        eprintln!("(no bundles found)");
    }

    Ok(0)
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
