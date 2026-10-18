use anyhow::{bail, Result};

// Receipt schema bound, distinct from the narrower Trust Basis claim rules.
// Preserve accepted references verbatim; JSON Schema length counts Unicode scalars.
pub(super) fn validate_source_artifact_ref(reference: &str) -> Result<()> {
    if !(1..=512).contains(&reference.chars().count()) {
        bail!("source-artifact-ref must contain 1 to 512 Unicode scalar values");
    }
    Ok(())
}
