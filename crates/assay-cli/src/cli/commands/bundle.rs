use crate::cli::args::BundleArgs;

#[path = "bundle/coverage.rs"]
mod coverage;
#[path = "bundle/implementation.rs"]
mod implementation;
#[path = "bundle/paths.rs"]
mod paths;
#[path = "bundle/verify.rs"]
mod verify;

pub async fn run(args: BundleArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    implementation::run(args, legacy_mode).await
}
