//! `assay evidence store-status` - Check evidence store connectivity and status.

use anyhow::{Context, Result};
use assay_evidence::{resolve_store_url, ObjectStoreBundleStore, StoreSpec};
use clap::{Args, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Args, Clone)]
pub struct StoreStatusArgs {
    /// Store URL (e.g., s3://bucket/prefix, file:///path)
    #[arg(long, env = "ASSAY_STORE_URL")]
    pub store: Option<String>,

    /// Path to store config YAML (default: .assay/store.yaml)
    #[arg(long)]
    pub store_config: Option<PathBuf>,

    /// Output format
    #[arg(long, value_enum, default_value = "table")]
    pub format: StatusFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum StatusFormat {
    /// Human-readable table
    Table,
    /// JSON output
    Json,
    /// Machine-friendly key=value
    Plain,
}

pub async fn cmd_store_status(args: StoreStatusArgs) -> Result<i32> {
    let url = resolve_store_url(args.store.as_deref(), args.store_config.as_deref())
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let spec = StoreSpec::parse(&url).with_context(|| format!("invalid store URL: {}", url))?;

    let store = ObjectStoreBundleStore::from_spec(&spec)
        .await
        .with_context(|| "failed to connect to store")?;

    let status = store.store_status(&spec).await;

    match args.format {
        StatusFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&status)?);
        }
        StatusFormat::Plain => {
            println!("reachable={}", status.reachable);
            println!("readable={}", status.readable);
            println!("writable={}", status.writable);
            println!("backend={}", status.backend);
            println!("bucket={}", status.bucket.as_deref().unwrap_or("-"));
            println!("prefix={}", status.prefix);
            println!("bundle_count={}", status.bundle_count);
            println!("total_size_bytes={}", status.total_size_bytes);
            println!("object_lock={}", status.object_lock);
        }
        StatusFormat::Table => {
            let check = |ok: bool| if ok { "OK" } else { "FAIL" };

            eprintln!("Evidence Store Status");
            eprintln!("====================");
            eprintln!();
            eprintln!("  Backend:      {}", status.backend);
            eprintln!(
                "  Bucket:       {}",
                status.bucket.as_deref().unwrap_or("-")
            );
            eprintln!(
                "  Prefix:       {}",
                if status.prefix.is_empty() {
                    "(none)"
                } else {
                    &status.prefix
                }
            );
            eprintln!();
            eprintln!("  Reachable:    {}", check(status.reachable));
            eprintln!("  Readable:     {}", check(status.readable));
            eprintln!("  Writable:     {}", check(status.writable));
            eprintln!("  Object Lock:  {}", status.object_lock);
            eprintln!();
            eprintln!("  Bundles:      {}", status.bundle_count);
            eprintln!("  Total size:   {}", format_size(status.total_size_bytes));

            if !status.reachable {
                eprintln!();
                eprintln!("Store is not reachable. Check your URL and credentials.");
            }
        }
    }

    if status.reachable {
        Ok(0)
    } else {
        Ok(1)
    }
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
