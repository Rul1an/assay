use crate::errors::{ConfigError, ConfigLoadError};
use crate::model::EvalConfig;
use crate::render_safety::{render_safe, Sink, MAX_RENDER_FIELD};
use std::path::Path;

pub mod otel;
pub mod path_resolver;
pub mod resolve;

pub const SUPPORTED_CONFIG_VERSION: u32 = 1;

/// What a caller wants the loader to refuse, rather than merely parse.
///
/// Each field is a separate axis on purpose. `strict_unknown_fields` and
/// `allow_ineffective_assertions` decide different things — a key the schema does not know versus
/// an assertion that no trace could ever fail — and folding them into one flag would mean a caller
/// who wanted one silently acquired the other.
#[derive(Debug, Clone, Copy, Default)]
pub struct LoadOptions {
    /// Treat a v0 config as v0 rather than trusting its declared version.
    pub legacy_mode: bool,
    /// Refuse a config carrying keys this version does not understand.
    pub strict_unknown_fields: bool,
    /// Accept a config whose `assertions:` include one that cannot fail.
    ///
    /// **Refusing is the default, and this is the escape hatch.** The phased route in #1949 was
    /// warning, then opt-in, then default at a major: `assay validate` has warned since #1983, the
    /// opt-in landed as `--deny-ineffective-assertions`, and 5.0.0 is the major that carries the
    /// flip (#1949).
    ///
    /// The polarity is inverted rather than the default being overridden, so that
    /// `#[derive(Default)]` still produces the intended behaviour. A `deny_*` field defaulting to
    /// `true` needs a hand-written `Default`, and every `..Default::default()` in the tree would
    /// then depend on that impl being right. `false` meaning "do not allow" is the same fact with
    /// nothing to keep in sync.
    pub allow_ineffective_assertions: bool,
}

/// Convenience loader for the two older axes.
///
/// It no longer preserves the pre-5.0.0 behaviour and is not meant to: `..Default::default()` now
/// carries the ineffective-assertion refusal, so a caller on this path gets it too. That is the
/// point of the flip in #1949 rather than an oversight, and a caller who needs the old behaviour
/// asks for it through [`load_config_with`] with `allow_ineffective_assertions: true`.
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
    load_config_with_cause(path, opts).map_err(ConfigLoadError::into_config_error)
}

/// The one config read. Public [`load_config_with`] drops the I/O kind.
pub fn load_config_with_cause(
    path: &Path,
    opts: LoadOptions,
) -> Result<EvalConfig, ConfigLoadError> {
    let LoadOptions {
        legacy_mode,
        strict_unknown_fields: strict,
        allow_ineffective_assertions,
    } = opts;
    let raw = std::fs::read_to_string(path).map_err(|e| {
        ConfigLoadError::from_read(
            format!("failed to read config {}: {}", path.display(), e),
            e.kind(),
        )
    })?;

    let mut ignored_keys = std::collections::HashSet::new();
    let deserializer = serde_yaml::Deserializer::from_str(&raw);

    // serde_ignored wrapper to capture unknown fields
    let mut cfg: EvalConfig = serde_ignored::deserialize(deserializer, |path| {
        ignored_keys.insert(path.to_string());
    })
    // Parse Display can quote the offending scalar. Bound that excerpt through
    // the existing render-safety pipeline (redact-before-truncate). This is not
    // a read ceiling, and it does not claim every YAML error echoes input.
    .map_err(|e| {
        ConfigLoadError::new(format!(
            "failed to parse YAML: {}",
            render_safe(Sink::Json, &e.to_string(), MAX_RENDER_FIELD)
        ))
    })?;

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
                return Err(ConfigLoadError::new(format!(
                    "Top-level 'policies' is not valid in configVersion: {}. Did you mean to run assay migrate on a v0 config, or remove legacy keys? (file: {})",
                    cfg.version,
                    path.display()
                )));
            }

            // Generic strict error
            return Err(ConfigLoadError::new(format!(
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
        return Err(ConfigLoadError::new(format!(
            "unsupported config version {} (supported: 0, {})",
            cfg.version, SUPPORTED_CONFIG_VERSION
        )));
    }

    if cfg.tests.is_empty() {
        return Err(ConfigLoadError::new("config has no tests"));
    }

    // Fail closed before execution rather than after. An assertion that cannot fail reports a pass
    // carrying no information, and a run that reaches it has already spent the time and money to
    // produce that non-answer. The decision itself is `validate::ineffective_assertions`, which is
    // the same code `assay validate` sweeps with, so this cannot drift away from what the warning
    // says. Diagnostics stay value-free: they name the test, the index, the variant and the
    // responsible field, never the configured value.
    if !allow_ineffective_assertions {
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
            return Err(ConfigLoadError::new(format!(
                "{} assertion(s) cannot fail and were refused ({}): {} \
                 An assertion that cannot fail reports a pass carrying no information. \
                 Fix the assertion, or pass --allow-ineffective-assertions to run anyway.",
                ineffective.len(),
                path.display(),
                detail
            )));
        }
    }

    normalize_paths(&mut cfg, path)
        .map_err(|e| ConfigLoadError::new(format!("failed to normalize config paths: {}", e)))?;

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

#[cfg(test)]
mod tests {
    use super::{load_config_with, load_config_with_cause};
    use std::io::ErrorKind;
    use std::path::Path;

    #[test]
    fn absent_config_read_preserves_not_found_kind() {
        let path = Path::new("definitely-absent-2206.yaml");
        let typed = load_config_with_cause(path, Default::default())
            .expect_err("absent path must fail the read");
        assert_eq!(typed.io_kind(), Some(ErrorKind::NotFound));
        assert!(
            typed.to_string().contains("failed to read config"),
            "Display must stay the read-failure string: {typed}"
        );

        let public = load_config_with(path, Default::default()).expect_err("absent path");
        assert_eq!(typed.to_string(), public.to_string());
        assert_eq!(typed.into_config_error().0, public.0);
    }

    /// github-token shape from `render_safety/rules.rs` (`ghp_` + 36 alphanumerics).
    /// A space then padding keeps the extra bytes outside that rule so redaction
    /// cannot swallow the whole scalar (the `{36,}` quantifier is open-ended).
    fn long_secret_yaml() -> (String, String) {
        let token = format!("ghp_{}", "A".repeat(36));
        let yaml = format!("version: \"{token} {}\"\n", "x".repeat(300));
        (token, yaml)
    }

    #[test]
    fn malformed_yaml_does_not_invent_not_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("bad.yaml");
        std::fs::write(&path, "version: [\n").expect("write malformed yaml");
        let err = load_config_with_cause(&path, Default::default()).expect_err("malformed yaml");
        assert_eq!(
            err.io_kind(),
            None,
            "YAML failures must not carry a read I/O kind: {err}"
        );
        assert!(
            err.to_string().contains("failed to parse YAML"),
            "concise malformed YAML must stay diagnosed: {err}"
        );
        let public = load_config_with(&path, Default::default()).expect_err("malformed yaml");
        assert_eq!(err.to_string(), public.to_string());
    }

    /// The YAML parse-Display fold is the one ceiling: prefix plus a render-safe
    /// excerpt. Not a read ceiling, not a claim that every YAML error echoes input.
    #[test]
    fn yaml_parse_error_is_redacted_and_bounded() {
        use crate::render_safety::MAX_RENDER_FIELD;

        let (token, yaml) = long_secret_yaml();
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("secret.yaml");
        std::fs::write(&path, &yaml).expect("write long-secret yaml");
        let err = load_config_with_cause(&path, Default::default())
            .expect_err("long secret scalar must fail YAML parse");
        let message = err.to_string();
        assert!(
            message.contains("failed to parse YAML"),
            "parse failures must stay diagnosed: {message}"
        );
        assert!(
            !message.contains(&token),
            "raw credential must not survive the parse-Display fold: {message}"
        );
        assert!(
            message.contains("<redacted:"),
            "redaction placeholder must be visible: {message}"
        );
        assert!(
            message.contains("(truncated)"),
            "truncation must be visible: {message}"
        );
        assert!(
            message.chars().count() <= MAX_RENDER_FIELD + 80,
            "ConfigLoadError is prefix + bounded excerpt, not the full scalar ({} chars): {message}",
            message.chars().count()
        );
        assert!(
            message.len() < yaml.len(),
            "bounded diagnostic must be shorter than the fixture that produced it"
        );
        assert_eq!(
            err.io_kind(),
            None,
            "parse fold must not invent a read kind"
        );
    }
}
