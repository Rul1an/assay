use crate::errors::ConfigError;
use crate::model::EvalConfig;
use std::path::Path;

pub const SUPPORTED_CONFIG_VERSION: u32 = 1;

pub fn load_config(path: &Path) -> Result<EvalConfig, ConfigError> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| ConfigError(format!("failed to read config {}: {}", path.display(), e)))?;
    let cfg: EvalConfig = serde_yaml::from_str(&raw)
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
    Ok(cfg)
}

pub fn write_sample_config(path: &Path) -> Result<(), ConfigError> {
    std::fs::write(path, include_str!("../../../eval.yaml"))
        .map_err(|e| ConfigError(format!("failed to write sample config: {}", e)))?;
    Ok(())
}
