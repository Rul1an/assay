use crate::cli::args::CoverageArgs;
use anyhow::Result;
use std::path::Path;

mod format_md;
mod generate;
mod io;
mod legacy;
mod report;
mod schema;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoverageOutputFormat {
    Json,
    Markdown,
}

const DEFAULT_ROUTES_TOP: usize = 10;

pub(crate) async fn write_generated_coverage_report(
    input: &Path,
    out: &Path,
    declared_tools: &[String],
    source: &str,
) -> Result<i32> {
    write_generated_coverage_report_with_format(
        input,
        out,
        declared_tools,
        source,
        CoverageOutputFormat::Json,
        DEFAULT_ROUTES_TOP,
    )
    .await
}

pub(crate) async fn write_generated_coverage_report_with_format(
    input: &Path,
    out: &Path,
    declared_tools: &[String],
    source: &str,
    format: CoverageOutputFormat,
    routes_top: usize,
) -> Result<i32> {
    generate::write_generated_coverage_report_with_format(
        input,
        out,
        declared_tools,
        source,
        format,
        routes_top,
    )
    .await
}

pub async fn cmd_coverage(args: CoverageArgs) -> Result<i32> {
    if args.input.is_none() && args.out_md.is_some() {
        eprintln!("Measurement error: --out-md is only supported with --input mode");
        return Ok(crate::exit_codes::EXIT_CONFIG_ERROR);
    }

    if args.input.is_some() {
        return generate::cmd_coverage_generate(&args).await;
    }

    legacy::cmd_coverage_legacy(args).await
}
