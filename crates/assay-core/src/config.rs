use crate::errors::ConfigError;
use crate::model::EvalConfig;
use std::path::Path;

pub mod otel;
pub mod path_resolver;
pub mod resolve;

pub const SUPPORTED_CONFIG_VERSION: u32 = 1;

/// What a caller wants the loader to refuse, rather than merely parse.
///
/// Each field is a separate axis on purpose. `strict_unknown_fields` and
/// `deny_ineffective_assertions` refuse different things — a key the schema does not know versus an
/// assertion that no trace could ever fail — and folding them into one flag would mean a caller who
/// wanted one silently acquired the other.
#[derive(Debug, Clone, Copy, Default)]
pub struct LoadOptions {
    /// Treat a v0 config as v0 rather than trusting its declared version.
    pub legacy_mode: bool,
    /// Refuse a config carrying keys this version does not understand.
    pub strict_unknown_fields: bool,
    /// Refuse a config whose `assertions:` include one that cannot fail.
    ///
    /// Opt-in, and deliberately not on by default: `assay validate` has reported these as a
    /// warning since #1983, and turning that into a load-time error for every caller at once would
    /// break suites that are running today. The phased route in #1949 is warning, then opt-in here,
    /// then default at a major after an announced window.
    pub deny_ineffective_assertions: bool,
}

/// Backwards-compatible loader. Every existing caller keeps its behaviour, and the new refusal is
/// reachable only through [`load_config_with`], which is what makes it opt-in.
pub fn load_config(
    path: &Path,
    legacy_mode: bool,
    strict: bool,
) -> Result<EvalConfig, ConfigError> {
    load_config_with(
        path,
        LoadOptions {
            legacy_mode,
            strict_unknown_fields: strict,
            ..Default::default()
        },
    )
}

pub fn load_config_with(path: &Path, opts: LoadOptions) -> Result<EvalConfig, ConfigError> {
    let LoadOptions {
        legacy_mode,
        strict_unknown_fields: strict,
        deny_ineffective_assertions,
    } = opts;
    let raw = std::fs::read_to_string(path)
        .map_err(|e| ConfigError(format!("failed to read config {}: {}", path.display(), e)))?;

    let mut ignored_keys = std::collections::HashSet::new();
    let deserializer = serde_yaml::Deserializer::from_str(&raw);

    // serde_ignored wrapper to capture unknown fields
    let mut cfg: EvalConfig = serde_ignored::deserialize(deserializer, |path| {
        ignored_keys.insert(path.to_string());
    })
    .map_err(|e| ConfigError(format!("failed to parse YAML: {}", e)))?;

    // Check strictness / significant unknown fields
    if strict && !ignored_keys.is_empty() {
        // Whitelist common YAML anchor keys
        let meaningful_unknowns: Vec<_> = ignored_keys
            .iter()
            .filter(|k| *k != "definitions" && !k.starts_with("_") && !k.starts_with("x-"))
            .collect();

        if meaningful_unknowns.is_empty() {
            // All unknowns are whitelisted (e.g. anchors). PASS.
        } else {
            // Special helpful error for v0 'policies'
            if ignored_keys.contains("policies") {
                return Err(ConfigError(format!(
                    "Top-level 'policies' is not valid in configVersion: {}. Did you mean to run assay migrate on a v0 config, or remove legacy keys? (file: {})",
                    cfg.version,
                    path.display()
                )));
            }

            // Generic strict error
            return Err(ConfigError(format!(
                "Unknown fields detected in strict mode: {:?} (file: {})",
                meaningful_unknowns,
                path.display()
            )));
        }
    } else if !ignored_keys.is_empty() {
        // In non-strict mode, we ideally WARN, but standard logging might not be initialized here.
        // For now, we proceed as 'careful ignore' but validated at least.
        // The user specifically asked for migrate FAIL (strict=true) and run WARN.
        eprintln!("WARN: Ignored unknown config fields: {:?}", ignored_keys);
    }

    // Legacy override
    if legacy_mode {
        cfg.version = 0;
    }

    // Allow 0 or 1
    if cfg.version != 0 && cfg.version != SUPPORTED_CONFIG_VERSION {
        return Err(ConfigError(format!(
            "unsupported config version {} (supported: 0, {})",
            cfg.version, SUPPORTED_CONFIG_VERSION
        )));
    }

    if cfg.tests.is_empty() {
        return Err(ConfigError("config has no tests".into()));
    }

    // Fail closed before execution rather than after. An assertion that cannot fail reports a pass
    // carrying no information, and a run that reaches it has already spent the time and money to
    // produce that non-answer. The decision itself is `validate::ineffective_assertions`, which is
    // the same code `assay validate` sweeps with, so this cannot drift away from what the warning
    // says. Diagnostics stay value-free: they name the test, the index, the variant and the
    // responsible field, never the configured value.
    if deny_ineffective_assertions {
        let ineffective = crate::validate::ineffective_assertions(&cfg);
        if !ineffective.is_empty() {
            let detail = ineffective
                .iter()
                .map(|d| {
                    let test = d
                        .context
                        .get("test_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    let index = d
                        .context
                        .get("assertion_index")
                        .and_then(|v| v.as_u64())
                        .map(|i| i.to_string())
                        .unwrap_or_else(|| "?".into());
                    format!("test '{}' assertion {}: {}", test, index, d.message)
                })
                .collect::<Vec<_>>()
                .join("; ");
            return Err(ConfigError(format!(
                "{} assertion(s) cannot fail and were refused because --deny-ineffective-assertions is set ({}): {}",
                ineffective.len(),
                path.display(),
                detail
            )));
        }
    }

    normalize_paths(&mut cfg, path)
        .map_err(|e| ConfigError(format!("failed to normalize config paths: {}", e)))?;

    Ok(cfg)
}

fn normalize_paths(cfg: &mut EvalConfig, config_path: &Path) -> anyhow::Result<()> {
    let r = path_resolver::PathResolver::new(config_path);

    for tc in &mut cfg.tests {
        if let crate::model::Expected::JsonSchema { schema_file, .. } = &mut tc.expected {
            if let Some(orig) = schema_file.clone() {
                let before = orig.clone();
                r.resolve_opt_str(schema_file);

                if let Some(resolved) = schema_file.as_ref() {
                    if *resolved != before {
                        let meta = tc.metadata.get_or_insert_with(|| serde_json::json!({}));
                        if !meta.get("assay").is_some_and(|v| v.is_object()) {
                            meta["assay"] = serde_json::json!({});
                        }

                        meta["assay"]["schema_file_original"] = serde_json::json!(before);
                        meta["assay"]["schema_file_resolved"] = serde_json::json!(resolved);
                        meta["assay"]["config_dir"] = serde_json::json!(config_path
                            .parent()
                            .unwrap_or(Path::new("."))
                            .to_string_lossy());
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn write_sample_config(path: &Path) -> Result<(), ConfigError> {
    std::fs::write(
        path,
        r#"version: 1
suite: demo
model: dummy
settings:
  parallel: 4
  timeout_seconds: 30
  cache: true
tests:
  - id: t1_must_contain
    tags: ["smoke"]
    input:
      prompt: "Say hello and mention Amsterdam."
    expected:
      type: must_contain
      must_contain: ["hello", "Amsterdam"]
  - id: t2_must_not_contain
    tags: ["smoke"]
    input:
      prompt: "Write a sentence without the word banana."
    expected:
      type: must_not_contain
      must_not_contain: ["banana"]
"#,
    )
    .map_err(|e| ConfigError(format!("failed to write sample config: {}", e)))?;
    Ok(())
}
