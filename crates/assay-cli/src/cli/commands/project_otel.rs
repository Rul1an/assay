//! `assay project-otel` — read-only projection of assay evidence into the OTel GenAI + OpenInference
//! view.
//!
//! Guardrail: this command must never be smarter than the library projector. It reads files,
//! deserializes JSON, calls `assay_core::otel::projection::project`, and writes the result. All
//! projection semantics live in `assay_core`, so there is exactly one projection truth — never a
//! second, divergent CLI projection.

use std::path::Path;

use serde_json::Value;

use crate::cli::args::ProjectOtelArgs;
use crate::exit_codes::{EXIT_CONFIG_ERROR, EXIT_SUCCESS};

fn read_json(path: &Path) -> anyhow::Result<Value> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|e| anyhow::anyhow!("invalid JSON in {}: {e}", path.display()))
}

/// Project the supplied artifacts. Input/IO errors are reported on stderr and return
/// `EXIT_CONFIG_ERROR`, leaving stdout empty; on success the projection is the only thing written to
/// stdout (or to `--out`), as pure JSON.
pub fn run(args: ProjectOtelArgs) -> anyhow::Result<i32> {
    let capability_surface = match read_json(&args.capability_surface) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {e}");
            return Ok(EXIT_CONFIG_ERROR);
        }
    };
    let observation_health = match args
        .observation_health
        .as_deref()
        .map(read_json)
        .transpose()
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {e}");
            return Ok(EXIT_CONFIG_ERROR);
        }
    };
    let enforcement_health = match args
        .enforcement_health
        .as_deref()
        .map(read_json)
        .transpose()
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {e}");
            return Ok(EXIT_CONFIG_ERROR);
        }
    };

    let projection = assay_core::otel::projection::project(
        &capability_surface,
        observation_health.as_ref(),
        enforcement_health.as_ref(),
    );
    let json = serde_json::to_string_pretty(&projection)?;

    match &args.out {
        Some(path) => {
            if let Err(e) = std::fs::write(path, format!("{json}\n")) {
                eprintln!("error: cannot write {}: {e}", path.display());
                return Ok(EXIT_CONFIG_ERROR);
            }
        }
        None => println!("{json}"),
    }
    Ok(EXIT_SUCCESS)
}
