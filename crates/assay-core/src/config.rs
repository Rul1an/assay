use crate::errors::{ConfigError, ConfigLoadError};
use crate::model::EvalConfig;
use crate::render_safety::{render_safe, Sink, MAX_RENDER_FIELD};
use std::fs::{File, Metadata, OpenOptions};
use std::io::Read;
use std::path::Path;

pub mod otel;
pub mod path_resolver;
pub mod resolve;

pub const SUPPORTED_CONFIG_VERSION: u32 = 1;

/// Inclusive byte ceiling for the one config-read seam. A caller config is not
/// hostile ingest (not ADR-043); this is robustness so a 256 MiB `--config`
/// is not fully materialized. Stated once; `doctor`, `run` and `validate`
/// share [`load_config_with_cause`].
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

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
    let raw = read_config_source(path)?;

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
            render_yaml_parse_error(&e)
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

/// The one config-read rule: open, inspect the opened file, refuse a
/// non-regular path, then apply the inclusive byte ceiling before YAML.
///
/// `metadata.len()` is only an early rejection. The read itself is bounded
/// to `MAX_CONFIG_BYTES + 1` so a regular file that grows after stat cannot
/// bypass the ceiling. Exact `MAX_CONFIG_BYTES` is allowed. `O_NONBLOCK` is
/// Unix-only so a FIFO fails on the file-type check instead of hanging in
/// `open`. Not ADR-043 and not a second limit.
fn read_config_source(path: &Path) -> Result<String, ConfigLoadError> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NONBLOCK);
    }
    // Intentional caller-selected config path, read-only, opened-file
    // regular-type check, bounded before parse.
    // codeql[rust/path-injection]
    let file = opts.open(path).map_err(|e| {
        ConfigLoadError::from_read(
            format!("failed to read config {}: {}", path.display(), e),
            e.kind(),
        )
    })?;
    let meta = file.metadata().map_err(|e| {
        ConfigLoadError::from_read(
            format!("failed to read config {}: {}", path.display(), e),
            e.kind(),
        )
    })?;
    read_opened_config(path, file, meta)
}

/// File-type, early-len, and the load-bearing `take(MAX+1)` bound. Production
/// passes freshly-stated metadata; tests may pass stale metadata to prove a
/// file that grew after stat is still refused.
fn read_opened_config(path: &Path, file: File, meta: Metadata) -> Result<String, ConfigLoadError> {
    if !meta.file_type().is_file() {
        return Err(ConfigLoadError::new(format!(
            "config {} is not a regular file",
            path.display()
        )));
    }
    if meta.len() > MAX_CONFIG_BYTES {
        return Err(ConfigLoadError::new(format!(
            "config {} exceeds the {MAX_CONFIG_BYTES}-byte ceiling",
            path.display()
        )));
    }
    let mut raw = String::new();
    file.take(MAX_CONFIG_BYTES + 1)
        .read_to_string(&mut raw)
        .map_err(|e| {
            ConfigLoadError::from_read(
                format!("failed to read config {}: {}", path.display(), e),
                e.kind(),
            )
        })?;
    if (raw.len() as u64) > MAX_CONFIG_BYTES {
        return Err(ConfigLoadError::new(format!(
            "config {} exceeds the {MAX_CONFIG_BYTES}-byte ceiling",
            path.display()
        )));
    }
    Ok(raw)
}

/// Bound a `serde_yaml::Error` Display through `render_safe`, but keep the
/// `location()` mark inside the same `MAX_RENDER_FIELD` budget. Truncating the
/// whole Display would drop a trailing `at line N column M`. Reserve and
/// reattach that mark only when it is a real suffix. Libyaml Displays put
/// context after the problem-mark (`… at line N column M, while scanning …`);
/// `strip_suffix` is then `None` and the full Display goes through
/// `render_safe` once — no second append. `Sink::Json` is identity today
/// (M5); this is not a second sanitizer and does not raise the budget.
fn render_yaml_parse_error(err: &serde_yaml::Error) -> String {
    let display = err.to_string();
    let Some(loc) = err.location() else {
        return render_safe(Sink::Json, &display, MAX_RENDER_FIELD);
    };
    let mark = format!(" at line {} column {}", loc.line(), loc.column());
    match display.strip_suffix(mark.as_str()) {
        Some(diagnosis) => {
            let reserved = mark.chars().count();
            let budget = MAX_RENDER_FIELD.saturating_sub(reserved);
            let safe = render_safe(Sink::Json, diagnosis, budget);
            format!("{safe}{mark}")
        }
        None => render_safe(Sink::Json, &display, MAX_RENDER_FIELD),
    }
}

#[cfg(test)]
mod tests {
    use super::{load_config_with, load_config_with_cause, read_opened_config, MAX_CONFIG_BYTES};
    use std::fs::OpenOptions;
    use std::io::{ErrorKind, Write};
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

    /// serde_yaml Display's trailing mark, from `Error::location()`, not from
    /// scraping the Display string (that is what truncation drops).
    fn yaml_location_mark(yaml: &str) -> String {
        let err = serde_yaml::from_str::<crate::model::EvalConfig>(yaml)
            .expect_err("fixture must fail YAML parse");
        let loc = err
            .location()
            .expect("serde_yaml must report a location for this fixture");
        format!("at line {} column {}", loc.line(), loc.column())
    }

    fn load_parse_message(yaml: &str) -> String {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("bad.yaml");
        std::fs::write(&path, yaml).expect("write yaml");
        load_config_with_cause(&path, Default::default())
            .expect_err("fixture must fail YAML parse")
            .to_string()
    }

    /// Tab + unterminated quote are libyaml Displays: problem-mark is not a
    /// suffix (`… at line N column M, while scanning …` and a second context
    /// mark). The fold must not append `location()` again.
    #[test]
    fn yaml_parse_error_keeps_libyaml_context_without_duplicating_problem_mark() {
        let tab = "version: 1\n\tfoo: 1\n";
        let tab_msg = load_parse_message(tab);
        let tab_mark = yaml_location_mark(tab);
        assert!(
            tab_msg.contains("failed to parse YAML"),
            "tab input must stay diagnosed: {tab_msg}"
        );
        assert_eq!(
            tab_msg.matches(&tab_mark).count(),
            1,
            "tab problem-mark must appear once: {tab_msg}"
        );
        assert!(
            tab_msg.contains("while scanning"),
            "tab context text must be kept: {tab_msg}"
        );
        assert!(
            tab_msg.contains("at line 1 column 10"),
            "tab context-mark must be kept: {tab_msg}"
        );

        let quote = "version: \"hello\n";
        let quote_msg = load_parse_message(quote);
        let quote_mark = yaml_location_mark(quote);
        assert!(
            quote_msg.contains("failed to parse YAML"),
            "unterminated quote must stay diagnosed: {quote_msg}"
        );
        assert_eq!(
            quote_msg.matches(&quote_mark).count(),
            1,
            "quote problem-mark must appear once: {quote_msg}"
        );
        assert!(
            quote_msg.contains("while scanning a quoted scalar"),
            "quote context text must be kept: {quote_msg}"
        );
        assert!(
            quote_msg.contains("at line 1 column 10"),
            "quote context-mark must be kept: {quote_msg}"
        );
        assert_ne!(
            quote_mark.as_str(),
            "at line 1 column 10",
            "quote fixture must keep problem-mark distinct from context-mark"
        );
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
        let message = err.to_string();
        assert!(
            message.contains("failed to parse YAML"),
            "concise malformed YAML must stay diagnosed: {message}"
        );
        let mark = yaml_location_mark("version: [\n");
        assert!(
            message.contains(&mark),
            "short diagnosis must keep the location mark: {message}"
        );
        assert_eq!(
            message.matches(&mark).count(),
            1,
            "short diagnosis must not duplicate the location mark: {message}"
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
        let mark = yaml_location_mark(&yaml);
        assert!(
            message.contains(&mark),
            "location must survive the same total budget: {message}"
        );
        assert_eq!(
            message.matches(&mark).count(),
            1,
            "location mark must appear once: {message}"
        );
        let trunc_at = message
            .find("(truncated)")
            .expect("truncation marker already asserted");
        let mark_at = message.find(&mark).expect("location mark already asserted");
        assert!(
            mark_at > trunc_at,
            "location is reserved after the bound excerpt, not swallowed by truncate: {message}"
        );
        assert_eq!(
            err.io_kind(),
            None,
            "parse fold must not invent a read kind"
        );
    }

    fn yaml_of_len(len: usize) -> String {
        let prefix = "version: 1\nsuite: ceiling\nmodel: dummy\ntests: []\n";
        assert!(len >= prefix.len(), "fixture shorter than the YAML prefix");
        let mut yaml = String::from(prefix);
        yaml.push_str(&"#".repeat(len - prefix.len()));
        yaml
    }

    /// A FIFO must fail with a diagnosis, not block. `io_kind` stays off
    /// NotFound so a missing file remains a distinct class.
    #[cfg(unix)]
    #[test]
    fn fifo_config_is_refused_without_hanging() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("fifo.yaml");
        std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .expect("mkfifo");
        let path2 = path.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(load_config_with_cause(&path2, Default::default()));
        });
        let result = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("FIFO must return promptly, not hang the loader");
        let err = result.expect_err("FIFO must be refused");
        assert_ne!(
            err.io_kind(),
            Some(ErrorKind::NotFound),
            "FIFO must not look like a missing file: {err}"
        );
        assert!(
            err.to_string().contains("regular file"),
            "FIFO diagnosis must name the file-type rule: {err}"
        );
        assert!(
            !err.to_string().contains("failed to parse YAML"),
            "FIFO must be refused before YAML parse: {err}"
        );
    }

    #[test]
    fn config_over_the_byte_ceiling_is_refused_before_parse() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("oversize.yaml");
        let yaml = yaml_of_len((MAX_CONFIG_BYTES as usize) + 1);
        std::fs::write(&path, &yaml).expect("write oversize yaml");
        let err = load_config_with_cause(&path, Default::default())
            .expect_err("ceiling+1 must be refused");
        assert_ne!(
            err.io_kind(),
            Some(ErrorKind::NotFound),
            "oversize must not look like a missing file: {err}"
        );
        assert!(
            err.to_string().contains(&MAX_CONFIG_BYTES.to_string()),
            "oversize diagnosis must state the ceiling: {err}"
        );
        assert!(
            !err.to_string().contains("failed to parse YAML"),
            "ceiling+1 must be refused before YAML parse: {err}"
        );
        assert!(
            !err.to_string().contains("config has no tests"),
            "ceiling+1 must not reach post-parse validation: {err}"
        );
    }

    #[test]
    fn config_at_the_byte_ceiling_is_allowed_to_parse() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("exact.yaml");
        let yaml = yaml_of_len(MAX_CONFIG_BYTES as usize);
        std::fs::write(&path, &yaml).expect("write exact-ceiling yaml");
        let err = load_config_with_cause(&path, Default::default())
            .expect_err("prefix-only YAML still has no tests");
        assert!(
            !err.to_string()
                .contains(&format!("{MAX_CONFIG_BYTES}-byte")),
            "exact ceiling must not be refused as oversize: {err}"
        );
        assert!(
            err.to_string().contains("config has no tests"),
            "exact-ceiling ASCII YAML is valid and must fail only as no tests: {err}"
        );
        assert_eq!(
            err.io_kind(),
            None,
            "exact-ceiling parse is not a read kind"
        );
    }

    /// Stale metadata says the file still fits. A second handle grows it past
    /// MAX. The helper must refuse on the read bound, not parse.
    #[test]
    fn config_that_grows_after_stat_is_refused_before_parse() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("grow.yaml");
        let initial = yaml_of_len(64);
        std::fs::write(&path, &initial).expect("write small yaml");
        let file = OpenOptions::new()
            .read(true)
            .open(&path)
            .expect("open for read");
        let meta = file.metadata().expect("stat opened file");
        assert!(
            meta.len() <= MAX_CONFIG_BYTES,
            "stale metadata must still pass the early len check"
        );
        assert!(meta.file_type().is_file());

        let grown = yaml_of_len((MAX_CONFIG_BYTES as usize) + 1);
        let mut writer = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&path)
            .expect("open for grow");
        writer.write_all(grown.as_bytes()).expect("grow past MAX");
        writer.flush().expect("flush grown file");
        drop(writer);

        let err = match read_opened_config(&path, file, meta) {
            Err(err) => err,
            Ok(raw) => panic!(
                "growth after stat must be refused, helper returned Ok(len={})",
                raw.len()
            ),
        };
        assert_ne!(
            err.io_kind(),
            Some(ErrorKind::NotFound),
            "grown file must not look like a missing file: {err}"
        );
        assert!(
            err.to_string().contains(&MAX_CONFIG_BYTES.to_string()),
            "growth-after-stat diagnosis must state the ceiling: {err}"
        );
        assert!(
            !err.to_string().contains("failed to parse YAML"),
            "growth after stat must be refused before YAML parse: {err}"
        );
        assert!(
            !err.to_string().contains("config has no tests"),
            "growth after stat must not reach post-parse validation: {err}"
        );
    }
}
