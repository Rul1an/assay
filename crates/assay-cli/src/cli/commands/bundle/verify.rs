use crate::cli::args::BundleVerifyArgs;
use crate::exit_codes;
use anyhow::Context;
use assay_core::replay::verify_bundle;

pub(super) fn cmd_verify(args: BundleVerifyArgs) -> anyhow::Result<i32> {
    let file = std::fs::File::open(&args.bundle)
        .with_context(|| format!("failed to open bundle: {}", args.bundle.display()))?;
    let res = verify_bundle(file)?;
    for w in &res.warnings {
        eprintln!("warning: {}", w);
    }
    if !res.errors.is_empty() {
        for e in &res.errors {
            eprintln!("error: {}", e);
        }
        return Ok(exit_codes::EXIT_CONFIG_ERROR);
    }
    if res.warnings.is_empty() {
        eprintln!("bundle verify: OK ({})", args.bundle.display());
    } else {
        eprintln!(
            "bundle verify: OK with warnings ({}, {} warning(s))",
            args.bundle.display(),
            res.warnings.len()
        );
    }
    Ok(exit_codes::EXIT_SUCCESS)
}
