use crate::cli::args::CalibrateArgs;

pub async fn cmd_calibrate(args: CalibrateArgs) -> anyhow::Result<i32> {
    let report = if let Some(run_path) = &args.run {
        let raw = std::fs::read_to_string(run_path)?;
        let run: assay_core::report::RunArtifacts = serde_json::from_str(&raw)?;
        assay_core::calibration::from_run(&run, args.target_tail)?
    } else {
        let suite = args.suite.clone().ok_or_else(|| {
            anyhow::anyhow!("config error: --suite required when calibrating from --db")
        })?;
        let store = assay_core::storage::Store::open(&args.db)?;
        assay_core::calibration::from_db(&store, &suite, args.last, args.target_tail)?
    };

    let json = serde_json::to_string_pretty(&report)?;
    std::fs::write(&args.out, &json)?;
    eprintln!("wrote {}", args.out.display());

    // Optional: Print a human readable summary to stderr
    eprintln!("\nCalibration Summary:");
    for m in report.metrics {
        eprintln!(
            "  {}: p10={:.3}, p50={:.3}. Recommended Min Score: {:.3}",
            m.key.metric, m.p10, m.p50, m.recommended_min_score
        );
    }

    Ok(0)
}
