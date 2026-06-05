//! Doctor command facade for Wave53.

mod fixes;
mod implementation;
mod parse_error;
mod patching;

use crate::cli::args::DoctorArgs;

pub async fn run(args: DoctorArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    implementation::run(args, legacy_mode).await
}
