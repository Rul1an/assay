#[cfg(target_os = "linux")]
pub(crate) fn normalize_path_syntactic(input: &str) -> String {
    let is_absolute = input.starts_with('/');
    let mut parts = Vec::new();
    for part in input.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            x => parts.push(x),
        }
    }
    if is_absolute {
        if parts.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", parts.join("/"))
        }
    } else {
        parts.join("/")
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn resolve_cgroup_id(pid: u32) -> anyhow::Result<u64> {
    use std::io::BufRead;
    use std::os::linux::fs::MetadataExt;

    let cgroup_path = format!("/proc/{}/cgroup", pid);
    let file = std::fs::File::open(&cgroup_path)?;
    let reader = std::io::BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        if line.starts_with("0::") {
            let path = line.trim_start_matches("0::");
            let path = if path.is_empty() { "/" } else { path };

            let full_path = format!("/sys/fs/cgroup{}", path);
            let metadata = std::fs::metadata(&full_path)
                .map_err(|e| anyhow::anyhow!("Failed to stat {}: {}", full_path, e))?;
            return Ok(metadata.st_ino());
        }
    }

    Err(anyhow::anyhow!(
        "No Cgroup V2 entry found in {}",
        cgroup_path
    ))
}
