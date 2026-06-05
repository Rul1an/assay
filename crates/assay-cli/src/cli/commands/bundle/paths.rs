use crate::cli::args::BundleCreateArgs;
use std::path::{Path, PathBuf};

pub(super) fn select_source_root(
    args: &BundleCreateArgs,
    cwd: &Path,
) -> anyhow::Result<(PathBuf, String)> {
    if let Some(from) = &args.from {
        let p = absolutize(from, cwd);
        if !p.exists() {
            anyhow::bail!("--from path does not exist: {}", p.display());
        }
        if p.is_file() {
            let parent = p
                .parent()
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| cwd.to_path_buf());
            return Ok((parent, "explicit-from".to_string()));
        }
        return Ok((p, "explicit-from".to_string()));
    }
    if let Some(run_id) = &args.run_id {
        for rel in &[
            format!(".assay/{}", run_id),
            format!(".assay/run_{}", run_id),
            format!(".assay/runs/{}", run_id),
        ] {
            let p = cwd.join(rel);
            if p.exists() {
                return Ok((p, "run-id".to_string()));
            }
        }
        anyhow::bail!(
            "--run-id was provided but no matching path exists under .assay for id {}",
            run_id
        );
    }
    if let Some(latest) = find_latest_run_json(cwd)? {
        let parent = latest
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| cwd.to_path_buf());
        return Ok((parent, "mtime-latest".to_string()));
    }
    Ok((cwd.to_path_buf(), "cwd-fallback".to_string()))
}

pub(super) fn find_run_json(source_root: &Path, from: Option<&PathBuf>) -> Option<PathBuf> {
    if let Some(from) = from {
        if from.is_file() && from.file_name().and_then(|x| x.to_str()) == Some("run.json") {
            return Some(from.clone());
        }
    }
    find_first_existing(
        source_root,
        &[PathBuf::from("run.json"), PathBuf::from(".assay/run.json")],
    )
}

pub(super) fn find_summary_json(source_root: &Path) -> Option<PathBuf> {
    find_first_existing(
        source_root,
        &[
            PathBuf::from("summary.json"),
            PathBuf::from(".assay/summary.json"),
        ],
    )
}

pub(super) fn select_config_path(args: &BundleCreateArgs, source_root: &Path) -> Option<PathBuf> {
    if let Some(cfg) = &args.config {
        return Some(cfg.clone());
    }
    find_first_existing(
        source_root,
        &[PathBuf::from("eval.yaml"), PathBuf::from("assay.yaml")],
    )
}

pub(super) fn select_trace_path(args: &BundleCreateArgs, source_root: &Path) -> Option<PathBuf> {
    if let Some(t) = &args.trace_file {
        return Some(t.clone());
    }
    find_first_existing(
        source_root,
        &[
            PathBuf::from("trace.jsonl"),
            PathBuf::from("traces/ci.jsonl"),
            PathBuf::from("traces/run.jsonl"),
            PathBuf::from("traces/trace.jsonl"),
        ],
    )
}

pub(super) fn collect_files_recursive(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    collect_files_recursive_inner(root, &mut out)?;
    Ok(out)
}

pub(super) fn cassette_dirs(source_root: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![
        source_root.join("cassettes"),
        source_root.join(".assay/cassettes"),
        source_root.join(".assay/vcr"),
    ];
    if let Ok(vcr_dir) = std::env::var("ASSAY_VCR_DIR") {
        let p = PathBuf::from(vcr_dir);
        if p.is_absolute() {
            dirs.push(p);
        } else {
            dirs.push(source_root.join(p));
        }
    }
    dirs
}

fn find_latest_run_json(root: &Path) -> anyhow::Result<Option<PathBuf>> {
    let candidates = collect_named_files(root, "run.json", 6)?;
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for p in candidates {
        let meta = match std::fs::metadata(&p) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let mtime = match meta.modified() {
            Ok(t) => t,
            Err(_) => continue,
        };
        match &best {
            Some((bt, _)) if &mtime <= bt => {}
            _ => best = Some((mtime, p)),
        }
    }
    Ok(best.map(|(_, p)| p))
}

pub(super) fn find_first_existing(source_root: &Path, candidates: &[PathBuf]) -> Option<PathBuf> {
    for c in candidates {
        let p = if c.is_absolute() {
            c.clone()
        } else {
            source_root.join(c)
        };
        if p.exists() && p.is_file() {
            return Some(p);
        }
    }
    None
}

fn collect_named_files(
    root: &Path,
    needle: &str,
    max_depth: usize,
) -> anyhow::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    collect_named_files_inner(root, needle, max_depth, 0, &mut out)?;
    Ok(out)
}

fn collect_named_files_inner(
    dir: &Path,
    needle: &str,
    max_depth: usize,
    depth: usize,
    out: &mut Vec<PathBuf>,
) -> anyhow::Result<()> {
    if depth > max_depth {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if should_skip_recursive_dir(name) {
                continue;
            }
            collect_named_files_inner(&p, needle, max_depth, depth + 1, out)?;
        } else if ft.is_file() && p.file_name().and_then(|s| s.to_str()) == Some(needle) {
            out.push(p);
        }
    }
    Ok(())
}

fn collect_files_recursive_inner(dir: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if should_skip_recursive_dir(name) {
                continue;
            }
            collect_files_recursive_inner(&path, out)?;
        } else if ft.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn should_skip_recursive_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "node_modules" | ".venv" | "venv" | "__pycache__" | "dist" | "build"
    )
}

fn absolutize(path: &Path, cwd: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}
