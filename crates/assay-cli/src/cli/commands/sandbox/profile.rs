use crate::cli::args::SandboxArgs;
use crate::profile::{ProfileCollector, ProfileConfig};
use sha2::Digest;

pub(super) fn maybe_profile_begin(
    args: &SandboxArgs,
    assay_tmp: Option<&std::path::Path>,
) -> Option<ProfileCollector> {
    let _ = args.profile.as_ref()?;

    let cwd = std::env::current_dir()
        .ok()
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let home = std::env::var("HOME").ok().map(std::path::PathBuf::from);

    Some(ProfileCollector::new(ProfileConfig {
        cwd,
        home,
        assay_tmp: assay_tmp.map(|p| p.to_path_buf()),
    }))
}

pub(super) fn maybe_profile_finish(
    report: crate::profile::ProfileReport,
    args: &SandboxArgs,
) -> anyhow::Result<()> {
    let sugg_cfg = crate::profile::suggest::SuggestConfig {
        widen_dirs_to_glob: true,
    };
    let suggestion = report.to_suggestion(sugg_cfg);

    let content = match args.profile_format.as_str() {
        "json" => crate::profile::writer::write_json(&suggestion)?,
        _ => crate::profile::writer::write_yaml(&suggestion),
    };

    let out_path = args.profile.as_ref().expect("profiler active");

    crate::profile::writer::save_atomic(out_path, &content)?;

    let evidence_profile_path = evidence_profile_path(out_path, &args.profile_format);
    let run_id = evidence_profile_run_id(args, &report);
    let evidence_profile_name = out_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("assay-sandbox")
        .to_string();
    let evidence_profile = report.to_evidence_profile(&evidence_profile_name, &run_id);
    crate::cli::commands::profile_types::save_profile(&evidence_profile, &evidence_profile_path)?;

    let report_path = args.profile_report.clone().unwrap_or_else(|| {
        let mut p = out_path.clone();
        if let Some(fname) = p.file_name() {
            let new_name = format!("{}.report.md", fname.to_string_lossy());
            p.set_file_name(new_name);
        } else {
            p.set_extension("report.md");
        }
        p
    });

    let report_md = format!(
        "# Assay Profile Report\n\n\
         - **Command**: {:?}\n\
         - **Status**: Finished\n\
         - **Counters**: {:?}\n\
         - **Notes**: {:?}\n",
        args.command, suggestion.meta.counters, suggestion.meta.notes
    );

    crate::profile::writer::save_atomic(&report_path, &report_md)?;

    if !args.quiet {
        eprintln!(
            "Profile: {} (and {})",
            out_path.display(),
            report_path.display()
        );
        eprintln!("Evidence Profile: {}", evidence_profile_path.display());
    }

    Ok(())
}

fn evidence_profile_path(out_path: &std::path::Path, profile_format: &str) -> std::path::PathBuf {
    let ext = if profile_format == "json" {
        "evidence.json"
    } else {
        "evidence.yaml"
    };
    let mut path = out_path.to_path_buf();
    let stem = out_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("assay-sandbox");
    path.set_file_name(format!("{stem}.{ext}"));
    path
}

pub(super) fn evidence_profile_run_id(
    args: &SandboxArgs,
    report: &crate::profile::ProfileReport,
) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(args.command.join("\0").as_bytes());
    for (name, count) in &report.agg.counters {
        hasher.update(name.as_bytes());
        hasher.update(count.to_string().as_bytes());
    }
    for note in &report.agg.notes {
        hasher.update(note.as_bytes());
    }
    for (argv0, hits) in &report.agg.execs {
        hasher.update(argv0.as_bytes());
        hasher.update(hits.to_string().as_bytes());
    }
    let mut fs_entries = report.agg.fs.clone();
    fs_entries.sort();
    for (op, path, backend) in fs_entries {
        hasher.update(op.as_str().as_bytes());
        hasher.update(path.as_bytes());
        hasher.update(backend.as_str().as_bytes());
    }
    let mut degradations = report.agg.sandbox_degradations.clone();
    degradations.sort();
    for degradation in degradations {
        hasher.update(
            serde_json::to_string(&degradation)
                .expect("sandbox degradation payload should serialize deterministically")
                .as_bytes(),
        );
    }

    let digest = hex::encode(hasher.finalize());
    format!("sandbox_{}", &digest[..16])
}
