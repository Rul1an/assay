use anyhow::Context;
use sha2::{Digest, Sha256};
use std::io::ErrorKind;
use std::io::Read;
use std::path::{Path, PathBuf};

pub(super) fn apply_seed_override(config_path: &Path, seed: u64) -> anyhow::Result<()> {
    let ext = config_path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if ext == "json" {
        let raw = std::fs::read(config_path)?;
        let mut root: serde_json::Value = serde_json::from_slice(&raw)
            .context("failed to parse JSON config for seed override")?;
        let Some(obj) = root.as_object_mut() else {
            anyhow::bail!("JSON config root must be object for seed override");
        };
        let settings = obj
            .entry("settings".to_string())
            .or_insert_with(|| serde_json::json!({}));
        let Some(settings_obj) = settings.as_object_mut() else {
            anyhow::bail!("JSON config settings must be object for seed override");
        };
        settings_obj.insert("seed".to_string(), serde_json::json!(seed));
        write_file_atomic(config_path, serde_json::to_string_pretty(&root)?.as_bytes())?;
        return Ok(());
    }

    let raw = std::fs::read_to_string(config_path)?;
    let mut root: serde_yaml::Value =
        serde_yaml::from_str(&raw).context("failed to parse YAML config for seed override")?;
    let Some(root_map) = root.as_mapping_mut() else {
        anyhow::bail!("YAML config root must be mapping for seed override");
    };

    let settings_key = serde_yaml::Value::String("settings".to_string());
    if !root_map.contains_key(&settings_key) {
        root_map.insert(
            settings_key.clone(),
            serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
        );
    }

    if !root_map
        .get(&settings_key)
        .map(|v| v.is_mapping())
        .unwrap_or(false)
    {
        root_map.insert(
            settings_key.clone(),
            serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
        );
    }

    let Some(settings) = root_map
        .get_mut(&settings_key)
        .and_then(serde_yaml::Value::as_mapping_mut)
    else {
        anyhow::bail!("YAML config settings must be mapping for seed override");
    };
    settings.insert(
        serde_yaml::Value::String("seed".to_string()),
        serde_yaml::to_value(seed)?,
    );

    write_file_atomic(config_path, serde_yaml::to_string(&root)?.as_bytes())?;
    Ok(())
}

fn write_file_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("path has no parent: {}", path.display()))?;
    let base = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("path has no filename: {}", path.display()))?;

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("clock before UNIX_EPOCH")?
        .as_nanos();
    let tmp_name = format!(".{}.tmp-{}-{}", base, std::process::id(), stamp);
    let tmp_path = parent.join(tmp_name);

    std::fs::write(&tmp_path, bytes)
        .with_context(|| format!("failed writing temp file {}", tmp_path.display()))?;
    if let Err(err) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(err).with_context(|| {
            format!(
                "failed replacing {} with temp file {}",
                path.display(),
                tmp_path.display()
            )
        });
    }
    Ok(())
}

pub(super) fn write_entries(workspace: &Path, entries: &[(String, Vec<u8>)]) -> anyhow::Result<()> {
    for (rel, data) in entries {
        let target = workspace.join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, data)?;
    }
    Ok(())
}

pub(super) fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0_u8; 8192];
    loop {
        let read = f.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

pub(super) struct ReplayWorkspace {
    path: PathBuf,
}

impl ReplayWorkspace {
    pub(super) fn new() -> anyhow::Result<Self> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("clock before UNIX_EPOCH")?
            .as_nanos();
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("assay-replay-{}-{}", pid, now));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ReplayWorkspace {
    fn drop(&mut self) {
        if let Err(err) = std::fs::remove_dir_all(&self.path) {
            if err.kind() != ErrorKind::NotFound {
                eprintln!(
                    "warning: failed to clean replay workspace {}: {}",
                    self.path.display(),
                    err
                );
            }
        }
    }
}
