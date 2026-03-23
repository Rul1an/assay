use crate::cli::args::{TrustBasisArgs, TrustBasisGenerateArgs, TrustBasisSub};
use crate::exit_codes::EXIT_SUCCESS;
use anyhow::{Context, Result};
use assay_evidence::lint::engine::LintOptions;
use assay_evidence::lint::packs::load_packs;
use assay_evidence::{
    generate_trust_basis, to_canonical_json_bytes, TrustBasisOptions, VerifyLimits,
};
use std::fs::File;
use std::io::Write;

pub fn run(args: TrustBasisArgs) -> Result<i32> {
    match args.cmd {
        TrustBasisSub::Generate(args) => cmd_generate(args),
    }
}

fn cmd_generate(args: TrustBasisGenerateArgs) -> Result<i32> {
    let bundle = File::open(&args.bundle)
        .with_context(|| format!("failed to open bundle {}", args.bundle.display()))?;

    let lint = if let Some(pack_refs) = &args.pack {
        let packs = load_packs(pack_refs).context("failed to load trust-basis packs")?;
        Some(LintOptions {
            packs,
            max_results: Some(args.max_results),
            bundle_path: Some(args.bundle.display().to_string()),
        })
    } else {
        None
    };

    let trust_basis =
        generate_trust_basis(bundle, VerifyLimits::default(), TrustBasisOptions { lint })
            .context("failed to generate trust basis")?;

    let output =
        to_canonical_json_bytes(&trust_basis).context("failed to serialize trust basis")?;

    if let Some(out_path) = args.out {
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create trust-basis output directory {}",
                    parent.display()
                )
            })?;
        }
        std::fs::write(&out_path, output)
            .with_context(|| format!("failed to write trust basis to {}", out_path.display()))?;
        eprintln!("Wrote canonical trust basis to {}", out_path.display());
    } else {
        std::io::stdout()
            .write_all(&output)
            .context("failed to write trust basis to stdout")?;
    }

    Ok(EXIT_SUCCESS)
}
