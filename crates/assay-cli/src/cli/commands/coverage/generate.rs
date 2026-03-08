use super::io;
use super::{report, schema, CoverageOutputFormat};
use crate::cli::args::CoverageArgs;
use crate::exit_codes;
use anyhow::Result;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

pub(super) async fn write_generated_coverage_report_with_format(
    input: &Path,
    out: &Path,
    declared_tools: &[String],
    source: &str,
    format: CoverageOutputFormat,
    routes_top: usize,
) -> Result<i32> {
    let report_value =
        match build_and_validate_generated_coverage_report(input, declared_tools, source).await {
            Ok(v) => v,
            Err(code) => return Ok(code),
        };

    io::write_generated_coverage_payload(out, &report_value, format, routes_top).await
}

async fn build_and_validate_generated_coverage_report(
    input: &Path,
    declared_tools: &[String],
    source: &str,
) -> std::result::Result<Value, i32> {
    use crate::exit_codes::EXIT_CONFIG_ERROR;

    let report_value =
        match report::build_coverage_report_from_input(input, declared_tools, source).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Measurement error: {e}");
                return Err(EXIT_CONFIG_ERROR);
            }
        };

    if let Err(e) = schema::validate_coverage_report_v1(&report_value) {
        eprintln!("Measurement error: coverage report schema validation failed: {e}");
        return Err(EXIT_CONFIG_ERROR);
    }

    Ok(report_value)
}

pub(super) async fn cmd_coverage_generate(args: &CoverageArgs) -> Result<i32> {
    use crate::exit_codes::EXIT_CONFIG_ERROR;

    if args.declared_tools.iter().any(|t| t.trim().is_empty()) {
        eprintln!("Measurement error: --declared-tool must not be empty");
        return Ok(EXIT_CONFIG_ERROR);
    }

    if args.trace_file.is_some() {
        eprintln!("Measurement error: --input and --trace-file/--traces cannot be used together");
        return Ok(EXIT_CONFIG_ERROR);
    }

    let input = args
        .input
        .as_ref()
        .expect("input mode already checked to be present");
    let out = match args.out.as_ref() {
        Some(out) => out,
        None => {
            eprintln!("Measurement error: --out is required when --input is used");
            return Ok(EXIT_CONFIG_ERROR);
        }
    };

    let declared_tools = match load_declared_tools(args).await {
        Ok(v) => v,
        Err(code) => return Ok(code),
    };

    let output_format = match parse_generate_output_format(&args.format) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Measurement error: {e}");
            return Ok(EXIT_CONFIG_ERROR);
        }
    };

    let primary_format = if args.out_md.is_some() {
        CoverageOutputFormat::Json
    } else {
        output_format
    };

    let status = write_generated_coverage_report_with_format(
        input,
        out,
        &declared_tools,
        "jsonl",
        primary_format,
        args.routes_top,
    )
    .await?;

    if status != exit_codes::EXIT_SUCCESS {
        return Ok(status);
    }

    if let Some(out_md) = args.out_md.as_ref() {
        return write_generated_coverage_report_with_format(
            input,
            out_md,
            &declared_tools,
            "jsonl",
            CoverageOutputFormat::Markdown,
            args.routes_top,
        )
        .await;
    }

    Ok(status)
}

async fn load_declared_tools(args: &CoverageArgs) -> std::result::Result<Vec<String>, i32> {
    use crate::exit_codes::EXIT_CONFIG_ERROR;

    let mut declared = BTreeSet::new();

    for raw in &args.declared_tools {
        let tool = raw.trim();
        if tool.is_empty() {
            eprintln!("Measurement error: --declared-tool must not be empty");
            return Err(EXIT_CONFIG_ERROR);
        }
        declared.insert(tool.to_string());
    }

    if let Some(path) = args.declared_tools_file.as_ref() {
        let content = match tokio::fs::read_to_string(path).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!(
                    "Measurement error: failed to read --declared-tools-file {}: {e}",
                    path.display()
                );
                return Err(EXIT_CONFIG_ERROR);
            }
        };

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            declared.insert(line.to_string());
        }
    }

    Ok(declared.into_iter().collect())
}

fn parse_generate_output_format(raw: &str) -> std::result::Result<CoverageOutputFormat, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "json" | "text" => Ok(CoverageOutputFormat::Json),
        "md" | "markdown" | "github" => Ok(CoverageOutputFormat::Markdown),
        other => Err(format!(
            "--format must be one of: json|md for --input mode (got '{other}')"
        )),
    }
}
