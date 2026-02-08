mod dispatch;

pub mod baseline;
pub(crate) mod bundle;
pub mod calibrate;
pub(crate) mod replay;
pub mod trace;

pub(crate) mod ci;
pub mod config_path;
pub mod coverage;
pub mod demo;
pub mod discover;
pub mod doctor;
pub mod events;
pub mod evidence;
pub mod explain;
pub mod fix;
pub mod generate;
pub mod heuristics;
pub mod import;
pub mod init;
pub mod init_ci;
pub mod kill;
pub mod mcp;
pub mod migrate;
pub mod monitor;
pub(crate) mod pipeline;
pub(crate) mod pipeline_error;
pub mod policy;
pub mod profile;
#[cfg(test)]
mod profile_simulation_test;
pub mod profile_types;
pub(crate) mod quarantine;
pub mod record;
pub(crate) mod reporting;
pub(crate) mod run;
pub(crate) mod run_output;
pub(crate) mod runner_builder;
pub mod sandbox;
pub mod setup;
#[cfg(feature = "sim")]
pub mod sim;
pub mod tool;
pub mod validate;
pub mod watch;
pub use dispatch::dispatch;
