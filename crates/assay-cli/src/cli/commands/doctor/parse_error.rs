use std::path::Path;

use anyhow::Context;
use assay_core::config::load_config;
use assay_core::errors::similarity::closest_prompt;
use dialoguer::{theme::ColorfulTheme, Confirm};

use crate::cli::args::DoctorArgs;

use super::patching::{print_unified_diff, write_text_file};

pub(super) fn try_fix_parse_error(
    args: &DoctorArgs,
    config_path: &Path,
    err: &str,
    legacy_mode: bool,
) -> anyhow::Result<i32> {
    let (unknown, candidates) = parse_unknown_field_error(err)
        .map(|(u, c)| (u.to_string(), c))
        .unwrap_or_else(|| (String::new(), Vec::new()));

    if unknown.is_empty() || candidates.is_empty() {
        println!("No auto-fixable config parse issue detected.");
        return Ok(1);
    }

    let replacement = closest_prompt(&unknown, candidates.iter()).and_then(|m| {
        if m.similarity >= 0.80 {
            Some(m.prompt)
        } else {
            None
        }
    });

    let Some(replacement) = replacement else {
        println!(
            "No safe replacement found for '{}'. Try fixing the key manually.",
            unknown
        );
        return Ok(1);
    };

    let do_apply = if args.yes || args.dry_run {
        true
    } else {
        Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "Replace key '{}' with '{}' in {}?",
                unknown,
                replacement,
                config_path.display()
            ))
            .default(false)
            .interact()
            .unwrap_or(false)
    };

    if !do_apply {
        println!("No fixes applied.");
        return Ok(1);
    }

    let before = std::fs::read_to_string(config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;
    let Some(after) = replace_yaml_key(&before, &unknown, &replacement) else {
        println!(
            "Could not find YAML key '{}' to replace in {}.",
            unknown,
            config_path.display()
        );
        return Ok(1);
    };

    if args.dry_run {
        print_unified_diff(
            &config_path.display().to_string(),
            "rename_config_key",
            &before,
            &after,
        );
        println!("Dry run complete. 1 fix(es) previewed.");
        return Ok(1);
    }

    write_text_file(config_path, &after)
        .with_context(|| format!("failed to write {}", config_path.display()))?;
    println!(
        "Applied: replaced '{}' with '{}' in {}",
        unknown,
        replacement,
        config_path.display()
    );

    match load_config(config_path, legacy_mode, false) {
        Ok(_) => {
            println!("Config parses successfully after fix.");
            Ok(0)
        }
        Err(e) => {
            println!("Config still has issues after fix: {}", e);
            Ok(1)
        }
    }
}

fn parse_unknown_field_error(err: &str) -> Option<(&str, Vec<String>)> {
    let unknown = err
        .split("unknown field `")
        .nth(1)?
        .split('`')
        .next()
        .filter(|s| !s.is_empty())?;

    let expected = err.split("expected one of").nth(1)?;
    let mut candidates = extract_backticked_tokens(expected);
    if candidates.is_empty() {
        let expected = expected.split(" at line").next().unwrap_or(expected);
        candidates = expected
            .split(',')
            .map(|s| s.trim().trim_matches('`').trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    if candidates.is_empty() {
        None
    } else {
        Some((unknown, candidates))
    }
}

fn extract_backticked_tokens(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = input;
    while let Some(start) = rest.find('`') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('`') else {
            break;
        };
        let candidate = rest[..end].trim();
        if !candidate.is_empty() {
            out.push(candidate.to_string());
        }
        rest = &rest[end + 1..];
    }
    out
}

fn replace_yaml_key(content: &str, from: &str, to: &str) -> Option<String> {
    let mut changed = false;
    let mut out = String::with_capacity(content.len());

    for line in content.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix(&format!("{}:", from)) {
            let indent = &line[..line.len() - trimmed.len()];
            out.push_str(indent);
            out.push_str(to);
            out.push(':');
            out.push_str(rest);
            out.push('\n');
            changed = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }

    if changed {
        Some(out)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_unknown_field_error, replace_yaml_key};

    #[test]
    fn parse_unknown_field_extracts_candidates() {
        let err = "unknown field `response_format`, expected one of `format`, `out`, `trace_file`";
        let (unknown, candidates) = parse_unknown_field_error(err).expect("parsed");
        assert_eq!(unknown, "response_format");
        assert!(candidates.iter().any(|c| c == "format"));
    }

    #[test]
    fn parse_unknown_field_ignores_line_column_suffix() {
        let err = "unknown field `response_format`, expected one of `format`, `out`, `trace_file` at line 4 column 3";
        let (unknown, candidates) = parse_unknown_field_error(err).expect("parsed");
        assert_eq!(unknown, "response_format");
        assert!(candidates.iter().any(|c| c == "trace_file"));
        assert!(candidates.iter().all(|c| !c.contains("line")));
    }

    #[test]
    fn replace_yaml_key_rewrites_key() {
        let input = "version: 1\nresponse_format: text\n";
        let out = replace_yaml_key(input, "response_format", "format").expect("replacement");
        assert!(out.contains("format: text"));
        assert!(!out.contains("response_format: text"));
    }
}
