//! Runner-spike command facade for Wave53.

mod args;
mod cgroup;
mod exit_status;
mod implementation;
mod logs;
mod phases;
mod redaction;
mod spec;

#[allow(unused_imports)]
pub use args::{RunnerSpikeArgs, RunnerSpikeCommand, RunnerSpikeRunArgs};

pub async fn run(args: RunnerSpikeArgs) -> anyhow::Result<i32> {
    implementation::run(args).await
}
