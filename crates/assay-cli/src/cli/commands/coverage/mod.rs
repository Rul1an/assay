use crate::cli::args::{CoverageArgs, CoverageFormat};
use crate::cli_failure::CliFailure;
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
    /// What this mode is asked for by default when `--format` is absent.
    pub(crate) const DEFAULT: CoverageFormat = CoverageFormat::Json;

    /// `Err` carries the message for the one spelling this mode does not honour.
    ///
    /// `text` used to be accepted here and silently mean the canonical JSON report. That was not a
    /// chosen alias: the argument's shared default was `text`, so the mode had to accept it, and
    /// meaning JSON by it was the only thing left to do. The default now belongs to the mode, so
    /// the spelling can be refused the way `md` already is in legacy mode.
    pub(crate) fn narrow(format: CoverageFormat) -> std::result::Result<Self, &'static str> {
        match format {
            CoverageFormat::Json => Ok(Self::Json),
            CoverageFormat::Md | CoverageFormat::Markdown => Ok(Self::Markdown),
            CoverageFormat::Text => Err(
                "--format text is only supported without --input mode; --input mode writes \
                     json or md",
            ),
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
    /// What this mode is asked for by default when `--format` is absent.
    pub(crate) const DEFAULT: CoverageFormat = CoverageFormat::Text;

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
        return Err(CliFailure::coverage_invalid_args(
            "--out-md is only supported with --input mode",
        )
        .into());
    }

    // Both mode rules sit here: each applies its own default and each refuses by name the spelling
    // it cannot honour, rather than reinterpreting it. Neither mode accepts a value only because
    // the other mode's default requires it.
    if args.input.is_some() {
        let requested = args.format.unwrap_or(CoverageOutputFormat::DEFAULT);
        let output = match CoverageOutputFormat::narrow(requested) {
            Ok(output) => output,
            Err(message) => {
                eprintln!("Measurement error: {message}");
                return Ok(crate::exit_codes::EXIT_CONFIG_ERROR);
            }
        };
        return generate::cmd_coverage_generate(&args, output).await;
    }

    let requested = args.format.unwrap_or(LegacyOutputFormat::DEFAULT);
    let output = match LegacyOutputFormat::narrow(requested) {
        Ok(output) => output,
        Err(message) => {
            eprintln!("Measurement error: {message}");
            return Ok(crate::exit_codes::EXIT_CONFIG_ERROR);
        }
    };

    legacy::cmd_coverage_legacy(args, output).await
}
