use super::args::*;

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
pub mod policy;
pub mod profile;
#[cfg(test)]
mod profile_simulation_test;
pub mod profile_types;
pub(crate) mod quarantine;
pub mod record;
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

use crate::exit_codes::EXIT_SUCCESS;

pub async fn dispatch(cli: Cli, legacy_mode: bool) -> anyhow::Result<i32> {
    match cli.cmd {
        Command::Init(args) => init::run(args).await,
        Command::Run(args) => run::run(args, legacy_mode).await,
        Command::Ci(args) => ci::run(args, legacy_mode).await,
        Command::Validate(args) => validate::run(args, legacy_mode).await,
        Command::Fix(args) => fix::run(args, legacy_mode).await,
        Command::Doctor(args) => doctor::run(args, legacy_mode).await,
        Command::Watch(args) => watch::run(args, legacy_mode).await,
        Command::Import(args) => import::cmd_import(args),
        Command::Quarantine(args) => quarantine::run(args).await,
        Command::Trace(args) => trace::cmd_trace(args, legacy_mode).await,
        Command::Calibrate(args) => calibrate::cmd_calibrate(args).await,
        Command::Baseline(args) => match args.cmd {
            BaselineSub::Report(report_args) => {
                baseline::cmd_baseline_report(report_args).map(|_| EXIT_SUCCESS)
            }
            BaselineSub::Record(record_args) => {
                baseline::cmd_baseline_record(record_args).map(|_| EXIT_SUCCESS)
            }
            BaselineSub::Check(check_args) => {
                baseline::cmd_baseline_check(check_args).map(|_| EXIT_SUCCESS)
            }
        },
        Command::Migrate(args) => migrate::cmd_migrate(args),
        Command::Coverage(args) => coverage::cmd_coverage(args).await,
        Command::Explain(args) => explain::run(args).await,
        Command::Demo(args) => demo::cmd_demo(args).await,
        Command::InitCi(args) => init_ci::cmd_init_ci(args),
        Command::Mcp(args) => mcp::run(args).await,
        Command::Discover(args) => discover::run(args).await,
        Command::Kill(args) => kill::run(args).await,
        Command::Monitor(args) => monitor::run(args).await,
        Command::Policy(args) => policy::run(args).await,
        Command::Generate(args) => generate::run(args),
        Command::Record(args) => record::run(args).await,
        Command::Profile(args) => profile::run(args),
        Command::Sandbox(args) => sandbox::run(args).await,
        Command::Evidence(args) => evidence::run(args),
        Command::Bundle(args) => bundle::run(args, legacy_mode).await,
        Command::Replay(args) => replay::run(args, legacy_mode).await,
        #[cfg(feature = "sim")]
        Command::Sim(args) => sim::run(args),
        Command::Setup(args) => setup::run(args).await,
        Command::Tool(args) => Ok(tool::cmd_tool(args.cmd)),
        Command::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(EXIT_SUCCESS)
        }
    }
}
