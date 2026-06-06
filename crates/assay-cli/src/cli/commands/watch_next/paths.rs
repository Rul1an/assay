use anyhow::Result;
use assay_core::config::{load_config, path_resolver::PathResolver};
use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::cli::args::WatchArgs;

pub(super) fn refresh_watch_targets(
    args: &WatchArgs,
    legacy_mode: bool,
    watch_targets: &mut Vec<PathBuf>,
) -> Result<bool> {
    let next = collect_watch_paths(args, legacy_mode)?;
    if *watch_targets == next {
        return Ok(false);
    }

    let previous: BTreeSet<PathBuf> = watch_targets.iter().cloned().collect();
    let current: BTreeSet<PathBuf> = next.iter().cloned().collect();
    eprintln!("Updated watch paths:");
    for path in current.difference(&previous) {
        eprintln!("  + {}", path.display());
    }
    for path in previous.difference(&current) {
        eprintln!("  - {}", path.display());
    }

    *watch_targets = next;
    Ok(true)
}

pub(super) fn collect_watch_paths(args: &WatchArgs, legacy_mode: bool) -> Result<Vec<PathBuf>> {
    let mut paths = BTreeSet::new();

    paths.insert(args.config.clone());
    if let Some(trace) = &args.trace_file {
        paths.insert(trace.clone());
    }
    if let Some(baseline) = &args.baseline {
        paths.insert(baseline.clone());
    }

    if args.config.exists() {
        match load_config(&args.config, legacy_mode, false) {
            Ok(cfg) => {
                let resolver = PathResolver::new(&args.config);
                for test in &cfg.tests {
                    if let Some(policy_path) = test.expected.get_policy_path() {
                        let mut resolved = policy_path.to_string();
                        resolver.resolve_str(&mut resolved);
                        paths.insert(PathBuf::from(resolved));
                    }
                }
            }
            Err(err) => {
                eprintln!(
                    "warning: failed to parse config while collecting watch paths: {}; keeping core watch targets (config/trace/baseline)",
                    err,
                );
            }
        }
    }

    Ok(paths.into_iter().collect())
}
