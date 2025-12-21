use crate::errors::ConfigError;
use crate::model::EvalConfig;
use std::path::Path;

pub mod path_resolver;

pub const SUPPORTED_CONFIG_VERSION: u32 = 1;

pub fn load_config(path: &Path) -> Result<EvalConfig, ConfigError> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| ConfigError(format!("failed to read config {}: {}", path.display(), e)))?;
    let mut cfg: EvalConfig = serde_yaml::from_str(&raw)
        .map_err(|e| ConfigError(format!("failed to parse YAML: {}", e)))?;
    if cfg.version != SUPPORTED_CONFIG_VERSION {
        return Err(ConfigError(format!(
            "unsupported config version {} (supported: {})",
            cfg.version, SUPPORTED_CONFIG_VERSION
        )));
    }
    if cfg.tests.is_empty() {
        return Err(ConfigError("config has no tests".into()));
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
                        if !meta.get("verdict").is_some_and(|v| v.is_object()) {
                            meta["verdict"] = serde_json::json!({});
                        }

                        meta["verdict"]["schema_file_original"] = serde_json::json!(before);
                        meta["verdict"]["schema_file_resolved"] = serde_json::json!(resolved);
                        meta["verdict"]["config_dir"] = serde_json::json!(config_path
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
    std::fs::write(path, r#"version: 1
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
"#)
        .map_err(|e| ConfigError(format!("failed to write sample config: {}", e)))?;
    Ok(())
}
