use crate::cli::args::{CoverageArgs, CoverageFormat};
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

impl CoverageOutputFormat {
    /// What `--input` mode makes of a spelling. It has no text output, so `text` names the
    /// canonical JSON report here; that is why the argument's shared default is accepted at all.
    pub(crate) fn narrow(format: CoverageFormat) -> Self {
        match format {
            CoverageFormat::Text | CoverageFormat::Json => Self::Json,
            CoverageFormat::Md | CoverageFormat::Markdown => Self::Markdown,
        }
    }
}

/// The shapes legacy mode can print.
///
/// Narrowed from `CoverageFormat` before the renderer is reached, so no arm downstream has to
/// answer for a spelling this mode does not honour. Legacy has a text output that `--input` mode
/// lacks, which is the asymmetry that made one shared set unable to describe both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyOutputFormat {
    Text,
    Json,
    Markdown,
}

impl LegacyOutputFormat {
    /// `Err` carries the message for the one spelling legacy mode does not honour. It used to fall
    /// through a `_` arm and print text, which is the defect this narrowing removes.
    pub(crate) fn narrow(format: CoverageFormat) -> std::result::Result<Self, &'static str> {
        match format {
            CoverageFormat::Text => Ok(Self::Text),
            CoverageFormat::Json => Ok(Self::Json),
            CoverageFormat::Markdown => Ok(Self::Markdown),
            CoverageFormat::Md => {
                Err("--format md is only supported with --input mode; use --format markdown")
            }
        }
    }
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

    // Both mode rules sit here, and both refuse an argument this mode cannot honour instead of
    // reinterpreting it.
    let output = match LegacyOutputFormat::narrow(args.format) {
        Ok(output) => output,
        Err(message) => {
            eprintln!("Measurement error: {message}");
            return Ok(crate::exit_codes::EXIT_CONFIG_ERROR);
        }
    };

    legacy::cmd_coverage_legacy(args, output).await
}
