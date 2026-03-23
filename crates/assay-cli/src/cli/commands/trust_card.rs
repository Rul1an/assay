use crate::cli::args::{TrustCardArgs, TrustCardGenerateArgs, TrustCardSub};
use crate::exit_codes::EXIT_SUCCESS;
use anyhow::{Context, Result};
use assay_evidence::lint::engine::LintOptions;
use assay_evidence::lint::packs::load_packs;
use assay_evidence::{
    generate_trust_basis, trust_basis_to_trust_card, trust_card_to_canonical_json_bytes,
    trust_card_to_markdown, TrustBasisOptions, VerifyLimits,
};
use std::fs::File;

pub fn run(args: TrustCardArgs) -> Result<i32> {
    match args.cmd {
        TrustCardSub::Generate(args) => cmd_generate(args),
    }
}

fn cmd_generate(args: TrustCardGenerateArgs) -> Result<i32> {
    let bundle = File::open(&args.bundle)
        .with_context(|| format!("failed to open bundle {}", args.bundle.display()))?;

    let lint = if let Some(pack_refs) = &args.pack {
        let packs = load_packs(pack_refs).context("failed to load trustcard packs")?;
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
            .context("failed to generate trust basis for trust card")?;

    let card = trust_basis_to_trust_card(&trust_basis);

    let json =
        trust_card_to_canonical_json_bytes(&card).context("failed to serialize trust card json")?;
    let md = trust_card_to_markdown(&card);

    std::fs::create_dir_all(&args.out_dir).with_context(|| {
        format!(
            "failed to create trust card output directory {}",
            args.out_dir.display()
        )
    })?;

    let json_path = args.out_dir.join("trustcard.json");
    let md_path = args.out_dir.join("trustcard.md");

    std::fs::write(&json_path, json)
        .with_context(|| format!("failed to write {}", json_path.display()))?;
    std::fs::write(&md_path, md)
        .with_context(|| format!("failed to write {}", md_path.display()))?;

    eprintln!(
        "Wrote trust card to {} and {}",
        json_path.display(),
        md_path.display()
    );

    Ok(EXIT_SUCCESS)
}
