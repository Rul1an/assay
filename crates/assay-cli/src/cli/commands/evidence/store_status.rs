//! `assay evidence store-status` - Check evidence store connectivity and status.

use anyhow::Result;
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
    let url = match resolve_store_url(args.store.as_deref(), args.store_config.as_deref()) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("Config error: {}", e);
            return Ok(2);
        }
    };

    let spec = match StoreSpec::parse(&url) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Config error: invalid store URL '{}': {}", url, e);
            return Ok(2);
        }
    };

    let store = match ObjectStoreBundleStore::from_spec(&spec).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Connection error: {}", e);
            return Ok(1);
        }
    };

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

            println!("Evidence Store Status");
            println!("====================");
            println!();
            println!("  Backend:      {}", status.backend);
            println!(
                "  Bucket:       {}",
                status.bucket.as_deref().unwrap_or("-")
            );
            println!(
                "  Prefix:       {}",
                if status.prefix.is_empty() {
                    "(none)"
                } else {
                    &status.prefix
                }
            );
            println!();
            println!("  Reachable:    {}", check(status.reachable));
            println!("  Readable:     {}", check(status.readable));
            println!("  Writable:     {}", check(status.writable));
            println!("  Object Lock:  {}", status.object_lock);
            println!();
            println!("  Bundles:      {}", status.bundle_count);
            println!("  Total size:   {}", format_size(status.total_size_bytes));

            if !status.reachable {
                eprintln!();
                eprintln!("Store is not reachable. Check your URL and credentials.");
            }
        }
    }

    let usable = status.reachable && status.readable && status.writable;
    if usable {
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
